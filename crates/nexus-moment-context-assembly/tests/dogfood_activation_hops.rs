//! V1.149 P2 dogfood — default-on lore activation + Relation hop expand
//! (DF-74) exercised end-to-end through `assemble_moment` on seeded Worlds.
//!
//! The P0/P1 unit suites already prove the engine truth tables, ordering
//! comparator, budget semantics and BFS termination in isolation. This file
//! is the **dogfood layer**: it seeds a Mira-at-Harbor-style World A (full
//! spoke `modules.activation` dialect + relation edges) and a richer
//! neutral-only World B, then verifies the assembled output end-to-end —
//! gating, dialect fire modes, emit order, hop pulls within budget, cycle
//! termination, duplicate-freedom, and the HARD neutral-only byte-
//! equivalence guarantee.
//!
//! Assertion design note: `InMemoryKbStore::query` iterates a `HashMap`
//! without sorting, so exact emit-order assertions are only valid where the
//! activation sort key fully determines the order. The World A fixture gives
//! every entry a distinct `(constant, priority, order)` triple (the single
//! tie — neutral vs hop-pulled at (0,0) — is resolved by the engine's
//! stable append order), so the emitted sequence is deterministic.

#![allow(clippy::manual_string_new, clippy::doc_markdown)]

use std::collections::{HashMap, HashSet};

use nexus_contracts::{BlockType, TimePolicy, Visibility};
use nexus_knowledge::world_kb::knowledge_entry::{KnowledgeEntryBody, KnowledgeEntryRecord};
use nexus_knowledge::world_kb::{InMemoryKbStore, KbStore};
use nexus_knowledge::{InMemoryKnowledgeStore, KnowledgeStore, KnowledgeTag};
use nexus_moment_context_assembly::stage0::Stage0Assembly;
use nexus_moment_context_assembly::{assemble_moment, MomentContext, MomentRequest};
use nexus_narrative::timeline_event::TimelineEventType;
use nexus_narrative::InMemoryNarrativeGateway;
use nexus_spoke_adapter::adapter::activation::{ActivationTraceEntry, HopEdge};

// ──────────────────────────────────────────────────────────────────────────
// World A — Mira-at-Harbor style activation World (Harbor → Dawn Dock, etc.)
// ──────────────────────────────────────────────────────────────────────────

const WORLD_A_ID: &str = "wld_dogfood_harbor";
const WORLD_B_ID: &str = "wld_dogfood_neutral";

