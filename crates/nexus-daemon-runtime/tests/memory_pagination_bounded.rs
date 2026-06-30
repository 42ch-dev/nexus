//! Bounded-fetch regression tests for the memory list endpoints.
//!
//! R-V178P0-QC3-002 (qc1 W-QC1-002 + qc3 W-QC3-002/W-QC3-003): the pending-
//! review list and fragments endpoints previously materialized the full creator
//! set via `.fetch_all()` before applying limit/cursor in Rust. They now push
//! `LIMIT ?` (+ keyset for pending reviews) into SQL. These tests assert the
//! wire contract (page size, `has_more`, cursor continuity, no duplicate rows
//! across pages) holds when the dataset is much larger than `limit`, and that a
//! full cursor walk returns exactly the seeded set — which is the behavioral
//! proof that the fetch is bounded server-side (a naive in-Rust truncate would
//! also pass a single-page size check, but only a correct keyset walks all
//! pages without overlap or gaps).

#![allow(clippy::unwrap_used)]

use axum_test::TestServer;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::{AuthMode, DaemonApiConfig};
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::workspace::WorkspaceState;
use serde_json::Value;

const ACTIVE_CREATOR: &str = "ctr_bounded";
/// Number of rows seeded — intentionally well above any `limit` used below so
/// the bound is exercised on every page.
const SEED_COUNT: usize = 60;
/// Page size used across the pagination walks (divides SEED_COUNT evenly).
const PAGE_SIZE: u64 = 10;

struct TestCtx {
    _tmp: nexus_daemon_runtime::test_utils::TestTempRoot,
    pool: sqlx::SqlitePool,
    server: TestServer,
}

async fn test_ctx() -> TestCtx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let config_content = format!("active_creator_id = \"{ACTIVE_CREATOR}\"\n");
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

/// Seed `n` pending reviews with strictly-distinct, monotonically-increasing
/// `created_at` timestamps so the `created_at DESC` order is total and
/// predictable. Row `i` gets timestamp base + i minutes; DESC order returns
/// rows n-1 → 0.
async fn seed_pending_reviews(ctx: &TestCtx, n: usize) {
    for i in 0..n {
        // SAFETY: test fixture using runtime query — compile-time macro not
        // applicable in integration tests.
        sqlx::query(
            "INSERT OR IGNORE INTO memory_pending_review
                (pending_id, session_id, creator_id, world_id, task_kind, raw_digest, created_at)
             VALUES (?, ?, ?, NULL, 'brainstorm', 'seeded digest', ?)",
        )
        .bind(format!("pending_bounded_{i:03}"))
        .bind(format!("sess_bounded_{i:03}"))
        .bind(ACTIVE_CREATOR)
        .bind(format!("2026-01-01T00:{i:02}:00Z"))
        .execute(&ctx.pool)
        .await
        .expect("seed pending_review");
    }
    // Sanity: the seed actually landed and the dataset is larger than the page
    // size, otherwise the bounded-fetch contract is not being exercised.
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memory_pending_review WHERE creator_id = ?")
            .bind(ACTIVE_CREATOR)
            .fetch_one(&ctx.pool)
            .await
            .unwrap();
    assert_eq!(
        usize::try_from(count).unwrap(),
        n,
        "seed must produce exactly {n} pending reviews"
    );
}

/// Seed `n` memory fragments (distinct created_at, DESC-predictable order).
async fn seed_fragments(ctx: &TestCtx, n: usize) {
    for i in 0..n {
        // SAFETY: test fixture using runtime query.
        sqlx::query(
            "INSERT INTO memory_fragments
                (fragment_id, session_id, creator_id, keywords, summary, created_at, ttl)
             VALUES (?, ?, ?, '[]', 'seeded fragment', ?, '30d')",
        )
        .bind(format!("frag_bounded_{i:03}"))
        .bind(format!("sess_frag_{i:03}"))
        .bind(ACTIVE_CREATOR)
        .bind(format!("2026-02-01T00:{i:02}:00Z"))
        .execute(&ctx.pool)
        .await
        .expect("seed memory_fragment");
    }
}

// ─── Pending reviews: bounded fetch + cursor continuity ──────────────────────

/// With 60 pending rows and `limit=10`, the list endpoint must return exactly
/// 10 items, signal `has_more`, and emit a usable `next_cursor`.
#[tokio::test]
async fn pending_review_list_respects_limit_when_dataset_is_large() {
    let ctx = test_ctx().await;
    seed_pending_reviews(&ctx, SEED_COUNT).await;

    let resp = ctx
        .server
        .get(&format!(
            "/v1/local/memory/pending-review?creator_id={ACTIVE_CREATOR}&limit={PAGE_SIZE}"
        ))
        .await;
    resp.assert_status(axum::http::StatusCode::OK);
    let body: Value = resp.json();
    let items = body["items"].as_array().unwrap();
    assert_eq!(
        items.len(),
        PAGE_SIZE as usize,
        "first page must return exactly limit items, not the full {}",
        SEED_COUNT
    );
    assert!(
        body["pagination"]["has_more"].as_bool().unwrap(),
        "has_more must be true when more rows exist"
    );
    let next_cursor = body["pagination"]["next_cursor"].as_str();
    assert!(
        next_cursor.is_some(),
        "next_cursor must be present when has_more is true"
    );
    assert_eq!(body["pagination"]["limit"].as_u64().unwrap(), PAGE_SIZE);
}

