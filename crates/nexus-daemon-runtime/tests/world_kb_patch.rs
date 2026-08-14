//! V1.73 P0 World KB patch-route integration tests.
//!
//! Exercises the four World KB Daemon API handlers directly against a
//! canonical daemon `WorkspaceState` with a seeded creator/world/WorldKbEntry:
//! - `patch_entity` happy path + per-row OCC 409 conflict + 422 validation.
//! - `promote_candidate` adopt + reject (entity-scope-model §5.5.2 state machine).
//! - `get_graph` + `get_candidates` read projections.
//!
//! Regression coverage: a stale `expected_version` must short-circuit as 409
//! BEFORE any write (per-row OCC catches stale writes from both canvas and
//! daemon-side writers).

use axum::extract::{Path, Query, State};
use axum::Json;
use nexus_contracts::BlockType;
use nexus_contracts::{
    WorldKbKeyBlockStateResponse, WorldKbPatchEntityRequest, WorldKbPromoteCandidateRequest,
};
use nexus_daemon_runtime::api::handlers::world_kb::{
    get_candidates, get_graph, get_key_block_state, patch_entity, promote_candidate,
    CandidatesQuery, GraphQuery,
};
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_knowledge::world_kb::KbStore;
use nexus_local_db::kb_extract_job::insert_pending;
use nexus_local_db::kb_store::SqliteKbStore;

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

