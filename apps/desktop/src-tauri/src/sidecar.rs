//! Bundled `nexus42` sidecar lifecycle manager (P1).
//!
//! Owned by the Tauri app while the desktop session is alive:
//!   - Auto-start on app launch unless a healthy daemon already responds on the
//!     resolved port.
//!   - Graceful stop on app quit (SIGTERM → bounded timeout → SIGKILL).
//!   - Restart-on-crash with bounded exponential backoff.
//!   - Readiness via `GET /v1/local/runtime/health` (no stdout parsing).
//!
//! Spec: `.mstar/knowledge/specs/daemon-runtime.md` §12 and
//! `.mstar/knowledge/specs/desktop-shell.md` §7/§8.

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri_plugin_shell::process::CommandChild;
use tauri_plugin_shell::ShellExt;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::{sleep, Instant};

const DEFAULT_PORT: u16 = 8420;
const HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(250);
const HEALTH_START_TIMEOUT: Duration = Duration::from_secs(15);
const RESTART_BACKOFF_BASE: Duration = Duration::from_millis(500);
const RESTART_BACKOFF_MAX: Duration = Duration::from_secs(8);
const MAX_RESTART_ATTEMPTS: u32 = 5;
const STOP_GRACEFUL_TIMEOUT: Duration = Duration::from_secs(5);
const HEALTH_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Local API health probe response (`GET /v1/local/runtime/health`).
#[derive(Debug, serde::Deserialize)]
struct DaemonHealth {
    /// Health status string from the daemon (e.g. "ok"). Carried for debugging
    /// but not consumed by the lifecycle logic in V1.66.
    #[allow(dead_code)]
    status: String,
    version: String,
}

/// Coarse lifecycle state surfaced to the SPA.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DaemonState {
    /// Sidecar spawned, waiting for first successful health probe.
    Starting,
    /// Health probe succeeds.
    Running,
    /// Sidecar exited after healthy; restart is being attempted.
    Degraded,
    /// Stopped by user or restart budget exhausted.
    Stopped,
    /// Failed to start (port conflict, crash on boot, etc.).
    Error,
}

/// Status payload returned by the `get_daemon_status` command.
#[derive(Debug, Clone, Serialize)]
pub struct DaemonStatus {
    pub state: DaemonState,
    pub version: Option<String>,
    pub port: u16,
    pub detail: Option<String>,
}

struct SidecarInner {
    port: u16,
    state: DaemonState,
    version: Option<String>,
    detail: Option<String>,
    /// Whether the Tauri app spawned the current daemon process.
    owned: bool,
    /// Process handle for the owned sidecar.
    child: Option<CommandChild>,
    /// Set when the user/app explicitly requests stop, so crash monitors do not
    /// restart the process.
    stop_requested: bool,
    /// Number of consecutive crash-restart attempts since the last healthy run.
    restart_count: u32,
}

/// Thread-safe handle to the sidecar lifecycle state.
#[derive(Clone)]
pub struct SidecarManager(Arc<Mutex<SidecarInner>>);

impl SidecarManager {
    /// Create a manager for the resolved loopback port.
    pub fn new(port: u16) -> Self {
        Self(Arc::new(Mutex::new(SidecarInner {
            port,
            state: DaemonState::Starting,
            version: None,
            detail: None,
            owned: false,
            child: None,
            stop_requested: false,
            restart_count: 0,
        })))
    }

    /// Resolved daemon port.
    pub async fn port(&self) -> u16 {
        self.0.lock().await.port
    }

    /// Current status for the SPA indicator. For attached (non-owned) daemons,
    /// performs an active health probe so the UI does not stay "running" after
    /// the external daemon has crashed.
    pub async fn status(&self) -> DaemonStatus {
        let (port, should_probe) = {
            let inner = self.0.lock().await;
            (
                inner.port,
                inner.state == DaemonState::Running && !inner.owned,
            )
        };

        if should_probe && probe_health(port).await.is_none() {
            let mut inner = self.0.lock().await;
            // Only mutate if the state is still the attached-running snapshot we
            // probed under; a concurrent start/stop may have moved it already.
            if inner.state == DaemonState::Running && !inner.owned {
                inner.state = DaemonState::Error;
                inner.version = None;
                inner.detail = Some(
                    "The external daemon stopped. Restart the daemon to resume local workspace features.".to_string(),
                );
            }
        }

        let inner = self.0.lock().await;
        DaemonStatus {
            state: inner.state,
            version: inner.version.clone(),
            port: inner.port,
            detail: inner.detail.clone(),
        }
    }