/// Build the World A fixture: world + 3 timeline beats + 12 KB entries
/// carrying the full `modules.activation` dialect + 7 relation edges.
///
/// The scan text (stage0 + timeline outline beats) contains: `lighthouse`,
/// `Harbor Guild`, `dawn dock`, `evening bell`, `quay`, `tide`, `morning
/// bell`, `harbor guild hall` — and deliberately NO `barnacle`, `harbor
/// lantern`, or `starfish` (the keys of the three entries that must NOT
/// fire by keyword).
fn build_harbor_fixture() -> (
    InMemoryNarrativeGateway<InMemoryKbStore>,
    InMemoryKbStore,
    Stage0Assembly,
) {
    let narrative = InMemoryNarrativeGateway::new(InMemoryKbStore::new());
    let kb = InMemoryKbStore::new();

    // ── World (frozen created_at) ──
    let mut world = nexus_narrative::world::World::new(
        WORLD_A_ID,
        "ctr_dogfood",
        "Mira at Harbor",
        "mira-at-harbor",
        Visibility::Private,
        TimePolicy::Manual,
    );
    world.created_at = "2026-01-01T00:00:00Z".to_string();
    narrative.insert_world(world);

    // ── Timeline beats (outline scan sources, frozen) ──
    let mut evt1 = nexus_narrative::timeline_event::TimelineEvent::new(
        WORLD_A_ID,
        "fbk_root",
        TimelineEventType::StoryAdvance,
        1,
    );
    evt1.timeline_event_id = "evt_dogfood_001".to_string();
    evt1.created_at = "2026-01-01T00:00:01Z".to_string();
    evt1.title = Some("The lighthouse keeper meets the Harbor Guild at the dawn dock.".to_string());
    narrative.insert_event(evt1);

    let mut evt2 = nexus_narrative::timeline_event::TimelineEvent::new(
        WORLD_A_ID,
        "fbk_root",
        TimelineEventType::StoryAdvance,
        2,
    );
    evt2.timeline_event_id = "evt_dogfood_002".to_string();
    evt2.created_at = "2026-01-01T00:00:02Z".to_string();
    evt2.title = Some("The evening bell rings across the quay as the tide turns.".to_string());
    narrative.insert_event(evt2);

    let mut evt3 = nexus_narrative::timeline_event::TimelineEvent::new(
        WORLD_A_ID,
        "fbk_root",
        TimelineEventType::StoryAdvance,
        3,
    );
    evt3.timeline_event_id = "evt_dogfood_003".to_string();
    evt3.created_at = "2026-01-01T00:00:03Z".to_string();
    evt3.title = Some("The morning bell answers from the harbor guild hall.".to_string());
    narrative.insert_event(evt3);

    // ── Stage0 (scan source; no wall-clock dep) ──
    let stage0 = Stage0Assembly {
        personality: "A creative writer who loves worldbuilding.".to_string(),
        experience: "Published 3 novels about seaside towns.".to_string(),
        system_prefix: "You are an AI co-writer for a fantasy novel.".to_string(),
        user_prompt: "The tide rises at the quay as dawn breaks over the harbor.".to_string(),
        ..Stage0Assembly::default()
    };

    (narrative, kb, stage0)
}

fn activation(
    keys: &[&str],
    secondary_keys: &[&str],
    logic: &str,
    constant: bool,
    priority: f64,
    order: f64,
    match_mode: Option<&str>,
) -> serde_json::Value {
    let mut module = serde_json::json!({
        "keys": keys,
        "secondary_keys": secondary_keys,
        "logic": logic,
        "constant": constant,
        "priority": priority,
        "order": order,
    });
    if let Some(mode) = match_mode {
        module["match"] = serde_json::Value::String(mode.to_string());
    }
    serde_json::json!({ "activation": module })
}

fn entry(
    world_id: &str,
    id: &str,
    block_type: BlockType,
    name: &str,
    summary: &str,
    modules: Option<serde_json::Value>,
) -> KnowledgeEntryRecord {
    let mut e = KnowledgeEntryRecord::new(world_id, block_type, name);
    e.entry_id = id.to_string();
    e.created_at = "2026-01-01T00:00:04Z".to_string();
    e.body = Some(KnowledgeEntryBody {
        summary: Some(summary.to_string()),
        ..Default::default()
    });
    e.modules = modules;
    e
}