/// Like [`seed_key_block`] but sets `created_from_command_id` for promote
/// attribution tests.
// All params are distinct fixture dimensions; bundling them into a struct
// would obscure the per-argument seeding.
#[allow(clippy::too_many_arguments)]
async fn seed_key_block_attributed(
    pool: &sqlx::SqlitePool,
    key_block_id: &str,
    world_id: &str,
    block_type: &str,
    canonical_name: &str,
    status: &str,
    revision: Option<i64>,
    body_json: Option<&str>,
    created_from_command_id: &str,
) {
    // SAFETY: test-only seed against the known kb_key_blocks schema.
    sqlx::query(
        "INSERT INTO kb_key_blocks \
         (key_block_id, world_id, block_type, canonical_name, status, revision, body_json, \
          created_from_command_id, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))",
    )
    .bind(key_block_id)
    .bind(world_id)
    .bind(block_type)
    .bind(canonical_name)
    .bind(status)
    .bind(revision)
    .bind(body_json)
    .bind(created_from_command_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a `kb_extract_jobs` promotion-candidate row directly (bypassing the
/// `insert_pending` helper, which sets `work_entry_id = canonical_name_guess`
/// and so cannot produce two same-name rows). Lets the test model two distinct
/// extraction jobs that happen to guess the same canonical name (e.g. the same
/// character extracted from two different source works).
#[allow(clippy::too_many_arguments)]
async fn seed_pending_candidate(
    pool: &sqlx::SqlitePool,
    job_id: &str,
    work_entry_id: &str,
    world_id: &str,
    block_type_guess: &str,
    canonical_name_guess: &str,
) {
    // SAFETY: test-only seed against the known kb_extract_jobs schema.
    sqlx::query(
        "INSERT INTO kb_extract_jobs \
         (job_id, creator_id, workspace_id, work_entry_id, world_id, status, \
          promotion_status, proposed_payload, block_type_guess, canonical_name_guess, version) \
         VALUES (?, 'test_creator', 'ws', ?, ?, 'done', 'pending', ?, ?, ?, 0)",
    )
    .bind(job_id)
    .bind(work_entry_id)
    .bind(world_id)
    .bind(NOVEL_CHARACTER_BODY)
    .bind(block_type_guess)
    .bind(canonical_name_guess)
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

// ─── patch-entity ───────────────────────────────────────────────────────────

// V1.143 P1: patch_entity now routes the canonical edit through
// `orchestrate_upsert` via `NexusAdapter`. The adapter port methods are
// natively `async fn` (spoke-operations 0.9.1 surface, V1.153 P0 T2) and run
// on the test runtime; the multi-threaded flavor is retained from the
// pre-0.9.1 `block_in_place` bridge era (harmless either way — same
// rationale as the promote_adopt tests below). The fast-fail patch_entity
// tests (stale version / deleted /
// cross-author) short-circuit on pre-orchestrator guards and stay on the
// default current-thread runtime.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_entity_title_bumps_version() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
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
        patch: serde_json::from_value(serde_json::json!({"title": "Aria Stormwind"})).unwrap(),
    };
    let Json(resp) = patch_entity(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect("patch should succeed");

    assert_eq!(resp.version, 1, "NULL revision should bump to 1");
    assert_eq!(resp.entity.canonical_name.to_string(), "Aria Stormwind");
    assert_eq!(resp.entity.status, "confirmed");
}

/// V1.143 Phase5 (Greptile P1): `patch_entity` must preserve the full
/// `WorldKbBody` through the orchestrator upsert cutover. Spoke's
/// `BodyAttributeValue` only models string/number/bool; null/array/object
/// attribute values used to be silently dropped on the persist round-trip
/// (`build_spoke_upsert_request` → spoke seam → `put_update`). The conversion seam
/// now carries the full body losslessly via a reserved
/// `extensions.nexus._nexus_body` carrier, so a title-only patch on an entity
/// whose body carries null/array/object attributes preserves them exactly.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_entity_preserves_null_array_object_body_attributes() {
    let (_tmp, state) = fresh_state().await;
    // Seed an entity whose body carries a null, an array, and an object
    // attribute value — none of which fit spoke's BodyAttributeValue slot.
    let seeded_body = r#"{"attributes":{"weight":5,"named":null,"contents":["sword","potion"],"metadata":{"rarity":"common"}}}"#;
    seed_key_block(
        state.pool().unwrap(),
        "kb_backpack",
        "wld_test_world",
        "item",
        "Backpack",
        "confirmed",
        None,
        Some(seeded_body),
    )
    .await;

    // Title-only patch: the existing body is re-persisted through the
    // orchestrator (post_patch.body = kb.body), exercising the spoke seam.
    let req = WorldKbPatchEntityRequest {
        entity_id: "kb_backpack".to_string(),
        expected_version: 0,
        patch: serde_json::from_value(serde_json::json!({"title": "Hero Backpack"})).unwrap(),
    };
    let Json(resp) = patch_entity(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect("patch should succeed");
    assert_eq!(resp.version, 1);
    assert_eq!(resp.entity.canonical_name.to_string(), "Hero Backpack");

    // Every attribute value must survive the orchestrator round-trip —
    // number, null, array, AND object. (Pre-fix, only `weight` survived; the
    // rest were dropped by the spoke typed-body conversion.)
    let attrs = resp
        .entity
        .body
        .get("attributes")
        .expect("persisted body has attributes");
    assert_eq!(attrs["weight"].as_f64(), Some(5.0));
    assert_eq!(attrs["named"], serde_json::Value::Null);
    assert_eq!(attrs["contents"], serde_json::json!(["sword", "potion"]));
    assert_eq!(attrs["metadata"], serde_json::json!({"rarity": "common"}));
}

#[tokio::test]
async fn patch_entity_stale_version_returns_409() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
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
        patch: serde_json::from_value(serde_json::json!({"title": "Aria v2"})).unwrap(),
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
        state.pool().unwrap(),
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
        patch: serde_json::from_value(serde_json::json!({"title": "Ghost Renamed"})).unwrap(),
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
        patch: serde_json::from_value(serde_json::json!({"title": "Villain v2"})).unwrap(),
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

/// Regression for V1.73 greploop issue 3: `patch_entity` read the `WorldKbEntry` (and
/// ran the cross-world scope check) BEFORE `require_world_owner`. An
/// unauthenticated-but-locally-active creator could therefore distinguish
/// `NotFound` ("entity not in this world") from `Forbidden` ("not your world"),
/// leaking entity-existence signals across world boundaries.
///
/// Discriminating case: the active creator does NOT own the path world, and the
/// entity they quote exists in their OWN world (so `kb.world_id != path world`).
/// Under the buggy order this returned 404 `NotFound`; the fix runs
/// `require_world_owner` first (mirroring `promote_candidate` + the read
/// endpoints), so every cross-author request collapses to 403 regardless of
/// whether the entity exists in the path world.
#[tokio::test]
async fn patch_entity_cross_author_does_not_leak_existence() {
    let (_tmp, state) = fresh_state().await;

    // Foreign world owned by another creator.
    // SAFETY: test-only seed of a foreign-owned world + its owner creator.
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

    // An entity that exists in the ACTIVE creator's OWN world (not the foreign
    // path world). This is the row whose existence must NOT be revealed.
    seed_key_block(
        state.pool().unwrap(),
        "kb_mine",
        "wld_test_world",
        "character",
        "My Hero",
        "confirmed",
        Some(0),
        None,
    )
    .await;

    // Active creator (test_creator) does NOT own wld_other. Quoting an entity
    // that lives in their own world via the foreign world's path must collapse
    // to 403 Forbidden, NOT 404 NotFound.
    let req = WorldKbPatchEntityRequest {
        entity_id: "kb_mine".to_string(),
        expected_version: 0,
        patch: serde_json::from_value(serde_json::json!({"title": "Whatever"})).unwrap(),
    };
    let err = patch_entity(
        State(state.clone()),
        Path("wld_other".to_string()),
        Json(req),
    )
    .await
    .expect_err("cross-author must be forbidden before any entity read");
    assert_eq!(
        err.status_code(),
        axum::http::StatusCode::FORBIDDEN,
        "cross-author patch-entity must return 403, not leak existence via 404"
    );
}

/// V1.143 P1 T3: update happy path — prove the orchestrator's
/// `assert_revision_match` accepts a non-zero `expected_version` and the
/// revision chains (1 → 2), and that the SECOND edit's fields are reflected
/// in the post-state (not just the first bump from NULL/0 like
/// `patch_entity_title_bumps_version`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_entity_update_bumps_version_from_existing() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_veteran",
        "wld_test_world",
        "character",
        "Cael",
        "confirmed",
        Some(1), // already at revision 1 (a prior edit landed)
        None,
    )
    .await;

    let req = WorldKbPatchEntityRequest {
        entity_id: "kb_veteran".to_string(),
        expected_version: 1,
        patch: serde_json::from_value(serde_json::json!({"title": "Cael the Veteran"})).unwrap(),
    };
    let Json(resp) = patch_entity(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect("update should succeed");

    assert_eq!(resp.version, 2, "revision must chain 1 → 2");
    assert_eq!(resp.entity.canonical_name.to_string(), "Cael the Veteran");
    assert_eq!(resp.entity.status, "confirmed");
    assert_eq!(resp.entity.version, 2);
}

/// V1.143 P1 T3: create-on-existing — a client sends `expected_version = 0`
/// against an entity that already exists at revision 3 (as if it were
/// creating). The pre-orchestrator OCC guard short-circuits with 409, same as
/// any stale-version case. Distinct from `patch_entity_stale_version_returns_
/// 409` (which uses expected=2 vs stored=3): this documents the "client
/// thinks it's creating" confusion mode (expected=0 vs stored=3).
///
/// `wire_contracts_changed: false` — the 409 response shape matches the
/// pre-cutover behavior (status / `error_code` / details).
#[tokio::test]
async fn patch_entity_create_on_existing_returns_409() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_old",
        "wld_test_world",
        "character",
        "Elder",
        "confirmed",
        Some(3), // current version is 3
        None,
    )
    .await;

    let req = WorldKbPatchEntityRequest {
        entity_id: "kb_old".to_string(),
        expected_version: 0, // client mistakenly thinks this is new
        patch: serde_json::from_value(serde_json::json!({"title": "Elder v2"})).unwrap(),
    };
    let err = patch_entity(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("create-on-existing must 409");
    assert_eq!(err.status_code(), axum::http::StatusCode::CONFLICT);
    assert_eq!(err.error_code(), "world_kb_conflict");
    let details = err.error_details().expect("conflict details");
    assert_eq!(details["current_version"], 3);
    assert_eq!(details["entity_id"], "kb_old");
}

/// V1.160 P1 create-on-absent happy path (entity-scope-model §5.1.2): store
/// `NotFound` + `expected_version: 0` is the create convention (client-minted
/// `entity_id` + absent row + version 0). The handler must branch to the
/// create path — NOT the pre-V1.160 500 — build a fresh `KnowledgeEntry`,
/// route it through `orchestrate_upsert` → `put_create`, and return HTTP 200
/// (NOT 201) with `version = 1` and the row persisted in the store.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_entity_create_on_absent_happy_path() {
    let (_tmp, state) = fresh_state().await;
    // NO entity seeded — the store read must return NotFound for the create
    // convention to fire. `kb_<32-hex>` mirrors the client-minted id shape
    // the frontend era-create-dialog sends.
    let entity_id = "kb_9f8e7d6c5b4a39281726354453627180".to_string();
    let req = WorldKbPatchEntityRequest {
        entity_id: entity_id.clone(),
        expected_version: 0,
        patch: serde_json::from_value(serde_json::json!({
            "title": "New Era",
            "block_type": "era",
            "body": {
                "attributes": {
                    "era_type": "kingdom",
                    "world_summary": "The age of the three kingdoms.",
                }
            },
        }))
        .unwrap(),
    };
    let Json(resp) = patch_entity(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect("create-on-absent should succeed");

    assert_eq!(resp.version, 1, "create must land at revision 1");
    assert_eq!(resp.entity.key_block_id, entity_id);
    assert_eq!(resp.entity.canonical_name.to_string(), "New Era");
    assert_eq!(resp.entity.block_type.to_string(), "era");
    assert_eq!(resp.entity.status, "provisional");

    // The row must be persisted: same id, world, revision 1.
    let store = SqliteKbStore::new(state.pool().unwrap().clone());
    let stored = store
        .get_knowledge_entry(&entity_id)
        .await
        .expect("created row must exist in the store");
    assert_eq!(stored.entry_id, entity_id);
    assert_eq!(stored.world_id, "wld_test_world");
    assert_eq!(stored.revision, Some(1));
    assert_eq!(stored.status, "provisional");
}

/// V1.160 P1 update-on-absent (entity-scope-model §5.1.2): store `NotFound`
/// with `expected_version > 0` is client staleness — an update targeted at an
/// entity that does not exist must 409 `WorldKbConflictError`, never a silent
/// create. The absent row's revision is NULL-normalized to 0.
#[tokio::test]
async fn patch_entity_update_on_absent_returns_409() {
    let (_tmp, state) = fresh_state().await;
    let req = WorldKbPatchEntityRequest {
        entity_id: "kb_ghost".to_string(),
        expected_version: 3, // client believes the entity exists at rev 3
        patch: serde_json::from_value(serde_json::json!({"title": "Ghost"})).unwrap(),
    };
    let err = patch_entity(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("update-on-absent must 409");
    assert_eq!(err.status_code(), axum::http::StatusCode::CONFLICT);
    assert_eq!(err.error_code(), "world_kb_conflict");
    let details = err.error_details().expect("conflict details");
    assert_eq!(details["current_version"], 0);
    assert_eq!(details["entity_id"], "kb_ghost");
}

/// V1.160 P1 create required-field validation: create has no pre-read entity
/// to inherit a canonical name from, so a missing `patch.title` is a 422
/// `WorldKbValidationError` — the create arm must never reach the
/// orchestrator with an empty canonical name.
#[tokio::test]
async fn patch_entity_create_missing_title_rejected_422() {
    let (_tmp, state) = fresh_state().await;
    let req = WorldKbPatchEntityRequest {
        entity_id: "kb_no_title".to_string(),
        expected_version: 0,
        patch: serde_json::from_value(serde_json::json!({
            "block_type": "era",
            "body": { "attributes": { "era_type": "kingdom" } },
        }))
        .unwrap(),
    };
    let err = patch_entity(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("create without title must 422");
    assert_eq!(
        err.status_code(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(err.error_code(), "world_kb_validation_failed");
}

/// V1.160 P1 create required-field validation: a missing `patch.block_type`
/// is a 422 `WorldKbValidationError` (no pre-read entity to inherit the
/// block type from).
#[tokio::test]
async fn patch_entity_create_missing_block_type_rejected_422() {
    let (_tmp, state) = fresh_state().await;
    let req = WorldKbPatchEntityRequest {
        entity_id: "kb_no_type".to_string(),
        expected_version: 0,
        patch: serde_json::from_value(serde_json::json!({"title": "No Type"})).unwrap(),
    };
    let err = patch_entity(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("create without block_type must 422");
    assert_eq!(
        err.status_code(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(err.error_code(), "world_kb_validation_failed");
}

/// V1.160 P1 fix-wave (QC2-F001): a whitespace-only `title` (e.g. `"   "`)
/// passes the handler's `validate_canonical_name` (spaces are not rejected
/// there) and the schema `minLength: 1`, but the orchestrator rejects it with
/// `EmptyCanonicalName` — which previously fell through `map_upsert_reject`
/// to a 500. The create arm must reject whitespace-only titles as 422 BEFORE
/// building the spoke request.
#[tokio::test]
async fn patch_entity_create_whitespace_title_rejected_422() {
    let (_tmp, state) = fresh_state().await;
    let req = WorldKbPatchEntityRequest {
        entity_id: "kb_9f8e7d6c5b4a39281726354453627180".to_string(),
        expected_version: 0,
        patch: serde_json::from_value(serde_json::json!({
            "title": "   ",
            "block_type": "era",
        }))
        .unwrap(),
    };
    let err = patch_entity(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("create with whitespace-only title must 422");
    assert_eq!(
        err.status_code(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(err.error_code(), "world_kb_validation_failed");
}

/// V1.160 P1 fix-wave (QC2-F002 / QC3-S004): the `kb_key_blocks` CHECK
/// constraint only enforces the `kb_%` prefix, so an arbitrary `entity_id`
/// passes the handler and fails the INSERT with a CHECK error → 500. Spec
/// §5.1.2 makes `kb_<hex>` normative — the create arm must reject
/// non-conforming ids as 422 before building the spoke request. No row may be
/// written for any malformed id.
#[tokio::test]
async fn patch_entity_create_malformed_entity_id_rejected_422() {
    let (_tmp, state) = fresh_state().await;
    for malformed in [
        "entity_123",                           // no kb_ prefix
        "kb_",                                  // empty hex suffix
        "kb_zzz",                               // non-hex suffix
        "kb_9f8e7d6c5b4a39281726354453627180!", // trailing non-hex
    ] {
        let req = WorldKbPatchEntityRequest {
            entity_id: malformed.to_string(),
            expected_version: 0,
            patch: serde_json::from_value(serde_json::json!({
                "title": "Valid Title",
                "block_type": "era",
            }))
            .unwrap(),
        };
        let err = patch_entity(
            State(state.clone()),
            Path("wld_test_world".to_string()),
            Json(req),
        )
        .await
        .expect_err("malformed entity_id must 422");
        assert_eq!(
            err.status_code(),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "entity_id {malformed:?} must be 422"
        );
        assert_eq!(err.error_code(), "world_kb_validation_failed");
    }
}

/// V1.160 P1 authz-first regression (entity-scope-model §5.1.2): the create
/// arm runs only on store `NotFound` under an already-authz-checked PATH
/// `world_id`. A foreign (non-owned) world must 403 BEFORE any entity read —
/// even when the entity is absent (create intent), so no existence signal
/// leaks across world boundaries.
#[tokio::test]
async fn patch_entity_create_cross_author_forbidden() {
    let (_tmp, state) = fresh_state().await;
    // World owned by a different creator (seed creator + world for FK).
    // SAFETY: test-only seed of a foreign-owned world + its owner creator.
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

    // Create-shaped request (minted id, expected_version 0) against the
    // foreign world — must 403, not 404/422/200.
    let req = WorldKbPatchEntityRequest {
        entity_id: "kb_other_new".to_string(),
        expected_version: 0,
        patch: serde_json::from_value(serde_json::json!({
            "title": "New Thing",
            "block_type": "era",
        }))
        .unwrap(),
    };
    let err = patch_entity(
        State(state.clone()),
        Path("wld_other".to_string()),
        Json(req),
    )
    .await
    .expect_err("create in a foreign world must 403");
    assert_eq!(err.status_code(), axum::http::StatusCode::FORBIDDEN);
}

/// V1.160 P1 terminal Found ≠ absent (entity-scope-model §5.1.2, VC-2): a
/// soft-deleted row is `Ok(kb)` — Found, not `NotFound` — so a create-shaped
/// request (`expected_version: 0`) against a deleted id hits the existing
/// terminal-status guard (422), NEVER the create arm. No insert may occur.
#[tokio::test]
async fn patch_entity_create_on_deleted_entity_rejected_422() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
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
        expected_version: 0, // create-shaped — but the id is Found (deleted)
        patch: serde_json::from_value(serde_json::json!({
            "title": "Ghost Reborn",
            "block_type": "character",
        }))
        .unwrap(),
    };
    let err = patch_entity(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("deleted entity patch must 422 (terminal, not create)");
    assert_eq!(
        err.status_code(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(err.error_code(), "world_kb_validation_failed");

    // The terminal guard fired before any write: the row is still deleted at
    // the seeded revision, and no create insert happened.
    let store = SqliteKbStore::new(state.pool().unwrap().clone());
    let stored = store
        .get_knowledge_entry("kb_dead")
        .await
        .expect("seeded row still present");
    assert_eq!(stored.status, "deleted");
    assert_eq!(stored.revision, Some(0));
    assert_eq!(stored.canonical_name, "Ghost");
}

/// V1.160 P1 era create round-trip (entity-scope-model §5.1.2): the
/// era-create-dialog payload (`body.attributes.era_type` + `world_summary`)
/// must survive the orchestrator persist + response projection exactly.
/// `era` is cross-profile (no `novel_category` enforcement), so the body
/// validates under `ValidationMode::Novel`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_entity_create_with_era_block_type() {
    let (_tmp, state) = fresh_state().await;
    let entity_id = "kb_5a4b3c2d1e0f9a8b7c6d5e4f3a2b1c0d".to_string();
    let req = WorldKbPatchEntityRequest {
        entity_id: entity_id.clone(),
        expected_version: 0,
        patch: serde_json::from_value(serde_json::json!({
            "title": "The Long Peace",
            "block_type": "era",
            "body": {
                "attributes": {
                    "era_type": "kingdom",
                    "world_summary": "A century without war.",
                }
            },
        }))
        .unwrap(),
    };
    let Json(resp) = patch_entity(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect("era create should succeed");
    assert_eq!(resp.version, 1);
    assert_eq!(resp.entity.block_type.to_string(), "era");

    // The era body attributes must round-trip through the orchestrator.
    let attrs = resp
        .entity
        .body
        .get("attributes")
        .expect("persisted body has attributes");
    assert_eq!(attrs["era_type"], serde_json::json!("kingdom"));
    assert_eq!(
        attrs["world_summary"],
        serde_json::json!("A century without war.")
    );

    // Store-side verification: body attributes persisted verbatim.
    let store = SqliteKbStore::new(state.pool().unwrap().clone());
    let stored = store
        .get_knowledge_entry(&entity_id)
        .await
        .expect("created era row exists");
    let stored_attrs = stored
        .body
        .as_ref()
        .expect("stored body present")
        .attributes
        .as_ref()
        .expect("stored body has attributes");
    assert_eq!(stored_attrs["era_type"], serde_json::json!("kingdom"));
    assert_eq!(
        stored_attrs["world_summary"],
        serde_json::json!("A century without war.")
    );
}

/// V1.143 P1 T3 / C1 regression (reviewer-requested): the orchestrator's
/// `validate_update_path` treats `status = "merged"` as terminal (alongside
/// `"deleted"`), whereas the pre-cutover `patch_entity` only rejected
/// `"deleted"` and left `"merged"` editable (the old comment read: "'merged'
/// entities remain editable to allow post-merge cleanup"). The cutover
/// tightens this — a merged entity is now rejected by the orchestrator path.
///
/// ## Latency honesty note
///
/// This difference is **latent in production today**: no daemon write path
/// hardcodes `kb_key_blocks.status = 'merged'`. The only status-writers are
/// `delete_knowledge_entry` (→ `"deleted"` only), the generic
/// `put_knowledge_entry` / `update_key_block_auxiliary_fields_in_tx` (which
/// echo whatever status the caller passes), and the parameterized
/// `kb_key_block` capability in `nexus-orchestration` (which accepts any
/// string from an agent). So `status = "merged"` can only reach storage via
/// an agent-driven capability call or a direct spoke upsert — not via the
/// daemon's own canvas-facing handlers. The domain model DOES treat `merged`
/// as a real lifecycle state (read-filters in `kb_store::list_by_world` and
/// `extract_sync` exclude it alongside `deleted`/`deprecated`; the in-memory
/// `KnowledgeEntry` transition table has `merged` as a terminal state), so
/// the behavioral difference is real even if production traffic has not yet
/// exercised it.
///
/// This test seeds `status = "merged"` directly (bypassing production paths,
/// since none produce it) and confirms the orchestrator rejects the patch
/// with 422 — documenting the new, tighter behavior.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_entity_merged_status_rejected_by_orchestrator() {
    let (_tmp, state) = fresh_state().await;
    seed_key_block(
        state.pool().unwrap(),
        "kb_merged",
        "wld_test_world",
        "character",
        "Absorbed",
        "merged", // terminal in spoke; editable pre-cutover, rejected now
        Some(0),
        None,
    )
    .await;

    let req = WorldKbPatchEntityRequest {
        entity_id: "kb_merged".to_string(),
        expected_version: 0, // matches stored revision — passes OCC
        patch: serde_json::from_value(serde_json::json!({"title": "Absorbed Rename"})).unwrap(),
    };
    // The pre-orchestrator guard (`if kb.status == "deleted"`) does NOT fire
    // (status is "merged", not "deleted"), so the request reaches
    // `orchestrate_upsert`. The orchestrator's `validate_update_path` then
    // rejects with `KnowledgeEntryTerminalStatus`, which maps to 422
    // `world_kb_validation_failed`.
    let err = patch_entity(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("merged entity must be rejected as terminal by the orchestrator");
    assert_eq!(
        err.status_code(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(err.error_code(), "world_kb_validation_failed");
}

// ─── promote-candidate ──────────────────────────────────────────────────────

const NOVEL_CHARACTER_BODY: &str =
    r#"{"summary":"A brave hero","attributes":{"novel_category":"character"}}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn promote_adopt_confirms_candidate() {
    // V1.142 P2: promote_adopt routes through `orchestrate_promote` via
    // `NexusAdapter`. The adapter port methods are natively `async fn`
    // (spoke-operations 0.9.1 surface, V1.153 P0 T2); the multi-threaded
    // flavor is retained from the pre-0.9.1 `block_in_place` bridge era.
    let (_tmp, state) = fresh_state().await;
    let candidate = insert_pending(
        state.pool().unwrap(),
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
        action: "adopt".parse().unwrap(),
        expected_version: u64::try_from(candidate.version).unwrap_or(0),
        merge_target_id: None,
        patch: None,
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
    assert_eq!(entity.canonical_name.to_string(), "Kael");
    assert_eq!(resp.job.status, "confirmed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn promote_adopt_rollbacks_entry_when_job_flip_races() {
    let (_tmp, state) = fresh_state().await;
    let pool = state.pool().unwrap().clone();
    let candidate = insert_pending(
        &pool,
        "test_creator",
        "ws",
        "wld_test_world",
        None,
        None,
        "character",
        "CompensateMe",
        NOVEL_CHARACTER_BODY,
    )
    .await
    .unwrap();

    // Deterministic flip failure: when orchestrate_promote INSERTs the confirmed
    // row, reject the pending job before mark_confirmed runs.
    let job_id = candidate.job_id.clone();
    let trigger_sql = format!(
        "CREATE TRIGGER trg_reject_on_compensate_insert \
         AFTER INSERT ON kb_key_blocks \
         WHEN NEW.canonical_name = 'CompensateMe' \
         BEGIN \
           UPDATE kb_extract_jobs \
           SET promotion_status = 'rejected', version = version + 1 \
           WHERE job_id = '{job_id}' AND promotion_status = 'pending'; \
         END"
    );
    sqlx::query(&trigger_sql).execute(&pool).await.unwrap();

    let req = WorldKbPromoteCandidateRequest {
        job_id: candidate.job_id.clone(),
        candidate_id: "kb_cand".to_string(),
        action: "adopt".parse().unwrap(),
        expected_version: u64::try_from(candidate.version).unwrap_or(0),
        merge_target_id: None,
        patch: None,
    };
    let err = promote_candidate(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("flip failure must surface validation error");
    sqlx::query("DROP TRIGGER IF EXISTS trg_reject_on_compensate_insert")
        .execute(&pool)
        .await
        .ok();

    assert_eq!(
        err.status_code(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        "flip failure surfaces as validation_failed"
    );

    let store = SqliteKbStore::new(pool.clone());
    let active = store
        .get_active_by_unique_key("wld_test_world", "CompensateMe", BlockType::Character)
        .await
        .unwrap();
    assert!(
        active.is_none(),
        "atomic rollback must leave no active entry when job flip races"
    );

    seed_pending_candidate(
        &pool,
        "xj_compensate_retry",
        "work_retry_source",
        "wld_test_world",
        "character",
        "CompensateMe",
    )
    .await;
    let req2 = WorldKbPromoteCandidateRequest {
        job_id: "xj_compensate_retry".to_string(),
        candidate_id: "kb_cand2".to_string(),
        action: "adopt".parse().unwrap(),
        expected_version: 0,
        merge_target_id: None,
        patch: None,
    };
    let Json(resp2) =
        promote_candidate(State(state), Path("wld_test_world".to_string()), Json(req2))
            .await
            .expect("retry adopt after rollback must succeed");
    assert_eq!(resp2.job.status, "confirmed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn promote_adopt_rollbacks_entry_when_job_flip_cas_errors() {
    let (_tmp, state) = fresh_state().await;
    let pool = state.pool().unwrap().clone();
    let candidate = insert_pending(
        &pool,
        "test_creator",
        "ws",
        "wld_test_world",
        None,
        None,
        "character",
        "CasFailMe",
        NOVEL_CHARACTER_BODY,
    )
    .await
    .unwrap();

    let job_id = candidate.job_id.clone();
    let trigger_sql = format!(
        "CREATE TRIGGER trg_abort_flip_cas \
         BEFORE UPDATE ON kb_extract_jobs \
         WHEN OLD.job_id = '{job_id}' AND NEW.promotion_status = 'confirmed' \
         BEGIN \
           SELECT RAISE(ABORT, 'simulated flip CAS failure'); \
         END"
    );
    sqlx::query(&trigger_sql).execute(&pool).await.unwrap();

    let req = WorldKbPromoteCandidateRequest {
        job_id: candidate.job_id.clone(),
        candidate_id: "kb_cand".to_string(),
        action: "adopt".parse().unwrap(),
        expected_version: u64::try_from(candidate.version).unwrap_or(0),
        merge_target_id: None,
        patch: None,
    };
    let err = promote_candidate(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(req),
    )
    .await
    .expect_err("CAS error during flip must fail");
    sqlx::query("DROP TRIGGER IF EXISTS trg_abort_flip_cas")
        .execute(&pool)
        .await
        .ok();

    assert_eq!(
        err.status_code(),
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        "CAS execute error during flip surfaces as internal after rollback"
    );

    let store = SqliteKbStore::new(pool.clone());
    let active = store
        .get_active_by_unique_key("wld_test_world", "CasFailMe", BlockType::Character)
        .await
        .unwrap();
    assert!(
        active.is_none(),
        "CAS-error path must roll back the in-flight entry"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn promote_adopt_pending_collision_does_not_delete_attributed_entry() {
    let (_tmp, state) = fresh_state().await;
    let pool = state.pool().unwrap().clone();

    // Simulated in-flight / stale partial state: active entry stamped with
    // this job_id while the job is still pending — retry must not delete.
    seed_key_block_attributed(
        &pool,
        "kb_orphan_prior",
        "wld_test_world",
        "character",
        "OrphanRetry",
        "confirmed",
        Some(1),
        Some(NOVEL_CHARACTER_BODY),
        "xj_orphan_retry",
    )
    .await;
    seed_pending_candidate(
        &pool,
        "xj_orphan_retry",
        "work_orphan_source",
        "wld_test_world",
        "character",
        "OrphanRetry",
    )
    .await;

    let err = promote_candidate(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(WorldKbPromoteCandidateRequest {
            job_id: "xj_orphan_retry".to_string(),
            candidate_id: "kb_cand".to_string(),
            action: "adopt".parse().unwrap(),
            expected_version: 0,
            merge_target_id: None,
            patch: None,
        }),
    )
    .await
    .expect_err("pending collision must not delete");
    assert_eq!(
        err.status_code(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    let details = err.error_details().expect("validation details");
    let errors = details["validation_summary"]["errors"]
        .as_array()
        .expect("errors array");
    assert!(
        errors
            .iter()
            .any(|e| { e.as_str().is_some_and(|msg| msg.contains("still pending")) }),
        "must surface pending collision: {details:?}"
    );

    let store = SqliteKbStore::new(pool);
    let active = store
        .get_active_by_unique_key("wld_test_world", "OrphanRetry", BlockType::Character)
        .await
        .unwrap()
        .expect("entry must remain active — retry path never deletes");
    assert_eq!(active.entry_id, "kb_orphan_prior");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn promote_adopt_retry_does_not_delete_unattributed_collision() {
    let (_tmp, state) = fresh_state().await;
    let pool = state.pool().unwrap().clone();

    // Independent pre-existing entry (no promotion job stamp).
    seed_key_block(
        &pool,
        "kb_preexisting",
        "wld_test_world",
        "character",
        "PreExisting",
        "confirmed",
        Some(1),
        Some(NOVEL_CHARACTER_BODY),
    )
    .await;
    seed_pending_candidate(
        &pool,
        "xj_unattributed_collision",
        "work_collision_source",
        "wld_test_world",
        "character",
        "PreExisting",
    )
    .await;

    let err = promote_candidate(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(WorldKbPromoteCandidateRequest {
            job_id: "xj_unattributed_collision".to_string(),
            candidate_id: "kb_cand".to_string(),
            action: "adopt".parse().unwrap(),
            expected_version: 0,
            merge_target_id: None,
            patch: None,
        }),
    )
    .await
    .expect_err("unattributed collision must not auto-delete");
    assert_eq!(
        err.status_code(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    let details = err.error_details().expect("validation details");
    let errors = details["validation_summary"]["errors"]
        .as_array()
        .expect("errors array");
    assert!(
        errors
            .iter()
            .any(|e| { e.as_str().is_some_and(|msg| msg.contains("still pending")) }),
        "must surface pending collision without delete: {details:?}"
    );
    assert!(
        !errors.iter().any(|e| {
            e.as_str()
                .is_some_and(|msg| msg.contains("automatically removed"))
        }),
        "retry path must not auto-delete: {details:?}"
    );

    let store = SqliteKbStore::new(pool);
    let active = store
        .get_active_by_unique_key("wld_test_world", "PreExisting", BlockType::Character)
        .await
        .unwrap()
        .expect("pre-existing entry must remain active");
    assert_eq!(active.entry_id, "kb_preexisting");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn promote_adopt_confirmed_job_does_not_recover_unattributed_collision() {
    let (_tmp, state) = fresh_state().await;
    let pool = state.pool().unwrap().clone();
    let job_id = "xj_confirmed_unattrib";

    // Independent pre-existing entry (no promotion job stamp).
    seed_key_block(
        &pool,
        "kb_preexisting_confirmed",
        "wld_test_world",
        "character",
        "ConfirmedCollision",
        "confirmed",
        Some(1),
        Some(NOVEL_CHARACTER_BODY),
    )
    .await;
    seed_pending_candidate(
        &pool,
        job_id,
        "work_confirmed_collision",
        "wld_test_world",
        "character",
        "ConfirmedCollision",
    )
    .await;

    // Simulate a concurrent flip confirming the job while this adopt attempt
    // hits the unique-key collision (outer gate still sees pending).
    let trigger_sql = format!(
        "CREATE TRIGGER trg_confirm_on_collision_insert \
         BEFORE INSERT ON kb_key_blocks \
         WHEN NEW.canonical_name = 'ConfirmedCollision' \
         BEGIN \
           UPDATE kb_extract_jobs \
           SET promotion_status = 'confirmed', version = version + 1 \
           WHERE job_id = '{job_id}' AND promotion_status = 'pending'; \
         END"
    );
    sqlx::query(&trigger_sql).execute(&pool).await.unwrap();

    let err = promote_candidate(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(WorldKbPromoteCandidateRequest {
            job_id: job_id.to_string(),
            candidate_id: "kb_cand".to_string(),
            action: "adopt".parse().unwrap(),
            expected_version: 0,
            merge_target_id: None,
            patch: None,
        }),
    )
    .await
    .expect_err("confirmed job must not adopt unrelated active entry");
    sqlx::query("DROP TRIGGER IF EXISTS trg_confirm_on_collision_insert")
        .execute(&pool)
        .await
        .ok();
    assert_eq!(
        err.status_code(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    let details = err.error_details().expect("validation details");
    let errors = details["validation_summary"]["errors"]
        .as_array()
        .expect("errors array");
    assert!(
        errors.iter().any(|e| {
            e.as_str()
                .is_some_and(|msg| msg.contains("already exists in this world"))
        }),
        "must surface collision without adopting unrelated entry: {details:?}"
    );

    let store = SqliteKbStore::new(pool);
    let active = store
        .get_active_by_unique_key("wld_test_world", "ConfirmedCollision", BlockType::Character)
        .await
        .unwrap()
        .expect("pre-existing entry must remain active");
    assert_eq!(active.entry_id, "kb_preexisting_confirmed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn promote_adopt_retry_recovers_when_job_confirmed_with_attributed_entry() {
    let (_tmp, state) = fresh_state().await;
    let pool = state.pool().unwrap().clone();
    let job_id = "xj_retry_recover_attributed";

    // Durable success from a prior attempt: confirmed job + attributed active entry.
    seed_key_block_attributed(
        &pool,
        "kb_retry_recover",
        "wld_test_world",
        "character",
        "RetryRecover",
        "confirmed",
        Some(1),
        Some(NOVEL_CHARACTER_BODY),
        job_id,
    )
    .await;
    seed_pending_candidate(
        &pool,
        job_id,
        "work_retry_recover",
        "wld_test_world",
        "character",
        "RetryRecover",
    )
    .await;
    sqlx::query(
        "UPDATE kb_extract_jobs \
         SET promotion_status = 'confirmed', version = 1 \
         WHERE job_id = ?",
    )
    .bind(job_id)
    .execute(&pool)
    .await
    .unwrap();

    let resp = promote_candidate(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(WorldKbPromoteCandidateRequest {
            job_id: job_id.to_string(),
            candidate_id: "kb_cand".to_string(),
            action: "adopt".parse().unwrap(),
            expected_version: 0,
            merge_target_id: None,
            patch: None,
        }),
    )
    .await
    .expect("attributed confirmed-job retry must return 200 (F-001 / QC2)");

    let entity = resp.entity.as_ref().expect("adopt entity");
    assert_eq!(entity.canonical_name.to_string(), "RetryRecover");
    assert_eq!(entity.status, "confirmed");
    assert_eq!(entity.key_block_id, "kb_retry_recover");
    assert_eq!(resp.job.status, "confirmed");
    assert_eq!(resp.version, 1);

    let store = SqliteKbStore::new(pool);
    let active = store
        .get_active_by_unique_key("wld_test_world", "RetryRecover", BlockType::Character)
        .await
        .unwrap()
        .expect("single active entry");
    assert_eq!(active.created_from_command_id.as_deref(), Some(job_id));
}

#[tokio::test]
async fn promote_reject_dismisses_candidate() {
    let (_tmp, state) = fresh_state().await;
    let candidate = insert_pending(
        state.pool().unwrap(),
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
        action: "reject".parse().unwrap(),
        expected_version: u64::try_from(candidate.version).unwrap_or(0),
        merge_target_id: None,
        patch: None,
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
        state.pool().unwrap(),
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
        action: "adopt".parse().unwrap(),
        expected_version: u64::try_from(candidate.version).unwrap_or(0) + 100, // stale
        merge_target_id: None,
        patch: None,
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
        state.pool().unwrap(),
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
        state.pool().unwrap(),
        "kb_two",
        "wld_test_world",
        "item",
        "Sword",
        "deleted",
        Some(0),
        None,
    )
    .await;

    let Json(resp) = get_graph(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Query(GraphQuery {
            include_suggested: None,
        }),
    )
    .await
    .expect("graph should succeed");
    assert_eq!(resp.entities.len(), 1, "deleted entities are excluded");
    assert_eq!(resp.entities[0].key_block_id, "kb_one");
    assert!(
        resp.relationships.is_empty(),
        "relationships deferred to V1.74"
    );
}

/// V1.164 P3 T1 (AR-2): `WorldKbEntityProjection.modules` carries the
/// functional-dialect modules verbatim from `kb_key_blocks.modules_json`
/// (entity seeded with `modules.mental`), and stays absent (empty map on the
/// Rust side, omitted from the wire via `skip_serializing_if`) for entities
/// without modules data.
#[tokio::test]
async fn get_graph_projects_modules_from_modules_json() {
    let (_tmp, state) = fresh_state().await;
    let pool = state.pool().unwrap();
    // SAFETY: test-only seed against the known kb_key_blocks schema
    // (20260525_kb_key_blocks.sql + 20260731120000_modules_json.sql).
    sqlx::query(
        "INSERT INTO kb_key_blocks \
            (key_block_id, world_id, block_type, canonical_name, status, revision, \
             modules_json, created_at, updated_at) \
           VALUES (?, ?, 'character', 'Harbor Master', 'confirmed', 1, ?, \
             datetime('now'), datetime('now'))",
    )
    .bind("kb_mental")
    .bind("wld_test_world")
    .bind(
        r#"{"mental":{"beliefs":{"ref":"kb_harbor_beliefs","count":2},"goals":["rule the harbor"],"emotions":{"fear":"storms"}}}"#,
    )
    .execute(pool)
    .await
    .unwrap();
    // Entity without modules data — modules must project as absent.
    sqlx::query(
        "INSERT INTO kb_key_blocks \
            (key_block_id, world_id, block_type, canonical_name, status, revision, \
             created_at, updated_at) \
           VALUES (?, ?, 'item', 'Plain Anchor', 'confirmed', 0, \
             datetime('now'), datetime('now'))",
    )
    .bind("kb_plain")
    .bind("wld_test_world")
    .execute(pool)
    .await
    .unwrap();

    let Json(resp) = get_graph(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Query(GraphQuery {
            include_suggested: None,
        }),
    )
    .await
    .expect("graph should succeed");
    assert_eq!(resp.entities.len(), 2);

    let mental = resp
        .entities
        .iter()
        .find(|e| e.key_block_id == "kb_mental")
        .expect("mental entity present");
    assert_eq!(
        mental.modules.get("mental").and_then(|m| m.get("goals")),
        Some(&serde_json::json!(["rule the harbor"])),
        "modules.mental.goals carried verbatim"
    );
    assert_eq!(
        mental.modules.get("mental").and_then(|m| m.get("emotions")),
        Some(&serde_json::json!({"fear": "storms"})),
        "modules.mental.emotions carried verbatim"
    );

    let plain = resp
        .entities
        .iter()
        .find(|e| e.key_block_id == "kb_plain")
        .expect("plain entity present");
    assert!(
        plain.modules.is_empty(),
        "no modules data → empty modules map (omitted from wire)"
    );
    // Struct-side emptiness is indistinguishable from `{}`; pin the actual
    // wire behavior — `skip_serializing_if` must omit the `modules` KEY.
    let plain_wire = serde_json::to_value(plain).expect("plain entity serializes");
    assert!(
        plain_wire.get("modules").is_none(),
        "no modules data → 'modules' key must be absent from serialized JSON: {plain_wire}"
    );
}

#[tokio::test]
async fn get_candidates_returns_pending() {
    let (_tmp, state) = fresh_state().await;
    insert_pending(
        state.pool().unwrap(),
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
    assert_eq!(resp.items[0].block_type, "character".parse().unwrap());
}

/// Regression for V1.73 qc3 W-01: cursor pagination must reach every pending
/// candidate, not just the first `limit + 1` window. Seeds 4 candidates,
/// walks the list with `limit = 2`, and asserts all 4 are returned exactly
/// once across the two pages (no loss, no duplication). The expected order is
/// derived from the seeded rows using the same `(created_at, job_id)`
/// comparator the storage query uses, so the assertion holds whether or not
/// the inserts land in the same `datetime('now')` second.
// Long integration test; splitting would obscure the end-to-end scenario.
#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn get_candidates_multi_page_cursor_reaches_all_rows() {
    let (_tmp, state) = fresh_state().await;

    // Seed 4 pending candidates; collect the returned rows so we can derive
    // the expected keyset order independently of the handler.
    let mut seeded: Vec<nexus_local_db::kb_extract_job::KbExtractPromotion> = Vec::new();
    for idx in 0..4u8 {
        let row = insert_pending(
            state.pool().unwrap(),
            "test_creator",
            "ws",
            "wld_test_world",
            None,
            None,
            "character",
            &format!("Cand {idx}"),
            NOVEL_CHARACTER_BODY,
        )
        .await
        .expect("insert_pending should succeed");
        seeded.push(row);
    }
    // Expected keyset order: (created_at ASC, job_id ASC) — mirrors the SQL
    // `ORDER BY created_at ASC, job_id ASC` in `list_pending_for_world_after`.
    seeded.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.job_id.cmp(&b.job_id))
    });
    let expected_names: Vec<String> = seeded
        .iter()
        .map(|c| c.canonical_name_guess.clone().unwrap_or_default())
        .collect();
    let expected_ids: Vec<String> = seeded.iter().map(|c| c.job_id.clone()).collect();

    // Page 1: limit=2, no cursor.
    let Json(page1) = get_candidates(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Query(CandidatesQuery {
            limit: Some(2),
            cursor: None,
        }),
    )
    .await
    .expect("page 1 should succeed");
    assert_eq!(
        page1.items.len(),
        2,
        "page 1 should return exactly `limit` items"
    );
    assert_eq!(page1.items[0].canonical_name, expected_names[0]);
    assert_eq!(page1.items[1].canonical_name, expected_names[1]);
    assert_eq!(page1.items[0].job_id, expected_ids[0]);
    assert_eq!(page1.items[1].job_id, expected_ids[1]);
    assert!(
        page1.pagination.has_more,
        "page 1 must signal has_more when more rows remain"
    );
    let cursor1 = page1
        .pagination
        .next_cursor
        .clone()
        .expect("page 1 must return a next_cursor");

    // Page 2: limit=2, cursor from page 1 — must reach the REMAINING rows,
    // not re-skip inside the first truncated window.
    let Json(page2) = get_candidates(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Query(CandidatesQuery {
            limit: Some(2),
            cursor: Some(cursor1),
        }),
    )
    .await
    .expect("page 2 should succeed");
    assert_eq!(
        page2.items.len(),
        2,
        "page 2 should return the remaining 2 items (the W-01 bug returned 0)"
    );
    assert_eq!(page2.items[0].canonical_name, expected_names[2]);
    assert_eq!(page2.items[1].canonical_name, expected_names[3]);
    assert_eq!(page2.items[0].job_id, expected_ids[2]);
    assert_eq!(page2.items[1].job_id, expected_ids[3]);
    assert!(
        !page2.pagination.has_more,
        "page 2 is the last page; has_more must be false"
    );
    assert!(
        page2.pagination.next_cursor.is_none(),
        "page 2 is the last page; next_cursor must be absent"
    );

    // No loss, no duplication across the full walk.
    let mut seen: Vec<String> = page1
        .items
        .iter()
        .map(|c| c.job_id.clone())
        .chain(page2.items.iter().map(|c| c.job_id.clone()))
        .collect();
    seen.sort();
    assert_eq!(
        seen,
        {
            let mut all = expected_ids.clone();
            all.sort();
            all
        },
        "every seeded candidate must appear exactly once across pages 1+2"
    );

    // Page 3: cursor past the end — must be empty, not an error.
    let cursor2 = page2
        .pagination
        .next_cursor
        .clone()
        .or_else(|| {
            // Last page has no next_cursor by design; synthesize a cursor from
            // the final row so we can prove a follow-up request stays empty
            // rather than re-issuing page 2.
            seeded
                .last()
                .map(|r| format!("kbp:{}|{}", r.created_at, r.job_id))
        })
        .expect("a synthesized terminal cursor must be available");
    let Json(page3) = get_candidates(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Query(CandidatesQuery {
            limit: Some(2),
            cursor: Some(cursor2),
        }),
    )
    .await
    .expect("page 3 (past end) should succeed, not error");
    assert!(
        page3.items.is_empty(),
        "a cursor past the last row must yield an empty page, not a repeat"
    );
    assert!(!page3.pagination.has_more);
}

/// Regression for V1.73 greploop issue 2: `candidate_id` was projected from the
/// non-unique `canonical_name_guess`. Two pending candidates that share the
/// same guessed name (the same character extracted from two different source
/// works — distinct `work_entry_id`) collided on `candidate_id`, so their React
/// Flow node IDs clashed and `candidateItems.find(c => c.candidate_id === ...)`
/// resolved to the FIRST match, promoting the wrong `job_id`. The fix projects
/// `candidate_id` from the unique row PK `job_id`.
#[tokio::test]
async fn get_candidates_distinct_candidate_id_for_same_canonical_name() {
    let (_tmp, state) = fresh_state().await;

    // Two pending candidates with the SAME canonical_name_guess but distinct
    // work_entry_id (the idempotency index is on (creator, work_entry_id,
    // world), so distinct work_entry_id lets both rows coexist).
    seed_pending_candidate(
        state.pool().unwrap(),
        "xj_aaaaaa0000000000000000000001",
        "we_source_work_one",
        "wld_test_world",
        "character",
        "Duplicate Name",
    )
    .await;
    seed_pending_candidate(
        state.pool().unwrap(),
        "xj_aaaaaa0000000000000000000002",
        "we_source_work_two",
        "wld_test_world",
        "character",
        "Duplicate Name",
    )
    .await;

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

    assert_eq!(
        resp.items.len(),
        2,
        "both same-name candidates must be listed"
    );
    let ids: Vec<String> = resp.items.iter().map(|c| c.candidate_id.clone()).collect();
    assert_ne!(
        ids[0], ids[1],
        "candidate_id must be unique per row even when canonical_name_guess collides"
    );
    // The fix: candidate_id == job_id (the row PK), not canonical_name_guess.
    assert!(
        resp.items.iter().all(|c| c.candidate_id == c.job_id),
        "candidate_id must equal job_id; got {ids:?}"
    );
    // Display name is still the shared guess.
    assert!(
        resp.items
            .iter()
            .all(|c| c.canonical_name == "Duplicate Name"),
        "canonical_name stays the guessed display name"
    );
}

/// Regression for the V1.73 greploop iter-2 greptile P1: when a concurrent
/// write bumps `kb_extract_jobs.version` between the outer version check and
/// the promote-reject CAS UPDATE, the 409 conflict MUST report the re-read
/// current version — NOT the stale `req.expected_version`. Otherwise the
/// canvas client (`promotion-inspector.tsx`) resubmits with the same stale
/// version and hits a second, avoidable conflict.
///
/// We force the CAS-miss path deterministically by bumping the candidate's
/// `version` directly (simulating a concurrent write) while keeping it
/// `pending`, then issuing a single `reject` with the now-stale
/// `expected_version`. The candidate reaches the CAS precondition (still
/// pending) but the CAS UPDATE affects 0 rows (version mismatch) -> 409.
/// The losing 409 must therefore carry `current_version = V+1`, not the stale
/// `expected_version = V`. (An earlier `tokio::join!` form was
/// non-deterministic: the winning reject could terminalize the candidate
/// before the loser's validation read, yielding a 422 instead of the 409.)
#[tokio::test]
async fn promote_reject_cas_miss_conflict_carries_bumped_version() {
    let (_tmp, state) = fresh_state().await;
    let candidate = insert_pending(
        state.pool().unwrap(),
        "test_creator",
        "ws",
        "wld_test_world",
        None,
        None,
        "character",
        "Racea",
        NOVEL_CHARACTER_BODY,
    )
    .await
    .unwrap();
    let stale_expected = u64::try_from(candidate.version).unwrap_or(0);

    let mk_req = || WorldKbPromoteCandidateRequest {
        job_id: candidate.job_id.clone(),
        candidate_id: "kb_cand".to_string(),
        action: "reject".parse().unwrap(),
        expected_version: stale_expected,
        merge_target_id: None,
        patch: None,
    };

    // Deterministically create the CAS-miss scenario: bump the candidate's
    // version directly (simulating a concurrent write) WITHOUT transitioning
    // its state, so it stays `pending` and reaches the CAS precondition. The
    // previous `tokio::join!` form relied on scheduler-dependent interleaving
    // and could let the winning reject terminalize the candidate before the
    // loser's validation read it — producing a 422 ("already terminal") instead
    // of the intended 409 CAS-miss on some CI runners.
    // SAFETY: dynamic SQL — test-only version bump; compile-time macro not applicable.
    sqlx::query("UPDATE kb_extract_jobs SET version = version + 1 WHERE job_id = ?")
        .bind(&candidate.job_id)
        .execute(state.pool().unwrap())
        .await
        .unwrap();

    // The candidate is still pending but its version is now stale_expected + 1,
    // so the promote CAS UPDATE affects 0 rows -> 409 conflict.
    let loser = promote_candidate(
        State(state.clone()),
        Path("wld_test_world".to_string()),
        Json(mk_req()),
    )
    .await
    .expect_err("stale expected_version must produce a 409 CAS-miss conflict");

    assert_eq!(loser.status_code(), axum::http::StatusCode::CONFLICT);
    assert_eq!(loser.error_code(), "world_kb_conflict");
    let details = loser.error_details().expect("conflict must carry details");
    // The fix: `current_version` is re-read after the CAS miss, reflecting the
    // winner's bump (V+1). Pre-fix this was the stale `expected_version` (V).
    assert_eq!(
        details["current_version"],
        serde_json::json!(stale_expected + 1),
        "CAS-miss 409 must report the re-read bumped version, not the stale expected_version"
    );
    assert_ne!(
        details["current_version"],
        serde_json::json!(stale_expected),
        "regression: CAS-miss 409 echoed the stale expected_version"
    );
    assert_eq!(details["entity_id"], candidate.job_id);
}

/// Regression for the V1.73 greploop iter-5 greptile P1: when a concurrent
/// write bumps the merge TARGET's `kb_key_blocks.revision` between the merge's
/// read and its in-tx CAS UPDATE, the 409 conflict MUST distinctly mark the
/// TARGET as the conflicting entity (`conflicting_path = "merge_target"`,
/// `entity_id = <target_id>`) — NOT the candidate's `"version"` path.
///
/// Without the marker, the client (`promotion-inspector.tsx`) cannot tell a
/// target conflict from a candidate conflict: it treats the 409's
/// `current_version` (the target's revision) as the candidate's version and
/// retries the promote with `expected_version = <target revision>`, which fails
/// the candidate CAS again — a two-round-trip conflict loop with misleading
/// modal text.
///
/// Deterministic setup (same spirit as `promote_reject_cas_miss_*`): hold a
/// RESERVED write lock with `BEGIN IMMEDIATE`, let `promote_merge` read the
/// target at revision V and block on its in-tx CAS, then bump the target
/// revision and commit. The promote CAS then affects 0 rows — the bug path —
/// without relying on `tokio::join!` scheduler interleaving (which can fully
/// serialize on CI and let both merges succeed).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn promote_merge_target_cas_miss_marks_target_conflict() {
    let (_tmp, state) = fresh_state().await;
    let pool = state.pool().unwrap();
    // Confirmed merge target at revision 0.
    seed_key_block(
        pool,
        "kb_target",
        "wld_test_world",
        "character",
        "Aria",
        "confirmed",
        Some(0),
        None,
    )
    .await;
    seed_pending_candidate(
        pool,
        "xj_merge_c1",
        "Racea1",
        "wld_test_world",
        "character",
        "Racea",
    )
    .await;

    let req = WorldKbPromoteCandidateRequest {
        job_id: "xj_merge_c1".to_string(),
        candidate_id: "kb_cand".to_string(),
        action: "merge".parse().unwrap(),
        expected_version: 0,
        merge_target_id: Some("kb_target".to_string()),
        patch: None,
    };

    // Hold RESERVED so the promote's CAS write blocks after its target read.
    let mut lock_conn = pool.acquire().await.expect("acquire lock connection");
    // SAFETY: test-only lock; no schema validation needed.
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *lock_conn)
        .await
        .expect("BEGIN IMMEDIATE");

    let state_for_promote = state.clone();
    let promote = tokio::spawn(async move {
        promote_candidate(
            State(state_for_promote),
            Path("wld_test_world".to_string()),
            Json(req),
        )
        .await
    });

    // Promote must reach the blocked CAS write (cannot finish while we hold
    // the lock). Wait until it is clearly in-flight, then bump the target.
    let started = std::time::Instant::now();
    loop {
        assert!(
            !promote.is_finished(),
            "promote finished while RESERVED lock held — cannot force target CAS miss"
        );
        if started.elapsed() >= std::time::Duration::from_millis(100) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    // Simulate the concurrent writer that wins the target CAS.
    // SAFETY: test-only revision bump; compile-time macro not applicable.
    sqlx::query(
        "UPDATE kb_key_blocks \
         SET revision = COALESCE(revision, 0) + 1, updated_at = datetime('now') \
         WHERE key_block_id = ?",
    )
    .bind("kb_target")
    .execute(&mut *lock_conn)
    .await
    .expect("bump target revision");
    sqlx::query("COMMIT")
        .execute(&mut *lock_conn)
        .await
        .expect("COMMIT");
    drop(lock_conn);

    let loser = promote
        .await
        .expect("promote task join")
        .expect_err("stale target revision must produce merge_target 409");

    assert_eq!(loser.status_code(), axum::http::StatusCode::CONFLICT);
    assert_eq!(loser.error_code(), "world_kb_conflict");
    let details = loser.error_details().expect("conflict must carry details");
    // The fix: the target CAS miss is tagged `conflicting_path = "merge_target"`
    // (distinct from the candidate's `"version"`) so the client can distinguish
    // a target conflict from a candidate conflict. Pre-fix this was
    // `conflicting_path = "version"` — indistinguishable from a candidate CAS
    // miss, causing the client to retry the candidate with the target's
    // revision as `expected_version`.
    assert_eq!(
        details["conflicting_path"], "merge_target",
        "target CAS-miss 409 must tag conflicting_path = merge_target so the \
         client distinguishes a target conflict from a candidate conflict"
    );
    assert_eq!(
        details["entity_id"], "kb_target",
        "target CAS-miss 409 must carry the target's entity_id"
    );
    assert_eq!(
        details["current_version"], 1,
        "target CAS-miss 409 must report the bumped target revision (V+1)"
    );
}

// ─── computable WorldKbEntry state read (V1.114 P2) ───────────────────────────────

#[tokio::test]
async fn get_key_block_state_computable_returns_state() {
    let (_tmp, state) = fresh_state().await;
    let body_json = serde_json::json!({
        "summary": "A computable hero",
        "attributes": {"max_hp": 100},
        "computable": true,
        "state": {"character": {"current_hp": 85, "status_effects": ["poisoned"]}}
    })
    .to_string();
    seed_key_block(
        state.pool().unwrap(),
        "kb_hero",
        "wld_test_world",
        "character",
        "Hero",
        "confirmed",
        Some(7),
        Some(&body_json),
    )
    .await;

    let Json(resp) = get_key_block_state(
        State(state.clone()),
        Path(("wld_test_world".to_string(), "kb_hero".to_string())),
    )
    .await
    .expect("state read should succeed");

    let expected = WorldKbKeyBlockStateResponse {
        state: Some(
            serde_json::json!({"character": {"current_hp": 85, "status_effects": ["poisoned"]}})
                .as_object()
                .unwrap()
                .clone(),
        ),
        is_computable: true,
        version: 7,
    };
    assert_eq!(
        serde_json::to_value(&resp).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
}

#[tokio::test]
async fn get_key_block_state_non_computable_returns_null_state() {
    let (_tmp, state) = fresh_state().await;
    let body_json = serde_json::json!({
        "summary": "A plain scene",
        "attributes": {"novel_category": "scene"},
        "computable": false
    })
    .to_string();
    seed_key_block(
        state.pool().unwrap(),
        "kb_scene",
        "wld_test_world",
        "scene",
        "Forest",
        "confirmed",
        Some(3),
        Some(&body_json),
    )
    .await;

    let Json(resp) = get_key_block_state(
        State(state.clone()),
        Path(("wld_test_world".to_string(), "kb_scene".to_string())),
    )
    .await
    .expect("state read should succeed");

    assert!(resp.state.is_none());
    assert!(!resp.is_computable);
    assert_eq!(resp.version, 3);
}

#[tokio::test]
async fn get_key_block_state_missing_block_returns_404() {
    let (_tmp, state) = fresh_state().await;

    let err = get_key_block_state(
        State(state.clone()),
        Path(("wld_test_world".to_string(), "kb_missing".to_string())),
    )
    .await
    .expect_err("missing key block must 404");
    assert_eq!(err.status_code(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_key_block_state_cross_world_returns_404() {
    let (_tmp, state) = fresh_state().await;
    // Seed a second world owned by the same creator so the world-owner check
    // passes; the key block simply lives in a different world.
    // SAFETY: test-only seed of a second world row.
    sqlx::query(
        "INSERT OR IGNORE INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, created_at) \
         VALUES ('wld_other', 'ws', 'test_creator', 'Other', 'other-world', 'active', 'private', \
          'manual', '{}', datetime('now'))",
    )
    .execute(state.pool().unwrap())
    .await
    .unwrap();
    seed_key_block(
        state.pool().unwrap(),
        "kb_other",
        "wld_other",
        "character",
        "Other",
        "confirmed",
        Some(1),
        None,
    )
    .await;

    let err = get_key_block_state(
        State(state.clone()),
        Path(("wld_test_world".to_string(), "kb_other".to_string())),
    )
    .await
    .expect_err("cross-world key block must 404");
    assert_eq!(err.status_code(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_key_block_state_unknown_world_returns_404() {
    let (_tmp, state) = fresh_state().await;

    let err = get_key_block_state(
        State(state.clone()),
        Path(("wld_unknown".to_string(), "kb_hero".to_string())),
    )
    .await
    .expect_err("unknown world must 404");
    assert_eq!(err.status_code(), axum::http::StatusCode::NOT_FOUND);
}
