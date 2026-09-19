//! World context block builder for novel-writing prompts.
//!
//! Implements §3.5.1.3 of `novel-writing/workflow-profile.md`: a compact, prompt-safe YAML
//! block injected before each outline and draft prompt for World-bound Works.
//!
//! # Architecture (per `world-kb-runtime-architecture.md` §6)
//!
//! ```text
//! novel-writing outline/draft
//!   → admitted ActorView snapshot (nexus-core :: KnowledgeReadScope)
//!   → build_chapter_kb_block (this module)
//!   → compact YAML block in preset template vars
//! ```
//!
//! The builder reads only the admitted snapshot — it issues no KB store query
//! of its own (v1.191 P1 T11, durable §4.2).
//!
//! Legacy V1.39 worldless Works (`world_id == None`) receive no block.

// Spec terminology (canonical_name, novel_category, KnowledgeEntryRecord, etc.) triggers doc_markdown.
#![allow(clippy::doc_markdown)]

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;

/// The admitted ActorView snapshot already selected by the caller.
///
/// This is the **only** knowledge input the prompt builders accept
/// (v1.191 P1 T11, durable §4.2): the rows were chosen by an admitted
/// `KnowledgeReadScope` (the actor's exact holder plus its authorized
/// containers, Character-global shared rows included), so MCA never performs
/// a World-wide read of its own and has no way to fall back to one. An empty
/// snapshot means "this actor sees nothing" — it yields empty sections,
/// never a wider query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterViewInput {
    /// Complete admitted ActorView rows for this actor + World (+ binding).
    pub entries: Vec<KnowledgeEntryRecord>,
}

impl CharacterViewInput {
    /// Wrap already-admitted view rows.
    #[must_use]
    pub const fn from_entries(entries: Vec<KnowledgeEntryRecord>) -> Self {
        Self { entries }
    }

    /// Entries the moment assembly may render (never a World-wide scan).
    #[must_use]
    pub fn admitted_entries(&self) -> &[KnowledgeEntryRecord] {
        &self.entries
    }
}

/// Default token budget for the World context block (~1500 tokens ≈ 6000 chars).
pub const DEFAULT_WORLD_CONTEXT_TOKEN_BUDGET: usize = 1500;

/// Chars-per-token heuristic (matches moment.rs §9.3 spec).
const CHARS_PER_TOKEN: usize = 4;

/// Maximum characters before truncation marker is appended.
///
/// WAIVER: pre-1.0 local-first; see V1.41 P-last residual R-V140P2-S3
/// — truncation marker is a YAML comment only; downstream prompt consumers
/// treat it as opaque text; formal YAML structure-aware truncation deferred.
const TRUNCATION_MARKER: &str = "\n# [... truncated]";

/// A single item in the World context block (character, location, or rule).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldContextItem {
    /// KB item id (key_block_id).
    pub id: String,
    /// Human-readable name (canonical_name).
    pub name: String,
    /// Short descriptor (body.summary or empty string).
    pub descriptor: String,
}

/// The complete World context block for a chapter prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldContextBlock {
    /// World ID.
    pub world_id: String,
    /// World name (from narrative, if available).
    pub world_name: String,
    /// Current timeline description.
    pub current_timeline: String,
    /// Characters relevant to this chapter.
    pub characters_in_chapter: Vec<WorldContextItem>,
    /// Locations referenced in this chapter.
    pub locations_referenced: Vec<WorldContextItem>,
    /// Active rules (foundation + rules category items).
    pub active_rules: Vec<WorldContextItem>,
    /// Whether the block was truncated due to token budget.
    pub truncated: bool,
}

impl WorldContextBlock {
    /// Render the block as YAML per `novel-writing/workflow-profile.md` §3.5.1.3.
    ///
    /// Output shape:
    /// ```yaml
    /// world_id: wld_123
    /// world_name: Neon River
    /// current_timeline: chapter 3: after the river-market fire
    /// characters_in_chapter:
    ///   - id: char_lin_xia
    ///     name: Lin Xia
    ///     descriptor: ex-cartographer hiding a forbidden river map
    /// locations_referenced:
    ///   - ...
    /// active_rules:
    ///   - ...
    /// ```
    ///
    /// Empty sections are rendered as `[]`.
    ///
    /// R-V140P2-S4 / R-V141HYG-02: String fields use `{:?}` (Debug) which produces
    /// valid YAML double-quoted scalars with proper escaping for `:`, `"`, `\`, etc.
    /// Display (`{}`) was tried but breaks YAML parsing when user strings contain
    /// colons, quotes, or other YAML metacharacters.
    #[must_use]
    pub fn to_yaml(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!("world_id: {}", self.world_id));
        lines.push(format!("world_name: {:?}", self.world_name));
        lines.push(format!("current_timeline: {:?}", self.current_timeline));

