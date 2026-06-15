//! Hermetic integration tests for the master-decision timeout watcher
//! (V1.39 P4 T5).
//!
//! Drives [`stale_findings_watcher::run_one_sweep`] deterministically
//! against a freshly-migrated DB, asserting the AC1..AC3 contract from
//! `.mstar/plans/2026-06-09-v1.39-master-decision-timeout.md`:
//!
//! - AC1: a stale open finding produced past the threshold is detected
//!   (the sweep does not panic and returns successfully).
//! - AC2: the sweep is non-blocking and tolerates DB-level edge cases
//!   (missing Work) without crashing.
//! - AC3: a new `novel-review-master` schedule is created **only** when
//!   the owning Work has explicitly opted in via
//!   `auto_review_master_on_timeout = true`. The default flag value of
//!   `false` produces **no** schedule.
//!
//! Hermetic: each test seeds its own DB and uses a tiny threshold (60s)
//! so we can drive a stale row by setting its `created_at` to
//! `now - 7200`. No env-var manipulation needed because `run_one_sweep`
//! accepts the threshold as a parameter — the mocked clock is the
//! synthetic `created_at` value on each finding.

#![allow(clippy::unwrap_used)]

use nexus_daemon_runtime::stale_findings_watcher::run_one_sweep;
use nexus_daemon_runtime::test_utils;
use nexus_local_db::findings::{create_finding, Finding};
use nexus_local_db::works::{create_work_atomic, WorkRecord};
use sqlx::{Row, SqlitePool};

const TEST_CREATOR: &str = "ctr_p4_t5";
const TEST_WORKSPACE: &str = "ws";

// ─── Helpers ───────────────────────────────────────────────────────────────

async fn fresh_pool() -> (SqlitePool, test_utils::TestTempRoot) {
    let (tmp, _nexus_home, db_path) = test_utils::create_test_workspace().await;
    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    (pool, tmp)
}

async fn seed_work(pool: &SqlitePool, work_id: &str, opted_in: bool) {
    let record = WorkRecord {
        work_id: work_id.to_string(),
        creator_id: TEST_CREATOR.to_string(),
        workspace_slug: TEST_WORKSPACE.to_string(),
        status: "active".to_string(),
        title: "P4 T5 Work".to_string(),
        long_term_goal: "Cover master-decision timeout".to_string(),
        initial_idea: "A test idea".to_string(),
        creative_brief: None,
        intake_status: "complete".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-06-09T10:00:00Z".to_string(),
        updated_at: "2026-06-09T10:00:00Z".to_string(),
        current_stage: "research".to_string(),
        stage_status: "active".to_string(),
        work_profile: Some("novel".to_string()),
        work_ref: Some("p4-t5".to_string()),
        total_planned_chapters: None,
        current_chapter: 0,
        auto_chain_enabled: true,
        driver_schedule_id: None,
        auto_chain_interrupted: false,
        auto_review_master_on_timeout: opted_in,
        runtime_lock_holder: None,
        runtime_lock_acquired_at: None,
        completion_locked_at: None,
        novel_completion_status: None,
        lineage_from_work_id: None,
    };
    let _ = create_work_atomic(pool, &record, None).await.unwrap();
}

async fn seed_open_finding(pool: &SqlitePool, finding_id: &str, work_id: &str, age_seconds: i64) {
    let now = chrono::Utc::now().timestamp();
    let f = Finding {
        finding_id: finding_id.to_string(),
        work_id: work_id.to_string(),
        chapter: None,
        severity: "major".to_string(),
        status: "open".to_string(),
        title: "T5 stale finding".to_string(),
        description: "Past the master-decision SLA".to_string(),
        target_executor: "master".to_string(),
        creator_id: TEST_CREATOR.to_string(),
        kind: "craft".to_string(),
        rule_suggestion: None,
        created_at: now - age_seconds,
        updated_at: now - age_seconds,
    };
    create_finding(pool, &f).await.unwrap();
}

async fn review_master_schedule_count(pool: &SqlitePool, work_id: &str) -> i64 {
    sqlx::query(
        "SELECT COUNT(*) AS n FROM creator_schedules \
         WHERE preset_id = 'novel-review-master' AND work_id = ?",
    )
    .bind(work_id)
    .fetch_one(pool)
    .await
    .unwrap()
    .get::<i64, _>("n")
}

// ─── Tests ─────────────────────────────────────────────────────────────────

/// AC1 baseline: empty findings table → silent sweep, no schedule rows.
#[tokio::test]
async fn sweep_with_no_findings_is_a_no_op() {
    let (pool, _tmp) = fresh_pool().await;

    run_one_sweep(&pool, 60).await;

    let total: i64 = sqlx::query("SELECT COUNT(*) AS n FROM creator_schedules")
        .fetch_one(&pool)
        .await
        .unwrap()
        .get::<i64, _>("n");
    assert_eq!(total, 0, "no schedules should be created from an empty DB");
}

/// AC3 default-off: a stale finding on a Work that has not opted in must
/// **not** trigger any schedule creation.
#[tokio::test]
async fn stale_finding_without_optin_does_not_enqueue() {
    let (pool, _tmp) = fresh_pool().await;
    let work_id = "wrk_p4t5_default";
    seed_work(&pool, work_id, false).await;
    seed_open_finding(&pool, "fnd_p4t5_default", work_id, 7200).await;

    run_one_sweep(&pool, 60).await;

    assert_eq!(
        review_master_schedule_count(&pool, work_id).await,
        0,
        "default-off Work must not get a novel-review-master schedule"
    );
}

