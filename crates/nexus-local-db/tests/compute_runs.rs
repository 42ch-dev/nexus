//! Tests for `compute_runs` CRUD (V1.147 P0 T2).
//!
//! Covers: insert/get/status transitions/list pagination + filter;
//! unique `run_id` violation; adapter-style row with NULL `run_id` coexists.
//!
//! Task 2 fix wave adds: status-transition guards, rollback, no-op
//! transition errors, empty-creator_world_ids filter behaviour.

use nexus_local_db::compute_runs::{
    get_run, insert_run, list_runs, set_run_applied_in_tx, set_run_discarded, set_run_failed,
    set_run_succeeded, RunListFilters, RUN_STATUS_APPLIED, RUN_STATUS_DISCARDED, RUN_STATUS_FAILED,
    RUN_STATUS_RUNNING, RUN_STATUS_SUCCEEDED,
};
use nexus_local_db::LocalDbError;

async fn setup_db() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    (pool, dir)
}

/// Helper: insert a run and transition it to succeeded, returning the `run_id`.
async fn insert_and_succeed(pool: &sqlx::SqlitePool, world_id: &str, module_id: &str) -> String {
    let run_id = insert_run(pool, world_id, module_id, None, None, None, None)
        .await
        .unwrap();
    let affected = set_run_succeeded(pool, &run_id, r"{}").await.unwrap();
    assert_eq!(affected, 1, "succeeded should affect exactly 1 row");
    run_id
}

// ── insert + get ────────────────────────────────────────────────

#[tokio::test]
async fn insert_and_get_roundtrip() {
    let (pool, _dir) = setup_db().await;

    let run_id = insert_run(&pool, "world-1", "module-a", None, None, None, None)
        .await
        .unwrap();
    assert!(run_id.starts_with("run_"));

    let row = get_run(&pool, &run_id)
        .await
        .unwrap()
        .expect("row must exist");
    assert_eq!(row.run_id, run_id);
    assert_eq!(row.world_id, "world-1");
    assert_eq!(row.module_id, "module-a");
    assert_eq!(row.module_version, None);
    assert_eq!(row.status, RUN_STATUS_RUNNING);
    assert!(row.proposals_json.is_none());
    assert!(row.error_json.is_none());
    assert!(!row.created_at.is_empty());
    assert!(row.updated_at.is_none());
    assert!(row.accepted_at.is_none());
    assert!(row.branch_id.is_none());
    assert!(row.timeline_head_event_id.is_none());
}

#[tokio::test]
async fn insert_with_branch_snapshot_roundtrips() {
    let (pool, _dir) = setup_db().await;

    let run_id = insert_run(
        &pool,
        "world-bs",
        "module-bs",
        None,
        Some("fbk_side1"),
        Some("evt_head"),
        None,
    )
    .await
    .unwrap();

    let row = get_run(&pool, &run_id)
        .await
        .unwrap()
        .expect("row must exist");
    assert_eq!(row.branch_id.as_deref(), Some("fbk_side1"));
    assert_eq!(row.timeline_head_event_id.as_deref(), Some("evt_head"));
}

#[tokio::test]
async fn get_nonexistent_run() {
    let (pool, _dir) = setup_db().await;
    let row = get_run(&pool, "run_nonexistent").await.unwrap();
    assert!(row.is_none());
}

