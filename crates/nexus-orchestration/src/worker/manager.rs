//! Worker Manager — spawn, supervise, and shut down worker child processes.
//!
//! Architecture:
//! - `WorkerManager` spawns child processes and tracks them.
//! - `WorkerHandle` provides IPC via [`IpcClient`] and shutdown.
//! - A background supervisor task per worker calls `child.wait()` and
//!   emits `WorkerEvent::Crashed` on unexpected exit.
//!
//! The `Child` is moved into the supervisor task. The handle holds the
//! `IpcClient` (wrapping stdin/stdout), the PID, and a
//! `CancellationToken` for in-flight request cancellation.
//!
//! ## WS2 R4: SIGTERM → SIGKILL escalation
//!
//! On shutdown, the supervisor sends SIGTERM first, waits for the grace period
//! from `shutdown_grace`, then escalates to SIGKILL if the worker hasn't exited.
//! This follows the graceful shutdown pattern from §6.5 of the design spec.
//!
//! ## WS-E T1: Persistent IpcClient
//!
//! `WorkerHandle` now holds an [`IpcClient`] internally. The legacy
//! `call_json_rpc` method is preserved for backward compatibility — it
//! delegates to the internal `IpcClient`.
//!
//! Design: `.agents/plans/knowledge/orchestration-engine-v1.md` §6.1, §6.5.

use crate::worker::ipc::IpcClient;
use crate::worker::transport::StdioTransport;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use serde_json::Value;
use std::time::Duration;
use thiserror::Error;
use tokio::process::Command;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors from worker management operations.
#[derive(Error, Debug)]
pub enum WorkerError {
    #[error("failed to spawn worker: {0}")]
    SpawnFailed(std::io::Error),
    #[error("IPC error: {0}")]
    Ipc(String),
    #[error("worker crashed with exit status: {0}")]
    Crashed(String),
    #[error("shutdown timeout — worker did not exit within grace period")]
    ShutdownTimeout,
    #[error("worker not running")]
    NotRunning,
    #[error("internal: {0}")]
    Internal(String),
}

// ---------------------------------------------------------------------------
// WorkerSpec
// ---------------------------------------------------------------------------

/// Describes how to launch a worker process.
#[derive(Debug, Clone)]
pub struct WorkerSpec {
    /// Path to the worker binary or script.
    pub program: String,
    /// Arguments passed to the worker program.
    pub args: Vec<String>,
    /// Environment variables set for the worker process.
    pub env: Vec<(String, String)>,
    /// Current working directory for the worker.
    pub working_dir: Option<String>,
}

impl WorkerSpec {
    /// Create a spec from a program path (no extra args).
    pub fn from_program(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            working_dir: None,
        }
    }

    /// Test helper — creates a spec for a shell script at the given path.
    pub fn test_stub(path: &str) -> Self {
        Self::from_program("bash").with_arg(path)
    }

    /// Add an argument.
    pub fn with_arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Add an environment variable.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }
}

// ---------------------------------------------------------------------------
// WorkerEvent
// ---------------------------------------------------------------------------

/// Events emitted by the worker manager about worker state changes.
#[derive(Debug, Clone)]
pub enum WorkerEvent {
    /// Worker process exited unexpectedly.
    Crashed { pid: u32, exit_status: Option<i32> },
    /// Worker started successfully.
    Started { pid: u32 },
    /// Worker was gracefully shut down.
    Stopped { pid: u32 },
}

// ---------------------------------------------------------------------------
// WorkerHandle
// ---------------------------------------------------------------------------

/// Handle to a running worker process.
///
/// Owns an [`IpcClient`] for multiplexed IPC and a cancellation token.
/// The `Child` itself is owned by a background supervisor task.
///
/// ## Backward compatibility
///
/// The legacy `call_json_rpc` method is preserved — it delegates to the
/// internal `IpcClient::call`. New code should prefer using
/// [`WorkerHandle::ipc_client`] directly for concurrent requests.
pub struct WorkerHandle {
    /// PID of the worker process.
    pid: u32,
    /// Cancellation token — fires on shutdown or crash.
    cancel: CancellationToken,
    /// Broadcast sender for events (used by IpcClient for notification routing in WS3).
    #[allow(dead_code)]
    event_tx: broadcast::Sender<WorkerEvent>,
    /// Grace period for shutdown (default: 30 seconds, configurable via spec).
    shutdown_grace: Duration,
    /// Persistent IpcClient for multiplexed JSON-RPC communication.
    ipc: IpcClient,
    /// Whether a clean shutdown has been requested.
    shutdown_requested: bool,
}

