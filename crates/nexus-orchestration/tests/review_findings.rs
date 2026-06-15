//! V1.47 P0 — `novel-chapter-review` (FL-E `review` stage) → findings hook.
//!
//! Hermetic integration tests for the supervisor terminal hook that persists
//! ≥1 finding row when a `novel-chapter-review` schedule completes.
//!
//! Covers plan acceptance criteria:
//! - AC1 (#1): Auto-chain review stage creates ≥1 finding row for a novel Work
//!   with chapter context.
//! - AC2 (#2): On-demand `creator run <review_preset_id> <work_id>` reaches
//!   the same code path (no `driver_schedule_id` set on the Work).
//! - AC3 (#3): Findings include `kind`, `severity`, `target_executor`,
//!   optional `rule_suggestion` in stored metadata/body contract per spec §8.
//! - AC4 (#4): Auto-chain driver invariant preserved — finding creation does
//!   not fork or cancel the active FL-E driver (work stays at review stage;
//!   the next stage advance proceeds normally afterward).
//! - AC5 (#5): ≥1 hermetic integration test for review → finding insert.

#![allow(clippy::unwrap_used)]

use nexus_contracts::local::schedule::ScheduleStatus;
use nexus_local_db::findings::{self, FindingListFilters};
use nexus_local_db::works::{self, WorkRecord};
use nexus_orchestration::auto_chain;
use nexus_orchestration::schedule::supervisor::ScheduleSupervisor;
use sqlx::SqlitePool;
use std::sync::Arc;

// ── Test fixtures ───────────────────────────────────────────────────────────

const CREATOR: &str = "ctr_review_test";