// Long integration test; splitting would obscure the end-to-end scenario
#[allow(clippy::too_many_lines)]
async fn seed_harbor_kb(kb: &InMemoryKbStore) -> Vec<KnowledgeEntryRecord> {
    let entries = vec![
        // Constant seed — always fires, top of the constant band.
        entry(
            WORLD_A_ID,
            "kb_harbor",
            BlockType::InfoPoint,
            "Harbor",
            "The old harbor where ships rest.",
            Some(activation(&[], &[], "and_any", true, 10.0, 0.0, None)),
        ),
        // Primary literal fire (also self-match via canonical name).
        entry(
            WORLD_A_ID,
            "kb_dawn_dock",
            BlockType::InfoPoint,
            "Dawn Dock",
            "Dawn's first light on the dock planks.",
            Some(activation(&["dawn"], &[], "and_any", false, 9.0, 0.0, None)),
        ),
        // Secondary logic (and_any): primary `harbor` + secondary `guild`.
        entry(
            WORLD_A_ID,
            "kb_harbor_guild",
            BlockType::Faction,
            "Harbor Guild",
            "The guild that keeps the harbor books.",
            Some(activation(
                &["harbor"],
                &["guild"],
                "and_any",
                false,
                8.0,
                0.0,
                None,
            )),
        ),
        // Non-literal match mode #1: whole_word.
        entry(
            WORLD_A_ID,
            "kb_tide_altar",
            BlockType::InfoPoint,
            "Tide Altar",
            "A shrine where the tide is honored.",
            Some(activation(
                &["tide"],
                &[],
                "and_any",
                false,
                7.0,
                0.0,
                Some("whole_word"),
            )),
        ),
        // Non-literal match mode #2: regex (fires on lowercase `lighthouse`
        // in the timeline beat; `Lighthouse` canonical does NOT match the
        // case-sensitive pattern).
        entry(
            WORLD_A_ID,
            "kb_lighthouse",
            BlockType::Item,
            "Lighthouse",
            "The beacon above the harbor mouth.",
            Some(activation(
                &["light.*"],
                &[],
                "and_any",
                false,
                5.0,
                5.0,
                Some("regex"),
            )),
        ),
        // Same priority band, distinct order → order ascending (lower first).
        // Keys fire ONLY from the external timeline scan (not self-match).
        entry(
            WORLD_A_ID,
            "kb_order_evening",
            BlockType::Item,
            "Evening Order Bell",
            "Rung at dusk.",
            Some(activation(
                &["evening bell"],
                &[],
                "and_any",
                false,
                3.0,
                1.0,
                None,
            )),
        ),
        entry(
            WORLD_A_ID,
            "kb_order_morning",
            BlockType::Item,
            "Morning Order Bell",
            "Rung at sunrise.",
            Some(activation(
                &["morning bell"],
                &[],
                "and_any",
                false,
                3.0,
                2.0,
                None,
            )),
        ),
        // Low-priority fire — sorts below the priority-2 hop-pulled entry.
        entry(
            WORLD_A_ID,
            "kb_low_priority_quay",
            BlockType::InfoPoint,
            "Low Priority Quay",
            "A small dock for fishing boats.",
            Some(activation(&["quay"], &[], "and_any", false, 1.0, 0.0, None)),
        ),
        // Neutral — no activation module; must remain matched (byte-
        // equivalence family).
        entry(
            WORLD_A_ID,
            "kb_old_passage",
            BlockType::InfoPoint,
            "Old Passage",
            "The narrow lane behind the warehouses.",
            None,
        ),
        // Gating: keyed entry whose key appears NOWHERE → must be filtered.
        entry(
            WORLD_A_ID,
            "kb_seagull",
            BlockType::Species,
            "Seagull",
            "A gray bird watches the docks.",
            Some(activation(
                &["barnacle"],
                &[],
                "and_any",
                false,
                4.0,
                0.0,
                None,
            )),
        ),
        // Hop targets: keyed but non-firing (keys absent from scan), so they
        // may only enter via relation hops from Harbor.
        entry(
            WORLD_A_ID,
            "kb_cove_keeper",
            BlockType::Character,
            "Cove Keeper",
            "The keeper who minds the seaward cove.",
            Some(activation(
                &["harbor lantern"],
                &[],
                "and_any",
                false,
                2.0,
                0.0,
                None,
            )),
        ),
        entry(
            WORLD_A_ID,
            "kb_tide_pool",
            BlockType::InfoPoint,
            "Tide Pool",
            "A quiet pool among the rocks.",
            Some(activation(
                &["starfish"],
                &[],
                "and_any",
                false,
                0.0,
                0.0,
                None,
            )),
        ),
    ];
    for e in &entries {
        kb.insert_knowledge_entry(e.clone())
            .await
            .expect("insert kb entry");
    }
    entries
}

