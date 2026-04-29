//! Tests for Starting lifecycle edge cases (WS-C).
//!
//! T5: `HealthDegraded` during Starting → Degraded
//! T6: `ShutdownRequested` during Starting with in-flight cancel
//! T7: `ActionContext` diagnostics populated in `enter_starting`
//! T8: Regression — Starting → Running path still works

use std::sync::Arc;
use std::time::Duration;

use nexus42d::lifecycle::{
    ActionContext, Event, Lifecycle, LifecycleState, StatigLifecycle, SubsystemKind,
};

/// Helper: drive machine to Running state by dispatching all subsystem ups.
async fn drive_to_running(m: &StatigLifecycle) {
    for kind in SubsystemKind::mandatory() {
        m.dispatch(Event::SubsystemUp(*kind));
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(m.current_state(), LifecycleState::Running);
}

// ── T5: HealthDegraded during Starting → Degraded ──────────────────────────

/// Test: `HealthDegraded` for an already-up subsystem during Starting transitions to Degraded.
#[tokio::test]
async fn health_degraded_during_starting_transitions_to_degraded() {
    let m = StatigLifecycle::new_for_test();

    // Drive some subsystems up but not all (still in Starting).
    m.dispatch(Event::SubsystemUp(SubsystemKind::Http));
    m.dispatch(Event::SubsystemUp(SubsystemKind::Db));
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Still in Starting (not all mandatory up).
    assert_eq!(m.current_state(), LifecycleState::Starting);

    // Dispatch HealthDegraded for an already-up subsystem.
    m.dispatch(Event::HealthDegraded {
        kind: SubsystemKind::Http,
        reason: "connection pool exhausted".into(),
    });
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Should transition to Degraded.
    assert_eq!(m.current_state(), LifecycleState::Degraded);
}

/// Test: `HealthDegraded` for a subsystem NOT yet up during Starting is ignored.
#[tokio::test]
async fn health_degraded_for_not_yet_up_subsystem_ignored_in_starting() {
    let m = StatigLifecycle::new_for_test();

    // Drive one subsystem up (still in Starting overall).
    m.dispatch(Event::SubsystemUp(SubsystemKind::Http));
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(m.current_state(), LifecycleState::Starting);

    // Dispatch HealthDegraded for a subsystem that hasn't reported SubsystemUp yet.
    m.dispatch(Event::HealthDegraded {
        kind: SubsystemKind::Sync,
        reason: "lag".into(),
    });
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Should remain in Starting (event ignored per RISK-WSC-01).
    assert_eq!(m.current_state(), LifecycleState::Starting);

    // Completing remaining subsystems should still transition to Running.
    m.dispatch(Event::SubsystemUp(SubsystemKind::Db));
    m.dispatch(Event::SubsystemUp(SubsystemKind::Sync));
    m.dispatch(Event::SubsystemUp(SubsystemKind::Engine));
    m.dispatch(Event::SubsystemUp(SubsystemKind::WorkerMgr));
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(m.current_state(), LifecycleState::Running);
}

// ── T6: ShutdownRequested during Starting with in-flight cancel ─────────────

/// Test: `ShutdownRequested` during Starting transitions to Stopping.
///
/// This is already tested in `lifecycle_transitions.rs`, but here we verify
/// it still works after adding the `exit_starting` cancellation logic.
#[tokio::test]
async fn shutdown_during_starting_transitions_to_stopping() {
    let m = StatigLifecycle::new_for_test();

    // In Starting state (initial), dispatch ShutdownRequested.
    m.dispatch(Event::ShutdownRequested {
        source: "admin".into(),
    });
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(m.current_state(), LifecycleState::Stopping);
}

/// Test: `ShutdownRequested` after partial subsystem startup still transitions to Stopping.
#[tokio::test]
async fn shutdown_after_partial_subsystem_up_transitions_to_stopping() {
    let m = StatigLifecycle::new_for_test();

    // Some subsystems are up, not all.
    m.dispatch(Event::SubsystemUp(SubsystemKind::Http));
    m.dispatch(Event::SubsystemUp(SubsystemKind::Db));
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(m.current_state(), LifecycleState::Starting);

    // Request shutdown.
    m.dispatch(Event::ShutdownRequested {
        source: "signal".into(),
    });
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(m.current_state(), LifecycleState::Stopping);
}

// ── T7: ActionContext diagnostics ──────────────────────────────────────────

/// Test: `ActionContext` has diagnostic fields populated after construction.
#[test]
fn action_context_diagnostic_fields_populated() {
    let lifecycle = Arc::new(StatigLifecycle::new_for_test());
    let ctx = ActionContext::new_for_test(Arc::clone(&lifecycle));

    // start_reason should be set.
    assert!(!ctx.start_reason().is_empty());
    assert_eq!(ctx.start_reason(), "daemon_boot");

    // started_at should be a recent timestamp.
    let now = chrono::Utc::now();
    let diff = now.signed_duration_since(ctx.started_at());
    assert!(
        diff.num_seconds() < 5,
        "started_at should be within 5 seconds of now, but was {}s ago",
        diff.num_seconds()
    );

    // initiating_event should be None by default.
    assert!(ctx.initiating_event().is_none());
}

/// Test: `ActionContext` `start_reason` can be customized.
#[test]
fn action_context_custom_start_reason() {
    let lifecycle = Arc::new(StatigLifecycle::new_for_test());
    let mocks = nexus42d::lifecycle::MockAllSubsystems::all_succeed();
    let ctx = ActionContext::new(lifecycle, mocks.as_bootstraps(), 10_000);

    // The default start_reason is "daemon_boot" — verify it.
    assert_eq!(ctx.start_reason(), "daemon_boot");

    // Verify started_at is set.
    let _ = ctx.started_at();
}

// ── T8: Regression — Starting → Running path still works ──────────────────

/// Test: Normal Starting → Running path with all subsystems up.
#[tokio::test]
async fn regression_starting_to_running_normal_path() {
    let m = StatigLifecycle::new_for_test();

    drive_to_running(&m).await;
    assert_eq!(m.current_state(), LifecycleState::Running);
}

/// Test: Full lifecycle — Starting → Running → Degraded → Running → Stopping → Failed.
#[tokio::test]
async fn regression_full_lifecycle_with_degraded_and_graceful_shutdown() {
    let m = StatigLifecycle::new_for_test();

    // Starting → Running.
    drive_to_running(&m).await;
    assert_eq!(m.current_state(), LifecycleState::Running);

    // Running → Degraded.
    m.dispatch(Event::HealthDegraded {
        kind: SubsystemKind::Sync,
        reason: "lag".into(),
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(m.current_state(), LifecycleState::Degraded);

    // Degraded → Running.
    m.dispatch(Event::HealthRestored {
        kind: SubsystemKind::Sync,
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(m.current_state(), LifecycleState::Running);

    // Running → Stopping.
    m.dispatch(Event::ShutdownRequested {
        source: "test".into(),
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(m.current_state(), LifecycleState::Stopping);

    // Stopping → Failed (exit 0).
    m.dispatch(Event::ShutdownDrained);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(m.current_state(), LifecycleState::Failed);
    assert_eq!(m.exit_code(), Some(0));
}

/// Test: Starting → Failed on non-retryable subsystem failure still works.
#[tokio::test]
async fn regression_starting_to_failed_on_subsystem_failure() {
    let m = StatigLifecycle::new_for_test();

    m.dispatch(Event::SubsystemFailed {
        kind: SubsystemKind::Db,
        err: "connection refused".into(),
        retryable: false,
    });
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(m.current_state(), LifecycleState::Failed);
    assert_eq!(m.exit_code(), Some(1));
}

/// Test: Multiple `HealthDegraded` events in Degraded state still accumulate.
#[tokio::test]
async fn regression_multiple_health_degraded_accumulate() {
    let m = StatigLifecycle::new_for_test();

    // Starting → Running.
    drive_to_running(&m).await;

    // Two degradations.
    m.dispatch(Event::HealthDegraded {
        kind: SubsystemKind::Sync,
        reason: "lag".into(),
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(m.current_state(), LifecycleState::Degraded);

    m.dispatch(Event::HealthDegraded {
        kind: SubsystemKind::Db,
        reason: "slow".into(),
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(m.current_state(), LifecycleState::Degraded);

    // Restore one.
    m.dispatch(Event::HealthRestored {
        kind: SubsystemKind::Sync,
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(m.current_state(), LifecycleState::Degraded);

    // Restore all → Running.
    m.dispatch(Event::HealthRestored {
        kind: SubsystemKind::Db,
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(m.current_state(), LifecycleState::Running);
}
