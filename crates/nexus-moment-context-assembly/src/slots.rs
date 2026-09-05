//! V1.150 P0 — preset slot routing for the assembled World-KB lore block
//! (DF-75, spec `fl-l-w5-prompt-control-plane.md` §1.1 / §2).
//!
//! A **thin post-activation step**: it consumes the V1.149-emitted activated
//! candidate list (already filtered and sorted by the activation engine —
//! priority desc, order asc, stable index, `constant:true` band first) and
//! routes each entry into a named, ordered slot **within** the
//! `## World Knowledge Base` section. No new matching logic — V1.149 already
//! decided what fires; this module only shapes assembly output.
//!
//! # Routing table (product-locked meanings, spec §2)
//!
//! | Entry hint | Target slot | Rendered heading |
//! |------------|-------------|------------------|
//! | `position_hint:"before_defs"` | `world.before` | `### World (Before)` |
//! | `position_hint:"after_defs"` | `world.after` | `### World (After)` |
//! | `position_hint:"outlet"` + `outlet:"style.post_history"` | `style.post_history` | `### Style (Post-History)` (tail) |
//! | `position_hint:"outlet"` + `outlet:"<other>"` | `kb.outlet.<name>` | `### Outlet: <name>` (open; sorted by `<name>`) |
//! | no hint / `position_hint:"depth"` / unknown hint | default fallback | existing flat entry block (no sub-heading) |
//!
//! `position_hint:"depth"` is parsed + preserved but **not actioned** (locked
//! Non-Goal — chat-history depth is not Nexus-native); it routes to the
//! default fallback. Unknown `position_hint` values route to the default
//! fallback (round-trip safe, consumer-only discipline). An `outlet` hint
//! without a paired `outlet` name — or with an empty/whitespace one, which
//! would render a nameless `### Outlet: ` heading — also falls back. Unknown
//! `outlet` strings
//! are **not** errors — they open a `kb.outlet.<name>` slot so author packs
//! round-trip without code changes (spec §2).
//!
//! Outlet names are **normalized before routing** (R-001 hardening): names
//! are trimmed (so a `"style.post_history "` near-variant still matches the
//! reserved well-known outlet — and its stage gate — instead of opening a
//! near-duplicate open outlet), and names carrying structural characters
//! (newline/CR, `#`, or any other control char) fall back to the default
//! slot — a crafted name must not break the `### Outlet: <name>` heading or
//! inject a fake sub-heading.
//!
//! # Emit order (spec §2, Q5 provisional lock)
//!
//! Within `## World Knowledge Base` (top → bottom):
//! `### World (Before)` → default fallback → `### World (After)` →
//! `### Outlet: <name>` (sorted by name) → `### Style (Post-History)`.
//! Empty slots are omitted entirely. The `moment.directive` slot is a
//! **top-level** section (`## Moment Directive`) reserved by P0 but never
//! rendered here — P1 fills it (see `MomentContext::moment_directive`).
//!
//! # Byte-equivalence anchor (AC-I1b, HARD)
//!
//! A World whose entries carry no routing hints routes every entry into the
//! default fallback, and [`render_slots`] then produces exactly the V1.149
//! flat block (no sub-headings) — the neutral-only byte-equivalence promise.
//!
//! # Within-slot order
//!
//! Entries are iterated in the V1.149-emitted order (already
//! priority-then-order with the `constant:true` band first — stable engine
//! sort, spec §4) and appended to their slot, so each slot preserves that
//! exact relative order. An entry routed to a slot never re-sorts across
//! slots.

use std::collections::BTreeMap;

use crate::generation::GenerationStage;
use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;

/// Section heading for the `world.before` slot (Q1 provisional lock —
/// `guides/mca-section-audit.md`; product meaning locked in spec §2).
const WORLD_BEFORE_HEADING: &str = "### World (Before)";
/// Section heading for the `world.after` slot (Q1 provisional lock).
const WORLD_AFTER_HEADING: &str = "### World (After)";
/// Section heading prefix for the open `kb.outlet.<name>` slots — rendered as
/// `### Outlet: <name>` (Q1 provisional lock).
const OUTLET_HEADING_PREFIX: &str = "### Outlet: ";
/// Section heading for the `style.post_history` slot (Q1 provisional lock —
/// the one reserved well-known outlet name, tail of the lore block).
const STYLE_POST_HISTORY_HEADING: &str = "### Style (Post-History)";

/// The well-known outlet name reserved by V1.150 (spec §2). Every other
/// `outlet` string opens a `kb.outlet.<name>` slot.
const WELL_KNOWN_STYLE_OUTLET: &str = "style.post_history";

