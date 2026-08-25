//! Hermetic CLI integration tests — `creator memory pending count` + `review`
//! drain + `--json` on daemon-backed memory verbs (V1.175 P1 Task 4, group 7):
//! end-to-end against a live daemon fixture with hermetic `HOME` (AR-83 #6 /
//! AR-86).
//!
//! Each test seeds pending-review rows directly into the fixture pool, then
//! drives the REAL `nexus42` binary. The review drain loop is exercised with
//! a small queue (single call, `has_more = false`); the count leaf reads the
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