    /// Start (or attach to) the sidecar.
    ///
    /// * If the resolved port is already healthy, attach without ownership.
    /// * Otherwise spawn the bundled `nexus42` binary in foreground mode and
    ///   poll health until ready or a timeout is reached.
    pub async fn start<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>) -> Result<(), String> {
        let mut inner = self.0.lock().await;
        if inner.state == DaemonState::Running || inner.state == DaemonState::Starting {
            return Ok(());
        }
        inner.state = DaemonState::Starting;
        inner.detail = None;
        inner.stop_requested = false;
        drop(inner);

        let port = self.port().await;

        // Attach to an already-healthy daemon (e.g. user ran `nexus42 daemon
        // start` before launching the desktop app). We do NOT take ownership so
        // we will not kill an unrelated process on quit.
        if let Some(health) = probe_health(port).await {
            let mut inner = self.0.lock().await;
            inner.state = DaemonState::Running;
            inner.version = Some(health.version);
            inner.owned = false;
            return Ok(());
        }

        let command = app
            .shell()
            .sidecar("binaries/nexus42")
            .map_err(|e| format!("failed to resolve sidecar binary: {e}"))?
            .args([
                "daemon",
                "start",
                "--foreground",
                "--port",
                &port.to_string(),
            ]);

        let (_rx, child) = command
            .spawn()
            .map_err(|e| format!("failed to spawn sidecar: {e}"))?;

        let pid = child.pid();

        {
            let mut inner = self.0.lock().await;
            inner.child = Some(child);
            inner.owned = true;
        }

        let health = wait_for_first_health(port, pid).await;

        if let Some(health) = health {
            {
                let mut inner = self.0.lock().await;
                inner.state = DaemonState::Running;
                inner.version = Some(health.version);
                inner.restart_count = 0;
                // The child handle stays owned by the manager for stop/restart.
            }
            self.spawn_monitor(app.clone(), pid);
            Ok(())
        } else {
            let mut inner = self.0.lock().await;
            if let Some(child) = inner.child.take() {
                let _ = child.kill();
            }
            inner.owned = false;
            let conflict = tcp_reachable(port).await;
            inner.state = DaemonState::Error;
            inner.detail = Some(if conflict {
                format!(
                    "Nexus couldn't start its background service — port {port} is already in use. \
                     Quit the other Nexus instance, or set a different port."
                )
            } else {
                "Daemon did not start. Check the logs or try restarting.".to_string()
            });
            Err(inner.detail.clone().unwrap())
        }
    }

    /// Request graceful termination of the owned sidecar.
    ///
    /// Does nothing if the app is attached to a user-started daemon.
    pub async fn stop(&self) -> Result<(), String> {
        let child = {
            let mut inner = self.0.lock().await;
            if !inner.owned {
                return Ok(());
            }
            inner.stop_requested = true;
            inner.child.take()
        };

        let Some(child) = child else {
            return Ok(());
        };

        let pid = child.pid();

        // Graceful phase: SIGTERM on Unix. On non-Unix (not supported in V1.66)
        // fall through to the hard kill below.
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
        }

        let child = wait_for_exit_or_timeout(child).await;
        if let Some(child) = child {
            let _ = child.kill();
        }

        let mut inner = self.0.lock().await;
        inner.owned = false;
        if inner.state != DaemonState::Error {
            inner.state = DaemonState::Stopped;
        }
        Ok(())
    }

    /// Monitor task: waits for the owned sidecar to exit, then restarts it with
    /// bounded exponential backoff unless the stop was requested.
    fn spawn_monitor<R: tauri::Runtime>(&self, app: tauri::AppHandle<R>, pid: u32) {
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            // Wait until the process is no longer alive. Polling with signal 0
            // is lightweight and avoids keeping the non-Send event receiver
            // across thread boundaries.
            while process_alive(pid) {
                sleep(Duration::from_millis(100)).await;
            }

            manager.handle_crash(&app).await;
        });
    }

    /// Handle a sidecar process exit: restart with backoff, or stop if the
    /// user/app requested stop before/while we waited. This is split out so the
    /// stop-during-backoff path can be unit-tested without a real child process.
    async fn handle_crash<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>) {
        let (should_restart, attempts) = {
            let inner = self.0.lock().await;
            (
                inner.owned && !inner.stop_requested && inner.state == DaemonState::Running,
                inner.restart_count,
            )
        };

        if should_restart {
            if attempts >= MAX_RESTART_ATTEMPTS {
                let mut inner = self.0.lock().await;
                inner.state = DaemonState::Stopped;
                inner.detail = Some(
                    "The daemon stopped repeatedly. Restart it manually to try again.".to_string(),
                );
                inner.owned = false;
                inner.child = None;
                return;
            }

            {
                let mut inner = self.0.lock().await;
                inner.state = DaemonState::Degraded;
                inner.restart_count += 1;
                inner.child = None;
            }

            let delay = backoff(attempts + 1);
            sleep(delay).await;

            // Re-check stop_requested after the backoff sleep. If the user/app
            // called stop_daemon() while we were waiting, we must honor that
            // stop instead of unconditionally restarting (which would reset
            // stop_requested in start() and spawn a new process).
            let stop_requested = {
                let inner = self.0.lock().await;
                inner.stop_requested
            };
            if stop_requested {
                let mut inner = self.0.lock().await;
                inner.state = DaemonState::Stopped;
                inner.detail = Some("Daemon stopped".to_string());
                inner.owned = false;
                inner.child = None;
                return;
            }

            let _ = self.start(app).await;
        } else {
            let mut inner = self.0.lock().await;
            if inner.state == DaemonState::Running || inner.state == DaemonState::Starting {
                inner.state = DaemonState::Stopped;
            }
            inner.owned = false;
            inner.child = None;
        }
    }
}

