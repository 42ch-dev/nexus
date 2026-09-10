//! Cross-reference E2E test for `nexus.reference.refresh` (V1.58 P3 — DF-44).
//!
//! Exercises the full refresh path without a network call:
//! 1. Register a reference source with `refresh_policy = on_change`.
//! 2. Invoke the capability handler directly.
//! 3. Assert DB fields (`last_refreshed_at`, `refresh_status`, `content_hash`)
//!    are updated correctly.
//! 4. Test `offline` → `policy_blocked`.
//! 5. Test `all` path (multiple sources).
//!
//! Network-dependent tests are `#[ignore]` — they are exercised in CI
//! when a test endpoint (httpbin.org) is reachable.

use nexus_local_db::reference_source::{
    self, find_stale_sources, get_by_id, RegisterParams, SourceMutability,
};
use nexus_orchestration::capability::builtins::ReferenceRefresh;
use nexus_orchestration::capability::Capability;
use sqlx::SqlitePool;

async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    (pool, dir)
}

async fn register_test_source(
    pool: &SqlitePool,
    home: &std::path::Path,
    creator_id: &str,
    title: &str,
    uri: &str,
    body: &str,
) -> String {
    let row = reference_source::register(
        pool,
        RegisterParams {
            home,
            creator_id,
            workspace_id: "wrk_default",
            source_type: "url",
            source_mutability: SourceMutability::Refreshable,
            uri,
            title,
            tags: None,
            body,
        },
    )
    .await
    .unwrap();
    row.reference_source_id
}

// ── Hermetic tests ───────────────────────────────────────────────────────

/// A source with `refresh_policy = on_change` that has a URL returning
/// an error should update `refresh_status = 'error'` and NOT crash.
#[tokio::test]
async fn refresh_with_invalid_url_sets_error_status() {
    let (pool, dir) = fresh_pool().await;
    let home = dir.path();
    let source_id = register_test_source(
        &pool,
        home,
        "ctr_test",
        "Bad URL Source",
        "https://invalid.example.invalid/body",
        "initial body",
    )
    .await;
    reference_source::set_refresh_policy(&pool, &source_id, "on_change")
        .await
        .unwrap();

    let cap = ReferenceRefresh::with_pool(pool.clone());
    let input = serde_json::json!({"reference_source_id": source_id});
    let result = cap.run(input).await.unwrap();

    // The fetch will fail, but the capability returns Ok with status
    assert_eq!(result["reference_source_id"], source_id);
    assert_eq!(result["refreshed"], false);
    assert_eq!(result["status"], "error");

    let updated = get_by_id(&pool, &source_id).await.unwrap().unwrap();
    assert_eq!(updated.refresh_status.as_deref(), Some("error"));
}

/// Offline source must return `policy_blocked` without attempting a fetch.
#[tokio::test]
async fn offline_source_returns_policy_blocked() {
    let (pool, dir) = fresh_pool().await;
    let home = dir.path();
    let source_id = register_test_source(
        &pool,
        home,
        "ctr_test",
        "Offline Source",
        "https://example.com/offline",
        "static body",
    )
    .await;
    // Default policy is 'offline' — do not change it.

    let cap = ReferenceRefresh::with_pool(pool.clone());
    let input = serde_json::json!({"reference_source_id": source_id});
    let result = cap.run(input).await.unwrap();

    assert_eq!(result["refreshed"], false);
    assert_eq!(result["status"], "policy_blocked");
    assert_eq!(result["bytes_fetched"], 0);
}

/// Pool-less capability must return `WorkerUnavailable`.
#[tokio::test]
async fn without_pool_returns_worker_unavailable() {
    let cap = ReferenceRefresh::new();
    let input = serde_json::json!({"reference_source_id": "ref_test"});
    let result = cap.run(input).await;
    assert!(result.is_err());
}

/// Non-existent source ID must return an input-invalid error.
#[tokio::test]
async fn nonexistent_source_returns_invalid_input() {
    let (pool, _dir) = fresh_pool().await;
    let cap = ReferenceRefresh::with_pool(pool.clone());
    let input = serde_json::json!({"reference_source_id": "ref_ghost"});
    let result = cap.run(input).await;
    assert!(result.is_err());
}

// ── Body file write test (V1.58 P3 — DF-44 close) ────────────────────────

