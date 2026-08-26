//! seeded queue depth. `pending-show` walks list pagination (PR #230 Greptile
//! P1) so an ID past the first 50-row page is still found.
//! seeded queue depth.

mod common;

use common::LiveDaemon;
use nexus_local_db::pending_review::{create_pending_review, PendingReviewRecord};
use std::process::Output;

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// The daemon's memory handlers validate `creator_id` against the `ctr_`
/// prefix (R-V133P4-07), so the fixture's `test_creator` is rejected. Seed a
/// `ctr_`-prefixed creator + rewrite the hermetic config to make it active.
const MEMORY_CREATOR: &str = "ctr_testcreator";

async fn seed_memory_creator(d: &LiveDaemon) {
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES ('ctr_testcreator', 'Memory Test Creator', 'active', datetime('now'), '{}')",
    )
    .execute(&d.pool)
    .await
    .expect("seed memory creator");

    // Rewrite config.toml so the daemon's active-creator read resolves to
    // the ctr_-prefixed creator (the fixture's `test_creator` fails the
    // daemon's creator_id format validation).
    let config_path = d.home.path().join(".nexus42").join("config.toml");
    let existing = std::fs::read_to_string(&config_path).expect("read config");
    let daemon_url = existing
        .lines()
        .find_map(|l| l.strip_prefix("daemon_url = "))
        .map(str::to_string)
        .expect("daemon_url in config");
    let config = format!(
        "active_creator_id = \"{MEMORY_CREATOR}\"\n\
         daemon_url = {daemon_url}\n\
         \n\
         [active_workspace_slug_by_creator]\n\
         \"{MEMORY_CREATOR}\" = \"default\"\n"
    );
    std::fs::write(&config_path, config).expect("rewrite config");
}

