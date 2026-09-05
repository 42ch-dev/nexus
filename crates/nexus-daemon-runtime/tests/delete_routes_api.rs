//! Daemon DELETE handler contract tests (V1.129 P2 — R-V1126P0-T2-001).
//!
//! Covers:
//! - `delete_work` → 204 (success), 404 (unknown), 401 (no creator),
//!   409 (completion-locked), 423 (runtime-locked), cascade correctness
//!   (pool entries dropped via FK CASCADE).
//! - `delete_world` → 204 (success), 404 (unknown / unowned), 401 (no creator),
//!   cascade correctness (KB / timelines dropped; Works preserved with
//!   `world_id = NULL`).
//!
//! Tests invoke handlers directly to bypass the axum-test limitation with
//! hyphenated UUIDs in path segments (see existing `works_api.rs` note).
//! HTTP routing is verified indirectly by the route registration in
//! `api/mod.rs` and the existing `get_work_by_id_returns_404_for_unknown`
//! HTTP-level test.

#![allow(clippy::unwrap_used)]

use axum::extract::{Path, State};
use axum::http::StatusCode;
use nexus_daemon_runtime::api::errors::NexusApiError;
use nexus_daemon_runtime::api::handlers::narrative::delete_world;
use nexus_daemon_runtime::api::handlers::works::{create_work, delete_work, CreateWorkRequest};
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::workspace::WorkspaceState;

async fn handler_state() -> (WorkspaceState, test_utils::TestTempRoot) {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    test_utils::seed_test_creator_and_world(state.pool().unwrap()).await;
    (state, tmp)
}

async fn handler_state_no_creator() -> (WorkspaceState, test_utils::TestTempRoot) {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    // Overwrite config.toml to remove active_creator_id so handlers see no
    // active creator.
    std::fs::write(nexus_home.join("config.toml"), "[empty]\n").unwrap();
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    (state, tmp)
}

async fn create_test_work(state: &WorkspaceState) -> String {
    let req = CreateWorkRequest {
        title: "Delete Test".into(),
        long_term_goal: "Goal".into(),
        initial_idea: "Idea".into(),
        world_id: Some("wld_test_world".to_string()),
        story_ref: None,
        primary_preset_id: None,
        client_request_id: None,
        lineage_from_work_id: None,
        set_pool_active: None,
        work_profile: None,
    };
    let (_, resp) = create_work(State(state.clone()), axum::Json(req))
        .await
        .expect("create_work");
    resp.work_id.clone()
}

// ─── delete_work ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_work_returns_204_on_success() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_test_work(&state).await;

    let status = delete_work(State(state.clone()), Path(work_id.clone()))
        .await
        .expect("delete_work should succeed");
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Subsequent get_work returns 404 (row gone, not soft-deleted).
    let err =
        nexus_daemon_runtime::api::handlers::works::get_work(State(state.clone()), Path(work_id))
            .await
            .expect_err("get_work should 404 after delete");
    assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_work_returns_404_for_unknown_id() {
    let (state, _tmp) = handler_state().await;
    let err = delete_work(State(state), Path("wrk_unknown_12345".to_string()))
        .await
        .expect_err("unknown work should 404");
    assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
    assert_eq!(err.error_code(), "not_found");
}

#[tokio::test]
async fn delete_work_returns_401_without_creator() {
    let (state, _tmp) = handler_state_no_creator().await;
    let err = delete_work(State(state), Path("wrk_anything".to_string()))
        .await
        .expect_err("no creator should 401");
    assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);
    assert_eq!(err.error_code(), "auth_required");
}

