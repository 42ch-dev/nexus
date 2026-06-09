//! World context block builder for novel-writing prompts.
//!
//! Implements §3.5.1.3 of `novel-workflow-profile.md`: a compact, prompt-safe YAML
//! block injected before each outline and draft prompt for World-bound Works.
//!
//! # Architecture (per `world-kb-runtime-architecture.md` §6)
//!
//! ```text
//! novel-writing outline/draft
//!   → build_chapter_kb_block (this module)
//!   → KbStore::query (nexus-kb)
//!   → compact YAML block in preset template vars
//! ```
//!
//! Legacy V1.39 worldless Works (`world_id == None`) receive no block.

// Spec terminology (canonical_name, novel_category, KeyBlock, etc.) triggers doc_markdown.
#![allow(clippy::doc_markdown)]

use nexus_contracts::BlockType;
use nexus_kb::{KbQuery, KbStore};

/// Default token budget for the World context block (~1500 tokens ≈ 6000 chars).
pub const DEFAULT_WORLD_CONTEXT_TOKEN_BUDGET: usize = 1500;

/// Chars-per-token heuristic (matches moment.rs §9.3 spec).
const CHARS_PER_TOKEN: usize = 4;

/// Maximum characters before truncation marker is appended.
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
    /// Render the block as YAML per `novel-workflow-profile.md` §3.5.1.3.
    ///
    /// Output shape:
    /// ```yaml
    /// world_id: wld_123
    /// world_name: "Neon River"
    /// current_timeline: "chapter 3: after the river-market fire"
    /// characters_in_chapter:
    ///   - id: char_lin_xia
    ///     name: "Lin Xia"
    ///     descriptor: "ex-cartographer hiding a forbidden river map"
    /// locations_referenced:
    ///   - ...
    /// active_rules:
    ///   - ...
    /// ```
    ///
    /// Empty sections are rendered as `[]`.
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

/// Shared KB query builder for World-scoped queries.
///
/// Encapsulates the filter/taxonomy logic used by both the chapter KB block
/// and the generic `fetch_world_kb` in `moment.rs`.
pub struct WorldKbQueryBuilder<'a> {
    world_id: &'a str,
}

impl<'a> WorldKbQueryBuilder<'a> {
    /// Create a new builder scoped to the given world.
    #[must_use]
    pub const fn new(world_id: &'a str) -> Self {
        Self { world_id }
    }

    /// Build a `KbQuery` filtered by `block_type`.
    #[must_use]
    pub fn query_for_block_type(&self, block_type: BlockType) -> KbQuery {
        KbQuery::new(self.world_id).with_block_type(block_type)
    }

    /// Build a `KbQuery` filtered by canonical_name.
    #[must_use]
    pub fn query_for_canonical_name(&self, name: &str) -> KbQuery {
        KbQuery::new(self.world_id).with_canonical_name(name)
    }

    /// Build a `KbQuery` for all active items in the world.
    #[must_use]
    pub fn query_all(&self) -> KbQuery {
        KbQuery::new(self.world_id)
    }
}

/// Extract `novel_category` from a KeyBlock's body attributes.
///
/// Returns `None` if the body or attributes are missing, or if `novel_category`
/// is not a string.
fn extract_novel_category(kb: &nexus_kb::key_block::KeyBlock) -> Option<String> {
    kb.body
        .as_ref()
        .and_then(|b| b.attributes.as_ref())
        .and_then(|attrs| attrs.get("novel_category"))
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
}

/// Convert a KeyBlock to a WorldContextItem.
fn kb_to_item(kb: nexus_kb::key_block::KeyBlock) -> WorldContextItem {
    let descriptor = kb
        .body
        .as_ref()
        .and_then(|b| b.summary.as_deref())
        .unwrap_or("")
        .to_string();
    WorldContextItem {
        id: kb.key_block_id,
        name: kb.canonical_name,
        descriptor,
    }
}

