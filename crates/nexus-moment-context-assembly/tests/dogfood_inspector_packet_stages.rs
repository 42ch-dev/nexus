//! V1.151 P2 dogfood — enriched assembly inspector packet (DF-76, spec
//! `fl-l-w6-assembly-inspector.md` §2) exercised end-to-end on a seeded
//! World across generation stages, through the P0/P1 surfaces:
//! `assemble_moment_with_directive` (assembly) → `build_inspector_packet`
//! (the relocated P0 builder) — **the AC-D1 dogfood evidence**.
//!
//! The unit suites already prove the packet shape (inspector.rs), the slot
//! gate matrix (slots.rs) and the directive lifecycle (P1) in isolation.
//! This file is the **dogfood layer**: a Mira-at-Harbor-style World whose
//! lore fires into every routing shape (`world.before` via
//! `before_defs`, `world.after` via `after_defs`, the reserved
//! `style.post_history` outlet, an open `kb.outlet.aether` slot, a
//! no-hint constant, a keyword-fired entry, and a truly neutral entry with
//! no activation module) plus a relation edge pulling one keyed-but-non-
//! firing entry via a hop, is assembled at `produce` / `review` (and the
//! `persist` off-side) with an active Moment Directive, asserting:
//!
//! - `modules.placement` / `modules.activation_trace` carry the fired
//!   entries with reasons (unchanged spoke recipe, AC-I3)
//! - `slot_map` routes every fired entry into its expected slot id and
//!   reflects the post-stage-gate reality (`style.post_history` on for
//!   `produce`/`review`, off for `persist` — spec §4 matrix)
//! - `budget` carries the primary + hop token estimates with the caller
//!   cap / remaining
//! - `moment_directive` carries status/metadata only — the body never
//!   appears in the packet (AC-I3, by construction)
//! - the hop-pulled entry carries hop fields on `ctx.activation_trace`
//!   (the packet's `modules.activation_trace` is the unchanged spoke
//!   4-field shape) and its placement row carries the `relation hop`
//!   reason
//!
//! AC-I6 (neutral-only byte-equivalence) is **not** re-tested here — it is
//! the frozen golden suites' contract (`tests/golden_neutral_only_default_on.rs`,
//! `tests/golden_flag_off.rs`, `tests/golden_slots_neutral_only.rs`, and
//! V1.149's `dogfood_world_b_neutral_only_byte_equivalent`). This dogfood is
//! observational: it reads `build_inspector_packet` output, never mutates
//! the KB (AC-I6 — packet emission is a separate path from
//! `to_full_context()`).

#![allow(clippy::manual_string_new, clippy::doc_markdown)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use nexus_contracts::{BlockType, TimePolicy, Visibility};
use nexus_knowledge::world_kb::knowledge_entry::{KnowledgeEntryBody, KnowledgeEntryRecord};
use nexus_knowledge::world_kb::{InMemoryKbStore, KbStore};
use nexus_knowledge::InMemoryKnowledgeStore;
use nexus_moment_context_assembly::directive::{
    ActiveDirective, DirectiveDepth, DirectiveStore, DirectiveTtlKind,
};
use nexus_moment_context_assembly::stage0::Stage0Assembly;
use nexus_moment_context_assembly::{
    assemble_moment_with_directive, build_inspector_packet, GenerationStage, MomentContext,
    MomentRequest,
};
use nexus_narrative::timeline_event::TimelineEventType;
use nexus_narrative::InMemoryNarrativeGateway;
use nexus_spoke_adapter::adapter::activation::HopEdge;

const WORLD_ID: &str = "wld_dogfood_inspector";
/// Directive body used by the stub — must never leak into the packet.
const DIRECTIVE_BODY: &str = "Keep the prose terse and harbor-voiced.";
/// Hop cap threaded via `MomentRequest::with_hop_max_tokens` — must surface
/// in the packet's `budget.cap`.
const HOP_CAP: usize = 100_000;

