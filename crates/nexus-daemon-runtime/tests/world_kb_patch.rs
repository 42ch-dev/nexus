//! V1.73 P0 World KB patch-route integration tests.
//!
//! Exercises the four World KB Local API handlers directly against a
//! canonical daemon `WorkspaceState` with a seeded creator/world/KeyBlock:
//! - `patch_entity` happy path + per-row OCC 409 conflict + 422 validation.
//! - `promote_candidate` adopt + reject (entity-scope-model §5.5.2 state machine).
//! - `get_graph` + `get_candidates` read projections.
//!
//! Regression coverage: a stale `expected_version` must short-circuit as 409
//! BEFORE any write (per-row OCC catches stale writes from both canvas and
//! daemon-side writers).

use axum::extract::{Path, Query, State};
use axum::Json;
use nexus_contracts::{
    WorldKbEntityPatch, WorldKbPatchEntityRequest, WorldKbPromoteCandidateRequest,
};
use nexus_daemon_runtime::api::handlers::world_kb::{
    get_candidates, get_graph, patch_entity, promote_candidate, CandidatesQuery,
};
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_local_db::kb_extract_job::insert_pending;

/// Seed a `kb_key_blocks` row directly (bypassing store validation) with a
/// controlled `status` and `revision`, returning its id.
// 8 params mirrors the kb_key_blocks column layout — same rationale as
//  nexus_local_db::kb_extract_job::insert_pending.
#[allow(clippy::too_many_arguments)]
async fn seed_key_block(
    pool: &sqlx::SqlitePool,
    key_block_id: &str,
    world_id: &str,
    block_type: &str,
    canonical_name: &str,
    status: &str,
    revision: Option<i64>,
    body_json: Option<&str>,
) {
    // SAFETY: test-only seed against the known kb_key_blocks schema.
    sqlx::query(
        "INSERT INTO kb_key_blocks \
         (key_block_id, world_id, block_type, canonical_name, status, revision, body_json, \
          created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))",
    )
    .bind(key_block_id)
    .bind(world_id)
    .bind(block_type)
    .bind(canonical_name)
    .bind(status)
    .bind(revision)
    .bind(body_json)
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
    nexus_daemon_runtime::test_utils::seed_test_creator_and_world(state.pool()).await;
    (tmp, state)
}

// ─── patch-entity ───────────────────────────────────────────────────────────

#[tokio::test]
async fn patch_entity_title_bumps_version() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool(),
        "kb_hero",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
        None, // NULL revision — normalized to 0
        None,
    )
    .await;

    let req = WorldKbPatchEntityRequest {
        entity_id: "kb_hero".to_string(),
        expected_version: 0,
        patch: WorldKbEntityPatch {
            title: Some("Aria Stormwind".to_string()),
            body: None,
            aliases: None,
            block_type: None,
        },
        idempotency_key: None,
    };
    let Json(resp) = patch_entity(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect("patch should succeed");

    assert_eq!(resp.version, 1, "NULL revision should bump to 1");
    assert_eq!(resp.entity.canonical_name, "Aria Stormwind");
    assert_eq!(resp.entity.status, "confirmed");
}

#[tokio::test]
async fn patch_entity_stale_version_returns_409() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool(),
        "kb_hero",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
        Some(3), // current version is 3
        None,
    )
    .await;

    let req = WorldKbPatchEntityRequest {
        entity_id: "kb_hero".to_string(),
        expected_version: 2, // stale
        patch: WorldKbEntityPatch {
            title: Some("Aria v2".to_string()),
            body: None,
            aliases: None,
            block_type: None,
        },
        idempotency_key: None,
    };
    let err = patch_entity(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("stale version must 409");
    assert_eq!(err.status_code(), axum::http::StatusCode::CONFLICT);
    assert_eq!(err.error_code(), "world_kb_conflict");
    let details = err.error_details().expect("conflict details");
    assert_eq!(details["current_version"], 3);
    assert_eq!(details["entity_id"], "kb_hero");
}

#[tokio::test]
async fn patch_entity_deleted_entity_rejected_422() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool(),
        "kb_dead",
        "wld_test_world",
        "character",
        "Ghost",
        "deleted",
        Some(0),
        None,
    )
    .await;

    let req = WorldKbPatchEntityRequest {
        entity_id: "kb_dead".to_string(),
        expected_version: 0,
        patch: WorldKbEntityPatch {
            title: Some("Ghost Renamed".to_string()),
            body: None,
            aliases: None,
            block_type: None,
        },
        idempotency_key: None,
    };
    let err = patch_entity(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("deleted entity patch must 422");
    assert_eq!(
        err.status_code(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(err.error_code(), "world_kb_validation_failed");
}

#[tokio::test]
async fn patch_entity_cross_author_forbidden() {
    let (_tmp, state) = fresh_state().await;
    // World owned by a different creator (seed creator + world for FK).
    // SAFETY: test-only seed of a foreign-owned world + its owner creator.
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES ('other_creator', 'Other', 'active', datetime('now'), '{}')",
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, created_at) \
         VALUES ('wld_other', 'ws', 'other_creator', 'Other', 'other-world', 'active', 'private', \
          'manual', '{}', datetime('now'))",
    )
    .execute(state.pool())
    .await
    .unwrap();
    seed_key_block(
        state.pool(),
        "kb_other",
        "wld_other",
        "character",
        "Villain",
        "confirmed",
        Some(0),
        None,
    )
    .await;

    let req = WorldKbPatchEntityRequest {
        entity_id: "kb_other".to_string(),
        expected_version: 0,
        patch: WorldKbEntityPatch {
            title: Some("Villain v2".to_string()),
            body: None,
            aliases: None,
            block_type: None,
        },
        idempotency_key: None,
    };
    let err = patch_entity(
        State(state.clone()),
        Path("wld_other".to_string()),
        Json(req),
    )
    .await
    .expect_err("cross-author must 403");
    assert_eq!(err.status_code(), axum::http::StatusCode::FORBIDDEN);
}

