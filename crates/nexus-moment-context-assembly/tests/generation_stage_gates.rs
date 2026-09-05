//! V1.150 P2 — generation-stage gate through `assemble_moment` (AC-I4,
//! DF-75 — spec `fl-l-w5-prompt-control-plane.md` §4 / Q4 lock).
//!
//! The spec §4 fill matrix gates the `style.post_history` slot to
//! `produce` + `review` (the one gate AC-I4 requires to be verified), runs
//! **no** lore slots for `system_maintenance`, and keeps every slot on for
//! `unspecified` (direct CLI / inspector path — the neutral AC-I1b anchor).
//! `moment.directive` is NOT stage-gated (spec §4: TTL governs lifetime,
//! not stage).
//!
//! Determinism: every entry carries a distinct `priority`, so the engine's
//! stable sort fully determines the emitted order regardless of
//! `InMemoryKbStore` HashMap iteration order.

#![allow(clippy::manual_string_new, clippy::doc_markdown)]

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;
use nexus_knowledge::world_kb::{InMemoryKbStore, KbStore};
use nexus_knowledge::InMemoryKnowledgeStore;
use nexus_moment_context_assembly::directive::{
    ActiveDirective, DirectiveDepth, DirectiveStore, DirectiveTtlKind,
};
use nexus_moment_context_assembly::stage0::Stage0Assembly;
use nexus_moment_context_assembly::{
    assemble_moment, assemble_moment_with_directive, GenerationStage, MomentRequest,
};
use nexus_narrative::InMemoryNarrativeGateway;

const WORLD_ID: &str = "wld_stage_gates";

/// Build a `KnowledgeEntryRecord` with the given `modules.activation` payload.
fn entry(name: &str, id: &str, activation: &serde_json::Value) -> KnowledgeEntryRecord {
    let mut entry = KnowledgeEntryRecord::new(WORLD_ID, BlockType::Character, name);
    entry.entry_id = id.to_string();
    entry.modules = Some(serde_json::json!({ "activation": activation }));
    entry
}

