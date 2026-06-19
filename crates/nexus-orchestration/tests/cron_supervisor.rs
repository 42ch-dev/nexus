//! Hermetic tests for the daemon-side cron evaluator (V1.50 T-A P1).
//!
//! Spec: `.mstar/knowledge/specs/novel-writing/cron-staggering.md` §4.
//! Plan acceptance criteria §1 (fire/skip/idempotent), §3 (synthetic tick),
//! §4 (same-minute double fire), §5 (TOCTOU), §6 (partial index).

use std::sync::Arc;

use chrono::TimeZone;
use nexus_local_db::works::{self, WorkRecord};
use nexus_orchestration::schedule::cron_supervisor::{self, CronFireSummary};
use sqlx::SqlitePool;

// ── Test helpers ───────────────────────────────────────────────────────────

/// Open a migrated in-memory pool (tempfile + forget; pattern from
/// `supervisor_cross_volume.rs`).
async fn test_pool() -> SqlitePool {
    let db = tempfile::Builder::new()
        .prefix("cron_sup_test_")
        .suffix(".db")
        .tempfile()
        .unwrap();
    let db_path = db.path().to_path_buf();
    std::mem::forget(db);
    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    pool
}

/// Standard healthy Work (intake complete, no lock, not completed).
fn test_work(work_id: &str) -> WorkRecord {
    WorkRecord {
        work_id: work_id.to_string(),
        creator_id: "ctr_test".to_string(),
        workspace_slug: "ws".to_string(),
        status: "active".to_string(),
        title: "Cron Test Novel".to_string(),
        long_term_goal: "Test cron firing".to_string(),
        initial_idea: "An idea".to_string(),
        creative_brief: None,
        intake_status: "complete".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-06-18T10:00:00Z".to_string(),
        updated_at: "2026-06-18T10:00:00Z".to_string(),
        current_stage: "intake".to_string(),
        stage_status: "complete".to_string(),
        work_profile: Some("novel".to_string()),
        work_ref: Some(format!("cron-{work_id}")),
        total_planned_chapters: Some(3),
        current_chapter: 0,
        auto_chain_enabled: true,
        driver_schedule_id: None,
        auto_chain_interrupted: false,
        auto_review_master_on_timeout: false,
        runtime_lock_holder: None,
        runtime_lock_acquired_at: None,
        completion_locked_at: None,
        novel_completion_status: None,
        lineage_from_work_id: None,
    }
}

/// Write `schedule_json` for a Work via the DAO (simulating the CLI `cron set`).
async fn set_schedule(pool: &SqlitePool, work_id: &str, json: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    works::set_schedule_json(pool, work_id, json, &now)
        .await
        .unwrap();
}

/// Build a per-Work schedule blob (spec §2.1 shape).
fn schedule_blob(
    brainstorm_cron: &str,
    write_cron: &str,
    brainstorm_on: bool,
    write_on: bool,
) -> String {
    serde_json::json!({
        "tz": "UTC",
        "roles": {
            "brainstorm": {"cron": brainstorm_cron, "enabled": brainstorm_on},
            "write": {"cron": write_cron, "enabled": write_on}
        }
    })
    .to_string()
}

/// Seed a Work + its `schedule_json`.
async fn seed_work(pool: &SqlitePool, work: &WorkRecord, schedule_json: &str) {
    works::create_work(pool, work).await.unwrap();
    set_schedule(pool, &work.work_id, schedule_json).await;
}

/// Count pending schedules for (`work_id`, `preset_id`).
async fn count_schedules(pool: &SqlitePool, work_id: &str, preset_id: &str) -> i64 {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM creator_schedules WHERE work_id = ? AND preset_id = ?",
    )
    .bind(work_id)
    .bind(preset_id)
    .fetch_one(pool)
    .await
    .unwrap();
    n
}

// ── AC1 + AC3: fire on cron match; no-match → no enqueue ───────────────────

