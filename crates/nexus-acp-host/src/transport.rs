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
                        self.agent_path,
                        Some(e.to_string()),
                    ));
                }
            }
        }

        #[cfg(windows)]
        {
            // On Windows, directly kill the process
            self.child
                .kill()
                .await
                .map_err(|e| AcpError::agent_crashed(None, self.agent_path, Some(e.to_string())))?;

            let status =
                self.child.wait().await.map_err(|e| {
                    AcpError::agent_crashed(None, self.agent_path, Some(e.to_string()))
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
            assert!(!p.as_str().is_empty());
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
    async fn spawn_fails_for_missing_executable() {
        let spawner = AgentSpawner::new(PathBuf::from("/tmp"));

        let result = spawner.spawn("nonexistent-program", &[]);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(err, AcpError::ExecutableNotFound { .. }));
        assert!(err.to_string().contains("not found on PATH"));
    }
}