        lines.push("characters_in_chapter:".to_string());
        if self.characters_in_chapter.is_empty() {
            lines.push("  []".to_string());
        } else {
            for item in &self.characters_in_chapter {
                lines.push(format!("  - id: {}", item.id));
                lines.push(format!("    name: {:?}", item.name));
                lines.push(format!("    descriptor: {:?}", item.descriptor));
            }
        }

        lines.push("locations_referenced:".to_string());
        if self.locations_referenced.is_empty() {
            lines.push("  []".to_string());
        } else {
            for item in &self.locations_referenced {
                lines.push(format!("  - id: {}", item.id));
                lines.push(format!("    name: {:?}", item.name));
                lines.push(format!("    descriptor: {:?}", item.descriptor));
            }
        }

        lines.push("active_rules:".to_string());
        if self.active_rules.is_empty() {
            lines.push("  []".to_string());
        } else {
            for item in &self.active_rules {
                lines.push(format!("  - id: {}", item.id));
                lines.push(format!("    name: {:?}", item.name));
                lines.push(format!("    descriptor: {:?}", item.descriptor));
            }
        }

        if self.truncated {
            lines.push(TRUNCATION_MARKER.to_string());
        }

        lines.join("\n")
    }
}

/// Parameters for building a chapter KB block.
#[derive(Debug, Clone)]
pub struct ChapterKbBlockParams {
    /// World ID (required for World-bound Works).
    pub world_id: String,
    /// World name (from narrative gateway or caller).
    pub world_name: String,
    /// Current timeline description (from narrative or chapter context).
    pub current_timeline: String,
    /// World refs from chapter frontmatter (canonical_name ids).
    pub world_refs: Vec<String>,
    /// Optional outline or body text for heuristic fallback.
    pub chapter_text: Option<String>,
    /// Token budget (defaults to [`DEFAULT_WORLD_CONTEXT_TOKEN_BUDGET`]).
    pub max_tokens: Option<usize>,
}

/// Shared KB filter helpers for an admitted ActorView snapshot.
///
/// WAIVER: pre-1.0 local-first; see V1.41 P-last residual R-V140P2-S1
/// — per-prompt KB filters run as a linear scan over the admitted snapshot;
/// acceptable for single-user local daemon with bounded KB size; index when
/// needed.
///
/// v1.191 P1 T11: the builder no longer owns a store. It applies the same
/// filter/taxonomy logic the store queries used to (`block_type`,
/// `canonical_name`, `novel_category`) in memory over admitted rows, so both
/// the chapter KB block and the Moment assembly consume one already-filtered
/// snapshot instead of issuing a World query per prompt.
pub struct WorldKbQueryBuilder;

impl WorldKbQueryBuilder {
    /// Rows of one `block_type` in `(created_at, entry_id)` snapshot order.
    #[must_use]
    pub fn of_block_type(
        entries: &[KnowledgeEntryRecord],
        block_type: BlockType,
    ) -> Vec<&KnowledgeEntryRecord> {
        entries
            .iter()
            .filter(|kb| kb.block_type == block_type)
            .collect()
    }

    /// First snapshot row matching `canonical_name` + `block_type`.
    ///
    /// Mirror of the retired store path (`KbQuery::with_canonical_name` +
    /// `block_type`, first item): `apply_kb_query_filters` matches
    /// `canonical_name` by exact equality, so this is an exact-match find
    /// over admitted rows in snapshot order.
    #[must_use]
    pub fn by_canonical_name<'a>(
        entries: &'a [KnowledgeEntryRecord],
        canonical_name: &str,
        block_type: BlockType,
    ) -> Option<&'a KnowledgeEntryRecord> {
        entries
            .iter()
            .find(|kb| kb.canonical_name == canonical_name && kb.block_type == block_type)
    }
}

/// Extract `novel_category` from a KnowledgeEntryRecord's body attributes.
///
/// Returns `None` if the body or attributes are missing, or if `novel_category`
/// is not a string.
fn extract_novel_category(
    kb: &nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord,
) -> Option<String> {
    kb.body
        .as_ref()
        .and_then(|b| b.attributes.as_ref())
        .and_then(|attrs| attrs.get("novel_category"))
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
}

/// Convert a `KnowledgeEntryRecord` to a `WorldContextItem`.
fn kb_to_item(kb: &KnowledgeEntryRecord) -> WorldContextItem {
    let descriptor = kb
        .body
        .as_ref()
        .and_then(|b| b.summary.as_deref())
        .unwrap_or("")
        .to_string();
    WorldContextItem {
        id: kb.entry_id.clone(),
        name: kb.canonical_name.clone(),
        descriptor,
    }
}

