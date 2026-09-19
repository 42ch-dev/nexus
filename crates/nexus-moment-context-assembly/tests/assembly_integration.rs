//! Integration tests for moment context assembly with all domain stores.
//!
//! These tests verify that `assemble_moment` correctly aggregates context from
//! all four domain sources:
//!
//! 1. Creator Memory (SOUL sections, long-term memories, fragment keywords)
//! 2. Narrative (World/Timeline/Event state)
//! 3. World KB (key blocks)
//! 4. User Knowledge (entries)
//!
//! All tests use in-memory implementations — no I/O or network required.

#![allow(clippy::manual_string_new, clippy::doc_markdown)]

use nexus_contracts::{BlockType, TimePolicy, Visibility};
use nexus_knowledge::world_kb::{InMemoryKbStore, KbStore};
use nexus_knowledge::{InMemoryKnowledgeStore, KnowledgeStore};
use nexus_moment_context_assembly::stage0::Stage0Assembly;
use nexus_moment_context_assembly::{assemble_moment, MomentContext, MomentRequest};
use nexus_narrative::InMemoryNarrativeGateway;

// ── Helpers ─────────────────────────────────────────────────────────

struct FixtureStores {
    narrative: InMemoryNarrativeGateway<InMemoryKbStore>,
    kb: InMemoryKbStore,
    knowledge: InMemoryKnowledgeStore,
}

impl FixtureStores {
    fn empty() -> Self {
        Self {
            narrative: InMemoryNarrativeGateway::new(InMemoryKbStore::new()),
            kb: InMemoryKbStore::new(),
            knowledge: InMemoryKnowledgeStore::new(),
        }
    }
}

fn stage0_with_soul_sections() -> Stage0Assembly {
    let mut ltm = nexus_creator_memory::LongTermMemory::new("story_summary");
    ltm.set_body("The hero's journey begins with a call to adventure.");
    Stage0Assembly {
        personality: "I am a speculative fiction writer who loves worldbuilding.".to_string(),
        experience: "Published 3 novels, 20 short stories.".to_string(),
        long_term_memories: vec![ltm],
        fragment_keywords: vec!["magic system".to_string(), "dragons".to_string()],
        system_prefix: "You are an AI co-writer.".to_string(),
        user_prompt: "Continue chapter 5 where the dragon appears.".to_string(),
        max_tokens: None,
    }
}

/// Seed a world into the narrative gateway.
fn seed_world(stores: &FixtureStores, world_id: &str, title: &str) {
    let world = nexus_narrative::world::World::new(
        world_id,
        "ctr_test",
        title,
        &title.to_lowercase().replace(' ', "-"),
        Visibility::Private,
        TimePolicy::Manual,
    );
    stores.narrative.insert_world(world);
}

/// Seed a timeline event.
fn seed_event(
    stores: &FixtureStores,
    world_id: &str,
    branch_id: &str,
    seq: u64,
    title: &str,
) -> String {
    let mut event = nexus_narrative::timeline_event::TimelineEvent::new(
        world_id,
        branch_id,
        nexus_narrative::timeline_event::TimelineEventType::StoryAdvance,
        seq,
    );
    event.title = Some(title.to_string());
    let id = event.timeline_event_id.clone();
    stores.narrative.insert_event(event);
    id
}

/// Seed a KB key block.
async fn seed_kb_block(stores: &FixtureStores, world_id: &str, block_type: BlockType, name: &str) {
    let kb = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::new(
        world_id, block_type, name,
    );
    stores
        .kb
        .insert_knowledge_entry(kb)
        .await
        .expect("insert key block");
}

/// Seed a knowledge entry.
async fn seed_knowledge(stores: &FixtureStores, user_id: &str, tags: &[&str], content: &str) {
    let tag_vec: Vec<nexus_knowledge::KnowledgeTag> = tags
        .iter()
        .map(|t| nexus_knowledge::KnowledgeTag::new(t))
        .collect();
    let entry = nexus_knowledge::UserKnowledgeEntry::new(user_id, tag_vec, content);
    stores
        .knowledge
        .store(entry)
        .await
        .expect("store knowledge");
}

// ── Tests ───────────────────────────────────────────────────────────

#[tokio::test]
async fn creator_memory_fixtures_appear_in_stage0() {
    let stores = FixtureStores::empty();
    let request = MomentRequest::new(stage0_with_soul_sections());

    let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;

    // Personality section is present
    assert!(
        ctx.stage0_context.contains("speculative fiction writer"),
        "stage0 must include personality"
    );
    // Experience section is present
    assert!(
        ctx.stage0_context.contains("3 novels"),
        "stage0 must include experience"
    );
    // Long-term memory is present (body content)
    assert!(
        ctx.stage0_context.contains("hero's journey"),
        "stage0 must include long-term memories"
    );
    // Fragment keywords are present
    assert!(
        ctx.stage0_context.contains("magic system"),
        "stage0 must include fragment keywords"
    );
    // System prefix is present
    assert!(
        ctx.stage0_context.contains("AI co-writer"),
        "stage0 must include system prefix"
    );
    // User prompt is present
    assert!(
        ctx.stage0_context.contains("chapter 5"),
        "stage0 must include user prompt"
    );
}

