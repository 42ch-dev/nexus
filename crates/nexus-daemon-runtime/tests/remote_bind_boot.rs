//! Remote-bind gate boot-path integration test.
//!
//! Exercises the actual `run_daemon()` entry point to prove the remote-bind
//! gate is evaluated before the HTTP listener is created.

#![allow(clippy::unwrap_used)]
// ENV_TEST_LOCK is deliberately held across awaits: it serializes env-var
// tests (NEXUS42_DAEMON_API_KEY / NEXUS_DAEMON_REMOTE_BIND) so concurrent
// boot tests don't observe each other's environment.
#![allow(clippy::await_holding_lock)]

use nexus_daemon_runtime::boot::{run_daemon, DaemonConfig};
use nexus_daemon_runtime::test_utils::create_test_workspace;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
        embedded_mcp: false,
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

/// Reserve an ephemeral TCP port on 127.0.0.1. The listener is dropped before
/// returning, so the port is free for the daemon to bind.
fn reserve_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve port");
    listener.local_addr().expect("local addr").port()
}

/// Perform an HTTPS GET over a pinned self-signed certificate and return the
/// raw HTTP/1.1 response (headers + body).
async fn https_get_pinned(host: &str, port: u16, path: &str, cert_pem: &[u8]) -> String {
    let certs: Vec<rustls::pki_types::CertificateDer<'static>> =
        rustls_pemfile::certs(&mut &cert_pem[..])
            .collect::<Result<Vec<_>, _>>()
            .expect("parse cert pem");
    assert!(
        !certs.is_empty(),
        "cert pem must contain at least one certificate"
    );

    let mut roots = rustls::RootCertStore::empty();
    roots.add(certs[0].clone()).expect("add cert to root store");

    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));

    let server_name =
        rustls::pki_types::ServerName::try_from(host.to_owned()).expect("server name");
    let stream = tokio::net::TcpStream::connect((host, port))
        .await
        .expect("tcp connect");
    let mut tls = connector
        .connect(server_name, stream)
        .await
        .expect("tls handshake");

    let request =
        format!("GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n");
    tls.write_all(request.as_bytes())
        .await
        .expect("write request");

    let mut buf = Vec::new();
    tls.read_to_end(&mut buf).await.expect("read response");
    String::from_utf8_lossy(&buf).to_string()
}

/// Perform a plain HTTP GET and return the raw HTTP/1.1 response.
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

#[tokio::test]
async fn run_daemon_remote_bind_serves_https_with_fingerprint_endpoint() {
    let _guard = ENV_TEST_LOCK.lock().expect("env test lock poisoned");

    let (_tmp, nexus_home, _db_path) = create_test_workspace().await;
    let (user_home, original_home) = enter_test_home(&nexus_home);

    std::env::set_var("NEXUS42_DAEMON_API_KEY", "test-key");
    std::env::set_var("NEXUS_DAEMON_REMOTE_BIND", "1");

    let port = reserve_port();
    let config = test_config("0.0.0.0", port);
    let handle = tokio::spawn(run_daemon(config));

    // Wait for the daemon to finish startup and bind the TLS listener.
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

    let cert_path = nexus_home_layout::tls_cert_path(&user_home);
    let cert_pem = tokio::fs::read(&cert_path).await.expect("read cert.pem");

    // HTTPS GET to the unguarded fingerprint endpoint should succeed and match
    // the persisted certificate.
    let response = https_get_pinned(
        "127.0.0.1",
        port,
        "/v1/daemon/runtime/cert-fingerprint",
        &cert_pem,
    )
    .await;
    assert!(
        response.contains("HTTP/1.1 200 OK"),
        "fingerprint endpoint should return 200 over TLS: {response}"
    );
    assert!(
        response.contains("\"fingerprint\":\"SHA256:"),
        "fingerprint should be present: {response}"
    );

    // The health endpoint should also be reachable over HTTPS.
    let health = https_get_pinned("127.0.0.1", port, "/v1/daemon/runtime/health", &cert_pem).await;
    assert!(
        health.contains("HTTP/1.1 200 OK"),
        "health endpoint should return 200 over TLS: {health}"
    );

    handle.abort();
    let _ = handle.await;

    restore_home(original_home);
}

#[tokio::test]
async fn run_daemon_loopback_serves_http_and_empty_fingerprint() {
    let _guard = ENV_TEST_LOCK.lock().expect("env test lock poisoned");

    let (_tmp, nexus_home, _db_path) = create_test_workspace().await;
    let (_user_home, original_home) = enter_test_home(&nexus_home);

    // No remote-bind env vars are required for loopback binds.
    let port = reserve_port();
    let config = test_config("127.0.0.1", port);
    let handle = tokio::spawn(run_daemon(config));

    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

    let response = http_get("127.0.0.1", port, "/v1/daemon/runtime/cert-fingerprint").await;
    assert!(
        response.contains("HTTP/1.1 200 OK"),
        "fingerprint endpoint should return 200 over plain HTTP: {response}"
    );
    assert!(
        response.contains("\"fingerprint\":\"\""),
        "loopback-only fingerprint should be empty: {response}"
    );
    assert!(
        response.contains("\"algorithm\":\"sha256\""),
        "loopback-only algorithm should be sha256 per contract: {response}"
    );

    handle.abort();
    let _ = handle.await;

    restore_home(original_home);
}
