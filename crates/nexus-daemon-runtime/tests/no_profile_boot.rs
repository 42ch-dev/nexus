//! AC-P0-1: daemon boots and serves Tier-0 routes without `active_creator_id`.

#![allow(clippy::unwrap_used)]

use nexus_daemon_runtime::boot::{run_daemon, DaemonConfig};
use std::sync::LazyLock;
use std::sync::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
async fn run_daemon_boots_without_active_creator_and_serves_health() {
    let _guard = ENV_TEST_LOCK.lock().expect("env test lock");

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

    tokio::time::sleep(std::time::Duration::from_millis(2000)).await;

    let health = http_get("127.0.0.1", port, "/v1/daemon/runtime/health").await;
    assert!(
        health.contains("HTTP/1.1 200 OK"),
        "health should return 200 without active creator: {health}"
    );

    handle.abort();
    let _ = handle.await;

    match original_home {
        Some(v) => std::env::set_var("HOME", v),
        None => std::env::remove_var("HOME"),
    }
}