/// A brainstorm role whose cron matches the current minute enqueues a pending
/// schedule with `preset_id = novel-brainstorm`.
#[tokio::test]
async fn cron_fires_on_match_enqueues_brainstorm() {
    let pool = test_pool().await;
    let work = test_work("wrk_fire_brain");
    // `* * * * *` matches every minute, so any `now` fires.
    seed_work(
        &pool,
        &work,
        &schedule_blob("* * * * *", "0 4 * * *", true, false),
    )
    .await;

    let now = chrono::Utc::now();
    let summary = cron_supervisor::evaluate_cron_fires(&pool, None, now).await;

    assert_eq!(summary.fired, 1, "brainstorm should fire: {summary:?}");
    assert_eq!(
        count_schedules(&pool, "wrk_fire_brain", "novel-brainstorm").await,
        1,
        "one novel-brainstorm schedule should be enqueued"
    );
}

/// A write role whose cron matches enqueues `preset_id = novel-write`.
#[tokio::test]
async fn cron_fires_on_match_enqueues_write() {
    let pool = test_pool().await;
    let work = test_work("wrk_fire_write");
    seed_work(
        &pool,
        &work,
        &schedule_blob("0 3 * * *", "* * * * *", false, true),
    )
    .await;

    let now = chrono::Utc::now();
    let summary = cron_supervisor::evaluate_cron_fires(&pool, None, now).await;

    assert_eq!(summary.fired, 1, "write should fire: {summary:?}");
    assert_eq!(
        count_schedules(&pool, "wrk_fire_write", "novel-write").await,
        1,
    );
}

/// A non-matching minute enqueues nothing (AC3 — non-match case).
#[tokio::test]
async fn cron_no_match_does_not_enqueue() {
    let pool = test_pool().await;
    let work = test_work("wrk_nomatch");
    // `0 3 * * *` fires only at 03:00 UTC.
    seed_work(
        &pool,
        &work,
        &schedule_blob("0 3 * * *", "0 4 * * *", true, true),
    )
    .await;

    // Pick a `now` that does NOT match either cron (e.g. 2026-06-19 05:30 UTC).
    let now = chrono::Utc.with_ymd_and_hms(2026, 6, 19, 5, 30, 0).unwrap();
    let summary = cron_supervisor::evaluate_cron_fires(&pool, None, now).await;

    assert_eq!(
        summary.fired, 0,
        "no role should fire at 05:30: {summary:?}"
    );
    assert_eq!(
        count_schedules(&pool, "wrk_nomatch", "novel-brainstorm").await,
        0
    );
    assert_eq!(
        count_schedules(&pool, "wrk_nomatch", "novel-write").await,
        0
    );
}

/// Both roles matching the same minute enqueue two distinct schedules.
#[tokio::test]
async fn cron_fires_both_roles_same_minute() {
    let pool = test_pool().await;
    let work = test_work("wrk_both");
    seed_work(
        &pool,
        &work,
        &schedule_blob("* * * * *", "* * * * *", true, true),
    )
    .await;

    let now = chrono::Utc::now();
    let summary = cron_supervisor::evaluate_cron_fires(&pool, None, now).await;

    assert_eq!(summary.fired, 2, "both roles should fire: {summary:?}");
    assert_eq!(
        count_schedules(&pool, "wrk_both", "novel-brainstorm").await,
        1
    );
    assert_eq!(count_schedules(&pool, "wrk_both", "novel-write").await, 1);
}

/// A disabled role does not fire even when its cron matches.
#[tokio::test]
async fn cron_skips_disabled_role() {
    let pool = test_pool().await;
    let work = test_work("wrk_disabled");
    seed_work(
        &pool,
        &work,
        &schedule_blob("* * * * *", "* * * * *", false, false),
    )
    .await;

    let now = chrono::Utc::now();
    let summary = cron_supervisor::evaluate_cron_fires(&pool, None, now).await;

    assert_eq!(
        summary.fired, 0,
        "disabled roles must not fire: {summary:?}"
    );
    // Three roles evaluated (V1.50 T-A P2 adds `review`): brainstorm + write
    // are disabled above, and `review` is absent from this two-role blob so it
    // deserialises to `None`. All three are clean no-match skips — the test's
    // intent ("disabled roles don't fire") still holds via `fired == 0`.
    assert_eq!(summary.skipped_no_match, 3);
}

