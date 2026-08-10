//! V1.74 World KB relationship integration tests.
//!
//! Exercises `patch_relationship` (add/update/remove) and the `get_graph`
//! projection directly against a canonical daemon `WorkspaceState`.

use axum::extract::{Path, Query, State};
use axum::Json;
use nexus_contracts::{
    world_kb_patch_relationship_request::{
        NexusWorldKbRelationshipInput, NexusWorldKbRelationshipKind,
    },
    WorldKbPatchRelationshipRequest, WorldKbRelationshipKind,
};
use nexus_daemon_runtime::api::handlers::world_kb::{get_graph, patch_relationship, GraphQuery};
use nexus_daemon_runtime::workspace::WorkspaceState;

fn relation_type_to_nexus(kind: WorldKbRelationshipKind) -> NexusWorldKbRelationshipKind {
    kind.as_str().parse().expect("wire enum parity")
}

async fn seed_key_block(
    pool: &sqlx::SqlitePool,
    key_block_id: &str,
    world_id: &str,
    block_type: &str,
    canonical_name: &str,
    status: &str,
) {
    sqlx::query(
        "INSERT INTO kb_key_blocks \
         (key_block_id, world_id, block_type, canonical_name, status, revision, body_json, \
          created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, 0, ?, datetime('now'), datetime('now'))",
    )
    .bind(key_block_id)
    .bind(world_id)
    .bind(block_type)
    .bind(canonical_name)
    .bind(status)
    .bind("{}")
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_source_anchor(pool: &sqlx::SqlitePool, key_block_id: &str, anchor_ordinal: i64) {
    sqlx::query(
        "INSERT INTO kb_source_anchors \
         (key_block_id, anchor_ordinal, source_anchor_json) \
         VALUES (?, ?, ?)",
    )
    .bind(key_block_id)
    .bind(anchor_ordinal)
    .bind(r#"{"reference":"work:we_source"}"#)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_key_block_with_source(
    pool: &sqlx::SqlitePool,
    key_block_id: &str,
    world_id: &str,
    block_type: &str,
    canonical_name: &str,
    status: &str,
) {
    seed_key_block(
        pool,
        key_block_id,
        world_id,
        block_type,
        canonical_name,
        status,
    )
    .await;
    sqlx::query("UPDATE kb_key_blocks SET source_work_id = 'we_source' WHERE key_block_id = ?")
        .bind(key_block_id)
        .execute(pool)
        .await
        .unwrap();
}

async fn fresh_state() -> (
    nexus_daemon_runtime::test_utils::TestTempRoot,
    WorkspaceState,
) {
    let (tmp, nexus_home, db_path, workspace_dir) =
        nexus_daemon_runtime::test_utils::create_initialized_test_workspace().await;
    let state = WorkspaceState::new_for_testing(
        nexus_home,
        db_path,
        Some(workspace_dir.to_string_lossy().to_string()),
    )
    .await;
    nexus_daemon_runtime::test_utils::seed_test_creator_and_world(state.pool().unwrap()).await;
    (tmp, state)
}

fn add_request(
    source: &str,
    target: &str,
    relation_type: WorldKbRelationshipKind,
) -> WorldKbPatchRelationshipRequest {
    WorldKbPatchRelationshipRequest {
        relationship_id: None,
        action: "add".parse().unwrap(),
        expected_version: Some(0),
        relationship: Some(NexusWorldKbRelationshipInput {
            source_entity_id: source.to_string(),
            target_entity_id: target.to_string(),
            relation_type: relation_type_to_nexus(relation_type),
            custom_label: None,
            symmetric: false,
            confidence: None,
            source_anchor_ids: Vec::new(),
            metadata: serde_json::Map::default(),
            needs_review: None,
        }),
    }
}

// V1.144 P2: add/update route through `orchestrate_relate` via
// `NexusAdapter`. The adapter port methods are natively `async fn`
// (spoke-operations 0.9.1 surface, V1.153 P0 T2); the multi-threaded flavor
// is retained from the pre-0.9.1 `block_in_place` bridge era (same
// rationale as the V1.143 patch_entity tests). Pre-orchestrator
// fast-fail tests (self-loop / bad-label / confidence / cross-world / bad
// anchor) short-circuit on handler guards and stay on the default runtime.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn add_relationship_returns_projected_row() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_a",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
    )
    .await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_b",
        "wld_test_world",
        "character",
        "Kael",
        "confirmed",
    )
    .await;

    let req = add_request("kb_a", "kb_b", WorldKbRelationshipKind::AlliedWith);
    let Json(resp) = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect("add should succeed");

    assert_eq!(
        resp.version, 1,
        "V1.144: spoke create seeds revision = 1 (not 0)"
    );
    let rel = resp.relationship.expect("response includes relationship");
    assert_eq!(rel.source_entity_id, "kb_a");
    assert_eq!(rel.target_entity_id, "kb_b");
    assert_eq!(rel.relation_type.to_string(), "allied_with");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_relationship_returns_bumped_version_and_projected_row() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_a",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
    )
    .await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_b",
        "wld_test_world",
        "character",
        "Kael",
        "confirmed",
    )
    .await;

    let Json(created) = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(add_request(
            "kb_a",
            "kb_b",
            WorldKbRelationshipKind::AlliedWith,
        )),
    )
    .await
    .unwrap();
    let rel_id = created.relationship.unwrap().relationship_id;

    let req = WorldKbPatchRelationshipRequest {
        relationship_id: Some(rel_id.clone()),
        action: "update".parse().unwrap(),
        // V1.144: add now seeds revision = 1 (spoke convention), so the CAS
        // base for the first update is 1 (was 0 pre-cutover).
        expected_version: Some(1),
        relationship: Some(NexusWorldKbRelationshipInput {
            source_entity_id: "kb_a".to_string(),
            target_entity_id: "kb_b".to_string(),
            relation_type: relation_type_to_nexus(WorldKbRelationshipKind::MentorOf),
            custom_label: None,
            symmetric: true,
            confidence: Some(0.75),
            source_anchor_ids: Vec::new(),
            metadata: serde_json::Map::default(),
            needs_review: None,
        }),
    };
    let Json(resp) = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect("update should succeed");

    assert_eq!(resp.version, 2, "V1.144: CAS bump 1 -> 2 (was 0 -> 1)");
    let rel = resp.relationship.unwrap();
    assert_eq!(rel.relationship_id, rel_id);
    assert_eq!(rel.relation_type.to_string(), "mentor_of");
    assert!(rel.symmetric);
    assert_eq!(rel.confidence.unwrap(), 0.75);
}