impl WorkerHandle {
    /// Send a JSON-RPC request to the worker and await the response.
    ///
    /// Delegates to the internal [`IpcClient`]. This method can be called
    /// multiple times — the underlying `IpcClient` supports concurrent
    /// in-flight requests.
    ///
    /// For concurrent requests, prefer [`WorkerHandle::ipc_client`] which
    /// returns `&IpcClient` for direct use with multiple outstanding calls.
    pub async fn call_json_rpc(&self, method: &str, params: Value) -> Result<Value, WorkerError> {
        self.ipc
            .call(method, params)
            .await
            .map_err(|e| WorkerError::Ipc(e.to_string()))
    }

    /// Request a graceful shutdown of the worker.
    ///
    /// Sends a `worker/shutdown` JSON-RPC request and fires the cancellation
    /// token. The supervisor task will send SIGTERM first, wait for the grace
    /// period, then SIGKILL if needed (WS2 R4).
    pub async fn shutdown(&mut self) -> Result<(), WorkerError> {
        self.shutdown_requested = true;
        self.cancel.cancel();

        info!(
            pid = self.pid,
            grace_ms = self.shutdown_grace.as_millis(),
            "requesting worker shutdown via cancellation token"
        );

        // Try to send a shutdown RPC via IpcClient.
        if !self.ipc.is_closed() {
            let grace_ms = self.shutdown_grace.as_millis() as u32;
            match self
                .ipc
                .call("worker/shutdown", serde_json::json!({"grace_ms": grace_ms}))
                .await
            {
                Ok(_) => {
                    info!(pid = self.pid, "worker acknowledged shutdown RPC");
                }
                Err(e) => {
                    // Worker may have already exited — this is fine.
                    warn!(pid = self.pid, error = %e, "shutdown RPC failed (worker may have already exited)");
                }
            }
        } else {
            info!(
                pid = self.pid,
                "IPC client already closed — supervisor will send SIGTERM"
            );
        }

        // Close the IpcClient to clean up the background reader.
        self.ipc.close().await;

        Ok(())
    }

    /// Return a reference to the internal [`IpcClient`].
    ///
    /// Use this for concurrent multiplexed requests — multiple callers can
    /// call [`IpcClient::call`] on the returned reference simultaneously.
    pub fn ipc_client(&self) -> &IpcClient {
        &self.ipc
    }

    /// Return the process ID.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Return a reference to the cancellation token.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Return the configured shutdown grace period.
    pub fn shutdown_grace(&self) -> Duration {
        self.shutdown_grace
    }
}

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        if !self.shutdown_requested {
            debug!(
                pid = self.pid,
                "WorkerHandle dropped without explicit shutdown — cancelling"
            );
            self.cancel.cancel();
        }
    }
}

// ---------------------------------------------------------------------------
// WorkerManager
// ---------------------------------------------------------------------------

/// Manages the lifecycle of worker child processes.
///
/// Each spawned worker gets a background supervisor task that monitors its
/// exit status. If the worker exits unexpectedly (non-zero or before a
/// clean shutdown), a `WorkerEvent::Crashed` is emitted.
pub struct WorkerManager {
    /// Broadcast channel for worker lifecycle events.
    event_tx: broadcast::Sender<WorkerEvent>,
}