fn harbor_edges() -> Vec<HopEdge> {
    vec![
        // Harbor → Dawn Dock style (the handbook fixture pair).
        HopEdge {
            relation_id: "rel_harbor_dock".to_string(),
            from_id: "kb_harbor".to_string(),
            to_id: "kb_dawn_dock".to_string(),
            relation_type: "connects".to_string(),
        },
        HopEdge {
            relation_id: "rel_harbor_guild".to_string(),
            from_id: "kb_harbor".to_string(),
            to_id: "kb_harbor_guild".to_string(),
            relation_type: "connects".to_string(),
        },
        HopEdge {
            relation_id: "rel_dock_lighthouse".to_string(),
            from_id: "kb_dawn_dock".to_string(),
            to_id: "kb_lighthouse".to_string(),
            relation_type: "connects".to_string(),
        },
        HopEdge {
            relation_id: "rel_lighthouse_altar".to_string(),
            from_id: "kb_lighthouse".to_string(),
            to_id: "kb_tide_altar".to_string(),
            relation_type: "connects".to_string(),
        },
        // Hop chain + cycle: Harbor → Cove Keeper → Tide Pool → Cove Keeper,
        // plus a Cove Keeper self-loop.
        HopEdge {
            relation_id: "rel_harbor_cove".to_string(),
            from_id: "kb_harbor".to_string(),
            to_id: "kb_cove_keeper".to_string(),
            relation_type: "harbor_cove_path".to_string(),
        },
        HopEdge {
            relation_id: "rel_cove_pool".to_string(),
            from_id: "kb_cove_keeper".to_string(),
            to_id: "kb_tide_pool".to_string(),
            relation_type: "cove_tide_pool".to_string(),
        },
        HopEdge {
            relation_id: "rel_cove_self".to_string(),
            from_id: "kb_cove_keeper".to_string(),
            to_id: "kb_cove_keeper".to_string(),
            relation_type: "self_loop".to_string(),
        },
    ]
}

/// Emit-order assertion helper: extract canonical names from the rendered
/// `world_kb` section, in emitted order.
fn kb_names(world_kb: &str) -> Vec<String> {
    world_kb
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("- **")?;
            let name = rest.split("**").next()?;
            Some(name.to_string())
        })
        .collect()
}

async fn assemble_harbor(
    narrative: &InMemoryNarrativeGateway<InMemoryKbStore>,
    kb: &InMemoryKbStore,
    stage0: &Stage0Assembly,
) -> MomentContext {
    let request = MomentRequest::new(stage0.clone())
        .with_world(WORLD_A_ID)
        .with_hop_edges(harbor_edges())
        // Generous budget: depth + cycle only bound the pull (spec Q1);
        // budget semantics are unit-covered in activation.rs.
        .with_hop_max_tokens(100_000);
    assemble_moment(&request, narrative, kb, &InMemoryKnowledgeStore::new()).await
}

fn trace_by_id(ctx: &MomentContext) -> HashMap<String, Vec<ActivationTraceEntry>> {
    let mut map: HashMap<String, Vec<ActivationTraceEntry>> = HashMap::new();
    for row in ctx.activation_trace.clone().unwrap_or_default() {
        map.entry(row.entry_id.clone()).or_default().push(row);
    }
    map
}

