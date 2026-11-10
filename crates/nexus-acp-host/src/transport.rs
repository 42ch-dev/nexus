//! Agent subprocess transport and lifecycle management.
//!
//! This module handles the low-level process management for ACP agents:
//! spawning subprocesses, managing stdio pipes, platform detection for
//! binary distributions, and graceful shutdown.
//!
//! # Architecture
//!
//! ```text
//! AgentSpawner ──► resolve agent-ref ──► Command
//!                                           │
//! AcpSession ──► spawn subprocess ──► Child process
//!      │                                    │
//!      └─► stdin/stdout pipes ──► ClientSideConnection (SDK)
//!      │                                    │
//!      └─► stderr (inherited) ──► agent logs
//!      │
//!      └─► graceful shutdown ──► cancel → SIGTERM → SIGKILL
//! ```
//!
//! # Platform Detection
//!
//! Binary agents are distributed per-platform. The current platform is
//! detected via `cfg!(target_os)` and `cfg!(target_arch)`:
//! - `darwin-aarch64` (macOS ARM)
//! - `darwin-x86_64` (macOS Intel)
//! - `linux-aarch64` (Linux ARM)
//! - `linux-x86_64` (Linux Intel)
//! - `windows-x86_64` (Windows Intel)

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::timeout;

use crate::error::{AcpError, AcpResult};

/// Platform identifier for binary agent distribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Platform {
    DarwinAarch64,
    DarwinX86_64,
    LinuxAarch64,
    LinuxX86_64,
    WindowsX86_64,
}

#[allow(dead_code)]
impl Platform {
    /// Detect the current platform.
    ///
    /// Returns `None` if the platform is not supported by ACP binary distribution.
    #[must_use]
    pub const fn current() -> Option<Self> {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            Some(Self::DarwinAarch64)
        }
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        {
            Some(Self::DarwinX86_64)
        }
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        {
            Some(Self::LinuxAarch64)
        }
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            Some(Self::LinuxX86_64)
        }
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            Some(Self::WindowsX86_64)
        }
        #[cfg(not(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "macos", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64"),
            all(target_os = "windows", target_arch = "x86_64")
        )))]
        {
            None
        }
    }

    /// Return the platform identifier string used in ACP registry manifests.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::DarwinAarch64 => "darwin-aarch64",
            Self::DarwinX86_64 => "darwin-x86_64",
            Self::LinuxAarch64 => "linux-aarch64",
            Self::LinuxX86_64 => "linux-x86_64",
            Self::WindowsX86_64 => "windows-x86_64",
        }
    }
}

/// Resolves agent references to launch commands.
///
/// For V1.0, this handles:
/// - `npx`-based agents: `npx <package> --acp`
/// - Binary agents: download + cache + execute
///
/// The full registry integration (Task 2) provides the distribution metadata.
/// This module focuses on the transport layer: spawning and managing the subprocess.
#[derive(Debug)]
#[allow(dead_code)]
pub struct AgentSpawner {
    /// Working directory for the agent subprocess.
    cwd: PathBuf,
}

#[allow(dead_code)]
impl AgentSpawner {
    /// Create a new spawner with the given working directory.
    #[must_use]
    pub const fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }

    /// Spawn an agent subprocess using the given launch command.
    ///
    /// This is the generic spawn method that works with any resolved command.
    /// The registry integration (Task 2) determines the actual command string.
    ///
    /// # Arguments
    ///
    /// * `program` — The executable to run (e.g., "npx", "/path/to/agent-binary")
    /// * `args` — Command-line arguments (e.g. `[`"@zed/claude-agent-acp@0.18.0"`, `"--acp"`]`)
    ///
    /// # Returns
    ///
    /// A tuple of:
    /// - `tokio::process::Child` — The spawned subprocess
    /// - `tokio::process::ChildStdin` — Pipe for sending JSON-RPC to the agent
    /// - `tokio::process::ChildStdout` — Pipe for receiving JSON-RPC from the agent
    ///
    /// # Errors
    ///
    /// Returns an error if the program cannot be found or spawned.
    ///
    /// # Panics
    ///
    /// Panics if stdin/stdout pipes cannot be extracted (pipe configuration error).
    pub fn spawn(
        &self,
        program: &str,
        args: &[&str],
    ) -> AcpResult<(
        tokio::process::Child,
        tokio::process::ChildStdin,
        tokio::process::ChildStdout,
    )> {
        self.spawn_with_env(program, args, &[])
    }

    /// Spawn an agent subprocess with additional environment variables.
    ///
    /// Like [`spawn`](Self::spawn), but also sets the given environment
    /// variables on the child process. Existing callers that don't need
    /// env vars should continue to use `spawn()`.
    ///
    /// # Arguments
    ///
    /// * `program` — The executable to run
    /// * `args` — Command-line arguments
    /// * `env` — Key-value pairs to set in the child's environment
    ///
    /// # Errors
    ///
    /// Returns an error if the program cannot be found or spawned.
    ///
    /// # Panics
    ///
    /// Panics if stdin/stdout pipes cannot be extracted (pipe configuration error).
    pub fn spawn_with_env(
        &self,
        program: &str,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> AcpResult<(
        tokio::process::Child,
        tokio::process::ChildStdin,
        tokio::process::ChildStdout,
    )> {
        // Construct the command
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(&self.cwd)
            .stdin(Stdio::piped()) // stdin for JSON-RPC requests
            .stdout(Stdio::piped()) // stdout for JSON-RPC responses
            .stderr(Stdio::inherit()) // stderr for agent logs (visible in terminal)
            .kill_on_drop(true); // Ensure subprocess is killed if the handle is dropped

        // Owned process-group identity (A5): on Unix the child becomes a new
        // process-group leader (PGID == child PID), so the owned process tree
        // — including any descendants the agent spawns — can be signalled and
        // reaped as one unit, guarded by the owned child's liveness. A reused
        // or unowned PID/group is never signalled.
        #[cfg(unix)]
        {
            cmd.process_group(0);
        }

        // Set environment variables
        for (key, value) in env {
            cmd.env(key, value);
        }

        // Spawn the process
        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AcpError::executable_not_found(program)
            } else {
                AcpError::connection_io(e)
            }
        })?;

        // Extract the pipes
        let stdin = child.stdin.take().expect("stdin pipe configured");
        let stdout = child.stdout.take().expect("stdout pipe configured");

        Ok((child, stdin, stdout))
    }

    /// Spawn a mock agent subprocess for testing.
    ///
    /// This uses simple commands like `echo` or `cat` to simulate an agent's
    /// stdio behavior without depending on a real ACP agent.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use std::path::PathBuf;
    /// use nexus_acp_host::transport::AgentSpawner;
    ///
    /// let spawner = AgentSpawner::new(PathBuf::from("/tmp"));
    /// let result = spawner.spawn_mock("echo", &["test-response"]);
    /// if let Ok((child, stdin, stdout)) = result {
    ///     // Use the spawned process
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the program cannot be spawned.
    pub fn spawn_mock(
        &self,
        program: &str,
        args: &[&str],
    ) -> AcpResult<(
        tokio::process::Child,
        tokio::process::ChildStdin,
        tokio::process::ChildStdout,
    )> {
        self.spawn(program, args)
    }
}

/// Platform process-birth identity token.
///
/// Captured at spawn as the numeric PID plus the operating system process
/// start time exposed by `sysinfo`. The token is re-validated before every
/// signal, so a recycled PID with a different birth time is never signalled.
/// The owned Unix process-group identity is `PGID == PID` (process-group
/// leader created at spawn).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessBirthToken {
    /// The numeric PID this token was captured for.
    pub pid: u32,
    /// Birth start tick (platform-specific; opaque).
    pub start_tick: u64,
}

#[cfg(unix)]
fn read_start_tick(pid: u32) -> Option<u64> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

    let pid = Pid::from_u32(pid);
    // sysinfo 0.40: `System::new()` returns `Result` (fails on unsupported
    // OS). Fail closed: no system snapshot → no birth token → cleanup
    // unconfirmed (verify refuses; capture yields None deterministically).
    let Ok(mut system) = System::new() else {
        return None;
    };
    // Selective refresh (sysinfo 0.40.1): load exactly this PID and nothing
    // else. `ProcessesToUpdate::Some` keeps the refresh bounded to one
    // process, `ProcessRefreshKind::nothing()` rules out every optional
    // field (PID / PPID / name / start time are always reported), and
    // `without_tasks()` avoids enumerating the process' threads.
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().without_tasks(),
    );
    system.process(pid).map(sysinfo::Process::start_time)
}

#[cfg(not(unix))]
fn read_start_tick(_pid: u32) -> Option<u64> {
    None
}