impl WorkerManager {
    /// Create a new worker manager.
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self { event_tx }
    }

    /// Spawn a new worker process from the given spec.
    ///
    /// Returns a [`WorkerHandle`] for IPC and lifecycle management.
    /// A background supervisor task monitors the child process and emits
    /// events via the manager's broadcast channel.
    pub async fn spawn(&self, spec: &WorkerSpec) -> Result<WorkerHandle, WorkerError> {
        self.spawn_with_grace(spec, Duration::from_secs(30)).await
    }

    /// Spawn a new worker process with a custom shutdown grace period.
    ///
    /// WS2 R4: The grace period controls how long the supervisor waits
    /// after SIGTERM before escalating to SIGKILL.
    pub async fn spawn_with_grace(
        &self,
        spec: &WorkerSpec,
        shutdown_grace: Duration,
    ) -> Result<WorkerHandle, WorkerError> {
        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args)
            .envs(spec.env.iter().cloned())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit());

        if let Some(dir) = &spec.working_dir {
            cmd.current_dir(dir);
        }

        let mut child = cmd.spawn().map_err(WorkerError::SpawnFailed)?;
        let pid = child.id().unwrap_or(0);

        info!(pid, program = %spec.program, "spawned worker");

        // Take stdin/stdout BEFORE moving the child into the supervisor.
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| WorkerError::Internal("failed to capture child stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| WorkerError::Internal("failed to capture child stdout".to_string()))?;

        // Create persistent IpcClient from the transport.
        let transport = StdioTransport::new(stdin, stdout);
        let ipc = IpcClient::new(Box::new(transport));

        let cancel = CancellationToken::new();
        let supervisor_cancel = cancel.clone();
        let event_tx = self.event_tx.clone();

        // WS2 R4: Pass grace period to supervisor for SIGTERM→SIGKILL sequence.
        let grace = shutdown_grace;

        // Spawn the supervisor task that waits for child exit.
        tokio::spawn(async move {
            tokio::select! {
                _ = supervisor_cancel.cancelled() => {
                    // WS2 R4: SIGTERM → SIGKILL escalation.
                    // 1. Send SIGTERM first.
                    let nix_pid = Pid::from_raw(pid as i32);
                    if let Err(e) = kill(nix_pid, Signal::SIGTERM) {
                        warn!(pid, error = %e, "failed to send SIGTERM to worker");
                        // If SIGTERM fails, fall back to SIGKILL immediately.
                        let _ = child.start_kill();
                    } else {
                        debug!(pid, grace_ms = grace.as_millis(), "sent SIGTERM, waiting for graceful exit");
                    }

                    // 2. Wait for grace period for clean exit.
                    match tokio::time::timeout(grace, child.wait()).await {
                        Ok(Ok(status)) => {
                            if status.success() {
                                debug!(pid, "worker exited cleanly after SIGTERM");
                                let _ = event_tx.send(WorkerEvent::Stopped { pid });
                            } else {
                                warn!(pid, code = ?status.code(), "worker exited with non-zero status after SIGTERM");
                                let _ = event_tx.send(WorkerEvent::Crashed {
                                    pid,
                                    exit_status: status.code(),
                                });
                            }
                        }
                        Ok(Err(e)) => {
                            warn!(pid, error = %e, "error waiting for worker after SIGTERM");
                            let _ = event_tx.send(WorkerEvent::Crashed {
                                pid,
                                exit_status: None,
                            });
                        }
                        Err(_) => {
                            // 3. Grace period expired — escalate to SIGKILL.
                            warn!(pid, "worker did not exit within grace period, sending SIGKILL");
                            let _ = child.start_kill();
                            let _ = child.wait().await;
                            let _ = event_tx.send(WorkerEvent::Crashed {
                                pid,
                                exit_status: None,
                            });
                        }
                    }
                }
                result = child.wait() => {
                    match result {
                        Ok(status) => {
                            if !status.success() {
                                warn!(
                                    pid,
                                    code = ?status.code(),
                                    "worker process exited with non-zero status"
                                );
                                let _ = event_tx.send(WorkerEvent::Crashed {
                                    pid,
                                    exit_status: status.code(),
                                });
                            } else {
                                // Clean exit (exit 0) — likely from a test script.
                                debug!(pid, "worker exited with status 0");
                                let _ = event_tx.send(WorkerEvent::Stopped { pid });
                            }
                        }
                        Err(e) => {
                            error!(pid, error = %e, "error waiting for worker");
                            let _ = event_tx.send(WorkerEvent::Crashed {
                                pid,
                                exit_status: None,
                            });
                        }
                    }
                }
            }
        });

        let handle = WorkerHandle {
            pid,
            cancel,
            event_tx: self.event_tx.clone(),
            shutdown_grace,
            ipc,
            shutdown_requested: false,
        };

        let _ = self.event_tx.send(WorkerEvent::Started { pid });

        Ok(handle)
    }

    /// Subscribe to worker lifecycle events.
    pub fn subscribe(&self) -> broadcast::Receiver<WorkerEvent> {
        self.event_tx.subscribe()
    }
}

impl Default for WorkerManager {
    fn default() -> Self {
        Self::new()
    }
}