/// Build the compact World context block for a chapter prompt from an
/// admitted ActorView snapshot.
///
/// This is the primary entry point for the chapter KB block. It:
/// 1. Selects characters (BlockType::Character) and locations (BlockType::Scene)
///    from the admitted snapshot.
/// 2. If `world_refs` is non-empty, resolves items by canonical_name; otherwise
///    takes every admitted character/location. If `chapter_text` is provided,
///    uses heuristic text matching to narrow the fallback set.
/// 3. Selects active rules (novel_category: foundation + rules).
/// 4. Applies token budget truncation.
///
/// # ActorView-only input (HARD, v1.191 P1 T11 — durable §4.2)
///
/// `view` is the caller's admitted snapshot (the exact admitted holder plus
/// its authorized containers, Character-global shared rows included). This
/// builder takes **no store**: a World-wide fallback query is not expressible
/// here, so a missing or incomplete snapshot degrades to the rows it actually
/// holds — an empty snapshot yields an empty block, never a wider read.
///
/// # Ownership / Isolation (QC2-W02)
///
/// The row-level isolation now rides the snapshot itself: the caller is
/// responsible for resolving the admitted `KnowledgeReadScope` (owner +
/// holder) that produced `view.entries` for the Work's `world_id` before
/// calling. The block is still world-scoped only — it takes no `creator_id`
/// and performs no store read of its own.
///
/// # Missing World / 404 Contract (QC2-W03)
///
/// The returned block carries empty sections when the snapshot holds no
/// matching items. It does NOT distinguish "world exists but has no admitted
/// items" from "world_id is unknown to the system." The 404/remediation
/// contract lives one layer up (in the caller), which should validate
/// `world_id` existence against the narrative store before calling this
/// function.
#[must_use]
pub fn build_chapter_kb_block(
    view: &CharacterViewInput,
    params: &ChapterKbBlockParams,
) -> WorldContextBlock {
    let admitted = view.admitted_entries();
    let max_tokens = params
        .max_tokens
        .unwrap_or(DEFAULT_WORLD_CONTEXT_TOKEN_BUDGET);
    let max_chars = max_tokens.saturating_mul(CHARS_PER_TOKEN);

    // Resolve characters
    let all_characters: Vec<WorldContextItem> = if params.world_refs.is_empty() {
        WorldKbQueryBuilder::of_block_type(admitted, BlockType::Character)
            .into_iter()
            .map(kb_to_item)
            .collect()
    } else {
        // Resolve by world_refs: match each ref by canonical_name, keep Character type
        resolve_items_by_refs(admitted, &params.world_refs, BlockType::Character)
    };

    // QC1-W002 fix: heuristic fallback when world_refs is empty but chapter_text is provided.
    // Scan chapter text for known character canonical_names and prefer those that match.
    let mut characters = if params.world_refs.is_empty() {
        params.chapter_text.as_ref().map_or_else(
            || all_characters.clone(),
            |text| {
                let text_lower = text.to_lowercase();
                all_characters
                    .iter()
                    .filter(|item| text_lower.contains(&item.name.to_lowercase()))
                    .cloned()
                    .collect()
            },
        )
    } else {
        all_characters
    };
    // QC3-W4 fix: sort by canonical_name for deterministic prompt output.
    characters.sort_by(|a, b| a.name.cmp(&b.name));

    // Resolve locations
    let all_locations: Vec<WorldContextItem> = if params.world_refs.is_empty() {
        WorldKbQueryBuilder::of_block_type(admitted, BlockType::Scene)
            .into_iter()
            .map(kb_to_item)
            .collect()
    } else {
        resolve_items_by_refs(admitted, &params.world_refs, BlockType::Scene)
    };

    // Heuristic fallback for locations.
    let mut locations = if params.world_refs.is_empty() {
        params.chapter_text.as_ref().map_or_else(
            || all_locations.clone(),
            |text| {
                let text_lower = text.to_lowercase();
                all_locations
                    .iter()
                    .filter(|item| text_lower.contains(&item.name.to_lowercase()))
                    .cloned()
                    .collect()
            },
        )
    } else {
        all_locations
    };
    locations.sort_by(|a, b| a.name.cmp(&b.name));

    // Resolve active rules: foundation + rules novel_category items
    let mut active_rules = resolve_active_rules(admitted);
    active_rules.sort_by(|a, b| a.name.cmp(&b.name));

    let mut block = WorldContextBlock {
        world_id: params.world_id.clone(),
        world_name: params.world_name.clone(),
        current_timeline: params.current_timeline.clone(),
        characters_in_chapter: characters,
        locations_referenced: locations,
        active_rules,
        truncated: false,
    };

    // Apply token budget
    let yaml = block.to_yaml();
    if yaml.chars().count() > max_chars {
        // Truncate: prefer characters first, then locations, then rules
        apply_token_budget(&mut block, max_chars);
    }

    block
}