/// Seed every routing shape: before_defs, after_defs, the reserved
/// `style.post_history` outlet, two open outlets, a no-hint neutral, and a
/// keyword-fired entry (all distinct priorities → deterministic order).
async fn seed_stage_fixture(kb: &InMemoryKbStore) {
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
        (
            "LoreMid",
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

async fn assemble_for(stage: Option<GenerationStage>) -> String {
    let narrative = InMemoryNarrativeGateway::new(InMemoryKbStore::new());
    let kb = InMemoryKbStore::new();
    let knowledge = InMemoryKnowledgeStore::new();
    seed_stage_fixture(&kb).await;

    let stage0 = Stage0Assembly {
        personality: "A king rules the land.".to_string(),
        experience: "10 years.".to_string(),
        user_prompt: "Write chapter 3.".to_string(),
        ..Stage0Assembly::default()
    };
    let mut request = MomentRequest::new(stage0).with_world(WORLD_ID);
    if let Some(stage) = stage {
        request = request.with_generation_stage(stage);
    }
    let ctx = assemble_moment(&request, &narrative, &kb, &knowledge).await;
    ctx.to_full_context()
}

#[tokio::test]
async fn style_post_history_fills_for_produce_and_review() {
    // AC-I4 on-side: `style.post_history` present for `produce` + `review`.
    for stage in [GenerationStage::Produce, GenerationStage::Review] {
        let full = assemble_for(Some(stage)).await;
        assert!(
            full.contains("### Style (Post-History)"),
            "{stage}: style slot must render"
        );
        assert!(
            full.contains("- **PostStyle**"),
            "{stage}: style entry must render"
        );
        // Every other slot still fills.
        for heading in [
            "### World (Before)",
            "### World (After)",
            "### Outlet: aether",
            "### Outlet: zone.z",
        ] {
            assert!(full.contains(heading), "{stage}: {heading} must render");
        }
        assert!(full.contains("- **Hero**"), "{stage}: fallback must render");
    }
}

#[tokio::test]
async fn style_post_history_absent_for_non_produce_review_stages() {
    // AC-I4 off-side: absent for intake / research / persist /
    // work_maintenance — while every other slot keeps filling.
    for stage in [
        GenerationStage::Intake,
        GenerationStage::Research,
        GenerationStage::Persist,
        GenerationStage::WorkMaintenance,
    ] {
        let full = assemble_for(Some(stage)).await;
        assert!(
            !full.contains("### Style (Post-History)"),
            "{stage}: style slot must NOT render"
        );
        assert!(
            !full.contains("- **PostStyle**"),
            "{stage}: style entry must NOT render"
        );
        for heading in [
            "### World (Before)",
            "### World (After)",
            "### Outlet: aether",
            "### Outlet: zone.z",
        ] {
            assert!(full.contains(heading), "{stage}: {heading} must render");
        }
        assert!(full.contains("- **Hero**"), "{stage}: fallback must render");
    }
}

#[tokio::test]
async fn system_maintenance_renders_no_lore_slots_at_all() {
    // `system_maintenance`: the whole matrix row is off — no lore slots, no
    // World-KB section (spec §4, `_system.*` isolation invariant).
    let full = assemble_for(Some(GenerationStage::SystemMaintenance)).await;
    assert!(
        !full.contains("## World Knowledge Base"),
        "system_maintenance must render no World-KB section"
    );
    assert!(!full.contains("- **Hero**"), "no lore entries at all");
    assert!(
        !full.contains("### "),
        "no slot sub-headings at all, got:\n{full}"
    );
}

#[tokio::test]
async fn unspecified_keeps_all_slots_on() {
    // `unspecified` (direct CLI / inspector path — the `None` default):
    // every slot fills (spec §4 row; the neutral byte-equivalence anchor).
    for stage in [None, Some(GenerationStage::Unspecified)] {
        let full = assemble_for(stage).await;
        for marker in [
            "### World (Before)",
            "- **Hero**",
            "### World (After)",
            "### Outlet: aether",
            "### Outlet: zone.z",
            "### Style (Post-History)",
            "- **PostStyle**",
        ] {
            assert!(full.contains(marker), "{stage:?}: {marker} must render");
        }
    }
}

/// In-memory `DirectiveStore` stub: serves a fixed directive and records
/// `after_injection` calls (used to prove the directive injects regardless
/// of stage).
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

// `unused_async_trait_impl` (new in clippy 1.98): the stub performs no async
// I/O; `async` is by `DirectiveStore` trait contract — toolchain-drift debt.
#[allow(clippy::unused_async_trait_impl)]
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

fn stub_directive() -> ActiveDirective {
    ActiveDirective {
        directive_id: "dir_stage".to_string(),
        body: "Keep the prose terse.".to_string(),
        insert_depth: DirectiveDepth::Tail,
        ttl_kind: DirectiveTtlKind::Generations,
        clear_on_scene_change: false,
        ttl_remaining: Some(3),
        status: "active".to_string(),
        scope_kind: "work".to_string(),
        scope_id: "wrk_1".to_string(),
    }
}

#[tokio::test]
async fn directive_injects_regardless_of_stage() {
    // spec §4: `moment.directive` is NOT stage-gated — it injects on every
    // in-scope assemble (TTL governs lifetime, not stage). Verified here
    // across a style-on stage, a style-off stage, and `system_maintenance`
    // (which gates ALL lore slots — the directive is not lore).
    for stage in [
        GenerationStage::Produce,
        GenerationStage::Persist,
        GenerationStage::SystemMaintenance,
    ] {
        let narrative = InMemoryNarrativeGateway::new(InMemoryKbStore::new());
        let kb = InMemoryKbStore::new();
        let knowledge = InMemoryKnowledgeStore::new();
        seed_stage_fixture(&kb).await;
        let store = StubDirectiveStore::with_directive(stub_directive());

        let stage0 = Stage0Assembly {
            personality: "A king rules the land.".to_string(),
            ..Stage0Assembly::default()
        };
        let request = MomentRequest::new(stage0)
            .with_world(WORLD_ID)
            .with_generation_stage(stage);
        let ctx =
            assemble_moment_with_directive(&request, &narrative, &kb, &knowledge, &store).await;

        let full = ctx.to_full_context();
        assert!(
            full.contains("## Moment Directive"),
            "{stage}: directive section must render"
        );
        assert!(
            full.contains("Keep the prose terse."),
            "{stage}: directive body must render"
        );
        assert_eq!(
            store.injection_count(),
            1,
            "{stage}: exactly one lifecycle call per injecting assemble"
        );

        // The stage gate still applies to lore independently of the
        // directive: style off for persist, no lore at all for
        // system_maintenance.
        if stage == GenerationStage::Persist {
            assert!(
                !full.contains("### Style (Post-History)"),
                "persist: lore style gate still applies with a directive active"
            );
        }
        if stage == GenerationStage::SystemMaintenance {
            assert!(
                !full.contains("- **Hero**"),
                "system_maintenance: no lore even with a directive active"
            );
        }
    }
}
