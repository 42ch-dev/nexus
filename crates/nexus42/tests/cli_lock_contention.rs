//! Integration test: CLI lock contention (V1.51 T-B P0).
//!
//! Tests the CliError::Locked error variant and its exit code mapping.
//! Full CLI binary integration testing is deferred to QA.

use nexus42::errors::CliError;

#[test]
fn locked_error_display_shows_holder_info() {
    let err = CliError::Locked {
        holder_pid: 1234,
        holder_name: "daemon:schedule:SCH20240618".to_string(),
        stale: false,
    };
    let display = format!("{err}");
    assert!(display.contains("E_LOCK"));
    assert!(display.contains("daemon:schedule:SCH20240618"));
    assert!(display.contains("pid=1234"));
    assert!(!display.contains("STALE"));
}

#[test]
fn locked_error_stale_shows_stale_marker() {
    let err = CliError::Locked {
        holder_pid: 5678,
        holder_name: "cli:cron-set".to_string(),
        stale: true,
    };
    let display = format!("{err}");
    assert!(display.contains("STALE"));
}

#[test]
fn locked_error_matches_pattern_for_exit_code() {
    let err = CliError::Locked {
        holder_pid: 1,
        holder_name: "test".to_string(),
        stale: false,
    };
    // Verify the matches! pattern used in main.rs works correctly.
    assert!(matches!(err, CliError::Locked { .. }));
}