// ── AC1 + AC4: per-Work gating (3 negative cases) ──────────────────────────

/// Gating: incomplete intake → skip (spec §4.3).
#[tokio::test]
async fn cron_skips_intake_incomplete() {
    let pool = test_pool().await;
    let mut work = test_work("wrk_intake");
    // Use a valid non-complete intake_status (CHECK constraint:
    // pending/in_progress/complete/skipped).
    work.intake_status = "in_progress".to_string();
    seed_work(
        &pool,
        &work,
        &schedule_blob("* * * * *", "* * * * *", true, true),
    )
    .await;

    let now = chrono::Utc::now();
    let summary = cron_supervisor::evaluate_cron_fires(&pool, None, now).await;

    assert_eq!(summary.fired, 0, "intake-incomplete Work must not fire");
    assert_eq!(summary.skipped_gated, 2, "both roles gated");
}

/// Gating: runtime lock held → skip (spec §4.3).
#[tokio::test]
async fn cron_skips_runtime_locked() {
    let pool = test_pool().await;
    let work = test_work("wrk_locked");
    seed_work(
        &pool,
        &work,
        &schedule_blob("* * * * *", "* * * * *", true, true),
    )
    .await;
    // create_work hardcodes runtime_lock_holder = NULL; set it via a direct
    // UPDATE to simulate an in-progress FL-E driver holding the lock.
    sqlx::query("UPDATE works SET runtime_lock_holder = ? WHERE work_id = ?")
        .bind("daemon:schedule:ACH123")
        .bind("wrk_locked")
        .execute(&pool)
        .await
        .unwrap();

    let now = chrono::Utc::now();
    let summary = cron_supervisor::evaluate_cron_fires(&pool, None, now).await;

    assert_eq!(summary.fired, 0, "runtime-locked Work must not fire");
    assert_eq!(summary.skipped_gated, 2);
}

/// Gating: completion-locked → skip (spec §4.3).
#[tokio::test]
async fn cron_skips_completion_locked() {
    let pool = test_pool().await;
    let work = test_work("wrk_done");
    seed_work(
        &pool,
        &work,
        &schedule_blob("* * * * *", "* * * * *", true, true),
    )
    .await;
    // create_work hardcodes completion_locked_at = NULL; set it via a direct
    // UPDATE to simulate a finalized Work.
    sqlx::query("UPDATE works SET completion_locked_at = ? WHERE work_id = ?")
        .bind("2026-06-18T12:00:00Z")
        .bind("wrk_done")
        .execute(&pool)
        .await
        .unwrap();

    let now = chrono::Utc::now();
    let summary = cron_supervisor::evaluate_cron_fires(&pool, None, now).await;

    assert_eq!(summary.fired, 0, "completion-locked Work must not fire");
    assert_eq!(summary.skipped_gated, 2);
}

// ── AC4: idempotency — same-minute double fire → second skipped ────────────

/// Two cron evaluations within the same minute for the same role+work: the
/// second is skipped because a prior schedule is still active (pending).
#[tokio::test]
async fn cron_idempotent_skip_second_fire_same_minute() {
    let pool = test_pool().await;
    let work = test_work("wrk_idem");
    seed_work(
        &pool,
        &work,
        &schedule_blob("* * * * *", "0 4 * * *", true, false),
    )
    .await;

    let now = chrono::Utc::now();

    // First evaluation: brainstorm fires (no prior active schedule).
    let s1 = cron_supervisor::evaluate_cron_fires(&pool, None, now).await;
    assert_eq!(s1.fired, 1, "first sweep should fire: {s1:?}");
    assert_eq!(
        count_schedules(&pool, "wrk_idem", "novel-brainstorm").await,
        1
    );

    // Second evaluation same minute: brainstorm skip (prior pending schedule).
    let s2 = cron_supervisor::evaluate_cron_fires(&pool, None, now).await;
    assert_eq!(s2.fired, 0, "second sweep must not fire: {s2:?}");
    assert_eq!(
        s2.skipped_idempotent, 1,
        "second brainstorm should be skipped by idempotency"
    );
    // Still only one schedule — no duplicate.
    assert_eq!(
        count_schedules(&pool, "wrk_idem", "novel-brainstorm").await,
        1
    );
}

