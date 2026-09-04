//! V1.150 P0 — end-to-end slot actioning through `assemble_moment` (DF-75,
//! spec §2 / Q5 provisional locks).
//!
//! The V1.149 activation engine fires entries (constant seeds + keyword
//! matches) and emits them in priority-then-order with the `constant:true`
//! band first; the slot-routing step then routes each fired entry into its
//! named slot within `## World Knowledge Base`:
//!
//! `### World (Before)` → default fallback (no sub-heading) →
//! `### World (After)` → `### Outlet: <name>` (sorted by name) →
//! `### Style (Post-History)` (tail).
//!
//! Determinism: every entry carries a distinct `priority`, so the engine's
//! stable sort fully determines the emitted order regardless of
//! `InMemoryKbStore` HashMap iteration order.

#![allow(clippy::manual_string_new, clippy::doc_markdown)]

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;
use nexus_knowledge::world_kb::{InMemoryKbStore, KbStore};
use nexus_knowledge::InMemoryKnowledgeStore;
use nexus_moment_context_assembly::stage0::Stage0Assembly;
use nexus_moment_context_assembly::{assemble_moment, MomentRequest};
use nexus_narrative::InMemoryNarrativeGateway;

const WORLD_ID: &str = "wld_slots_e2e";

/// Build a `KnowledgeEntryRecord` with the given `modules.activation` payload.
fn entry(name: &str, id: &str, activation: &serde_json::Value) -> KnowledgeEntryRecord {
    let mut entry = KnowledgeEntryRecord::new(WORLD_ID, BlockType::Character, name);
    entry.entry_id = id.to_string();
    entry.modules = Some(serde_json::json!({ "activation": activation }));
    entry
}

/// Seed the full routing-matrix fixture. Every entry fires (constant seeds
/// plus one keyword-fired non-constant entry) and carries a distinct
/// priority so the emitted order is deterministic.
async fn seed_slot_fixture(kb: &InMemoryKbStore) {
    let seeds: &[(&str, &str, serde_json::Value)] = &[
        (
            "WorldBefore",
            "kb_bf",
            serde_json::json!({"constant": true, "priority": 90, "position_hint": "before_defs"}),
        ),
        (
            "LoreMidA",
            "kb_m1",
            serde_json::json!({"constant": true, "priority": 80}),
        ),
        (
            "LoreMidB",
            "kb_m2",
            serde_json::json!({"constant": true, "priority": 70}),
        ),
        (
            "WorldAfter",
            "kb_af",
            serde_json::json!({"constant": true, "priority": 60, "position_hint": "after_defs"}),
        ),
        (
            "LoreZ",
            "kb_z",
            serde_json::json!({"constant": true, "priority": 50,
                "position_hint": "outlet", "outlet": "zone.z"}),
        ),
        (
            "LoreA",
            "kb_a",
            serde_json::json!({"constant": true, "priority": 40,
                "position_hint": "outlet", "outlet": "aether"}),
        ),
        (
            "PostStyle",
            "kb_ph",
            serde_json::json!({"constant": true, "priority": 30,
                "position_hint": "outlet", "outlet": "style.post_history"}),
        ),
        (
            "DepthLore",
            "kb_dp",
            serde_json::json!({"constant": true, "priority": 20, "position_hint": "depth"}),
        ),
        (
            "OddLore",
            "kb_uk",
            serde_json::json!({"constant": true, "priority": 10, "position_hint": "sideways"}),
        ),
        // Non-constant band: fires on "king" (present in the Stage-0 scan
        // text) — must sort AFTER the constant band despite the higher
        // priority (V1.149 spec §4 within-slot rule).
        (
            "LoreMidC",
            "kb_m3",
            serde_json::json!({"keys": ["king"], "priority": 100}),
        ),
    ];
    for (name, id, activation) in seeds {
        kb.insert_knowledge_entry(entry(name, id, activation))
            .await
            .expect("insert kb entry");
    }
}