/// The routing hint values actioned by V1.150 (spoke handbook
/// `domain-profile-lore-activation.md` §Position hint values).
const HINT_BEFORE_DEFS: &str = "before_defs";
const HINT_AFTER_DEFS: &str = "after_defs";
const HINT_OUTLET: &str = "outlet";

/// Named, ordered slots produced by routing the V1.149 matched candidate
/// list. Slots keep the V1.149 emitted order within themselves (priority
/// desc, order asc, stable index; `constant:true` band first).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlotRouting {
    /// `world.before` — entries with `position_hint:"before_defs"`.
    pub before: Vec<KnowledgeEntryRecord>,
    /// Default fallback — no hint, `position_hint:"depth"`, unknown hint, or
    /// an `outlet` hint without a paired name. Renders as the V1.149 flat
    /// entry block (no sub-heading) — the neutral-only byte-equivalence
    /// anchor.
    pub fallback: Vec<KnowledgeEntryRecord>,
    /// `world.after` — entries with `position_hint:"after_defs"`.
    pub after: Vec<KnowledgeEntryRecord>,
    /// `kb.outlet.<name>` — open outlets keyed by the outlet string.
    /// Rendered sorted by `<name>` (`BTreeMap` iteration order is the sort).
    pub outlets: BTreeMap<String, Vec<KnowledgeEntryRecord>>,
    /// `style.post_history` — the one reserved well-known outlet (tail of the
    /// lore block, after all open outlets).
    pub post_history: Vec<KnowledgeEntryRecord>,
}

/// One accepted entry's slot assignment.
///
/// The inspector packet `slot_map` row (V1.151 P0, DF-76 spec §2 H2):
/// `entry_id` → slot id (`world.before` | `default` | `world.after` |
/// `kb.outlet.<name>` | `style.post_history` | `moment.directive`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotMapEntry {
    /// The routed entry's stable id.
    pub entry_id: String,
    /// The named slot the entry landed in (product slot ids, spec §2).
    pub slot: String,
}

impl SlotRouting {
    /// Flatten the routed slots into the inspector slot map — every entry
    /// that survived the stage gate mapped to its slot id (spec §2 H2).
    ///
    /// Emit order follows the render order (top → bottom, spec §2 / Q5):
    /// `world.before` → `default` → `world.after` → `kb.outlet.<name>`
    /// (sorted by name, `BTreeMap` iteration) → `style.post_history`. The
    /// reserved `moment.directive` slot is never produced here — it is a
    /// top-level section, not a World-KB routing slot; the assembly capture
    /// appends a synthetic entry when a directive injects.
    #[must_use]
    pub fn to_slot_map(&self) -> Vec<SlotMapEntry> {
        let capacity = self
            .before
            .len()
            .saturating_add(self.fallback.len())
            .saturating_add(self.after.len())
            .saturating_add(self.post_history.len())
            .saturating_add(self.outlets.values().map(Vec::len).sum::<usize>());
        let mut map = Vec::with_capacity(capacity);
        for entry in &self.before {
            map.push(SlotMapEntry {
                entry_id: entry.entry_id.clone(),
                slot: "world.before".to_string(),
            });
        }
        for entry in &self.fallback {
            map.push(SlotMapEntry {
                entry_id: entry.entry_id.clone(),
                slot: "default".to_string(),
            });
        }
        for entry in &self.after {
            map.push(SlotMapEntry {
                entry_id: entry.entry_id.clone(),
                slot: "world.after".to_string(),
            });
        }
        for (name, entries) in &self.outlets {
            for entry in entries {
                map.push(SlotMapEntry {
                    entry_id: entry.entry_id.clone(),
                    slot: format!("kb.outlet.{name}"),
                });
            }
        }
        for entry in &self.post_history {
            map.push(SlotMapEntry {
                entry_id: entry.entry_id.clone(),
                slot: "style.post_history".to_string(),
            });
        }
        map
    }
}