// ── V1.144 P2 T4: relate-cutover behavior-equivalence (post-state) ──────────
//
// These tests prove the `orchestrate_relate` cutover round-trips through
// SQLite correctly: the spoke-assigned revision is reflected on the create
// response, and a subsequent `get_graph` re-read confirms the row persisted
// with the right column mapping (mirrors the V1.143 `patch_entity` re-read
// discipline). They assert POST-STATE (HTTP body + graph projection), never
// the call-stack.

/// A.1 — add happy: `patch_relationship(add)` returns the spoke-assigned
/// revision (1), and a `get_graph` re-read confirms the relationship persisted
/// with the correct source/target/relation mapping + a `stored` projection.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn add_relationship_reread_via_get_graph_confirms_persisted_mapping() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_a",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
    )
    .await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_b",
        "wld_test_world",
        "character",
        "Kael",
        "confirmed",
    )
    .await;

    let Json(resp) = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(add_request(
            "kb_a",
            "kb_b",
            WorldKbRelationshipKind::AlliedWith,
        )),
    )
    .await
    .expect("add should succeed");
    assert_eq!(
        resp.version, 1,
        "spoke create seeds revision = 1 (orchestrator path)"
    );
    let created_id = resp.relationship.unwrap().relationship_id;

    // Re-read through the public graph projection — proves the orchestrator
    // INSERT landed and the column mapping survives the spoke round-trip.
    let Json(graph) = get_graph(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Query(GraphQuery {
            include_suggested: None,
        }),
    )
    .await
    .expect("graph should succeed");
    let stored = graph
        .relationships
        .iter()
        .find(|r| r.relationship_id == created_id && r.projection_direction.to_string() == "stored")
        .expect("persisted row visible in graph as a stored projection");
    assert_eq!(stored.source_entity_id, "kb_a");
    assert_eq!(stored.target_entity_id, "kb_b");
    assert_eq!(stored.relation_type.to_string(), "allied_with");
}

