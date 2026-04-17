//! Tests for lifecycle entry/exit actions with subsystem bootstrap.
//!
//! Per plan §Task 4: verify subsystem bootstrap trait and lifecycle behavior.
//!
//! Note: Tests that involve subsystem spawning use new_for_test() with manual
//! SubsystemUp dispatch, because new_with_subsystems() has deferred initialization
//! that requires careful coordination (used in production main.rs, not tests).

use std::time::Duration;

use nexus42d::lifecycle::{
    Event, Lifecycle, LifecycleState, StatigLifecycle, SubsystemKind,
    MockAllSubsystems,
};

/// Test: SubsystemBootstrap trait implementations.
///
/// Verify that mock subsystems implement the trait correctly.
#[tokio::test]
async fn subsystem_bootstrap_trait_works() {
    let mocks = MockAllSubsystems::all_succeed();

    // Test each subsystem.
    for subsystem in mocks.as_bootstraps() {
        // Should succeed.
        let result = subsystem.start().await;
        assert!(result.is_ok());

        // Should be healthy.
        let health = subsystem.health().await;
        assert_eq!(health, nexus42d::lifecycle::SubsystemHealth::Up);

        // Should shutdown cleanly.
        let shutdown_result = subsystem.shutdown(1000).await;
        assert!(shutdown_result.is_ok());

        // After shutdown, should be down.
        let health_after = subsystem.health().await;
        assert_eq!(health_after, nexus42d::lifecycle::SubsystemHealth::Down);
    }
}

/// Test: Test mode lifecycle (no subsystem tasks spawned).
///
/// Verify that new_for_test() does not spawn subsystem tasks,
/// and tests can manually dispatch SubsystemUp events.
#[tokio::test]
async fn test_mode_no_subsystem_tasks_spawned() {
    let lifecycle = StatigLifecycle::new_for_test();

    // Initial state should be Starting.
    assert_eq!(lifecycle.current_state(), LifecycleState::Starting);

    // Wait a bit - no subsystem tasks should be spawned.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Still in Starting (no automatic SubsystemUp events).
    assert_eq!(lifecycle.current_state(), LifecycleState::Starting);

    // Manually dispatch SubsystemUp events to reach Running.
    for kind in SubsystemKind::mandatory() {
        lifecycle.dispatch(Event::SubsystemUp(*kind));
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Now in Running.
    assert_eq!(lifecycle.current_state(), LifecycleState::Running);
}

/// Test: Lifecycle transitions to Running when all subsystems report up.
///
/// This simulates the startup sequence without actual subsystem spawning.
#[tokio::test]
async fn lifecycle_transitions_to_running_on_subsystem_ups() {
    let lifecycle = StatigLifecycle::new_for_test();

    // Simulate startup
    lifecycle.dispatch(Event::ProcessStarted);
    
    // Dispatch all subsystem ups
    for kind in SubsystemKind::mandatory() {
        lifecycle.dispatch(Event::SubsystemUp(*kind));
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Should be in Running
    assert_eq!(lifecycle.current_state(), LifecycleState::Running);
}

/// Test: Lifecycle handles subsystem failure.
///
/// When a subsystem fails, lifecycle transitions to Failed.
#[tokio::test]
async fn lifecycle_handles_subsystem_failure() {
    let lifecycle = StatigLifecycle::new_for_test();

    // Simulate subsystem failure
    lifecycle.dispatch(Event::SubsystemFailed {
        kind: SubsystemKind::Db,
        err: "connection refused".into(),
        retryable: false,
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Should be in Failed
    assert_eq!(lifecycle.current_state(), LifecycleState::Failed);
    assert_eq!(lifecycle.exit_code(), Some(1));
}