/// Route the V1.149-emitted matched candidate list into named slots.
///
/// Consumes the list and moves each entry into exactly one slot; entries are
/// appended in emitted order, so every slot preserves the V1.149
/// priority-then-order (with the `constant:true` band first). Source entries
/// are **not** mutated and activation is **not** re-fired — routing only
/// reads `modules.activation.position_hint` / `outlet` and shapes assembly
/// output.
#[must_use]
pub fn route_slots(matched: Vec<KnowledgeEntryRecord>) -> SlotRouting {
    let mut routing = SlotRouting::default();
    for entry in matched {
        let (position_hint, outlet) = placement_of(&entry);
        // Outlet names are normalized before routing (R-001): trimmed so the
        // reserved `style.post_history` matches even with stray whitespace,
        // and names carrying structural characters (newline/CR/`#`/control
        // chars) fall back instead of rendering an injected `### Outlet:`
        // sub-heading.
        match (position_hint.as_deref(), outlet.as_deref().map(str::trim)) {
            (Some(HINT_BEFORE_DEFS), _) => routing.before.push(entry),
            (Some(HINT_AFTER_DEFS), _) => routing.after.push(entry),
            (Some(HINT_OUTLET), Some(WELL_KNOWN_STYLE_OUTLET)) => routing.post_history.push(entry),
            (Some(HINT_OUTLET), Some(name)) if is_safe_outlet_name(name) && !name.is_empty() => {
                routing
                    .outlets
                    .entry(name.to_string())
                    .or_default()
                    .push(entry);
            }
            // `depth` (parsed-not-actioned), unknown hints, `outlet` without a
            // paired name, empty/whitespace names, and structural-char names
            // (R-001 — would break the `### Outlet:` heading) → default
            // fallback (round-trip safe).
            _ => routing.fallback.push(entry),
        }
    }
    routing
}

/// V1.150 P2 — apply the spec §4 generation-stage fill matrix to the
/// V1.149-emitted matched candidate list, **before** slot routing.
///
/// The activation engine already decided what fires; this step shapes
/// assembly output by the request's generation stage. Matrix cells
/// (spec §4, `fl-l-w5-prompt-control-plane.md`):
///
/// | Slot family | `intake` | `research` | `produce` | `review` | `persist` | `work_maintenance` | `system_maintenance` | `unspecified` |
/// |---|---|---|---|---|---|---|---|---|
/// | `world.before` / default fallback / `world.after` / `kb.outlet.*` | on | on | on | on | on | on | **off** | on |
/// | `style.post_history` | **off** | **off** | on | on | **off** | **off** | **off** | on (current behavior) |
///
/// Every cell maps to either (a) a **tested gate** or (b) **current-behavior**
/// (no gate) — no silent half-gating:
///
/// - (a) `system_maintenance` runs **no lore slots at all** (the row is all
///   off) — preserves the `_system.*` isolation invariant (spec §4 rationale).
/// - (a) `style.post_history` fills **only** for `produce` + `review`; its
///   entries are **excluded** from the assembly for `intake` / `research` /
///   `persist` / `work_maintenance` / `system_maintenance` (tail-of-block
///   style/post-history guidance is noise when no prose is being generated
///   or revised, spec §4 rationale). Excluded entries are dropped, NOT
///   re-routed to the fallback — the product treats the guidance as noise
///   for that stage, and re-routing would still inject it.
/// - (b) `world.before` / default / `world.after` / `kb.outlet.*` fill for
///   every narrative stage + `work_maintenance` + `unspecified` — factual
///   lore is relevant whenever lore is relevant; activation already filtered
///   for relevance, the product does not second-guess it (spec §4 rationale).
///   No gate — these cells continue current behavior.
/// - (b) `unspecified` (direct `assemble-moment` CLI without a preset
///   context, or `None` on the request) keeps **every** slot on — the
///   inspector/debug path sees the full picture and the neutral
///   byte-equivalence anchor (spec §4; AC-I1b).
///
/// The activation trace is captured before this step (`assemble_moment`) and
/// still reflects the activation engine's verdict — this gate only shapes
/// assembly output.
#[must_use]
pub fn apply_stage_gate(
    matched: Vec<KnowledgeEntryRecord>,
    stage: Option<GenerationStage>,
) -> Vec<KnowledgeEntryRecord> {
    match stage.unwrap_or(GenerationStage::Unspecified) {
        // (a) tested gate — `system_maintenance`: no lore slots at all
        // (spec §4 row; `_system.*` isolation invariant).
        GenerationStage::SystemMaintenance => Vec::new(),
        // (a) tested gate — `style.post_history` off for every
        // non-`produce`/`review` stage; all other slots stay on
        // ((b) current-behavior).
        GenerationStage::Intake
        | GenerationStage::Research
        | GenerationStage::Persist
        | GenerationStage::WorkMaintenance => matched
            .into_iter()
            .filter(|entry| !routes_to_style_post_history(entry))
            .collect(),
        // (b) current-behavior — `produce` + `review` fill every slot
        // (incl. `style.post_history`); `unspecified` (direct CLI /
        // inspector path) keeps all slots on (spec §4 rows).
        GenerationStage::Produce | GenerationStage::Review | GenerationStage::Unspecified => {
            matched
        }
    }
}

/// True when the entry's placement routes it to the reserved
/// `style.post_history` slot (the one well-known outlet name, spec §2).
///
/// The outlet name is compared **trimmed** so the near-variant
/// `"style.post_history "` (trailing whitespace) is treated as the same
/// reserved outlet — it must not open a near-duplicate `kb.outlet.*` slot or
/// bypass the generation-stage gate (R-001).
fn routes_to_style_post_history(entry: &KnowledgeEntryRecord) -> bool {
    matches!(
        placement_of(entry),
        (Some(hint), Some(outlet)) if hint == HINT_OUTLET && outlet.trim() == WELL_KNOWN_STYLE_OUTLET
    )
}