/// Seed `n` pending-review rows for [`MEMORY_CREATOR`].
async fn seed_pending(d: &LiveDaemon, n: usize) {
    for i in 0..n {
        let record = PendingReviewRecord {
            pending_id: format!("pending_test_{i}"),
            session_id: format!("sess_test_{i}"),
            creator_id: MEMORY_CREATOR.to_string(),
            world_id: Some("wld_test_world".to_string()),
            task_kind: "research".to_string(),
            raw_digest: format!(
                "Research digest for pending review {i}: a sufficiently long body of \
                 informational content that classifies as FragmentOnly for research tasks."
            ),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        create_pending_review(&d.pool, &record)
            .await
            .expect("seed pending review");
    }
}

/// Seed `n` pending-review rows with strictly-decreasing `created_at` so the
/// daemon's `created_at DESC` page order is deterministic (row `0` newest).
async fn seed_pending_desc(d: &LiveDaemon, n: usize) {
    let base = chrono::Utc::now();
    for i in 0..n {
        // `i` is bounded by the test's seed count (60), far below `i64::MAX`.
        let minutes_back = i64::try_from(i).expect("seed count fits in i64");
        let record = PendingReviewRecord {
            pending_id: format!("pending_test_{i}"),
            session_id: format!("sess_test_{i}"),
            creator_id: MEMORY_CREATOR.to_string(),
            world_id: Some("wld_test_world".to_string()),
            task_kind: "research".to_string(),
            raw_digest: format!(
                "Research digest for pending review {i}: a sufficiently long body of \
                 informational content that counts as FragmentOnly for research tasks."
            ),
            created_at: (base - chrono::Duration::minutes(minutes_back)).to_rfc3339(),
        };
        create_pending_review(&d.pool, &record)
            .await
            .expect("seed pending review");
    }
}

// ── pending count ─────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_count_reports_seeded_depth() {
    let d = LiveDaemon::start().await;
    seed_memory_creator(&d).await;
    seed_pending(&d, 3).await;

    let out = d.cli(&["creator", "memory", "pending", "count"]).await;
    assert!(out.status.success(), "count failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("3 pending review(s)"), "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_count_json_emits_dto_verbatim() {
    let d = LiveDaemon::start().await;
    seed_memory_creator(&d).await;
    seed_pending(&d, 2).await;

    let out = d
        .cli(&["creator", "memory", "pending", "count", "--json"])
        .await;
    assert!(out.status.success(), "count failed: {}", stderr(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert_eq!(parsed["count"], 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_count_zero_when_empty() {
    let d = LiveDaemon::start().await;
    seed_memory_creator(&d).await;
    let out = d.cli(&["creator", "memory", "pending", "count"]).await;
    assert!(out.status.success(), "count failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("0 pending review(s)"), "{text}");
}

// ── review drain ──────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn review_drains_small_queue() {
    let d = LiveDaemon::start().await;
    seed_memory_creator(&d).await;
    seed_pending(&d, 2).await;

    let out = d.cli(&["creator", "memory", "review"]).await;
    assert!(out.status.success(), "review failed: {}", stderr(&out));
    let text = stdout(&out);
    // Research digests classify as FragmentOnly → fragmented count ≥ 1.
    assert!(text.contains("fragmented="), "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn review_json_emits_cumulative_report() {
    let d = LiveDaemon::start().await;
    seed_memory_creator(&d).await;
    seed_pending(&d, 1).await;

    let out = d.cli(&["creator", "memory", "review", "--json"]).await;
    assert!(out.status.success(), "review failed: {}", stderr(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert!(parsed["fragmented"].as_i64().unwrap_or(0) >= 1);
    assert_eq!(parsed["has_more"], false);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn review_empty_queue_prints_no_pending() {
    let d = LiveDaemon::start().await;
    seed_memory_creator(&d).await;
    let out = d.cli(&["creator", "memory", "review"]).await;
    assert!(out.status.success(), "review failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("No pending memories"), "{text}");
}

// ── pending-list --json ────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_list_json_emits_dto_verbatim() {
    let d = LiveDaemon::start().await;
    seed_memory_creator(&d).await;
    seed_pending(&d, 1).await;

    let out = d
        .cli(&["creator", "memory", "pending-list", "--json"])
        .await;
    assert!(
        out.status.success(),
        "pending-list failed: {}",
        stderr(&out)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    let items = parsed["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["pending_id"], "pending_test_0");
}

// ── fragments --json (AR-83 #3: wire DTO verbatim) ───────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fragments_json_emits_wrapper_dto() {
    let d = LiveDaemon::start().await;
    seed_memory_creator(&d).await;
    sqlx::query(
        "INSERT INTO memory_fragments \
         (fragment_id, session_id, creator_id, keywords, summary, created_at, ttl) \
         VALUES ('frag_test_1', 'sess_test_1', 'ctr_testcreator', '[]', \
                 'A test fragment summary', datetime('now'), NULL)",
    )
    .execute(&d.pool)
    .await
    .expect("seed memory fragment");

    let out = d.cli(&["creator", "memory", "fragments", "--json"]).await;
    assert!(out.status.success(), "fragments failed: {}", stderr(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    // The wire shape is `{ "fragments": [ … ] }` — the wrapper, not a bare array.
    let fragments = parsed["fragments"]
        .as_array()
        .expect("fragments wrapper array");
    assert_eq!(fragments.len(), 1);
    assert_eq!(fragments[0]["fragment_id"], "frag_test_1");
    assert_eq!(fragments[0]["summary"], "A test fragment summary");
}

// ── pending-show pagination walk (PR #230 Greptile P1) ────────────────────

/// `pending-show` must find an ID past the first page (50-row default list
/// limit): the leaf walks `pagination.next_cursor` until the ID is found or
/// the pages are exhausted instead of reporting not-found from page 1.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_show_finds_id_beyond_first_page() {
    let d = LiveDaemon::start().await;
    seed_memory_creator(&d).await;
    // Row `pending_test_0` is newest → it lands on page 1; row 55 lands on
    // page 2 (newest-first DESC order, 50 rows/page).
    seed_pending_desc(&d, 60).await;

    let out = d
        .cli(&["creator", "memory", "pending-show", "pending_test_55"])
        .await;
    assert!(
        out.status.success(),
        "pending-show failed: {}",
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("pending_id: pending_test_55"), "{text}");
    assert!(text.contains("sess_test_55"), "{text}");
}

/// The pagination walk must terminate with the not-found error when the ID
/// does not exist anywhere (bounded loop — no infinite page following).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_show_missing_id_past_first_page_fails_closed() {
    let d = LiveDaemon::start().await;
    seed_memory_creator(&d).await;
    seed_pending_desc(&d, 60).await;

    let out = d
        .cli(&[
            "creator",
            "memory",
            "pending-show",
            "pending_does_not_exist",
        ])
        .await;
    assert!(!out.status.success(), "missing pending-show must fail");
    let err = stderr(&out);
    assert!(
        err.contains("not found"),
        "stderr should surface not-found: {err}"
    );
}
