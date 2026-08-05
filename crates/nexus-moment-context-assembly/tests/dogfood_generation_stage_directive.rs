//! V1.150 P2 dogfood — generation-stage slot gating + Moment Directive
//! exercised together through `assemble_moment_with_directive` on a seeded
//! World (DF-75 — spec `fl-l-w5-prompt-control-plane.md` §4).
//!
//! The unit suites prove the gate matrix (slots.rs) and the directive
//! lifecycle (P1) in isolation. This file is the **dogfood layer**: a
//! Mira-at-Harbor-style World with every routing shape (`before_defs`,
//! `after_defs`, the reserved `style.post_history` outlet, two open
//! `kb.outlet.<name>` slots, a no-hint neutral entry, and a keyword-fired
//! entry) is assembled across `produce` / `review` / `persist` /
//! `system_maintenance` with an active Moment Directive, asserting:
//!
//! - slot placement + emit order per spec §2 / Q5
//! - the `style.post_history` gate (AC-I4): on for `produce`/`review`, off
//!   for `persist`, no lore at all for `system_maintenance`
//! - the directive injects **regardless of stage** (spec §4 — TTL governs
//!   lifetime, not stage), never inside the World-KB section (AC-I3)
//!
//! The directive lifecycle (set → inject N times → expire; scene-change
//! clear; Work-wins-over-World scope resolution) is exercised at the CLI
//! layer against the real `LocalDirectiveStore` in
//! `apps/nexus42/src/commands/creator/moment_directive.rs` — see the P2
//! plan closeout notes for the evidence.

#![allow(clippy::manual_string_new, clippy::doc_markdown)]

use nexus_contracts::{BlockType, TimePolicy, Visibility};
use nexus_knowledge::world_kb::knowledge_entry::WorldKbEntry;
use nexus_knowledge::world_kb::{InMemoryKbStore, KbStore};
use nexus_knowledge::InMemoryKnowledgeStore;
use nexus_moment_context_assembly::directive::{
    ActiveDirective, DirectiveDepth, DirectiveStore, DirectiveTtlKind,
};
use nexus_moment_context_assembly::stage0::Stage0Assembly;
use nexus_moment_context_assembly::{
    assemble_moment_with_directive, GenerationStage, MomentContext, MomentRequest,
};
use nexus_narrative::timeline_event::TimelineEventType;
use nexus_narrative::InMemoryNarrativeGateway;

const WORLD_ID: &str = "wld_dogfood_stage";

/// Build the dogfood World: world + timeline beat + Stage-0 whose scan text
/// fires the keyword entry (`king`).
fn build_world() -> (
    InMemoryNarrativeGateway<InMemoryKbStore>,
    InMemoryKbStore,
    Stage0Assembly,
) {
    let narrative = InMemoryNarrativeGateway::new(InMemoryKbStore::new());
    let kb = InMemoryKbStore::new();

    let mut world = nexus_narrative::world::World::new(
        WORLD_ID,
        "ctr_dogfood",
        "Stage Gate Harbor",
        "stage-gate-harbor",
        Visibility::Private,
        TimePolicy::Manual,
    );
    world.created_at = "2026-01-01T00:00:00Z".to_string();
    narrative.insert_world(world);

    let mut event = nexus_narrative::timeline_event::TimelineEvent::new(
        WORLD_ID,
        "fbk_root",
        TimelineEventType::StoryAdvance,
        1,
    );
    event.timeline_event_id = "evt_stage_001".to_string();
    event.created_at = "2026-01-01T00:00:01Z".to_string();
    event.title = Some("The king walks the quay at dawn.".to_string());
    narrative.insert_event(event);

    let stage0 = Stage0Assembly {
        personality: "A creative writer who loves worldbuilding.".to_string(),
        experience: "Published 3 novels about seaside towns.".to_string(),
        system_prefix: "You are an AI co-writer for a fantasy novel.".to_string(),
        user_prompt: "The king arrives at the harbor quay.".to_string(),
        ..Stage0Assembly::default()
    };

    (narrative, kb, stage0)
}

/// Build a `WorldKbEntry` with the given `modules.activation` payload.
fn entry(name: &str, id: &str, activation: &serde_json::Value) -> WorldKbEntry {
    let mut entry = WorldKbEntry::new(WORLD_ID, BlockType::Character, name);
    entry.entry_id = id.to_string();
    entry.modules = Some(serde_json::json!({ "activation": activation }));
    entry
}

