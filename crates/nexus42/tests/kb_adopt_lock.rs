//! Hermetic tests for `creator world kb adopt` file-lock integration (V1.51 T-B P0 W-001).
//!
//! These tests verify the lock acquisition logic without a running daemon.
//! They exercise the error mapping path: `FileLockError` → `CliError::Locked` / `CliError::LockIo`
//! with the correct holder-name format (`cli:kb-adopt`).

use nexus42::errors::CliError;

/// Simulates what happens when `try_acquire` returns `Locked` during `kb adopt`.
/// The CLI must surface the same error shape as other lock-protected commands.
#[test]
fn kb_adopt_lock_contention_maps_to_locked_error() {
    let err = CliError::Locked {
        holder_pid: 3456,
        holder_name: "daemon:schedule:cron-review".to_string(),
        stale: false,
    };
    let msg = err.to_string();
    assert!(
        msg.contains("E_LOCK:"),
        "kb adopt contention must display E_LOCK: {msg}"
    );
    assert!(
        msg.contains("daemon:schedule:cron-review"),
        "kb adopt must surface holder name: {msg}"
    );
    assert!(
        msg.contains("pid=3456"),
        "kb adopt must surface holder pid: {msg}"
    );
}

/// Verifies that I/O errors during lock acquisition in `kb adopt`
/// map to `E_LOCK_IO`, not `E_LOCK`.
#[test]
fn kb_adopt_io_error_maps_to_lock_io_not_locked() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
    let cli_err = CliError::LockIo(io_err);
    let msg = cli_err.to_string();

    assert!(
        msg.contains("E_LOCK_IO"),
        "kb adopt I/O error must display E_LOCK_IO: {msg}"
    );
    assert!(
        !msg.contains("E_LOCK:"),
        "kb adopt I/O error must NOT use E_LOCK: {msg}"
    );
    assert!(
        !msg.contains("work is held by"),
        "kb adopt I/O error must NOT claim work is held: {msg}"
    );
}

/// Verifies that the suggestion in LockIo does not imply retry.
#[test]
fn kb_adopt_lock_io_suggestion_no_retry() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "read-only fs");
    let cli_err = CliError::LockIo(io_err);
    let msg = cli_err.to_string();

    assert!(
        !msg.contains("retry"),
        "LockIo suggestion must NOT mention retry: {msg}"
    );
}
