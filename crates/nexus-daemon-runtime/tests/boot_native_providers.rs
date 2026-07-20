//! V1.127 P1 T1 — daemon boot registers native CLI providers.
//!
//! Regression guard for R-V1116P0QA-001: `boot.rs` previously constructed
//! `HostManager::new()` (empty) and never called `register_provider`, so the
//! agent-host `/providers` endpoint returned an empty list unless PATH scan
//! happened to surface a CLI. After the fix, `CodexNativeProvider` and
//! `ClaudeCliProvider` are registered unconditionally at boot and appear in
//! the catalog regardless of PATH state.
//!
//! This is an integration test (not an inline unit test) on purpose: it
//! exercises the real `run_daemon` boot path, so a regression that removes
//! the `register_provider` calls from `boot.rs` will fail here. A unit test
//! that reconstructs the calls inline would not catch that class of bug.

#![allow(clippy::unwrap_used)]

use nexus_daemon_runtime::boot::{run_daemon, DaemonConfig};
use std::sync::LazyLock;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

/// Serializes tests within this binary that mutate `HOME`. Mirrors the
/// pattern in `no_profile_boot.rs`.
static ENV_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn test_config(port: u16) -> DaemonConfig {
    DaemonConfig {
        port,
        host: "127.0.0.1".to_string(),
        socket_path: None,
        verbose: false,
        shutdown_grace_ms: 1000,
        cdn_url: None,
    }
}

fn reserve_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve port");
    listener.local_addr().expect("local addr").port()
}

async fn http_get(host: &str, port: u16, path: &str) -> String {
    let mut stream = tokio::net::TcpStream::connect((host, port))
        .await
        .expect("tcp connect");
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("write request");

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.expect("read response");
    String::from_utf8_lossy(&buf).to_string()
}

#[tokio::test]
async fn run_daemon_registers_native_agent_providers_at_boot() {
    let _guard = ENV_TEST_LOCK.lock().await;

    let tmp = tempfile::TempDir::new().expect("temp dir");
    let user_home = tmp.path();
    let nexus_home = user_home.join(".nexus42");
    nexus_home_layout::ensure_system_layout(&nexus_home).expect("system layout");

    let original_home = std::env::var("HOME").ok();
    std::env::set_var("HOME", user_home);
    std::env::remove_var("NEXUS42_DAEMON_API_KEY");
    std::env::remove_var("NEXUS_DAEMON_REMOTE_BIND");

    let port = reserve_port();
    let handle = tokio::spawn(run_daemon(test_config(port)));

    // Boot takes ~1-2s (DB init, subsystem wiring). Mirrors `no_profile_boot.rs`.
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let response = http_get("127.0.0.1", port, "/v1/daemon/agent-host/providers").await;

    // Sanity: endpoint is reachable and returns 200.
    assert!(
        response.contains("HTTP/1.1 200 OK"),
        "providers endpoint should return 200 OK: {response}"
    );

    // The fix: both native providers must appear in the catalog regardless of
    // whether the CLIs are installed on PATH in the test environment.
    assert!(
        response.contains("\"provider_id\":\"codex-native\""),
        "providers response should include codex-native: {response}"
    );
    assert!(
        response.contains("\"provider_id\":\"claude-native\""),
        "providers response should include claude-native: {response}"
    );

    handle.abort();
    let _ = handle.await;

    match original_home {
        Some(v) => std::env::set_var("HOME", v),
        None => std::env::remove_var("HOME"),
    }
}