/// A.2 — update happy: `patch_relationship(update)` bumps the revision and a
/// `get_graph` re-read confirms the mutated fields (`relation_type`, symmetric,
/// confidence) persisted — proving the orchestrator CAS update writes through.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_then_reread_via_get_graph_confirms_data_persisted() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_a",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
    )
    .await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_b",
        "wld_test_world",
        "character",
        "Kael",
        "confirmed",
    )
    .await;

    let Json(created) = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(add_request(
            "kb_a",
            "kb_b",
            WorldKbRelationshipKind::AlliedWith,
        )),
    )
    .await
    .unwrap();
    let rel_id = created.relationship.unwrap().relationship_id;

    let req = WorldKbPatchRelationshipRequest {
        relationship_id: Some(rel_id.clone()),
        action: "update".parse().unwrap(),
        // add seeds revision = 1 (spoke convention) → CAS base for the first
        // update is 1.
        expected_version: Some(1),
        relationship: Some(NexusWorldKbRelationshipInput {
            source_entity_id: "kb_a".to_string(),
            target_entity_id: "kb_b".to_string(),
            relation_type: relation_type_to_nexus(WorldKbRelationshipKind::MentorOf),
            custom_label: None,
            symmetric: true,
            confidence: Some(0.8),
            source_anchor_ids: Vec::new(),
            metadata: serde_json::Map::default(),
            needs_review: None,
        }),
    };
    let Json(resp) = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect("update should succeed");
    assert_eq!(resp.version, 2, "CAS bump 1 -> 2");

    // Re-read: the mutated fields must be the persisted ones, not just the
    // response echo — proves the orchestrator update wrote through to storage.
    let Json(graph) = get_graph(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Query(GraphQuery {
            include_suggested: None,
        }),
    )
    .await
    .expect("graph should succeed");
    let stored = graph
        .relationships
        .iter()
        .find(|r| r.relationship_id == rel_id && r.projection_direction.to_string() == "stored")
        .expect("updated row visible in graph");
    assert_eq!(stored.relation_type.to_string(), "mentor_of");
    assert!(stored.symmetric, "symmetric=true persisted");
    assert_eq!(stored.confidence.unwrap(), 0.8, "confidence=0.8 persisted");
}

/// A.4 — create-on-existing (`RelationAlreadyExists` → 409) is structurally
/// unreachable from the HTTP `patch_relationship(add)` path: the handler
/// generates a fresh `relationship_id` via `generate_relationship_id()` for
/// every add (`world_kb.rs` `patch_relationship_add`) and never honors a
/// client-supplied id, so two adds can never collide on the PK. This was
/// equally true pre-cutover (the legacy path used the same id generator), so
/// there is no behavior-equivalence regression to test at the handler layer.
///
/// The invariant itself — "storage rejects a duplicate relation PK with
/// `RelationAlreadyExists`, which `map_relate_reject` maps to 409" — is proven
/// at the port layer against the real `SQLite` store by
/// `put_relation_create_on_existing_rejects_already_exists` in
/// `nexus-spoke-adapter/src/adapter/relation_port.rs` (real-server, not a
/// mock). That test drives `RelationPort::put_relation(.., None)` twice with
/// the same id and asserts the second rejects with
/// `SpokeRejectCode::RelationAlreadyExists`, the exact code this handler's
/// `map_relate_reject` routes to `world_kb_conflict` (409).

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remove_relationship_returns_null_projection() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_a",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
    )
    .await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_b",
        "wld_test_world",
        "character",
        "Kael",
        "confirmed",
    )
    .await;

    let Json(created) = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(add_request(
            "kb_a",
            "kb_b",
            WorldKbRelationshipKind::AlliedWith,
        )),
    )
    .await
    .unwrap();
    let rel_id = created.relationship.unwrap().relationship_id;

    let req = WorldKbPatchRelationshipRequest {
        relationship_id: Some(rel_id),
        action: "remove".parse().unwrap(),
        // V1.144: remove stays on Surface A (unchanged), but the row was
        // created via the orchestrator path which seeds revision = 1, so the
        // CAS base for remove is the created version.
        expected_version: Some(created.version),
        relationship: None,
    };
    let Json(resp) = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect("remove should succeed");

    assert!(resp.relationship.is_none());
}

