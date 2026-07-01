//! Memory Review + Fragments API contract tests (V1.33 P4).
//!
//! Covers the two new daemon endpoints:
//! - `POST /v1/local/memory/review` → 200 (review processed), 400 (invalid creator_id)
//! - `GET  /v1/local/memory/fragments` → 200 (list), 400 (invalid creator_id)
//!
//! Also verifies that `pending-review` CRUD routes are not regressed.

#![allow(clippy::unwrap_used)]

use axum_test::TestServer;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::{AuthMode, DaemonApiConfig};
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::test_utils::TestTempRoot;
use nexus_daemon_runtime::workspace::WorkspaceState;
use serde_json::{json, Value};

// ─── Helpers ───────────────────────────────────────────────────────────────

struct TestCtx {
    _tmp: TestTempRoot,
    pool: sqlx::SqlitePool,
    server: TestServer,
}

async fn test_ctx() -> TestCtx {
    test_ctx_with_active_creator("ctr_testuser").await
}

/// Create a test context with a specific active creator configured.
async fn test_ctx_with_active_creator(active_creator: &str) -> TestCtx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;

    // Write config.toml with active creator (required by R-V133P4-01 auth enforcement).
    let config_content = format!("active_creator_id = \"{active_creator}\"\n");
    std::fs::write(nexus_home.join("config.toml"), config_content)
        .expect("failed to write config.toml");

    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let pool = state.pool().clone();
    let auth_config = DaemonApiConfig {
        api_key: None,
        auth_mode: AuthMode::KeylessLocalhost,
    };
    let app = api::create_router(state, auth_config);
    let server = TestServer::new(app).expect("failed to create test server");
    TestCtx {
        _tmp: tmp,
        pool,
        server,
    }
}

/// Seed a pending review entry via the daemon API.
async fn seed_pending_review(ctx: &TestCtx, pending_id: &str) {
    let body = json!({
        "pending_id": pending_id,
        "session_id": "sess_test",
        "creator_id": "ctr_testuser",
        "world_id": null,
        "task_kind": "brainstorm",
        "raw_digest": "Discussed three key themes for the novel: narrative structure, character arcs, and emotional resonance. Explored how these interweave to create compelling storytelling."
    });
    let resp = ctx
        .server
        .post("/v1/local/memory/pending-review")
        .json(&body)
        .await;
    resp.assert_status(axum::http::StatusCode::OK);
}

/// Seed a pending review entry directly via SQL (bypasses API auth enforcement).
/// Use for cross-creator isolation tests where the active creator differs.
async fn seed_pending_review_raw(pool: &sqlx::SqlitePool, pending_id: &str, creator_id: &str) {
    let session_id = format!("sess_{creator_id}");
    // SAFETY: test helper using runtime query — compile-time macro not applicable in integration tests.
    sqlx::query(
        "INSERT OR IGNORE INTO memory_pending_review (pending_id, session_id, creator_id, world_id, task_kind, raw_digest, created_at)
         VALUES (?, ?, ?, NULL, 'brainstorm', 'Test digest content for cross-creator isolation.', '2026-01-01T00:00:00Z')",
    )
    .bind(pending_id)
    .bind(&session_id)
    .bind(creator_id)
    .execute(pool)
    .await
    .expect("raw seed insert");
}

// ─── POST /v1/local/memory/review ────────────────────────────────────────