#[tokio::test]
async fn insert_with_module_version_and_params() {
    let (pool, _dir) = setup_db().await;

    let run_id = insert_run(
        &pool,
        "world-2",
        "module-b",
        Some("1.2.3"),
        None,
        None,
        Some(r#"{"key":"value"}"#),
    )
    .await
    .unwrap();

    let row = get_run(&pool, &run_id)
        .await
        .unwrap()
        .expect("row must exist");
    assert_eq!(row.module_version.as_deref(), Some("1.2.3"));
    assert_eq!(
        row.invocation_params_json.as_deref(),
        Some(r#"{"key":"value"}"#)
    );
}

// ── status transitions (happy path) ─────────────────────────────

#[tokio::test]
async fn transition_to_succeeded() {
    let (pool, _dir) = setup_db().await;
    let run_id = insert_run(&pool, "world-3", "module-c", None, None, None, None)
        .await
        .unwrap();

    let proposals = r#"{"state_delta":[]}"#;
    let affected = set_run_succeeded(&pool, &run_id, proposals).await.unwrap();
    assert_eq!(affected, 1);

    let row = get_run(&pool, &run_id)
        .await
        .unwrap()
        .expect("row must exist");
    assert_eq!(row.status, RUN_STATUS_SUCCEEDED);
    assert_eq!(row.proposals_json.as_deref(), Some(proposals));
    assert!(row.error_json.is_none());
    assert!(row.updated_at.is_some());
}

#[tokio::test]
async fn transition_to_failed() {
    let (pool, _dir) = setup_db().await;
    let run_id = insert_run(&pool, "world-4", "module-d", None, None, None, None)
        .await
        .unwrap();

    let error = r#"{"code":"compute_fuel_exhausted"}"#;
    let affected = set_run_failed(&pool, &run_id, error).await.unwrap();
    assert_eq!(affected, 1);

    let row = get_run(&pool, &run_id)
        .await
        .unwrap()
        .expect("row must exist");
    assert_eq!(row.status, RUN_STATUS_FAILED);
    assert_eq!(row.error_json.as_deref(), Some(error));
    assert!(row.proposals_json.is_none());
    assert!(row.updated_at.is_some());
}

#[tokio::test]
async fn transition_to_applied_in_tx() {
    let (pool, _dir) = setup_db().await;
    let run_id = insert_run(&pool, "world-5", "module-e", None, None, None, None)
        .await
        .unwrap();

    // Set succeeded first (precondition for applied)
    set_run_succeeded(&pool, &run_id, r#"{"state_delta":[]}"#)
        .await
        .unwrap();

    let accepted_at = "2026-07-31T12:00:00+00:00";
    let mut tx = pool.begin().await.unwrap();
    let affected = set_run_applied_in_tx(&mut tx, &run_id, accepted_at)
        .await
        .unwrap();
    assert_eq!(affected, 1);
    tx.commit().await.unwrap();

    let row = get_run(&pool, &run_id)
        .await
        .unwrap()
        .expect("row must exist");
    assert_eq!(row.status, RUN_STATUS_APPLIED);
    assert_eq!(row.accepted_at.as_deref(), Some(accepted_at));
    assert!(row.updated_at.is_some());
}

#[tokio::test]
async fn transition_to_discarded() {
    let (pool, _dir) = setup_db().await;
    let run_id = insert_and_succeed(&pool, "world-6", "module-f").await;

    let affected = set_run_discarded(&pool, &run_id).await.unwrap();
    assert_eq!(affected, 1);

    let row = get_run(&pool, &run_id)
        .await
        .unwrap()
        .expect("row must exist");
    assert_eq!(row.status, RUN_STATUS_DISCARDED);
    assert!(row.updated_at.is_some());
}

// ── guard: transition from wrong status errors ──────────────────

#[tokio::test]
async fn set_succeeded_guard_requires_running() {
    let (pool, _dir) = setup_db().await;
    let run_id = insert_and_succeed(&pool, "world-g1", "mod-g1").await;
    // Already succeeded — second succeed call must fail
    let err = set_run_succeeded(&pool, &run_id, r#"{"x":1}"#)
        .await
        .unwrap_err();
    assert_constraint_violation(&err, "not in 'running' status");
}

#[tokio::test]
async fn set_failed_guard_requires_running() {
    let (pool, _dir) = setup_db().await;
    let run_id = insert_run(&pool, "world-g2", "mod-g2", None, None, None, None)
        .await
        .unwrap();
    set_run_failed(&pool, &run_id, r#"{"e":1}"#).await.unwrap();
    // Already failed — second fail must error
    let err = set_run_failed(&pool, &run_id, r#"{"e":2}"#)
        .await
        .unwrap_err();
    assert_constraint_violation(&err, "not in 'running' status");
}

#[tokio::test]
async fn set_applied_guard_requires_succeeded() {
    let (pool, _dir) = setup_db().await;
    // Running run — cannot apply directly
    let run_id = insert_run(&pool, "world-g3", "mod-g3", None, None, None, None)
        .await
        .unwrap();
    let accepted_at = "2026-07-31T12:00:00+00:00";
    let mut tx = pool.begin().await.unwrap();
    let err = set_run_applied_in_tx(&mut tx, &run_id, accepted_at)
        .await
        .unwrap_err();
    // TX is dirty — rollback is fine
    let _ = tx.rollback().await;
    assert_constraint_violation(&err, "not in 'succeeded' status");
}

#[tokio::test]
async fn set_applied_guard_requires_succeeded_after_failed() {
    let (pool, _dir) = setup_db().await;
    let run_id = insert_run(&pool, "world-g3b", "mod-g3b", None, None, None, None)
        .await
        .unwrap();
    set_run_failed(&pool, &run_id, r"{}").await.unwrap();
    // Failed → cannot accept
    let accepted_at = "2026-07-31T12:00:00+00:00";
    let mut tx = pool.begin().await.unwrap();
    let err = set_run_applied_in_tx(&mut tx, &run_id, accepted_at)
        .await
        .unwrap_err();
    let _ = tx.rollback().await;
    assert_constraint_violation(&err, "not in 'succeeded' status");
}

#[tokio::test]
async fn set_discarded_guard_requires_succeeded() {
    let (pool, _dir) = setup_db().await;
    // Running run — cannot discard directly
    let run_id = insert_run(&pool, "world-g4", "mod-g4", None, None, None, None)
        .await
        .unwrap();
    let err = set_run_discarded(&pool, &run_id).await.unwrap_err();
    assert_constraint_violation(&err, "not in 'succeeded' status");
}

#[tokio::test]
async fn set_discarded_on_already_discarded_errors() {
    let (pool, _dir) = setup_db().await;
    let run_id = insert_and_succeed(&pool, "world-g5", "mod-g5").await;
    set_run_discarded(&pool, &run_id).await.unwrap();
    // Second discard → error
    let err = set_run_discarded(&pool, &run_id).await.unwrap_err();
    assert_constraint_violation(&err, "not in 'succeeded' status");
}

// ── guard: transition on nonexistent run_id errors ──────────────

#[tokio::test]
async fn transition_nonexistent_run_errors() {
    let (pool, _dir) = setup_db().await;
    let nonexistent = "run_00000000-0000-0000-0000-000000000000";

    let err = set_run_succeeded(&pool, nonexistent, r"{}")
        .await
        .unwrap_err();
    assert_constraint_violation(&err, "not in 'running' status");

    let err = set_run_failed(&pool, nonexistent, r"{}").await.unwrap_err();
    assert_constraint_violation(&err, "not in 'running' status");

    let err = set_run_discarded(&pool, nonexistent).await.unwrap_err();
    assert_constraint_violation(&err, "not in 'succeeded' status");

    let mut tx = pool.begin().await.unwrap();
    let err = set_run_applied_in_tx(&mut tx, nonexistent, "2026-07-31T00:00:00+00:00")
        .await
        .unwrap_err();
    let _ = tx.rollback().await;
    assert_constraint_violation(&err, "not in 'succeeded' status");
}

// ── rollback: applied_in_tx rolls back correctly ────────────────

#[tokio::test]
async fn applied_in_tx_rollback_preserves_status() {
    let (pool, _dir) = setup_db().await;
    let run_id = insert_and_succeed(&pool, "world-rb", "mod-rb").await;

    let accepted_at = "2026-07-31T12:00:00+00:00";
    let mut tx = pool.begin().await.unwrap();
    let affected = set_run_applied_in_tx(&mut tx, &run_id, accepted_at)
        .await
        .unwrap();
    assert_eq!(affected, 1);

    // Rollback — status must revert to succeeded.
    // (We cannot verify inside the tx via get_run because pool reads from a
    // different connection and WAL mode hides uncommitted writes.)
    tx.rollback().await.unwrap();

    let row = get_run(&pool, &run_id).await.unwrap().unwrap();
    assert_eq!(row.status, RUN_STATUS_SUCCEEDED);
    // accepted_at must NOT have been persisted
    assert!(row.accepted_at.is_none());
}

// ── list + pagination ───────────────────────────────────────────

#[tokio::test]
async fn list_runs_pagination() {
    let (pool, _dir) = setup_db().await;

    // Insert 3 runs
    let r1 = insert_run(&pool, "world-list", "mod-a", None, None, None, None)
        .await
        .unwrap();
    let r2 = insert_run(&pool, "world-list", "mod-a", None, None, None, None)
        .await
        .unwrap();
    let r3 = insert_run(&pool, "world-list", "mod-a", None, None, None, None)
        .await
        .unwrap();

    // Page 1: limit 2
    let filters = RunListFilters::default();
    let (items1, cursor1) = list_runs(&pool, &filters, None, 2).await.unwrap();
    assert_eq!(items1.len(), 2);
    assert!(cursor1.is_some());

    // Page 2: use cursor
    let (items2, cursor2) = list_runs(&pool, &filters, cursor1.as_deref(), 2)
        .await
        .unwrap();
    assert_eq!(items2.len(), 1);
    assert!(cursor2.is_none());

    // All 3 ids should be unique
    let mut all_ids: Vec<String> = items1
        .iter()
        .chain(items2.iter())
        .map(|r| r.run_id.clone())
        .collect();
    all_ids.sort();
    let mut expected = vec![r1, r2, r3];
    expected.sort();
    assert_eq!(all_ids, expected);
}

/// W1 fix: `list_runs` orders newest-first (`created_at DESC, run_id DESC`)
/// and the keyset cursor walks that order.
#[tokio::test]
async fn list_runs_orders_newest_first() {
    let (pool, _dir) = setup_db().await;

    let r1 = insert_run(&pool, "world-order", "mod-a", None, None, None, None)
        .await
        .unwrap();
    let r2 = insert_run(&pool, "world-order", "mod-a", None, None, None, None)
        .await
        .unwrap();
    let r3 = insert_run(&pool, "world-order", "mod-a", None, None, None, None)
        .await
        .unwrap();

    // Pin distinct created_at values (all rows were created in the same
    // instant before this point — the ordering test must not depend on
    // clock precision).
    // SAFETY: test-only — pinning timestamps to verify ORDER BY semantics.
    for (run_id, ts) in [
        (&r1, "2026-07-31T10:00:00.000Z"),
        (&r2, "2026-07-31T10:00:01.000Z"),
        (&r3, "2026-07-31T10:00:02.000Z"),
    ] {
        sqlx::query("UPDATE compute_sessions SET created_at = ? WHERE run_id = ?")
            .bind(ts)
            .bind(run_id)
            .execute(&pool)
            .await
            .unwrap();
    }

    let filters = RunListFilters::default();
    let (items1, cursor1) = list_runs(&pool, &filters, None, 2).await.unwrap();
    // Newest first: r3, then r2.
    assert_eq!(items1.len(), 2);
    assert_eq!(items1[0].run_id, r3, "newest run must come first");
    assert_eq!(items1[1].run_id, r2);
    assert!(cursor1.is_some());

    // Page 2 continues in the same ordering (r1 last).
    let (items2, cursor2) = list_runs(&pool, &filters, cursor1.as_deref(), 2)
        .await
        .unwrap();
    assert_eq!(items2.len(), 1);
    assert_eq!(items2[0].run_id, r1);
    assert!(cursor2.is_none());
}

#[tokio::test]
async fn list_runs_rejects_malformed_cursor() {
    let (pool, _dir) = setup_db().await;
    let filters = RunListFilters::default();

    let err = list_runs(&pool, &filters, Some("not-a-composite-cursor"), 10)
        .await
        .unwrap_err();
    assert!(
        matches!(err, LocalDbError::ValidationError(_)),
        "malformed cursor must be a validation error, got {err:?}"
    );
}

#[tokio::test]
async fn list_runs_filter_by_world_id() {
    let (pool, _dir) = setup_db().await;

    insert_run(&pool, "world-a", "mod-x", None, None, None, None)
        .await
        .unwrap();
    insert_run(&pool, "world-b", "mod-x", None, None, None, None)
        .await
        .unwrap();

    let filters = RunListFilters {
        world_id: Some("world-a".to_string()),
        ..Default::default()
    };
    let (items, _) = list_runs(&pool, &filters, None, 10).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].world_id, "world-a");
}

#[tokio::test]
async fn list_runs_filter_by_status() {
    let (pool, _dir) = setup_db().await;

    let r1 = insert_run(&pool, "world-s", "mod-x", None, None, None, None)
        .await
        .unwrap();
    let r2 = insert_run(&pool, "world-s", "mod-x", None, None, None, None)
        .await
        .unwrap();
    set_run_succeeded(&pool, &r2, r"{}").await.unwrap();

    let filters = RunListFilters {
        status: Some(RUN_STATUS_RUNNING.to_string()),
        ..Default::default()
    };
    let (items, _) = list_runs(&pool, &filters, None, 10).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].run_id, r1);
    assert_eq!(items[0].status, RUN_STATUS_RUNNING);

    let filters = RunListFilters {
        status: Some(RUN_STATUS_SUCCEEDED.to_string()),
        ..Default::default()
    };
    let (items, _) = list_runs(&pool, &filters, None, 10).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].run_id, r2);
}