impl ProcessBirthToken {
    /// Capture the birth token for a freshly spawned child.
    ///
    /// Returns `None` when the platform cannot provide a birth token or the
    /// process has already exited; callers must treat cleanup as unconfirmed
    /// rather than signalling on a PID alone.
    #[must_use]
    pub fn capture(pid: u32) -> Option<Self> {
        read_start_tick(pid).map(|start_tick| Self { pid, start_tick })
    }

    /// Verify the token still describes the same process birth.
    ///
    /// `true` only when the numeric PID matches AND the platform start time
    /// still matches (a recycled PID with a different start time returns
    /// `false`). On platforms without birth-token support, verification
    /// fails closed (never signal on PID alone).
    #[must_use]
    pub fn verify(&self) -> bool {
        read_start_tick(self.pid) == Some(self.start_tick)
    }
}

/// Owned ACP agent subprocess bound to exactly one Host session.
///
/// Unlike [`AcpSession`] (a CLI-owned one-shot), this struct is the
/// session-scoped process handle used by the Host's `AcpProvider`: it owns
/// the exact `tokio::process::Child` spawned for one Host session and
/// provides bounded cooperative-cancel → SIGTERM → SIGKILL → reap shutdown.
/// Only the exact owned child is ever signalled — a reused/unowned PID is
/// never touched.
pub struct ManagedAcpProcess {
    /// Agent identifier (for logging and error messages).
    agent_id: String,
    /// The subprocess handle (exact owned child). `None` only after the
    /// child has been taken by `Drop` for group teardown (I-005).
    child: Option<tokio::process::Child>,
    /// Path to the agent binary or command (for error reporting).
    agent_path: PathBuf,
    /// Platform process-birth identity: numeric PID plus OS start-time token,
    /// captured at spawn (== the owned process-group id on Unix). Retained
    /// across reap so group quiescence can be confirmed after the leader
    /// exits; the token is RE-VALIDATED before every signal so a recycled
    /// PID/group is never signalled. If it cannot be established or
    /// validated, cleanup is reported unconfirmed.
    birth: Option<ProcessBirthToken>,
}

impl ManagedAcpProcess {
    /// Wrap a freshly spawned child, capturing its platform birth identity.
    ///
    /// The OS birth token (PID + start time) is captured while the child is
    /// still alive; a platform that cannot provide one yields no token, and
    /// cleanup must then be reported unconfirmed rather than signalling on a
    /// PID alone.
    #[must_use]
    pub fn new(agent_id: String, child: tokio::process::Child, agent_path: PathBuf) -> Self {
        let birth = child.id().and_then(ProcessBirthToken::capture);
        Self {
            agent_id,
            child: Some(child),
            agent_path,
            birth,
        }
    }