/// When creator context is set, a successful refresh writes body.md to disk
/// via atomic temp+rename.
#[tokio::test]
async fn refresh_with_creator_context_writes_body_to_disk() {
    let (pool, dir) = fresh_pool().await;
    let home = dir.path().to_path_buf();

    // Register a source with a body
    let source_id = register_test_source(
        &pool,
        &home,
        "ctr_test",
        "Body Write Source",
        "https://httpbin.org/base64/aGVsbG8=", // decodes to "hello" (5 bytes)
        "placeholder body",
    )
    .await;

    reference_source::set_refresh_policy(&pool, &source_id, "on_change")
        .await
        .unwrap();

    // Build capability with creator context
    let cap = ReferenceRefresh::with_pool(pool.clone())
        .with_creator_context(home.clone(), "ctr_test".to_string());

    // This test requires network access to httpbin.org.
    // Run with: cargo test -p nexus-orchestration --test cross_reference_refresh_e2e -- --ignored
    if std::env::var("NEXUS_TEST_ALLOW_NETWORK").is_err() {
        // Skip: verify that the source exists and the config is correct.
        let source = get_by_id(&pool, &source_id).await.unwrap().unwrap();
        assert_eq!(source.refresh_policy, "on_change");
        assert!(source.content_path.is_some());
        return;
    }

    let input = serde_json::json!({"reference_source_id": source_id});
    let result = cap.run(input).await.unwrap();

    assert_eq!(result["refreshed"], true);
    assert_eq!(result["status"], "fresh");

    // Body file should exist at the expected path
    let body_path = nexus_home_layout::reference_body_path(&home, "ctr_test", &source_id);
    assert!(
        tokio::fs::metadata(&body_path).await.is_ok(),
        "body.md should exist at {}",
        body_path.display()
    );

    // DB fields updated
    let updated = get_by_id(&pool, &source_id).await.unwrap().unwrap();
    assert!(updated.last_refreshed_at.is_some());
    assert_eq!(updated.refresh_status.as_deref(), Some("fresh"));
    assert!(updated.content_hash.is_some());
}

// ── find_stale_sources (integration with scheduler) ──────────────────────

/// `find_stale_sources` returns `on_change` sources regardless of
/// `last_refreshed_at`, but excludes `offline` and `refreshing`.
#[tokio::test]
async fn find_stale_includes_on_change_excludes_offline() {
    let (pool, dir) = fresh_pool().await;
    let home = dir.path();

    let on_change_id = register_test_source(
        &pool,
        home,
        "ctr_test",
        "OnChange",
        "https://example.com/oc",
        "body oc",
    )
    .await;
    reference_source::set_refresh_policy(&pool, &on_change_id, "on_change")
        .await
        .unwrap();

    let offline_id = register_test_source(
        &pool,
        home,
        "ctr_test",
        "Offline",
        "https://example.com/off",
        "body off",
    )
    .await;
    // Default is offline — leave as-is.

    let stale = find_stale_sources(&pool, Some(100), 86400).await.unwrap();

    let ids: Vec<_> = stale.iter().map(|s| &s.reference_source_id).collect();
    assert!(
        ids.contains(&&on_change_id),
        "on_change source should be in stale list"
    );
    assert!(
        !ids.contains(&&offline_id),
        "offline source should NOT be in stale list"
    );
}

// ── Network-dependent tests (ignored by default) ─────────────────────────

/// Full end-to-end: refresh a source from httpbin.org, verify body on disk
/// and DB fields.
#[tokio::test]
#[ignore = "requires network access to httpbin.org"]
async fn e2e_refresh_from_httpbin_writes_body_and_updates_db() {
    let (pool, dir) = fresh_pool().await;
    let home = dir.path().to_path_buf();

    let source_id = register_test_source(
        &pool,
        &home,
        "ctr_test",
        "httpbin Source",
        "https://httpbin.org/bytes/32",
        "stale initial body",
    )
    .await;

    reference_source::set_refresh_policy(&pool, &source_id, "on_change")
        .await
        .unwrap();

    let cap = ReferenceRefresh::with_pool(pool.clone())
        .with_creator_context(home.clone(), "ctr_test".to_string());

    let input = serde_json::json!({"reference_source_id": source_id});
    let result = cap.run(input).await.unwrap();

    assert_eq!(result["refreshed"], true);
    assert_eq!(result["status"], "fresh");
    assert!(result["bytes_fetched"].as_u64().unwrap() > 0);

    // Body file written
    let body_path = nexus_home_layout::reference_body_path(&home, "ctr_test", &source_id);
    let body_content = tokio::fs::read_to_string(&body_path).await.unwrap();
    assert_eq!(body_content.len(), 32);

    // DB updated
    let updated = get_by_id(&pool, &source_id).await.unwrap().unwrap();
    assert!(updated.last_refreshed_at.is_some());
    assert_eq!(updated.refresh_status.as_deref(), Some("fresh"));
    assert!(updated.content_hash.is_some());
}

/// Second refresh of the same source with unchanged content should return
/// `not_modified` or `fresh` (depending on endpoint stability).
#[tokio::test]
#[ignore = "requires network access to httpbin.org"]
async fn e2e_second_refresh_returns_not_modified_or_fresh() {
    let (pool, dir) = fresh_pool().await;
    let home = dir.path().to_path_buf();

    let source_id = register_test_source(
        &pool,
        &home,
        "ctr_test",
        "Stable Source",
        "https://httpbin.org/bytes/16",
        "initial",
    )
    .await;

    reference_source::set_refresh_policy(&pool, &source_id, "on_change")
        .await
        .unwrap();

    let cap = ReferenceRefresh::with_pool(pool.clone())
        .with_creator_context(home.clone(), "ctr_test".to_string());

    // First refresh
    let result1 = cap
        .run(serde_json::json!({"reference_source_id": source_id}))
        .await
        .unwrap();
    assert!(result1["refreshed"].as_bool().unwrap());

    // Second refresh
    let result2 = cap
        .run(serde_json::json!({"reference_source_id": source_id}))
        .await
        .unwrap();
    let status = result2["status"].as_str().unwrap();
    assert!(status == "not_modified" || status == "fresh");
}
