//! V1.127 P1 — daemon boot registers native CLI providers (greploop hardened).
//!
//! Regression guard for R-V1116P0QA-001 + greptile P1/P2 (PR #161):
//! - The daemon probes CLI presence via `which::which()` before calling
//!   `register_provider`. Providers whose CLI is absent from PATH are NOT
//!   registered, so the `/providers` endpoint does not surface them.
//! - Tests use RAII guards (`PathGuard` + `BootTestGuard`) so env-var
//!   mutation and daemon task cleanup are panic-safe — a failed assertion
//!   restores `HOME`/`PATH` and aborts the daemon task automatically.

#![allow(clippy::unwrap_used)]

use nexus_daemon_runtime::boot::{run_daemon, DaemonConfig};
use std::sync::LazyLock;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

/// Serializes tests that mutate `HOME` and `PATH` so they do not collide
/// with each other or with `no_profile_boot.rs`-style tests in other
/// binaries. Within this binary, the lock ensures at most one daemon
/// instance is booted at a time against the mutated env.
static ENV_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

// ── RAII guards (greptile P2 — panic-safe env cleanup) ─────────────

/// Replace `PATH` with a single directory on construction; restore the
/// previous value on drop. Mirrors the `PathGuard` pattern in
/// `crates/nexus-agent-host/src/discovery/path_scan.rs` (qc1 W-002 fix).
struct PathGuard {
    previous: Option<String>,
}

impl PathGuard {
    fn replace(dir: &std::path::Path) -> Self {
        let previous = std::env::var("PATH").ok();
        let new_path = std::env::join_paths([dir.to_path_buf()]).expect("valid PATH join");
        std::env::set_var("PATH", new_path);
        Self { previous }
    }
}

impl Drop for PathGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(p) => std::env::set_var("PATH", p),
            None => std::env::remove_var("PATH"),
        }
    }
}

/// Save `HOME`, set it to the test temp dir, and track the spawned daemon
/// task. On drop (even if an assertion panics): restore `HOME` AND abort
/// the daemon task so parallel tests do not collide with a leaked daemon
/// reading a deleted home path (greptile P2).
///
/// Also saves/removes/restores `DSH_RUNTIME_BIN`: boot registers
/// `dsh-native` when that env var is non-empty (PD-4), so tests that
/// assert PATH-only registration must not inherit a developer's env value.
struct BootTestGuard {
    original_home: Option<String>,
    original_dsh_runtime_bin: Option<String>,
    daemon_handle: Option<tokio::task::JoinHandle<anyhow::Result<()>>>,
}

impl BootTestGuard {
    fn new(temp_home: &std::path::Path) -> Self {
        let original_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", temp_home);
        std::env::remove_var("NEXUS42_DAEMON_API_KEY");
        std::env::remove_var("NEXUS_DAEMON_REMOTE_BIND");
        let original_dsh_runtime_bin = std::env::var("DSH_RUNTIME_BIN").ok();
        std::env::remove_var("DSH_RUNTIME_BIN");
        Self {
            original_home,
            original_dsh_runtime_bin,
            daemon_handle: None,
        }
    }

    fn track_daemon(&mut self, handle: tokio::task::JoinHandle<anyhow::Result<()>>) {
        self.daemon_handle = Some(handle);
    }
}

