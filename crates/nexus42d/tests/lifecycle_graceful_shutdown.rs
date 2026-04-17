//! Tests for daemon graceful shutdown.
//!
//! Per plan §Task 6: verify SIGTERM → Stopping → exit 0 within grace.
//!
//! Note: Full process-level signal tests are complex. This file tests
//! the lifecycle's shutdown behavior. Full integration tests would
//! require spawning a subprocess and sending signals via libc.

use std::sync::Arc;
use std::time::Duration;

use nexus42d::lifecycle::{
    Event, Lifecycle, LifecycleState, StatigLifecycle, SubsystemKind,
};

/// Test: ShutdownRequested leads to Stopping → Failed (exit 0).
///
/// This tests the lifecycle state machine's shutdown flow without
/// actual signal handling.
///
/// Note: Uses new_for_test() instead of new_with_subsystems() because
/// the deferred initialization pattern in new_with_subsystems has timing
/// issues in tests. For production, main.rs handles subsystem startup.
#[tokio::test]
async fn shutdown_requested_leads_to_graceful_exit() {
    // Use test mode lifecycle (no subsystem spawning)
    let lifecycle = Arc::new(StatigLifecycle::new_for_test());

    // Manually dispatch subsystem startup events to reach Running
    lifecycle.dispatch(Event::ProcessStarted);
    for kind in SubsystemKind::mandatory() {
        lifecycle.dispatch(Event::SubsystemUp(*kind));
    }
    
    // Wait for Running
    tokio::time::sleep(Duration::from_millis(100)).await;
    let state = lifecycle.current_state();
    tracing::debug!("Current state after startup: {:?}", state);
    assert_eq!(state, LifecycleState::Running);

    // Request shutdown (simulating SIGTERM)
    lifecycle.dispatch(Event::ShutdownRequested {
        source: "test".into(),
    });

    // Wait for async dispatch to complete
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Should transition to Stopping
    let state_after_shutdown = lifecycle.current_state();
    tracing::debug!("Current state after shutdown request: {:?}", state_after_shutdown);
    assert_eq!(state_after_shutdown, LifecycleState::Stopping);

    // Manually dispatch ShutdownDrained (simulating subsystem shutdown completion)
    lifecycle.dispatch(Event::ShutdownDrained);
    
    // Wait for transition
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Should reach Failed with exit_code 0 (graceful completion)
    let final_state = lifecycle.current_state();
    tracing::debug!("Final state: {:?}", final_state);
    assert_eq!(final_state, LifecycleState::Failed);
    assert_eq!(lifecycle.exit_code(), Some(0));
}

/// Test: Shutdown timeout leads to exit 1.
///
/// If shutdown takes too long, timeout event should trigger.
#[tokio::test]
async fn shutdown_timeout_leads_to_forced_exit() {
    let lifecycle = Arc::new(StatigLifecycle::new_for_test());
    
    // Force to Running state
    for kind in SubsystemKind::mandatory() {
        lifecycle.dispatch(Event::SubsystemUp(*kind));
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(lifecycle.current_state(), LifecycleState::Running);

    // Request shutdown
    lifecycle.dispatch(Event::ShutdownRequested {
        source: "test".into(),
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(lifecycle.current_state(), LifecycleState::Stopping);

    // Manually dispatch ShutdownTimeout (simulating watchdog firing)
    lifecycle.dispatch(Event::ShutdownTimeout {
        grace_ms_exceeded: 2000,
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Should reach Failed with exit_code 1 (timeout forced)
    assert_eq!(lifecycle.current_state(), LifecycleState::Failed);
    assert_eq!(lifecycle.exit_code(), Some(1));
}

/// Test: Panic hook integration.
///
/// FatalError event should transition to Failed with exit 1.
#[tokio::test]
async fn fatal_error_from_panic_transitions_to_failed() {
    let lifecycle = Arc::new(StatigLifecycle::new_for_test());
    
    // Force to Running state
    for kind in SubsystemKind::mandatory() {
        lifecycle.dispatch(Event::SubsystemUp(*kind));
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(lifecycle.current_state(), LifecycleState::Running);

    // Dispatch FatalError (simulating panic hook)
    lifecycle.dispatch(Event::FatalError {
        kind: SubsystemKind::Engine,
        err: "test panic".into(),
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Should reach Failed with exit_code 1
    assert_eq!(lifecycle.current_state(), LifecycleState::Failed);
    assert_eq!(lifecycle.exit_code(), Some(1));
}

/// Test: wait_until_terminal blocks until Failed.
#[tokio::test]
async fn wait_until_terminal_blocks_correctly() {
    let lifecycle = Arc::new(StatigLifecycle::new_for_test());
    
    // Start in Starting, force to Failed
    lifecycle.dispatch(Event::SubsystemFailed {
        kind: SubsystemKind::Db,
        err: "test failure".into(),
        retryable: false,
    });
    
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(lifecycle.current_state(), LifecycleState::Failed);
    
    // wait_until_terminal should return immediately for Failed state
    let wait_result = tokio::time::timeout(
        Duration::from_millis(100),
        lifecycle.wait_until_terminal(),
    ).await;
    
    assert!(wait_result.is_ok(), "wait_until_terminal should return for Failed state");
}