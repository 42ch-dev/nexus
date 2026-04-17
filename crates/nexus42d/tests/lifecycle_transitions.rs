//! Tests for lifecycle HSM transitions.
//!
//! Per plan §Task 3: verify transitions match spec §4 matrix.

use nexus42d::lifecycle::{Event, Lifecycle, LifecycleState, StatigLifecycle, SubsystemKind};

/// Helper: drive machine to Running state by dispatching all subsystem ups.
async fn drive_to_running(m: &StatigLifecycle) {
    m.dispatch(Event::ProcessStarted);
    for kind in SubsystemKind::mandatory() {
        m.dispatch(Event::SubsystemUp(*kind));
    }
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(m.current_state(), LifecycleState::Running);
}

/// Helper: drive machine to Stopping state.
async fn drive_to_stopping(m: &StatigLifecycle) {
    drive_to_running(m).await;
    m.dispatch(Event::ShutdownRequested {
        source: "test".into(),
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(m.current_state(), LifecycleState::Stopping);
}

/// Test: Starting → Running on all mandatory subsystems up.
#[tokio::test]
async fn starting_to_running_on_all_subsystems_up() {
    let m = StatigLifecycle::new_for_test();

    // Dispatch all mandatory subsystems up.
    for kind in SubsystemKind::mandatory() {
        m.dispatch(Event::SubsystemUp(*kind));
    }

    // Give async dispatch a tick:
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    assert_eq!(m.current_state(), LifecycleState::Running);
}

/// Test: Running → Degraded → Running cycle.
#[tokio::test]
async fn running_to_degraded_to_running() {
    let m = StatigLifecycle::new_for_test();

    // Drive to Running first.
    drive_to_running(&m).await;

    // Dispatch HealthDegraded.
    m.dispatch(Event::HealthDegraded {
        kind: SubsystemKind::Sync,
        reason: "lag".into(),
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(m.current_state(), LifecycleState::Degraded);

    // Dispatch HealthRestored.
    m.dispatch(Event::HealthRestored {
        kind: SubsystemKind::Sync,
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(m.current_state(), LifecycleState::Running);
}

/// Test: Alive → Stopping → Failed on ShutdownDrained (exit 0).
#[tokio::test]
async fn alive_to_stopping_to_failed_on_drained() {
    let m = StatigLifecycle::new_for_test();

    // Drive to Running.
    drive_to_running(&m).await;

    // Dispatch ShutdownRequested.
    m.dispatch(Event::ShutdownRequested {
        source: "signal".into(),
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(m.current_state(), LifecycleState::Stopping);

    // Dispatch ShutdownDrained.
    m.dispatch(Event::ShutdownDrained);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(m.current_state(), LifecycleState::Failed);

    // Exit code should be 0 (graceful completion).
    assert_eq!(m.exit_code(), Some(0));
}

/// Test: Stopping → Failed on ShutdownTimeout (exit 1).
#[tokio::test]
async fn stopping_to_failed_on_timeout() {
    let m = StatigLifecycle::new_for_test();

    // Drive to Stopping first.
    drive_to_stopping(&m).await;

    // Dispatch ShutdownTimeout.
    m.dispatch(Event::ShutdownTimeout {
        grace_ms_exceeded: 20_000,
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(m.current_state(), LifecycleState::Failed);

    // Exit code should be 1 (timeout forced exit).
    assert_eq!(m.exit_code(), Some(1));
}

/// Test: Starting handles ShutdownRequested → Stopping (abort-on-start).
#[tokio::test]
async fn starting_to_stopping_on_shutdown_requested() {
    let m = StatigLifecycle::new_for_test();

    // In Starting state (initial), dispatch ShutdownRequested.
    m.dispatch(Event::ShutdownRequested {
        source: "admin".into(),
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(m.current_state(), LifecycleState::Stopping);
}

/// Test: SubsystemFailed with retryable=false transitions to Failed.
#[tokio::test]
async fn subsystem_failed_non_retryable_transitions_to_failed() {
    let m = StatigLifecycle::new_for_test();

    m.dispatch(Event::SubsystemFailed {
        kind: SubsystemKind::Db,
        err: "connection refused".into(),
        retryable: false,
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(m.current_state(), LifecycleState::Failed);
    assert_eq!(m.exit_code(), Some(1));
}

/// Test: FatalError from alive superstate transitions to Failed.
#[tokio::test]
async fn fatal_error_from_alive_to_failed() {
    let m = StatigLifecycle::new_for_test();

    // Drive to Running first.
    drive_to_running(&m).await;

    m.dispatch(Event::FatalError {
        kind: SubsystemKind::Http,
        err: "listener dead".into(),
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(m.current_state(), LifecycleState::Failed);
    assert_eq!(m.exit_code(), Some(1));
}

/// Test: Multiple HealthDegraded events accumulate in Degraded state.
#[tokio::test]
async fn multiple_health_degraded_accumulate() {
    let m = StatigLifecycle::new_for_test();

    // Drive to Running first.
    drive_to_running(&m).await;

    // First degradation.
    m.dispatch(Event::HealthDegraded {
        kind: SubsystemKind::Sync,
        reason: "lag".into(),
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(m.current_state(), LifecycleState::Degraded);

    // Second degradation (different subsystem).
    m.dispatch(Event::HealthDegraded {
        kind: SubsystemKind::Db,
        reason: "slow".into(),
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    // Still in Degraded (not Failed).
    assert_eq!(m.current_state(), LifecycleState::Degraded);

    // Restore one subsystem.
    m.dispatch(Event::HealthRestored {
        kind: SubsystemKind::Sync,
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    // Still Degraded (Db still degraded).
    assert_eq!(m.current_state(), LifecycleState::Degraded);

    // Restore remaining subsystem.
    m.dispatch(Event::HealthRestored {
        kind: SubsystemKind::Db,
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    // Now back to Running.
    assert_eq!(m.current_state(), LifecycleState::Running);
}