// ─── promote-candidate ──────────────────────────────────────────────────────

const NOVEL_CHARACTER_BODY: &str =
    r#"{"summary":"A brave hero","attributes":{"novel_category":"character"}}"#;

#[tokio::test]
async fn promote_adopt_confirms_candidate() {
    let (_tmp, state) = fresh_state().await;
    let candidate = insert_pending(
        state.pool(),
        "test_creator",
        "ws",
        "wld_test_world",
        None,
        None,
        "character",
        "Kael",
        NOVEL_CHARACTER_BODY,
    )
    .await
    .unwrap();

    let req = WorldKbPromoteCandidateRequest {
        job_id: candidate.job_id.clone(),
        candidate_id: "kb_cand".to_string(),
        action: "adopt".to_string(),
        expected_version: u64::try_from(candidate.version).unwrap_or(0),
        merge_target_id: None,
        patch: None,
        idempotency_key: None,
    };
    let Json(resp) = promote_candidate(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect("adopt should succeed");

    let entity = resp.entity.expect("adopt returns a confirmed entity");
    assert_eq!(entity.status, "confirmed");
    assert_eq!(entity.canonical_name, "Kael");
    assert_eq!(resp.job.status, "confirmed");
}

#[tokio::test]
async fn promote_reject_dismisses_candidate() {
    let (_tmp, state) = fresh_state().await;
    let candidate = insert_pending(
        state.pool(),
        "test_creator",
        "ws",
        "wld_test_world",
        None,
        None,
        "character",
        "Rejecta",
        NOVEL_CHARACTER_BODY,
    )
    .await
    .unwrap();

    let req = WorldKbPromoteCandidateRequest {
        job_id: candidate.job_id.clone(),
        candidate_id: "kb_cand".to_string(),
        action: "reject".to_string(),
        expected_version: u64::try_from(candidate.version).unwrap_or(0),
        merge_target_id: None,
        patch: None,
        idempotency_key: None,
    };
    let Json(resp) = promote_candidate(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect("reject should succeed");

    assert!(resp.entity.is_none(), "reject returns no entity");
    assert_eq!(resp.job.status, "rejected");
}

#[tokio::test]
async fn promote_stale_version_returns_409() {
    let (_tmp, state) = fresh_state().await;
    let candidate = insert_pending(
        state.pool(),
        "test_creator",
        "ws",
        "wld_test_world",
        None,
        None,
        "character",
        "Stalea",
        NOVEL_CHARACTER_BODY,
    )
    .await
    .unwrap();

    let req = WorldKbPromoteCandidateRequest {
        job_id: candidate.job_id.clone(),
        candidate_id: "kb_cand".to_string(),
        action: "adopt".to_string(),
        expected_version: u64::try_from(candidate.version).unwrap_or(0) + 100, // stale
        merge_target_id: None,
        patch: None,
        idempotency_key: None,
    };
    let err = promote_candidate(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("stale promote must 409");
    assert_eq!(err.status_code(), axum::http::StatusCode::CONFLICT);
    assert_eq!(err.error_code(), "world_kb_conflict");
}

// ─── read endpoints ─────────────────────────────────────────────────────────

#[tokio::test]
async fn get_graph_returns_non_deleted_entities() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool(),
        "kb_one",
        "wld_test_world",
        "character",
        "Hero",
        "confirmed",
        Some(1),
        None,
    )
    .await;
    seed_key_block(
        state.pool(),
        "kb_two",
        "wld_test_world",
        "item",
        "Sword",
        "deleted",
        Some(0),
        None,
    )
    .await;

    let Json(resp) = get_graph(State(state.clone()), Path("wld_test_world".to_string()))
        .await
        .expect("graph should succeed");
    assert_eq!(resp.entities.len(), 1, "deleted entities are excluded");
    assert_eq!(resp.entities[0].key_block_id, "kb_one");
    assert!(
        resp.relationships.is_empty(),
        "relationships deferred to V1.74"
    );
}

#[tokio::test]
async fn get_candidates_returns_pending() {
    let (_tmp, state) = fresh_state().await;
    insert_pending(
        state.pool(),
        "test_creator",
        "ws",
        "wld_test_world",
        None,
        None,
        "character",
        "Cand One",
        NOVEL_CHARACTER_BODY,
    )
    .await
    .unwrap();

    let Json(resp) = get_candidates(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Query(CandidatesQuery {
            limit: None,
            cursor: None,
        }),
    )
    .await
    .expect("candidates should succeed");
    assert_eq!(resp.items.len(), 1);
    assert_eq!(resp.items[0].canonical_name, "Cand One");
    assert_eq!(
        resp.items[0].block_type,
        nexus_contracts::BlockType::Character
    );
}