    /// The owned child's current PID, if it is still running.
    #[must_use]
    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().and_then(tokio::process::Child::id)
    }
    /// Wait for the owned leader while retaining its captured birth identity.
    ///
    /// # Errors
    /// Returns the child's `wait` error, or a typed error when the child was
    /// already taken for teardown.
    pub async fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        match self.child.as_mut() {
            Some(child) => child.wait().await,
            None => Err(std::io::Error::other(
                "owned ACP child already taken for teardown",
            )),
        }
    }

    /// The platform process-birth identity (numeric PID + start token).
    #[must_use]
    pub const fn birth(&self) -> Option<&ProcessBirthToken> {
        self.birth.as_ref()
    }

    /// Non-blocking check whether the owned child has exited.
    pub fn poll_exit(&mut self) -> Option<std::process::ExitStatus> {
        self.child.as_mut().map_or_else(
            || Some(std::process::ExitStatus::default()),
            |child| child.try_wait().ok().flatten(),
        )
    }

    /// Whether the owned child is still running.
    pub fn is_running(&mut self) -> bool {
        self.child
            .as_ref()
            .and_then(tokio::process::Child::id)
            .is_some()
    }

    /// The owned child handle, or a typed error when it was already taken
    /// by `Drop` teardown (I-005). All lifecycle methods route through this
    /// so a dropped child is never signalled/wait-ed on a stale handle.
    fn child_mut(&mut self) -> AcpResult<&mut tokio::process::Child> {
        self.child.as_mut().ok_or_else(|| {
            AcpError::agent_crashed(
                None,
                self.agent_path.clone(),
                Some("owned ACP child already taken for teardown".into()),
            )
        })
    }

    /// Guarded signal: only signal the owned group after the platform
    /// birth token is re-validated (PID + start time still match).
    #[cfg(unix)]
    fn signal_group_guarded(birth: &ProcessBirthToken, signal: nix::sys::signal::Signal) -> bool {
        if !birth.verify() {
            tracing::warn!(
                pid = birth.pid,
                "process birth token invalidated; refusing to signal possibly-recycled group"
            );
            return false;
        }
        let pgrp = nix::unistd::Pid::from_raw(birth.pid.cast_signed());
        nix::sys::signal::killpg(pgrp, signal).is_ok()
    }

    /// Poll the owned process group until quiescent or the bound expires.
    ///
    /// `killpg(pgrp, None)` returns `Ok` while at least one signallable member
    /// exists (guarding against signalling a recycled group id) and `ESRCH`
    /// when the group is fully gone. `EPERM` also proves the group still
    /// exists: the kernel reports it when no member can be signalled, which is
    /// the state of an owned group whose members have all exited but whose
    /// zombies are not reaped yet. It is therefore polled out inside the bound
    /// (never treated as evidence of a recycled group, and never signalled).
    /// Any other error means existence cannot be confirmed and is treated as
    /// not-quiescent (never signalled).
    #[cfg(unix)]
    async fn wait_group_quiescent(pgrp: nix::unistd::Pid, bound: Duration) -> bool {
        tokio::time::timeout(bound, async {
            loop {
                match nix::sys::signal::killpg(pgrp, None) {
                    Ok(()) | Err(nix::errno::Errno::EPERM) => {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                    Err(nix::errno::Errno::ESRCH) => return true,
                    Err(e) => {
                        tracing::warn!(pgrp = %pgrp, error = %e, "process-group existence unconfirmed");
                        return false;
                    }
                }
            }
        })
        .await
        .unwrap_or(false)
    }

    /// Bounded owned process-tree shutdown: wait `grace` for cooperative
    /// exit, then signal the owned process group (SIGTERM), then SIGKILL,
    /// reaping the exact owned child.
    ///
    /// Identity safety: the child is a process-group leader (PGID == child
    /// PID) created at spawn. Every signal targets the owned group via
    /// `killpg`, guarded by the owned child's liveness; a reused/unowned PID
    /// or group is never signalled, and the owned child is always reaped.
    ///
    /// # Errors
    ///
    /// Returns an error when the owned child cannot be confirmed reaped after
    /// the final SIGKILL (cleanup unconfirmed must remain visibly
    /// interrupted, never silently successful).
    // Bounded teardown is a single linear sequence (signal → escalate →
    // reap → group quiescence); splitting it would obscure the ordering.
    #[allow(unreachable_code, clippy::too_many_lines)]
    pub async fn shutdown(&mut self, grace: Duration) -> AcpResult<()> {
        tracing::info!(
            agent_id = %self.agent_id,
            grace_ms = grace.as_millis(),
            "Initiating owned ACP process-tree shutdown"
        );

        // Step 1: wait up to `grace` for cooperative exit (the caller has
        // already sent the ACP session/cancel notification).
        let wait_result = timeout(grace, self.child_mut()?.wait()).await;
        match wait_result {
            Ok(Ok(status)) => {
                tracing::info!(
                    agent_id = %self.agent_id,
                    exit_code = ?status.code(),
                    "Owned ACP process exited cooperatively"
                );
                return Ok(());
            }
            Ok(Err(e)) => {
                tracing::error!(
                    agent_id = %self.agent_id,
                    error = %e,
                    "Failed to wait for owned ACP process exit"
                );
                return Err(AcpError::cleanup_unconfirmed(e.to_string()));
            }
            Err(_) => {
                tracing::warn!(
                    agent_id = %self.agent_id,
                    timeout = ?grace,
                    "Owned ACP process did not exit within grace, proceeding to group SIGTERM"
                );
            }
        }

        #[cfg(unix)]
        {
            use nix::sys::signal::Signal;

            // Group identity comes from the platform process-birth identity
            // (PGID == birth PID), NOT the live child id: after the leader is
            // reaped, the birth identity still identifies the owned group.
            // Every signal re-validates the birth token (PID + OS start
            // time); a missing or invalidated token means cleanup is
            // UNCONFIRMED — the group is never signalled on a PID alone.
            let Some(birth) = self.birth.clone() else {
                // No birth token: the platform cannot prove ownership. Reap
                // the leader and report unconfirmed cleanup, never signal.
                let status = self
                    .child_mut()?
                    .wait()
                    .await
                    .map_err(|e| AcpError::cleanup_unconfirmed(e.to_string()))?;
                tracing::warn!(
                    agent_id = %self.agent_id,
                    exit_code = ?status.code(),
                    "no process-birth token; owned ACP cleanup unconfirmed (leader reaped)"
                );
                return Err(AcpError::cleanup_unconfirmed(
                    "No process-birth token; owned ACP process cleanup unconfirmed",
                ));
            };
            let pgrp = nix::unistd::Pid::from_raw(birth.pid.cast_signed());

            // If the leader already exited during the grace window, confirm
            // the whole owned group is quiescent (guarded by the birth
            // token's numeric identity) before claiming success; survivors
            // are still SIGKILLed as a unit.
            if self.child_mut()?.id().is_none() {
                if Self::wait_group_quiescent(pgrp, grace).await {
                    let status = self
                        .child_mut()?
                        .wait()
                        .await
                        .map_err(|e| AcpError::cleanup_unconfirmed(e.to_string()))?;
                    tracing::info!(
                        agent_id = %self.agent_id,
                        exit_code = ?status.code(),
                        "Owned ACP process exited between grace and group signal; group quiescent"
                    );
                    return Ok(());
                }
                if Self::signal_group_guarded(&birth, Signal::SIGKILL)
                    && Self::wait_group_quiescent(pgrp, grace).await
                {
                    let _ = self.child_mut()?.wait().await;
                    return Ok(());
                }
                return Err(AcpError::cleanup_unconfirmed(
                    "Owned ACP process group cleanup unconfirmed after SIGKILL",
                ));
            }

            // Step 2: SIGTERM the owned process group (leader + descendants).
            if Self::signal_group_guarded(&birth, Signal::SIGTERM) {
                tracing::debug!(agent_id = %self.agent_id, "SIGTERM sent to owned ACP process group");
            } else {
                tracing::warn!(
                    agent_id = %self.agent_id,
                    "birth token invalidated or SIGTERM failed; owned group not signalled"
                );
                // Escalate immediately: unconfirmed group signal means
                // cleanup is not confirmed.
                if !Self::signal_group_guarded(&birth, Signal::SIGKILL) {
                    let _ = self.child_mut()?.wait().await;
                    return Err(AcpError::cleanup_unconfirmed(
                        "Owned ACP process group cleanup unconfirmed (signal refused)",
                    ));
                }
            }

            let wait_result = timeout(grace, self.child_mut()?.wait()).await;
            match wait_result {
                Ok(Ok(status)) => {
                    // Leader exited after group SIGTERM. Ownership is only
                    // confirmed when the WHOLE owned group is quiescent: a
                    // descendant that ignored SIGTERM must be SIGKILLed.
                    if Self::wait_group_quiescent(pgrp, grace).await {
                        tracing::info!(
                            agent_id = %self.agent_id,
                            exit_code = ?status.code(),
                            "Owned ACP process group quiescent after group SIGTERM"
                        );
                        return Ok(());
                    }
                    tracing::warn!(
                        agent_id = %self.agent_id,
                        "Descendants remain after group SIGTERM, proceeding to group SIGKILL"
                    );
                    if Self::signal_group_guarded(&birth, Signal::SIGKILL)
                        && Self::wait_group_quiescent(pgrp, grace).await
                    {
                        tracing::info!(
                            agent_id = %self.agent_id,
                            exit_code = ?status.code(),
                            "Owned ACP process group quiescent after group SIGKILL"
                        );
                        return Ok(());
                    }
                    tracing::error!(
                        agent_id = %self.agent_id,
                        "Owned ACP process group still not quiescent after SIGKILL"
                    );
                    return Err(AcpError::cleanup_unconfirmed(
                        "Owned ACP process group cleanup unconfirmed after SIGKILL",
                    ));
                }
                Err(_) => {
                    tracing::warn!(
                        agent_id = %self.agent_id,
                        "Owned ACP process did not exit after group SIGTERM, proceeding to SIGKILL"
                    );
                    if Self::signal_group_guarded(&birth, Signal::SIGKILL) {
                        let status = self
                            .child_mut()?
                            .wait()
                            .await
                            .map_err(|e| AcpError::cleanup_unconfirmed(e.to_string()))?;
                        if Self::wait_group_quiescent(pgrp, grace).await {
                            tracing::info!(
                                agent_id = %self.agent_id,
                                exit_code = ?status.code(),
                                "Owned ACP process killed forcefully (group SIGKILL); group quiescent"
                            );
                            return Ok(());
                        }
                    }
                    tracing::error!(
                        agent_id = %self.agent_id,
                        "Owned ACP process group cleanup unconfirmed after SIGKILL"
                    );
                    let _ = self.child_mut()?.wait().await;
                    return Err(AcpError::cleanup_unconfirmed(
                        "Owned ACP process group cleanup unconfirmed after SIGKILL",
                    ));
                }
                Ok(Err(e)) => {
                    tracing::error!(
                        agent_id = %self.agent_id,
                        error = %e,
                        "Failed to wait after group SIGTERM"
                    );
                    return Err(AcpError::cleanup_unconfirmed(e.to_string()));
                }
            }
        }

        #[cfg(windows)]
        {
            let agent_path = self.agent_path.clone();
            self.child_mut()?.kill().await.map_err(|e| {
                AcpError::agent_crashed(None, agent_path.clone(), Some(e.to_string()))
            })?;
            let status = self.child_mut()?.wait().await.map_err(|e| {
                AcpError::agent_crashed(None, agent_path.clone(), Some(e.to_string()))
            })?;
            tracing::info!(
                agent_id = %self.agent_id,
                exit_code = ?status.code(),
                "Owned ACP process killed forcefully (Windows kill())"
            );
        }

        Ok(())
    }

    /// Bounded teardown of an owned child that failed to launch/connect.
    ///
    /// Used on launch-failure paths: the spawned child has no ACP session to
    /// cancel cooperatively, so we proceed directly to the owned process-tree
    /// termination/reap sequence (group SIGTERM → SIGKILL → reap). Every
    /// signal re-validates the platform process-birth token (PID + OS start
    /// time): a missing or invalidated token refuses to signal and returns
    /// cleanup-unconfirmed.
    ///
    /// # Errors
    ///
    /// Returns an error when the owned child/group cannot be confirmed
    /// reaped.
    // Single linear teardown sequence; see `shutdown`.
    #[allow(clippy::too_many_lines)]
    pub async fn terminate(mut self, grace: Duration) -> AcpResult<()> {
        // I-005: take the owned child out so `Drop` does not re-run
        // teardown on the already-reaped handle after this consumes self.
        let mut child = self.child.take().ok_or_else(|| {
            AcpError::agent_crashed(
                None,
                self.agent_path.clone(),
                Some("owned ACP child already taken for teardown".into()),
            )
        })?;

        #[cfg(unix)]
        {
            use nix::sys::signal::Signal;
            let Some(birth) = self.birth.clone() else {
                // No birth token: the platform cannot prove ownership. Reap
                // the leader if it is already dead and report unconfirmed
                // cleanup, never signal. Every wait is bounded, and a
                // deadline expiry leaks the child handle (mem::forget)
                // instead of dropping it: dropping would fire kill_on_drop
                // and turn the refusal into an implicit kill.
                match timeout(grace, child.wait()).await {
                    Ok(Ok(_)) => {}
                    Ok(Err(e)) => return Err(AcpError::cleanup_unconfirmed(e.to_string())),
                    Err(_) => {
                        std::mem::forget(child);
                        return Err(AcpError::cleanup_unconfirmed(
                            "No process-birth token; owned ACP process left unreaped past the cleanup deadline",
                        ));
                    }
                }
                tracing::warn!(
                    agent_id = %self.agent_id,
                    "no process-birth token; owned ACP launch teardown unconfirmed"
                );
                return Err(AcpError::cleanup_unconfirmed(
                    "No process-birth token; owned ACP process cleanup unconfirmed",
                ));
            };
            let pgrp = nix::unistd::Pid::from_raw(birth.pid.cast_signed());

            // 1. Signal FIRST, while identity is provable: on platforms where
            // a zombie leader's start time stays readable (Linux), the
            // SIGTERM reaches live descendants even though the leader already
            // exited. A refusal means identity cannot be proven — the
            // dead-but-unreaped-leader case — handled by reap-on-sight below;
            // an unprovable identity is never signalled twice.
            let sigterm_sent = Self::signal_group_guarded(&birth, Signal::SIGTERM);
            if sigterm_sent {
                tracing::debug!(
                    agent_id = %self.agent_id,
                    "SIGTERM sent to owned ACP process group"
                );

                #[cfg(target_os = "linux")]
                {
                    // 2. Observe the leader's exit WITHOUT reaping (Linux:
                    // safe `waitid(WEXITED | WNOWAIT | WNOHANG)` leaves the
                    // zombie unreaped; bounded by the cleanup deadline).
                    // Reaping here would invalidate the birth token and
                    // permanently disable the SIGKILL escalation — the
                    // group leak this ordering prevents. A probe failure
                    // surfaces immediately; deadline expiry only means the
                    // leader ignored SIGTERM and is escalated while its
                    // identity verifies below.
                    let observed = tokio::time::timeout(grace, async {
                        loop {
                            match nix::sys::wait::waitid(
                                nix::sys::wait::Id::Pid(pgrp),
                                nix::sys::wait::WaitPidFlag::WEXITED
                                    | nix::sys::wait::WaitPidFlag::WNOWAIT
                                    | nix::sys::wait::WaitPidFlag::WNOHANG,
                            ) {
                                Ok(nix::sys::wait::WaitStatus::StillAlive) => {
                                    tokio::time::sleep(Duration::from_millis(10)).await;
                                }
                                Ok(_) => return Ok(()),
                                Err(e) => return Err(AcpError::cleanup_unconfirmed(e.to_string())),
                            }
                        }
                    })
                    .await;
                    if let Ok(Err(e)) = observed {
                        return Err(e);
                    }

                    // 3. One guarded SIGKILL while the token still verifies:
                    // lethal for SIGTERM-ignoring survivors, a no-op for the
                    // leader zombie and already-dead members. The leader's
                    // own zombie keeps `killpg(_, 0)` non-ESRCH, so proving
                    // group quiescence BEFORE the reap is impossible — the
                    // quiescence check after the reap proves the outcome.
                    if !Self::signal_group_guarded(&birth, Signal::SIGKILL) {
                        if timeout(grace, child.wait()).await.is_err() {
                            std::mem::forget(child);
                        }
                        return Err(AcpError::cleanup_unconfirmed(
                            "Owned ACP process group cleanup unconfirmed (identity invalidated before SIGKILL)",
                        ));
                    }

                    // 4. Reap the leader (bounded), then prove the whole
                    // group gone.
                    let status = match timeout(grace, child.wait()).await {
                        Ok(Ok(status)) => status,
                        Ok(Err(e)) => return Err(AcpError::cleanup_unconfirmed(e.to_string())),
                        Err(_) => {
                            std::mem::forget(child);
                            return Err(AcpError::cleanup_unconfirmed(
                                "Owned ACP process group cleanup unconfirmed (leader reap deadline expired)",
                            ));
                        }
                    };
                    if Self::wait_group_quiescent(pgrp, grace).await {
                        tracing::info!(
                            agent_id = %self.agent_id,
                            exit_code = ?status.code(),
                            "Owned ACP process group quiescent after teardown; leader reaped"
                        );
                        return Ok(());
                    }
                    return Err(AcpError::cleanup_unconfirmed(
                        "Owned ACP process group cleanup unconfirmed after SIGKILL",
                    ));
                }

                #[cfg(not(target_os = "linux"))]
                {
                    // PLATFORM LIMITATION (macOS / other unix): nix exposes
                    // `waitid` only on Linux/FreeBSD, and once the leader
                    // exits its zombie identity is unreadable here, so an
                    // escalation after exit is impossible. This is not the
                    // intended ordering — it is the best safe ordering this
                    // platform allows: graceful leader exit (bounded), then
                    // quiescence; survivors are refused (honest
                    // CleanupUnconfirmed), and a leader still alive past the
                    // grace still gets its guarded SIGKILL while provable.
                    let wait_result = timeout(grace, child.wait()).await;
                    match wait_result {
                        Ok(Ok(_)) => {
                            // Leader reaped: remaining members are the
                            // descendants, and the identity to signal them
                            // is gone on this platform.
                            if Self::wait_group_quiescent(pgrp, grace).await {
                                tracing::info!(
                                    agent_id = %self.agent_id,
                                    "Owned ACP process group quiescent after group SIGTERM"
                                );
                                return Ok(());
                            }
                            tracing::warn!(
                                agent_id = %self.agent_id,
                                "Descendants remain after group SIGTERM; identity unprovable after leader exit, refusing to signal"
                            );
                            return Err(AcpError::cleanup_unconfirmed(
                                "Owned ACP process group cleanup unconfirmed (signal refused)",
                            ));
                        }
                        Err(_) => {
                            // Leader ignored SIGTERM and is still alive —
                            // identity provable: guarded SIGKILL, bounded
                            // reap, quiescence.
                            tracing::warn!(
                                agent_id = %self.agent_id,
                                "Owned ACP process did not exit after group SIGTERM, proceeding to SIGKILL"
                            );
                            if !Self::signal_group_guarded(&birth, Signal::SIGKILL) {
                                if timeout(grace, child.wait()).await.is_err() {
                                    std::mem::forget(child);
                                }
                                return Err(AcpError::cleanup_unconfirmed(
                                    "Owned ACP process group cleanup unconfirmed (identity invalidated before SIGKILL)",
                                ));
                            }
                            let status = match timeout(grace, child.wait()).await {
                                Ok(Ok(status)) => status,
                                Ok(Err(e)) => {
                                    return Err(AcpError::cleanup_unconfirmed(e.to_string()));
                                }
                                Err(_) => {
                                    std::mem::forget(child);
                                    return Err(AcpError::cleanup_unconfirmed(
                                        "Owned ACP process group cleanup unconfirmed (leader reap deadline expired)",
                                    ));
                                }
                            };
                            if Self::wait_group_quiescent(pgrp, grace).await {
                                tracing::info!(
                                    agent_id = %self.agent_id,
                                    exit_code = ?status.code(),
                                    "Owned ACP process killed forcefully (group SIGKILL); group quiescent"
                                );
                                return Ok(());
                            }
                            if timeout(grace, child.wait()).await.is_err() {
                                std::mem::forget(child);
                            }
                            return Err(AcpError::cleanup_unconfirmed(
                                "Owned ACP process group cleanup unconfirmed after SIGKILL",
                            ));
                        }
                        Ok(Err(e)) => {
                            // A failed reap leaves liveness unproven.
                            return Err(AcpError::cleanup_unconfirmed(e.to_string()));
                        }
                    }
                }
            }

            tracing::warn!(
                agent_id = %self.agent_id,
                "birth token invalidated or SIGTERM failed; owned group not signalled"
            );
            // SIGTERM refused: identity cannot be proven — the
            // dead-but-unreaped-leader case, or an invalidated token — and
            // an unprovable identity is never signalled in any form. Reap a
            // leader that already exited (zombie) so an already-dead tree is
            // confirmed gone (the observed macOS CI launch-failure case)
            // instead of misreported, then let bounded waits and the
            // quiescence check decide. All waits are bounded; a deadline
            // expiry leaks the handle instead of dropping it into an
            // implicit kill.
            let _ = child.try_wait();
            match timeout(grace, child.wait()).await {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => return Err(AcpError::cleanup_unconfirmed(e.to_string())),
                Err(_) => {
                    std::mem::forget(child);
                    return Err(AcpError::cleanup_unconfirmed(
                        "Owned ACP process group cleanup unconfirmed (signal refused; leader alive past the cleanup deadline)",
                    ));
                }
            }
            if Self::wait_group_quiescent(pgrp, grace).await {
                return Ok(());
            }
            // Survivors cannot be signalled on an unprovable identity.
            Err(AcpError::cleanup_unconfirmed(
                "Owned ACP process group cleanup unconfirmed (signal refused)",
            ))
        }

        #[cfg(not(unix))]
        {
            let agent_path = self.agent_path.clone();
            let _ = child.kill().await;
            child
                .wait()
                .await
                .map_err(|e| AcpError::agent_crashed(None, agent_path, Some(e.to_string())))?;
            Ok(())
        }
    }
}

