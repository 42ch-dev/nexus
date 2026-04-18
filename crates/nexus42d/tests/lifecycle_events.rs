//! Tests for lifecycle Event enum and LifecycleState.
//!
//! Per plan §Task 2: ensure all variants match spec §3.

use nexus42d::lifecycle::{Event, LifecycleState, SubsystemKind};

#[test]
fn event_variants_cover_spec_section_3() {
    // Each variant names a spec row — ensure we construct them:
    let _ = Event::ProcessStarted;
    let _ = Event::SubsystemUp(SubsystemKind::Http);
    let _ = Event::SubsystemFailed {
        kind: SubsystemKind::Db,
        err: "boom".into(),
        retryable: false,
    };
    let _ = Event::HealthDegraded {
        kind: SubsystemKind::Sync,
        reason: "lag".into(),
    };
    let _ = Event::HealthRestored {
        kind: SubsystemKind::Sync,
    };
    let _ = Event::ShutdownRequested {
        source: "signal".into(),
    };
    let _ = Event::ShutdownDrained;
    let _ = Event::ShutdownTimeout {
        grace_ms_exceeded: 20_000,
    };
    let _ = Event::FatalError {
        kind: SubsystemKind::Http,
        err: "listener dead".into(),
    };
}

#[test]
fn lifecycle_state_variants_cover_spec() {
    // All 5 states from spec §2 (excluding Stopped which is pseudo-state).
    let states = [
        LifecycleState::Starting,
        LifecycleState::Running,
        LifecycleState::Degraded,
        LifecycleState::Stopping,
        LifecycleState::Failed,
    ];

    for state in &states {
        // Only Failed is terminal.
        let is_terminal = state.is_terminal();
        assert_eq!(is_terminal, *state == LifecycleState::Failed);
    }
}

#[test]
fn subsystem_kind_mandatory_set() {
    // Per spec: Http, Db, Sync, Engine, WorkerMgr are mandatory.
    // AcpRegistry is optional.
    let mandatory = SubsystemKind::mandatory();
    assert_eq!(mandatory.len(), 5);
    assert!(mandatory.contains(&SubsystemKind::Http));
    assert!(mandatory.contains(&SubsystemKind::Db));
    assert!(mandatory.contains(&SubsystemKind::Sync));
    assert!(mandatory.contains(&SubsystemKind::Engine));
    assert!(mandatory.contains(&SubsystemKind::WorkerMgr));

    // AcpRegistry is NOT mandatory.
    assert!(!SubsystemKind::AcpRegistry.is_mandatory());
}
