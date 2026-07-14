//! Bundled `nexus42` sidecar lifecycle manager (P1).
//!
//! Owned by the Tauri app while the desktop session is alive:
//!   - Auto-start on app launch unless a healthy daemon already responds on the
//!     resolved port.
//!   - Graceful stop on app quit (SIGTERM → bounded timeout → SIGKILL).
//!   - Restart-on-crash with bounded exponential backoff.
//!   - Readiness via `GET /v1/daemon/runtime/health` (no stdout parsing).
//!
//! Spec: `.mstar/specs/daemon-runtime.md` §12 and
//! `.mstar/specs/desktop-shell.md` §7/§8.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde::Serialize;
use tauri::Emitter;
use tauri::Manager;
use tauri_plugin_shell::process::CommandChild;
use tauri_plugin_shell::ShellExt;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::{sleep, Instant};

const DEFAULT_PORT: u16 = 8420;
const HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(250);
/// Fast polling interval used during the first second of `wait_for_first_health`
/// to catch the daemon ready transition sooner. After `FAST_POLL_WINDOW` the
/// loop falls back to the steady `HEALTH_POLL_INTERVAL`.
const HEALTH_POLL_INTERVAL_FAST: Duration = Duration::from_millis(100);
/// Duration of the fast-poll window at the start of `wait_for_first_health`.
const FAST_POLL_WINDOW: Duration = Duration::from_secs(1);
const HEALTH_START_TIMEOUT: Duration = Duration::from_secs(15);
const RESTART_BACKOFF_BASE: Duration = Duration::from_millis(500);
const RESTART_BACKOFF_MAX: Duration = Duration::from_secs(8);
const MAX_RESTART_ATTEMPTS: u32 = 5;
const STOP_GRACEFUL_TIMEOUT: Duration = Duration::from_secs(5);
const HEALTH_PROBE_TIMEOUT: Duration = Duration::from_secs(2);
/// Tight gate for the optimistic `probe_port_state` TCP connect. Loopback
/// connects normally resolve in <10 ms; the gate prevents a hung port from
/// blocking the cold-start path.
const PORT_PROBE_TIMEOUT: Duration = Duration::from_millis(150);
/// Maximum bytes of daemon stderr to retain for diagnostic surfacing.
/// Capped to avoid unbounded buffering; truncated at the nearest newline
/// boundary at or below this size (keep the tail, drop the head).
const STDERR_TAIL_MAX_BYTES: usize = 2 * 1024;
/// Tauri event emitted whenever the daemon lifecycle state changes. The SPA
/// subscribes via `window.__TAURI__.event.listen` instead of polling.
const DAEMON_STATUS_EVENT: &str = "nexus://daemon-status-changed";
/// Shared health-probe client so `probe_health` does not allocate a new
/// `reqwest::Client` on every call (QC3-S2).
static HEALTH_CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();

/// Daemon API health probe response (`GET /v1/daemon/runtime/health`).
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

/// Result of the fast TCP port gate used before the heavier HTTP health probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PortState {
    /// Nothing is listening on the loopback port; safe to spawn optimistically.
    Free,
    /// A TCP listener accepted the connect; an HTTP health probe is needed to
    /// decide whether to attach or report a port conflict.
    Occupied,
    /// The gate timed out or produced an inconclusive connect error; fall back
    /// to the HTTP health probe.
    Unknown,
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
    /// App handle used to emit lifecycle state events to the SPA. `None` in
    /// unit tests that do not construct a Tauri app.
    app_handle: Option<tauri::AppHandle<tauri::Wry>>,
    /// Bounded tail of the daemon's stderr output, captured for
    /// diagnostic surfacing on Error transitions. Capped at
    /// STDERR_TAIL_MAX_BYTES (nearest newline boundary). Cleared on
    /// each new spawn.
    stderr_tail: Option<String>,
}

/// Thread-safe handle to the sidecar lifecycle state.
#[derive(Clone)]
pub struct SidecarManager(Arc<Mutex<SidecarInner>>);

impl SidecarManager {
    /// Create a manager for the resolved loopback port.
    pub fn new(port: u16) -> Self {
        Self(Arc::new(Mutex::new(SidecarInner {
            port,
            state: DaemonState::Stopped,
            version: None,
            detail: None,
            owned: false,
            child: None,
            stop_requested: false,
            restart_count: 0,
            app_handle: None,
            stderr_tail: None,
        })))
    }

    /// Resolved daemon port.
    pub async fn port(&self) -> u16 {
        self.0.lock().await.port
    }