impl Drop for ManagedAcpProcess {
    /// I-005: ownership-guarded group teardown when the launch future is
    /// dropped mid-`connect_session` (e.g. the `HostManager` launch timeout
    /// drops the future, dropping this handle). A plain `tokio::process::Child`
    /// drop kills only the leader; the owned process GROUP (leader +
    /// descendants) would be orphaned. This `Drop` takes the child and runs
    /// the same bounded guarded teardown as [`Self::terminate`] on a
    /// detached task when a tokio runtime is available, or a synchronous
    /// guarded `killpg(SIGKILL)` + reap loop otherwise.
    fn drop(&mut self) {
        let Some(child) = self.child.take() else {
            return; // already taken (terminate consumed it or prior drop)
        };
        let agent_id = self.agent_id.clone();
        let agent_path = self.agent_path.clone();
        let birth = self.birth.clone();

        // Detached async teardown when a runtime is live: reconstruct the
        // handle and run the full bounded terminate sequence (guarded
        // SIGTERM → SIGKILL → reap → group quiescence).
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let owned = Self {
                    agent_id: agent_id.clone(),
                    child: Some(child),
                    agent_path,
                    birth,
                };
                if let Err(e) = owned.terminate(Duration::from_secs(5)).await {
                    tracing::warn!(
                        agent_id = %agent_id,
                        error = %e,
                        "dropped owned ACP process teardown unconfirmed (I-005)"
                    );
                }
            });
            return;
        }

        // No runtime: synchronous guarded teardown. Signal the owned group
        // (birth-validated) and reap the leader with a bounded wait loop.
        #[cfg(unix)]
        {
            use nix::sys::signal::Signal;

            let mut child = child;
            if let Some(birth) = &birth {
                if Self::signal_group_guarded(birth, Signal::SIGKILL) {
                    let pgrp = nix::unistd::Pid::from_raw(birth.pid.cast_signed());
                    let deadline = std::time::Instant::now() + Duration::from_secs(5);
                    while std::time::Instant::now() < deadline {
                        match nix::sys::signal::killpg(pgrp, None) {
                            Ok(()) => std::thread::sleep(Duration::from_millis(10)),
                            Err(nix::errno::Errno::ESRCH) => break,
                            Err(e) => {
                                tracing::warn!(
                                    agent_id = %agent_id,
                                    pgrp = %pgrp,
                                    error = %e,
                                    "synchronous drop teardown: group existence unconfirmed"
                                );
                                break;
                            }
                        }
                    }
                } else {
                    tracing::warn!(
                        agent_id = %agent_id,
                        "synchronous drop teardown: birth token invalidated; refusing to signal possibly-recycled group"
                    );
                }
            }
            // Reap the leader (bounded): `try_wait` until it exits or the
            // bound expires; a still-running leader is left for the OS
            // reaper (it was SIGKILLed above).
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if std::time::Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Ok(None) => {
                        tracing::warn!(
                            agent_id = %agent_id,
                            "synchronous drop teardown: leader not reaped within bound"
                        );
                        break;
                    }
                    Err(e) => {
                        tracing::warn!(
                            agent_id = %agent_id,
                            error = %e,
                            "synchronous drop teardown: leader reap error"
                        );
                        break;
                    }
                }
            }
        }
        #[cfg(not(unix))]
        {
            // Non-Unix: best-effort kill + reap (no process-group identity).
            let mut child = child;
            let _ = child.start_kill();
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while std::time::Instant::now() < deadline {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                    Err(_) => break,
                }
            }
        }
    }
}