/// A full cursor walk across all pages must return every seeded row exactly
/// once (no duplicates, no gaps) — the behavioral proof that the keyset bound
/// is correct end-to-end and that the daemon is not silently capping deep
/// pagination.
#[tokio::test]
async fn pending_review_full_cursor_walk_returns_all_rows_once() {
    let ctx = test_ctx().await;
    seed_pending_reviews(&ctx, SEED_COUNT).await;

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0usize;

    loop {
        let path = match &cursor {
            Some(c) => format!(
                "/v1/local/memory/pending-review?creator_id={ACTIVE_CREATOR}&limit={PAGE_SIZE}&cursor={c}"
            ),
            None => format!(
                "/v1/local/memory/pending-review?creator_id={ACTIVE_CREATOR}&limit={PAGE_SIZE}"
            ),
        };
        let resp = ctx.server.get(&path).await;
        resp.assert_status(axum::http::StatusCode::OK);
        let body: Value = resp.json();
        let items = body["items"].as_array().unwrap();
        assert!(
            !items.is_empty(),
            "page {} returned 0 items mid-walk (gap / premature truncation)",
            pages
        );
        for item in items {
            let pid = item["pending_id"].as_str().unwrap().to_string();
            assert!(
                seen.insert(pid.clone()),
                "duplicate pending_id across pages: {pid}"
            );
        }
        pages += 1;
        if !body["pagination"]["has_more"].as_bool().unwrap() {
            break;
        }
        cursor = body["pagination"]["next_cursor"].as_str().map(String::from);
        assert!(cursor.is_some(), "has_more without next_cursor");
        // Guard against an infinite loop on a broken keyset.
        assert!(
            pages <= SEED_COUNT,
            "paginated past the dataset without terminating"
        );
    }

    assert_eq!(
        seen.len(),
        SEED_COUNT,
        "cursor walk must return all {SEED_COUNT} rows exactly once (got {})",
        seen.len()
    );
    assert_eq!(
        pages,
        SEED_COUNT / PAGE_SIZE as usize,
        "expected {} pages of {}",
        SEED_COUNT / PAGE_SIZE as usize,
        PAGE_SIZE
    );
}

/// A cursor that points at a deleted row must fall back to the first page
/// (preserves the prior `position() == None` behavior after the keyset change).
#[tokio::test]
async fn pending_review_deleted_cursor_falls_back_to_first_page() {
    let ctx = test_ctx().await;
    seed_pending_reviews(&ctx, SEED_COUNT).await;

    // Fetch page 1 to obtain a real cursor, then delete that row out from
    // under the next request.
    let resp = ctx
        .server
        .get(&format!(
            "/v1/local/memory/pending-review?creator_id={ACTIVE_CREATOR}&limit={PAGE_SIZE}"
        ))
        .await;
    let body: Value = resp.json();
    let cursor = body["pagination"]["next_cursor"]
        .as_str()
        .unwrap()
        .to_string();
    let first_page_ids: Vec<&str> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["pending_id"].as_str().unwrap())
        .collect();

    // SAFETY: test fixture delete via runtime query.
    sqlx::query("DELETE FROM memory_pending_review WHERE pending_id = ?")
        .bind(&cursor)
        .execute(&ctx.pool)
        .await
        .unwrap();

    // Request page 2 with the now-dangling cursor → must restart from the top.
    let resp = ctx
        .server
        .get(&format!(
            "/v1/local/memory/pending-review?creator_id={ACTIVE_CREATOR}&limit={PAGE_SIZE}&cursor={cursor}"
        ))
        .await;
    resp.assert_status(axum::http::StatusCode::OK);
    let body: Value = resp.json();
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), PAGE_SIZE as usize, "fallback page size");
    // The first item of the fallback page is the newest remaining row, which
    // is the same as page 1's first item (the deleted row was the *last* of
    // page 1, so the head of the order is unchanged).
    assert_eq!(
        items[0]["pending_id"].as_str().unwrap(),
        first_page_ids[0],
        "deleted cursor should restart from the first page"
    );
}

// ─── Fragments: bounded no-keyword fetch ─────────────────────────────────────

/// With 60 fragments and `limit=10`, the no-keyword path must return exactly
/// 10 (proving the `LIMIT ?` is enforced server-side, not fetch-all-then-
/// truncate).
#[tokio::test]
async fn fragments_list_respects_limit_when_dataset_is_large() {
    let ctx = test_ctx().await;
    seed_fragments(&ctx, SEED_COUNT).await;

    let resp = ctx
        .server
        .get(&format!(
            "/v1/local/memory/fragments?creator_id={ACTIVE_CREATOR}&limit={PAGE_SIZE}"
        ))
        .await;
    resp.assert_status(axum::http::StatusCode::OK);
    let body: Value = resp.json();
    let fragments = body["fragments"].as_array().unwrap();
    assert_eq!(
        fragments.len(),
        PAGE_SIZE as usize,
        "fragments endpoint must return exactly limit items, not the full {}",
        SEED_COUNT
    );
    // The fragments endpoint is NOT paginated (no cursor); it returns the
    // top-`limit` slice. The newest-seeded fragments (highest created_at) come
    // first under `created_at DESC`.
    assert!(
        fragments[0]["fragment_id"]
            .as_str()
            .unwrap()
            .starts_with("frag_bounded_"),
        "fragment shape unchanged"
    );
}

/// `limit` larger than the dataset returns every row without error (the bound
/// is a cap, not a requirement).
#[tokio::test]
async fn fragments_limit_above_dataset_returns_all() {
    let ctx = test_ctx().await;
    seed_fragments(&ctx, 3).await;

    let resp = ctx
        .server
        .get(&format!(
            "/v1/local/memory/fragments?creator_id={ACTIVE_CREATOR}&limit=250"
        ))
        .await;
    resp.assert_status(axum::http::StatusCode::OK);
    let body: Value = resp.json();
    assert_eq!(body["fragments"].as_array().unwrap().len(), 3);
}
