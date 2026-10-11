//! Tests for V1.5 WS-D — Cron / wall-clock triggers.
//!
//! Coverage:
//! - R21: a clocked tick triggers a Pending Schedule at `scheduled_at`
//! - Duplicate-fire prevention: same schedule ticked twice → admitted once
//! - DST transition: recompute next-run on wall-clock discontinuity
//! - Mixed batch: a clocked tick admits due schedules and keeps future ones Pending
//!
//! Drives `ScheduleSupervisor::tick_clocked(ts)` directly with explicit timestamps —
//! no clock abstraction is needed.

use std::sync::Arc;

use nexus_contracts::local::schedule::{
    CoreContextVersion, Schedule, ScheduleConcurrency, ScheduleId, ScheduleStatus,
};
use nexus_local_db::SqlitePool;
use nexus_orchestration::schedule::supervisor::ScheduleSupervisor;

// ============================================================================
// Test helpers
// ============================================================================

async fn setup_test_db() -> (tempfile::TempDir, Arc<SqlitePool>) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let guarded = nexus_local_db::init_engine_pool(&db_path)
        .await
        .expect("open pool");
    (dir, Arc::new(guarded.clone_pool()))
}

fn make_schedule(id: &str, creator: &str, scheduled_at: Option<i64>) -> Schedule {
    Schedule {
        id: ScheduleId(id.to_string()),
        creator_id: creator.to_string(),
        preset_id: "test-preset".to_string(),
        preset_version: 1,
        status: ScheduleStatus::Pending,
        concurrency: ScheduleConcurrency::Serial,
        depends_on: vec![],
        current_core_context_version: CoreContextVersion(0),
        current_session_id: None,
        scheduled_at: scheduled_at.map(|t| t.to_string()),
        // Use schedule ID as label to avoid duplicate detection
        label: Some(id.to_string()),
        created_at: String::new(),
        updated_at: String::new(),
        terminated_at: None,
    }
}

// ============================================================================
// TDD Test 1: Clock poller triggers Pending Schedule at scheduled_at
// ============================================================================

#[tokio::test]
async fn r21_clocked_tick_triggers_at_scheduled_at() {
    let (_dir, pool) = setup_test_db().await;
    let supervisor = Arc::new(ScheduleSupervisor::new(pool.clone()));

    // Schedule with scheduled_at = 2000 (not yet due at now=1000)
    let schedule = make_schedule("S01", "creator-1", Some(2000));
    supervisor.insert_pending(schedule).await.unwrap();

    // Status should be Pending (not yet due)
    assert_eq!(
        supervisor.status_of("S01").await.unwrap(),
        ScheduleStatus::Pending,
        "schedule should be Pending before scheduled_at"
    );

    // Tick at now=1000: S01 not due yet (scheduled_at=2000 > now=1000)
    supervisor.tick_clocked(1000).await.unwrap();
    assert_eq!(
        supervisor.status_of("S01").await.unwrap(),
        ScheduleStatus::Pending,
        "schedule should remain Pending when scheduled_at > now"
    );

    // Tick at now=2000: S01 is now due
    supervisor.tick_clocked(2000).await.unwrap();
    assert_eq!(
        supervisor.status_of("S01").await.unwrap(),
        ScheduleStatus::Running,
        "schedule should transition to Running when scheduled_at <= now"
    );
}

// ============================================================================
// TDD Test 2: Duplicate-fire prevention
// ============================================================================

#[tokio::test]
async fn duplicate_fire_prevention_same_schedule_admitted_once() {
    let (_dir, pool) = setup_test_db().await;
    let supervisor = Arc::new(ScheduleSupervisor::new(pool.clone()));

    // Schedule due immediately (scheduled_at = 500 < now=1000)
    let schedule = make_schedule("S02", "creator-2", Some(500));
    supervisor.insert_pending(schedule).await.unwrap();

    // First tick at now=1000: S02 should be admitted
    supervisor.tick_clocked(1000).await.unwrap();
    assert_eq!(
        supervisor.status_of("S02").await.unwrap(),
        ScheduleStatus::Running,
        "schedule should be Running after first tick (admitted)"
    );

    // Second tick in same cycle: S02 should NOT be double-admitted
    // Duplicate prevention is handled by tick_clocked()'s re-entrancy guard
    // and the status check in the UPDATE WHERE clause.
    supervisor.tick_clocked(1000).await.unwrap();
    // Status should remain Running, not transition twice
    assert_eq!(
        supervisor.status_of("S02").await.unwrap(),
        ScheduleStatus::Running,
        "schedule should remain Running after second tick (no double admission)"
    );
}