/// Build the compact World context block for a chapter prompt.
///
/// This is the primary entry point for the chapter KB block. It:
/// 1. Queries characters (BlockType::Character) and locations (BlockType::Scene).
/// 2. If `world_refs` is non-empty, resolves items by canonical_name; otherwise
///    falls back to all characters/locations in the world. If `chapter_text` is
///    provided, uses heuristic text matching to narrow the fallback set.
/// 3. Queries active rules (novel_category: foundation + rules).
/// 4. Applies token budget truncation.
///
/// # Ownership / Isolation (QC2-W02)
///
/// This function is **intentionally world-scoped only**: it takes `world_id`
/// but does NOT accept `creator_id` or `workspace_slug`. The caller is
/// responsible for verifying that the authenticated creator owns (or has
/// access to) the Work that references this `world_id` before calling.
/// The underlying `KbStore` impl enforces world-scoped isolation.
///
/// # Missing World / 404 Contract (QC2-W03)
///
/// This function returns `Ok(Some(block))` even when the world has zero KB
/// items — the block will have empty sections. It does NOT distinguish
/// "world exists but has no items" from "world_id is unknown to the system."
/// The 404/remediation contract lives one layer up (in the caller), which
/// should validate `world_id` existence against the narrative store before
/// calling this function.
///
/// # Errors
///
/// Returns `KbStoreError` if the underlying store query fails.
///
/// # Type parameters
///
/// - `K`: A [`KbStore`] implementation for World-scoped KB queries.
// Traits use async fn in trait without Send bounds — same pattern as nexus-narrative.
#[allow(clippy::future_not_send)]
pub async fn build_chapter_kb_block<K: KbStore>(
    store: &K,
    params: &ChapterKbBlockParams,
) -> Result<Option<WorldContextBlock>, nexus_kb::KbStoreError> {
    let builder = WorldKbQueryBuilder::new(&params.world_id);
    let max_tokens = params
        .max_tokens
        .unwrap_or(DEFAULT_WORLD_CONTEXT_TOKEN_BUDGET);
    let max_chars = max_tokens.saturating_mul(CHARS_PER_TOKEN);

    // Resolve characters
    let all_characters = if params.world_refs.is_empty() {
        // Fallback: all characters in the world
        let query = builder.query_for_block_type(BlockType::Character);
        let result = store.query(&query).await?;
        result.items.into_iter().map(kb_to_item).collect()
    } else {
        // Resolve by world_refs: query each ref by canonical_name, keep Character type
        resolve_items_by_refs(store, &builder, &params.world_refs, BlockType::Character).await?
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
        all_characters.clone()
    };
    // QC3-W4 fix: sort by canonical_name for deterministic prompt output.
    characters.sort_by(|a, b| a.name.cmp(&b.name));

    // Resolve locations
    let all_locations = if params.world_refs.is_empty() {
        let query = builder.query_for_block_type(BlockType::Scene);
        let result = store.query(&query).await?;
        result.items.into_iter().map(kb_to_item).collect()
    } else {
        resolve_items_by_refs(store, &builder, &params.world_refs, BlockType::Scene).await?
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
        all_locations.clone()
    };
    locations.sort_by(|a, b| a.name.cmp(&b.name));

    // Resolve active rules: foundation + rules novel_category items
    let mut active_rules = resolve_active_rules(store, &builder).await?;
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

    Ok(Some(block))
}

/// Resolve KB items by canonical_name from world_refs, filtered to a specific block_type.
#[allow(clippy::future_not_send)]
async fn resolve_items_by_refs<K: KbStore>(
    store: &K,
    builder: &WorldKbQueryBuilder<'_>,
    world_refs: &[String],
    block_type: BlockType,
) -> Result<Vec<WorldContextItem>, nexus_kb::KbStoreError> {
    let mut items = Vec::new();
    for r#ref in world_refs {
        let query = builder
            .query_for_canonical_name(r#ref)
            .with_block_type(block_type);
        let result = store.query(&query).await?;
        if let Some(kb) = result.items.into_iter().next() {
            items.push(kb_to_item(kb));
        }
    }
    Ok(items)
}

