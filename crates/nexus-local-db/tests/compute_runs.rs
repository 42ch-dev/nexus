//! Tests for `compute_runs` CRUD (V1.147 P0 T2).
//!
//! Covers: insert/get/status transitions/list pagination + filter;
//! unique run_id violation; adapter-style row with NULL run_id coexists.

use nexus_local_db::compute_runs::{
    get_run, insert_run, list_runs, set_run_applied_in_tx, set_run_discarded, set_run_failed,
    set_run_succeeded, RunListFilters, RUN_STATUS_APPLIED, RUN_STATUS_DISCARDED, RUN_STATUS_FAILED,
    RUN_STATUS_RUNNING, RUN_STATUS_SUCCEEDED,
};

async fn setup_db() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    (pool, dir)
}

// ── insert + get ────────────────────────────────────────────────

#[tokio::test]
async fn insert_and_get_roundtrip() {
    let (pool, _dir) = setup_db().await;

    let run_id = insert_run(&pool, "world-1", "module-a", None, None)
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

// ── status transitions ──────────────────────────────────────────

#[tokio::test]
async fn transition_to_succeeded() {
    let (pool, _dir) = setup_db().await;
    let run_id = insert_run(&pool, "world-3", "module-c", None, None)
        .await
        .unwrap();

    let proposals = r#"{"state_delta":[]}"#;
    set_run_succeeded(&pool, &run_id, proposals).await.unwrap();

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
    let run_id = insert_run(&pool, "world-4", "module-d", None, None)
        .await
        .unwrap();

    let error = r#"{"code":"compute_fuel_exhausted"}"#;
    set_run_failed(&pool, &run_id, error).await.unwrap();

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
    let run_id = insert_run(&pool, "world-5", "module-e", None, None)
        .await
        .unwrap();

    // Set succeeded first (precondition for applied)
    set_run_succeeded(&pool, &run_id, r#"{"state_delta":[]}"#)
        .await
        .unwrap();

    let accepted_at = "2026-07-31T12:00:00+00:00";
    let mut tx = pool.begin().await.unwrap();
    set_run_applied_in_tx(&mut tx, &run_id, accepted_at)
        .await
        .unwrap();
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
    let run_id = insert_run(&pool, "world-6", "module-f", None, None)
        .await
        .unwrap();

    set_run_succeeded(&pool, &run_id, r#"{}"#).await.unwrap();
    set_run_discarded(&pool, &run_id).await.unwrap();

    let row = get_run(&pool, &run_id)
        .await
        .unwrap()
        .expect("row must exist");
    assert_eq!(row.status, RUN_STATUS_DISCARDED);
    assert!(row.updated_at.is_some());
}

// ── list + pagination ───────────────────────────────────────────

#[tokio::test]
async fn list_runs_pagination() {
    let (pool, _dir) = setup_db().await;

    // Insert 3 runs
    let r1 = insert_run(&pool, "world-list", "mod-a", None, None)
        .await
        .unwrap();
    let r2 = insert_run(&pool, "world-list", "mod-a", None, None)
        .await
        .unwrap();
    let r3 = insert_run(&pool, "world-list", "mod-a", None, None)
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

#[tokio::test]
async fn list_runs_filter_by_world_id() {
    let (pool, _dir) = setup_db().await;

    insert_run(&pool, "world-a", "mod-x", None, None)
        .await
        .unwrap();
    insert_run(&pool, "world-b", "mod-x", None, None)
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

    let r1 = insert_run(&pool, "world-s", "mod-x", None, None)
        .await
        .unwrap();
    let r2 = insert_run(&pool, "world-s", "mod-x", None, None)
        .await
        .unwrap();
    set_run_succeeded(&pool, &r2, r#"{}"#).await.unwrap();

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

    insert_run(&pool, "w-1", "mod-x", None, None).await.unwrap();
    insert_run(&pool, "w-2", "mod-x", None, None).await.unwrap();
    insert_run(&pool, "w-3", "mod-x", None, None).await.unwrap();

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

    let run_id = insert_run(&pool, "world-u", "mod-u", None, None)
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
    let run_id = insert_run(&pool, "world-c", "mod-c", None, None)
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
    let direct_ids: Vec<&str> = items.iter().map(|r| r.run_id.as_str()).collect();
    assert!(direct_ids.contains(&run_id.as_str()));
    // Adapter row (NULL run_id) not returned.
    assert_eq!(items.len(), 1);
}

// ── set_run_succeeded clears error_json ─────────────────────────

#[tokio::test]
async fn succeeded_clears_error_json() {
    let (pool, _dir) = setup_db().await;
    let run_id = insert_run(&pool, "world-e", "mod-e", None, None)
        .await
        .unwrap();

    // Mark failed first
    set_run_failed(&pool, &run_id, r#"{"code":"test"}"#)
        .await
        .unwrap();
    let row = get_run(&pool, &run_id).await.unwrap().unwrap();
    assert!(row.error_json.is_some());

    // Then mark succeeded — error should be cleared
    set_run_succeeded(&pool, &run_id, r#"{"ok":true}"#)
        .await
        .unwrap();
    let row = get_run(&pool, &run_id).await.unwrap().unwrap();
    assert_eq!(row.status, RUN_STATUS_SUCCEEDED);
    assert!(row.error_json.is_none());
    assert!(row.proposals_json.is_some());
}