#[tokio::test]
async fn add_self_loop_rejects_422() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_a",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
    )
    .await;

    let req = add_request("kb_a", "kb_a", WorldKbRelationshipKind::AlliedWith);
    let err = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("self-loop must 422");
    assert_eq!(
        err.status_code(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(err.error_code(), "world_kb_validation_failed");
}

#[tokio::test]
async fn add_custom_without_label_rejects_422() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_a",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
    )
    .await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_b",
        "wld_test_world",
        "character",
        "Kael",
        "confirmed",
    )
    .await;

    let req = add_request("kb_a", "kb_b", WorldKbRelationshipKind::Custom);
    let err = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("custom without label must 422");
    assert_eq!(
        err.status_code(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(err.error_code(), "world_kb_validation_failed");
}

#[tokio::test]
async fn add_confidence_out_of_range_rejects_422() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_a",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
    )
    .await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_b",
        "wld_test_world",
        "character",
        "Kael",
        "confirmed",
    )
    .await;

    let mut req = add_request("kb_a", "kb_b", WorldKbRelationshipKind::AlliedWith);
    req.relationship.as_mut().unwrap().confidence = Some(1.5);
    let err = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("out-of-range confidence must 422");
    assert_eq!(
        err.status_code(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(err.error_code(), "world_kb_validation_failed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_stale_version_returns_409() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_a",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
    )
    .await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_b",
        "wld_test_world",
        "character",
        "Kael",
        "confirmed",
    )
    .await;

    let Json(created) = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(add_request(
            "kb_a",
            "kb_b",
            WorldKbRelationshipKind::AlliedWith,
        )),
    )
    .await
    .unwrap();
    let rel_id = created.relationship.unwrap().relationship_id;

    let req = WorldKbPatchRelationshipRequest {
        relationship_id: Some(rel_id),
        action: "update".parse().unwrap(),
        expected_version: Some(99),
        relationship: Some(NexusWorldKbRelationshipInput {
            source_entity_id: "kb_a".to_string(),
            target_entity_id: "kb_b".to_string(),
            relation_type: relation_type_to_nexus(WorldKbRelationshipKind::MentorOf),
            custom_label: None,
            symmetric: true,
            confidence: Some(0.75),
            source_anchor_ids: Vec::new(),
            metadata: serde_json::Map::default(),
            needs_review: None,
        }),
    };
    let err = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("stale version must 409");
    assert_eq!(err.status_code(), axum::http::StatusCode::CONFLICT);
    assert_eq!(err.error_code(), "world_kb_conflict");
    let details = err.error_details().expect("conflict details");
    // V1.144: add seeds revision = 1 (spoke convention), so the stale-precheck
    // current_version the handler reports is 1 (was 0 pre-cutover).
    assert_eq!(details["current_version"], 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_graph_includes_symmetric_reverse_projection() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_a",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
    )
    .await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_b",
        "wld_test_world",
        "character",
        "Kael",
        "confirmed",
    )
    .await;

    let mut req = add_request("kb_a", "kb_b", WorldKbRelationshipKind::RivalOf);
    req.relationship.as_mut().unwrap().symmetric = true;
    let _ = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .unwrap();

    let Json(graph) = get_graph(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Query(GraphQuery {
            include_suggested: None,
        }),
    )
    .await
    .expect("graph should succeed");
    assert_eq!(
        graph.relationships.len(),
        2,
        "symmetric relationship emits forward + reverse"
    );
    let stored = graph
        .relationships
        .iter()
        .find(|r| r.projection_direction.to_string() == "stored")
        .expect("stored projection");
    let reverse = graph
        .relationships
        .iter()
        .find(|r| r.projection_direction.to_string() == "symmetric_reverse")
        .expect("reverse projection");
    assert_eq!(stored.relationship_id, reverse.relationship_id);
    assert_eq!(stored.source_entity_id, reverse.target_entity_id);
    assert_eq!(stored.target_entity_id, reverse.source_entity_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn add_with_valid_anchor_succeeds() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block_with_source(
        state.pool().unwrap(),
        "kb_a",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
    )
    .await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_b",
        "wld_test_world",
        "character",
        "Kael",
        "confirmed",
    )
    .await;
    seed_source_anchor(state.pool().unwrap(), "kb_a", 1).await;

    let mut req = add_request("kb_a", "kb_b", WorldKbRelationshipKind::AlliedWith);
    req.relationship.as_mut().unwrap().source_anchor_ids = vec!["sa_kb_a".to_string()];
    let Json(resp) = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect("add with valid anchor should succeed");
    let rel = resp.relationship.unwrap();
    assert_eq!(rel.source_anchor_ids, vec!["sa_kb_a"]);
}