#[tokio::test]
async fn list_runs_filter_by_creator_world_ids() {
    let (pool, _dir) = setup_db().await;

    insert_run(&pool, "w-1", "mod-x", None, None, None, None)
        .await
        .unwrap();
    insert_run(&pool, "w-2", "mod-x", None, None, None, None)
        .await
        .unwrap();
    insert_run(&pool, "w-3", "mod-x", None, None, None, None)
        .await
        .unwrap();

    let filters = RunListFilters {
        creator_world_ids: Some(vec!["w-1".to_string(), "w-2".to_string()]),
        ..Default::default()
    };
    let (items, _) = list_runs(&pool, &filters, None, 10).await.unwrap();
    assert_eq!(items.len(), 2);
    for item in &items {
        assert!(["w-1", "w-2"].contains(&item.world_id.as_str()));
    }
}

#[tokio::test]
async fn list_runs_empty_creator_world_ids_returns_nothing() {
    let (pool, _dir) = setup_db().await;

    insert_run(&pool, "w-1", "mod-x", None, None, None, None)
        .await
        .unwrap();
    insert_run(&pool, "w-2", "mod-x", None, None, None, None)
        .await
        .unwrap();

    let filters = RunListFilters {
        creator_world_ids: Some(vec![]),
        ..Default::default()
    };
    let (items, _) = list_runs(&pool, &filters, None, 10).await.unwrap();
    assert!(items.is_empty(), "empty set should match nothing");
}