#[tokio::test]
async fn slots_actioning_routes_and_orders_end_to_end() {
    let narrative = InMemoryNarrativeGateway::new(InMemoryKbStore::new());
    let kb = InMemoryKbStore::new();
    let knowledge = InMemoryKnowledgeStore::new();
    seed_slot_fixture(&kb).await;

    let stage0 = Stage0Assembly {
        personality: "A king rules the land.".to_string(),
        experience: "10 years.".to_string(),
        user_prompt: "Write chapter 3.".to_string(),
        ..Stage0Assembly::default()
    };
    let request = MomentRequest::new(stage0).with_world(WORLD_ID);
    let ctx = assemble_moment(&request, &narrative, &kb, &knowledge).await;
    let kb_text = ctx.world_kb.clone().expect("world_kb must be present");

    // 1. Slot sections render in the locked emit order (spec §2 / Q5):
    //    World (Before) → fallback → World (After) → Outlet: aether →
    //    Outlet: zone.z (sorted) → Style (Post-History).
    let pos_before = kb_text.find("### World (Before)").expect("before slot");
    let pos_after = kb_text.find("### World (After)").expect("after slot");
    let pos_outlet_a = kb_text.find("### Outlet: aether").expect("outlet aether");
    let pos_outlet_z = kb_text.find("### Outlet: zone.z").expect("outlet zone.z");
    let pos_style = kb_text
        .find("### Style (Post-History)")
        .expect("style slot");
    assert!(
        pos_before < pos_after
            && pos_after < pos_outlet_a
            && pos_outlet_a < pos_outlet_z
            && pos_outlet_z < pos_style,
        "slot emit order must be before → after → outlets (sorted) → style"
    );

    // 2. The fallback block sits between the before and after slots and has
    //    no sub-heading of its own.
    let fallback_segment = &kb_text[pos_before..pos_after];
    assert!(
        fallback_segment.contains("- **LoreMidA**")
            && fallback_segment.contains("- **LoreMidB**")
            && fallback_segment.contains("- **DepthLore**")
            && fallback_segment.contains("- **OddLore**"),
        "default fallback entries must render between the before and after slots"
    );

    // 3. Within-slot order = V1.149 priority-then-order with the constant
    //    band first: LoreMidC (non-constant, priority 100) sorts after the
    //    constant band despite the higher priority; the constant entries
    //    keep priority desc.
    let pos_mid_a = kb_text.find("- **LoreMidA**").expect("LoreMidA");
    let pos_mid_b = kb_text.find("- **LoreMidB**").expect("LoreMidB");
    let pos_depth = kb_text.find("- **DepthLore**").expect("DepthLore");
    let pos_odd = kb_text.find("- **OddLore**").expect("OddLore");
    let pos_mid_c = kb_text.find("- **LoreMidC**").expect("LoreMidC");
    assert!(
        pos_mid_a < pos_mid_b
            && pos_mid_b < pos_depth
            && pos_depth < pos_odd
            && pos_odd < pos_mid_c,
        "fallback order must be constant band (priority desc) then non-constant"
    );

    // 4. Routing table: each hinted entry lands in exactly its slot.
    let before_segment = &kb_text[..pos_after];
    assert!(
        before_segment.contains("- **WorldBefore**")
            && !before_segment.contains("- **WorldAfter**"),
        "before_defs routes to the before slot only"
    );
    let after_segment = &kb_text[pos_after..pos_outlet_a];
    assert!(
        after_segment.contains("- **WorldAfter**") && !after_segment.contains("- **WorldBefore**"),
        "after_defs routes to the after slot only"
    );
    let style_segment = &kb_text[pos_outlet_z..];
    assert!(
        style_segment.contains("- **PostStyle**")
            && !style_segment.contains("### Outlet: style.post_history"),
        "style.post_history routes to the reserved tail slot, not an open outlet"
    );

    // 5. P0 reservation: no Moment Directive section renders.
    assert!(
        !ctx.to_full_context().contains("## Moment Directive"),
        "reserved directive slot must stay empty in P0"
    );

    // 6. All ten fired entries are present exactly once (nothing dropped).
    for name in [
        "WorldBefore",
        "LoreMidA",
        "LoreMidB",
        "WorldAfter",
        "LoreZ",
        "LoreA",
        "PostStyle",
        "DepthLore",
        "OddLore",
        "LoreMidC",
    ] {
        assert_eq!(
            kb_text.matches(name).count(),
            1,
            "entry {name} must appear exactly once in the assembled slots"
        );
    }
}
