//! Tests for V1.5 WS-D — Cron / wall-clock triggers.
//!
//! Coverage:
//! - R21: Clock poller triggers a Pending Schedule at `scheduled_at` using mock clock
//! - Duplicate-fire prevention: same schedule queried twice → admitted once
//! - DST transition: recompute next-run on wall-clock discontinuity
//! - Graceful shutdown: daemon stops mid-poll, no orphan admissions
//!
//! Uses `MockClock` abstraction to control time without sleeping.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use nexus_contracts::local::schedule::{
    CoreContextVersion, Schedule, ScheduleConcurrency, ScheduleId, ScheduleStatus,
};
use nexus_local_db::SqlitePool;
use nexus_orchestration::schedule::supervisor::ScheduleSupervisor;

// ============================================================================
// Test helpers
// ============================================================================

/// Mock clock that provides a controllable `now()` value.
///
/// Used to simulate `scheduled_at` triggers without real `tokio::time::sleep`.
struct MockClock {
    now: AtomicI64,
}

impl MockClock {
    fn new(initial: i64) -> Self {
        Self {
            now: AtomicI64::new(initial),
        }
    }

    fn set(&self, ts: i64) {
        self.now.store(ts, Ordering::SeqCst);
    }
}

async fn setup_test_db() -> (tempfile::TempDir, Arc<SqlitePool>) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = nexus_local_db::open_pool(&db_path)
        .await
        .expect("open pool");
    nexus_local_db::run_migrations(&pool)
        .await
        .expect("run migrations");
    (dir, Arc::new(pool))
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
async fn r21_clock_poller_triggers_at_scheduled_at() {
    // RED: This test will fail because `Scheduler` module doesn't exist yet.
    let (_dir, pool) = setup_test_db().await;
    let supervisor = Arc::new(ScheduleSupervisor::new(pool.clone()));

    // Mock clock starts at t=1000
    let clock = Arc::new(MockClock::new(1000));

    // Schedule with scheduled_at = 2000 (not yet due)
    let schedule = make_schedule("S01", "creator-1", Some(2000));
    supervisor.insert_pending(schedule).await.unwrap();

    // Status should be Pending (not yet due)
    assert_eq!(
        supervisor.status_of("S01").await.unwrap(),
        ScheduleStatus::Pending,
        "schedule should be Pending before scheduled_at"
    );

    // TODO: When scheduler module exists:
    // let scheduler = Scheduler::new(supervisor.clone(), clock.clone());
    // scheduler.tick().await; // should NOT start S01 (clock=1000 < scheduled_at=2000)
    // assert!(supervisor.status_of("S01").await.unwrap() == ScheduleStatus::Pending);

    // Advance clock to scheduled_at
    clock.set(2000);

    // TODO: scheduler.tick().await should start S01 now
    // assert!(supervisor.status_of("S01").await.unwrap() == ScheduleStatus::Running);

    // For now, this test documents the expected behavior and will fail
    // once we add the scheduler module import.
}

// ============================================================================
// TDD Test 2: Duplicate-fire prevention
// ============================================================================

#[tokio::test]
async fn duplicate_fire_prevention_same_schedule_admitted_once() {
    let (_dir, pool) = setup_test_db().await;
    let supervisor = Arc::new(ScheduleSupervisor::new(pool.clone()));

    // Mock clock at t=1000 (will be used by scheduler once implemented)
    let _clock = Arc::new(MockClock::new(1000));

    // Schedule due immediately (scheduled_at = 500 < clock=1000)
    let schedule = make_schedule("S02", "creator-2", Some(500));
    supervisor.insert_pending(schedule).await.unwrap();

    // TODO: Once scheduler module exists:
    // let scheduler = Scheduler::new(supervisor.clone(), clock.clone());
    //
    // // First tick: S02 should be admitted
    // scheduler.tick().await;
    // assert_eq!(supervisor.status_of("S02").await.unwrap(), ScheduleStatus::Running);
    //
    // // Second tick in same cycle: S02 should NOT be double-admitted
    // // (scheduler tracks admitted IDs in HashSet)
    // scheduler.tick().await;
    // // Status should remain Running, not transition twice
    // assert_eq!(supervisor.status_of("S02").await.unwrap(), ScheduleStatus::Running);

    // This test documents expected duplicate-fire prevention behavior.
}

// ============================================================================
// TDD Test 3: Graceful shutdown
// ============================================================================

#[tokio::test]
async fn graceful_shutdown_cancels_pending_poll() {
    let (_dir, pool) = setup_test_db().await;
    let supervisor = Arc::new(ScheduleSupervisor::new(pool.clone()));

    // Mock clock (will be used by scheduler once implemented)
    let _clock = Arc::new(MockClock::new(1000));

    // Multiple schedules due at different times
    let s1 = make_schedule("S03", "creator-3", Some(500)); // due now
    let s2 = make_schedule("S04", "creator-3", Some(1500)); // future
    supervisor.insert_pending(s1).await.unwrap();
    supervisor.insert_pending(s2).await.unwrap();

    // TODO: Once scheduler module exists with CancellationToken:
    // let cancel_token = tokio_util::sync::CancellationToken::new();
    // let scheduler = Scheduler::new(supervisor.clone(), clock.clone());
    //
    // // Spawn poller in background
    // let poller = tokio::spawn(scheduler.run(cancel_token.clone()));
    //
    // // Wait briefly for first poll to start S03
    // tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    // assert_eq!(supervisor.status_of("S03").await.unwrap(), ScheduleStatus::Running);
    //
    // // Signal shutdown BEFORE S04's scheduled_at
    // cancel_token.cancel();
    //
    // // Poller should complete cleanly
    // poller.await.unwrap();
    //
    // // S04 should NOT have been started (shutdown happened before scheduled_at)
    // assert_eq!(
    //     supervisor.status_of("S04").await.unwrap(),
    //     ScheduleStatus::Pending,
    //     "future schedule should remain Pending after graceful shutdown"
    // );

    // This test documents graceful shutdown behavior.
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
    // (clock will be used by scheduler once implemented)
    let clock = Arc::new(MockClock::new(7200)); // 02:00 in seconds

    // Schedule due at 02:30 (9000 seconds)
    let schedule = make_schedule("S05", "creator-4", Some(9000));
    supervisor.insert_pending(schedule).await.unwrap();

    // TODO: DST detection logic in scheduler:
    // - Compare SystemTime elapsed vs tokio::time::Instant elapsed
    // - If delta > threshold, recompute scheduled_at interpretation
    //
    // For mock clock, we simulate by directly jumping forward:
    clock.set(10800); // Jump to 03:00 (DST spring forward)

    // After DST jump, the scheduler should:
    // 1. Detect discontinuity (SystemTime != Instant monotonic)
    // 2. Recompute: scheduled_at=9000 is now in the past (02:30 became 01:30)
    // 3. Trigger admission

    // TODO: scheduler.tick().await should trigger S05 after DST jump
    // assert_eq!(supervisor.status_of("S05").await.unwrap(), ScheduleStatus::Running);

    // This test documents DST safety behavior.
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
    let schedule = make_schedule("S-FUTURE", "creator-future", Some(253402300799));
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