/// Seed every routing shape (all constant seeds with distinct priorities →
/// deterministic emit order, spec §2 / Q5):
///
/// `### World (Before)` → fallback (Hero + keyword-fired entry) →
/// `### World (After)` → `### Outlet: aether` → `### Outlet: zone.z` →
/// `### Style (Post-History)` (tail).
async fn seed_routing_shapes(kb: &InMemoryKbStore) {
    let seeds: &[(&str, &str, serde_json::Value)] = &[
        (
            "WorldBefore",
            "kb_bf",
            serde_json::json!({"constant": true, "priority": 90, "position_hint": "before_defs"}),
        ),
        (
            "Hero",
            "kb_fb",
            serde_json::json!({"constant": true, "priority": 80}),
        ),
        (
            "WorldAfter",
            "kb_af",
            serde_json::json!({"constant": true, "priority": 70, "position_hint": "after_defs"}),
        ),
        (
            "LoreZ",
            "kb_z",
            serde_json::json!({"constant": true, "priority": 60,
                "position_hint": "outlet", "outlet": "zone.z"}),
        ),
        (
            "LoreA",
            "kb_a",
            serde_json::json!({"constant": true, "priority": 50,
                "position_hint": "outlet", "outlet": "aether"}),
        ),
        (
            "PostStyle",
            "kb_ph",
            serde_json::json!({"constant": true, "priority": 40,
                "position_hint": "outlet", "outlet": "style.post_history"}),
        ),
        // Keyword-fired (non-constant) — sorts after the constant band.
        (
            "Court Scribe",
            "kb_mid",
            serde_json::json!({"keys": ["king"], "priority": 30}),
        ),
    ];
    for (name, id, activation) in seeds {
        kb.insert_knowledge_entry(entry(name, id, activation))
            .await
            .expect("insert kb entry");
    }
}

/// In-memory `DirectiveStore` stub serving one fixed active directive and
/// counting injections.
#[derive(Default)]
struct StubDirectiveStore {
    active: Option<ActiveDirective>,
    calls: std::sync::Arc<std::sync::Mutex<usize>>,
}

impl StubDirectiveStore {
    fn with_directive(active: ActiveDirective) -> Self {
        Self {
            active: Some(active),
            ..Self::default()
        }
    }

    fn injection_count(&self) -> usize {
        *self.calls.lock().unwrap()
    }
}

impl DirectiveStore for StubDirectiveStore {
    async fn load_active(
        &self,
        _creator_id: Option<&str>,
        _work_id: Option<&str>,
        _world_id: Option<&str>,
    ) -> Option<ActiveDirective> {
        self.active.clone()
    }

    async fn after_injection(
        &self,
        _directive_id: &str,
        _event_id: Option<&str>,
        _work_id: Option<&str>,
    ) {
        *self.calls.lock().unwrap() += 1;
    }
}

fn directive() -> ActiveDirective {
    ActiveDirective {
        directive_id: "dir_dogfood".to_string(),
        body: "Keep the prose terse and harbor-voiced.".to_string(),
        insert_depth: DirectiveDepth::Tail,
        ttl_kind: DirectiveTtlKind::Generations,
        clear_on_scene_change: false,
        ttl_remaining: Some(3),
        status: "active".to_string(),
        scope_kind: "work".to_string(),
        scope_id: "wrk_1".to_string(),
    }
}

async fn assemble(
    stage: GenerationStage,
) -> (
    MomentContext,
    StubDirectiveStore,
    InMemoryNarrativeGateway<InMemoryKbStore>,
) {
    let (narrative, kb, stage0) = build_world();
    seed_routing_shapes(&kb).await;
    let knowledge = InMemoryKnowledgeStore::new();
    let store = StubDirectiveStore::with_directive(directive());

    let request = MomentRequest::new(stage0)
        .with_world(WORLD_ID)
        .with_generation_stage(stage);
    let ctx = assemble_moment_with_directive(&request, &narrative, &kb, &knowledge, &store).await;
    (ctx, store, narrative)
}