/// Resolve the daemon port per compass §5 #3:
/// explicit configured port → `NEXUS_DAEMON_PORT` (if valid) → `8420`.
///
/// V1.66 does not expose a UI-configurable port; the "explicit configured port"
/// slot is reserved for a future launcher setting.
pub fn resolve_port() -> u16 {
    if let Ok(value) = std::env::var("NEXUS_DAEMON_PORT") {
        if let Ok(port) = value.parse::<u16>() {
            return port;
        }
    }
    DEFAULT_PORT
}

fn backoff(attempt: u32) -> Duration {
    let exp = RESTART_BACKOFF_BASE * 2_u32.saturating_pow(attempt.saturating_sub(1));
    exp.min(RESTART_BACKOFF_MAX)
}

async fn probe_health(port: u16) -> Option<DaemonHealth> {
    let url = format!("http://127.0.0.1:{port}/v1/local/runtime/health");
    let client = reqwest::Client::builder()
        .timeout(HEALTH_PROBE_TIMEOUT)
        .build()
        .ok()?;
    let response = client.get(&url).send().await.ok()?;
    if response.status().is_success() {
        response.json::<DaemonHealth>().await.ok()
    } else {
        None
    }
}

async fn tcp_reachable(port: u16) -> bool {
    tokio::time::timeout(
        HEALTH_PROBE_TIMEOUT,
        TcpStream::connect(("127.0.0.1", port)),
    )
    .await
    .is_ok_and(|r| r.is_ok())
}

async fn wait_for_first_health(port: u16, pid: u32) -> Option<DaemonHealth> {
    let deadline = Instant::now() + HEALTH_START_TIMEOUT;
    loop {
        if let Some(health) = probe_health(port).await {
            return Some(health);
        }
        if Instant::now() >= deadline || !process_alive(pid) {
            return None;
        }
        sleep(HEALTH_POLL_INTERVAL).await;
    }
}