    /// Store the app handle so the manager can emit lifecycle events to the SPA.
    /// Synchronous so it can be called from the Tauri `setup` hook before any
    /// async tasks run.
    ///
    /// # Implementation note (R-V167PSEC-QC1-S-BLOCKINGLOCK)
    ///
    /// This uses `blocking_lock()` because `setup` runs on the main thread
    /// before the Tauri async runtime has spawned any tasks that could hold
    /// the manager's async `Mutex`. Calling this after async tasks are running
    /// would risk a deadlock; the only valid call site is the `setup` closure
    /// (see `lib.rs`). The value is stored as `Option<AppHandle>` so a missing
    /// handle is handled gracefully downstream rather than panicking.
    pub fn set_app_handle(&self, app_handle: tauri::AppHandle<tauri::Wry>) {
        self.0.blocking_lock().app_handle = Some(app_handle);
    }

    /// Emit the current daemon status to all SPA subscribers. Silently ignores
    /// emit failures (e.g. no webview listeners) so state-machine logic stays
    /// decoupled from UI delivery.
    async fn notify(&self) {
        let app_handle = { self.0.lock().await.app_handle.clone() };
        if let Some(app_handle) = app_handle {
            let status = self.status().await;
            let _ = app_handle.emit(DAEMON_STATUS_EVENT, &status);
        }
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

    /// Start (or attach to) the sidecar from a manual user/command request.
    ///
    /// Resets the crash-restart budget so a previously exhausted manager can be
    /// recovered by the user (QC3-S1 / qc3 W-2).
    pub async fn start_daemon<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
    ) -> Result<(), String> {
        self.start_with_budget(app, true).await
    }