#[tokio::test]
async fn add_with_invalid_anchor_rejects_422() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_a",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
    )
    .await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_b",
        "wld_test_world",
        "character",
        "Kael",
        "confirmed",
    )
    .await;

    let mut req = add_request("kb_a", "kb_b", WorldKbRelationshipKind::AlliedWith);
    req.relationship.as_mut().unwrap().source_anchor_ids = vec!["sa_missing".to_string()];
    let err = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("invalid anchor must 422");
    assert_eq!(
        err.status_code(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(err.error_code(), "world_kb_validation_failed");
}

#[tokio::test]
async fn add_cross_world_entity_rejects_422() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_a",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
    )
    .await;
    // kb_b exists in a different world — the handler should reject it.
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES ('other_creator', 'Other', 'active', datetime('now'), '{}')",
    )
    .execute(state.pool().unwrap())
    .await
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, created_at) \
         VALUES ('wld_other', 'ws', 'other_creator', 'Other', 'other-world', 'active', 'private', \
          'manual', '{}', datetime('now'))",
    )
    .execute(state.pool().unwrap())
    .await
    .unwrap();
    seed_key_block(
        state.pool().unwrap(),
        "kb_b",
        "wld_other",
        "character",
        "Kael",
        "confirmed",
    )
    .await;

    let req = add_request("kb_a", "kb_b", WorldKbRelationshipKind::AlliedWith);
    let err = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("cross-world entity must 422");
    assert_eq!(
        err.status_code(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(err.error_code(), "world_kb_validation_failed");
}

async fn seed_other_world(pool: &sqlx::SqlitePool, world_id: &str) {
    let row = sqlx::query!(
        "SELECT owner_creator_id, workspace_id FROM narrative_worlds WHERE world_id = ?",
        "wld_test_world"
    )
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query!(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))",
        world_id,
        row.workspace_id,
        row.owner_creator_id,
        "Other",
        "other-world",
        "active",
        "private",
        "manual",
        "{}"
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn relationship_in_other_world(state: &WorkspaceState, other_world_id: &str) -> String {
    seed_other_world(state.pool().unwrap(), other_world_id).await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_other_a",
        other_world_id,
        "character",
        "Other A",
        "confirmed",
    )
    .await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_other_b",
        other_world_id,
        "character",
        "Other B",
        "confirmed",
    )
    .await;
    let Json(created) = patch_relationship(
        State(state.clone()),
        Path(other_world_id.to_string()),
        Json(add_request(
            "kb_other_a",
            "kb_other_b",
            WorldKbRelationshipKind::AlliedWith,
        )),
    )
    .await
    .expect("relationship in other world should be created");
    created.relationship.unwrap().relationship_id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_cross_world_relationship_returns_403() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_a",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
    )
    .await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_b",
        "wld_test_world",
        "character",
        "Kael",
        "confirmed",
    )
    .await;
    let rel_id = relationship_in_other_world(&state, "wld_other").await;

    let req = WorldKbPatchRelationshipRequest {
        relationship_id: Some(rel_id),
        action: "update".parse().unwrap(),
        expected_version: Some(0),
        relationship: Some(NexusWorldKbRelationshipInput {
            source_entity_id: "kb_a".to_string(),
            target_entity_id: "kb_b".to_string(),
            relation_type: relation_type_to_nexus(WorldKbRelationshipKind::MentorOf),
            custom_label: None,
            symmetric: true,
            confidence: Some(0.75),
            source_anchor_ids: Vec::new(),
            metadata: serde_json::Map::default(),
            needs_review: None,
        }),
    };
    let err = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("cross-world update must 403");
    assert_eq!(err.status_code(), axum::http::StatusCode::FORBIDDEN);
    assert_eq!(err.error_code(), "forbidden");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remove_cross_world_relationship_returns_403() {
    let (_tmp, state) = fresh_state().await;
    let rel_id = relationship_in_other_world(&state, "wld_other_remove").await;

    let req = WorldKbPatchRelationshipRequest {
        relationship_id: Some(rel_id),
        action: "remove".parse().unwrap(),
        expected_version: Some(0),
        relationship: None,
    };
    let err = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("cross-world remove must 403");
    assert_eq!(err.status_code(), axum::http::StatusCode::FORBIDDEN);
    assert_eq!(err.error_code(), "forbidden");
}