/// Once the prior schedule transitions to a terminal state (completed), the
/// next cron evaluation fires again — idempotency is "active-only".
#[tokio::test]
async fn cron_refires_after_prior_schedule_terminal() {
    let pool = test_pool().await;
    let work = test_work("wrk_refire");
    seed_work(
        &pool,
        &work,
        &schedule_blob("* * * * *", "0 4 * * *", true, false),
    )
    .await;

    let now = chrono::Utc::now();
    let s1 = cron_supervisor::evaluate_cron_fires(&pool, None, now).await;
    assert_eq!(s1.fired, 1);

    // Simulate the prior schedule completing.
    let schedule_id: String = sqlx::query_scalar(
        "SELECT schedule_id FROM creator_schedules \
         WHERE work_id = 'wrk_refire' AND preset_id = 'novel-brainstorm'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE creator_schedules SET status = 'completed' WHERE schedule_id = ?")
        .bind(&schedule_id)
        .execute(&pool)
        .await
        .unwrap();

    // Next evaluation: fires again (prior is terminal, not active).
    let s2 = cron_supervisor::evaluate_cron_fires(&pool, None, now).await;
    assert_eq!(s2.fired, 1, "should re-fire after prior completed: {s2:?}");
    assert_eq!(
        count_schedules(&pool, "wrk_refire", "novel-brainstorm").await,
        2,
        "two distinct schedules now exist"
    );
}

// ── AC5: TOCTOU — set_schedule_json_tx CAS ──────────────────────────────────

/// The CAS rejects a write when the stored value changed between read and
/// write (lost-update prevention). Closes R-V150P0-W5.
#[tokio::test]
async fn set_schedule_json_tx_rejects_stale_preimage() {
    let pool = test_pool().await;
    let work = test_work("wrk_cas");
    works::create_work(&pool, &work).await.unwrap();

    // Writer B writes first.
    set_schedule(&pool, "wrk_cas", "{\"v\":\"B\"}").await;

    // Writer A (which read None earlier) attempts a CAS with the stale
    // pre-image None → must fail because the row now holds "{\"v\":\"B\"}".
    let mut tx = pool.begin().await.unwrap();
    let applied = works::set_schedule_json_tx(
        &mut tx,
        "wrk_cas",
        None,
        "{\"v\":\"A\"}",
        &chrono::Utc::now().to_rfc3339(),
    )
    .await
    .unwrap();
    assert!(
        !applied,
        "CAS must reject when stored value != expected pre-image"
    );
    tx.rollback().await.unwrap();

    // The stored value is B's, not A's — no lost update.
    let stored = works::get_schedule_json(&pool, "wrk_cas").await.unwrap();
    assert_eq!(stored.as_deref(), Some("{\"v\":\"B\"}"));
}

/// The CAS applies when the pre-image matches.
#[tokio::test]
async fn set_schedule_json_tx_applies_on_matching_preimage() {
    let pool = test_pool().await;
    let work = test_work("wrk_cas_ok");
    works::create_work(&pool, &work).await.unwrap();
    set_schedule(&pool, "wrk_cas_ok", "{\"v\":\"initial\"}").await;

    let mut tx = pool.begin().await.unwrap();
    let applied = works::set_schedule_json_tx(
        &mut tx,
        "wrk_cas_ok",
        Some("{\"v\":\"initial\"}"),
        "{\"v\":\"updated\"}",
        &chrono::Utc::now().to_rfc3339(),
    )
    .await
    .unwrap();
    assert!(applied, "CAS must apply when pre-image matches");
    tx.commit().await.unwrap();

    let stored = works::get_schedule_json(&pool, "wrk_cas_ok").await.unwrap();
    assert_eq!(stored.as_deref(), Some("{\"v\":\"updated\"}"));
}