    /// Start (or attach to) the sidecar for automatic starts.
    ///
    /// Used for app-launch auto-start and monitor-driven crash restarts. The
    /// crash-restart budget is **not** reset, so repeated crashes still exhaust
    /// `MAX_RESTART_ATTEMPTS` (qc3 W-2).
    pub async fn start<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>) -> Result<(), String> {
        self.start_with_budget(app, false).await
    }

    /// Shared implementation of `start` / `start_daemon`.
    ///
    /// * If the resolved port is already healthy, attach without ownership.
    /// * Otherwise spawn the bundled `nexus42` binary in foreground mode and
    ///   poll health until ready or a timeout is reached.
    async fn start_with_budget<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        reset_budget: bool,
    ) -> Result<(), String> {
        let mut inner = self.0.lock().await;
        if inner.state == DaemonState::Running
            || (inner.state == DaemonState::Starting && inner.child.is_some())
        {
            return Ok(());
        }
        inner.state = DaemonState::Starting;
        inner.detail = None;
        inner.stderr_tail = None;
        inner.stop_requested = false;
        if reset_budget {
            inner.restart_count = 0;
        }
        drop(inner);

        let port = self.port().await;

        // Three-valued port gate. If the OS can tell us conclusively that the
        // port is free, spawn the bundled sidecar immediately without paying
        // the HTTP health-probe round-trip. If the port is occupied (or the
        // gate is inconclusive), run the HTTP probe so the external-daemon
        // attach path is preserved and we never spawn onto an occupied port.
        let port_probe_start = Instant::now();
        let port_state = probe_port_state(port).await;
        let port_probe_elapsed = port_probe_start.elapsed();
        let port_state_str = match port_state {
            PortState::Free => "free",
            PortState::Occupied => "occupied",
            PortState::Unknown => "unknown",
        };
        tracing::info!(
            phase = "port_probe",
            port = port,
            port_state = port_state_str,
            elapsed_ms = port_probe_elapsed.as_millis() as u64,
            "port probe completed"
        );

        if port_state != PortState::Free {
            let attach_probe_start = Instant::now();
            let health = probe_health(port).await;
            let attach_probe_elapsed = attach_probe_start.elapsed();
            tracing::info!(
                phase = "attach_probe",
                port = port,
                attached = health.is_some(),
                elapsed_ms = attach_probe_elapsed.as_millis() as u64,
                "attach probe completed"
            );

            if let Some(health) = health {
                // If the listening daemon binary is older than the bundled
                // sidecar, replace it so Setup/agent scan pick up current
                // detection logic instead of silently attaching to a stale
                // `nexus42` left on port 8420 from a previous build.
                if let Some(stale_pid) = stale_external_daemon_pid(app, port) {
                    tracing::warn!(
                        phase = "stale_daemon_replace",
                        port = port,
                        pid = stale_pid,
                        version = %health.version,
                        "external daemon binary is older than bundled sidecar; replacing"
                    );
                    stop_external_daemon(port, stale_pid).await;
                    // Fall through to spawn the bundled sidecar.
                } else {
                    let mut inner = self.0.lock().await;
                    inner.state = DaemonState::Running;
                    inner.version = Some(health.version);
                    inner.owned = false;
                    return Ok(());
                }
            } else if port_state == PortState::Occupied {
                // The port accepted a TCP connect but does not look like our
                // daemon. Treat it as a conflict rather than spawning a second
                // process that will lose the port race.
                let message = format!(
                    "Nexus couldn't start its background service — port {port} is already in use. \
                     Quit the other Nexus instance, or set a different port."
                );
                let mut inner = self.0.lock().await;
                inner.state = DaemonState::Error;
                inner.detail = Some(message.clone());
                drop(inner);
                self.notify().await;
                return Err(message);
            }
        }

        let command = app
            .shell()
            .sidecar("nexus42")
            .map_err(|e| format!("failed to resolve sidecar binary: {e}"))?
            .args([
                "daemon",
                "start",
                "--foreground",
                "--port",
                &port.to_string(),
            ]);

        let (rx, child) = command
            .spawn()
            .map_err(|e| format!("failed to spawn sidecar: {e}"))?;

        let pid = child.pid();
        tracing::info!(
            phase = "spawn",
            port = port,
            pid = pid,
            "sidecar spawned"
        );

        let stderr_tail = Arc::new(Mutex::new(String::new()));
        let stderr_tail_for_drain = stderr_tail.clone();
        let inner_for_drain = self.0.clone();
        tauri::async_runtime::spawn(async move {
            drain_stderr(rx, stderr_tail_for_drain.clone()).await;
            let tail = {
                let lock = stderr_tail_for_drain.lock().await;
                lock.clone()
            };
            let mut inner = inner_for_drain.lock().await;
            inner.stderr_tail = Some(tail);
        });

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
                // A healthy run clears the crash budget so later crashes start
                // counting from zero.
                inner.restart_count = 0;
                // The child handle stays owned by the manager for stop/restart.
            }
            self.notify().await;
            self.spawn_monitor(app.clone(), pid);
            Ok(())
        } else {
            // Take the child handle out and kill it, then release the inner lock
            // so the stderr drain task can finish without a lock-order deadlock.
            let child = {
                let mut inner = self.0.lock().await;
                inner.owned = false;
                inner.child.take()
            };
            if let Some(child) = child {
                let _ = child.kill();
            }

            // Build the diagnostic message outside the inner lock.
            let stderr_snapshot = {
                let stderr = stderr_tail.lock().await;
                if stderr.is_empty() {
                    None
                } else {
                    Some(stderr.trim().to_string())
                }
            };
            let conflict = tcp_reachable(port).await;
            let message = if conflict {
                format!(
                    "Nexus couldn't start its background service — port {port} is already in use. \
                     Quit the other Nexus instance, or set a different port."
                )
            } else {
                "Daemon did not start. Check the logs or try restarting.".to_string()
            };
            let message = format_error_detail(message.as_str(), stderr_snapshot.as_deref());

            let mut inner = self.0.lock().await;
            inner.state = DaemonState::Error;
            inner.detail = Some(message.clone());
            drop(inner);
            self.notify().await;
            Err(message)
        }
    }

    /// Whether this manager spawned the current daemon process.
    pub async fn is_owned(&self) -> bool {
        self.0.lock().await.owned
    }

    /// Whether a daemon is currently considered running (owned or attached).
    pub async fn is_running(&self) -> bool {
        self.0.lock().await.state == DaemonState::Running
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
        drop(inner);
        self.notify().await;
        Ok(())
    }

    /// Detach from an owned sidecar without terminating it.
    ///
    /// Used when the user quits the desktop shell but chooses to keep the
    /// daemon running (dev workflows that leave CLI/sidecar up).
    pub async fn release_without_stop(&self) {
        let mut inner = self.0.lock().await;
        inner.stop_requested = true;
        inner.owned = false;
        // Drop the child handle without killing — the process keeps running.
        let _ = inner.child.take();
        if inner.state != DaemonState::Error {
            inner.state = DaemonState::Stopped;
        }
        drop(inner);
        self.notify().await;
    }

    /// Stop a daemon that is listening on the managed port, whether or not this
    /// manager owns the process handle (attached external daemon).
    pub async fn stop_listening_daemon(&self) -> Result<(), String> {
        if self.is_owned().await {
            return self.stop().await;
        }
        let port = self.port().await;
        if let Some(pid) = listener_pid(port) {
            stop_external_daemon(port, pid).await;
        }
        let mut inner = self.0.lock().await;
        inner.owned = false;
        if inner.state != DaemonState::Error {
            inner.state = DaemonState::Stopped;
        }
        inner.version = None;
        drop(inner);
        self.notify().await;
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
                drop(inner);
                self.notify().await;
                return;
            }

            {
                let mut inner = self.0.lock().await;
                inner.state = DaemonState::Degraded;
                inner.restart_count += 1;
                inner.child = None;
            }
            self.notify().await;

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
                drop(inner);
                self.notify().await;
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
            drop(inner);
            self.notify().await;
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
    let base = RESTART_BACKOFF_BASE * 2_u32.saturating_pow(attempt.saturating_sub(1));
    let capped = base.min(RESTART_BACKOFF_MAX);
    // Add ±25% jitter to avoid synchronized restart storms (QC3-S5).
    let jitter_percent = fastrand::u32(75..=125);
    let base_millis = capped.as_millis() as u64;
    let jittered_millis = base_millis * u64::from(jitter_percent) / 100;
    let delay = Duration::from_millis(jittered_millis).min(RESTART_BACKOFF_MAX);
    tracing::info!(
        attempt = attempt,
        base_millis = base_millis,
        jitter_percent = jitter_percent,
        delay_millis = delay.as_millis() as u64,
        "sidecar restart backoff computed"
    );
    delay
}

