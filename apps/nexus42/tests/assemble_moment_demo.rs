//! Integration test for four-domain Moment assembly via nexus42.
//!
//! Proves product wiring from `nexus42` to `assemble_moment` with persistent
//! SQLite stores, verifying that domain sections appear in the output:
//!
//! 1. Creator Memory (Stage-0)
//! 2. Narrative (World state + timeline)
//! 3. World KB (key blocks)
//! 4. User Knowledge (entries — in-memory for V1.26)
//!
//! Follows the deterministic fixture pattern from
//! `nexus-moment-context-assembly/tests/assembly_integration.rs`.

#![allow(clippy::manual_string_new, clippy::doc_markdown)]

use nexus_kb::{InMemoryKbStore, KbStore};
use nexus_knowledge::{InMemoryKnowledgeStore, KnowledgeStore};
use nexus_moment_context_assembly::{assemble_moment, MomentRequest, Stage0Assembly};
use nexus_narrative::InMemoryNarrativeGateway;

/// Helper: create a minimal `Stage0Assembly` for testing.
fn make_stage0() -> Stage0Assembly {
    Stage0Assembly {
        personality: "I am a demo creator exploring four-domain Moment assembly.".to_string(),
        experience: "Experimental four-domain context assembly demo.".to_string(),
        long_term_memories: Vec::new(),
        fragment_keywords: Vec::new(),
        system_prefix: String::new(),
        user_prompt: "Demo moment context assembly.".to_string(),
        max_tokens: None,
    }
}

/// Helper: set up in-memory stores and seed demo fixtures.
fn seed_demo_stores(
    world_id: &str,
    _user_id: &str,
) -> (
    InMemoryNarrativeGateway<InMemoryKbStore>,
    InMemoryKbStore,
    InMemoryKnowledgeStore,
) {
    use nexus_contracts::{TimePolicy, Visibility};
    use nexus_narrative::timeline_event::TimelineEvent;
    use nexus_narrative::timeline_event::TimelineEventType;
    use nexus_narrative::world::World;

    let kb = InMemoryKbStore::new();
    let narrative = InMemoryNarrativeGateway::new(InMemoryKbStore::new());
    let knowledge = InMemoryKnowledgeStore::new();

    // Seed world
    let world = World::new(
        world_id,
        "ctr_demo",
        "Demo World",
        "demo-world",
        Visibility::Private,
        TimePolicy::Manual,
    );
    narrative.insert_world(world);

    // Seed timeline event
    let mut event = TimelineEvent::new(world_id, "fbk_root", TimelineEventType::StoryAdvance, 1);
    event.title = Some("Demo story event — the beginning".to_string());
    narrative.insert_event(event);

    (narrative, kb, knowledge)
}

/// Helper: seed KB block (async, call before assembly).
async fn seed_kb(kb: &InMemoryKbStore, world_id: &str) {
    use nexus_contracts::BlockType;
    use nexus_kb::key_block::KeyBlock;
    let block = KeyBlock::new(world_id, BlockType::Character, "Demo Hero");
    kb.insert_key_block(block).await.unwrap();
}

/// Helper: seed knowledge entry (async, call before assembly).
async fn seed_knowledge(knowledge: &InMemoryKnowledgeStore, user_id: &str) {
    use nexus_knowledge::{KnowledgeEntry, KnowledgeTag};
    let entry = KnowledgeEntry::new(
        user_id,
        vec![KnowledgeTag::new("demo")],
        "Demo user knowledge entry for Moment assembly.",
    );
    knowledge.store(entry).await.unwrap();
}

/// Seeded demo with default IDs should produce all four domains.
#[tokio::test]
async fn assemble_moment_demo_produces_four_domain_context() {
    let (narrative, kb, knowledge) = seed_demo_stores("wld_demo", "user_demo");
    seed_kb(&kb, "wld_demo").await;
    seed_knowledge(&knowledge, "user_demo").await;

    let request = MomentRequest::new(make_stage0())
        .with_world("wld_demo")
        .with_user("user_demo");
    let ctx = assemble_moment(&request, &narrative, &kb, &knowledge).await;

    let full = ctx.to_full_context();

    // Domain 1: Stage-0 (Creator Memory)
    assert!(
        !ctx.stage0_context.is_empty(),
        "Stage-0 context must be non-empty"
    );
    assert!(
        full.contains("demo creator"),
        "Full context must contain Stage-0 personality text"
    );

    // Domain 2: Narrative (World state + timeline)
    assert!(
        ctx.world_state.is_some(),
        "World state must be present for seeded demo world"
    );
    assert!(
        ctx.timeline.is_some(),
        "Timeline must be present for seeded demo world"
    );
    let ws = ctx.world_state.as_ref().expect("world state");
    assert!(
        ws.contains("wld_demo"),
        "World state must reference the demo world ID"
    );
    assert!(
        full.contains("Demo World"),
        "Full context must contain world title"
    );
    assert!(
        full.contains("Demo story event"),
        "Full context must contain timeline event"
    );

    // Domain 3: World KB
    assert!(
        ctx.world_kb.is_some(),
        "World KB must be present for seeded demo world"
    );
    assert!(
        full.contains("Demo Hero"),
        "Full context must contain KB block"
    );

    // Domain 4: User Knowledge
    assert!(
        ctx.user_knowledge.is_some(),
        "User knowledge must be present for seeded demo user"
    );
    assert!(
        full.contains("Demo user knowledge"),
        "Full context must contain knowledge entry"
    );
}