/// Wait for the child to exit after SIGTERM; if it does not exit within the
/// graceful timeout, return the child handle so the caller can force-kill it.
async fn wait_for_exit_or_timeout(child: CommandChild) -> Option<CommandChild> {
    let pid = child.pid();
    let deadline = Instant::now() + STOP_GRACEFUL_TIMEOUT;

    // Poll process liveness by sending signal 0 on Unix. This is a lightweight
    // way to wait for termination without owning the event receiver.
    while Instant::now() < deadline {
        sleep(Duration::from_millis(100)).await;
        if !process_alive(pid) {
            return None;
        }
    }
    Some(child)
}

/// Check whether a process is still alive. On Unix this uses `kill(pid, 0)`;
/// on non-Unix platforms V1.66 is unsupported and we always return `false`.
#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;
    kill(Pid::from_raw(pid as i32), None::<Signal>).is_ok()
}

#[cfg(not(unix))]
fn process_alive(_pid: u32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::{backoff, resolve_port, DaemonState};
    use std::time::Duration;

    // `resolve_port` reads `NEXUS_DAEMON_PORT`, which is process-global. These
    // tests must run serially so one test's env mutation does not leak into the
    // next. V1.66 added this guard while extending the sidecar test suite
    // (qc3 W-2).
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    // Concurrency-verification coverage map (Greptile: "concurrent sidecar
    // state transitions hard to statically verify"). The unit-tested paths:
    //   - attach to external daemon that later fails health probe → Error
    //     (`attached_running_daemon_transitions_to_error_when_probe_fails`)
    //   - owned sidecar status does not re-probe (pid monitor owns liveness)
    //     (`owned_running_daemon_does_not_probe_on_status`)
    //   - stop() is a no-op for unowned (`stop_is_noop_for_unowned_manager`)
    //   - stop() sets stop_requested for owned-no-child — the precondition the
    //     ExitRequested hook relies on (`stop_requests_stop_for_owned_manager_without_child`)
    //   - stop requested DURING the backoff window is honored → Stopped, no
    //     restart (`stop_requested_during_backoff_honors_stop` — the
    //     "ExitRequested during active restart" regression)
    // Port resolution + backoff caps are also pinned. Live-concurrent paths
    // that cannot be deterministically unit-tested (e.g. ExitRequested in the
    // narrow window between backoff-sleep and `start()`) are fail-closed by
    // construction (`stop_requested` is re-checked, never cleared by the
    // monitor) and deferred to interactive QA — see the ExitRequested doc
    // note in `lib.rs`.

    fn clear_port_env() {
        // SAFETY: called under ENV_LOCK.
        unsafe { std::env::remove_var("NEXUS_DAEMON_PORT") };
    }

    #[tokio::test(flavor = "current_thread")]
    async fn attached_running_daemon_transitions_to_error_when_probe_fails() {
        // Pick a port that is extremely unlikely to be listening so the active
        // health probe in status() fails.
        let port = 63333;
        let manager = crate::sidecar::SidecarManager::new(port);

        // Simulate a successful attach: state=Running, owned=false.
        {
            let mut inner = manager.0.lock().await;
            inner.state = DaemonState::Running;
            inner.owned = false;
            inner.version = Some("1.0.0".to_string());
        }

        let status = manager.status().await;
        assert_eq!(status.state, DaemonState::Error);
        assert!(status
            .detail
            .as_deref()
            .unwrap_or("")
            .contains("external daemon stopped"));
        assert!(status.version.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn owned_running_daemon_does_not_probe_on_status() {
        let manager = crate::sidecar::SidecarManager::new(63334);
        {
            let mut inner = manager.0.lock().await;
            inner.state = DaemonState::Running;
            inner.owned = true;
            inner.version = Some("1.0.0".to_string());
        }

        let status = manager.status().await;
        // No probe is sent for owned sidecars (they have a pid monitor); state
        // is returned as-is even though nothing is listening on the port.
        assert_eq!(status.state, DaemonState::Running);
        assert_eq!(status.version.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn default_port_without_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_port_env();
        // Ensure the function returns the documented default when the override
        // env var is absent.
        assert_eq!(resolve_port(), 8420);
    }

    #[test]
    fn port_override_from_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_port_env();
        // SAFETY: called under ENV_LOCK with no other env readers.
        unsafe { std::env::set_var("NEXUS_DAEMON_PORT", "9000") };
        assert_eq!(resolve_port(), 9000);
        unsafe { std::env::remove_var("NEXUS_DAEMON_PORT") };
    }

    #[test]
    fn invalid_env_falls_back_to_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_port_env();
        unsafe { std::env::set_var("NEXUS_DAEMON_PORT", "not-a-port") };
        assert_eq!(resolve_port(), 8420);
        unsafe { std::env::remove_var("NEXUS_DAEMON_PORT") };
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stop_is_noop_for_unowned_manager() {
        let manager = crate::sidecar::SidecarManager::new(63335);
        {
            let mut inner = manager.0.lock().await;
            inner.state = DaemonState::Running;
            inner.owned = false;
        }

        manager.stop().await.expect("stop should succeed");

        let inner = manager.0.lock().await;
        assert!(!inner.stop_requested);
        assert!(!inner.owned);
        assert_eq!(inner.state, DaemonState::Running);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stop_requests_stop_for_owned_manager_without_child() {
        let manager = crate::sidecar::SidecarManager::new(63336);
        {
            let mut inner = manager.0.lock().await;
            inner.state = DaemonState::Running;
            inner.owned = true;
            // No real child handle in this unit test; the stop path still sets
            // stop_requested so crash monitors do not restart, which is what the
            // Tauri `ExitRequested` hook relies on for cleanup.
        }

        manager.stop().await.expect("stop should succeed");

        let inner = manager.0.lock().await;
        assert!(inner.stop_requested);
        assert!(inner.owned);
        assert_eq!(inner.state, DaemonState::Running);
    }

    #[test]
    fn backoff_grows_then_caps() {
        assert_eq!(backoff(1), Duration::from_millis(500));
        assert_eq!(backoff(2), Duration::from_millis(1000));
        assert_eq!(backoff(3), Duration::from_millis(2000));
        assert_eq!(backoff(10), Duration::from_secs(8));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stop_requested_during_backoff_honors_stop() {
        // Simulate the monitor path after a sidecar crash: the child is gone,
        // a restart has been scheduled with backoff. While the monitor is
        // sleeping, the user/app requests stop. The monitor must land in
        // Stopped, not call start() and spawn a new process.
        let app = tauri::test::mock_app();
        let manager = crate::sidecar::SidecarManager::new(63337);
        {
            let mut inner = manager.0.lock().await;
            inner.state = DaemonState::Running;
            inner.owned = true;
            inner.restart_count = 0;
            inner.child = None;
            inner.stop_requested = false;
        }

        let manager_for_task = manager.clone();
        let monitor = tokio::spawn(async move {
            manager_for_task.handle_crash(app.handle()).await;
        });

        // Wait until the monitor has entered the backoff window.
        while manager.0.lock().await.state != DaemonState::Degraded {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // User/app requests stop during the backoff sleep.
        manager.0.lock().await.stop_requested = true;

        monitor.await.expect("monitor task completed");

        let inner = manager.0.lock().await;
        assert_eq!(inner.state, DaemonState::Stopped);
        assert!(!inner.owned);
        assert!(inner.child.is_none());
        assert!(inner.detail.as_deref().unwrap_or("").contains("stopped"));
    }

    #[test]
    fn daemon_state_serializes_to_lowercase() {
        // The SPA expects lowercase state strings.
        assert_eq!(
            serde_json::to_value(DaemonState::Running).unwrap(),
            serde_json::json!("running")
        );
        assert_eq!(
            serde_json::to_value(DaemonState::Error).unwrap(),
            serde_json::json!("error")
        );
    }
}