/// True when an outlet name is safe to render inside a `### Outlet: <name>`
/// heading: no newline/CR (would break the heading and inject a second
/// section), no `#` (would fake a nested sub-heading), and no other control
/// characters (R-001). Unsafe names fall back like empty/whitespace ones —
/// the router opens a slot only for well-formed names.
fn is_safe_outlet_name(name: &str) -> bool {
    name.chars()
        .all(|c| c != '\n' && c != '\r' && c != '#' && !c.is_control())
}

/// Render the routed slots into the `## World Knowledge Base` body.
///
/// Emit order (top → bottom, spec §2 / Q5): `### World (Before)` → default
/// fallback (existing flat entry block, **no** sub-heading) → `### World
/// (After)` → `### Outlet: <name>` (sorted by name) → `### Style
/// (Post-History)`. Empty slots are omitted entirely.
///
/// Returns `None` when every slot is empty (caller omits the whole World-KB
/// section — same as V1.149).
#[must_use]
pub fn render_slots(routing: &SlotRouting) -> Option<String> {
    let mut blocks: Vec<String> = Vec::new();

    if !routing.before.is_empty() {
        blocks.push(format!(
            "{WORLD_BEFORE_HEADING}\n\n{}",
            format_entries(&routing.before)
        ));
    }
    if !routing.fallback.is_empty() {
        blocks.push(format_entries(&routing.fallback));
    }
    if !routing.after.is_empty() {
        blocks.push(format!(
            "{WORLD_AFTER_HEADING}\n\n{}",
            format_entries(&routing.after)
        ));
    }
    for (name, entries) in &routing.outlets {
        blocks.push(format!(
            "{OUTLET_HEADING_PREFIX}{name}\n\n{}",
            format_entries(entries)
        ));
    }
    if !routing.post_history.is_empty() {
        blocks.push(format!(
            "{STYLE_POST_HISTORY_HEADING}\n\n{}",
            format_entries(&routing.post_history)
        ));
    }

    if blocks.is_empty() {
        None
    } else {
        Some(blocks.join("\n\n"))
    }
}

/// Read the parsed-but-preserved placement fields
/// (`modules.activation.position_hint` / `outlet`) from a matched entry.
///
/// V1.149 parsed and preserved these fields (spec §2 — "parsed +
/// round-tripped; not actioned"); V1.150 P0 actions them for slot routing.
/// Reading the two strings is not matching logic — the activation engine
/// already decided what fires. `None`/malformed values fall back to
/// "no hint".
fn placement_of(entry: &KnowledgeEntryRecord) -> (Option<String>, Option<String>) {
    let Some(modules) = entry.modules.as_ref() else {
        return (None, None);
    };
    let Some(activation) = modules.get("activation") else {
        return (None, None);
    };
    let position_hint = activation
        .get("position_hint")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);
    let outlet = activation
        .get("outlet")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);
    (position_hint, outlet)
}