/// Build the dogfood World: world + timeline beat + Stage-0 whose scan
/// text (timeline + user_prompt) fires the keyword entry (`king`).
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
        "Inspector Harbor",
        "inspector-harbor",
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
    event.timeline_event_id = "evt_inspector_001".to_string();
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

/// Build a `KnowledgeEntryRecord` with the given `modules.activation` payload.
fn entry(name: &str, id: &str, activation: &serde_json::Value) -> KnowledgeEntryRecord {
    let mut entry = KnowledgeEntryRecord::new(WORLD_ID, BlockType::Character, name);
    entry.entry_id = id.to_string();
    entry.created_at = "2026-01-01T00:00:04Z".to_string();
    entry.body = Some(KnowledgeEntryBody {
        summary: Some(format!("{name} — a harbor fixture entry.")),
        ..Default::default()
    });
    entry.modules = Some(serde_json::json!({ "activation": activation }));
    entry
}

/// Build a neutral `KnowledgeEntryRecord` — no `modules.activation` at all.
fn neutral_entry(name: &str, id: &str) -> KnowledgeEntryRecord {
    let mut entry = KnowledgeEntryRecord::new(WORLD_ID, BlockType::InfoPoint, name);
    entry.entry_id = id.to_string();
    entry.created_at = "2026-01-01T00:00:04Z".to_string();
    entry.body = Some(KnowledgeEntryBody {
        summary: Some(format!("{name} — a neutral harbor fixture entry.")),
        ..Default::default()
    });
    entry
}

/// Seed every routing shape (all constant seeds with distinct priorities →
/// deterministic emit order, spec §2 / Q5):
///
/// `world.before` (kb_bf) → default (kb_hero + kb_mid + kb_hidden +
/// kb_neutral) → `world.after` (kb_af) → `kb.outlet.aether` (kb_a) →
/// `style.post_history` (kb_ph, tail). The hop target `kb_hidden` keys
/// (`barnacle`) appear NOWHERE in the scan text, so it can only enter via
/// the relation edge from the constant seed `kb_hero`.
async fn seed_kb(kb: &InMemoryKbStore) {
    let seeds: &[(&str, &str, serde_json::Value)] = &[
        (
            "WorldBefore",
            "kb_bf",
            serde_json::json!({"constant": true, "priority": 90, "position_hint": "before_defs"}),
        ),
        (
            "Hero",
            "kb_hero",
            serde_json::json!({"constant": true, "priority": 80}),
        ),
        (
            "WorldAfter",
            "kb_af",
            serde_json::json!({"constant": true, "priority": 70, "position_hint": "after_defs"}),
        ),
        (
            "LoreA",
            "kb_a",
            serde_json::json!({"constant": true, "priority": 60,
                "position_hint": "outlet", "outlet": "aether"}),
        ),
        (
            "PostStyle",
            "kb_ph",
            serde_json::json!({"constant": true, "priority": 50,
                "position_hint": "outlet", "outlet": "style.post_history"}),
        ),
        // Keyword-fired (non-constant) — sorts after the constant band.
        (
            "Court Scribe",
            "kb_mid",
            serde_json::json!({"keys": ["king"], "priority": 30}),
        ),
        // Keyed but non-firing — only reachable via the relation hop.
        (
            "Hidden Cove",
            "kb_hidden",
            serde_json::json!({"keys": ["barnacle"], "priority": 2}),
        ),
    ];
    for (name, id, activation) in seeds {
        kb.insert_knowledge_entry(entry(name, id, activation))
            .await
            .expect("insert kb entry");
    }
    // Neutral — no activation module → default slot, "no activation module".
    kb.insert_knowledge_entry(neutral_entry("Neutral", "kb_neutral"))
        .await
        .expect("insert neutral kb entry");
}