/// Truncate `buf` to keep at most `STDERR_TAIL_MAX_BYTES` at the tail, cutting
/// at the nearest newline boundary so we don't surface a partial line.
fn trim_stderr_tail(buf: &mut String) {
    if buf.len() > STDERR_TAIL_MAX_BYTES {
        let keep_start = buf.len().saturating_sub(STDERR_TAIL_MAX_BYTES);
        // Snap to a UTF-8 char boundary so buf[keep_start..] doesn't panic on
        // multibyte input (daemon logs may contain Unicode paths, accented letters).
        let safe_start = buf.ceil_char_boundary(keep_start);
        if let Some(nl) = buf[safe_start..].find('\n') {
            buf.replace_range(..safe_start + nl + 1, "");
        } else {
            buf.replace_range(..safe_start, "");
        }
    }
}

/// Drain the command's stderr event stream into a bounded tail buffer.
/// Runs concurrently with the health probe so it never blocks the spawn path.
async fn drain_stderr(
    mut rx: tokio::sync::mpsc::Receiver<tauri_plugin_shell::process::CommandEvent>,
    tail: Arc<Mutex<String>>,
) {
    let mut buf = String::new();
    while let Some(event) = rx.recv().await {
        match event {
            tauri_plugin_shell::process::CommandEvent::Stderr(bytes) => {
                buf.push_str(&String::from_utf8_lossy(&bytes));
                trim_stderr_tail(&mut buf);
            }
            tauri_plugin_shell::process::CommandEvent::Terminated(payload) => {
                tracing::debug!(
                    code = payload.code,
                    signal = payload.signal,
                    "sidecar stderr task saw termination"
                );
                break;
            }
            tauri_plugin_shell::process::CommandEvent::Error(err) => {
                tracing::debug!(error = err, "sidecar command event error");
            }
            _ => {}
        }
    }
    let mut lock = tail.lock().await;
    *lock = buf;
}

/// Combine the generic error message with captured stderr when present.
fn format_error_detail(message: &str, stderr: Option<&str>) -> String {
    if let Some(stderr) = stderr.filter(|s| !s.trim().is_empty()) {
        format!("{message}\n\nDaemon output:\n{}", stderr.trim())
    } else {
        message.to_string()
    }
}

/// Return the listener PID when an external daemon on `port` should be
/// replaced by the bundled sidecar (process started before sidecar mtime).
///
/// Returning the verified PID lets the stop path signal that same process
/// instead of rediscovering the port owner (which may have changed).
fn stale_external_daemon_pid<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    port: u16,
) -> Option<u32> {
    let sidecar = newest_bundled_sidecar_path(app)?;
    let sidecar_mtime = file_mtime(&sidecar)?;
    let pid = listener_pid(port)?;
    let started_at = process_start_time(pid)?;
    tracing::debug!(
        phase = "stale_daemon_compare",
        port = port,
        pid = pid,
        sidecar = %sidecar.display(),
        started_at = ?started_at,
        sidecar_mtime = ?sidecar_mtime,
        "compared external daemon start time to bundled sidecar mtime"
    );
    // 1s skew tolerance for filesystem / process-start clock granularity.
    if started_at + Duration::from_secs(1) < sidecar_mtime {
        Some(pid)
    } else {
        None
    }
}

fn process_start_time(pid: u32) -> Option<SystemTime> {
    // Prefer Linux `etimes` (seconds); fall back to portable `etime`
    // (`[[dd-]hh:]mm:ss`) which macOS supports.
    let elapsed = process_elapsed_secs(pid)?;
    SystemTime::now().checked_sub(Duration::from_secs(elapsed))
}