// ── V1.76: needs_review gate + extraction suggestions ─────────────────────

/// Seed a `needs_review=1/source`='extraction' suggestion directly into the DB
/// (simulating what the extraction pipeline's upsert produces).
async fn seed_extraction_suggestion(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    source_id: &str,
    target_id: &str,
    relation_type: &str,
    confidence: Option<f64>,
) -> String {
    let rel_id = format!("rel_ext_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO kb_relationships \
         (relationship_id, world_id, source_entity_id, target_entity_id, relation_type, \
          symmetric, confidence, source_anchor_ids, metadata, created_at, updated_at, \
          revision, needs_review, source) \
         VALUES (?, ?, ?, ?, ?, 1, ?, '[]', '{\"source_quote\":\"q\"}', \
          datetime('now'), datetime('now'), 0, 1, 'extraction')",
    )
    .bind(&rel_id)
    .bind(world_id)
    .bind(source_id)
    .bind(target_id)
    .bind(relation_type)
    .bind(confidence)
    .execute(pool)
    .await
    .unwrap();
    rel_id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_graph_hides_needs_review_by_default() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_a",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
    )
    .await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_b",
        "wld_test_world",
        "character",
        "Kael",
        "confirmed",
    )
    .await;

    // One confirmed (manual) + one suggested (extraction) relationship.
    let _ = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(add_request(
            "kb_a",
            "kb_b",
            WorldKbRelationshipKind::AlliedWith,
        )),
    )
    .await
    .unwrap();
    seed_extraction_suggestion(
        state.pool().unwrap(),
        "wld_test_world",
        "kb_a",
        "kb_b",
        "rival_of",
        Some(0.8),
    )
    .await;

    // Default (include_suggested=None): only the confirmed relationship shows.
    let Json(graph) = get_graph(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Query(GraphQuery {
            include_suggested: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(
        graph.relationships.len(),
        1,
        "default excludes needs_review suggestions"
    );

    // include_suggested=true: both show.
    let Json(graph) = get_graph(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Query(GraphQuery {
            include_suggested: Some(true),
        }),
    )
    .await
    .unwrap();
    assert_eq!(
        graph.relationships.len(),
        3,
        "include_suggested surfaces suggestions (1 confirmed + 1 symmetric pair = 3)"
    );
    // The suggestion carries needs_review + source markers.
    let suggestion = graph
        .relationships
        .iter()
        .find(|r| r.needs_review)
        .expect("suggestion present with include_suggested");
    assert_eq!(suggestion.source.to_string(), "extraction");
    assert!(suggestion.needs_review);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn promote_suggestion_clears_needs_review() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_a",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
    )
    .await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_b",
        "wld_test_world",
        "character",
        "Kael",
        "confirmed",
    )
    .await;

    let rel_id = seed_extraction_suggestion(
        state.pool().unwrap(),
        "wld_test_world",
        "kb_a",
        "kb_b",
        "allied_with",
        Some(0.75),
    )
    .await;

    // Promote: update with needs_review=false clears the gate.
    let req = WorldKbPatchRelationshipRequest {
        relationship_id: Some(rel_id.clone()),
        action: "update".parse().unwrap(),
        expected_version: Some(0),
        relationship: Some(NexusWorldKbRelationshipInput {
            source_entity_id: "kb_a".to_string(),
            target_entity_id: "kb_b".to_string(),
            relation_type: relation_type_to_nexus(WorldKbRelationshipKind::AlliedWith),
            custom_label: None,
            symmetric: true,
            confidence: Some(0.75),
            source_anchor_ids: Vec::new(),
            metadata: serde_json::Map::default(),
            needs_review: Some(false),
        }),
    };
    let Json(resp) = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect("promote should succeed");
    let promoted = resp.relationship.unwrap();
    assert!(
        !promoted.needs_review,
        "promotion clears the needs_review gate"
    );
    assert_eq!(
        promoted.source.to_string(),
        "extraction",
        "source provenance preserved"
    );

    // After promotion the suggestion shows in the default graph.
    let Json(graph) = get_graph(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Query(GraphQuery {
            include_suggested: None,
        }),
    )
    .await
    .unwrap();
    assert!(
        graph
            .relationships
            .iter()
            .any(|r| r.relationship_id == rel_id),
        "promoted suggestion now visible in the default graph"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_preserves_needs_review_when_omitted() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_a",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
    )
    .await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_b",
        "wld_test_world",
        "character",
        "Kael",
        "confirmed",
    )
    .await;

    let rel_id = seed_extraction_suggestion(
        state.pool().unwrap(),
        "wld_test_world",
        "kb_a",
        "kb_b",
        "allied_with",
        None,
    )
    .await;

    // A routine edit that omits needs_review must NOT accidentally promote.
    let req = WorldKbPatchRelationshipRequest {
        relationship_id: Some(rel_id),
        action: "update".parse().unwrap(),
        expected_version: Some(0),
        relationship: Some(NexusWorldKbRelationshipInput {
            source_entity_id: "kb_a".to_string(),
            target_entity_id: "kb_b".to_string(),
            relation_type: relation_type_to_nexus(WorldKbRelationshipKind::MentorOf),
            custom_label: None,
            symmetric: true,
            confidence: Some(0.75),
            source_anchor_ids: Vec::new(),
            metadata: serde_json::Map::default(),
            needs_review: None,
        }),
    };
    let Json(resp) = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .unwrap();
    assert!(
        resp.relationship.unwrap().needs_review,
        "omitting needs_review preserves the suggestion gate"
    );
}

