//! Tests for Schedule admission logic (WS7 T2).
//!
//! Admission rules per spec §5.1:
//! - Serial: blocks if ANY running schedule exists for the creator
//! - ParallelWith(ids): admits only if ALL listed IDs are in the running set
//! - ParallelAny: always admits (ACP serialization at dispatch site, not here)
//! - Dependencies: `depends_on` must all be in Completed/Cancelled status

use std::sync::Arc;

use nexus_contracts::local::schedule::{
    CoreContextVersion, ParallelWithIds, Schedule, ScheduleConcurrency, ScheduleId, ScheduleStatus,
};
use nexus_orchestration::schedule::admission::{admit, CompletedSet, RunningSet};
use nexus_orchestration::schedule::supervisor::ScheduleSupervisor;

/// Helper: build a minimal [`Schedule`] for admission testing.
fn sched(id: &str, c: ScheduleConcurrency, deps: Vec<&str>) -> Schedule {
    Schedule {
        id: ScheduleId(id.to_string()),
        creator_id: "c".to_string(),
        preset_id: "p".to_string(),
        preset_version: 1,
        status: ScheduleStatus::Pending,
        concurrency: c,
        depends_on: deps
            .into_iter()
            .map(|d| ScheduleId(d.to_string()))
            .collect(),
        current_core_context_version: CoreContextVersion(0),
        current_session_id: None,
        scheduled_at: None,
        label: None,
        created_at: String::new(),
        updated_at: String::new(),
        terminated_at: None,
    }
}

// ── Admission unit tests ───────────────────────────────────────────────

#[test]
fn serial_blocks_behind_any_running() {
    let pending = sched("01A", ScheduleConcurrency::Serial, vec![]);
    let running = RunningSet::from(vec![sched("01B", ScheduleConcurrency::Serial, vec![])]);
    assert!(!admit(&pending, &running, &CompletedSet::empty()));
}

#[test]
fn parallel_with_admits_when_all_running_are_whitelisted() {
    let running_id = ScheduleId("01B".to_string());
    let pending = sched(
        "01A",
        ScheduleConcurrency::ParallelWith(ParallelWithIds {
            schedule_ids: vec![running_id.clone()],
        }),
        vec![],
    );
    let running = RunningSet::from(vec![sched("01B", ScheduleConcurrency::Serial, vec![])]);
    assert!(admit(&pending, &running, &CompletedSet::empty()));
}

#[test]
fn parallel_with_blocks_when_running_not_in_whitelist() {
    // 01A can only run with 01B, but 01C is running (not whitelisted)
    let pending = sched(
        "01A",
        ScheduleConcurrency::ParallelWith(ParallelWithIds {
            schedule_ids: vec![ScheduleId("01B".to_string())],
        }),
        vec![],
    );
    let running = RunningSet::from(vec![sched("01C", ScheduleConcurrency::Serial, vec![])]);
    assert!(!admit(&pending, &running, &CompletedSet::empty()));
}

#[test]
fn parallel_any_always_admits_subject_to_worker_cap() {
    let pending = sched("01A", ScheduleConcurrency::ParallelAny, vec![]);
    let running = RunningSet::from(vec![sched("01B", ScheduleConcurrency::Serial, vec![])]);
    assert!(admit(&pending, &running, &CompletedSet::empty())); // ACP-busy serialization is enforced at dispatch site, not here
}

#[test]
fn dep_unsatisfied_blocks_start() {
    let pending = sched("01A", ScheduleConcurrency::Serial, vec!["01B"]);
    let running = RunningSet::empty(); // 01B not completed → blocked
    assert!(!admit(&pending, &running, &CompletedSet::empty())); // deps not satisfied
}

#[test]
fn dep_satisfied_allows_start() {
    let pending = sched("01A", ScheduleConcurrency::Serial, vec!["01B"]);
    let running = RunningSet::empty();
    let completed = CompletedSet::from(vec![ScheduleId("01B".to_string())]);
    assert!(admit(&pending, &running, &completed));
}

#[test]
fn serial_admits_when_nothing_running_and_no_deps() {
    let pending = sched("01A", ScheduleConcurrency::Serial, vec![]);
    assert!(admit(
        &pending,
        &RunningSet::empty(),
        &CompletedSet::empty()
    ));
}

#[test]
fn parallel_with_admits_when_nothing_running() {
    let pending = sched(
        "01A",
        ScheduleConcurrency::ParallelWith(ParallelWithIds {
            schedule_ids: vec![ScheduleId("01B".to_string())],
        }),
        vec![],
    );
    // Nothing running → admission passes (whitelist constraint is vacuously true)
    assert!(admit(
        &pending,
        &RunningSet::empty(),
        &CompletedSet::empty()
    ));
}

#[test]
fn dep_failed_blocks_start() {
    // depends_on must be Completed or Cancelled; Failed does not satisfy.
    let pending = sched("01A", ScheduleConcurrency::Serial, vec!["01B"]);
    let running = RunningSet::empty();
    let completed = CompletedSet::empty(); // 01B is Failed, not in completed set
    assert!(!admit(&pending, &running, &completed));
}

// ── Integration test: two serial schedules hand-off ────────────────────

#[tokio::test]
async fn two_serial_schedules_hand_off_after_first_completes() {
    let supervisor = test_supervisor_with_inmemory_db().await;

    // Add two serial schedules for the same creator
    supervisor
        .insert_pending(sched("01A", ScheduleConcurrency::Serial, vec![]))
        .await
        .unwrap();
    supervisor
        .insert_pending(sched("01B", ScheduleConcurrency::Serial, vec![]))
        .await
        .unwrap();

    // Tick: 01A should start, 01B stays Pending (serial blocks behind running)
    supervisor.tick().await.unwrap();
    assert_eq!(
        supervisor.status_of("01A").await.unwrap(),
        ScheduleStatus::Running
    );
    assert_eq!(
        supervisor.status_of("01B").await.unwrap(),
        ScheduleStatus::Pending
    );

    // Simulate 01A completing (no real session engine — direct status flip)
    supervisor
        .on_schedule_terminal("01A", ScheduleStatus::Completed)
        .await
        .unwrap();

    // After terminal callback: tick is triggered internally.
    // 01B should now be Running since nothing else is.
    assert_eq!(
        supervisor.status_of("01B").await.unwrap(),
        ScheduleStatus::Running
    );
}

/// Helper: create a [`ScheduleSupervisor`] backed by a fresh temp SQLite DB
/// with the WS7 migration applied.
async fn test_supervisor_with_inmemory_db() -> Arc<ScheduleSupervisor> {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = nexus_local_db::open_pool(&db_path)
        .await
        .expect("open pool");
    nexus_local_db::run_migrations(&pool)
        .await
        .expect("run migrations");
    // Keep dir alive by leaking it — fine for tests
    std::mem::forget(dir);
    Arc::new(ScheduleSupervisor::new(Arc::new(pool)))
}