#[tokio::test]
async fn narrative_context_provides_world_and_timeline() {
    let stores = FixtureStores::empty();
    seed_world(&stores, "wld_narnia", "Narnia Chronicles");
    seed_event(&stores, "wld_narnia", "fbk_root", 1, "The Wardrobe");
    seed_event(&stores, "wld_narnia", "fbk_root", 2, "The White Witch");

    let request = MomentRequest::new(Stage0Assembly::default()).with_world("wld_narnia");

    let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;

    // World state
    let ws = ctx.world_state.expect("world state must be present");
    assert!(
        ws.contains("Narnia Chronicles"),
        "world state must include title"
    );
    assert!(
        ws.contains("wld_narnia"),
        "world state must include world_id"
    );

    // Timeline
    let tl = ctx.timeline.expect("timeline must be present");
    assert!(
        tl.contains("The Wardrobe"),
        "timeline must include first event"
    );
    assert!(
        tl.contains("The White Witch"),
        "timeline must include second event"
    );
}

#[tokio::test]
async fn world_kb_assets_are_included_by_block_type() {
    let stores = FixtureStores::empty();
    seed_world(&stores, "wld_rpg", "RPG World");
    seed_kb_block(&stores, "wld_rpg", BlockType::Character, "Aria the Mage").await;
    seed_kb_block(&stores, "wld_rpg", BlockType::Scene, "Crystal Tower").await;
    seed_kb_block(
        &stores,
        "wld_rpg",
        BlockType::InfoPoint,
        "The Great Cataclysm",
    )
    .await;

    let request = MomentRequest::new(Stage0Assembly::default()).with_world("wld_rpg");

    let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;

    let kb_text = ctx.world_kb.expect("world KB must be present");
    assert!(
        kb_text.contains("Aria the Mage"),
        "KB must list character block"
    );
    assert!(
        kb_text.contains("Crystal Tower"),
        "KB must list location block"
    );
    assert!(
        kb_text.contains("The Great Cataclysm"),
        "KB must list lore block"
    );
}

#[tokio::test]
async fn user_knowledge_entries_are_included_with_tags() {
    let stores = FixtureStores::empty();
    seed_knowledge(
        &stores,
        "user_42",
        &["worldbuilding", "magic"],
        "Magic requires a price.",
    )
    .await;
    seed_knowledge(
        &stores,
        "user_42",
        &["character"],
        "Villains need motivations.",
    )
    .await;
    seed_knowledge(&stores, "user_99", &["worldbuilding"], "Other user's note.").await;

    let request = MomentRequest::new(Stage0Assembly::default()).with_user("user_42");

    let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;

    let uk_text = ctx.user_knowledge.expect("user knowledge must be present");
    assert!(
        uk_text.contains("Magic requires a price"),
        "must include first entry"
    );
    assert!(
        uk_text.contains("Villains need motivations"),
        "must include second entry"
    );
    assert!(
        !uk_text.contains("Other user's note"),
        "must NOT include other user's entries"
    );
    // Tags are present in formatted output
    assert!(
        uk_text.contains("worldbuilding"),
        "formatted output must show tags"
    );
}

#[tokio::test]
async fn full_assembly_produces_correct_moment_context() {
    let stores = FixtureStores::empty();

    // Seed all four domain sources
    seed_world(&stores, "wld_epic", "The Great Epic");
    seed_event(&stores, "wld_epic", "fbk_root", 1, "The Call to Adventure");
    seed_kb_block(&stores, "wld_epic", BlockType::Character, "Protagonist").await;
    seed_knowledge(&stores, "user_writer", &["style"], "Use active voice.").await;

    let request = MomentRequest::new(stage0_with_soul_sections())
        .with_world("wld_epic")
        .with_user("user_writer");

    let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;

    // All fields populated
    assert!(!ctx.stage0_context.is_empty(), "stage0 must be non-empty");
    assert!(ctx.world_state.is_some(), "world_state must be populated");
    assert!(ctx.timeline.is_some(), "timeline must be populated");
    assert!(ctx.world_kb.is_some(), "world_kb must be populated");
    assert!(
        ctx.user_knowledge.is_some(),
        "user_knowledge must be populated"
    );

    // Full context combines everything
    let full = ctx.to_full_context();
    assert!(
        full.contains("speculative fiction writer"),
        "full must have personality"
    );
    assert!(
        full.contains("The Great Epic"),
        "full must have world state"
    );
    assert!(
        full.contains("The Call to Adventure"),
        "full must have timeline"
    );
    assert!(full.contains("Protagonist"), "full must have KB");
    assert!(
        full.contains("Use active voice"),
        "full must have knowledge"
    );
}