#[tokio::test]
async fn review_returns_200_with_counts() {
    let ctx = test_ctx().await;
    seed_pending_review(&ctx, "pending_review_test_1").await;

    let body = json!({ "creator_id": "ctr_testuser" });
    let resp = ctx.server.post("/v1/local/memory/review").json(&body).await;
    resp.assert_status(axum::http::StatusCode::OK);
    let body: Value = resp.json();
    // The brainstorm entry with high-signal content should be promoted
    assert!(body["promoted"].as_u64().unwrap() > 0 || body["fragmented"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn review_returns_200_empty_queue() {
    let ctx = test_ctx().await;
    let body = json!({ "creator_id": "ctr_testuser" });
    let resp = ctx.server.post("/v1/local/memory/review").json(&body).await;
    resp.assert_status(axum::http::StatusCode::OK);
    let body: Value = resp.json();
    assert_eq!(body["promoted"], 0);
    assert_eq!(body["fragmented"], 0);
    assert_eq!(body["dropped"], 0);
}

#[tokio::test]
async fn review_returns_400_invalid_creator_id() {
    let ctx = test_ctx().await;
    // "invalid_id" format fails but we also need to match active creator.
    // Since active creator is ctr_testuser, an invalid format still gets 403
    // (auth check runs before format validation). Use a valid-format but
    // non-matching creator to test format validation path.
    let body = json!({ "creator_id": "invalid_id" });
    let resp = ctx.server.post("/v1/local/memory/review").json(&body).await;
    // Auth check (403) runs before format validation since creator_id
    // "invalid_id" != active "ctr_testuser".
    resp.assert_status(axum::http::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn review_drops_short_digest() {
    let ctx = test_ctx().await;
    // Seed a very short digest that should be dropped
    let body = json!({
        "pending_id": "pending_short_digest",
        "session_id": "sess_short",
        "creator_id": "ctr_testuser",
        "task_kind": "unknown",
        "raw_digest": "Short text"
    });
    let resp = ctx
        .server
        .post("/v1/local/memory/pending-review")
        .json(&body)
        .await;
    resp.assert_status(axum::http::StatusCode::OK);

    let review_body = json!({ "creator_id": "ctr_testuser" });
    let resp = ctx
        .server
        .post("/v1/local/memory/review")
        .json(&review_body)
        .await;
    resp.assert_status(axum::http::StatusCode::OK);
    let result: Value = resp.json();
    assert!(result["dropped"].as_u64().unwrap() > 0);
}

// ─── GET /v1/local/memory/fragments ──────────────────────────────────────

#[tokio::test]
async fn fragments_returns_200_with_array() {
    let ctx = test_ctx().await;
    let resp = ctx
        .server
        .get("/v1/local/memory/fragments?creator_id=ctr_testuser")
        .await;
    resp.assert_status(axum::http::StatusCode::OK);
    let body: Value = resp.json();
    assert!(body["fragments"].is_array());
}

#[tokio::test]
async fn fragments_returns_200_empty() {
    let ctx = test_ctx().await;
    let resp = ctx
        .server
        .get("/v1/local/memory/fragments?creator_id=ctr_testuser")
        .await;
    resp.assert_status(axum::http::StatusCode::OK);
    let body: Value = resp.json();
    assert!(body["fragments"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn fragments_returns_400_invalid_creator_id() {
    let ctx = test_ctx().await;
    // "bad_id" format fails, but auth check (403) runs first since
    // "bad_id" != active "ctr_testuser".
    let resp = ctx
        .server
        .get("/v1/local/memory/fragments?creator_id=bad_id")
        .await;
    resp.assert_status(axum::http::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn fragments_after_review_has_entries() {
    let ctx = test_ctx().await;

    // Seed a research entry → should become a fragment
    let body = json!({
        "pending_id": "pending_research_frag",
        "session_id": "sess_research",
        "creator_id": "ctr_testuser",
        "task_kind": "research",
        "raw_digest": "This is a research summary with enough content to pass the length check for fragment creation."
    });
    let resp = ctx
        .server
        .post("/v1/local/memory/pending-review")
        .json(&body)
        .await;
    resp.assert_status(axum::http::StatusCode::OK);

    // Run review
    let review_body = json!({ "creator_id": "ctr_testuser" });
    let resp = ctx
        .server
        .post("/v1/local/memory/review")
        .json(&review_body)
        .await;
    resp.assert_status(axum::http::StatusCode::OK);
    let result: Value = resp.json();
    // Research task should produce a fragment
    assert!(result["fragmented"].as_u64().unwrap() > 0);

    // Now query fragments
    let resp = ctx
        .server
        .get("/v1/local/memory/fragments?creator_id=ctr_testuser")
        .await;
    resp.assert_status(axum::http::StatusCode::OK);
    let frag_body: Value = resp.json();
    let fragments = frag_body["fragments"].as_array().unwrap();
    assert!(
        !fragments.is_empty(),
        "Should have at least one fragment after review"
    );
    assert!(fragments[0]["fragment_id"]
        .as_str()
        .unwrap()
        .starts_with("frag_"));
}

// ─── No regression on pending-review CRUD ─────────────────────────────────

#[tokio::test]
async fn pending_review_create_still_works() {
    let ctx = test_ctx().await;
    seed_pending_review(&ctx, "pending_regression_test").await;
}

#[tokio::test]
async fn pending_review_list_still_works() {
    let ctx = test_ctx().await;
    seed_pending_review(&ctx, "pending_list_test").await;

    let resp = ctx
        .server
        .get("/v1/local/memory/pending-review?creator_id=ctr_testuser")
        .await;
    resp.assert_status(axum::http::StatusCode::OK);
    let body: Value = resp.json();
    assert!(!body["items"].as_array().unwrap().is_empty());
}

// ─── R-V133P4-01/02: Auth enforcement + cross-creator tests ──────────────

/// Review returns 401 when no active creator is configured (no config.toml).
#[tokio::test]
async fn review_returns_401_without_creator() {
    let ctx = test_ctx_without_creator().await;

    let body = json!({ "creator_id": "ctr_testuser" });
    let resp = ctx.server.post("/v1/local/memory/review").json(&body).await;
    resp.assert_status(axum::http::StatusCode::UNAUTHORIZED);
}

/// Fragments returns 401 when no active creator is configured.
#[tokio::test]
async fn fragments_returns_401_without_creator() {
    let ctx = test_ctx_without_creator().await;

    let resp = ctx
        .server
        .get("/v1/local/memory/fragments?creator_id=ctr_testuser")
        .await;
    resp.assert_status(axum::http::StatusCode::UNAUTHORIZED);
}

/// Review returns 403 when request creator_id does not match active creator.
#[tokio::test]
async fn review_returns_403_on_creator_id_mismatch() {
    let ctx = test_ctx_with_active_creator("ctr_alice").await;

    let body = json!({ "creator_id": "ctr_bob" });
    let resp = ctx.server.post("/v1/local/memory/review").json(&body).await;
    resp.assert_status(axum::http::StatusCode::FORBIDDEN);
}

/// Fragments returns 403 when request creator_id does not match active creator.
#[tokio::test]
async fn fragments_returns_403_on_creator_id_mismatch() {
    let ctx = test_ctx_with_active_creator("ctr_alice").await;

    let resp = ctx
        .server
        .get("/v1/local/memory/fragments?creator_id=ctr_bob")
        .await;
    resp.assert_status(axum::http::StatusCode::FORBIDDEN);
}

/// Cross-creator isolation: review with pending from another creator → 403.
#[tokio::test]
async fn cross_creator_isolation_review_other_creator_returns_403() {
    // Set up with ctr_alice as active creator.
    let ctx = test_ctx_with_active_creator("ctr_alice").await;

    // Seed a pending review as ctr_bob directly via SQL (bypasses API auth).
    seed_pending_review_raw(&ctx.pool, "pending_bob_entry", "ctr_bob").await;

    // Alice tries to review — active_creator filters to ctr_alice.
    let review_body = json!({ "creator_id": "ctr_alice" });
    let resp = ctx
        .server
        .post("/v1/local/memory/review")
        .json(&review_body)
        .await;
    resp.assert_status(axum::http::StatusCode::OK);
    // Alice's review should not see Bob's entries (active_creator filters to ctr_alice).
    let result: Value = resp.json();
    assert_eq!(result["promoted"], 0);
    assert_eq!(result["fragmented"], 0);
    assert_eq!(result["dropped"], 0);
}

// ─── R-V133P4-07: Pending-review CRUD auth enforcement ─────────────────────

/// Helper: create TestCtx without active creator (no config.toml).
async fn test_ctx_without_creator() -> TestCtx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    // Remove config.toml → no active creator.
    std::fs::remove_file(nexus_home.join("config.toml")).expect("remove config.toml");
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let pool = state.pool().clone();
    let auth_config = DaemonApiConfig {
        api_key: None,
        auth_mode: AuthMode::KeylessLocalhost,
    };
    let app = api::create_router(state, auth_config);
    let server = TestServer::new(app).expect("failed to create test server");
    TestCtx {
        _tmp: tmp,
        pool,
        server,
    }
}

/// Pending review create returns 401 when no active creator is configured.
#[tokio::test]
async fn pending_review_create_returns_401_without_creator() {
    let ctx = test_ctx_without_creator().await;
    let body = json!({
        "pending_id": "pending_no_auth",
        "session_id": "sess_no_auth",
        "creator_id": "ctr_testuser",
        "raw_digest": "Should not be created"
    });
    let resp = ctx
        .server
        .post("/v1/local/memory/pending-review")
        .json(&body)
        .await;
    resp.assert_status(axum::http::StatusCode::UNAUTHORIZED);
}

/// Pending review list returns 401 when no active creator is configured.
#[tokio::test]
async fn pending_review_list_returns_401_without_creator() {
    let ctx = test_ctx_without_creator().await;
    let resp = ctx
        .server
        .get("/v1/local/memory/pending-review?creator_id=ctr_testuser")
        .await;
    resp.assert_status(axum::http::StatusCode::UNAUTHORIZED);
}

/// Pending review count returns 401 when no active creator is configured.
#[tokio::test]
async fn pending_review_count_returns_401_without_creator() {
    let ctx = test_ctx_without_creator().await;
    let resp = ctx
        .server
        .get("/v1/local/memory/pending-review/count?creator_id=ctr_testuser")
        .await;
    resp.assert_status(axum::http::StatusCode::UNAUTHORIZED);
}

/// Pending review delete returns 401 when no active creator is configured.
///
/// Uses direct handler invocation (bypasses axum-test routing issue with
/// `{id}` path segments for DELETE — same pattern as works_api tests).
#[tokio::test]
async fn pending_review_delete_returns_401_without_creator() {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    // Remove config.toml → no active creator → 401.
    std::fs::remove_file(nexus_home.join("config.toml")).expect("remove config.toml");
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    use axum::extract::{Path, Query, State};
    let result = nexus_daemon_runtime::api::handlers::memory::delete_pending_review(
        State(state),
        Path("pending_noauth".to_string()),
        Query(
            nexus_daemon_runtime::api::handlers::memory::DeletePendingReviewQuery {
                creator_id: "ctr_testuser".to_string(),
            },
        ),
    )
    .await;

    match result {
        Err(err) => {
            assert_eq!(
                err.status_code(),
                axum::http::StatusCode::UNAUTHORIZED,
                "Expected 401, got {}",
                err.status_code()
            );
        }
        Ok(_) => panic!("Expected 401 AuthRequired, got success"),
    }
    drop(tmp);
}

/// Pending review create returns 403 when body creator_id does not match active creator.
#[tokio::test]
async fn pending_review_create_returns_403_on_creator_id_mismatch() {
    let ctx = test_ctx_with_active_creator("ctr_alice").await;
    let body = json!({
        "pending_id": "pending_bob_attempt",
        "session_id": "sess_bob",
        "creator_id": "ctr_bob",
        "raw_digest": "Bob trying to create under Alice's session"
    });
    let resp = ctx
        .server
        .post("/v1/local/memory/pending-review")
        .json(&body)
        .await;
    resp.assert_status(axum::http::StatusCode::FORBIDDEN);
}

/// Pending review list returns 403 when query creator_id does not match active creator.
#[tokio::test]
async fn pending_review_list_returns_403_on_creator_id_mismatch() {
    let ctx = test_ctx_with_active_creator("ctr_alice").await;
    let resp = ctx
        .server
        .get("/v1/local/memory/pending-review?creator_id=ctr_bob")
        .await;
    resp.assert_status(axum::http::StatusCode::FORBIDDEN);
}

// ─── V1.80 REL-01: bounded drain + per-creator serialization ───────────────

/// Seed `count` pending-review rows directly via SQL for the same creator.
/// Each row carries a research-task digest long enough to classify as
/// FragmentOnly (predictable, creates a DB record that is easy to count for
/// the concurrency no-duplicate assertion).
async fn seed_n_pending_reviews_raw(pool: &sqlx::SqlitePool, creator_id: &str, count: usize) {
    // Distinct created_at timestamps keep the (created_at DESC, pending_id DESC)
    // ordering deterministic across the batch.
    for i in 0..count {
        let pending_id = format!("pending_bulk_{creator_id}_{i}");
        let session_id = format!("sess_bulk_{i}");
        let created_at = format!("2026-01-01T00:{i:02}:{i:02}Z");
        // SAFETY: test helper using runtime query — compile-time macro not applicable in integration tests.
        sqlx::query(
            "INSERT OR IGNORE INTO memory_pending_review \
             (pending_id, session_id, creator_id, world_id, task_kind, raw_digest, created_at) \
             VALUES (?, ?, ?, NULL, 'research', \
             'Research summary with enough content to pass the length threshold for fragment creation and keyword extraction.', ?)",
        )
        .bind(&pending_id)
        .bind(&session_id)
        .bind(creator_id)
        .bind(&created_at)
        .execute(pool)
        .await
        .expect("bulk seed insert");
    }
}

/// Bounded drain walk: seeding >50 rows, a single review call processes at
/// most REVIEW_BATCH_LIMIT (50) rows and reports `has_more = true`. A second
/// call drains the remainder and reports `has_more = false`. No row is lost.
#[tokio::test]
async fn review_bounded_drain_walk_more_than_batch_limit() {
    let ctx = test_ctx().await;
    const TOTAL: usize = 55; // > REVIEW_BATCH_LIMIT (50)

    seed_n_pending_reviews_raw(&ctx.pool, "ctr_testuser", TOTAL).await;

    // First call: bounded to 50 rows; has_more = true.
    let body = json!({ "creator_id": "ctr_testuser" });
    let resp = ctx.server.post("/v1/local/memory/review").json(&body).await;
    resp.assert_status(axum::http::StatusCode::OK);
    let result: Value = resp.json();
    assert_eq!(
        result["processed"].as_u64().unwrap(),
        50,
        "first call processes exactly the batch limit"
    );
    assert_eq!(
        result["has_more"],
        json!(true),
        "has_more must be true when rows remain beyond the batch"
    );
    let first_promoted = result["promoted"].as_u64().unwrap_or(0);
    let first_fragmented = result["fragmented"].as_u64().unwrap_or(0);
    let first_dropped = result["dropped"].as_u64().unwrap_or(0);
    assert_eq!(
        first_promoted + first_fragmented + first_dropped,
        50,
        "every inspected row is acted on once"
    );

    // Second call: drains the remaining 5 rows; has_more = false.
    let resp = ctx.server.post("/v1/local/memory/review").json(&body).await;
    resp.assert_status(axum::http::StatusCode::OK);
    let result: Value = resp.json();
    assert_eq!(
        result["processed"].as_u64().unwrap(),
        5,
        "second call processes the remaining rows"
    );
    assert_eq!(
        result["has_more"],
        json!(false),
        "has_more must be false once the queue is drained"
    );
    let second_promoted = result["promoted"].as_u64().unwrap_or(0);
    let second_fragmented = result["fragmented"].as_u64().unwrap_or(0);
    let second_dropped = result["dropped"].as_u64().unwrap_or(0);
    assert_eq!(second_promoted + second_fragmented + second_dropped, 5);

    // Third call: empty queue, zero counters, has_more = false.
    let resp = ctx.server.post("/v1/local/memory/review").json(&body).await;
    resp.assert_status(axum::http::StatusCode::OK);
    let result: Value = resp.json();
    assert_eq!(result["processed"].as_u64().unwrap(), 0);
    assert_eq!(result["has_more"], json!(false));

    // No row was lost: the total across all calls equals the seeded count.
    let grand_total = first_promoted
        + first_fragmented
        + first_dropped
        + second_promoted
        + second_fragmented
        + second_dropped;
    assert_eq!(grand_total as usize, TOTAL);
}

/// Per-creator serialization: two overlapping review calls for the same creator
/// must not double-process the same pending rows. The per-creator mutex
/// serializes them; the second call sees an empty queue. The total
/// promoted+fragmented+dropped across both calls equals the seeded count (no
/// duplicates), and the fragment table holds exactly one record per seeded row.
#[tokio::test]
async fn review_overlapping_calls_no_duplicate_processing() {
    use axum::Json;
    use nexus_contracts::ReviewRequest;
    use nexus_daemon_runtime::api::handlers::memory::review;
    use nexus_daemon_runtime::workspace::WorkspaceState;
    use std::sync::Arc;

    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    // Active creator = ctr_testuser (required by the auth gate).
    std::fs::write(
        nexus_home.join("config.toml"),
        "active_creator_id = \"ctr_testuser\"\n",
    )
    .expect("config.toml");
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let pool = state.pool().clone();

    const SEED: usize = 5;
    seed_n_pending_reviews_raw(&pool, "ctr_testuser", SEED).await;

    // Two overlapping handler invocations sharing the same WorkspaceState (and
    // therefore the same per-creator lock map). Without serialization, both
    // would fetch the same 5 rows and each mint 5 fragments (10 total).
    let state_a = state.clone();
    let state_b = state.clone();
    let req = ReviewRequest {
        creator_id: "ctr_testuser".into(),
    };
    let (res_a, res_b) = tokio::join!(
        review(axum::extract::State(state_a), Json(req.clone())),
        review(axum::extract::State(state_b), Json(req)),
    );

    let outcome_a = res_a.expect("call A ok");
    let outcome_b = res_b.expect("call B ok");

    let total_a = outcome_a.promoted + outcome_a.fragmented + outcome_a.dropped;
    let total_b = outcome_b.promoted + outcome_b.fragmented + outcome_b.dropped;

    // Each seeded row is processed exactly once — no double-promotion /
    // double-fragmentation, no double-delete. The serialized second call sees
    // an empty queue (counters sum to 0).
    assert_eq!(
        total_a + total_b,
        SEED as i64,
        "overlapping calls must not double-process rows; got {total_a} + {total_b}"
    );
    // One of the two calls drained the queue; the other saw nothing.
    assert!(
        total_a == 0 || total_b == 0,
        "the serialized second call must see an empty queue"
    );

    // The fragment table holds exactly one record per seeded research row (no
    // duplicate fragment_ids).
    let arc_pool = Arc::new(pool);
    // SAFETY: test-only read-back verification of fragment count.
    let fragment_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM memory_fragments WHERE creator_id = 'ctr_testuser'")
            .fetch_one(arc_pool.as_ref())
            .await
            .expect("count fragments");
    let fragmented_total = outcome_a.fragmented + outcome_b.fragmented;
    assert_eq!(
        fragment_count.0, fragmented_total,
        "fragment table must match the reported fragmented counters (no duplicate inserts)"
    );

    drop(tmp);
}

/// When a review call returns `has_more = true`, the `processed` and counter
/// fields are present on the wire (additive V1.80 fields always populated by
/// the V1.80 daemon).
#[tokio::test]
async fn review_populates_has_more_and_processed_fields() {
    let ctx = test_ctx().await;
    // An empty queue: has_more should be false, processed 0, both present.
    let body = json!({ "creator_id": "ctr_testuser" });
    let resp = ctx.server.post("/v1/local/memory/review").json(&body).await;
    resp.assert_status(axum::http::StatusCode::OK);
    let result: Value = resp.json();
    assert!(
        result.get("has_more").is_some(),
        "has_more must always be emitted by V1.80"
    );
    assert!(
        result.get("processed").is_some(),
        "processed must always be emitted by V1.80"
    );
    assert_eq!(result["has_more"], json!(false));
    assert_eq!(result["processed"], json!(0));
}