// ── V1.77: relationship graph cap ──────────────────────────────────────────

/// Seed `count` confirmed, non-symmetric relationships with strictly increasing
/// `updated_at` values so `ORDER BY updated_at DESC` is deterministic.
async fn seed_many_relationships(pool: &sqlx::SqlitePool, world_id: &str, count: usize) {
    for i in 0..count {
        let rel_id = format!("rel_cap_{i:04}");
        let updated = format!(
            "2026-06-30T{:02}:{:02}:{:02}.000Z",
            i / 3600,
            (i % 3600) / 60,
            i % 60
        );
        sqlx::query(
            "INSERT INTO kb_relationships \
             (relationship_id, world_id, source_entity_id, target_entity_id, relation_type, \
              symmetric, confidence, source_anchor_ids, metadata, created_at, updated_at, \
              revision, needs_review, source) \
             VALUES (?, ?, 'kb_a', 'kb_b', 'allied_with', 0, NULL, '[]', '{}', ?, ?, 0, 0, 'manual')",
        )
        .bind(&rel_id)
        .bind(world_id)
        .bind(&updated)
        .bind(&updated)
        .execute(pool)
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn get_graph_truncates_relationships_at_cap() {
    // GRAPH_RELATIONSHIP_CAP is 1000 in src/api/handlers/world_kb.rs.
    const CAP: usize = 1000;
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_a",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
    )
    .await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_b",
        "wld_test_world",
        "character",
        "Kael",
        "confirmed",
    )
    .await;

    seed_many_relationships(state.pool().unwrap(), "wld_test_world", CAP + 2).await;

    let Json(graph) = get_graph(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Query(GraphQuery {
            include_suggested: None,
        }),
    )
    .await
    .expect("graph should succeed");

    assert_eq!(
        graph.relationships.len(),
        CAP,
        "projects at most GRAPH_RELATIONSHIP_CAP stored rows"
    );

    let ids: std::collections::HashSet<_> = graph
        .relationships
        .iter()
        .map(|r| r.relationship_id.as_str())
        .collect();

    // The two oldest relationships (lowest updated_at) are silently dropped.
    assert!(!ids.contains("rel_cap_0000"));
    assert!(!ids.contains("rel_cap_0001"));
    // The newest relationships are retained.
    assert!(ids.contains("rel_cap_0002"));
    assert!(ids.contains(format!("rel_cap_{CAP:04}").as_str()));
}