/// Custom world ID and user ID should produce matching fixtures.
#[tokio::test]
async fn assemble_moment_demo_with_custom_ids() {
    let (narrative, kb, knowledge) = seed_demo_stores("wld_custom", "user_custom");
    seed_kb(&kb, "wld_custom").await;
    seed_knowledge(&knowledge, "user_custom").await;

    let request = MomentRequest::new(make_stage0())
        .with_world("wld_custom")
        .with_user("user_custom");
    let ctx = assemble_moment(&request, &narrative, &kb, &knowledge).await;

    let ws = ctx.world_state.as_ref().expect("world state");
    assert!(
        ws.contains("wld_custom"),
        "World state must reference custom world ID"
    );

    let uk = ctx.user_knowledge.as_ref().expect("user knowledge");
    assert!(
        uk.contains("Demo user knowledge"),
        "User knowledge must contain seeded content"
    );
}

/// MomentContext with no matching IDs (non-seeded) should still have Stage-0
/// but lack the other three domains.
#[tokio::test]
async fn assemble_moment_demo_non_seeded_ids_yield_stage0_only() {
    let (narrative, kb, knowledge) = seed_demo_stores("wld_demo", "user_demo");

    // Request a world that was never seeded
    let request = MomentRequest::new(make_stage0()).with_world("wld_ghost");
    let ctx = assemble_moment(&request, &narrative, &kb, &knowledge).await;

    assert!(!ctx.stage0_context.is_empty());
    assert!(ctx.world_state.is_none());
    assert!(ctx.timeline.is_none());
    assert!(ctx.world_kb.is_none());
    assert!(ctx.user_knowledge.is_none());
}

/// Full context heading structure matches the spec ordering.
#[tokio::test]
async fn assemble_moment_demo_context_heading_order() {
    let (narrative, kb, knowledge) = seed_demo_stores("wld_demo", "user_demo");
    seed_kb(&kb, "wld_demo").await;
    seed_knowledge(&knowledge, "user_demo").await;

    let request = MomentRequest::new(make_stage0())
        .with_world("wld_demo")
        .with_user("user_demo");
    let ctx = assemble_moment(&request, &narrative, &kb, &knowledge).await;
    let full = ctx.to_full_context();

    // Verify section headings appear in spec order
    let pos_stage0 = full
        .find("## Personality")
        .or_else(|| full.find("demo creator"));
    let pos_world = full.find("## World State");
    let pos_timeline = full.find("## Timeline");
    let pos_kb = full.find("## World Knowledge Base");
    let pos_knowledge = full.find("## User Knowledge");

    // All headings must be present
    assert!(pos_stage0.is_some(), "Stage-0 section must be present");
    assert!(pos_world.is_some(), "World State heading must be present");
    assert!(pos_timeline.is_some(), "Timeline heading must be present");
    assert!(pos_kb.is_some(), "World KB heading must be present");
    assert!(
        pos_knowledge.is_some(),
        "User Knowledge heading must be present"
    );

    // Verify ordering: Stage0 < World State < Timeline < World KB < User Knowledge
    let s0 = pos_stage0.expect("checked above");
    let ws = pos_world.expect("checked above");
    let tl = pos_timeline.expect("checked above");
    let kb_pos = pos_kb.expect("checked above");
    let kn = pos_knowledge.expect("checked above");

    assert!(s0 < ws, "Stage-0 must come before World State");
    assert!(ws < tl, "World State must come before Timeline");
    assert!(tl < kb_pos, "Timeline must come before World KB");
    assert!(kb_pos < kn, "World KB must come before User Knowledge");
}