/// AC3 opt-in: a stale finding on an opted-in Work creates exactly one
/// `novel-review-master` schedule, with the expected `preset_id`,
/// `status='pending'`, and an `RVM`-prefixed ID.
#[tokio::test]
async fn stale_finding_with_optin_enqueues_review_master() {
    let (pool, _tmp) = fresh_pool().await;
    let work_id = "wrk_p4t5_optin";
    seed_work(&pool, work_id, true).await;
    seed_open_finding(&pool, "fnd_p4t5_optin", work_id, 7200).await;

    run_one_sweep(&pool, 60).await;

    let row = sqlx::query(
        "SELECT preset_id, status, work_id, schedule_id \
         FROM creator_schedules WHERE work_id = ?",
    )
    .bind(work_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(row.get::<String, _>("preset_id"), "novel-review-master");
    assert_eq!(row.get::<String, _>("status"), "pending");
    assert_eq!(row.get::<String, _>("work_id"), work_id);
    let schedule_id: String = row.get("schedule_id");
    assert!(
        schedule_id.starts_with("RVM"),
        "schedule_id should use RVM prefix: {schedule_id}"
    );
    assert_eq!(
        review_master_schedule_count(&pool, work_id).await,
        1,
        "exactly one schedule per stale finding"
    );
}

/// AC3 mixed: with two stale findings — one opted-in Work, one default —
/// only the opted-in Work gets a schedule.
#[tokio::test]
async fn mixed_optin_only_enqueues_for_opted_in_work() {
    let (pool, _tmp) = fresh_pool().await;
    let opted_in = "wrk_p4t5_yes";
    let default_off = "wrk_p4t5_no";
    seed_work(&pool, opted_in, true).await;
    seed_work(&pool, default_off, false).await;
    seed_open_finding(&pool, "fnd_yes", opted_in, 7200).await;
    seed_open_finding(&pool, "fnd_no", default_off, 7200).await;

    run_one_sweep(&pool, 60).await;

    assert_eq!(
        review_master_schedule_count(&pool, opted_in).await,
        1,
        "opted-in Work must get exactly one schedule"
    );
    assert_eq!(
        review_master_schedule_count(&pool, default_off).await,
        0,
        "default Work must get no schedule"
    );
}

/// AC1 freshness gate: a finding younger than the threshold is not
/// stale and must not trigger an enqueue even when the flag is on.
#[tokio::test]
async fn fresh_finding_does_not_enqueue_even_when_opted_in() {
    let (pool, _tmp) = fresh_pool().await;
    let work_id = "wrk_p4t5_fresh";
    seed_work(&pool, work_id, true).await;
    // Age 5s vs threshold 60s — not stale.
    seed_open_finding(&pool, "fnd_fresh", work_id, 5).await;

    run_one_sweep(&pool, 60).await;

    assert_eq!(
        review_master_schedule_count(&pool, work_id).await,
        0,
        "fresh finding must not enqueue review-master"
    );
}

/// AC1 + AC3 status gate: a stale finding whose status has already moved
/// to `resolved` is no longer "open" and must not be reported as stale or
/// trigger an enqueue, even when the owning Work has opted in. This
/// guards the `list_all_stale_open_findings` filter.
#[tokio::test]
async fn resolved_finding_is_not_stale() {
    let (pool, _tmp) = fresh_pool().await;
    let work_id = "wrk_p4t5_resolved";
    seed_work(&pool, work_id, true).await;
    // Seed an old open finding then close it — only resolved findings
    // remain in the table for the sweep to consider.
    seed_open_finding(&pool, "fnd_resolved", work_id, 7200).await;
    sqlx::query("UPDATE findings SET status = 'resolved' WHERE finding_id = ?")
        .bind("fnd_resolved")
        .execute(&pool)
        .await
        .unwrap();

    run_one_sweep(&pool, 60).await;

    assert_eq!(
        review_master_schedule_count(&pool, work_id).await,
        0,
        "resolved finding must not trigger enqueue"
    );
}

/// AC2 best-effort + AC3 stability: two repeated sweeps on the same
/// stale-finding fixture must each enqueue exactly one schedule per
/// finding, confirming the sweep is idempotent-per-sweep (one row per
/// invocation per finding) and never panics on the second pass.
#[tokio::test]
async fn repeated_sweeps_remain_stable() {
    let (pool, _tmp) = fresh_pool().await;
    let work_id = "wrk_p4t5_repeat";
    seed_work(&pool, work_id, true).await;
    seed_open_finding(&pool, "fnd_repeat", work_id, 7200).await;

    run_one_sweep(&pool, 60).await;
    run_one_sweep(&pool, 60).await;

    // Each sweep enqueues a fresh schedule because the underlying
    // finding remains open. AC3 contract: opt-in is sticky until the
    // operator (or the review-master run) resolves the finding.
    assert_eq!(
        review_master_schedule_count(&pool, work_id).await,
        2,
        "two sweeps on persistently-stale finding produce two schedules"
    );
}