/// Relation graph enabling hop expansion: the constant seed `kb_hero`
/// BFS-expands to pull the keyed-but-non-firing `kb_hidden` at depth 1.
fn hop_edges() -> Vec<HopEdge> {
    vec![HopEdge {
        relation_id: "rel_hero_cove".to_string(),
        from_id: "kb_hero".to_string(),
        to_id: "kb_hidden".to_string(),
        relation_type: "hero_hidden_path".to_string(),
    }]
}

/// In-memory `DirectiveStore` stub serving one fixed active directive and
/// counting injections (V1.150 dogfood adapter pattern).
#[derive(Default)]
struct StubDirectiveStore {
    active: Option<ActiveDirective>,
    calls: Arc<Mutex<usize>>,
}

impl StubDirectiveStore {
    fn with_directive(active: ActiveDirective) -> Self {
        Self {
            active: Some(active),
            ..Self::default()
        }
    }

    fn injection_count(&self) -> usize {
        *self.calls.lock().expect("lock calls")
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
        *self.calls.lock().expect("lock calls") += 1;
    }
}

fn directive() -> ActiveDirective {
    ActiveDirective {
        directive_id: "dir_dogfood".to_string(),
        body: DIRECTIVE_BODY.to_string(),
        insert_depth: DirectiveDepth::Tail,
        ttl_kind: DirectiveTtlKind::Generations,
        clear_on_scene_change: true,
        ttl_remaining: Some(3),
        status: "active".to_string(),
        scope_kind: "work".to_string(),
        scope_id: "wrk_1".to_string(),
    }
}

async fn assemble(stage: GenerationStage) -> (MomentContext, StubDirectiveStore) {
    let (narrative, kb, stage0) = build_world();
    seed_kb(&kb).await;
    let knowledge = InMemoryKnowledgeStore::new();
    let store = StubDirectiveStore::with_directive(directive());

    let request = MomentRequest::new(stage0)
        .with_world(WORLD_ID)
        .with_creator("ctr_dogfood")
        .with_work("wrk_1")
        .with_hop_edges(hop_edges())
        .with_hop_max_tokens(HOP_CAP)
        .with_generation_stage(stage);
    let ctx = assemble_moment_with_directive(&request, &narrative, &kb, &knowledge, &store).await;
    (ctx, store)
}

/// Flatten the packet's `slot_map` into `entry_id → slot`.
fn slot_map(packet: &serde_json::Value) -> HashMap<String, String> {
    packet["slot_map"]
        .as_array()
        .expect("slot_map array")
        .iter()
        .map(|row| {
            (
                row["entry_id"].as_str().expect("slot entry_id").to_string(),
                row["slot"].as_str().expect("slot id").to_string(),
            )
        })
        .collect()
}

