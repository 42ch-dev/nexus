//! Remote-bind gate boot-path integration test.
//!
//! Exercises the actual `run_daemon()` entry point to prove the remote-bind
//! gate is evaluated before the HTTP listener is created.

#![allow(clippy::unwrap_used)]

use nexus_daemon_runtime::boot::{run_daemon, DaemonConfig};
use nexus_daemon_runtime::test_utils::create_test_workspace;
use std::path::PathBuf;

/// Lock to serialize env-var tests that read `NEXUS42_DAEMON_API_KEY`
/// and `NEXUS_DAEMON_REMOTE_BIND`.
static ENV_TEST_LOCK: std::sync::LazyLock<std::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(()));

fn test_config(host: &str, port: u16) -> DaemonConfig {
    DaemonConfig {
        port,
        host: host.to_string(),
        socket_path: None,
        verbose: false,
        shutdown_grace_ms: 1000,
        cdn_url: None,
    }
}

/// Set `HOME` to the temporary user home and clear the remote-bind env vars.
/// Returns the original `HOME` value (if any) so callers can restore it.
fn enter_test_home(nexus_home: &std::path::Path) -> (PathBuf, Option<String>) {
    let user_home = nexus_home.parent().expect("nexus_home must have a parent");
    let original_home = std::env::var("HOME").ok();
    std::env::set_var("HOME", user_home);
    std::env::remove_var("NEXUS42_DAEMON_API_KEY");
    std::env::remove_var("NEXUS_DAEMON_REMOTE_BIND");
    (user_home.to_path_buf(), original_home)
}

fn restore_home(original_home: Option<String>) {
    match original_home {
        Some(v) => std::env::set_var("HOME", v),
        None => std::env::remove_var("HOME"),
    }
}

#[tokio::test]
async fn run_daemon_rejects_remote_bind_without_env_vars() {
    let _guard = ENV_TEST_LOCK.lock().expect("env test lock poisoned");

    let (_tmp, nexus_home, _db_path) = create_test_workspace().await;
    let (_user_home, original_home) = enter_test_home(&nexus_home);

    let config = test_config("0.0.0.0", 0);
    let result = run_daemon(config).await;

    restore_home(original_home);

    let err = result.expect_err("run_daemon should fail for remote bind without env vars");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("Refusing to bind") || msg.contains("remote bind requires"),
        "error should be remote-bind gate: {msg}"
    );
}

#[tokio::test]
async fn run_daemon_allows_remote_bind_with_env_vars() {
    let _guard = ENV_TEST_LOCK.lock().expect("env test lock poisoned");

    let (_tmp, nexus_home, _db_path) = create_test_workspace().await;
    let (_user_home, original_home) = enter_test_home(&nexus_home);

    std::env::set_var("NEXUS42_DAEMON_API_KEY", "test-key");
    std::env::set_var("NEXUS_DAEMON_REMOTE_BIND", "1");

    let config = test_config("0.0.0.0", 0);
    let handle = tokio::spawn(run_daemon(config));

    // Wait long enough for workspace init, engine wiring, the gate check,
    // and the TCP listener to bind on an OS-assigned port.
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;

    assert!(
        !handle.is_finished(),
        "run_daemon should still be running after remote-bind gate passes"
    );

    handle.abort();
    let _ = handle.await;

    restore_home(original_home);
}
