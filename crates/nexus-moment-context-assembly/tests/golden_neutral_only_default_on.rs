//! Golden regression test — **neutral-only** World under **default-on** lore
//! activation (the HARD ship guarantee).
//!
//! V1.149 P0 T2 (spec §1 neutral-only): a World whose KB contains NO
//! `modules.activation` modules must produce `assemble_moment` output that is
//! byte-identical to V1.146 flag-off. No entries are silently filtered,
//! reordered, or dropped; no new tokens are consumed.
//!
//! The test proves this two ways:
//! 1. In-test: default-on output == explicit-off output for the same fixture.
//! 2. Frozen: default-on output is byte-identical to the checked-in golden.

#![allow(clippy::manual_string_new, clippy::doc_markdown)]

use nexus_contracts::{BlockType, TimePolicy, Visibility};
use nexus_knowledge::world_kb::{InMemoryKbStore, KbStore};
use nexus_knowledge::{InMemoryKnowledgeStore, KnowledgeStore, KnowledgeTag};
use nexus_moment_context_assembly::stage0::Stage0Assembly;
use nexus_moment_context_assembly::{assemble_moment, MomentRequest};
use nexus_narrative::timeline_event::TimelineEventType;
use nexus_narrative::InMemoryNarrativeGateway;

/// Deterministic neutral-only fixture: no entry carries `modules.activation`.
/// All wall-clock / uuid sources are frozen to fixed values.
fn build_neutral_fixture() -> (
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
        "wld_golden_neutral",
        "ctr_golden",
        "Neutral Golden World",
        "neutral-golden-world",
        Visibility::Private,
        TimePolicy::Manual,
    );
    world.created_at = "2026-01-01T00:00:00Z".to_string();
    narrative.insert_world(world);

    // ── Timeline event (frozen id + created_at) ──
    let mut event = nexus_narrative::timeline_event::TimelineEvent::new(
        "wld_golden_neutral",
        "fbk_root",
        TimelineEventType::StoryAdvance,
        1,
    );
    event.timeline_event_id = "evt_neutral_001".to_string();
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

/// Seed KB entries (all neutral: no activation module) + user knowledge.
///
/// NOTE: exactly ONE KB entry — `InMemoryKbStore::query` iterates a `HashMap`
/// without sorting, so multi-entry fixtures would make the byte-exact golden
/// order nondeterministic (the V1.146 flag-off golden has the same shape).
/// Neutral-variant coverage lives in unit tests
/// (`activation_flag_on_no_activation_module_includes_all` etc.).
async fn seed_neutral_stores(kb: &InMemoryKbStore, knowledge: &InMemoryKnowledgeStore) {
    use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;

    // Entry with NO modules map at all.
    let mut kb_plain =
        KnowledgeEntryRecord::new("wld_golden_neutral", BlockType::Character, "Hero");
    kb_plain.entry_id = "kb_neutral_001".to_string();
    kb_plain.created_at = "2026-01-01T00:00:02Z".to_string();
    kb.insert_knowledge_entry(kb_plain)
        .await
        .expect("insert kb entry");

    // User knowledge entry (frozen id + timestamps)
    let mut uke = nexus_knowledge::UserKnowledgeEntry::new(
        "user_golden",
        vec![KnowledgeTag::new("lore")],
        "The hero wields a legendary sword forged in dragon fire.",
    );
    uke.id = "kno_neutral_001".to_string();
    uke.created_at = "2026-01-01T00:00:04Z".to_string();
    uke.updated_at = "2026-01-01T00:00:04Z".to_string();
    knowledge.store(uke).await.expect("store knowledge");
}

/// Assemble with the given activation flag and return the full context bytes.
async fn assemble_with_flag(
    stage0: Stage0Assembly,
    narrative: &InMemoryNarrativeGateway<InMemoryKbStore>,
    kb: &InMemoryKbStore,
    knowledge: &InMemoryKnowledgeStore,
    activation_enabled: bool,
) -> String {
    let request = MomentRequest::new(stage0)
        .with_world("wld_golden_neutral")
        .with_user("user_golden")
        .with_activation_enabled(activation_enabled);
    assemble_moment(&request, narrative, kb, knowledge)
        .await
        .to_full_context()
}

#[tokio::test]
async fn dump_neutral_only_golden_for_regeneration() {
    // Temporary helper: prints the default-on output so the golden file can be
    // regenerated. Kept on purpose (documented regen path, same pattern as
    // other goldens in this crate).
    let (narrative, kb, knowledge, stage0) = build_neutral_fixture();
    seed_neutral_stores(&kb, &knowledge).await;

    let output = assemble_with_flag(stage0, &narrative, &kb, &knowledge, true).await;
    println!("===GOLDEN-BEGIN===\n{output}\n===GOLDEN-END===");
}

#[tokio::test]
async fn neutral_only_default_on_golden() {
    let (narrative, kb, knowledge, stage0) = build_neutral_fixture();
    seed_neutral_stores(&kb, &knowledge).await;

    // Hard guarantee: default-on == explicit-off (V1.146 flag-off semantics).
    let default_on = assemble_with_flag(stage0.clone(), &narrative, &kb, &knowledge, true).await;
    let explicit_off = assemble_with_flag(stage0, &narrative, &kb, &knowledge, false).await;
    assert_eq!(
        default_on, explicit_off,
        "neutral-only World: default-on activation output must be byte-identical \
         to explicit-off (V1.146 flag-off) output"
    );

    // Frozen guarantee: default-on output matches the checked-in golden.
    let golden = include_str!("fixtures/assemble_moment_neutral_only_default_on.golden");
    assert_eq!(
        default_on, golden,
        "Golden mismatch: neutral-only World under default-on activation output has changed.\n\
         If the change is intentional (e.g. a deliberate format update), re-generate the golden:\n\
         Run the test with RUST_LOG=info and capture the actual output to replace the golden file."
    );
}