/// Two concurrent writers serialised via CAS: A reads, B writes, A's CAS
/// fails, A retries with the fresh pre-image and succeeds. This is the
/// R-V150P0-W5 race scenario (daemon writer vs CLI writer).
#[tokio::test]
async fn set_schedule_json_tx_concurrent_writers_serialise() {
    let pool = test_pool().await;
    let work = test_work("wrk_cas_race");
    works::create_work(&pool, &work).await.unwrap();

    // A reads (sees None).
    let a_read = works::get_schedule_json(&pool, "wrk_cas_race")
        .await
        .unwrap();

    // B races ahead and writes its config.
    set_schedule(&pool, "wrk_cas_race", "{\"v\":\"B\"}").await;

    // A's first CAS attempt with the stale pre-image fails.
    let mut tx = pool.begin().await.unwrap();
    let applied_a1 = works::set_schedule_json_tx(
        &mut tx,
        "wrk_cas_race",
        a_read.as_deref(),
        "{\"v\":\"A-lost\"}",
        &chrono::Utc::now().to_rfc3339(),
    )
    .await
    .unwrap();
    assert!(!applied_a1, "A's stale CAS must fail");
    tx.rollback().await.unwrap();

    // A re-reads, retries with the fresh pre-image → succeeds.
    let a_reread = works::get_schedule_json(&pool, "wrk_cas_race")
        .await
        .unwrap();
    let mut tx2 = pool.begin().await.unwrap();
    let applied_a2 = works::set_schedule_json_tx(
        &mut tx2,
        "wrk_cas_race",
        a_reread.as_deref(),
        "{\"v\":\"A-merged\"}",
        &chrono::Utc::now().to_rfc3339(),
    )
    .await
    .unwrap();
    assert!(
        applied_a2,
        "A's retried CAS with fresh pre-image must apply"
    );
    tx2.commit().await.unwrap();

    let stored = works::get_schedule_json(&pool, "wrk_cas_race")
        .await
        .unwrap();
    assert_eq!(stored.as_deref(), Some("{\"v\":\"A-merged\"}"));
}

/// CAS on a missing Work returns `MissingVersionKey` (not a false Ok(false)).
#[tokio::test]
async fn set_schedule_json_tx_missing_work_errors() {
    let pool = test_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let result = works::set_schedule_json_tx(
        &mut tx,
        "wrk_nonexistent",
        None,
        "{}",
        &chrono::Utc::now().to_rfc3339(),
    )
    .await;
    assert!(
        result.is_err(),
        "missing Work should error, not return Ok(false)"
    );
    tx.rollback().await.unwrap();
}

// ── AC6: partial index used in scan ─────────────────────────────────────────

/// The scan query (`list_works_with_schedule_json`) uses the partial index
/// `idx_works_schedule_json_nonempty` (S-001). Verified via EXPLAIN QUERY PLAN.
#[tokio::test]
async fn partial_index_used_in_schedule_json_scan() {
    let pool = test_pool().await;

    // EXPLAIN QUERY PLAN returns 4 columns per step: (id, parent, notused,
    // detail) — the first three are INTEGER, the fourth is TEXT. We decode
    // all four and inspect the `detail` column for the index name.
    let rows: Vec<(i64, i64, i64, String)> = sqlx::query_as(
        "EXPLAIN QUERY PLAN \
         SELECT work_id FROM works \
         WHERE schedule_json IS NOT NULL AND schedule_json != ''",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let plan = rows
        .into_iter()
        .map(|(_, _, _, detail)| detail)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        plan.contains("idx_works_schedule_json_nonempty"),
        "scan must use the partial index; plan was:\n{plan}"
    );
}

