//! Golden regression test — `assemble_moment` with `activation_enabled: false`.
//!
//! V1.146 P4 T4: HARD gate. The test captures a deterministic fixture output
//! and verifies it is byte-identical to a checked-in golden. Any drift in the
//! OFF path (e.g. unintentional activation gate leaking into the OFF branch)
//! causes this test to fail.

#![allow(clippy::manual_string_new, clippy::doc_markdown)]

use nexus_contracts::{BlockType, TimePolicy, Visibility};
use nexus_knowledge::world_kb::{InMemoryKbStore, KbStore};
use nexus_knowledge::{InMemoryKnowledgeStore, KnowledgeStore, KnowledgeTag};
use nexus_moment_context_assembly::stage0::Stage0Assembly;
use nexus_moment_context_assembly::{assemble_moment, MomentRequest};
use nexus_narrative::timeline_event::TimelineEventType;
use nexus_narrative::InMemoryNarrativeGateway;

/// Deterministic fixture: all wall-clock / uuid sources are frozen to fixed
/// values so the output is reproducible across runs.
fn build_deterministic_fixture() -> (
    InMemoryNarrativeGateway<InMemoryKbStore>,
    InMemoryKbStore,
    InMemoryKnowledgeStore,
    Stage0Assembly,
) {
    let narrative = InMemoryNarrativeGateway::new(InMemoryKbStore::new());
    let kb = InMemoryKbStore::new();
    let knowledge = InMemoryKnowledgeStore::new();

    // ── World (frozen created_at) ──
    let mut world = nexus_narrative::world::World::new(
        "wld_golden",
        "ctr_golden",
        "Golden World",
        "golden-world",
        Visibility::Private,
        TimePolicy::Manual,
    );
    world.created_at = "2026-01-01T00:00:00Z".to_string();
    narrative.insert_world(world);

    // ── Timeline event (frozen id + created_at) ──
    let mut event = nexus_narrative::timeline_event::TimelineEvent::new(
        "wld_golden",
        "fbk_root",
        TimelineEventType::StoryAdvance,
        1,
    );
    event.timeline_event_id = "evt_golden_001".to_string();
    event.created_at = "2026-01-01T00:00:01Z".to_string();
    event.title = Some("The Beginning".to_string());
    narrative.insert_event(event);

    // ── Stage0 (no wall-clock dep) ──
    let stage0 = Stage0Assembly {
        personality: "A creative writer who loves worldbuilding.".to_string(),
        experience: "Published 3 novels and 20 short stories.".to_string(),
        system_prefix: "You are an AI co-writer for a fantasy novel.".to_string(),
        user_prompt: "Continue chapter 5 where the dragon appears.".to_string(),
        ..Stage0Assembly::default()
    };

    (narrative, kb, knowledge, stage0)
}

/// Seed KB entry and user knowledge into the in-memory stores (async).
async fn seed_stores(
    kb: &InMemoryKbStore,
    knowledge: &InMemoryKnowledgeStore,
) {
    // KB entry with activation module (tests that flag OFF still includes it)
    use nexus_knowledge::world_kb::knowledge_entry::WorldKbEntry;
    let mut kb_entry = WorldKbEntry::new("wld_golden", BlockType::Character, "Hero");
    kb_entry.entry_id = "kb_golden_001".to_string();
    kb_entry.created_at = "2026-01-01T00:00:02Z".to_string();
    kb_entry.modules = Some(serde_json::json!({
        "activation": {"key": ["dragon"], "logic": "and_any"}
    }));
    kb.insert_knowledge_entry(kb_entry).await.expect("insert kb entry");

    // User knowledge entry (frozen id + timestamps)
    let mut uke = nexus_knowledge::UserKnowledgeEntry::new(
        "user_golden",
        vec![KnowledgeTag::new("lore")],
        "The hero wields a legendary sword forged in dragon fire.",
    );
    uke.id = "kno_golden_001".to_string();
    uke.created_at = "2026-01-01T00:00:03Z".to_string();
    uke.updated_at = "2026-01-01T00:00:03Z".to_string();
    knowledge.store(uke).await.expect("store knowledge");
}

#[tokio::test]
async fn assemble_moment_flag_off_golden() {
    let (narrative, kb, knowledge, stage0) = build_deterministic_fixture();
    seed_stores(&kb, &knowledge).await;

    // activation_enabled: false (default) — no activation filtering
    let request = MomentRequest::new(stage0)
        .with_world("wld_golden")
        .with_user("user_golden");

    let ctx = assemble_moment(&request, &narrative, &kb, &knowledge).await;
    let output = ctx.to_full_context();

    // Read golden from checked-in file
    let golden = include_str!("fixtures/assemble_moment_flag_off.golden");

    assert_eq!(
        output, golden,
        "Golden mismatch: assemble_moment with activation_enabled=false output has changed.\n\
         If the change is intentional (e.g. a deliberate format update), re-generate the golden:\n\
         Run the test with RUST_LOG=info and capture the actual output to replace the golden file."
    );
}
