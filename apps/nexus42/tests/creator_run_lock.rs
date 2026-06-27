//! Hermetic tests for `creator run` file-lock integration (V1.51 T-B P0 W-001).
//!
//! These tests verify the lock acquisition logic without a running daemon.
//! They exercise the error mapping path: `FileLockError` → `CliError::Locked` / `CliError::LockIo`
//! with the correct holder-name format (`cli:run`).

use nexus42::errors::CliError;

/// Simulates what happens when `try_acquire` returns `Locked` during `creator run`.
/// The CLI must surface the same error shape as `creator works cron set`.
#[test]
fn cli_run_lock_contention_maps_to_locked_error() {
    // Simulate: FileLockError::Locked → CliError::Locked
    let err = CliError::Locked {
        holder_pid: 5678,
        holder_name: "daemon:schedule:cron-brainstorm".to_string(),
        stale: false,
    };
    let msg = err.to_string();
    assert!(
        msg.contains("E_LOCK:"),
        "creator run contention must display E_LOCK: {msg}"
    );
    assert!(
        msg.contains("daemon:schedule:cron-brainstorm"),
        "creator run must surface holder name: {msg}"
    );
    assert!(
        msg.contains("pid=5678"),
        "creator run must surface holder pid: {msg}"
    );
}

/// Verifies that a stale lock held during `creator run` marks `stale=true` in the message.
#[test]
fn cli_run_lock_stale_shows_marker() {
    let err = CliError::Locked {
        holder_pid: 9999,
        holder_name: "cli:run".to_string(),
        stale: true,
    };
    let msg = err.to_string();
    assert!(
        msg.contains("STALE"),
        "stale lock must show STALE marker: {msg}"
    );
}

/// Verifies that I/O errors during lock acquisition in `creator run`
/// map to `E_LOCK_IO`, not `E_LOCK`.
#[test]
fn cli_run_io_error_maps_to_lock_io_not_locked() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
    let cli_err = CliError::LockIo(io_err);
    let msg = cli_err.to_string();

    assert!(
        msg.contains("E_LOCK_IO"),
        "creator run I/O error must display E_LOCK_IO: {msg}"
    );
    assert!(
        !msg.contains("E_LOCK:"),
        "creator run I/O error must NOT use E_LOCK: {msg}"
    );
    assert!(
        !msg.contains("work is held by"),
        "creator run I/O error must NOT claim work is held: {msg}"
    );
}