fn novel_work(work_id: &str, chapter: i32, total: i32) -> WorkRecord {
    WorkRecord {
        work_id: work_id.to_string(),
        creator_id: CREATOR.to_string(),
        workspace_slug: "ws".to_string(),
        status: "active".to_string(),
        title: "Reflection Loop Test Novel".to_string(),
        long_term_goal: "Finish a short novel".to_string(),
        initial_idea: "A detective story".to_string(),
        creative_brief: None,
        intake_status: "complete".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-06-15T10:00:00Z".to_string(),
        updated_at: "2026-06-15T10:00:00Z".to_string(),
        current_stage: "review".to_string(),
        stage_status: "active".to_string(),
        work_profile: Some("novel".to_string()),
        work_ref: Some("reflection-test".to_string()),
        total_planned_chapters: if total > 0 { Some(total) } else { None },
        current_chapter: chapter,
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

async fn test_pool() -> SqlitePool {
    let db = tempfile::Builder::new()
        .prefix("review_findings_test_")
        .suffix(".db")
        .tempfile()
        .unwrap();
    let db_path = db.path().to_path_buf();
    std::mem::forget(db);

    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    pool
}

/// Insert a `creator_schedules` row directly for the test scenario.
async fn insert_schedule(
    pool: &SqlitePool,
    schedule_id: &str,
    preset_id: &str,
    status: &str,
    work_id: &str,
) {
    let now = chrono::Utc::now().timestamp();
    // SAFETY: test-only — DML helper for schedule row insertion.
    sqlx::query(
        r"INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version,
            label, created_at, updated_at, work_id)
           VALUES (?, ?, ?, 1, ?, 'serial', 0, ?, ?, ?, ?)",
    )
    .bind(schedule_id)
    .bind(CREATOR)
    .bind(preset_id)
    .bind(status)
    .bind(format!("review-{work_id}"))
    .bind(now)
    .bind(now)
    .bind(work_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn count_findings(pool: &SqlitePool, work_id: &str) -> i64 {
    let filters = FindingListFilters {
        work_id: Some(work_id.to_string()),
        ..Default::default()
    };
    let rows = findings::list_findings(pool, CREATOR, &filters)
        .await
        .unwrap();
    i64::try_from(rows.len()).unwrap_or(0)
}

// ── AC1: Auto-chain review stage creates ≥1 finding row ─────────────────────

#[tokio::test]
async fn ac1_auto_chain_review_terminal_persists_finding() {
    let pool = test_pool().await;
    let sup = Arc::new(ScheduleSupervisor::new(Arc::new(pool.clone())));

    // Work at review stage, chapter 2 of 5 — current_chapter reflects the
    // most recently finalized chapter (chapter 2 done, now reviewing).
    let work = novel_work("wrk_ac1", 2, 5);
    works::create_work(&pool, &work).await.unwrap();

    // The review schedule is the active driver — set it.
    insert_schedule(
        &pool,
        "sch_ac1_review",
        "novel-chapter-review",
        "running",
        "wrk_ac1",
    )
    .await;
    auto_chain::set_driver(&pool, CREATOR, "wrk_ac1", "sch_ac1_review", "review")
        .await
        .unwrap();

    // Pre-state: no findings.
    assert_eq!(count_findings(&pool, "wrk_ac1").await, 0);

    // Mark the review stage complete (as the schedule runner would).
    works::patch_work(
        &pool,
        CREATOR,
        "wrk_ac1",
        &works::WorkPatch {
            stage_status: Some("complete".to_string()),
            ..Default::default()
        },
        &chrono::Utc::now().to_rfc3339(),
    )
    .await
    .unwrap();

    // Terminal transition — supervisor hook fires.
    sup.on_schedule_terminal("sch_ac1_review", ScheduleStatus::Completed)
        .await
        .unwrap();

    // AC1: ≥1 finding row exists for the novel Work.
    let n = count_findings(&pool, "wrk_ac1").await;
    assert!(
        n >= 1,
        "auto-chain review terminal must persist ≥1 finding; got {n}"
    );

    // AC3: the finding has the required V1.47 §8.2 fields.
    let filters = FindingListFilters {
        work_id: Some("wrk_ac1".to_string()),
        ..Default::default()
    };
    let rows = findings::list_findings(&pool, CREATOR, &filters)
        .await
        .unwrap();
    let f = &rows[0];
    assert_eq!(f.work_id, "wrk_ac1");
    assert_eq!(f.status, "open");
    // chapter context derived from work.current_chapter (V1.38 §4.5.2)
    assert_eq!(f.chapter, Some(2));
    // Required V1.47 fields per spec §8.2
    assert!(
        !f.kind.is_empty(),
        "finding must set `kind` (got empty); per spec §8.2"
    );
    assert!(
        !f.severity.is_empty(),
        "finding must set `severity` (got empty)"
    );
    assert!(
        !f.target_executor.is_empty(),
        "finding must set `target_executor` (got empty)"
    );
    assert!(
        !f.description.is_empty(),
        "finding body must be non-empty (got empty)"
    );

    // AC4: the active driver was NOT canceled — auto-chain advanced to persist.
    let updated = works::get_work(&pool, CREATOR, "wrk_ac1")
        .await
        .unwrap()
        .unwrap();
    assert!(
        !updated.auto_chain_interrupted,
        "auto_chain_interrupted should remain false; got {}",
        updated.auto_chain_interrupted
    );
    assert!(
        matches!(updated.current_stage.as_str(), "persist"),
        "auto-chain should have advanced review → persist after findings were written; got stage='{}'",
        updated.current_stage
    );
}

// ── AC2: On-demand `creator run` reaches the same code path ─────────────────

#[tokio::test]
async fn ac2_on_demand_review_run_persists_finding_same_path() {
    let pool = test_pool().await;
    let sup = Arc::new(ScheduleSupervisor::new(Arc::new(pool.clone())));

    // Work at produce-complete — user manually triggers review via CLI.
    // The schedule created by `creator run novel-chapter-review` does NOT
    // become the work's driver_schedule_id (CLI stage_advance path).
    let mut work = novel_work("wrk_ac2", 1, 3);
    work.current_stage = "produce".to_string();
    work.stage_status = "complete".to_string();
    works::create_work(&pool, &work).await.unwrap();

    // On-demand schedule (not set as driver).
    insert_schedule(
        &pool,
        "sch_ac2_review_ondemand",
        "novel-chapter-review",
        "running",
        "wrk_ac2",
    )
    .await;
    // NOTE: deliberately do NOT call set_driver — this is the on-demand path.

    // Pre-state: no findings.
    assert_eq!(count_findings(&pool, "wrk_ac2").await, 0);

    // Terminal transition — supervisor hook fires on schedule_id +
    // preset_id match, independent of driver status.
    sup.on_schedule_terminal("sch_ac2_review_ondemand", ScheduleStatus::Completed)
        .await
        .unwrap();

    // AC2: ≥1 finding row created via the same code path as AC1.
    let n = count_findings(&pool, "wrk_ac2").await;
    assert!(
        n >= 1,
        "on-demand review terminal must persist ≥1 finding; got {n}"
    );

    // Same required-fields check (AC3) — the finding is well-formed.
    let filters = FindingListFilters {
        work_id: Some("wrk_ac2".to_string()),
        ..Default::default()
    };
    let rows = findings::list_findings(&pool, CREATOR, &filters)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let f = &rows[0];
    assert_eq!(f.work_id, "wrk_ac2");
    assert!(!f.kind.is_empty());
    assert!(!f.severity.is_empty());
    assert!(!f.target_executor.is_empty());
    assert!(!f.description.is_empty());

    // AC4 invariant: on-demand path does NOT touch driver_schedule_id.
    // The Work's driver remains None (was None at seed; never set).
    let updated = works::get_work(&pool, CREATOR, "wrk_ac2")
        .await
        .unwrap()
        .unwrap();
    assert!(
        updated.driver_schedule_id.is_none(),
        "on-demand review must NOT set driver_schedule_id; got {:?}",
        updated.driver_schedule_id
    );
}

// ── Negative: non-review schedules are no-ops for the hook ──────────────────

#[tokio::test]
async fn negative_non_review_preset_does_not_persist_finding() {
    let pool = test_pool().await;
    let sup = Arc::new(ScheduleSupervisor::new(Arc::new(pool.clone())));

    let work = novel_work("wrk_neg", 1, 3);
    works::create_work(&pool, &work).await.unwrap();

    // A `novel-writing` (produce) schedule completing must NOT create findings.
    insert_schedule(
        &pool,
        "sch_neg_produce",
        "novel-writing",
        "running",
        "wrk_neg",
    )
    .await;

    sup.on_schedule_terminal("sch_neg_produce", ScheduleStatus::Completed)
        .await
        .unwrap();

    // No findings persisted by the review hook (other hooks may create
    // schedules but none of them write findings).
    assert_eq!(
        count_findings(&pool, "wrk_neg").await,
        0,
        "non-review preset must not trigger findings hook"
    );
}

// ── AC3: rule_suggestion is stored as metadata on the finding row ───────────

#[tokio::test]
async fn ac3_rule_suggestion_field_exists_and_round_trips() {
    let pool = test_pool().await;

    let work = novel_work("wrk_ac3", 1, 3);
    works::create_work(&pool, &work).await.unwrap();

    // Use the from-review DAO directly to verify the rule_suggestion field
    // is persisted (mirrors what the supervisor hook does internally).
    let verdict = findings::ReviewVerdictFinding {
        work_id: "wrk_ac3".to_string(),
        chapter: Some(1),
        severity: "minor".to_string(),
        title: "POV drift in ch1".to_string(),
        description: "POV shifts from first to third person mid-chapter.".to_string(),
        target_executor: "write".to_string(),
        creator_id: CREATOR.to_string(),
        kind: "craft".to_string(),
        rule_suggestion: Some(
            "Consider a Layer 2 rule: pin POV per chapter in AGENTS.md.".to_string(),
        ),
        source_schedule_id: None,
    };
    let finding_id = findings::create_finding_from_review(&pool, &verdict)
        .await
        .unwrap();

    let fetched = findings::get_finding(&pool, CREATOR, &finding_id)
        .await
        .unwrap()
        .expect("finding must exist after insert");
    assert_eq!(fetched.kind, "craft");
    assert_eq!(
        fetched.rule_suggestion.as_deref(),
        Some("Consider a Layer 2 rule: pin POV per chapter in AGENTS.md.")
    );
}

// ── AC5: Idempotency — repeating the review terminal hook does not duplicate ─

/// V1.47 P0 fix (qc1 W-2 / qc2 W-1 / qc3 W-2): calling the review terminal
/// hook twice on the same chapter + schedule must produce exactly **one**
/// finding row. The `source_schedule_id` partial unique index guarantees this
/// at the DB level; `create_finding_from_review` uses `ON CONFLICT DO NOTHING`
/// and returns the existing finding id on conflict.
#[tokio::test]
async fn ac5_idempotent_review_repeat_no_duplicate_finding() {
    let pool = test_pool().await;
    let sup = Arc::new(ScheduleSupervisor::new(Arc::new(pool.clone())));

    let work = novel_work("wrk_ac5", 3, 5);
    works::create_work(&pool, &work).await.unwrap();

    insert_schedule(
        &pool,
        "sch_ac5_review",
        "novel-chapter-review",
        "running",
        "wrk_ac5",
    )
    .await;
    auto_chain::set_driver(&pool, CREATOR, "wrk_ac5", "sch_ac5_review", "review")
        .await
        .unwrap();

    // Pre-state: no findings.
    assert_eq!(count_findings(&pool, "wrk_ac5").await, 0);

    // First terminal transition — creates the finding.
    sup.on_schedule_terminal("sch_ac5_review", ScheduleStatus::Completed)
        .await
        .unwrap();
    let n1 = count_findings(&pool, "wrk_ac5").await;
    assert_eq!(n1, 1, "first terminal must create exactly 1 finding; got {n1}");

    // Simulate a second terminal transition for the SAME schedule (e.g.
    // supervisor retry or double-fire). The schedule row was already flipped
    // to 'completed' by the first call, so we reset it to 'running' to make
    // the second on_schedule_terminal accept the transition.
    sqlx::query("UPDATE creator_schedules SET status = 'running' WHERE schedule_id = ?")
        .bind("sch_ac5_review")
        .execute(&pool)
        .await
        .unwrap();

    sup.on_schedule_terminal("sch_ac5_review", ScheduleStatus::Completed)
        .await
        .unwrap();

    // AC5: still exactly 1 finding — the ON CONFLICT DO NOTHING guard
    // prevented the duplicate.
    let n2 = count_findings(&pool, "wrk_ac5").await;
    assert_eq!(
        n2, 1,
        "second terminal for the same schedule must NOT create a duplicate; got {n2}"
    );
}