/// Manages the lifecycle of an ACP agent session.
///
/// This struct owns the subprocess, the I/O future handle, and provides
/// methods for graceful shutdown. It is designed to be used with the
/// `AcpSdkAdapter` which wraps the actual ACP SDK connection.
///
/// # Lifecycle
///
/// 1. **Create**: Spawn process → establish SDK connection
/// 2. **Use**: Initialize → session → prompt loop
/// 3. **Destroy**: Cancel notification → wait 5s → SIGTERM → SIGKILL
///
/// # Shutdown Protocol
///
/// Per tech spec §2.3, graceful shutdown follows this sequence:
/// 1. Send `cancel` notification to the agent
/// 2. Wait up to 5 seconds for the agent to finish
/// 3. If still running, send SIGTERM
/// 4. If still running after SIGTERM, send SIGKILL
#[allow(dead_code)]
pub struct AcpSession {
    /// Agent identifier (for logging and error messages).
    agent_id: String,
    /// The subprocess handle.
    child: tokio::process::Child,
    /// Path to the agent binary or command (for error reporting).
    agent_path: PathBuf,
    /// Handle to the background I/O task (if spawned separately).
    _io_task: Option<JoinHandle<()>>,
    /// Cancellation signal sender (for graceful shutdown).
    cancel_tx: Option<oneshot::Sender<()>>,
}

#[allow(dead_code)]
impl AcpSession {
    /// Create a new session from a spawned subprocess.
    ///
    /// This is called after `AgentSpawner::spawn()` and before establishing
    /// the ACP SDK connection. The actual SDK wiring happens in `client.rs`.
    ///
    /// # Arguments
    ///
    /// * `agent_id` — The agent's identifier (e.g., "claude-acp")
    /// * `child` — The spawned subprocess
    /// * `agent_path` — The binary path or command string (for error messages)
    /// * `io_task` — Optional handle to the background I/O task (if spawned)
    /// * `cancel_tx` — Optional cancellation signal sender
    #[must_use]
    pub const fn new(
        agent_id: String,
        child: tokio::process::Child,
        agent_path: PathBuf,
        io_task: Option<JoinHandle<()>>,
        cancel_tx: Option<oneshot::Sender<()>>,
    ) -> Self {
        Self {
            agent_id,
            child,
            agent_path,
            _io_task: io_task,
            cancel_tx,
        }
    }

    /// Perform graceful shutdown of the agent subprocess.
    ///
    /// This implements the shutdown protocol from tech spec §2.3:
    /// 1. Send cancel notification (if `cancel_tx` is present)
    /// 2. Wait up to 5 seconds for graceful exit
    /// 3. Send SIGTERM if still running
    /// 4. Send SIGKILL if still running after SIGTERM
    ///
    /// # Errors
    ///
    /// Returns an error if the subprocess fails to terminate after SIGKILL,
    /// or if there's an I/O error during the shutdown sequence.
    #[allow(clippy::too_many_lines)]
    pub async fn shutdown(mut self) -> AcpResult<()> {
        tracing::info!(
            agent_id = %self.agent_id,
            "Initiating graceful shutdown for agent session"
        );

        // Step 1: Send cancel notification (if available)
        if let Some(cancel_tx) = self.cancel_tx.take() {
            // Send the cancel signal; ignore errors if receiver already dropped
            let _ = cancel_tx.send(());
            tracing::debug!(agent_id = %self.agent_id, "Cancel notification sent");
        }

        // Step 2: Wait up to 5 seconds for graceful exit
        let graceful_timeout = Duration::from_secs(5);
        let wait_result = timeout(graceful_timeout, self.child.wait()).await;

        match wait_result {
            Ok(Ok(status)) => {
                tracing::info!(
                    agent_id = %self.agent_id,
                    exit_code = ?status.code(),
                    "Agent exited gracefully"
                );
                return Ok(());
            }
            Ok(Err(e)) => {
                tracing::error!(
                    agent_id = %self.agent_id,
                    error = %e,
                    "Failed to wait for agent exit"
                );
                return Err(AcpError::agent_crashed(
                    None,
                    self.agent_path,
                    Some(e.to_string()),
                ));
            }
            Err(_) => {
                tracing::warn!(
                    agent_id = %self.agent_id,
                    timeout = ?graceful_timeout,
                    "Agent did not exit within timeout, proceeding to SIGTERM"
                );
            }
        }

        // Step 3: Send SIGTERM (Unix) or skip to kill (Windows)
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;

            let pid = self.child.id().ok_or_else(|| {
                AcpError::agent_crashed(
                    None,
                    self.agent_path.clone(),
                    Some("Cannot get PID: process has already exited".into()),
                )
            })?;
            let pid = Pid::from_raw(pid.cast_signed());
            if let Err(e) = kill(pid, Signal::SIGTERM) {
                tracing::warn!(
                    agent_id = %self.agent_id,
                    error = %e,
                    "Failed to send SIGTERM to agent"
                );
            } else {
                tracing::debug!(agent_id = %self.agent_id, "SIGTERM sent to agent");
            }

            // Step 4: Wait briefly after SIGTERM, then SIGKILL if needed
            let sigterm_timeout = Duration::from_secs(2);
            let wait_result = timeout(sigterm_timeout, self.child.wait()).await;

            match wait_result {
                Ok(Ok(status)) => {
                    tracing::info!(
                        agent_id = %self.agent_id,
                        exit_code = ?status.code(),
                        "Agent exited after SIGTERM"
                    );
                    return Ok(());
                }
                Err(_) => {
                    tracing::warn!(
                        agent_id = %self.agent_id,
                        "Agent did not exit after SIGTERM, proceeding to SIGKILL"
                    );

                    // SIGKILL
                    let pid = self.child.id().ok_or_else(|| {
                        AcpError::agent_crashed(
                            None,
                            self.agent_path.clone(),
                            Some("Cannot get PID: process has already exited".into()),
                        )
                    })?;
                    let pid = Pid::from_raw(pid.cast_signed());
                    if let Err(e) = kill(pid, Signal::SIGKILL) {
                        tracing::error!(
                            agent_id = %self.agent_id,
                            error = %e,
                            "Failed to send SIGKILL to agent"
                        );
                        return Err(AcpError::agent_crashed(
                            None,
                            self.agent_path,
                            Some(format!("Failed to kill agent: {e}")),
                        ));
                    }

                    // Final wait after SIGKILL
                    let status = self.child.wait().await.map_err(|e| {
                        AcpError::agent_crashed(None, self.agent_path, Some(e.to_string()))
                    })?;

                    tracing::info!(
                        agent_id = %self.agent_id,
                        exit_code = ?status.code(),
                        "Agent killed forcefully (SIGKILL)"
                    );
                }
                Ok(Err(e)) => {
                    tracing::error!(
                        agent_id = %self.agent_id,
                        error = %e,
                        "Failed to wait after SIGTERM"
                    );
                    return Err(AcpError::agent_crashed(
                        None,
                        self.agent_path.clone(),
                        Some(e.to_string()),
                    ));
                }
            }
        }