/// Format `WorldKB` entries into markdown context text lines — the exact
/// V1.149 flat-block format (`- **name** [BlockType]: summary`). The default
/// fallback slot renders through this function so neutral-only output stays
/// byte-identical to V1.149 (AC-I1b); the activation off-switch in
/// `assemble_moment` also renders through it so flag-off output stays the
/// V1.149 flat block (no slot sub-headings).
pub(crate) fn format_entries(entries: &[KnowledgeEntryRecord]) -> String {
    entries
        .iter()
        .map(|kb| {
            let summary = kb
                .body
                .as_ref()
                .and_then(|b| b.summary.as_ref())
                .map_or("(no summary)", std::string::String::as_str);
            format!(
                "- **{}** [{:?}]: {summary}",
                kb.canonical_name, kb.block_type
            )
        })
        .collect::<Vec<String>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::BlockType;

    /// Helper: build a `KnowledgeEntryRecord` with a `modules.activation` JSON payload
    /// (or `None` for a fully neutral entry).
    fn entry(name: &str, id: &str, activation: Option<serde_json::Value>) -> KnowledgeEntryRecord {
        let mut entry = KnowledgeEntryRecord::new("wld_1", BlockType::Character, name);
        entry.entry_id = id.to_string();
        entry.modules = activation.map(|a| serde_json::json!({ "activation": a }));
        entry
    }

    fn with_hint(hint: &str) -> serde_json::Value {
        serde_json::json!({ "position_hint": hint })
    }

    fn with_outlet(outlet: &str) -> serde_json::Value {
        serde_json::json!({ "position_hint": "outlet", "outlet": outlet })
    }

    fn names(entries: &[KnowledgeEntryRecord]) -> Vec<&str> {
        entries.iter().map(|e| e.canonical_name.as_str()).collect()
    }

    #[test]
    fn before_defs_routes_to_world_before() {
        let routing = route_slots(vec![entry("Rules", "kb_1", Some(with_hint("before_defs")))]);
        assert_eq!(names(&routing.before), vec!["Rules"]);
        assert!(routing.fallback.is_empty());
        assert!(routing.after.is_empty());
        assert!(routing.outlets.is_empty());
        assert!(routing.post_history.is_empty());
    }

    #[test]
    fn after_defs_routes_to_world_after() {
        let routing = route_slots(vec![entry(
            "Reminders",
            "kb_2",
            Some(with_hint("after_defs")),
        )]);
        assert_eq!(names(&routing.after), vec!["Reminders"]);
        assert!(routing.before.is_empty());
        assert!(routing.fallback.is_empty());
        assert!(routing.outlets.is_empty());
        assert!(routing.post_history.is_empty());
    }

    #[test]
    fn style_post_history_outlet_routes_to_tail_slot() {
        let routing = route_slots(vec![entry(
            "PostStyle",
            "kb_3",
            Some(with_outlet("style.post_history")),
        )]);
        assert_eq!(names(&routing.post_history), vec!["PostStyle"]);
        assert!(routing.outlets.is_empty(), "reserved outlet is not open");
        assert!(routing.fallback.is_empty());
    }

    #[test]
    fn open_outlets_are_named_and_sorted_by_name() {
        let routing = route_slots(vec![
            entry("LoreZ", "kb_z", Some(with_outlet("zone.z"))),
            entry("LoreA", "kb_a", Some(with_outlet("aether"))),
        ]);
        let keys: Vec<&str> = routing.outlets.keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["aether", "zone.z"], "outlets sort by name");
        assert_eq!(names(&routing.outlets["aether"]), vec!["LoreA"]);
        assert_eq!(names(&routing.outlets["zone.z"]), vec!["LoreZ"]);
    }

    #[test]
    fn no_hint_routes_to_default_fallback() {
        let routing = route_slots(vec![entry("Hero", "kb_n", None)]);
        assert_eq!(names(&routing.fallback), vec!["Hero"]);
        assert!(routing.before.is_empty());
        assert!(routing.after.is_empty());
    }

    #[test]
    fn depth_hint_routes_to_default_fallback() {
        // `position_hint:"depth"` is parsed + preserved but NOT actioned
        // (locked Non-Goal) → default fallback.
        let routing = route_slots(vec![entry("DepthLore", "kb_d", Some(with_hint("depth")))]);
        assert_eq!(names(&routing.fallback), vec!["DepthLore"]);
    }

    #[test]
    fn unknown_hint_routes_to_default_fallback() {
        // Unknown `position_hint` values are ignored for routing (consumer-
        // only, round-trip safe) → default fallback.
        let routing = route_slots(vec![entry("OddLore", "kb_u", Some(with_hint("sideways")))]);
        assert_eq!(names(&routing.fallback), vec!["OddLore"]);
    }

    #[test]
    fn outlet_hint_without_name_routes_to_default_fallback() {
        // `outlet` hint with no paired outlet name is malformed → fallback
        // (no error, no invented outlet).
        let routing = route_slots(vec![entry(
            "Nameless",
            "kb_x",
            Some(serde_json::json!({ "position_hint": "outlet" })),
        )]);
        assert_eq!(names(&routing.fallback), vec!["Nameless"]);
        assert!(routing.outlets.is_empty());
        assert!(routing.post_history.is_empty());
    }

    #[test]
    fn outlet_without_outlet_hint_is_ignored_for_routing() {
        // The `outlet` field is only meaningful paired with `position_hint:
        // "outlet"` (handbook); a bare outlet string must not open a slot.
        let routing = route_slots(vec![entry(
            "BareOutlet",
            "kb_b",
            Some(serde_json::json!({ "outlet": "aether" })),
        )]);
        assert_eq!(names(&routing.fallback), vec!["BareOutlet"]);
        assert!(routing.outlets.is_empty());
    }

    #[test]
    fn empty_or_whitespace_outlet_name_routes_to_default_fallback() {
        // `position_hint:"outlet"` with an empty/whitespace `outlet` string
        // would open a nameless `kb.outlet.<"">` slot and render a bare
        // `### Outlet: ` heading — route to the default fallback instead.
        for bad_name in ["", "   "] {
            let routing = route_slots(vec![entry(
                "NamelessOutlet",
                "kb_e",
                Some(with_outlet(bad_name)),
            )]);
            assert_eq!(
                names(&routing.fallback),
                vec!["NamelessOutlet"],
                "empty/whitespace outlet name must fall back ({bad_name:?})"
            );
            assert!(
                routing.outlets.is_empty(),
                "no outlet slot may be opened for {bad_name:?}"
            );
            assert!(
                routing.post_history.is_empty(),
                "no style slot for {bad_name:?}"
            );
            let rendered = render_slots(&routing).expect("fallback present");
            assert!(
                !rendered.contains("### Outlet: "),
                "no nameless outlet heading for {bad_name:?}, got: {rendered}"
            );
        }
    }

    #[test]
    fn style_outlet_with_trailing_space_still_matches_reserved() {
        // R-001: a `"style.post_history "` near-variant (trailing whitespace)
        // must route to the reserved tail slot — not open a near-duplicate
        // `kb.outlet."style.post_history "` slot (which would also bypass the
        // generation-stage gate).
        for variant in [
            "style.post_history ",
            "  style.post_history",
            " style.post_history ",
        ] {
            let routing = route_slots(vec![entry(
                "PostStyle",
                "kb_ph",
                Some(with_outlet(variant)),
            )]);
            assert_eq!(
                names(&routing.post_history),
                vec!["PostStyle"],
                "near-variant {variant:?} must hit the reserved style slot"
            );
            assert!(
                routing.outlets.is_empty(),
                "no open outlet may be opened for {variant:?}"
            );
        }
    }

    #[test]
    fn outlet_names_with_structural_chars_fall_back() {
        // R-001: outlet names carrying newline/CR/`#`/control chars would
        // break the `### Outlet: <name>` heading (newline injects a second
        // section; `#` fakes a nested sub-heading) — they must fall back and
        // never render as an outlet heading.
        for bad_name in ["zone.\nFake", "zone\rFake", "zone#Fake", "zone\u{0000}Fake"] {
            let routing = route_slots(vec![entry("Lore", "kb_s", Some(with_outlet(bad_name)))]);
            assert_eq!(
                names(&routing.fallback),
                vec!["Lore"],
                "structural outlet name must fall back ({bad_name:?})"
            );
            assert!(
                routing.outlets.is_empty(),
                "no outlet slot may be opened for {bad_name:?}"
            );
            let rendered = render_slots(&routing).expect("fallback present");
            assert!(
                !rendered.contains("### Outlet: "),
                "no outlet heading for {bad_name:?}, got: {rendered:?}"
            );
            assert!(
                !rendered.contains("\n### "),
                "no injected sub-heading for {bad_name:?}, got: {rendered:?}"
            );
        }
    }

    #[test]
    fn hint_wins_over_outlet_when_both_present() {
        // R-003 coverage: `position_hint` takes precedence over a paired
        // `outlet` name — `before_defs`/`after_defs` route to their slots even
        // when the entry also carries an outlet (match-arm order; the outlet
        // hint only opens a slot when the hint IS `outlet`).
        let routing = route_slots(vec![
            entry(
                "Rules",
                "kb_1",
                Some(serde_json::json!({ "position_hint": "before_defs", "outlet": "aether" })),
            ),
            entry(
                "Reminders",
                "kb_2",
                Some(serde_json::json!({
                    "position_hint": "after_defs",
                    "outlet": "style.post_history"
                })),
            ),
            entry(
                "PostStyle",
                "kb_3",
                Some(serde_json::json!({
                    "position_hint": "before_defs",
                    "outlet": "style.post_history"
                })),
            ),
        ]);
        assert_eq!(
            names(&routing.before),
            vec!["Rules", "PostStyle"],
            "before_defs hint beats the paired outlet"
        );
        assert_eq!(
            names(&routing.after),
            vec!["Reminders"],
            "after_defs hint beats the paired outlet"
        );
        assert!(routing.outlets.is_empty());
        assert!(routing.post_history.is_empty());
    }

    #[test]
    fn within_slot_keeps_emitted_order() {
        // The matched list arrives in V1.149 emit order (priority desc, order
        // asc, stable; constant band first). Routing appends in that order,
        // so each slot preserves it — no re-sort across slots.
        let routing = route_slots(vec![
            entry("High", "kb_h", None),
            entry("Mid", "kb_m", None),
            entry("Low", "kb_l", None),
        ]);
        assert_eq!(names(&routing.fallback), vec!["High", "Mid", "Low"]);
    }

    #[test]
    fn routing_does_not_mutate_source_entries() {
        // Slots only shape assembly output: routed entries keep their modules
        // JSON byte-identical (no mutation, no activation re-fire).
        let activation = serde_json::json!({
            "keys": ["king"],
            "constant": true,
            "position_hint": "before_defs"
        });
        let modules = Some(serde_json::json!({ "activation": activation }));
        let src = entry("Rules", "kb_1", Some(activation));
        let src_before = serde_json::to_string(&src).expect("entry serializes");

        let routing = route_slots(vec![src]);
        let routed = &routing.before[0];
        let src_after = serde_json::to_string(routed).expect("entry serializes");
        assert_eq!(src_before, src_after, "routed entry must be byte-identical");
        assert_eq!(routed.modules, modules, "modules preserved verbatim");
    }

    #[test]
    fn render_emit_order_matches_spec() {
        // One entry per slot; rendered order must be: World (Before) → flat
        // fallback block (no heading) → World (After) → Outlet: aether →
        // Outlet: zone.z (sorted) → Style (Post-History) (tail).
        let routing = route_slots(vec![
            entry(
                "PostStyle",
                "kb_ph",
                Some(with_outlet("style.post_history")),
            ),
            entry("LoreZ", "kb_z", Some(with_outlet("zone.z"))),
            entry("Reminders", "kb_af", Some(with_hint("after_defs"))),
            entry("Hero", "kb_fb", None),
            entry("LoreA", "kb_a", Some(with_outlet("aether"))),
            entry("Rules", "kb_bf", Some(with_hint("before_defs"))),
        ]);
        let rendered = render_slots(&routing).expect("slots present");

        let before_pos = rendered.find("### World (Before)").expect("before slot");
        let hero_pos = rendered.find("- **Hero**").expect("fallback block");
        let after_pos = rendered.find("### World (After)").expect("after slot");
        let outlet_aether_pos = rendered.find("### Outlet: aether").expect("outlet aether");
        let outlet_zone_pos = rendered.find("### Outlet: zone.z").expect("outlet zone.z");
        let style_pos = rendered
            .find("### Style (Post-History)")
            .expect("style slot");
        let hero_last_pos = rendered.rfind("- **Hero**").expect("fallback block");
        assert!(
            before_pos < hero_pos
                && hero_last_pos < after_pos
                && after_pos < outlet_aether_pos
                && outlet_aether_pos < outlet_zone_pos
                && outlet_zone_pos < style_pos,
            "emit order must be World (Before) → fallback → World (After) → \
             outlets (sorted) → Style (Post-History)"
        );
    }

    #[test]
    fn render_fallback_only_is_v149_flat_block() {
        // Neutral-only: every entry routes to the default fallback, and the
        // rendered body is byte-identical to the V1.149 flat block format
        // (no sub-headings) — the AC-I1b anchor at the render level.
        let routing = route_slots(vec![
            entry("Hero", "kb_1", None),
            entry("Castle", "kb_2", None),
        ]);
        let rendered = render_slots(&routing).expect("fallback present");
        assert_eq!(
            rendered,
            "- **Hero** [Character]: (no summary)\n- **Castle** [Character]: (no summary)"
        );
        assert!(
            !rendered.contains("### "),
            "no sub-headings when neutral-only"
        );
    }

    #[test]
    fn render_omits_empty_slots() {
        // Only `world.before` populated → only its sub-heading renders.
        let routing = route_slots(vec![entry("Rules", "kb_1", Some(with_hint("before_defs")))]);
        let rendered = render_slots(&routing).expect("before present");
        assert_eq!(
            rendered,
            "### World (Before)\n\n- **Rules** [Character]: (no summary)"
        );
    }

    #[test]
    fn render_none_when_every_slot_empty() {
        let routing = route_slots(Vec::new());
        assert_eq!(render_slots(&routing), None);
    }

    // ── V1.150 P2: generation-stage gate (spec §4 / AC-I4) ────────────────

    /// A mixed matched list covering every routing shape: `before_defs`,
    /// `after_defs`, the reserved style outlet, two open outlets, and a
    /// no-hint neutral entry.
    fn mixed_matched_list() -> Vec<KnowledgeEntryRecord> {
        vec![
            entry("Rules", "kb_bf", Some(with_hint("before_defs"))),
            entry("Hero", "kb_fb", None),
            entry("Reminders", "kb_af", Some(with_hint("after_defs"))),
            entry("LoreA", "kb_a", Some(with_outlet("aether"))),
            entry("LoreZ", "kb_z", Some(with_outlet("zone.z"))),
            entry(
                "PostStyle",
                "kb_ph",
                Some(with_outlet("style.post_history")),
            ),
        ]
    }

    #[test]
    fn stage_gate_keeps_style_slot_for_produce_and_review() {
        // AC-I4 on-side: `style.post_history` fills for `produce` + `review`.
        for stage in [GenerationStage::Produce, GenerationStage::Review] {
            let gated = apply_stage_gate(mixed_matched_list(), Some(stage));
            assert!(
                names(&gated).contains(&"PostStyle"),
                "style entry must survive {stage}"
            );
            assert_eq!(gated.len(), 6, "{stage}: nothing dropped");
        }
    }

    #[test]
    fn stage_gate_excludes_style_slot_for_non_produce_review_stages() {
        // AC-I4 off-side: `style.post_history` is off for intake / research /
        // persist / work_maintenance. Only the style entry is excluded —
        // every other slot keeps filling (current-behavior cells).
        for stage in [
            GenerationStage::Intake,
            GenerationStage::Research,
            GenerationStage::Persist,
            GenerationStage::WorkMaintenance,
        ] {
            let gated = apply_stage_gate(mixed_matched_list(), Some(stage));
            assert!(
                !names(&gated).contains(&"PostStyle"),
                "style entry must be excluded for {stage}"
            );
            assert_eq!(gated.len(), 5, "{stage}: only the style entry is gated off");
            for kept in ["Rules", "Hero", "Reminders", "LoreA", "LoreZ"] {
                assert!(names(&gated).contains(&kept), "{stage}: {kept} kept");
            }
        }
    }

    #[test]
    fn stage_gate_excludes_trailing_space_style_variant() {
        // R-001: the trimmed comparison in `routes_to_style_post_history`
        // closes the near-variant bypass — a `"style.post_history "` entry is
        // gated off for `persist` just like the exact name.
        let gated = apply_stage_gate(
            vec![entry(
                "PostStyle",
                "kb_ph",
                Some(with_outlet("style.post_history ")),
            )],
            Some(GenerationStage::Persist),
        );
        assert_eq!(
            gated,
            Vec::<KnowledgeEntryRecord>::new(),
            "trailing-space style variant must be gated off for persist"
        );
    }

    #[test]
    fn stage_gate_system_maintenance_runs_no_lore_slots() {
        // `system_maintenance`: the whole row is off — no lore slots at all
        // (spec §4; `_system.*` isolation invariant).
        assert_eq!(
            apply_stage_gate(
                mixed_matched_list(),
                Some(GenerationStage::SystemMaintenance)
            ),
            Vec::<KnowledgeEntryRecord>::new(),
            "system_maintenance must produce no lore entries"
        );
    }

    #[test]
    fn stage_gate_unspecified_keeps_all_slots_on() {
        // `unspecified` (direct CLI / inspector path, and the `None` default):
        // every slot fills — current behavior (spec §4 row; AC-I1b anchor).
        for stage in [None, Some(GenerationStage::Unspecified)] {
            let gated = apply_stage_gate(mixed_matched_list(), stage);
            assert_eq!(
                names(&gated),
                vec!["Rules", "Hero", "Reminders", "LoreA", "LoreZ", "PostStyle"],
                "unspecified ({stage:?}) keeps all slots on"
            );
        }
    }

    #[test]
    fn stage_gate_style_entries_drop_not_reroute_to_fallback() {
        // The gate EXCLUDES style entries for off-stages — they are not
        // re-routed into the fallback (which would still inject the
        // post-history guidance the product treats as noise).
        let gated = apply_stage_gate(mixed_matched_list(), Some(GenerationStage::Persist));
        let routing = route_slots(gated);
        assert!(
            routing.post_history.is_empty(),
            "persist: style slot must stay empty"
        );
        assert!(
            !names(&routing.fallback).contains(&"PostStyle"),
            "persist: style entry must not leak into the fallback"
        );
    }

    #[test]
    fn stage_gate_render_persist_omits_style_heading() {
        // End-to-end at the render level: a persist-stage assembly renders
        // every slot heading except `### Style (Post-History)`.
        let gated = apply_stage_gate(mixed_matched_list(), Some(GenerationStage::Persist));
        let rendered = render_slots(&route_slots(gated)).expect("slots present");
        assert!(!rendered.contains("### Style (Post-History)"));
        assert!(rendered.contains("### World (Before)"));
        assert!(rendered.contains("### World (After)"));
        assert!(rendered.contains("### Outlet: aether"));
        assert!(rendered.contains("### Outlet: zone.z"));
        assert!(rendered.contains("- **Hero**"));
    }

    #[test]
    fn stage_gate_unspecified_renders_byte_equivalent_to_ungated() {
        // `None`/`unspecified` on the gate is byte-identical to not running
        // the gate at all — the neutral path (AC-I1b at the slot layer).
        let gated = apply_stage_gate(mixed_matched_list(), None);
        let ungated = mixed_matched_list();
        assert_eq!(
            render_slots(&route_slots(gated)),
            render_slots(&route_slots(ungated)),
            "unspecified gate must not change rendered bytes"
        );
    }
}