fn process_elapsed_secs(pid: u32) -> Option<u64> {
    if let Ok(output) = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "etimes="])
        .output()
    {
        if output.status.success() {
            if let Ok(secs) = String::from_utf8_lossy(&output.stdout).trim().parse::<u64>() {
                return Some(secs);
            }
        }
    }
    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "etime="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_ps_etime(String::from_utf8_lossy(&output.stdout).trim())
}

/// Parse `ps -o etime=` output: `SS`, `MM:SS`, `HH:MM:SS`, or `DD-HH:MM:SS`.
fn parse_ps_etime(raw: &str) -> Option<u64> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    let (days, rest) = match s.split_once('-') {
        Some((d, r)) => (d.parse::<u64>().ok()?, r),
        None => (0, s),
    };
    let parts: Vec<&str> = rest.split(':').collect();
    let secs = match parts.as_slice() {
        [ss] => ss.parse::<u64>().ok()?,
        [mm, ss] => mm.parse::<u64>().ok()? * 60 + ss.parse::<u64>().ok()?,
        [hh, mm, ss] => {
            hh.parse::<u64>().ok()? * 3600
                + mm.parse::<u64>().ok()? * 60
                + ss.parse::<u64>().ok()?
        }
        _ => return None,
    };
    Some(days * 86_400 + secs)
}

fn newest_bundled_sidecar_path<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(resource) = app.path().resource_dir() {
        candidates.push(resource.join("nexus42"));
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        candidates.push(resource.join("nexus42-aarch64-apple-darwin"));
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        candidates.push(resource.join("nexus42-x86_64-apple-darwin"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("nexus42"));
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            {
                candidates.push(dir.join("nexus42-aarch64-apple-darwin"));
                candidates.push(dir.join("../binaries/nexus42-aarch64-apple-darwin"));
            }
            #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
            {
                candidates.push(dir.join("nexus42-x86_64-apple-darwin"));
                candidates.push(dir.join("../binaries/nexus42-x86_64-apple-darwin"));
            }
        }
    }

    candidates
        .into_iter()
        .filter(|p| p.is_file())
        .max_by_key(|p| file_mtime(p).unwrap_or(SystemTime::UNIX_EPOCH))
}

fn file_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

fn listener_pid(port: u16) -> Option<u32> {
    let output = std::process::Command::new("lsof")
        .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-t"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .find_map(|line| line.trim().parse::<u32>().ok())
}

/// Stop a previously verified listener PID. Does not re-resolve the port
/// owner — a different process may have bound the port after the stale check.
async fn stop_external_daemon(port: u16, pid: u32) {
    #[cfg(unix)]
    {
        #[allow(clippy::cast_possible_wrap)]
        let nix_pid = nix::unistd::Pid::from_raw(pid as i32);
        let _ = nix::sys::signal::kill(nix_pid, nix::sys::signal::Signal::SIGTERM);
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if nix::sys::signal::kill(nix_pid, None) == Err(nix::errno::Errno::ESRCH) {
                return;
            }
            sleep(Duration::from_millis(100)).await;
        }
        let _ = nix::sys::signal::kill(nix_pid, nix::sys::signal::Signal::SIGKILL);
        let kill_deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < kill_deadline {
            if nix::sys::signal::kill(nix_pid, None) == Err(nix::errno::Errno::ESRCH) {
                return;
            }
            sleep(Duration::from_millis(50)).await;
        }
        let _ = port;
    }
    #[cfg(not(unix))]
    {
        let _ = port;
        let _ = pid;
    }
}

async fn probe_health(port: u16) -> Option<DaemonHealth> {
    let url = format!("http://127.0.0.1:{port}/v1/daemon/runtime/health");
    let client = HEALTH_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(HEALTH_PROBE_TIMEOUT)
            .build()
            .expect("reqwest health client should build")
    });
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

/// Fast three-valued TCP gate for the daemon port.
///
/// * Connect succeeds → `Occupied` (something is listening).
/// * Connect refused → `Free` (the OS conclusively reports nothing listening).
/// * Timeout or any other connect error → `Unknown` (do not guess; fall back
///   to the HTTP health probe).
async fn probe_port_state(port: u16) -> PortState {
    match tokio::time::timeout(PORT_PROBE_TIMEOUT, TcpStream::connect(("127.0.0.1", port))).await {
        Ok(Ok(_)) => PortState::Occupied,
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => PortState::Free,
        Ok(Err(_)) => PortState::Unknown,
        Err(_) => PortState::Unknown,
    }
}