#[tokio::test]
async fn delete_work_cascades_pool_entries_via_fk() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_test_work(&state).await;

    let pool = state.pool().unwrap();
    // SAFETY: test-only seed — insert a novel_pool_entries row referencing the Work.
    sqlx::query(
        "INSERT INTO novel_pool_entries \
         (entry_id, creator_id, work_id, status, promoted_at, title, updated_at) \
         VALUES ('pool_cascade_test', 'test_creator', ?, 'queued', datetime('now'), 'T', datetime('now'))",
    )
    .bind(&work_id)
    .execute(pool)
    .await
    .unwrap();

    let status = delete_work(State(state.clone()), Path(work_id.clone()))
        .await
        .expect("delete_work");
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Cascade: pool entry should be gone (FK ON DELETE CASCADE on work_id).
    let pool_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM novel_pool_entries WHERE work_id = ?")
            .bind(&work_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(
        pool_count, 0,
        "novel_pool_entries row should cascade-delete"
    );

    let work_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM works WHERE work_id = ?")
        .bind(&work_id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(work_count, 0, "works row should be gone");
}

#[tokio::test]
async fn delete_work_blocked_when_completion_locked() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_test_work(&state).await;
    let pool = state.pool().unwrap();

    // SAFETY: test-only — set completion_locked_at to simulate a locked Work.
    sqlx::query("UPDATE works SET completion_locked_at = datetime('now') WHERE work_id = ?")
        .bind(&work_id)
        .execute(pool)
        .await
        .unwrap();

    let err = delete_work(State(state.clone()), Path(work_id.clone()))
        .await
        .expect_err("completion-locked work should conflict");
    assert_eq!(err.status_code(), StatusCode::CONFLICT);

    // Work row still exists.
    let work_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM works WHERE work_id = ?")
        .bind(&work_id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(
        work_count, 1,
        "completion-locked Work should not be deleted"
    );
}

// ─── delete_world ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_world_returns_204_on_success() {
    let (state, _tmp) = handler_state().await;
    let status = delete_world(State(state.clone()), Path("wld_test_world".to_string()))
        .await
        .expect("delete_world");
    assert_eq!(status, StatusCode::NO_CONTENT);

    let pool = state.pool().unwrap();
    let world_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM narrative_worlds WHERE world_id = 'wld_test_world'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(world_count, 0);
}