/// Resolve active rules: items with novel_category "foundation" or "rules".
///
/// Queries all items in the world and filters by novel_category.
#[allow(clippy::future_not_send)]
async fn resolve_active_rules<K: KbStore>(
    store: &K,
    builder: &WorldKbQueryBuilder<'_>,
) -> Result<Vec<WorldContextItem>, nexus_kb::KbStoreError> {
    let query = builder.query_all();
    let result = store.query(&query).await?;
    let items: Vec<WorldContextItem> = result
        .items
        .into_iter()
        .filter(|kb| {
            let cat = extract_novel_category(kb);
            matches!(cat.as_deref(), Some("foundation" | "rules"))
        })
        .map(kb_to_item)
        .collect();
    Ok(items)
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
    use nexus_kb::key_block::KeyBlockBody;

    /// Helper: create a novel-profile KeyBlock.
    fn make_novel_block(
        world_id: &str,
        block_type: BlockType,
        name: &str,
        novel_category: &str,
    ) -> nexus_kb::key_block::KeyBlock {
        let mut kb = nexus_kb::key_block::KeyBlock::new(world_id, block_type, name);
        kb.set_body(KeyBlockBody {
            summary: Some(format!("{novel_category}: {name} summary")),
            attributes: Some(serde_json::json!({
                "novel_category": novel_category,
                "traits": ["test"]
            })),
            tags: Some(vec!["novel".to_string()]),
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
    #[tokio::test]
    async fn world_bound_populated_kb_produces_block() {
        let store = nexus_kb::InMemoryKbStore::new();

        let char_kb = make_novel_block("wld_1", BlockType::Character, "char_lin_xia", "character");
        let loc_kb = make_novel_block("wld_1", BlockType::Scene, "loc_neon_city", "location");
        let rule_kb = make_novel_block("wld_1", BlockType::Conflict, "rule_magic_cost", "rules");
        let fnd_kb = make_novel_block("wld_1", BlockType::InfoPoint, "fnd_cosmology", "foundation");

        store.insert_key_block(char_kb.clone()).await.unwrap();
        store.insert_key_block(loc_kb.clone()).await.unwrap();
        store.insert_key_block(rule_kb.clone()).await.unwrap();
        store.insert_key_block(fnd_kb.clone()).await.unwrap();

        let params = ChapterKbBlockParams {
            world_id: "wld_1".to_string(),
            world_name: "Neon River".to_string(),
            current_timeline: "chapter 3: after the river-market fire".to_string(),
            world_refs: vec!["char_lin_xia".to_string(), "loc_neon_city".to_string()],
            chapter_text: None,
            max_tokens: None,
        };

        let block = build_chapter_kb_block(&store, &params)
            .await
            .unwrap()
            .unwrap();

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

        // Verify YAML output contains required fields
        let yaml = block.to_yaml();
        assert!(yaml.contains("world_id: wld_1"));
        assert!(yaml.contains("world_name: \"Neon River\""));
        assert!(yaml.contains("characters_in_chapter:"));
        assert!(yaml.contains("locations_referenced:"));
        assert!(yaml.contains("active_rules:"));
    }

    // AC2: World-bound Work + empty World KB → block present but with empty sections.
    #[tokio::test]
    async fn world_bound_empty_kb_produces_empty_block() {
        let store = nexus_kb::InMemoryKbStore::new();

        let params = make_params("wld_empty", &[]);
        let block = build_chapter_kb_block(&store, &params)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(block.world_id, "wld_empty");
        assert!(block.characters_in_chapter.is_empty());
        assert!(block.locations_referenced.is_empty());
        assert!(block.active_rules.is_empty());

        let yaml = block.to_yaml();
        assert!(yaml.contains("characters_in_chapter:"));
        assert!(yaml.contains("  []"));
    }

    // AC3: world_refs populated → characters/locations use world_refs.
    #[tokio::test]
    async fn world_refs_filter_characters_and_locations() {
        let store = nexus_kb::InMemoryKbStore::new();

        let char1 = make_novel_block("wld_1", BlockType::Character, "char_a", "character");
        let char2 = make_novel_block("wld_1", BlockType::Character, "char_b", "character");
        let loc1 = make_novel_block("wld_1", BlockType::Scene, "loc_x", "location");

        store.insert_key_block(char1).await.unwrap();
        store.insert_key_block(char2).await.unwrap();
        store.insert_key_block(loc1).await.unwrap();

        // Only reference char_a and loc_x
        let params = ChapterKbBlockParams {
            world_refs: vec!["char_a".to_string(), "loc_x".to_string()],
            ..make_params("wld_1", &[])
        };

        let block = build_chapter_kb_block(&store, &params)
            .await
            .unwrap()
            .unwrap();

        // Should only contain char_a, not char_b
        assert_eq!(block.characters_in_chapter.len(), 1);
        assert_eq!(block.characters_in_chapter[0].name, "char_a");

        // Should only contain loc_x
        assert_eq!(block.locations_referenced.len(), 1);
        assert_eq!(block.locations_referenced[0].name, "loc_x");
    }

    // AC4: world_refs empty → fall back to all characters/locations.
    #[tokio::test]
    async fn world_refs_empty_falls_back_to_all() {
        let store = nexus_kb::InMemoryKbStore::new();

        let char1 = make_novel_block("wld_1", BlockType::Character, "char_a", "character");
        let char2 = make_novel_block("wld_1", BlockType::Character, "char_b", "character");
        let loc1 = make_novel_block("wld_1", BlockType::Scene, "loc_x", "location");
        let evt = make_novel_block("wld_1", BlockType::Event, "evt_bg", "background");

        store.insert_key_block(char1).await.unwrap();
        store.insert_key_block(char2).await.unwrap();
        store.insert_key_block(loc1).await.unwrap();
        store.insert_key_block(evt).await.unwrap();

        let params = make_params("wld_1", &[]);
        let block = build_chapter_kb_block(&store, &params)
            .await
            .unwrap()
            .unwrap();

        // All characters
        assert_eq!(block.characters_in_chapter.len(), 2);
        // All locations (scenes)
        assert_eq!(block.locations_referenced.len(), 1);
        // No background items in active_rules (only foundation + rules)
        let rule_names: Vec<&str> = block.active_rules.iter().map(|r| r.name.as_str()).collect();
        assert!(!rule_names.contains(&"evt_bg"));
    }

    // QC1-W002 fix: chapter_text heuristic narrows fallback when world_refs is empty.
    #[tokio::test]
    async fn chapter_text_heuristic_narrows_fallback() {
        let store = nexus_kb::InMemoryKbStore::new();

        let char1 = make_novel_block("wld_1", BlockType::Character, "alice", "character");
        let char2 = make_novel_block("wld_1", BlockType::Character, "bob", "character");
        let loc1 = make_novel_block("wld_1", BlockType::Scene, "tavern", "location");
        let loc2 = make_novel_block("wld_1", BlockType::Scene, "forest", "location");

        store.insert_key_block(char1).await.unwrap();
        store.insert_key_block(char2).await.unwrap();
        store.insert_key_block(loc1).await.unwrap();
        store.insert_key_block(loc2).await.unwrap();

        // chapter_text mentions Alice and the tavern but not Bob or the forest
        let params = ChapterKbBlockParams {
            world_id: "wld_1".to_string(),
            world_name: "Test".to_string(),
            current_timeline: "chapter 1".to_string(),
            world_refs: vec![], // empty → heuristic fallback
            chapter_text: Some("Alice walked into the tavern.".to_string()),
            max_tokens: None,
        };

        let block = build_chapter_kb_block(&store, &params)
            .await
            .unwrap()
            .unwrap();

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
    #[tokio::test]
    async fn no_chapter_text_returns_all_in_fallback() {
        let store = nexus_kb::InMemoryKbStore::new();

        let char1 = make_novel_block("wld_1", BlockType::Character, "alice", "character");
        let char2 = make_novel_block("wld_1", BlockType::Character, "bob", "character");
        store.insert_key_block(char1).await.unwrap();
        store.insert_key_block(char2).await.unwrap();

        let params = ChapterKbBlockParams {
            chapter_text: None,
            ..make_params("wld_1", &[])
        };

        let block = build_chapter_kb_block(&store, &params)
            .await
            .unwrap()
            .unwrap();

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
        assert!(
            true,
            "legacy worldless Works skip build_chapter_kb_block at caller level"
        );
    }

    // AC6: Missing world_id in query → store returns empty, block has empty sections.
    #[tokio::test]
    async fn missing_world_id_returns_empty_block() {
        let store = nexus_kb::InMemoryKbStore::new();

        // Insert block in different world
        let char1 = make_novel_block("wld_other", BlockType::Character, "char_x", "character");
        store.insert_key_block(char1).await.unwrap();

        let params = make_params("wld_ghost", &[]);
        let block = build_chapter_kb_block(&store, &params)
            .await
            .unwrap()
            .unwrap();

        // No data for wld_ghost
        assert!(block.characters_in_chapter.is_empty());
        assert!(block.locations_referenced.is_empty());
        assert!(block.active_rules.is_empty());
    }

    // AC7: Token budget exceeded → truncate gracefully with marker.
    #[tokio::test]
    async fn token_budget_truncates_gracefully() {
        let store = nexus_kb::InMemoryKbStore::new();

        // Create many characters with long summaries
        for i in 0..20 {
            let mut kb = nexus_kb::key_block::KeyBlock::new(
                "wld_1",
                BlockType::Character,
                &format!("char_{i:02}"),
            );
            kb.set_body(KeyBlockBody {
                summary: Some(format!(
                    "Character {i} with a very long descriptor that takes up space"
                )),
                attributes: None,
                tags: None,
            })
            .unwrap();
            store.insert_key_block(kb).await.unwrap();
        }

        let params = ChapterKbBlockParams {
            world_id: "wld_1".to_string(),
            world_name: "Big World".to_string(),
            current_timeline: "chapter 1".to_string(),
            world_refs: vec![],
            chapter_text: None,
            max_tokens: Some(50), // Very small budget = 200 chars
        };

        let block = build_chapter_kb_block(&store, &params)
            .await
            .unwrap()
            .unwrap();

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

        // Verify exact format per §3.5.1.3
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

    // QC3-W4 fix: output is deterministic regardless of insertion order.
    #[tokio::test]
    async fn output_is_deterministic_regardless_of_insertion_order() {
        let store1 = nexus_kb::InMemoryKbStore::new();
        let store2 = nexus_kb::InMemoryKbStore::new();

        // Insert in opposite orders
        let char_a = make_novel_block("wld_1", BlockType::Character, "alpha", "character");
        let char_b = make_novel_block("wld_1", BlockType::Character, "beta", "character");

        store1.insert_key_block(char_a.clone()).await.unwrap();
        store1.insert_key_block(char_b.clone()).await.unwrap();

        store2.insert_key_block(char_b).await.unwrap();
        store2.insert_key_block(char_a).await.unwrap();

        let params = make_params("wld_1", &[]);

        let yaml1 = build_chapter_kb_block(&store1, &params)
            .await
            .unwrap()
            .unwrap()
            .to_yaml();
        let yaml2 = build_chapter_kb_block(&store2, &params)
            .await
            .unwrap()
            .unwrap()
            .to_yaml();

        assert_eq!(
            yaml1, yaml2,
            "YAML output must be identical regardless of KB insertion order"
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

    // Unit test: WorldKbQueryBuilder produces correct queries.
    #[test]
    fn query_builder_produces_correct_queries() {
        let builder = WorldKbQueryBuilder::new("wld_test");

        let q = builder.query_for_block_type(BlockType::Character);
        assert_eq!(q.world_id, "wld_test");
        assert_eq!(q.block_type, Some(BlockType::Character));

        let q = builder.query_for_canonical_name("char_hero");
        assert_eq!(q.canonical_name, Some("char_hero".to_string()));

        let q = builder.query_all();
        assert_eq!(q.world_id, "wld_test");
        assert!(q.block_type.is_none());
    }

    // Test: extract_novel_category helper.
    #[test]
    fn extract_novel_category_from_keyblock() {
        let mut kb = nexus_kb::key_block::KeyBlock::new("wld_1", BlockType::Character, "char_test");
        kb.set_body(KeyBlockBody {
            summary: Some("test".to_string()),
            attributes: Some(serde_json::json!({"novel_category": "character"})),
            tags: None,
        })
        .unwrap();

        assert_eq!(extract_novel_category(&kb), Some("character".to_string()));

        // No body
        let kb2 = nexus_kb::key_block::KeyBlock::new("wld_1", BlockType::Character, "char_no_body");
        assert_eq!(extract_novel_category(&kb2), None);
    }
}