async fn wait_for_first_health(port: u16, pid: u32) -> Option<DaemonHealth> {
    let start = Instant::now();
    let deadline = start + HEALTH_START_TIMEOUT;
    let mut polls = 0u32;
    loop {
        polls += 1;
        if let Some(health) = probe_health(port).await {
            let elapsed = start.elapsed();
            tracing::info!(
                phase = "wait_for_ready",
                port = port,
                ready = true,
                polls = polls,
                elapsed_ms = elapsed.as_millis() as u64,
                "daemon became ready"
            );
            return Some(health);
        }
        if Instant::now() >= deadline || !process_alive(pid) {
            let elapsed = start.elapsed();
            tracing::info!(
                phase = "wait_for_ready",
                port = port,
                ready = false,
                polls = polls,
                elapsed_ms = elapsed.as_millis() as u64,
                "daemon did not become ready"
            );
            return None;
        }
        let elapsed = start.elapsed();
        let interval = if elapsed < FAST_POLL_WINDOW {
            HEALTH_POLL_INTERVAL_FAST
        } else {
            HEALTH_POLL_INTERVAL
        };
        sleep(interval).await;
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
    use super::{
        backoff, drain_stderr, format_error_detail, probe_port_state, resolve_port,
        trim_stderr_tail, DaemonState, PortState, MAX_RESTART_ATTEMPTS, STDERR_TAIL_MAX_BYTES,
    };
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::sync::Mutex;

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

    #[tokio::test(flavor = "current_thread")]
    async fn new_manager_starts_from_stopped_state() {
        let manager = crate::sidecar::SidecarManager::new(63341);
        let status = manager.status().await;
        assert_eq!(status.state, DaemonState::Stopped);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn probe_port_state_free_on_unused_port() {
        // Use a high ephemeral port that is extremely unlikely to be bound.
        let port = 63400;
        assert_eq!(probe_port_state(port).await, PortState::Free);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn probe_port_state_occupied_on_bound_listener() {
        let port = 63401;
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .expect("test listener should bind");
        assert_eq!(probe_port_state(port).await, PortState::Occupied);
        drop(listener);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn probe_port_state_unknown_when_connect_hangs() {
        // Simulate an inconclusive port by binding a listener and immediately
        // dropping it, then probing before the OS has fully reclaimed the
        // endpoint. This is best-effort: on most platforms the probe returns
        // Free, so the assertion only checks it does not return Occupied.
        let port = 63402;
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .expect("test listener should bind");
        drop(listener);
        let state = probe_port_state(port).await;
        assert_ne!(state, PortState::Occupied, "a just-dropped listener should not report occupied");
    }

    // FB-D1 cold-start latency budget (deterministic timing evidence).
    //
    // The three-valued `probe_port_state` gate replaces the unconditional HTTP
    // `probe_health` call on the Free-port cold-start path. The gate timeout is
    // `PORT_PROBE_TIMEOUT` (150 ms), while the HTTP probe timeout is
    // `HEALTH_PROBE_TIMEOUT` (2 s). Loopback connect-refuse normally resolves in
    // <10 ms and connect-success in <1 ms, so the Free decision drops from
    // "up to 2 s HTTP round-trip" to "sub-50 ms TCP gate" in practice. The
    // tests below assert a generous 500 ms upper bound to stay CI-safe.

    #[tokio::test(flavor = "current_thread")]
    async fn probe_port_state_free_is_fast() {
        // Measure the Free-port gate on a high unused port. This proves the
        // cold-start path does not pay the 2 s HTTP health-probe round-trip.
        let port = 63403;
        let start = std::time::Instant::now();
        let state = probe_port_state(port).await;
        let elapsed = start.elapsed();
        println!(
            "probe_port_state(Free) elapsed: {} ms",
            elapsed.as_millis()
        );
        assert_eq!(state, PortState::Free);
        assert!(
            elapsed < Duration::from_millis(500),
            "Free-port gate took {:?}, expected < 500 ms (no 2 s HTTP probe)",
            elapsed
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn probe_port_state_occupied_is_fast() {
        // Measure the Occupied-port gate on a bound-but-not-HTTP listener. The
        // HTTP probe only runs after this gate in the Occupied attach path.
        let port = 63404;
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .expect("test listener should bind");
        let start = std::time::Instant::now();
        let state = probe_port_state(port).await;
        let elapsed = start.elapsed();
        println!(
            "probe_port_state(Occupied) elapsed: {} ms",
            elapsed.as_millis()
        );
        drop(listener);
        assert_eq!(state, PortState::Occupied);
        assert!(
            elapsed < Duration::from_millis(500),
            "Occupied-port gate took {:?}, expected < 500 ms",
            elapsed
        );
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
    async fn release_without_stop_detaches_owned_manager() {
        let manager = crate::sidecar::SidecarManager::new(63337);
        {
            let mut inner = manager.0.lock().await;
            inner.state = DaemonState::Running;
            inner.owned = true;
        }

        manager.release_without_stop().await;

        let inner = manager.0.lock().await;
        assert!(inner.stop_requested);
        assert!(!inner.owned);
        assert_eq!(inner.state, DaemonState::Stopped);
        assert!(inner.child.is_none());
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
    fn backoff_grows_then_caps_with_jitter() {
        // Jitter is ±25% around the exponential base, capped at 8 s.
        let b1 = backoff(1);
        assert!(
            b1 >= Duration::from_millis(375) && b1 <= Duration::from_millis(625),
            "backoff(1) {b1:?} outside ±25% of 500 ms"
        );
        let b2 = backoff(2);
        assert!(
            b2 >= Duration::from_millis(750) && b2 <= Duration::from_millis(1250),
            "backoff(2) {b2:?} outside ±25% of 1 s"
        );
        let b10 = backoff(10);
        assert!(
            b10 >= Duration::from_millis(6000) && b10 <= Duration::from_secs(8),
            "backoff(10) {b10:?} outside ±25% of 8 s cap"
        );
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

    #[tokio::test(flavor = "current_thread")]
    async fn start_daemon_resets_crash_budget() {
        // A manual start must clear a non-zero crash budget. We spin up a tiny
        // loopback health server so the manager attaches without calling
        // `app.shell()` (the mock app does not initialize the shell plugin).
        let app = tauri::test::mock_app();
        let port = 63338;
        let manager = crate::sidecar::SidecarManager::new(port);
        {
            let mut inner = manager.0.lock().await;
            inner.state = DaemonState::Stopped;
            inner.restart_count = 3;
        }

        let server = spawn_health_server(port).await;
        manager
            .start_daemon(app.handle())
            .await
            .expect("attach should succeed");
        let _ = tokio::time::timeout(Duration::from_secs(1), server).await;

        let inner = manager.0.lock().await;
        assert_eq!(inner.restart_count, 0);
        assert_eq!(inner.state, DaemonState::Running);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn new_manager_start_attaches_when_health_ready() {
        // Regression: new() used to initialize as Starting, and start() treated
        // Starting as "already in progress" even when no child had been spawned.
        let app = tauri::test::mock_app();
        let port = 63342;
        let manager = crate::sidecar::SidecarManager::new(port);

        let server = spawn_health_server(port).await;
        manager
            .start(app.handle())
            .await
            .expect("attach should succeed");
        let _ = tokio::time::timeout(Duration::from_secs(1), server).await;

        let inner = manager.0.lock().await;
        assert_eq!(inner.state, DaemonState::Running);
        assert!(!inner.owned);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn starting_without_child_does_not_suppress_attach() {
        // Regression: Starting with no owned child must not short-circuit a
        // real attach/spawn attempt. The state can only suppress work when it
        // already carries an owned child handle.
        let app = tauri::test::mock_app();
        let port = 63343;
        let manager = crate::sidecar::SidecarManager::new(port);
        {
            let mut inner = manager.0.lock().await;
            inner.state = DaemonState::Starting;
            inner.owned = false;
            inner.child = None;
        }

        let server = spawn_health_server(port).await;
        manager
            .start(app.handle())
            .await
            .expect("attach should succeed");
        let _ = tokio::time::timeout(Duration::from_secs(1), server).await;

        let inner = manager.0.lock().await;
        assert_eq!(inner.state, DaemonState::Running);
        assert!(!inner.owned);
        assert!(inner.child.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn error_state_retries_and_attaches_when_health_ready() {
        // Error is a retryable state. A manual restart must clear the budget
        // and attach to a healthy daemon without fabricating ownership.
        let app = tauri::test::mock_app();
        let port = 63344;
        let manager = crate::sidecar::SidecarManager::new(port);
        {
            let mut inner = manager.0.lock().await;
            inner.state = DaemonState::Error;
            inner.restart_count = MAX_RESTART_ATTEMPTS;
            inner.detail = Some("previous failure".to_string());
        }

        let server = spawn_health_server(port).await;
        manager
            .start_daemon(app.handle())
            .await
            .expect("attach should succeed");
        let _ = tokio::time::timeout(Duration::from_secs(1), server).await;

        let inner = manager.0.lock().await;
        assert_eq!(inner.state, DaemonState::Running);
        assert!(!inner.owned);
        assert!(inner.child.is_none());
        assert_eq!(inner.restart_count, 0);
        assert!(inner.detail.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn crash_restart_stops_when_budget_exhausted() {
        // When the crash budget is already exhausted, handle_crash must not try
        // to restart and must land in Stopped (qc3 W-2).
        let app = tauri::test::mock_app();
        let manager = crate::sidecar::SidecarManager::new(63339);
        {
            let mut inner = manager.0.lock().await;
            inner.state = DaemonState::Running;
            inner.owned = true;
            inner.restart_count = MAX_RESTART_ATTEMPTS;
            inner.child = None;
        }

        manager.handle_crash(app.handle()).await;

        let inner = manager.0.lock().await;
        assert_eq!(inner.state, DaemonState::Stopped);
        assert!(!inner.owned);
        assert!(inner.child.is_none());
        assert_eq!(inner.restart_count, MAX_RESTART_ATTEMPTS);
        assert!(inner
            .detail
            .as_deref()
            .unwrap_or("")
            .contains("stopped repeatedly"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stderr_tail_capped_at_2kib() {
        // Feed a stderr stream that exceeds 2 KiB and verify the retained tail
        // is capped at the nearest newline boundary.
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        let tail = Arc::new(Mutex::new(String::new()));
        let tail_for_task = tail.clone();

        let drain = tokio::spawn(async move {
            drain_stderr(rx, tail_for_task).await;
        });

        let line = format!("{}\n", "x".repeat(100));
        for _ in 0..30 {
            tx.send(tauri_plugin_shell::process::CommandEvent::Stderr(
                line.clone().into_bytes(),
            ))
            .await
            .expect("send should succeed");
        }
        drop(tx);
        drain.await.expect("drain task should complete");

        let tail = tail.lock().await;
        assert!(
            tail.len() <= STDERR_TAIL_MAX_BYTES,
            "tail length {} exceeds {STDERR_TAIL_MAX_BYTES}",
            tail.len()
        );
        assert!(
            tail.ends_with('\n'),
            "tail should end at a newline boundary"
        );
    }

    #[test]
    fn stderr_tail_safely_snaps_to_char_boundary() {
        // Build a 2050-byte buffer where the raw 2048-byte cutoff falls inside
        // the middle byte of a 3-byte UTF-8 character. Without boundary
        // snapping, the function would panic on a non-char-boundary slice.
        let multibyte = "中"; // 3-byte UTF-8
        let prefix = "x";
        let suffix = "y".repeat(2046);
        let mut buf = format!("{prefix}{multibyte}{suffix}");
        let keep_start = buf.len().saturating_sub(STDERR_TAIL_MAX_BYTES);
        assert!(
            !buf.is_char_boundary(keep_start),
            "test precondition: keep_start should split a multibyte char"
        );

        trim_stderr_tail(&mut buf);

        assert_eq!(
            buf, suffix,
            "tail should resume from the next char boundary"
        );
        assert!(buf.len() <= STDERR_TAIL_MAX_BYTES);
    }

    #[test]
    fn error_detail_includes_stderr_when_nonempty() {
        let detail = format_error_detail(
            "Daemon did not start. Check the logs or try restarting.",
            Some("migration X failed"),
        );
        assert!(detail.contains("Daemon did not start"));
        assert!(detail.contains("Daemon output:"));
        assert!(detail.contains("migration X failed"));
    }

    #[test]
    fn error_detail_uses_generic_fallback_when_stderr_empty() {
        let detail = format_error_detail(
            "Daemon did not start. Check the logs or try restarting.",
            None,
        );
        assert_eq!(
            detail,
            "Daemon did not start. Check the logs or try restarting."
        );
        assert!(!detail.contains("Daemon output:"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stderr_tail_resets_on_new_spawn() {
        let app = tauri::test::mock_app();
        let port = 63340;
        let manager = crate::sidecar::SidecarManager::new(port);
        {
            let mut inner = manager.0.lock().await;
            inner.state = DaemonState::Stopped;
            inner.stderr_tail = Some("first spawn stderr".to_string());
        }

        let server = spawn_health_server(port).await;
        manager
            .start_daemon(app.handle())
            .await
            .expect("attach should succeed");
        let _ = tokio::time::timeout(Duration::from_secs(1), server).await;

        let inner = manager.0.lock().await;
        assert_eq!(inner.stderr_tail, None);
        assert_eq!(inner.state, DaemonState::Running);
    }

    /// Loopback HTTP server for tests that need a healthy daemon probe.
    ///
    /// The mock Tauri app does not initialize `tauri_plugin_shell`, so any test
    /// that reaches `app.shell()` panics. By serving the health endpoint we let
    /// `start()` / `start_daemon()` take the attach-without-spawn path.
    ///
    /// The server accepts connections in a loop: the new `probe_port_state`
    /// gate performs a TCP connect before `probe_health` sends the HTTP request,
    /// so the fixture must handle both.
    async fn spawn_health_server(port: u16) -> tokio::task::JoinHandle<()> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .expect("health server should bind");
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(conn) => conn,
                    Err(_) => break,
                };
                let mut buf = [0u8; 512];
                // Drain the request so the client sees a complete HTTP exchange.
                let _ = socket.read(&mut buf).await;
                let body = br#"{"status":"ok","version":"1.0.0"}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.write_all(body).await;
            }
        })
    }
}