#[tokio::test]
// Long integration test; splitting would obscure the end-to-end scenario
#[allow(clippy::too_many_lines)]
async fn dogfood_world_a_activation_hops_end_to_end() {
    let (narrative, kb, stage0) = build_harbor_fixture();
    seed_harbor_kb(&kb).await;
    let ctx = assemble_harbor(&narrative, &kb, &stage0).await;

    let kb_text = ctx.world_kb.as_deref().expect("world_kb must be present");
    let trace = trace_by_id(&ctx);

    // ── (a) Gating filters non-matching keyed entries ──
    assert!(
        !kb_text.contains("Seagull"),
        "gated entry 'Seagull' (keys [barnacle]) must not appear in output"
    );
    let seagull_rows = &trace["kb_seagull"];
    assert!(
        seagull_rows.iter().all(|r| !r.accepted),
        "Seagull must be rejected by the primary pass"
    );
    assert!(
        seagull_rows[0].reason.contains("no key matched"),
        "Seagull rejection reason: {}",
        seagull_rows[0].reason
    );

    // ── (b) Dialect fire-conditions ──
    // and_any secondary logic.
    let guild = &trace["kb_harbor_guild"];
    assert!(
        guild
            .iter()
            .any(|r| r.accepted && r.reason.contains("and_any")),
        "Harbor Guild must fire via and_any: {:?}",
        guild.iter().map(|r| &r.reason).collect::<Vec<_>>()
    );
    // whole_word match mode.
    let altar = &trace["kb_tide_altar"];
    assert!(
        altar
            .iter()
            .any(|r| r.accepted && r.reason.contains("whole_word")),
        "Tide Altar must fire via whole_word: {:?}",
        altar.iter().map(|r| &r.reason).collect::<Vec<_>>()
    );
    // regex match mode (case-sensitive; fires on timeline beat only).
    let lighthouse = &trace["kb_lighthouse"];
    assert!(
        lighthouse
            .iter()
            .any(|r| r.accepted && r.reason.contains("regex")),
        "Lighthouse must fire via regex: {:?}",
        lighthouse.iter().map(|r| &r.reason).collect::<Vec<_>>()
    );
    // constant seed.
    let harbor = &trace["kb_harbor"];
    assert!(
        harbor
            .iter()
            .any(|r| r.accepted && r.reason.contains("constant seed (constant)")),
        "Harbor must fire as a constant seed: {:?}",
        harbor.iter().map(|r| &r.reason).collect::<Vec<_>>()
    );
    // neutral.
    let passage = &trace["kb_old_passage"];
    assert!(
        passage
            .iter()
            .any(|r| r.accepted && r.reason == "no activation module"),
        "Old Passage must be included as neutral"
    );

    // ── (c) priority/order emit order ──
    let names = kb_names(kb_text);
    let expected_order = [
        "Harbor",             // constant band first
        "Dawn Dock",          // priority 9
        "Harbor Guild",       // priority 8
        "Tide Altar",         // priority 7 (whole_word)
        "Lighthouse",         // priority 5
        "Evening Order Bell", // priority 3, order 1 → before Morning
        "Morning Order Bell", // priority 3, order 2
        "Cove Keeper",        // priority 2 (hop-pulled, own sort key)
        "Low Priority Quay",  // priority 1
        "Old Passage",        // priority 0 neutral (stable before hop tie)
        "Tide Pool",          // priority 0 hop-pulled (stable after primary)
    ];
    assert_eq!(names, expected_order, "emitted order mismatch");

    // ── (d) hops pull neighbors within budget ──
    let cove_hop = trace["kb_cove_keeper"]
        .iter()
        .find(|r| r.accepted && r.hop_depth.is_some())
        .expect("Cove Keeper must carry a hop-accepted trace row");
    assert_eq!(cove_hop.hop_depth, Some(1), "Cove Keeper depth");
    assert_eq!(
        cove_hop.hop_origin_entry_id.as_deref(),
        Some("kb_harbor"),
        "Cove Keeper hop origin"
    );
    assert_eq!(
        cove_hop.source_relation_type.as_deref(),
        Some("harbor_cove_path")
    );
    assert_eq!(
        cove_hop.source_relation_id.as_deref(),
        Some("rel_harbor_cove")
    );

    let pool_hop = trace["kb_tide_pool"]
        .iter()
        .find(|r| r.accepted && r.hop_depth.is_some())
        .expect("Tide Pool must carry a hop-accepted trace row");
    assert_eq!(
        pool_hop.hop_depth,
        Some(2),
        "Tide Pool depth (≤ max_hops 2)"
    );
    assert_eq!(
        pool_hop.hop_origin_entry_id.as_deref(),
        Some("kb_cove_keeper"),
        "Tide Pool hop origin"
    );
    assert_eq!(
        pool_hop.source_relation_type.as_deref(),
        Some("cove_tide_pool")
    );

    // No re-fire: hop-pulled rows are NOT key matches.
    assert!(
        cove_hop.reason.starts_with("relation hop"),
        "hop-pulled row must not re-fire keys: {}",
        cove_hop.reason
    );

    // ── (e)+(f) cycle terminates; no duplicate entries ──
    // The Cove Keeper ↔ Tide Pool 2-cycle + self-loop would hang or double-
    // pull if the `visited` guard failed. Exactly-once presence is the proof.
    let unique: HashSet<String> = names.iter().cloned().collect();
    assert_eq!(unique.len(), names.len(), "no duplicate entries emitted");
    assert_eq!(kb_text.matches("Cove Keeper").count(), 1);
    assert_eq!(kb_text.matches("Tide Pool").count(), 1);

    // One primary trace row per entry (12), each accepted at most once; hop
    // rows carry the hop fields and are additional accepted rows only for
    // the two pulled entries. Seagull is the single rejected entry.
    assert_eq!(trace.len(), 12, "one primary trace row per entry");
    for (id, rows) in &trace {
        let accepted = rows.iter().filter(|r| r.accepted).count();
        let expected = usize::from(id.as_str() != "kb_seagull");
        assert_eq!(
            accepted, expected,
            "entry {id} must be accepted exactly {expected} time(s) (accepted rows: {accepted})"
        );
        // Pre-visited primary entries never get hop rows.
        if !matches!(id.as_str(), "kb_cove_keeper" | "kb_tide_pool") {
            assert!(
                rows.iter().all(|r| r.hop_depth.is_none()),
                "entry {id} must not carry hop fields"
            );
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// World B — neutral-only control: default-on must be byte-identical to the
// explicit off-switch (the HARD ship gate, on a richer multi-entry fixture).
// ──────────────────────────────────────────────────────────────────────────

fn build_neutral_world_b() -> (
    InMemoryNarrativeGateway<InMemoryKbStore>,
    InMemoryKbStore,
    InMemoryKnowledgeStore,
    Stage0Assembly,
) {
    let narrative = InMemoryNarrativeGateway::new(InMemoryKbStore::new());
    let kb = InMemoryKbStore::new();
    let knowledge = InMemoryKnowledgeStore::new();

    let mut world = nexus_narrative::world::World::new(
        WORLD_B_ID,
        "ctr_dogfood",
        "Neutral Harbor",
        "neutral-harbor",
        Visibility::Private,
        TimePolicy::Manual,
    );
    world.created_at = "2026-01-01T00:00:00Z".to_string();
    narrative.insert_world(world);

    let mut event = nexus_narrative::timeline_event::TimelineEvent::new(
        WORLD_B_ID,
        "fbk_root",
        TimelineEventType::StoryAdvance,
        1,
    );
    event.timeline_event_id = "evt_neutral_b_001".to_string();
    event.created_at = "2026-01-01T00:00:01Z".to_string();
    event.title = Some("The harbor wakes at dawn.".to_string());
    narrative.insert_event(event);

    let stage0 = Stage0Assembly {
        personality: "A careful historian of the coast.".to_string(),
        experience: "Ten years of harbor records.".to_string(),
        system_prefix: "You are an AI co-writer.".to_string(),
        user_prompt: "The tide turns and the quay fills with boats.".to_string(),
        ..Stage0Assembly::default()
    };

    (narrative, kb, knowledge, stage0)
}

async fn seed_neutral_world_b(
    kb: &InMemoryKbStore,
    knowledge: &InMemoryKnowledgeStore,
) -> Vec<KnowledgeEntryRecord> {
    let entries = vec![
        entry(
            WORLD_B_ID,
            "kb_nb_hero",
            BlockType::Character,
            "Hero",
            "A sailor from the outer isles.",
            None,
        ),
        entry(
            WORLD_B_ID,
            "kb_nb_tavern",
            BlockType::InfoPoint,
            "Harbor Tavern",
            "Where the crews drink.",
            None,
        ),
        entry(
            WORLD_B_ID,
            "kb_nb_map",
            BlockType::Item,
            "Master Map",
            "Charted by the harbor master.",
            None,
        ),
        entry(
            WORLD_B_ID,
            "kb_nb_driftwood",
            BlockType::Item,
            "Driftwood",
            "Washed up after the storm.",
            None,
        ),
        entry(
            WORLD_B_ID,
            "kb_nb_fisher",
            BlockType::Character,
            "Old Fisher",
            "Knows every shoal.",
            None,
        ),
    ];
    for e in &entries {
        kb.insert_knowledge_entry(e.clone())
            .await
            .expect("insert neutral kb entry");
    }

    let mut uke1 = nexus_knowledge::UserKnowledgeEntry::new(
        "user_dogfood_b",
        vec![KnowledgeTag::new("lore")],
        "The hero carries a compass that always points home.",
    );
    uke1.id = "kno_nb_001".to_string();
    uke1.created_at = "2026-01-01T00:00:04Z".to_string();
    uke1.updated_at = "2026-01-01T00:00:04Z".to_string();
    knowledge.store(uke1).await.expect("store knowledge");

    let mut uke2 = nexus_knowledge::UserKnowledgeEntry::new(
        "user_dogfood_b",
        vec![KnowledgeTag::new("lore")],
        "The old fisher owes the hero a favor from the storm year.",
    );
    uke2.id = "kno_nb_002".to_string();
    uke2.created_at = "2026-01-01T00:00:05Z".to_string();
    uke2.updated_at = "2026-01-01T00:00:05Z".to_string();
    knowledge.store(uke2).await.expect("store knowledge");

    entries
}

#[tokio::test]
async fn dogfood_world_b_neutral_only_byte_equivalent() {
    let (narrative, kb, knowledge, stage0) = build_neutral_world_b();
    let seeded = seed_neutral_world_b(&kb, &knowledge).await;

    // Default-on: `MomentRequest::new` leaves activation_enabled = true.
    let default_on_request = MomentRequest::new(stage0.clone())
        .with_world(WORLD_B_ID)
        .with_user("user_dogfood_b");
    let explicit_off_request = MomentRequest::new(stage0)
        .with_world(WORLD_B_ID)
        .with_user("user_dogfood_b")
        .with_activation_enabled(false);

    let default_on = assemble_moment(&default_on_request, &narrative, &kb, &knowledge).await;
    let explicit_off = assemble_moment(&explicit_off_request, &narrative, &kb, &knowledge).await;

    assert_eq!(
        default_on.to_full_context(),
        explicit_off.to_full_context(),
        "neutral-only World: default-on output must be byte-identical to \
         explicit activation_enabled:false (V1.146 flag-off semantics)"
    );

    // Nothing dropped, nothing reordered — every seeded entry present once.
    let kb_text = default_on.world_kb.as_deref().expect("world_kb present");
    for e in &seeded {
        assert_eq!(
            kb_text
                .matches(&format!("**{}**", e.canonical_name))
                .count(),
            1,
            "neutral entry {} must appear exactly once",
            e.canonical_name
        );
    }

    // Trace plumbing: default-on exposes per-entry neutral trace; off does not.
    assert!(
        default_on.activation_trace.is_some(),
        "default-on must expose an activation trace"
    );
    assert!(
        explicit_off.activation_trace.is_none(),
        "explicit-off must not expose an activation trace"
    );
}