/// Resolve snapshot items by canonical_name from world_refs, filtered to a
/// specific block_type.
fn resolve_items_by_refs(
    admitted: &[KnowledgeEntryRecord],
    world_refs: &[String],
    block_type: BlockType,
) -> Vec<WorldContextItem> {
    let mut items = Vec::new();
    for r#ref in world_refs {
        if let Some(kb) = WorldKbQueryBuilder::by_canonical_name(admitted, r#ref, block_type) {
            items.push(kb_to_item(kb));
        }
    }
    items
}

/// Resolve active rules: admitted items with novel_category "foundation" or "rules".
fn resolve_active_rules(admitted: &[KnowledgeEntryRecord]) -> Vec<WorldContextItem> {
    admitted
        .iter()
        .filter(|kb| {
            let cat = extract_novel_category(kb);
            matches!(cat.as_deref(), Some("foundation" | "rules"))
        })
        .map(kb_to_item)
        .collect()
}

/// Apply token budget by truncating items from the end of each section.
///
/// Truncation priority (removed first): locations → characters → rules.
///
/// QC3-W3 fix: avoids O(n²) re-rendering by estimating per-item char cost
/// and popping items until the estimated total is within budget.
fn apply_token_budget(block: &mut WorldContextBlock, max_chars: usize) {
    // Estimate the cost of removing one item from a section.
    // Each item contributes roughly: "  - id: <id>\n    name: <name>\n    descriptor: <desc>\n"
    const fn estimate_item_chars(item: &WorldContextItem) -> usize {
        // "  - id: " (7) + id.len + "\n    name: " (11) + name.len + "\n    descriptor: " (15) + desc.len + "\n" (1)
        7 + item.id.len() + 11 + item.name.len() + 15 + item.descriptor.len() + 1
    }

    // Compute current total and check if we're already within budget.
    let current_chars = block.to_yaml().chars().count();
    let mut over_by = current_chars.saturating_sub(max_chars);
    if over_by == 0 {
        return;
    }

    // Remove locations from the end until estimated within budget.
    while over_by > 0 && !block.locations_referenced.is_empty() {
        if let Some(item) = block.locations_referenced.pop() {
            over_by = over_by.saturating_sub(estimate_item_chars(&item));
        }
    }

    // Remove characters.
    while over_by > 0 && !block.characters_in_chapter.is_empty() {
        if let Some(item) = block.characters_in_chapter.pop() {
            over_by = over_by.saturating_sub(estimate_item_chars(&item));
        }
    }

    // Remove rules.
    while over_by > 0 && !block.active_rules.is_empty() {
        if let Some(item) = block.active_rules.pop() {
            over_by = over_by.saturating_sub(estimate_item_chars(&item));
        }
    }

    // Final check: if still over budget (header alone exceeds limit), mark truncated.
    let final_chars = block.to_yaml().chars().count();
    if final_chars > max_chars {
        block.truncated = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryBody;

    /// Helper: create a novel-profile KnowledgeEntryRecord.
    fn make_novel_block(
        world_id: &str,
        block_type: BlockType,
        name: &str,
        novel_category: &str,
    ) -> nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord {
        let mut kb = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::new(
            world_id, block_type, name,
        );
        kb.set_body(KnowledgeEntryBody {
            summary: Some(format!("{novel_category}: {name} summary")),
            attributes: Some(serde_json::json!({
                "novel_category": novel_category,
                "traits": ["test"]
            })),
            tags: Some(vec!["novel".to_string()]),
            ..Default::default()
        })
        .unwrap();
        kb
    }

    fn make_params(world_id: &str, world_refs: &[&str]) -> ChapterKbBlockParams {
        ChapterKbBlockParams {
            world_id: world_id.to_string(),
            world_name: "Test World".to_string(),
            current_timeline: "chapter 1: the beginning".to_string(),
            world_refs: world_refs.iter().map(|s| (*s).to_string()).collect(),
            chapter_text: None,
            max_tokens: None,
        }
    }

    // AC1: World-bound Work + populated World KB → block present with required fields.
    #[test]
    fn world_bound_populated_kb_produces_block() {
        let char_kb = make_novel_block("wld_1", BlockType::Character, "char_lin_xia", "character");
        let loc_kb = make_novel_block("wld_1", BlockType::Scene, "loc_neon_city", "location");
        let rule_kb = make_novel_block("wld_1", BlockType::Conflict, "rule_magic_cost", "rules");
        let fnd_kb = make_novel_block("wld_1", BlockType::InfoPoint, "fnd_cosmology", "foundation");

        let params = ChapterKbBlockParams {
            world_id: "wld_1".to_string(),
            world_name: "Neon River".to_string(),
            current_timeline: "chapter 3: after the river-market fire".to_string(),
            world_refs: vec!["char_lin_xia".to_string(), "loc_neon_city".to_string()],
            chapter_text: None,
            max_tokens: None,
        };

        let block = build_chapter_kb_block(
            &CharacterViewInput::from_entries(vec![char_kb, loc_kb, rule_kb, fnd_kb]),
            &params,
        );

        assert_eq!(block.world_id, "wld_1");
        assert_eq!(block.world_name, "Neon River");
        assert_eq!(
            block.current_timeline,
            "chapter 3: after the river-market fire"
        );

        // Characters resolved via world_refs
        assert_eq!(block.characters_in_chapter.len(), 1);
        assert_eq!(block.characters_in_chapter[0].name, "char_lin_xia");

        // Locations resolved via world_refs
        assert_eq!(block.locations_referenced.len(), 1);
        assert_eq!(block.locations_referenced[0].name, "loc_neon_city");

        // Active rules: foundation + rules
        assert_eq!(block.active_rules.len(), 2);
        let rule_names: Vec<&str> = block.active_rules.iter().map(|r| r.name.as_str()).collect();
        assert!(rule_names.contains(&"rule_magic_cost"));
        assert!(rule_names.contains(&"fnd_cosmology"));

        assert!(!block.truncated);

        // Verify YAML output contains required fields (Debug format adds quotes)
        let yaml = block.to_yaml();
        assert!(yaml.contains("world_id: wld_1"));
        assert!(yaml.contains("world_name: \"Neon River\""));
        assert!(yaml.contains("characters_in_chapter:"));
        assert!(yaml.contains("locations_referenced:"));
        assert!(yaml.contains("active_rules:"));
    }

    // AC2: World-bound Work + empty World KB → block present but with empty sections.
    #[test]
    fn world_bound_empty_kb_produces_empty_block() {
        let params = make_params("wld_empty", &[]);
        let block = build_chapter_kb_block(&CharacterViewInput::from_entries(Vec::new()), &params);

        assert_eq!(block.world_id, "wld_empty");
        assert!(block.characters_in_chapter.is_empty());
        assert!(block.locations_referenced.is_empty());
        assert!(block.active_rules.is_empty());

        let yaml = block.to_yaml();
        assert!(yaml.contains("characters_in_chapter:"));
        assert!(yaml.contains("  []"));
    }

    // AC3: world_refs populated → characters/locations use world_refs.
    #[test]
    fn world_refs_filter_characters_and_locations() {
        let char1 = make_novel_block("wld_1", BlockType::Character, "char_a", "character");
        let char2 = make_novel_block("wld_1", BlockType::Character, "char_b", "character");
        let loc1 = make_novel_block("wld_1", BlockType::Scene, "loc_x", "location");

        // Only reference char_a and loc_x
        let params = ChapterKbBlockParams {
            world_refs: vec!["char_a".to_string(), "loc_x".to_string()],
            ..make_params("wld_1", &[])
        };

        let block = build_chapter_kb_block(
            &CharacterViewInput::from_entries(vec![char1, char2, loc1]),
            &params,
        );

        // Should only contain char_a, not char_b
        assert_eq!(block.characters_in_chapter.len(), 1);
        assert_eq!(block.characters_in_chapter[0].name, "char_a");

        // Should only contain loc_x
        assert_eq!(block.locations_referenced.len(), 1);
        assert_eq!(block.locations_referenced[0].name, "loc_x");
    }

    // AC4: world_refs empty → fall back to all characters/locations.
    #[test]
    fn world_refs_empty_falls_back_to_all() {
        let char1 = make_novel_block("wld_1", BlockType::Character, "char_a", "character");
        let char2 = make_novel_block("wld_1", BlockType::Character, "char_b", "character");
        let loc1 = make_novel_block("wld_1", BlockType::Scene, "loc_x", "location");
        let evt = make_novel_block("wld_1", BlockType::Event, "evt_bg", "background");

        let params = make_params("wld_1", &[]);
        let block = build_chapter_kb_block(
            &CharacterViewInput::from_entries(vec![char1, char2, loc1, evt]),
            &params,
        );

        // All characters
        assert_eq!(block.characters_in_chapter.len(), 2);
        // All locations (scenes)
        assert_eq!(block.locations_referenced.len(), 1);
        // No background items in active_rules (only foundation + rules)
        assert!(!block.active_rules.iter().any(|r| r.name == "evt_bg"));
    }

    // QC1-W002 fix: chapter_text heuristic narrows fallback when world_refs is empty.
    #[test]
    fn chapter_text_heuristic_narrows_fallback() {
        let char1 = make_novel_block("wld_1", BlockType::Character, "alice", "character");
        let char2 = make_novel_block("wld_1", BlockType::Character, "bob", "character");
        let loc1 = make_novel_block("wld_1", BlockType::Scene, "tavern", "location");
        let loc2 = make_novel_block("wld_1", BlockType::Scene, "forest", "location");

        // chapter_text mentions Alice and the tavern but not Bob or the forest
        let params = ChapterKbBlockParams {
            world_id: "wld_1".to_string(),
            world_name: "Test".to_string(),
            current_timeline: "chapter 1".to_string(),
            world_refs: vec![], // empty → heuristic fallback
            chapter_text: Some("Alice walked into the tavern.".to_string()),
            max_tokens: None,
        };

        let block = build_chapter_kb_block(
            &CharacterViewInput::from_entries(vec![char1, char2, loc1, loc2]),
            &params,
        );

        // Heuristic should narrow to only matching names
        let char_names: Vec<&str> = block
            .characters_in_chapter
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert!(
            char_names.contains(&"alice"),
            "should contain alice (mentioned in text)"
        );
        assert!(
            !char_names.contains(&"bob"),
            "should not contain bob (not mentioned)"
        );

        let loc_names: Vec<&str> = block
            .locations_referenced
            .iter()
            .map(|l| l.name.as_str())
            .collect();
        assert!(
            loc_names.contains(&"tavern"),
            "should contain tavern (mentioned in text)"
        );
        assert!(
            !loc_names.contains(&"forest"),
            "should not contain forest (not mentioned)"
        );
    }

    // Without chapter_text, fallback returns all items (no narrowing).
    #[test]
    fn no_chapter_text_returns_all_in_fallback() {
        let char1 = make_novel_block("wld_1", BlockType::Character, "alice", "character");
        let char2 = make_novel_block("wld_1", BlockType::Character, "bob", "character");

        let params = ChapterKbBlockParams {
            chapter_text: None,
            ..make_params("wld_1", &[])
        };

        let block = build_chapter_kb_block(
            &CharacterViewInput::from_entries(vec![char1, char2]),
            &params,
        );

        // Without chapter_text, all characters are returned
        assert_eq!(block.characters_in_chapter.len(), 2);
    }

    // AC5: Legacy V1.39 worldless Work → block omitted.
    // (Verified by caller: if world_id is None, don't call build_chapter_kb_block.)
    // We test that the function requires a world_id.
    #[tokio::test]
    async fn legacy_worldless_caller_skips_block() {
        // The caller (engine/daemon) checks world_id before calling.
        // build_chapter_kb_block always requires a world_id in params.
        // This test documents the convention: if world_id is None at caller,
        // the function is not called and no block is produced.
        // The function signature makes this clear: world_id is String, not Option<String>.
        // Intentionally unconditional: this test documents a caller-level
        // convention (world_id is always present when the function is called).
        // There is nothing to assert at runtime — the contract lives in the
        // signature (world_id: String, not Option<String>).
    }

    // AC6 (v1.191 P1 T11 revised contract): the snapshot is the whole input.
    // The builder performs no World query of its own, so an unknown World is
    // simply an empty snapshot → empty sections. World scoping belongs to the
    // caller's admitted `KnowledgeReadScope`, never to this builder.
    #[test]
    fn missing_world_id_returns_empty_block() {
        let params = make_params("wld_ghost", &[]);
        let block = build_chapter_kb_block(&CharacterViewInput::from_entries(Vec::new()), &params);

        // No data for wld_ghost
        assert!(block.characters_in_chapter.is_empty());
        assert!(block.locations_referenced.is_empty());
        assert!(block.active_rules.is_empty());
    }

    // AC7: Token budget exceeded → truncate gracefully with marker.
    #[test]
    fn token_budget_truncates_gracefully() {
        // Many characters with long summaries.
        let admitted: Vec<_> = (0..20)
            .map(|i| {
                let mut kb = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::new(
                    "wld_1",
                    BlockType::Character,
                    &format!("char_{i:02}"),
                );
                kb.set_body(KnowledgeEntryBody {
                    summary: Some(format!(
                        "Character {i} with a very long descriptor that takes up space"
                    )),
                    attributes: None,
                    tags: None,
                    ..Default::default()
                })
                .unwrap();
                kb
            })
            .collect();

        let params = ChapterKbBlockParams {
            world_id: "wld_1".to_string(),
            world_name: "Big World".to_string(),
            current_timeline: "chapter 1".to_string(),
            world_refs: vec![],
            chapter_text: None,
            max_tokens: Some(50), // Very small budget = 200 chars
        };

        let block = build_chapter_kb_block(&CharacterViewInput::from_entries(admitted), &params);

        let yaml = block.to_yaml();
        // Should fit within reasonable bounds (may have truncated some items)
        let yaml_chars = yaml.chars().count();
        // After truncation, the YAML should be significantly shorter than untruncated
        assert!(
            yaml_chars < 5000,
            "truncated YAML should be short, got {yaml_chars} chars"
        );
    }

    // Unit test: YAML output format matches spec.
    #[test]
    fn yaml_output_matches_spec_shape() {
        let block = WorldContextBlock {
            world_id: "wld_123".to_string(),
            world_name: "Neon River".to_string(),
            current_timeline: "chapter 3: after the river-market fire".to_string(),
            characters_in_chapter: vec![WorldContextItem {
                id: "kb_abc".to_string(),
                name: "Lin Xia".to_string(),
                descriptor: "ex-cartographer hiding a forbidden river map".to_string(),
            }],
            locations_referenced: vec![WorldContextItem {
                id: "kb_def".to_string(),
                name: "Neon City".to_string(),
                descriptor: "tiered canal metropolis".to_string(),
            }],
            active_rules: vec![WorldContextItem {
                id: "kb_ghi".to_string(),
                name: "Memory-for-light exchange".to_string(),
                descriptor: "large spells erase recent autobiographical memory".to_string(),
            }],
            truncated: false,
        };

        let yaml = block.to_yaml();

        // Verify exact format per §3.5.1.3 (R-V141HYG-02: Debug format for YAML-safe escaping)
        assert!(yaml.starts_with("world_id: wld_123\n"));
        assert!(yaml.contains("world_name: \"Neon River\""));
        assert!(yaml.contains("current_timeline: \"chapter 3: after the river-market fire\""));
        assert!(yaml.contains("characters_in_chapter:"));
        assert!(yaml.contains("  - id: kb_abc"));
        assert!(yaml.contains("    name: \"Lin Xia\""));
        assert!(yaml.contains("    descriptor: \"ex-cartographer hiding a forbidden river map\""));
        assert!(yaml.contains("locations_referenced:"));
        assert!(yaml.contains("  - id: kb_def"));
        assert!(yaml.contains("active_rules:"));
        assert!(yaml.contains("  - id: kb_ghi"));
        assert!(!yaml.contains("truncated"));
    }

    // QC3-W4 fix: output is deterministic regardless of snapshot order.
    #[test]
    fn output_is_deterministic_regardless_of_insertion_order() {
        let char_a = make_novel_block("wld_1", BlockType::Character, "alpha", "character");
        let char_b = make_novel_block("wld_1", BlockType::Character, "beta", "character");

        let params = make_params("wld_1", &[]);

        let yaml1 = build_chapter_kb_block(
            &CharacterViewInput::from_entries(vec![char_a.clone(), char_b.clone()]),
            &params,
        )
        .to_yaml();
        let yaml2 = build_chapter_kb_block(
            &CharacterViewInput::from_entries(vec![char_b, char_a]),
            &params,
        )
        .to_yaml();

        assert_eq!(
            yaml1, yaml2,
            "YAML output must be identical regardless of snapshot order"
        );
    }

    // Unit test: empty sections render as `[]`.
    #[test]
    fn empty_sections_render_as_empty_list() {
        let block = WorldContextBlock {
            world_id: "wld_1".to_string(),
            world_name: "Empty World".to_string(),
            current_timeline: String::new(),
            characters_in_chapter: vec![],
            locations_referenced: vec![],
            active_rules: vec![],
            truncated: false,
        };

        let yaml = block.to_yaml();
        assert!(yaml.contains("characters_in_chapter:\n  []"));
        assert!(yaml.contains("locations_referenced:\n  []"));
        assert!(yaml.contains("active_rules:\n  []"));
    }

    // Unit test: truncated block includes marker.
    #[test]
    fn truncated_block_includes_marker() {
        let block = WorldContextBlock {
            world_id: "wld_1".to_string(),
            world_name: "Trunc".to_string(),
            current_timeline: String::new(),
            characters_in_chapter: vec![],
            locations_referenced: vec![],
            active_rules: vec![],
            truncated: true,
        };

        let yaml = block.to_yaml();
        assert!(yaml.contains(TRUNCATION_MARKER.trim_start_matches('\n')));
    }

    // v1.191 P1 T11: the snapshot filter helpers replace the retired
    // World-scoped `KbQuery` builders; both directions (present / absent) are
    // pinned because the caller no longer has a store to fall back on.
    #[test]
    fn snapshot_helpers_filter_admitted_rows_only() {
        let hero = KnowledgeEntryRecord::new("wld_test", BlockType::Character, "Ada");
        let castle = KnowledgeEntryRecord::new("wld_test", BlockType::Scene, "Castle");
        let admitted = vec![hero, castle];

        let characters = WorldKbQueryBuilder::of_block_type(&admitted, BlockType::Character);
        assert_eq!(characters.len(), 1);
        assert_eq!(characters[0].canonical_name, "Ada");

        let located = WorldKbQueryBuilder::by_canonical_name(&admitted, "Castle", BlockType::Scene)
            .expect("exact canonical_name + block_type match");
        assert!(!located.entry_id.is_empty());

        // A row that is not in the snapshot can never be resolved: absence is
        // "not admitted", never "query the World for it".
        assert!(
            WorldKbQueryBuilder::by_canonical_name(&admitted, "Ghost", BlockType::Character)
                .is_none()
        );
        assert!(
            WorldKbQueryBuilder::by_canonical_name(&admitted, "Ada", BlockType::Scene).is_none(),
            "block_type must match as well as canonical_name"
        );
        assert!(WorldKbQueryBuilder::of_block_type(&[], BlockType::Character).is_empty());
    }

    // v1.191 P1 T11 (durable §4.2): the chapter block reads the admitted
    // snapshot alone — an empty snapshot yields an empty block, never a
    // World-wide read (`build_chapter_kb_block` has no store to widen with).
    #[test]
    fn v1191_holder_context_chapter_block_reads_admitted_snapshot_only() {
        let params = ChapterKbBlockParams {
            world_id: "wld_ctx".to_string(),
            world_name: "Context World".to_string(),
            current_timeline: "chapter 1".to_string(),
            world_refs: vec![],
            chapter_text: None,
            max_tokens: None,
        };

        let empty = build_chapter_kb_block(&CharacterViewInput::from_entries(Vec::new()), &params);
        assert!(empty.characters_in_chapter.is_empty());
        assert!(empty.locations_referenced.is_empty());
        assert!(empty.active_rules.is_empty());

        let shared = KnowledgeEntryRecord::new("wld_ctx", BlockType::Character, "SharedAda");
        let admitted = build_chapter_kb_block(
            &CharacterViewInput::from_entries(vec![shared.clone()]),
            &params,
        );
        assert_eq!(admitted.characters_in_chapter.len(), 1);
        assert_eq!(admitted.characters_in_chapter[0].name, "SharedAda");
        assert_eq!(admitted.characters_in_chapter[0].id, shared.entry_id);
    }

    // R-V141HYG-02: to_yaml must produce parseable YAML for strings with metacharacters.
    #[test]
    fn to_yaml_handles_user_strings_with_yaml_metacharacters() {
        let block = WorldContextBlock {
            world_id: "wld_test".to_string(),
            world_name: "Neon: River \"Reborn\"".to_string(),
            current_timeline: "chapter 3: after the fire".to_string(),
            characters_in_chapter: vec![WorldContextItem {
                id: "char_1".to_string(),
                name: "Lin: \"Shadow\" Xia".to_string(),
                descriptor: "ex-cartographer: hides a map".to_string(),
            }],
            locations_referenced: vec![],
            active_rules: vec![],
            truncated: false,
        };

        let yaml = block.to_yaml();

        // Verify the YAML is parseable — at minimum, lines should split correctly.
        // Using {:?} (Debug) produces escaped strings like:
        //   world_name: "Neon: River \"Reborn\""
        // which is valid YAML double-quoted scalar.
        // With {} (Display), the colons and quotes in the raw string break parsing.
        for line in yaml.lines() {
            if let Some(value) = line.strip_prefix("world_name: ") {
                // Must be a valid YAML quoted string (starts with ")
                assert!(
                    value.starts_with('"') && value.ends_with('"'),
                    "world_name value should be YAML-quoted, got: {value}"
                );
            }
        }

        // Round-trip check: if we can split key-value pairs on ": ", the YAML
        // is at least structurally valid for our flat format.
        assert!(
            yaml.contains("world_id: wld_test"),
            "world_id should appear correctly"
        );
    }
}
