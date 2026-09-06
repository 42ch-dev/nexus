//! Golden regression test — **V1.150 P0 slot routing on a neutral-only
//! World** (DF-75, AC-I1b — the HARD non-regression promise carried from
//! V1.149).
//!
//! A World whose KB entries carry NO `modules.activation` modules AND has no
//! Moment Directive must produce `assemble_moment` output that is
//! **byte-identical to V1.149**:
//!
//! - No `## Moment Directive` section is rendered (the slot is reserved but
//!   empty in P0).
//! - No `### World (Before)` / `### World (After)` / `### Outlet: <name>` /
//!   `### Style (Post-History)` sub-headings render (slots are empty).
//! - Every entry routes to the default fallback, which renders exactly the
//!   V1.149 flat entry block.
//!
//! Proved two ways:
//! 1. In-test: the assembled output contains none of the V1.150 slot
//!    headings and no Moment Directive section.
//! 2. Frozen: the full output is byte-identical to the checked-in golden.

#![allow(clippy::manual_string_new, clippy::doc_markdown)]

use nexus_contracts::{BlockType, TimePolicy, Visibility};
use nexus_knowledge::world_kb::{InMemoryKbStore, KbStore};
use nexus_knowledge::{InMemoryKnowledgeStore, KnowledgeStore, KnowledgeTag};
use nexus_moment_context_assembly::stage0::Stage0Assembly;
use nexus_moment_context_assembly::{assemble_moment, MomentRequest};
use nexus_narrative::timeline_event::TimelineEventType;
use nexus_narrative::InMemoryNarrativeGateway;

/// V1.150 slot sub-headings that must NOT appear in neutral-only output.
const SLOT_HEADINGS: [&str; 4] = [
    "### World (Before)",
    "### World (After)",
    "### Outlet: ",
    "### Style (Post-History)",
];

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
        "wld_golden_slots",
        "ctr_golden",
        "Slots Neutral Golden World",
        "slots-neutral-golden-world",
        Visibility::Private,
        TimePolicy::Manual,
    );
    world.created_at = "2026-01-01T00:00:00Z".to_string();
    narrative.insert_world(world);

    // ── Timeline event (frozen id + created_at) ──
    let mut event = nexus_narrative::timeline_event::TimelineEvent::new(
        "wld_golden_slots",
        "fbk_root",
        TimelineEventType::StoryAdvance,
        1,
    );
    event.timeline_event_id = "evt_slots_neutral_001".to_string();
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

/// Seed a single neutral KB entry (no modules map) + user knowledge.
///
/// NOTE: exactly ONE KB entry — `InMemoryKbStore::query` iterates a `HashMap`
/// without sorting, so multi-entry fixtures would make the byte-exact golden
/// order nondeterministic (same convention as the V1.149 goldens).
async fn seed_neutral_stores(kb: &InMemoryKbStore, knowledge: &InMemoryKnowledgeStore) {
    use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;

    let mut kb_plain = KnowledgeEntryRecord::new("wld_golden_slots", BlockType::Character, "Hero");
    kb_plain.entry_id = "kb_slots_neutral_001".to_string();
    kb_plain.created_at = "2026-01-01T00:00:02Z".to_string();
    kb.insert_knowledge_entry(kb_plain)
        .await
        .expect("insert kb entry");

    let mut uke = nexus_knowledge::UserKnowledgeEntry::new(
        "user_golden",
        vec![KnowledgeTag::new("lore")],
        "The hero wields a legendary sword forged in dragon fire.",
    );
    uke.id = "kno_slots_neutral_001".to_string();
    uke.created_at = "2026-01-01T00:00:04Z".to_string();
    uke.updated_at = "2026-01-01T00:00:04Z".to_string();
    knowledge.store(uke).await.expect("store knowledge");
}

/// Assemble with default activation (ON) and return the full context bytes.
async fn assemble_default_on(
    stage0: Stage0Assembly,
    narrative: &InMemoryNarrativeGateway<InMemoryKbStore>,
    kb: &InMemoryKbStore,
    knowledge: &InMemoryKnowledgeStore,
) -> String {
    let request = MomentRequest::new(stage0)
        .with_world("wld_golden_slots")
        .with_user("user_golden");
    assemble_moment(&request, narrative, kb, knowledge)
        .await
        .to_full_context()
}

#[tokio::test]
async fn dump_slots_neutral_only_golden_for_regeneration() {
    // Temporary helper: prints the neutral-only output so the golden file can
    // be regenerated (documented regen path, same pattern as the V1.149
    // goldens).
    let (narrative, kb, knowledge, stage0) = build_neutral_fixture();
    seed_neutral_stores(&kb, &knowledge).await;

    let output = assemble_default_on(stage0, &narrative, &kb, &knowledge).await;
    println!("===GOLDEN-BEGIN===\n{output}\n===GOLDEN-END===");
}

#[tokio::test]
async fn slots_neutral_only_byte_equivalent_golden() {
    let (narrative, kb, knowledge, stage0) = build_neutral_fixture();
    seed_neutral_stores(&kb, &knowledge).await;

    let output = assemble_default_on(stage0, &narrative, &kb, &knowledge).await;

    // 1. P0 reservation: no Moment Directive section renders when no
    //    directive is active.
    assert!(
        !output.contains("## Moment Directive"),
        "reserved directive slot must not render in P0"
    );

    // 2. No V1.150 slot sub-headings render when all slots are empty.
    for heading in SLOT_HEADINGS {
        assert!(
            !output.contains(heading),
            "empty slot heading {heading:?} must not render in neutral-only output"
        );
    }

    // 3. The World-KB section is byte-identical to the V1.149 flat block.
    let world_kb = output
        .split("## World Knowledge Base")
        .nth(1)
        .expect("world KB section present")
        .split("\n\n## ")
        .next()
        .expect("section ends at the next heading")
        .trim();
    assert_eq!(
        world_kb, "- **Hero** [Character]: (no summary)",
        "neutral-only World-KB body must be the V1.149 flat block"
    );

    // 4. Frozen guarantee: full output matches the checked-in golden.
    let golden = include_str!("fixtures/assemble_moment_slots_neutral_only.golden");
    assert_eq!(
        output, golden,
        "Golden mismatch: neutral-only World under V1.150 slot routing output has changed.\n\
         If the change is intentional (e.g. a deliberate format update), re-generate the golden:\n\
         Run the dump test and capture the actual output to replace the golden file."
    );
}