impl Drop for BootTestGuard {
    fn drop(&mut self) {
        // Abort the daemon task first to prevent further work against the
        // stale HOME. `abort` is non-blocking; the `JoinHandle` is dropped
        // without awaiting (Drop is sync).
        if let Some(handle) = self.daemon_handle.take() {
            handle.abort();
        }
        match &self.original_home {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }
        match &self.original_dsh_runtime_bin {
            Some(value) => std::env::set_var("DSH_RUNTIME_BIN", value),
            None => std::env::remove_var("DSH_RUNTIME_BIN"),
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────

fn test_config(port: u16) -> DaemonConfig {
    DaemonConfig {
        port,
        host: "127.0.0.1".to_string(),
        socket_path: None,
        verbose: false,
        shutdown_grace_ms: 1000,
        cdn_url: None,
        embedded_mcp: false,
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

/// Write an executable stub binary into `dir` (greptile P1 — the daemon's
/// `which::which()` probe must find it).
fn write_cli_stub(dir: &std::path::Path, name: &str) {
    let path = dir.join(name);
    std::fs::write(&path, "#!/bin/sh\necho hello\n").expect("write stub");
    set_executable(&path);
}

#[cfg(unix)]
fn set_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(path, perms).expect("chmod +x");
}

#[cfg(not(unix))]
fn set_executable(_path: &std::path::Path) {
    // Windows: `which` resolves via PATHEXT; no executable bit needed.
}

// ── Tests ──────────────────────────────────────────────────────────

/// Positive case: with `codex`, `claude`, and `dsh-jsonrpc-agent` stub
/// binaries on PATH, the daemon registers all three providers and
/// `/providers` returns them.
#[tokio::test]
async fn run_daemon_registers_native_providers_when_clis_on_path() {
    let _lock = ENV_TEST_LOCK.lock().await;

    let tmp = tempfile::TempDir::new().expect("temp dir");
    let nexus_home = tmp.path().join(".nexus42");
    nexus_home_layout::ensure_system_layout(&nexus_home).expect("system layout");

    // Create stub CLIs in a bin subdir.
    let bin_dir = tmp.path().join("bin");
    std::fs::create_dir_all(&bin_dir).expect("create bin dir");
    write_cli_stub(&bin_dir, "codex");
    write_cli_stub(&bin_dir, "claude");
    write_cli_stub(&bin_dir, "dsh-jsonrpc-agent");

    // Isolate PATH so the daemon's `which::which()` finds only the stubs.
    let _path_guard = PathGuard::replace(&bin_dir);

    // Set HOME + track daemon task for panic-safe cleanup (greptile P2).
    // BootTestGuard also removes DSH_RUNTIME_BIN so dsh registration is
    // driven by the PATH stub, not by a developer's env var.
    let mut env_guard = BootTestGuard::new(tmp.path());

    let port = reserve_port();
    let handle = tokio::spawn(run_daemon(test_config(port)));
    env_guard.track_daemon(handle);

    // Boot takes ~1-2s (DB init, subsystem wiring).
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let response = http_get("127.0.0.1", port, "/v1/daemon/agent-host/providers").await;

    assert!(
        response.contains("HTTP/1.1 200 OK"),
        "providers endpoint should return 200 OK: {response}"
    );
    assert!(
        response.contains("\"provider_id\":\"codex-native\""),
        "providers response should include codex-native: {response}"
    );
    assert!(
        response.contains("\"provider_id\":\"claude-native\""),
        "providers response should include claude-native: {response}"
    );
    assert!(
        response.contains("\"provider_id\":\"dsh-native\""),
        "providers response should include dsh-native: {response}"
    );
    // Guards drop here: daemon aborted + HOME/PATH/DSH_RUNTIME_BIN
    // restored, panic-safe.
}

/// Negative case: without the CLIs on PATH, the daemon does NOT register
/// them and `/providers` returns an empty list. Prevents the false-promise
/// UX where the UI offers a session that fails at process-spawn time
/// (greptile P1). Covers `dsh-native` too: absent `dsh-jsonrpc-agent` AND
/// unset `DSH_RUNTIME_BIN` → skipped (PD-4).
#[tokio::test]
async fn run_daemon_skips_native_providers_when_clis_absent() {
    let _lock = ENV_TEST_LOCK.lock().await;

    let tmp = tempfile::TempDir::new().expect("temp dir");
    let nexus_home = tmp.path().join(".nexus42");
    nexus_home_layout::ensure_system_layout(&nexus_home).expect("system layout");

    // Isolate PATH to an empty dir so `which::which("codex")` / `"claude"`
    // / `"dsh-jsonrpc-agent"` do not find real binaries that might be
    // installed on the host. BootTestGuard removes DSH_RUNTIME_BIN so the
    // dsh env route cannot register the provider either.
    let empty_bin = tmp.path().join("empty-bin");
    std::fs::create_dir_all(&empty_bin).expect("create empty-bin dir");
    let _path_guard = PathGuard::replace(&empty_bin);

    let mut env_guard = BootTestGuard::new(tmp.path());

    let port = reserve_port();
    let handle = tokio::spawn(run_daemon(test_config(port)));
    env_guard.track_daemon(handle);

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let response = http_get("127.0.0.1", port, "/v1/daemon/agent-host/providers").await;

    assert!(
        response.contains("HTTP/1.1 200 OK"),
        "providers endpoint should return 200 OK: {response}"
    );
    assert!(
        !response.contains("codex-native"),
        "codex-native should NOT appear when CLI is absent: {response}"
    );
    assert!(
        !response.contains("claude-native"),
        "claude-native should NOT appear when CLI is absent: {response}"
    );
    assert!(
        !response.contains("dsh-native"),
        "dsh-native should NOT appear when CLI is absent and DSH_RUNTIME_BIN unset: {response}"
    );
}

/// dsh env-route variant (PD-4): with `dsh-jsonrpc-agent` NOT on PATH but
/// `DSH_RUNTIME_BIN` set, the daemon registers `dsh-native` (`runtime_bin`
/// left unset — the SDK resolves the env var itself) while codex/claude
/// stay unregistered.
#[tokio::test]
async fn run_daemon_registers_dsh_native_when_dsh_runtime_bin_set() {
    let _lock = ENV_TEST_LOCK.lock().await;

    let tmp = tempfile::TempDir::new().expect("temp dir");
    let nexus_home = tmp.path().join(".nexus42");
    nexus_home_layout::ensure_system_layout(&nexus_home).expect("system layout");

    // Stub runtime the env var points at (registration only requires a
    // non-empty value; the file keeps the scenario realistic).
    let bin_dir = tmp.path().join("bin");
    std::fs::create_dir_all(&bin_dir).expect("create bin dir");
    write_cli_stub(&bin_dir, "dsh-jsonrpc-agent");

    // Isolate PATH to an EMPTY dir: codex/claude/dsh-jsonrpc-agent must
    // not resolve; the dsh env route is the only registration source.
    let empty_bin = tmp.path().join("empty-bin");
    std::fs::create_dir_all(&empty_bin).expect("create empty-bin dir");
    let _path_guard = PathGuard::replace(&empty_bin);

    // Remove any developer DSH_RUNTIME_BIN first, then set the test value
    // so the assertion is deterministic; the guard restores the original
    // on drop.
    let mut env_guard = BootTestGuard::new(tmp.path());
    std::env::set_var("DSH_RUNTIME_BIN", bin_dir.join("dsh-jsonrpc-agent"));

    let port = reserve_port();
    let handle = tokio::spawn(run_daemon(test_config(port)));
    env_guard.track_daemon(handle);

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let response = http_get("127.0.0.1", port, "/v1/daemon/agent-host/providers").await;

    assert!(
        response.contains("HTTP/1.1 200 OK"),
        "providers endpoint should return 200 OK: {response}"
    );
    assert!(
        response.contains("\"provider_id\":\"dsh-native\""),
        "providers response should include dsh-native via DSH_RUNTIME_BIN: {response}"
    );
    assert!(
        !response.contains("codex-native"),
        "codex-native should NOT appear when CLI is absent: {response}"
    );
    assert!(
        !response.contains("claude-native"),
        "claude-native should NOT appear when CLI is absent: {response}"
    );
}
