//! Tests for lifecycle entry/exit actions with subsystem bootstrap.
//!
//! Per plan §Task 4: verify Starting.entry spawns subsystem tasks and
//! dispatches SubsystemUp/SubsystemFailed events.

use std::time::Duration;

use nexus42d::lifecycle::{
    Event, Lifecycle, LifecycleState, StatigLifecycle, SubsystemKind,
    MockAllSubsystems,
};

/// Test: Starting entry action spawns subsystem tasks that dispatch SubsystemUp.
///
/// This test verifies that when the lifecycle is created with real subsystems
/// (mocks that succeed), the Starting.entry action spawns tasks that
/// dispatch SubsystemUp events, leading to Running state.
#[tokio::test]
async fn starting_entry_dispatches_subsystem_up_for_mock() {
    // Create mock subsystems that all succeed on startup.
    let mocks = MockAllSubsystems::all_succeed();
    let subsystems = mocks.as_bootstraps();

    // Create lifecycle with subsystems (this will spawn tasks on enter_starting).
    let lifecycle = StatigLifecycle::new_with_subsystems(subsystems, 20_000);

    // Dispatch ProcessStarted to trigger the entry action.
    // Note: enter_starting is called automatically when entering Starting state,
    // which happens on initial state. But we need to trigger the subsystem spawns.
    // In the current design, enter_starting is called when entering Starting.
    lifecycle.dispatch(Event::ProcessStarted);

    // Wait for subsystem tasks to complete and dispatch SubsystemUp events.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Should be in Running state (all 5 mandatory subsystems up).
    assert_eq!(lifecycle.current_state(), LifecycleState::Running);
}

/// Test: Starting entry handles subsystem failure.
///
/// When a subsystem fails to start, it should dispatch SubsystemFailed
/// and transition to Failed state.
#[tokio::test]
async fn starting_entry_handles_subsystem_failure() {
    // Create mock subsystems where DB fails.
    let mocks = MockAllSubsystems::one_fails(SubsystemKind::Db);
    let subsystems = mocks.as_bootstraps();

    // Create lifecycle with subsystems.
    let lifecycle = StatigLifecycle::new_with_subsystems(subsystems, 20_000);

    // Trigger the lifecycle.
    lifecycle.dispatch(Event::ProcessStarted);

    // Wait for subsystem tasks to complete.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Should be in Failed state due to DB failure.
    assert_eq!(lifecycle.current_state(), LifecycleState::Failed);
    assert_eq!(lifecycle.exit_code(), Some(1));
}

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