        #[cfg(windows)]
        {
            // On Windows, directly kill the process.
            // V1.153 P2 T2: `agent_path` is non-Copy and both error closures
            // need it (the unix branch avoids this because its first use is
            // on a `return` path); clone once per closure — error path only.
            let agent_path = self.agent_path.clone();
            self.child.kill().await.map_err(|e| {
                AcpError::agent_crashed(None, agent_path.clone(), Some(e.to_string()))
            })?;

            let status = self.child.wait().await.map_err(|e| {
                AcpError::agent_crashed(None, agent_path.clone(), Some(e.to_string()))
            })?;

            tracing::info!(
                agent_id = %self.agent_id,
                exit_code = ?status.code(),
                "Agent killed forcefully (Windows kill())"
            );
        }

        Ok(())
    }

    /// Check if the subprocess is still running.
    pub fn is_running(&mut self) -> bool {
        self.child.id().is_some()
    }

    /// Get the agent identifier.
    #[must_use]
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    /// Get the agent path (for error reporting).
    #[must_use]
    pub const fn agent_path(&self) -> &PathBuf {
        &self.agent_path
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn platform_detection() {
        let platform = Platform::current();

        #[cfg(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "macos", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64"),
            all(target_os = "windows", target_arch = "x86_64")
        ))]
        {
            assert!(platform.is_some());
            let p = platform.unwrap();
            assert_ne!(p.as_str(), "");
        }

        #[cfg(not(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "macos", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64"),
            all(target_os = "windows", target_arch = "x86_64")
        )))]
        {
            assert!(platform.is_none());
        }
    }

    #[test]
    fn platform_as_str() {
        assert_eq!(Platform::DarwinAarch64.as_str(), "darwin-aarch64");
        assert_eq!(Platform::DarwinX86_64.as_str(), "darwin-x86_64");
        assert_eq!(Platform::LinuxAarch64.as_str(), "linux-aarch64");
        assert_eq!(Platform::LinuxX86_64.as_str(), "linux-x86_64");
        assert_eq!(Platform::WindowsX86_64.as_str(), "windows-x86_64");
    }

    #[tokio::test]
    async fn spawn_mock_agent() {
        let spawner = AgentSpawner::new(PathBuf::from("/tmp"));

        #[cfg(unix)]
        let result = spawner.spawn_mock("cat", &[]);

        #[cfg(windows)]
        let result = spawner.spawn_mock("cmd", &["/c", "type"]);

        if let Ok((child, _stdin, _stdout)) = result {
            assert!(child.id().is_some());

            // Clean up
            let mut child = child;
            child.kill().await.expect("failed to kill mock agent");
        }
    }

    #[tokio::test]
    async fn poll_exit_detects_external_sigkill() {
        let child = tokio::process::Command::new("sleep")
            .arg("300")
            .spawn()
            .expect("spawn sleep");
        let pid = child.id().expect("pid");
        let mut proc = ManagedAcpProcess::new(
            "sleep-fixture".to_string(),
            child,
            PathBuf::from("/usr/bin/sleep"),
        );
        std::process::Command::new("kill")
            .arg("-9")
            .arg(pid.to_string())
            .status()
            .expect("kill");
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            proc.poll_exit().is_some(),
            "expected poll_exit after external SIGKILL"
        );
    }

    #[tokio::test]
    async fn spawn_fails_for_missing_executable() {
        let spawner = AgentSpawner::new(PathBuf::from("/tmp"));

        let result = spawner.spawn("nonexistent-program", &[]);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(err, AcpError::ExecutableNotFound { .. }));
        assert!(err.to_string().contains("not found on PATH"));
    }

    /// An unavailable PID yields no birth token and can never verify: capture
    /// fails closed, so cleanup is reported unconfirmed instead of signalling
    /// on a PID alone.
    #[cfg(unix)]
    #[test]
    fn birth_token_unavailable_pid_fails_closed() {
        // Above every supported platform's `pid_max` (Linux caps at 2^22,
        // macOS at 99998), so this PID can never name a live process.
        let unavailable = i32::MAX.cast_unsigned();
        assert!(
            ProcessBirthToken::capture(unavailable).is_none(),
            "an unavailable PID must not produce a birth token"
        );

        let dangling = ProcessBirthToken {
            pid: unavailable,
            start_tick: 0,
        };
        assert!(!dangling.verify(), "an unavailable PID must never verify");
    }

    /// A recycled PID (same number, different OS start time) is never
    /// signalled: the guarded group signal refuses and the real owned group
    /// survives untouched.
    #[cfg(unix)]
    #[tokio::test]
    async fn guarded_group_signal_refuses_recycled_birth_token() {
        use nix::sys::signal::{killpg, Signal};

        let spawner = AgentSpawner::new(PathBuf::from("/tmp"));
        let (mut child, _stdin, _stdout) = spawner
            .spawn("/bin/sleep", &["300"])
            .expect("spawn grouped sleep fixture");
        let pid = child.id().expect("owned child pid");
        let birth = ProcessBirthToken::capture(pid).expect("live child has a birth token");
        let pgrp = nix::unistd::Pid::from_raw(pid.cast_signed());
        assert!(birth.verify(), "the live owned child must verify");
        assert!(killpg(pgrp, None).is_ok(), "the owned group must exist");

        // Same PID, different OS start time == a different process birth.
        let recycled = ProcessBirthToken {
            pid: birth.pid,
            start_tick: birth.start_tick.wrapping_add(1),
        };
        assert!(!recycled.verify(), "a different birth time must not verify");
        assert!(
            !ManagedAcpProcess::signal_group_guarded(&recycled, Signal::SIGTERM),
            "a recycled birth token must never be SIGTERMed"
        );
        assert!(
            !ManagedAcpProcess::signal_group_guarded(&recycled, Signal::SIGKILL),
            "even SIGKILL must be refused for a recycled birth token"
        );

        // Nothing reached the owned group: it is still alive and still
        // described by its true birth token.
        assert!(
            killpg(pgrp, None).is_ok(),
            "the owned group must be untouched"
        );
        assert!(birth.verify(), "the owned child must still be alive");

        // Owned cleanup with the true token still works.
        assert!(ManagedAcpProcess::signal_group_guarded(
            &birth,
            Signal::SIGKILL
        ));
        let _ = child.wait().await;
    }

    /// Owned-group teardown reaps the leader AND a descendant that inherited
    /// the owned process group — a leader-only teardown would orphan it.
    #[cfg(unix)]
    #[tokio::test]
    async fn guarded_terminate_reaps_owned_group_with_descendant() {
        use nix::sys::signal::{kill, killpg};
        use tokio::io::AsyncBufReadExt;

        let spawner = AgentSpawner::new(PathBuf::from("/tmp"));
        // Group leader (`sh`) waits on a background `sleep` that inherits the
        // owned process group without being the leader itself. The leader's
        // TERM trap reaps that descendant before exiting, so the end state is
        // observable without depending on when the OS reaps an orphan.
        // stderr is dropped inside the fixture: the shell's job-control notice
        // for the signalled background job is not part of this contract.
        let script = "exec 2>/dev/null; sleep 300 & echo $!; \
                      trap \"wait; exit 0\" TERM; while :; do sleep 0.05; done";
        let (child, _stdin, stdout) = spawner
            .spawn("/bin/sh", &["-c", script])
            .expect("spawn grouped fixture");
        let leader = child.id().expect("leader pid");

        let mut lines = tokio::io::BufReader::new(stdout).lines();
        let descendant: u32 = tokio::time::timeout(Duration::from_secs(5), lines.next_line())
            .await
            .expect("descendant pid line within bound")
            .expect("descendant pid line readable")
            .expect("descendant pid present")
            .trim()
            .parse()
            .expect("descendant pid is numeric");
        assert_ne!(descendant, leader, "the descendant is not the leader");

        let pgrp = nix::unistd::Pid::from_raw(leader.cast_signed());
        let leader_pid = nix::unistd::Pid::from_raw(leader.cast_signed());
        let descendant_pid = nix::unistd::Pid::from_raw(descendant.cast_signed());
        assert_eq!(
            nix::unistd::getpgid(Some(descendant_pid)).ok(),
            Some(pgrp),
            "the descendant must inherit the owned process group"
        );
        assert!(
            killpg(pgrp, None).is_ok(),
            "the owned group must exist before teardown"
        );
        assert!(
            kill(leader_pid, None).is_ok(),
            "the leader must be alive before teardown"
        );
        assert!(
            kill(descendant_pid, None).is_ok(),
            "the descendant must be alive before teardown"
        );

        let proc =
            ManagedAcpProcess::new("group-fixture".to_string(), child, PathBuf::from("/bin/sh"));
        proc.terminate(Duration::from_secs(5))
            .await
            .expect("owned group teardown must be confirmed");

        assert!(kill(leader_pid, None).is_err(), "the leader must be reaped");
        assert!(
            kill(descendant_pid, None).is_err(),
            "the group descendant must be reaped too"
        );
        assert!(
            killpg(pgrp, None).is_err(),
            "the whole owned group must be quiescent"
        );
    }

    /// A leader that already exited but is not reaped yet (zombie) is
    /// confirmed cleanup, never "signal refused" — the CI-observed macOS
    /// launch-failure misclassification route: `id()` is still `Some` (tokio
    /// reports `None` only after reaping) and the birth-token check cannot
    /// prove the leader's identity. The exit-state barrier polls the kernel's
    /// zombie state (`ps -o state=` leading `Z` — exactly when the exit
    /// status is waitable for the parent) with a bounded interval between
    /// checks; the timeout turns a wrong assumption into a failed test
    /// instead of a hang. sysinfo cannot be used for this on macOS: its
    /// backend drops zombies entirely (`get_bsd_info` fails, so the process
    /// is removed from the map), which is the same platform quirk the
    /// birth-token check trips over.
    ///
    /// The token is then overwritten with an impossible start tick so the
    /// identity check fails on every platform (Linux can still read a
    /// zombie's start time, macOS cannot): the pre-fix signalling path must
    /// refuse — and the fixed reap-on-sight path must confirm — on both.
    #[cfg(unix)]
    #[tokio::test]
    async fn terminate_confirms_leader_already_exited_at_teardown() {
        let spawner = AgentSpawner::new(PathBuf::from("/tmp"));
        let (child, _stdin, _stdout) = spawner
            .spawn("/bin/sleep", &["300"])
            .expect("spawn grouped sleep fixture");
        let leader = child.id().expect("owned child pid");
        let leader_pid = nix::unistd::Pid::from_raw(leader.cast_signed());

        let mut proc = ManagedAcpProcess::new(
            "zombie-fixture".to_string(),
            child,
            PathBuf::from("/bin/sleep"),
        );
        assert!(proc.pid().is_some(), "leader not yet reaped at spawn");

        nix::sys::signal::kill(leader_pid, nix::sys::signal::Signal::SIGKILL)
            .expect("SIGKILL the owned child");

        // Exact exit-state barrier: state Z means the kernel has made the
        // exit status waitable (zombie) while nobody has reaped it, so tokio
        // still owns the zombie below. The bound turns a wrong assumption
        // into a failed test instead of a hang; the interval keeps the poll
        // cheap without papering over the state check.
        await_zombie_state(leader).await;

        assert!(
            proc.pid().is_some(),
            "the leader is dead but tokio has not reaped it yet"
        );
        assert!(
            proc.birth().is_some(),
            "the spawn-time birth token must have been captured"
        );
        // Deliberate: identity can no longer be proven, on every platform, so
        // the pre-fix signalling path must refuse regardless of whether the
        // OS still reports a zombie's start time (Linux can, macOS cannot).
        proc.birth = Some(ProcessBirthToken {
            pid: leader,
            start_tick: u64::MAX,
        });
        assert!(
            !proc.birth.as_ref().expect("token").verify(),
            "precondition: the birth token must be unverifiable"
        );

        proc.terminate(Duration::from_secs(5))
            .await
            .expect("teardown of an already-exited leader must be confirmed cleanup");
    }

    /// Fixture-owned cleanup for a spawned owned group: `killpg(SIGKILL)`
    /// reaches the leader and every same-group descendant, and fires on every
    /// exit path (including panics during fixture setup) so no fixture
    /// process outlives the test — independent of the production call under
    /// test. A no-op once the group is gone.
    struct KillGroupOnDrop(nix::unistd::Pid);

    impl Drop for KillGroupOnDrop {
        fn drop(&mut self) {
            let _ = nix::sys::signal::killpg(self.0, nix::sys::signal::Signal::SIGKILL);
        }
    }

    /// Reads the `sh -c 'sleep 300 & echo $!; …'` fixture's descendant pid:
    /// shell sequencing writes it before the leader exits, so the read needs
    /// no polling.
    async fn read_descendant_pid(stdout: tokio::process::ChildStdout) -> u32 {
        use tokio::io::AsyncBufReadExt;

        tokio::time::timeout(Duration::from_secs(5), async {
            let mut lines = tokio::io::BufReader::new(stdout).lines();
            lines
                .next_line()
                .await
                .expect("descendant pid line readable")
                .expect("descendant pid line present")
                .trim()
                .parse()
                .expect("descendant pid is numeric")
        })
        .await
        .expect("descendant pid line within bound")
    }

    /// Reads the SIGTERM-ignoring fixture's `(pid, trap-ready)` pair. The
    /// descendant publishes the `ready` line only AFTER `trap "" TERM` made
    /// SIGTERM ignorable — the disposition is inherited by the exec'd
    /// `sleep`, so seeing `ready` proves the signal was trap-ready before
    /// any teardown signal.
    #[cfg(target_os = "linux")]
    async fn read_descendant_readiness(stdout: tokio::process::ChildStdout) -> u32 {
        use tokio::io::AsyncBufReadExt;

        tokio::time::timeout(Duration::from_secs(5), async {
            let mut lines = tokio::io::BufReader::new(stdout).lines();
            let mut pid = None;
            let mut ready = false;
            while !(pid.is_some() && ready) {
                let line = lines
                    .next_line()
                    .await
                    .expect("fixture line readable")
                    .expect("fixture line present");
                let trimmed = line.trim();
                if let Ok(value) = trimmed.parse::<u32>() {
                    pid = Some(value);
                } else if trimmed == "ready" {
                    ready = true;
                }
            }
            pid.expect("descendant pid line")
        })
        .await
        .expect("fixture readiness lines within bound")
    }

    /// A leader that exits while a same-group descendant stays alive: while
    /// identity is provable (Linux reads a zombie's start time), the guarded
    /// SIGTERM must reach the survivor and the teardown must be confirmed
    /// with the leader reaped. Reap-before-signal would destroy exactly that
    /// provable identity and leak the descendant — the A5 outcome this
    /// module prevents.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn terminate_signals_surviving_descendant_while_identity_is_provable() {
        let spawner = AgentSpawner::new(PathBuf::from("/tmp"));
        let (child, _stdin, stdout) = spawner
            .spawn("/bin/sh", &["-c", "sleep 300 & echo $!; exit 0"])
            .expect("spawn grouped fixture");
        let leader = child.id().expect("leader pid");
        let leader_pid = nix::unistd::Pid::from_raw(leader.cast_signed());
        // Fixture-owned cleanup before any fallible step: the guard kills the
        // whole owned group (leader + descendant) on every exit path.
        let _guard = KillGroupOnDrop(leader_pid);
        let descendant = read_descendant_pid(stdout).await;
        let descendant_pid = nix::unistd::Pid::from_raw(descendant.cast_signed());
        assert_ne!(descendant, leader, "the descendant is not the leader");

        let proc =
            ManagedAcpProcess::new("group-fixture".to_string(), child, PathBuf::from("/bin/sh"));
        await_zombie_state(leader).await;
        assert!(
            nix::sys::signal::kill(descendant_pid, None).is_ok(),
            "the group descendant must still be alive before teardown"
        );

        proc.terminate(Duration::from_secs(5))
            .await
            .expect("provable identity must signal the survivor and confirm cleanup");

        assert!(
            nix::sys::signal::kill(leader_pid, None).is_err(),
            "the leader must be reaped"
        );
        assert!(
            nix::sys::signal::kill(descendant_pid, None).is_err(),
            "the group survivor must be reaped too"
        );
    }

    /// A descendant that ignores SIGTERM must still be killed: the SIGKILL
    /// escalation re-verifies the birth token, which only works when the
    /// leader's reap is deferred until after the group is proven gone
    /// (reap-before-escalation would invalidate the token and leak the
    /// survivor). The fixture publishes readiness only after `trap "" TERM`
    /// took effect, so the test never signals a descendant whose default
    /// TERM disposition could still win the race. Linux-only: it needs a
    /// zombie leader whose start time the identity check can still read.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn terminate_escalates_to_sigkill_when_descendant_ignores_sigterm() {
        let spawner = AgentSpawner::new(PathBuf::from("/tmp"));
        let (child, _stdin, stdout) = spawner
            .spawn(
                "/bin/sh",
                &[
                    "-c",
                    "(trap \"\" TERM; echo ready; exec sleep 300) & echo $!; exit 0",
                ],
            )
            .expect("spawn grouped fixture");
        let leader = child.id().expect("leader pid");
        let leader_pid = nix::unistd::Pid::from_raw(leader.cast_signed());
        // Fixture-owned cleanup before any fallible step.
        let _guard = KillGroupOnDrop(leader_pid);
        let descendant = read_descendant_readiness(stdout).await;
        let descendant_pid = nix::unistd::Pid::from_raw(descendant.cast_signed());

        let proc =
            ManagedAcpProcess::new("group-fixture".to_string(), child, PathBuf::from("/bin/sh"));
        await_zombie_state(leader).await;
        assert!(
            nix::sys::signal::kill(descendant_pid, None).is_ok(),
            "the SIGTERM-ignoring descendant must be alive before teardown"
        );

        // grace bounds both the SIGTERM quiescence observation (the trap
        // ignores the signal, so this burns the full window) and the SIGKILL
        // wait.
        proc.terminate(Duration::from_secs(1))
            .await
            .expect("SIGKILL escalation must reap a SIGTERM-ignoring group");

        // Bounded disappearance: the escalation must have reaped the leader
        // AND SIGKILLed the trap-protected descendant.
        assert!(
            ManagedAcpProcess::wait_group_quiescent(leader_pid, Duration::from_secs(5)).await,
            "the whole owned group must be gone after the SIGKILL escalation"
        );
        assert!(
            nix::sys::signal::kill(leader_pid, None).is_err(),
            "the leader must be reaped"
        );
        assert!(
            nix::sys::signal::kill(descendant_pid, None).is_err(),
            "the SIGTERM-ignoring survivor must be SIGKILLed"
        );
    }

    /// An identity that cannot be proven must never be signalled: with the
    /// token invalidated, teardown refuses — the group survivor is untouched
    /// and the cleanup is reported as an unconfirmed liveness failure (the
    /// typed cross-platform contract, not a message match).
    #[cfg(unix)]
    #[tokio::test]
    async fn terminate_refuses_to_signal_survivors_when_identity_unprovable() {
        let spawner = AgentSpawner::new(PathBuf::from("/tmp"));
        let (child, _stdin, stdout) = spawner
            .spawn("/bin/sh", &["-c", "sleep 300 & echo $!; exit 0"])
            .expect("spawn grouped fixture");
        let leader = child.id().expect("leader pid");
        let leader_pid = nix::unistd::Pid::from_raw(leader.cast_signed());
        // Fixture-owned cleanup before any fallible step.
        let _guard = KillGroupOnDrop(leader_pid);
        let descendant = read_descendant_pid(stdout).await;
        let descendant_pid = nix::unistd::Pid::from_raw(descendant.cast_signed());

        let mut proc =
            ManagedAcpProcess::new("group-fixture".to_string(), child, PathBuf::from("/bin/sh"));
        await_zombie_state(leader).await;
        assert!(
            nix::sys::signal::kill(descendant_pid, None).is_ok(),
            "the group descendant must still be alive before teardown"
        );
        proc.birth = Some(ProcessBirthToken {
            pid: leader,
            start_tick: u64::MAX,
        });

        let err = proc
            .terminate(Duration::from_secs(5))
            .await
            .expect_err("an unprovable identity must refuse to signal");
        assert!(
            matches!(err, AcpError::CleanupUnconfirmed { .. }),
            "the refusal must be typed as a liveness failure: {err}"
        );
        assert!(
            err.to_string().contains("signal refused"),
            "the refusal must say the signal was refused: {err}"
        );
        assert!(
            nix::sys::signal::kill(descendant_pid, None).is_ok(),
            "the survivor must be untouched by a refused signal"
        );

        // Normal-path cleanup: bounded disappearance instead of a
        // fire-and-forget signal (the guard above covers failure paths).
        let _ = nix::sys::signal::kill(descendant_pid, nix::sys::signal::Signal::SIGKILL);
        let gone = tokio::time::timeout(Duration::from_secs(5), async {
            while nix::sys::signal::kill(descendant_pid, None).is_ok() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(
            gone.is_ok(),
            "fixture descendant {descendant} must disappear after the cleanup SIGKILL"
        );
    }

    /// A live leader with an unprovable identity must not hang the cleanup
    /// deadline: terminate(grace) returns the typed refusal within the bound,
    /// and never drops the `kill_on_drop` child into an implicit kill (the
    /// fixture leader is left running and killed explicitly afterwards).
    #[cfg(unix)]
    #[tokio::test]
    async fn terminate_returns_within_bound_when_identity_unprovable_and_leader_alive() {
        let spawner = AgentSpawner::new(PathBuf::from("/tmp"));
        let (child, _stdin, _stdout) = spawner
            .spawn("/bin/sleep", &["300"])
            .expect("spawn live sleep fixture");
        let leader = child.id().expect("leader pid");
        let leader_pid = nix::unistd::Pid::from_raw(leader.cast_signed());

        let mut proc = ManagedAcpProcess::new(
            "live-fixture".to_string(),
            child,
            PathBuf::from("/bin/sleep"),
        );
        // Fixture-owned cleanup before any fallible step: the forgotten
        // handle after the refusal cannot be dropped into an implicit kill,
        // so this guard is what kills the leader on every failure path.
        let _guard = KillGroupOnDrop(leader_pid);
        proc.birth = Some(ProcessBirthToken {
            pid: leader,
            start_tick: u64::MAX,
        });

        let grace = Duration::from_secs(1);
        let started = std::time::Instant::now();
        let err = proc
            .terminate(grace)
            .await
            .expect_err("an unprovable identity must refuse to signal");
        let elapsed = started.elapsed();
        assert!(
            matches!(err, AcpError::CleanupUnconfirmed { .. }),
            "the refusal must be typed as a liveness failure: {err}"
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "terminate must return within the cleanup deadline, took {elapsed:?}"
        );
        assert!(
            nix::sys::signal::kill(leader_pid, None).is_ok(),
            "the refused teardown must not have killed the leader"
        );

        // The handle was deliberately leaked (no implicit kill on drop):
        // kill the still-running fixture explicitly and reap it so nothing
        // outlives the test.
        nix::sys::signal::kill(leader_pid, nix::sys::signal::Signal::SIGKILL)
            .expect("SIGKILL the leaked live fixture");
        nix::sys::wait::waitpid(leader_pid, None).expect("reap the explicit kill");
    }

    /// Waits until `pid` is in the kernel's zombie state (exit status
    /// waitable, not yet reaped) or fails the test: a probe that cannot
    /// observe the state is an environment problem with an explicit reason,
    /// and reaching the bound means the assumed transition never happened.
    async fn await_zombie_state(pid: u32) {
        let barrier = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match leader_state_is_zombie(pid) {
                    Ok(true) => return,
                    Ok(false) => tokio::time::sleep(Duration::from_millis(10)).await,
                    Err(reason) => panic!("zombie-state probe unavailable: {reason}"),
                }
            }
        })
        .await;
        assert!(
            barrier.is_ok(),
            "leader must reach the unreaped zombie state within the bound"
        );
    }

    /// `Ok(true)` while `pid` is in the kernel's zombie state (exit status
    /// waitable, not yet reaped); `Ok(false)` while alive, dying, or gone.
    /// `Err` names a probe that cannot observe the state at all — an
    /// environment problem, never a state claim.
    #[cfg(target_os = "linux")]
    fn leader_state_is_zombie(pid: u32) -> Result<bool, String> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map_err(|e| format!("/proc/{pid}/stat unreadable: {e}"))?;
        let state = stat
            .rsplit(')')
            .next()
            .and_then(|rest| rest.trim_start().chars().next())
            .ok_or_else(|| format!("/proc/{pid}/stat is empty"))?;
        Ok(state == 'Z')
    }

    #[cfg(not(target_os = "linux"))]
    fn leader_state_is_zombie(pid: u32) -> Result<bool, String> {
        std::process::Command::new("ps")
            .args(["-o", "state=", "-p", &pid.to_string()])
            .output()
            .map_err(|e| format!("`ps` unavailable: {e}"))
            .and_then(|output| {
                if output.status.success() {
                    Ok(String::from_utf8_lossy(&output.stdout)
                        .trim()
                        .starts_with('Z'))
                } else {
                    Err(format!("`ps -o state= -p {pid}` failed: {}", output.status))
                }
            })
    }
}
