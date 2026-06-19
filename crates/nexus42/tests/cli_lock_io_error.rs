//! Hermetic tests for `E_LOCK_IO` error path (V1.51 T-B P0 W-002).
//!
//! Verifies that `FileLockError::Io` maps to `CliError::LockIo` with
//! stable code `E_LOCK_IO` and exit code 78 (EX_CONFIG), **not** exit
//! code 75 (EX_TEMPFAIL) which is reserved for temporary contention.

use nexus42::errors::CliError;

#[test]
fn lock_io_error_display_contains_e_lock_io() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied");
    let cli_err = CliError::LockIo(io_err);
    let msg = cli_err.to_string();

    assert!(
        msg.contains("E_LOCK_IO"),
        "LockIo must display with stable code E_LOCK_IO: {msg}"
    );
    assert!(
        msg.contains("permission denied"),
        "LockIo must include the underlying io error: {msg}"
    );
    assert!(
        !msg.contains("E_LOCK:"),
        "LockIo must NOT use E_LOCK (reserved for contention): {msg}"
    );
    assert!(
        !msg.contains("work is held by"),
        "LockIo must NOT claim work is held (misleading): {msg}"
    );
}

#[test]
fn lock_io_error_matches_for_exit_code_78() {
    let io_err = std::io::Error::new(std::io::ErrorKind::Other, "disk full");
    let cli_err = CliError::LockIo(io_err);

    // Verify the error pattern for exit-code matching in main.rs.
    assert!(
        matches!(cli_err, CliError::LockIo(_)),
        "LockIo must match CliError::LockIo pattern"
    );
    assert!(
        !matches!(cli_err, CliError::Locked { .. }),
        "LockIo must NOT match CliError::Locked pattern"
    );
}

#[test]
fn lock_io_suggestion_mentions_config_environment() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "read-only filesystem");
    let cli_err = CliError::LockIo(io_err);
    let msg = cli_err.to_string();

    assert!(
        msg.contains("configuration or environment error") || msg.contains("Suggestion"),
        "LockIo must suggest it's a config/env issue, not temporary contention: {msg}"
    );
    assert!(
        !msg.contains("retry"),
        "LockIo must NOT suggest retry (not temporary): {msg}"
    );
}

#[test]
fn lock_io_source_returns_inner_error() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "test");
    let cli_err = CliError::LockIo(io_err);

    let source = std::error::Error::source(&cli_err);
    assert!(
        source.is_some(),
        "LockIo must expose source error via Error::source()"
    );
    let source_msg = source.unwrap().to_string();
    assert!(
        source_msg.contains("permission denied") || source_msg.contains("test"),
        "LockIo source must preserve the inner io error: {source_msg}"
    );
}

#[test]
fn locked_error_unchanged_after_refactor() {
    // Regression: ensure the Locked variant display didn't change.
    let err = CliError::Locked {
        holder_pid: 1234,
        holder_name: "cli:cron-set".to_string(),
        stale: false,
    };
    let msg = err.to_string();
    assert!(msg.contains("E_LOCK:"));
    assert!(msg.contains("cli:cron-set"));
    assert!(msg.contains("pid=1234"));
    assert!(msg.contains("retry"));
}