// V1.146 P5 F-001: a routine update (e.g. label change) must not wipe unknown
// keys from the `extensions_nexus_json` column. This test seeds the column
// with an unknown key, updates via the handler, and verifies the key survives.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_preserves_unknown_extensions_nexus_keys() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_a",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
    )
    .await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_b",
        "wld_test_world",
        "character",
        "Kael",
        "confirmed",
    )
    .await;

    // Create via handler → valid row with revision = 1 (spoke convention).
    let Json(created) = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(add_request(
            "kb_a",
            "kb_b",
            WorldKbRelationshipKind::AlliedWith,
        )),
    )
    .await
    .unwrap();
    let rel_id = created.relationship.unwrap().relationship_id;

    // Inject unknown keys into the stored `extensions_nexus_json` column.
    sqlx::query("UPDATE kb_relationships SET extensions_nexus_json = ? WHERE relationship_id = ?")
        .bind(r#"{"world_id":"wld_test_world","custom_tag":"imported","batch_id":"B42"}"#)
        .bind(&rel_id)
        .execute(state.pool().unwrap())
        .await
        .unwrap();

    // Update: change only `relation_type`. Unknown keys must survive.
    let req = WorldKbPatchRelationshipRequest {
        relationship_id: Some(rel_id.clone()),
        action: "update".parse().unwrap(),
        expected_version: Some(1),
        relationship: Some(NexusWorldKbRelationshipInput {
            source_entity_id: "kb_a".to_string(),
            target_entity_id: "kb_b".to_string(),
            relation_type: relation_type_to_nexus(WorldKbRelationshipKind::MentorOf),
            custom_label: None,
            symmetric: false,
            confidence: None,
            source_anchor_ids: Vec::new(),
            metadata: serde_json::Map::default(),
            needs_review: None,
        }),
    };
    let Json(resp) = patch_relationship(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect("update should succeed");

    assert_eq!(resp.version, 2, "CAS bump after update");

    // Verify unknown keys survived the round-trip.
    let extensions_json: String = sqlx::query_scalar(
        "SELECT extensions_nexus_json FROM kb_relationships WHERE relationship_id = ?",
    )
    .bind(&rel_id)
    .fetch_one(state.pool().unwrap())
    .await
    .unwrap();

    let ns: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&extensions_json).expect("valid JSON");
    assert_eq!(
        ns.get("custom_tag").and_then(|v| v.as_str()),
        Some("imported"),
        "unknown key 'custom_tag' survives update"
    );
    assert_eq!(
        ns.get("batch_id").and_then(|v| v.as_str()),
        Some("B42"),
        "unknown key 'batch_id' survives update"
    );
}