// ── Empty / malformed schedule_json edge cases ──────────────────────────────

/// A Work with unparseable `schedule_json` is skipped (counted as parse error),
/// not crashed.
#[tokio::test]
async fn cron_skips_malformed_schedule_json() {
    let pool = test_pool().await;
    let work = test_work("wrk_badjson");
    seed_work(&pool, &work, "{not valid json").await;

    let now = chrono::Utc::now();
    let summary = cron_supervisor::evaluate_cron_fires(&pool, None, now).await;

    assert_eq!(summary.fired, 0);
    assert_eq!(
        summary.skipped_parse_error, 1,
        "malformed blob → parse error"
    );
}

/// A Work with no `schedule_json` at all is not even scanned (the partial index
/// excludes it). A healthy Work with an empty-string blob is likewise excluded.
#[tokio::test]
async fn cron_ignores_works_without_schedule_json() {
    let pool = test_pool().await;
    let work = test_work("wrk_nosched");
    works::create_work(&pool, &work).await.unwrap();
    // No set_schedule call → schedule_json is NULL.

    let now = chrono::Utc::now();
    let summary = cron_supervisor::evaluate_cron_fires(&pool, None, now).await;

    assert_eq!(
        summary.total_evaluated(),
        0,
        "NULL schedule_json not scanned"
    );
    assert_eq!(summary, CronFireSummary::default());
}

// ── AC2: enqueue → admit (auto-chain step advanced) ─────────────────────────

use nexus_orchestration::schedule::supervisor::ScheduleSupervisor;

/// A cron fire + supervisor tick admits the enqueued schedule (the cron
/// schedule becomes Running, ready for the executor). This is the hermetic
/// analog of AC2 ("creator run status after cron fire shows the auto-chain
/// step advanced") — the schedule is enqueued by the evaluator and admitted by
/// the supervisor tick, so the Work gains an active driver.
#[tokio::test]
async fn cron_fire_then_tick_admits_schedule() {
    let pool = Arc::new(test_pool().await);
    let work = test_work("wrk_admit");
    seed_work(
        &pool,
        &work,
        &schedule_blob("* * * * *", "0 4 * * *", true, false),
    )
    .await;

    let supervisor = Arc::new(ScheduleSupervisor::new(pool.clone()));

    // Step 1: cron evaluator enqueues a pending brainstorm schedule.
    let now = chrono::Utc::now();
    let summary = cron_supervisor::evaluate_cron_fires(&pool, None, now).await;
    assert_eq!(summary.fired, 1);

    // Step 2: supervisor tick admits it (status → running).
    supervisor.tick_clocked(now.timestamp()).await.unwrap();

    let status: String = sqlx::query_scalar(
        "SELECT status FROM creator_schedules \
         WHERE work_id = 'wrk_admit' AND preset_id = 'novel-brainstorm'",
    )
    .fetch_one(&*pool)
    .await
    .unwrap();
    assert_eq!(
        status, "running",
        "cron-fired schedule should be admitted to running by the supervisor tick"
    );
}

// ── Review role (V1.50 T-A P2) ──────────────────────────────────────────────
//
// Spec: `cron-staggering.md` §2.1 / §4.1 / §4.3. The `review` role enqueues a
// `novel-review-master` schedule (spec §2.1 table). These four tests extend
// the T-A P1 suite with review-specific fire / gating / idempotency / graceful
// skip coverage. The review cron reuses the *same* uniform `enqueue_cron_schedule`
// path as brainstorm/write, so the enqueued schedule carries a `CRON` prefix
// and a `cron:review:<work>` label — keeping cron-fired reviews distinguishable
// from the V1.39 stale-findings `auto-review-master` (`RVM`) path.