#[tokio::test]
async fn list_runs_empty() {
    let (pool, _dir) = setup_db().await;
    let filters = RunListFilters::default();
    let (items, cursor) = list_runs(&pool, &filters, None, 10).await.unwrap();
    assert!(items.is_empty());
    assert!(cursor.is_none());
}

// ── unique run_id violation ─────────────────────────────────────

#[tokio::test]
async fn insert_run_unique_id_violation() {
    let (pool, _dir) = setup_db().await;

    let run_id = insert_run(&pool, "world-u", "mod-u", None, None, None, None)
        .await
        .unwrap();

    // Manual insert with the same run_id should fail due to the unique partial index.
    // Include entry_id (NOT NULL) to satisfy the original constraint.
    // SAFETY: test-only — probes the unique index constraint.
    let result = sqlx::query(
        "INSERT INTO compute_sessions (run_id, world_id, module_id, status, created_at, entry_id) \
         VALUES (?, 'world-u2', 'mod-u2', 'running', ?, '')",
    )
    .bind(&run_id)
    .bind("2026-07-31T00:00:00+00:00")
    .execute(&pool)
    .await;

    assert!(result.is_err(), "duplicate run_id insert should fail");
}

// ── adapter-style row coexistence ───────────────────────────────

#[tokio::test]
async fn adapter_row_with_null_run_id_coexists() {
    let (pool, _dir) = setup_db().await;

    // Insert an adapter-style row (spoke path) — session_id is set, run_id is NULL.
    // SAFETY: test-only — probe for adapter/direct-lane coexistence.
    sqlx::query(
        "INSERT INTO compute_sessions (session_id, entry_id, state_json, created_at) \
         VALUES ('ses-test', 'entry-test', '{}', '2026-07-31T00:00:00+00:00')",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Insert a direct-lane row — run_id is set, session_id is NULL.
    let run_id = insert_run(&pool, "world-c", "mod-c", None, None, None, None)
        .await
        .unwrap();

    // Both coexist without conflict.
    let direct = get_run(&pool, &run_id)
        .await
        .unwrap()
        .expect("direct row must exist");
    assert_eq!(direct.run_id, run_id);

    // The adapter row should NOT appear in list_runs (which filters WHERE run_id IS NOT NULL).
    let filters = RunListFilters::default();
    let (items, _) = list_runs(&pool, &filters, None, 10).await.unwrap();
    assert!(items.iter().any(|r| r.run_id == run_id));
    // Adapter row (NULL run_id) not returned.
    assert_eq!(items.len(), 1);
}

// ── helpers ─────────────────────────────────────────────────────

/// Assert `err` is a `ConstraintViolation` whose message contains `fragment`.
fn assert_constraint_violation(err: &LocalDbError, fragment: &str) {
    let msg = format!("{err}");
    assert!(
        msg.contains("constraint violation"),
        "expected ConstraintViolation, got: {msg}"
    );
    assert!(
        msg.contains(fragment),
        "expected message containing '{fragment}', got: {msg}"
    );
}