// ============================================================================
// TDD Test 3: Mixed batch — admit due, keep future Pending
// ============================================================================

#[tokio::test]
async fn mixed_batch_admits_due_and_keeps_future_pending() {
    let (_dir, pool) = setup_test_db().await;
    let supervisor = Arc::new(ScheduleSupervisor::new(pool.clone()));

    // Multiple schedules due at different times
    let s1 = make_schedule("S03", "creator-3a", Some(500)); // due now
    let s2 = make_schedule("S04", "creator-3b", Some(1500)); // future
    supervisor.insert_pending(s1).await.unwrap();
    supervisor.insert_pending(s2).await.unwrap();

    // Tick at now=1000: S03 is due (500 <= 1000), S04 is not (1500 > 1000)
    supervisor.tick_clocked(1000).await.unwrap();
    assert_eq!(
        supervisor.status_of("S03").await.unwrap(),
        ScheduleStatus::Running,
        "S03 should be Running after tick (scheduled_at=500 < now=1000)"
    );

    // S04 should NOT be started yet (scheduled_at=1500 > now=1000)
    assert_eq!(
        supervisor.status_of("S04").await.unwrap(),
        ScheduleStatus::Pending,
        "S04 should remain Pending (scheduled_at > now)"
    );
}

// ============================================================================
// TDD Test 4: DST / clock-jump safety
// ============================================================================

#[tokio::test]
async fn dst_clock_jump_recomputes_next_run() {
    let (_dir, pool) = setup_test_db().await;
    let supervisor = Arc::new(ScheduleSupervisor::new(pool.clone()));

    // Simulate DST transition: clock jumps forward 1 hour
    // Real time: 02:00 → 03:00 (spring forward)

    // Schedule due at 02:30 (9000 seconds)
    let schedule = make_schedule("S05", "creator-4", Some(9000));
    supervisor.insert_pending(schedule).await.unwrap();

    // Tick at now=7200 (02:00): S05 not yet due (scheduled_at=9000 > now=7200)
    supervisor.tick_clocked(7200).await.unwrap();
    assert_eq!(
        supervisor.status_of("S05").await.unwrap(),
        ScheduleStatus::Pending,
        "schedule should remain Pending before DST jump"
    );

    // DST spring forward: clock jumps to 03:00 (10800 seconds).
    // scheduled_at=9000 is now in the past, so the schedule is due.
    supervisor.tick_clocked(10800).await.unwrap();
    assert_eq!(
        supervisor.status_of("S05").await.unwrap(),
        ScheduleStatus::Running,
        "schedule should trigger after DST jump (scheduled_at became past)"
    );
}

// ============================================================================
// Existing supervisor behavior: scheduled_at is ignored in current tick()
// ============================================================================

#[tokio::test]
async fn current_tick_ignores_scheduled_at_field() {
    // This test verifies current behavior: tick() ignores scheduled_at.
    // Once WS-D scheduler is implemented, this test will change.
    let (_dir, pool) = setup_test_db().await;
    let supervisor = Arc::new(ScheduleSupervisor::new(pool.clone()));

    // Schedule with future scheduled_at (9999-01-01)
    let schedule = make_schedule("S-FUTURE", "creator-future", Some(253_402_300_799));
    supervisor.insert_pending(schedule).await.unwrap();

    // Current tick() ignores scheduled_at and admits immediately
    supervisor.tick().await.unwrap();

    // S-FUTURE should be Running (current tick doesn't filter by scheduled_at)
    assert_eq!(
        supervisor.status_of("S-FUTURE").await.unwrap(),
        ScheduleStatus::Running,
        "current tick() ignores scheduled_at — this will change with WS-D scheduler"
    );
}