#[tokio::test]
async fn dogfood_produce_places_all_slots_and_directive() {
    // `produce`: every slot fills (style on — AC-I4 on-side) and the
    // directive injects between Timeline and World KB (above lore, below
    // system — Q1 lock).
    let (ctx, store, _narrative) = assemble(GenerationStage::Produce).await;
    let full = ctx.to_full_context();

    let pos_timeline = full.find("## Timeline").expect("timeline");
    let pos_directive = full.find("## Moment Directive").expect("directive");
    let pos_kb = full.find("## World Knowledge Base").expect("world kb");
    assert!(
        pos_timeline < pos_directive && pos_directive < pos_kb,
        "directive must sit between Timeline and World KB"
    );

    let kb_text = ctx.world_kb.as_deref().expect("world_kb present");
    let pos_before = kb_text.find("### World (Before)").expect("before");
    let pos_after = kb_text.find("### World (After)").expect("after");
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
    for name in [
        "WorldBefore",
        "Hero",
        "WorldAfter",
        "LoreA",
        "LoreZ",
        "PostStyle",
        "Court Scribe",
    ] {
        assert_eq!(
            kb_text.matches(&format!("- **{name}**")).count(),
            1,
            "entry {name} must appear exactly once"
        );
    }
    assert_eq!(store.injection_count(), 1, "one directive injection");
}

#[tokio::test]
async fn dogfood_review_keeps_style_slot() {
    // `review`: style still on (AC-I4 on-side) + directive injects.
    let (ctx, store, _narrative) = assemble(GenerationStage::Review).await;
    let kb_text = ctx.world_kb.as_deref().expect("world_kb present");
    assert!(
        kb_text.contains("### Style (Post-History)"),
        "review: style slot on"
    );
    assert!(kb_text.contains("- **PostStyle**"));
    assert!(
        ctx.to_full_context().contains("## Moment Directive"),
        "review: directive injects"
    );
    assert_eq!(store.injection_count(), 1);
}

#[tokio::test]
async fn dogfood_persist_gates_style_keeps_everything_else() {
    // `persist` (non-style stage): `style.post_history` off — its entry is
    // excluded, every other slot stays, directive still injects.
    let (ctx, store, _narrative) = assemble(GenerationStage::Persist).await;
    let kb_text = ctx.world_kb.as_deref().expect("world_kb present");
    assert!(
        !kb_text.contains("### Style (Post-History)"),
        "persist: style slot off"
    );
    assert!(
        !kb_text.contains("- **PostStyle**"),
        "persist: style entry gone"
    );
    for name in [
        "WorldBefore",
        "Hero",
        "WorldAfter",
        "LoreA",
        "LoreZ",
        "Court Scribe",
    ] {
        assert_eq!(
            kb_text.matches(&format!("- **{name}**")).count(),
            1,
            "persist: {name} must stay"
        );
    }
    assert!(
        ctx.to_full_context().contains("## Moment Directive"),
        "persist: directive still injects (not stage-gated)"
    );
    assert_eq!(store.injection_count(), 1);
}

#[tokio::test]
async fn dogfood_system_maintenance_has_no_lore_but_directive_lives() {
    // `system_maintenance`: no lore slots at all (`_system.*` isolation) —
    // but the directive is not lore and still injects (spec §4).
    let (ctx, store, _narrative) = assemble(GenerationStage::SystemMaintenance).await;
    assert!(
        ctx.world_kb.is_none(),
        "system_maintenance: no World-KB section"
    );
    let full = ctx.to_full_context();
    assert!(
        !full.contains("## World Knowledge Base"),
        "system_maintenance: no lore section"
    );
    assert!(
        full.contains("## Moment Directive"),
        "system_maintenance: directive still injects"
    );
    assert_eq!(store.injection_count(), 1);
}

#[tokio::test]
async fn dogfood_directive_never_enters_world_kb_section() {
    // AC-I3 at the dogfood layer: the directive body appears exactly once —
    // in its own `## Moment Directive` section — and never inside the
    // World-KB block (it is not lore, never a `modules.*` object).
    let (ctx, _store, _narrative) = assemble(GenerationStage::Produce).await;
    let full = ctx.to_full_context();
    assert_eq!(
        full.matches("Keep the prose terse and harbor-voiced.")
            .count(),
        1,
        "directive body must appear exactly once"
    );
    let kb_text = ctx.world_kb.expect("world_kb present");
    assert!(
        !kb_text.contains("Keep the prose terse"),
        "directive body must never appear inside the World-KB section"
    );
}