/// `produce`: every slot fills (style on — AC-I4 on-side), the directive
/// metadata is present (no body — AC-I3), and the hop entry carries hop
/// fields on the trace + the `relation hop` reason in placement.
// The one comprehensive packet assertion mirrors V1.149's single-big-test
// dogfood shape; splitting it would scatter one packet's evidence.
#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn dogfood_produce_packet_carries_slots_directive_and_hops() {
    let (ctx, store) = assemble(GenerationStage::Produce).await;
    let packet = build_inspector_packet(&ctx);

    // ── modules.*: fired entries with reasons (unchanged spoke recipe) ──
    let placement = packet["modules"]["placement"]
        .as_array()
        .expect("placement array");
    let placed: HashMap<&str, &serde_json::Value> = placement
        .iter()
        .map(|row| (row["entry_id"].as_str().expect("placed id"), row))
        .collect();
    for id in [
        "kb_bf",
        "kb_hero",
        "kb_af",
        "kb_a",
        "kb_ph",
        "kb_mid",
        "kb_hidden",
        "kb_neutral",
    ] {
        assert!(placed.contains_key(id), "placement must include {id}");
    }
    assert!(
        placed["kb_bf"]["reason"]
            .as_str()
            .expect("reason")
            .contains("constant seed"),
        "constant seed reason: {}",
        placed["kb_bf"]["reason"]
    );
    assert_eq!(
        placed["kb_neutral"]["reason"].as_str().expect("reason"),
        "no activation module"
    );
    assert!(
        placed["kb_mid"]["reason"]
            .as_str()
            .expect("reason")
            .contains("matched key"),
        "keyword-fired entry reason: {}",
        placed["kb_mid"]["reason"]
    );
    assert!(
        placed["kb_hidden"]["reason"]
            .as_str()
            .expect("reason")
            .starts_with("relation hop"),
        "hop-pulled entry must carry the relation-hop reason: {}",
        placed["kb_hidden"]["reason"]
    );

    let trace = packet["modules"]["activation_trace"]
        .as_array()
        .expect("trace array");
    // 8 primary rows + 1 extra hop-accepted row for kb_hidden (its primary
    // row is rejected — keys absent from the scan — the hop row carries the
    // pull). Same engine semantics V1.149's dogfood asserts on the ctx trace.
    assert_eq!(trace.len(), 9, "8 primary rows + 1 hop row");
    assert!(
        trace
            .iter()
            .any(|row| row["entry_id"] == "kb_mid" && row["accepted"] == true),
        "keyword-fired entry must be accepted in the activation trace"
    );
    let hidden_rows: Vec<&serde_json::Value> = trace
        .iter()
        .filter(|row| row["entry_id"] == "kb_hidden")
        .collect();
    assert_eq!(
        hidden_rows.len(),
        2,
        "kb_hidden must have a rejected primary row + an accepted hop row"
    );
    assert!(
        hidden_rows.iter().any(|row| row["accepted"] == false),
        "kb_hidden primary row rejected (no key matched)"
    );
    assert!(
        hidden_rows.iter().any(|row| row["accepted"] == true),
        "kb_hidden hop row accepted"
    );

    // Hop fields live on `ctx.activation_trace` (the packet's
    // `modules.activation_trace` is the unchanged spoke 4-field shape).
    let hidden = ctx
        .activation_trace
        .as_deref()
        .expect("activation_trace present")
        .iter()
        .find(|row| row.entry_id == "kb_hidden" && row.accepted && row.hop_depth.is_some())
        .expect("kb_hidden hop-accepted trace row");
    assert_eq!(hidden.hop_depth, Some(1), "Hidden Cove depth");
    assert_eq!(
        hidden.hop_origin_entry_id.as_deref(),
        Some("kb_hero"),
        "Hidden Cove hop origin"
    );
    assert_eq!(
        hidden.source_relation_type.as_deref(),
        Some("hero_hidden_path")
    );
    assert_eq!(hidden.source_relation_id.as_deref(), Some("rel_hero_cove"));

    // ── slot_map: every fired entry routed to its expected slot ──
    let slots = slot_map(&packet);
    assert_eq!(slots.len(), 9, "8 lore entries + 1 synthetic directive row");
    assert_eq!(slots["kb_bf"], "world.before");
    assert_eq!(slots["kb_af"], "world.after");
    assert_eq!(slots["kb_a"], "kb.outlet.aether");
    assert_eq!(slots["kb_ph"], "style.post_history");
    assert_eq!(slots["kb_hero"], "default");
    assert_eq!(slots["kb_mid"], "default");
    assert_eq!(slots["kb_neutral"], "default");
    assert_eq!(
        slots["kb_hidden"], "default",
        "hop-pulled entry routes to default"
    );
    assert_eq!(slots["dir_dogfood"], "moment.directive");

    // The rendered World-KB matches the slot map (post-gate reality).
    let kb_text = ctx.world_kb.as_deref().expect("world_kb present");
    for heading in [
        "### World (Before)",
        "### World (After)",
        "### Outlet: aether",
        "### Style (Post-History)",
    ] {
        assert!(kb_text.contains(heading), "missing {heading}");
    }

    // ── budget: primary + hop estimates with caller cap / remaining ──
    assert!(
        packet["budget"]["primary_tokens_est"]
            .as_u64()
            .expect("primary est")
            > 0,
        "primary estimate must be non-zero"
    );
    assert!(
        packet["budget"]["hop_tokens_est"]
            .as_u64()
            .expect("hop est")
            > 0,
        "hop estimate must be non-zero (Hidden Cove pulled)"
    );
    assert_eq!(
        packet["budget"]["cap"].as_u64().expect("cap"),
        HOP_CAP as u64,
        "caller hop cap must surface in the packet"
    );
    assert!(
        packet["budget"]["remaining"].is_number(),
        "remaining must be present when a cap was set"
    );

    // ── moment_directive: status/metadata only, body never present ──
    assert_eq!(
        packet["moment_directive"],
        serde_json::json!({
            "scope": "work",
            "scope_id": "wrk_1",
            "insert_depth": "tail",
            "ttl_kind": "generations",
            "ttl_remaining": 3,
            "clear_on_scene_change": true,
            "status": "active",
        })
    );
    assert!(
        packet["moment_directive"].get("body").is_none(),
        "moment_directive must never carry the body key"
    );
    assert!(
        !packet.to_string().contains(DIRECTIVE_BODY),
        "directive body must never leak into the packet (AC-I3)"
    );
    assert_eq!(store.injection_count(), 1, "one directive injection");
}