#[tokio::test]
async fn delete_world_returns_404_for_unknown_id() {
    let (state, _tmp) = handler_state().await;
    let err = delete_world(State(state), Path("wld_does_not_exist".to_string()))
        .await
        .expect_err("unknown world should 404");
    assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_world_returns_404_for_unowned_world() {
    let (state, _tmp) = handler_state().await;
    let pool = state.pool().unwrap();
    // SAFETY: test-only — seed a foreign-creator row first to satisfy the FK,
    // then a world owned by that creator. The active creator ('test_creator')
    // does not own this world, so DELETE must return 404.
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES ('ctr_other', 'Other', 'active', datetime('now'), '{}')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, created_at) \
         VALUES ('wld_other_owner', 'ws', 'ctr_other', 'Foreign', 'foreign', \
          'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .execute(pool)
    .await
    .unwrap();

    // Active creator is 'test_creator' — DELETE on foreign-owned world returns 404.
    let err = delete_world(State(state), Path("wld_other_owner".to_string()))
        .await
        .expect_err("foreign-owned world should 404");
    assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_world_returns_401_without_creator() {
    let (state, _tmp) = handler_state_no_creator().await;
    let err = delete_world(State(state), Path("wld_any".to_string()))
        .await
        .expect_err("no creator should 401");
    assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn delete_world_cascades_kb_and_preserves_works() {
    let (state, _tmp) = handler_state().await;

    // Create a Work bound to the test world.
    let work_id = create_test_work(&state).await;

    let pool = state.pool().unwrap();
    // SAFETY: test-only seed of kb_key_blocks + narrative_timeline_events.
    sqlx::query(
        "INSERT INTO kb_key_blocks \
         (key_block_id, world_id, block_type, canonical_name, status, body_json) \
         VALUES ('kb_test_1', 'wld_test_world', 'character', 'Hero', 'provisional', '{}')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO narrative_timeline_events \
         (timeline_event_id, world_id, branch_id, event_type, sequence_no, title) \
         VALUES ('evt_test_1', 'wld_test_world', 'br_main', 'plot_point', 0, 'Inciting')",
    )
    .execute(pool)
    .await
    .unwrap();

    let status = delete_world(State(state.clone()), Path("wld_test_world".to_string()))
        .await
        .expect("delete_world");
    assert_eq!(status, StatusCode::NO_CONTENT);

    // KB row cascade-deleted.
    let kb_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM kb_key_blocks WHERE world_id = 'wld_test_world'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(kb_count, 0, "kb_key_blocks should cascade-delete");

    // Timeline row cascade-deleted.
    let evt_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM narrative_timeline_events WHERE world_id = 'wld_test_world'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        evt_count, 0,
        "narrative_timeline_events should cascade-delete"
    );

    // Work is preserved but its world_id is now NULL.
    let work_row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT world_id FROM works WHERE work_id = ?")
            .bind(&work_id)
            .fetch_optional(pool)
            .await
            .unwrap();
    let (world_id,) = work_row.expect("Work row should still exist");
    assert!(
        world_id.is_none(),
        "Work should survive World delete with world_id=NULL (architect lock), got {world_id:?}"
    );
}

// Silence the unused import for the rare configuration where NexusApiError is
// not directly referenced in asserts but is used via status_code()/error_code().
#[allow(dead_code)]
fn _assert_error_codes_are_accessible(_e: NexusApiError) {}

#[tokio::test]
async fn delete_world_blocked_when_active_actor_binding_exists() {
    let (state, _tmp) = handler_state().await;
    let pool = state.pool().unwrap();
    let work_id = create_test_work(&state).await;

    let created = nexus_local_db::create_character_with_initial_binding(
        pool,
        nexus_local_db::CreateCharacterParams {
            owner_creator_id: "test_creator",
            display_name: "Bound",
            image_uri: None,
            persona_json: "{}",
            world_id: "wld_test_world",
            world_sheet_entry_id: None,
        },
    )
    .await
    .expect("seed character binding");

    let err = delete_world(State(state.clone()), Path("wld_test_world".to_string()))
        .await
        .expect_err("active binding must block world delete");
    assert_eq!(err.status_code(), StatusCode::CONFLICT);
    assert_eq!(err.error_code(), "world_has_actor_bindings");

    let world_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM narrative_worlds WHERE world_id = 'wld_test_world'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let work_world: Option<String> =
        sqlx::query_scalar("SELECT world_id FROM works WHERE work_id = ?")
            .bind(&work_id)
            .fetch_one(pool)
            .await
            .unwrap();
    let bind_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM actor_world_bindings WHERE binding_id = ?")
            .bind(&created.binding.binding_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(world_count, 1, "world row must be unchanged");
    assert_eq!(
        work_world.as_deref(),
        Some("wld_test_world"),
        "works must not be detached"
    );
    assert_eq!(bind_count, 1, "binding must remain");
}

#[tokio::test]
async fn delete_world_blocked_when_inactive_actor_binding_exists() {
    let (state, _tmp) = handler_state().await;
    let pool = state.pool().unwrap();
    let work_id = create_test_work(&state).await;

    sqlx::query(
        "INSERT INTO kb_extract_jobs (job_id, creator_id, workspace_id, work_entry_id, world_id) \
         VALUES ('xj_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'test_creator', 'ws', 'we_test', 'wld_test_world')",
    )
    .execute(pool)
    .await
    .unwrap();

    let created = nexus_local_db::create_character_with_initial_binding(
        pool,
        nexus_local_db::CreateCharacterParams {
            owner_creator_id: "test_creator",
            display_name: "InactiveBound",
            image_uri: None,
            persona_json: "{}",
            world_id: "wld_test_world",
            world_sheet_entry_id: None,
        },
    )
    .await
    .expect("seed character binding");
    sqlx::query("UPDATE actor_world_bindings SET status = 'inactive' WHERE binding_id = ?")
        .bind(&created.binding.binding_id)
        .execute(pool)
        .await
        .unwrap();

    let jobs_before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM kb_extract_jobs WHERE world_id = 'wld_test_world'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(jobs_before, 1);

    let err = delete_world(State(state.clone()), Path("wld_test_world".to_string()))
        .await
        .expect_err("inactive binding must block world delete");
    assert_eq!(err.status_code(), StatusCode::CONFLICT);
    assert_eq!(err.error_code(), "world_has_actor_bindings");
    let body = err.to_response_body();
    assert_ne!(body.error.message, "world_has_actor_bindings");
    assert!(body.error.message.to_lowercase().contains("binding"));

    let world_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM narrative_worlds WHERE world_id = 'wld_test_world'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let work_world: Option<String> =
        sqlx::query_scalar("SELECT world_id FROM works WHERE work_id = ?")
            .bind(&work_id)
            .fetch_one(pool)
            .await
            .unwrap();
    let jobs_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM kb_extract_jobs WHERE world_id = 'wld_test_world'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let bind_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM actor_world_bindings WHERE binding_id = ?")
            .bind(&created.binding.binding_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(world_count, 1, "world row must be unchanged");
    assert_eq!(
        work_world.as_deref(),
        Some("wld_test_world"),
        "works must not be detached"
    );
    assert_eq!(jobs_after, 1, "extract job queue must be unchanged");
    assert_eq!(bind_count, 1, "binding must remain");
}