/// Build a per-Work schedule blob that includes the `review` role (spec §2.1).
/// Mirrors [`schedule_blob`] but adds the third role so the 18 existing T-A P1
/// tests keep their two-role helper unchanged.
fn schedule_blob_review(
    brainstorm_cron: &str,
    write_cron: &str,
    review_cron: &str,
    brainstorm_on: bool,
    write_on: bool,
    review_on: bool,
) -> String {
    serde_json::json!({
        "tz": "UTC",
        "roles": {
            "brainstorm": {"cron": brainstorm_cron, "enabled": brainstorm_on},
            "write": {"cron": write_cron, "enabled": write_on},
            "review": {"cron": review_cron, "enabled": review_on}
        }
    })
    .to_string()
}

/// AC1 (T-A P2): the `review` role fires on its spec default `0,30 * * * *`
/// (matches `:00` and `:30`), enqueues a `novel-review-master` schedule, and
/// carries the cron-origin label/prefix so it stays distinguishable from the
/// V1.39 stale-findings `auto-review-master` path. A non-matching minute (`:15`)
/// does not fire.
#[tokio::test]
async fn cron_fires_review_role_enqueues_review_master() {
    let pool = test_pool().await;
    let work = test_work("wrk_fire_review");
    // Only review enabled; brainstorm/write crons set to a non-matching slot
    // and disabled so the single fire is unambiguously the review role.
    seed_work(
        &pool,
        &work,
        &schedule_blob_review("0 3 * * *", "0 4 * * *", "0,30 * * * *", false, false, true),
    )
    .await;

    // 14:00 UTC matches the `:00` slot of `0,30 * * * *`.
    let now = chrono::Utc.with_ymd_and_hms(2026, 6, 19, 14, 0, 0).unwrap();
    let summary = cron_supervisor::evaluate_cron_fires(&pool, None, now).await;

    assert_eq!(summary.fired, 1, "review should fire at :00: {summary:?}");
    assert_eq!(
        count_schedules(&pool, "wrk_fire_review", "novel-review-master").await,
        1,
        "one novel-review-master schedule should be enqueued"
    );

    // Provenance: the cron-fired review-master schedule carries the `CRON`
    // prefix and `cron:review:<work>` label (NOT the stale-findings `RVM` /
    // `auto-review-master:` shape). This is the Option A contract: one uniform
    // enqueue path preserves the trigger origin in the schedule row.
    let (schedule_id, label): (String, String) = sqlx::query_as(
        "SELECT schedule_id, label FROM creator_schedules \
         WHERE work_id = 'wrk_fire_review' AND preset_id = 'novel-review-master'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        schedule_id.starts_with("CRON"),
        "cron-fired review schedule must use the CRON prefix (got {schedule_id})"
    );
    assert_eq!(
        label, "cron:review:wrk_fire_review",
        "cron-fired review must carry the cron:review label"
    );

    // Non-matching minute (:15) does not enqueue a second review schedule.
    let now_off = chrono::Utc
        .with_ymd_and_hms(2026, 6, 19, 14, 15, 0)
        .unwrap();
    let summary_off = cron_supervisor::evaluate_cron_fires(&pool, None, now_off).await;
    assert_eq!(
        summary_off.fired, 0,
        "review must not fire at :15: {summary_off:?}"
    );
    assert_eq!(
        count_schedules(&pool, "wrk_fire_review", "novel-review-master").await,
        1,
    );
}

/// AC (T-A P2): the review role respects the same per-Work gating as
/// brainstorm/write (spec §4.3). A completion-locked Work skips the review
/// fire (counted as `skipped_gated`).
#[tokio::test]
async fn cron_review_respects_per_work_gating() {
    let pool = test_pool().await;
    let work = test_work("wrk_review_gated");
    // Only review enabled with an always-matching cron.
    seed_work(
        &pool,
        &work,
        &schedule_blob_review("0 3 * * *", "0 4 * * *", "* * * * *", false, false, true),
    )
    .await;
    // create_work hardcodes completion_locked_at = NULL; set it via a direct
    // UPDATE to simulate a finalized Work (mirrors `cron_skips_completion_locked`).
    sqlx::query("UPDATE works SET completion_locked_at = ? WHERE work_id = ?")
        .bind("2026-06-18T12:00:00Z")
        .bind("wrk_review_gated")
        .execute(&pool)
        .await
        .unwrap();

    let now = chrono::Utc::now();
    let summary = cron_supervisor::evaluate_cron_fires(&pool, None, now).await;

    assert_eq!(
        summary.fired, 0,
        "completion-locked Work must not fire review"
    );
    assert_eq!(
        summary.skipped_gated, 1,
        "review should be gated: {summary:?}"
    );
    assert_eq!(
        count_schedules(&pool, "wrk_review_gated", "novel-review-master").await,
        0,
    );
}