/// `review`: style still on (AC-I4 on-side) — the packet's `slot_map`
/// keeps the `style.post_history` row and the directive metadata stays.
#[tokio::test]
async fn dogfood_review_keeps_style_slot_in_packet() {
    let (ctx, store) = assemble(GenerationStage::Review).await;
    let packet = build_inspector_packet(&ctx);

    let slots = slot_map(&packet);
    assert_eq!(
        slots.get("kb_ph").map(String::as_str),
        Some("style.post_history"),
        "review: style slot on"
    );
    let kb_text = ctx.world_kb.as_deref().expect("world_kb present");
    assert!(kb_text.contains("### Style (Post-History)"));
    assert!(kb_text.contains("- **PostStyle**"));
    assert_eq!(
        packet["moment_directive"]["status"]
            .as_str()
            .expect("status"),
        "active",
        "review: directive metadata present"
    );
    assert!(
        !packet.to_string().contains(DIRECTIVE_BODY),
        "AC-I3: no body at review either"
    );
    assert_eq!(store.injection_count(), 1);
}

/// `persist` (non-style stage): the slot_map reflects the post-gate
/// reality — `style.post_history` is dropped (not re-routed), every other
/// slot stays, and the directive still injects (TTL governs lifetime, not
/// stage — spec §4).
#[tokio::test]
async fn dogfood_persist_gates_style_in_slot_map() {
    let (ctx, store) = assemble(GenerationStage::Persist).await;
    let packet = build_inspector_packet(&ctx);

    let slots = slot_map(&packet);
    assert!(
        !slots.values().any(|slot| slot == "style.post_history"),
        "persist: no style.post_history row in the slot map"
    );
    assert!(
        !slots.contains_key("kb_ph"),
        "persist: style entry dropped, not re-routed"
    );
    let kb_text = ctx.world_kb.as_deref().expect("world_kb present");
    assert!(!kb_text.contains("### Style (Post-History)"));
    assert!(!kb_text.contains("- **PostStyle**"));
    for (id, slot) in [
        ("kb_bf", "world.before"),
        ("kb_af", "world.after"),
        ("kb_a", "kb.outlet.aether"),
        ("kb_hero", "default"),
        ("kb_mid", "default"),
        ("kb_neutral", "default"),
        ("kb_hidden", "default"),
        ("dir_dogfood", "moment.directive"),
    ] {
        assert_eq!(
            slots.get(id).map(String::as_str),
            Some(slot),
            "persist: {id} must stay in {slot}"
        );
    }
    assert_eq!(
        packet["moment_directive"]["status"]
            .as_str()
            .expect("status"),
        "active",
        "persist: directive still injects (not stage-gated)"
    );
    assert_eq!(store.injection_count(), 1);
}