#[tokio::test]
async fn moment_context_default_is_all_none_except_stage0() {
    let ctx = MomentContext::default();
    assert!(ctx.stage0_context.is_empty());
    assert!(ctx.world_state.is_none());
    assert!(ctx.timeline.is_none());
    assert!(ctx.world_kb.is_none());
    assert!(ctx.user_knowledge.is_none());
}

#[tokio::test]
async fn missing_world_yields_no_narrative_or_kb_but_keeps_knowledge() {
    let stores = FixtureStores::empty();
    seed_knowledge(&stores, "user_42", &["test"], "Some knowledge.").await;

    // Request a non-existent world + valid user
    let request = MomentRequest::new(Stage0Assembly::default())
        .with_world("wld_ghost")
        .with_user("user_42");

    let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;

    assert!(ctx.world_state.is_none(), "missing world → no world state");
    assert!(ctx.timeline.is_none(), "missing world → no timeline");
    assert!(ctx.world_kb.is_none(), "missing world → no KB");
    assert!(
        ctx.user_knowledge.is_some(),
        "user knowledge is independent of world"
    );
}

// ── v1.191 P1 T11: ActorView-only model / inspect emission (durable §4.2) ──

/// A store holding rows the actor's snapshot does not admit can only influence
/// output through the admitted snapshot: neither the rendered World-KB section
/// nor the inspector packet may carry an un-admitted id or name, and the
/// inspector must not recompute a wider view of its own.
#[tokio::test]
async fn v1191_holder_context_model_and_inspect_emit_admitted_rows_only() {
    use nexus_moment_context_assembly::{
        build_inspector_packet, CharacterViewInput, MomentActorContext,
    };

    let stores = FixtureStores::empty();
    seed_world(&stores, "wld_ctx", "Context World");
    seed_event(&stores, "wld_ctx", "fbk_root", 1, "The Harbor Standoff");

    // Store rows: an admitted Character-global shared row plus two rows no
    // admitted selection hands this actor (a binding-local row and another
    // holder's owner-private row).
    let admitted = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::new(
        "wld_ctx",
        BlockType::Character,
        "AdmittedShared",
    );
    let admitted_id = admitted.entry_id.clone();
    stores
        .kb
        .insert_knowledge_entry(admitted.clone())
        .await
        .expect("insert admitted key block");
    let binding_local =
        nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::for_binding(
            "bnd_local",
            BlockType::Scene,
            "BindingLocalRow",
        );
    let binding_local_id = binding_local.entry_id.clone();
    stores.kb.insert_knowledge_entry(binding_local).await.unwrap();
    let mut other_private = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord::new(
        "wld_ctx",
        BlockType::Character,
        "OtherHolderPrivateRow",
    );
    other_private.holder_entry_id = Some("hld_other".to_string());
    other_private.disclosure =
        Some(nexus_knowledge::world_kb::DISCLOSURE_OWNER_PRIVATE.to_string());
    let other_private_id = other_private.entry_id.clone();
    stores.kb.insert_knowledge_entry(other_private).await.unwrap();

    let request = MomentRequest::new(Stage0Assembly {
        user_prompt: "Write the standoff.".to_string(),
        ..Stage0Assembly::default()
    })
    .with_world("wld_ctx")
    .with_actor(MomentActorContext::creator_with_view(
        CharacterViewInput::from_entries(vec![admitted]),
    ));

    let ctx = assemble_moment(&request, &stores.narrative, &stores.kb, &stores.knowledge).await;
    let kb_text = ctx.world_kb.clone().expect("admitted snapshot renders");
    assert!(kb_text.contains("AdmittedShared"), "{kb_text}");
    assert!(
        !kb_text.contains("BindingLocalRow") && !kb_text.contains("OtherHolderPrivateRow"),
        "un-admitted rows must never reach the model input: {kb_text}"
    );

    let packet = build_inspector_packet(&ctx);
    let packet_json = serde_json::to_string(&packet).expect("packet serializes");
    assert!(
        packet_json.contains(&admitted_id),
        "admitted row is traced: {packet_json}"
    );
    assert!(
        !packet_json.contains(&binding_local_id) && !packet_json.contains(&other_private_id),
        "the inspector packet must not carry an un-admitted row id: {packet_json}"
    );
    assert!(
        !packet_json.contains("BindingLocalRow") && !packet_json.contains("OtherHolderPrivateRow"),
        "the inspector packet must not carry an un-admitted row name: {packet_json}"
    );
}