/// AC (T-A P2): the review role honors the spec §4.2 idempotency guard. When a
/// prior `novel-review-master` schedule for the same Work is still active, the
/// review cron fire is skipped (counted as `skipped_idempotent`).
#[tokio::test]
async fn cron_review_respects_idempotency() {
    let pool = test_pool().await;
    let work = test_work("wrk_review_idem");
    seed_work(
        &pool,
        &work,
        &schedule_blob_review("0 3 * * *", "0 4 * * *", "* * * * *", false, false, true),
    )
    .await;
    // Pre-insert an ACTIVE novel-review-master schedule for this Work so the
    // idempotency guard (`has_active_role_schedule`) sees it.
    let now_ts = chrono::Utc::now().timestamp();
    sqlx::query(
        "INSERT INTO creator_schedules \
         (schedule_id, creator_id, preset_id, preset_version, status, \
          concurrency_kind, current_core_context_version, label, \
          created_at, updated_at, work_id) \
         VALUES ('RVM-PREEXISTING-REVIEW', 'ctr_test', 'novel-review-master', 1, \
                 'pending', 'serial', 0, 'preexisting-review-master', ?, ?, 'wrk_review_idem')",
    )
    .bind(now_ts)
    .bind(now_ts)
    .execute(&pool)
    .await
    .unwrap();

    let now = chrono::Utc::now();
    let summary = cron_supervisor::evaluate_cron_fires(&pool, None, now).await;

    assert_eq!(
        summary.fired, 0,
        "review must not fire while a prior review-master is active"
    );
    assert_eq!(
        summary.skipped_idempotent, 1,
        "review should be skipped by idempotency: {summary:?}"
    );
    // Exactly one review-master schedule (the pre-existing one); no duplicate.
    assert_eq!(
        count_schedules(&pool, "wrk_review_idem", "novel-review-master").await,
        1,
    );
}

/// AC4 (T-A P2): a Work whose `schedule_json` configures brainstorm/write but
/// NOT the `review` role skips review gracefully — no review-master schedule is
/// enqueued, and the absent role is a clean `skipped_no_match` (not an error).
/// This is the "no review work = no review schedule" graceful-skip case.
#[tokio::test]
async fn cron_review_graceful_when_no_review_role_configured() {
    let pool = test_pool().await;
    let work = test_work("wrk_no_review");
    // Two-role blob (no `review` key) — the `review` field deserialises to
    // `None` via `#[serde(default)]`, so the evaluator no-ops it cleanly.
    seed_work(
        &pool,
        &work,
        &schedule_blob("* * * * *", "0 4 * * *", true, false),
    )
    .await;

    let now = chrono::Utc::now();
    let summary = cron_supervisor::evaluate_cron_fires(&pool, None, now).await;

    // Brainstorm fires (its cron matches every minute); review is absent →
    // one fire (brainstorm) and one graceful review skip (skipped_no_match).
    assert_eq!(summary.fired, 1, "brainstorm should fire: {summary:?}");
    assert_eq!(
        summary.skipped_parse_error, 0,
        "absent review role must not be a parse error"
    );
    assert_eq!(
        count_schedules(&pool, "wrk_no_review", "novel-review-master").await,
        0,
        "no review-master schedule when review role is absent"
    );
    assert_eq!(
        count_schedules(&pool, "wrk_no_review", "novel-brainstorm").await,
        1,
    );
}
