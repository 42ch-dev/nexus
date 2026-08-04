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

use nexus_knowledge::world_kb::knowledge_entry::WorldKbEntry;

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
    pub before: Vec<WorldKbEntry>,
    /// Default fallback — no hint, `position_hint:"depth"`, unknown hint, or
    /// an `outlet` hint without a paired name. Renders as the V1.149 flat
    /// entry block (no sub-heading) — the neutral-only byte-equivalence
    /// anchor.
    pub fallback: Vec<WorldKbEntry>,
    /// `world.after` — entries with `position_hint:"after_defs"`.
    pub after: Vec<WorldKbEntry>,
    /// `kb.outlet.<name>` — open outlets keyed by the outlet string.
    /// Rendered sorted by `<name>` (`BTreeMap` iteration order is the sort).
    pub outlets: BTreeMap<String, Vec<WorldKbEntry>>,
    /// `style.post_history` — the one reserved well-known outlet (tail of the
    /// lore block, after all open outlets).
    pub post_history: Vec<WorldKbEntry>,
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
pub fn route_slots(matched: Vec<WorldKbEntry>) -> SlotRouting {
    let mut routing = SlotRouting::default();
    for entry in matched {
        let (position_hint, outlet) = placement_of(&entry);
        match (position_hint.as_deref(), outlet.as_deref()) {
            (Some(HINT_BEFORE_DEFS), _) => routing.before.push(entry),
            (Some(HINT_AFTER_DEFS), _) => routing.after.push(entry),
            (Some(HINT_OUTLET), Some(WELL_KNOWN_STYLE_OUTLET)) => routing.post_history.push(entry),
            (Some(HINT_OUTLET), Some(name)) if !name.trim().is_empty() => {
                routing
                    .outlets
                    .entry(name.to_string())
                    .or_default()
                    .push(entry);
            }
            // `depth` (parsed-not-actioned), unknown hints, `outlet` without a
            // paired name or with an empty/whitespace name (would render a
            // nameless `### Outlet: ` heading), and no hint → default fallback
            // (round-trip safe).
            _ => routing.fallback.push(entry),
        }
    }
    routing
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
fn placement_of(entry: &WorldKbEntry) -> (Option<String>, Option<String>) {
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
pub(crate) fn format_entries(entries: &[WorldKbEntry]) -> String {
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

    /// Helper: build a `WorldKbEntry` with a `modules.activation` JSON payload
    /// (or `None` for a fully neutral entry).
    fn entry(name: &str, id: &str, activation: Option<serde_json::Value>) -> WorldKbEntry {
        let mut entry = WorldKbEntry::new("wld_1", BlockType::Character, name);
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

    fn names(entries: &[WorldKbEntry]) -> Vec<&str> {
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
}
