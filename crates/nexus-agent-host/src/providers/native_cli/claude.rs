//! Claude Code CLI native provider adapter.
//!
//! Manages a `claude` subprocess using `tokio::process::Command`.
//! Multi-turn session continuity via `--session-id` and `--resume` flags
//! for per-invocation mode, or persistent child process reuse for
//! persistent mode.
//!
//! # Session Model
//!
//! Each `launch()` generates a host-side UUID (the Claude CLI session ID).
//!
//! **Per-invocation mode** (default, `persistent = false`):
//! The first `execute()` invocation passes `--session-id <uuid>` to the CLI.
//! Subsequent invocations pass `--resume <uuid>`, providing conversation
//! continuity across separate process spawns. Each execute spawns a new child.
//!
//! **Persistent mode** (`persistent = true`):
//! A single child process is spawned on the first `execute()` and reused
//! across subsequent calls. The child stays alive, reading prompts from
//! stdin and writing responses to stdout delimited by empty lines.
//! Useful for testing and for CLI tools that support interactive mode.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::sync::RwLock;

use crate::capability::model::{
    CapabilityDescriptor, FinishReason, HostContentBlock, HostEvent, HostEventStream,
    ManagedSessionHandle, OperationFailedEvent, OperationFinishedEvent, OperationStartedEvent,
    ProtocolKind, ProviderDescriptor, ProviderHealth, TextDeltaEvent,
};
use crate::config::TimeoutConfig;
use crate::error::{HostError, HostResult};
use crate::ids::{HostOperationId, HostSessionId, ProviderId};
use crate::ProviderAdapter;

/// Persistent I/O handles for a child process that stays alive across execute calls.
///
/// stdin and stdout are wrapped in `Arc<Mutex<>>` to allow shared access
/// between the session state and the event stream.
struct PersistentHandles {
    /// The child process. Used for liveness checks and forced termination.
    child: tokio::process::Child,
    /// stdin pipe — write prompts here, flush but do not close.
    stdin: Arc<tokio::sync::Mutex<tokio::process::ChildStdin>>,
    /// stdout pipe — read responses here until empty-line delimiter.
    stdout: Arc<tokio::sync::Mutex<tokio::io::BufReader<tokio::process::ChildStdout>>>,
    /// OS process ID for diagnostics.
    pid: u32,
}

impl std::fmt::Debug for PersistentHandles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PersistentHandles")
            .field("pid", &self.pid)
            .finish_non_exhaustive()
    }
}

impl PersistentHandles {
    /// Check whether the child process is still alive (has not exited).
    fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Kill the child process forcefully.
    async fn kill(&mut self) {
        let _ = self.child.kill().await;
    }

    /// Wait for the child to exit with a timeout.
    async fn wait_with_timeout(&mut self, timeout: std::time::Duration) {
        let _ = tokio::time::timeout(timeout, self.child.wait()).await;
    }
}

/// Internal state for a managed native CLI session.
struct NativeSession {
    /// The Claude CLI session ID (UUID) used for `--session-id` / `--resume`.
    /// Set at `launch()` time; used by `execute()` to pass the correct flags.
    claude_session_id: String,
    /// Whether the first `execute()` has been performed for this session.
    /// `false` → pass `--session-id`, `true` → pass `--resume`.
    first_exec_done: bool,
    /// Persistent child handles for reuse across execute calls.
    /// `Some` when a persistent-mode child is alive; `None` otherwise.
    persistent_handles: Option<PersistentHandles>,
}

impl std::fmt::Debug for NativeSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeSession")
            .field("claude_session_id", &self.claude_session_id)
            .field("first_exec_done", &self.first_exec_done)
            .field(
                "persistent_handles",
                &self.persistent_handles.as_ref().map(|h| h.pid),
            )
            .finish()
    }
}

/// Claude Code CLI native provider.
///
/// Spawns `claude` (or a configured command) as a subprocess and
/// normalizes its stdout/stderr into `HostEvent` items.
///
/// Two operating modes:
/// - **Per-invocation** (`persistent = false`, default): each `execute()` spawns
///   a new child process. Multi-turn continuity via `--session-id`/`--resume`.
/// - **Persistent** (`persistent = true`): first `execute()` spawns a child that
///   stays alive; subsequent `execute()` calls reuse the same process via
///   stdin/stdout with empty-line delimited responses.
pub struct ClaudeCliProvider {
    /// Provider ID (typically `claude-native` to avoid collision with ACP registry).
    provider_id: ProviderId,
    /// Display name.
    display_name: String,
    /// Command to execute (e.g., `claude`).
    command: String,
    /// Default arguments for non-interactive prompt mode.
    args: Vec<String>,
    /// Environment variables to inject.
    env: HashMap<String, String>,
    /// Active sessions: host session ID → native session state.
    sessions: Arc<RwLock<HashMap<HostSessionId, NativeSession>>>,
    /// Timeout configuration for stage-level enforcement.
    timeouts: TimeoutConfig,
    /// Whether to keep the child process alive across execute calls.
    /// When true, stdin/stdout are kept open and responses are delimited
    /// by empty lines. When false (default), each execute spawns a new child.
    persistent: bool,
    /// PIDs of spawned persistent children, tracked for Drop-time cleanup (R-011).
    /// Synchronized with `sessions` map: added on spawn, removed on shutdown kill.
    /// Uses `std::sync::Mutex` (not tokio) so `Drop` can access it synchronously.
    persistent_pids: std::sync::Mutex<Vec<u32>>,
}

impl ClaudeCliProvider {
    /// Create a new Claude CLI provider with the given configuration.
    #[must_use]
    pub fn new(
        provider_id: ProviderId,
        display_name: String,
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
        timeouts: TimeoutConfig,
    ) -> Self {
        Self {
            provider_id,
            display_name,
            command,
            args,
            env,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            timeouts,
            persistent: false,
            persistent_pids: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Create a new persistent-mode Claude CLI provider.
    ///
    /// In persistent mode, a single child process is spawned on the first
    /// `execute()` and reused across subsequent calls. The child stays alive,
    /// reading prompts from stdin and writing responses to stdout delimited
    /// by empty lines.
    #[must_use]
    pub fn new_persistent(
        provider_id: ProviderId,
        display_name: String,
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
        timeouts: TimeoutConfig,
    ) -> Self {
        Self {
            provider_id,
            display_name,
            command,
            args,
            env,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            timeouts,
            persistent: true,
            persistent_pids: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Create with default configuration for the `claude` command.
    #[must_use]
    pub fn default_config() -> Self {
        Self::new(
            ProviderId::new("claude-native"),
            "Claude Code CLI (native)".to_string(),
            "claude".to_string(),
            vec!["--print".to_string()],
            HashMap::new(),
            TimeoutConfig::default(),
        )
    }

    /// Build the event stream from stdout lines.
    ///
    /// Emits `OpStarted`, then `MessageDelta` per line, and a terminal
    /// `OpFinished`/`OpFailed` when stdout reaches EOF or an I/O error occurs.
    fn build_event_stream(
        stdout: Option<tokio::process::ChildStdout>,
        op_id: HostOperationId,
        session_id: HostSessionId,
    ) -> HostEventStream {
        let started = futures_util::stream::once({
            let op_id = op_id.clone();
            let session_id = session_id.clone();
            async move {
                Ok(HostEvent::OpStarted(OperationStartedEvent {
                    op_id,
                    session_id,
                }))
            }
        });

        let stdout_stream: HostEventStream = if let Some(stdout) = stdout {
            let reader = tokio::io::BufReader::new(stdout);
            futures_util::stream::unfold(
                (reader, op_id, session_id),
                |(mut reader, op_id, session_id)| async move {
                    let mut line = String::new();
                    match reader.read_line(&mut line).await {
                        Ok(0) => {
                            // EOF — emit terminal event
                            Some((
                                Ok(HostEvent::OpFinished(OperationFinishedEvent {
                                    session_id,
                                    op_id,
                                    reason: FinishReason::EndTurn,
                                })),
                                (reader, HostOperationId::new(), HostSessionId::new()),
                            ))
                        }
                        Ok(_) => {
                            let text = line
                                .trim_end_matches('\n')
                                .trim_end_matches('\r')
                                .to_string();
                            let next_op = op_id.clone();
                            let next_session = session_id.clone();
                            Some((
                                Ok(HostEvent::MessageDelta(TextDeltaEvent {
                                    session_id,
                                    op_id,
                                    text,
                                })),
                                (reader, next_op, next_session),
                            ))
                        }
                        Err(e) => Some((
                            Ok(HostEvent::OpFailed(OperationFailedEvent {
                                session_id,
                                op_id,
                                error_category: "io_error".to_string(),
                                error_message: e.to_string(),
                            })),
                            (reader, HostOperationId::new(), HostSessionId::new()),
                        )),
                    }
                },
            )
            .boxed()
        } else {
            futures_util::stream::once(async move {
                Ok(HostEvent::OpFailed(OperationFailedEvent {
                    session_id,
                    op_id,
                    error_category: "io_error".to_string(),
                    error_message: "stdout not captured".to_string(),
                }))
            })
            .boxed()
        };

        started.chain(stdout_stream).boxed()
    }

    /// Build a delimited event stream from a shared stdout handle.
    ///
    /// Used for persistent child mode. Reads lines from stdout until an empty
    /// line (delimiter) is encountered, indicating end-of-response. Also
    /// terminates on EOF (child exited) or I/O error.
    fn build_delimited_event_stream(
        stdout: Arc<tokio::sync::Mutex<tokio::io::BufReader<tokio::process::ChildStdout>>>,
        op_id: HostOperationId,
        session_id: HostSessionId,
    ) -> HostEventStream {
        let started = futures_util::stream::once({
            let op_id = op_id.clone();
            let session_id = session_id.clone();
            async move {
                Ok(HostEvent::OpStarted(OperationStartedEvent {
                    op_id,
                    session_id,
                }))
            }
        });

        let response_stream: HostEventStream = futures_util::stream::unfold(
            (stdout, op_id, session_id, false),
            |(stdout_arc, op_id, session_id, finished)| async move {
                if finished {
                    return None;
                }

                let mut line = String::new();
                let mut guard = stdout_arc.lock().await;
                let read_result = guard.read_line(&mut line).await;
                drop(guard);

                match read_result {
                    Ok(0) => {
                        // EOF — child exited
                        Some((
                            Ok(HostEvent::OpFinished(OperationFinishedEvent {
                                session_id,
                                op_id,
                                reason: FinishReason::EndTurn,
                            })),
                            (
                                stdout_arc,
                                HostOperationId::new(),
                                HostSessionId::new(),
                                true,
                            ),
                        ))
                    }
                    Ok(_) => {
                        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
                        if trimmed.is_empty() {
                            // Empty line delimiter — end of response
                            Some((
                                Ok(HostEvent::OpFinished(OperationFinishedEvent {
                                    session_id,
                                    op_id,
                                    reason: FinishReason::EndTurn,
                                })),
                                (
                                    stdout_arc,
                                    HostOperationId::new(),
                                    HostSessionId::new(),
                                    true,
                                ),
                            ))
                        } else {
                            Some((
                                Ok(HostEvent::MessageDelta(TextDeltaEvent {
                                    session_id: session_id.clone(),
                                    op_id: op_id.clone(),
                                    text: trimmed.to_string(),
                                })),
                                (stdout_arc, op_id, session_id, false),
                            ))
                        }
                    }
                    Err(e) => Some((
                        Ok(HostEvent::OpFailed(OperationFailedEvent {
                            session_id,
                            op_id,
                            error_category: "io_error".to_string(),
                            error_message: e.to_string(),
                        })),
                        (
                            stdout_arc,
                            HostOperationId::new(),
                            HostSessionId::new(),
                            true,
                        ),
                    )),
                }
            },
        )
        .boxed();

        started.chain(response_stream).boxed()
    }

    /// Spawn the CLI subprocess and write prompt to stdin.
    ///
    /// Returns `(stdout, stderr, child)` ready for event stream construction.
    async fn spawn_and_write_stdin(
        &self,
        full_args: &[String],
        prompt_text: &str,
    ) -> HostResult<(
        Option<tokio::process::ChildStdout>,
        Option<tokio::process::ChildStderr>,
        tokio::process::Child,
    )> {
        let mut cmd = tokio::process::Command::new(&self.command);
        cmd.args(full_args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .envs(&self.env);

        let mut child = cmd.spawn().map_err(|e| {
            HostError::launch_failed(
                self.provider_id.clone(),
                format!("failed to spawn '{}'", self.command),
                Some(e.to_string()),
            )
        })?;

        // Write prompt to stdin and close it
        let stdin = child.stdin.take();
        if let Some(mut stdin) = stdin {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(prompt_text.as_bytes()).await.map_err(|e| {
                HostError::protocol_error("failed to write prompt to stdin", Some(e.to_string()))
            })?;
            drop(stdin);
        }

        Ok((child.stdout.take(), child.stderr.take(), child))
    }
}

#[async_trait]
impl ProviderAdapter for ClaudeCliProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            provider_id: self.provider_id.clone(),
            display_name: self.display_name.clone(),
            protocol_kind: ProtocolKind::NativeCli,
            capabilities: CapabilityDescriptor::native_cli_limited(),
        }
    }

    async fn probe(
        &self,
        _request: crate::capability::model::ProbeRequest,
    ) -> HostResult<ProviderHealth> {
        // Cross-platform command lookup: `which` crate handles PATH scanning
        // and Windows PATHEXT resolution automatically. Wrapped in
        // spawn_blocking to keep the async runtime responsive, and
        // enforced with launch_ms timeout.
        let command = self.command.clone();
        let provider_id = self.provider_id.clone();
        let launch_dur = self.timeouts.launch_duration();

        let result = tokio::time::timeout(
            launch_dur,
            tokio::task::spawn_blocking(move || which::which(&command)),
        )
        .await
        .map_err(|_| {
            HostError::timeout(
                "probe",
                format!(
                    "command lookup timed out after {}ms",
                    self.timeouts.launch_ms
                ),
            )
            .with_provider(self.provider_id.clone())
        })?;

        let health = match result {
            Ok(Ok(resolved_path)) => ProviderHealth {
                provider_id,
                available: true,
                latency_ms: None,
                message: Some(resolved_path.to_string_lossy().into_owned()),
            },
            _ => ProviderHealth {
                provider_id,
                available: false,
                latency_ms: None,
                message: Some(format!("command '{}' not found on PATH", self.command)),
            },
        };
        Ok(health)
    }

    async fn launch(
        &self,
        spec: crate::capability::model::LaunchSpec,
    ) -> HostResult<ManagedSessionHandle> {
        // For native CLI providers, launch() only registers session state
        // (no process spawned yet — the actual process spawns in execute()).
        let host_session_id = HostSessionId::new();

        // Generate a UUID for Claude CLI session continuity.
        // This is passed as `--session-id <uuid>` on first execute and
        // `--resume <uuid>` on subsequent ones.
        let claude_session_id = uuid::Uuid::new_v4().to_string();

        // Store session state (no process spawned yet — that happens in execute()).
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(
                host_session_id.clone(),
                NativeSession {
                    claude_session_id,
                    first_exec_done: false,
                    persistent_handles: None,
                },
            );
        }

        tracing::info!(
            session_id = %host_session_id,
            provider_id = %self.provider_id,
            cwd = %spec.cwd.display(),
            "Native CLI session registered (process spawns on first execute)"
        );

        Ok(ManagedSessionHandle {
            provider_id: self.provider_id.clone(),
            session_id: host_session_id,
            capabilities: CapabilityDescriptor::native_cli_limited(),
        })
    }

    // Multi-turn execute involves session-state lookup, CLI flag assembly,
    // process spawn, stdin write, and stdout streaming — splitting would
    // reduce clarity, so allow the line count here.
    #[allow(clippy::too_many_lines)]
    async fn execute(
        &self,
        session: &ManagedSessionHandle,
        op: crate::capability::model::HostOperation,
    ) -> HostResult<HostEventStream> {
        let crate::capability::model::HostOperation::Prompt { op_id, content } = op else {
            return Err(HostError::capability_unsupported(
                self.provider_id.clone(),
                "non-prompt operation",
                "Native CLI provider only supports Prompt operations",
            ));
        };

        // Build prompt text from content blocks
        let prompt_text: String = content
            .iter()
            .map(|block| match block {
                HostContentBlock::Text { text } => text.as_str(),
                HostContentBlock::ResourceLink { uri, .. } => uri.as_str(),
            })
            .collect::<Vec<_>>()
            .join("\n");

        if prompt_text.is_empty() {
            return Err(HostError::protocol_error(
                "empty prompt text for native CLI",
                None,
            ));
        }

        // Try persistent-mode reuse first
        if self.persistent {
            let reuse_result = {
                let mut sessions = self.sessions.write().await;
                let native_session = sessions.get_mut(&session.session_id);
                if let Some(ns) = native_session {
                    if let Some(ref mut handles) = ns.persistent_handles {
                        if handles.is_alive() {
                            // Child is alive — reuse it
                            tracing::info!(
                                session_id = %session.session_id,
                                pid = handles.pid,
                                "Reusing persistent native CLI child"
                            );

                            // Write prompt to stdin (flush, don't close)
                            {
                                let mut stdin = handles.stdin.lock().await;
                                stdin.write_all(prompt_text.as_bytes()).await.map_err(|e| {
                                    HostError::protocol_error(
                                        "failed to write prompt to persistent stdin",
                                        Some(e.to_string()),
                                    )
                                })?;
                                stdin.write_all(b"\n").await.map_err(|e| {
                                    HostError::protocol_error(
                                        "failed to write newline delimiter",
                                        Some(e.to_string()),
                                    )
                                })?;
                                stdin.flush().await.map_err(|e| {
                                    HostError::protocol_error(
                                        "failed to flush persistent stdin",
                                        Some(e.to_string()),
                                    )
                                })?;
                                drop(stdin);
                            }

                            let stream = Self::build_delimited_event_stream(
                                handles.stdout.clone(),
                                op_id.clone(),
                                session.session_id.clone(),
                            );
                            drop(sessions);
                            Some(stream)
                        } else {
                            // Child exited — clear stale handles
                            tracing::info!(
                                session_id = %session.session_id,
                                "Persistent child exited, clearing handles"
                            );
                            ns.persistent_handles = None;
                            drop(sessions);
                            None
                        }
                    } else {
                        drop(sessions);
                        None
                    }
                } else {
                    drop(sessions);
                    None
                }
            };

            if let Some(stream) = reuse_result {
                return Ok(stream);
            }

            // No persistent handles or child dead — spawn new persistent child
            return self
                .execute_spawn_persistent(session, op_id, prompt_text)
                .await;
        }

        // Per-invocation mode: always spawn new child
        self.execute_per_invocation(session, op_id, prompt_text)
            .await
    }

    async fn cancel(
        &self,
        session: &ManagedSessionHandle,
        _op_id: HostOperationId,
    ) -> HostResult<()> {
        // R-012: For persistent mode, kill the persistent child to abort the
        // in-progress operation. The next execute() will spawn a fresh child.
        // For per-invocation mode, the child exits when stdin is closed —
        // nothing to cancel.
        if self.persistent {
            let mut sessions = self.sessions.write().await;
            if let Some(ns) = sessions.get_mut(&session.session_id) {
                if let Some(mut handles) = ns.persistent_handles.take() {
                    tracing::info!(
                        session_id = %session.session_id,
                        pid = handles.pid,
                        "Cancel: killing persistent native CLI child"
                    );
                    handles.kill().await;
                    // Remove from Drop-tracker (R-011).
                    let mut pids = self.persistent_pids.lock().expect("persistent_pids lock");
                    pids.retain(|&p| p != handles.pid);
                }
            }
        }
        tracing::info!(
            provider_id = %self.provider_id,
            persistent = self.persistent,
            "Native CLI cancel requested"
        );
        Ok(())
    }

    async fn shutdown(&self, session: ManagedSessionHandle) -> HostResult<()> {
        // Kill any alive persistent child with the configured shutdown timeout.
        {
            let mut sessions = self.sessions.write().await;
            if let Some(ns) = sessions.remove(&session.session_id) {
                if let Some(mut handles) = ns.persistent_handles {
                    let shutdown_dur = self.timeouts.shutdown_duration();
                    tracing::info!(
                        session_id = %session.session_id,
                        pid = handles.pid,
                        timeout_ms = self.timeouts.shutdown_ms,
                        "Killing persistent native CLI child"
                    );
                    handles.kill().await;
                    handles.wait_with_timeout(shutdown_dur).await;
                    // Remove from Drop-tracker since we've already killed it (R-011).
                    let mut pids = self.persistent_pids.lock().expect("persistent_pids lock");
                    pids.retain(|&p| p != handles.pid);
                }
            }
        }
        tracing::info!(
            session_id = %session.session_id,
            provider_id = %self.provider_id,
            "Native CLI session shut down"
        );
        Ok(())
    }

    fn capabilities(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::native_cli_limited()
    }
}

// ---------------------------------------------------------------------------
// R-011: Drop — kill lingering persistent children when provider is dropped
// ---------------------------------------------------------------------------

impl Drop for ClaudeCliProvider {
    fn drop(&mut self) {
        let pids = self.persistent_pids.lock().expect("persistent_pids lock");
        if pids.is_empty() {
            return;
        }
        tracing::warn!(
            provider_id = %self.provider_id,
            pids = ?*pids,
            "ClaudeCliProvider dropped with {} alive persistent children — killing",
            pids.len()
        );
        // Best-effort synchronous kill using the `kill` command.
        // We don't have Child handles (those are behind tokio RwLock which
        // can't be acquired synchronously), so we use OS process kill.
        for &pid in pids.iter() {
            #[cfg(unix)]
            {
                let pid_str = pid.to_string();
                let _ = std::process::Command::new("kill")
                    .arg("-9")
                    .arg(&pid_str)
                    .status();
            }
            #[cfg(not(unix))]
            {
                tracing::warn!(
                    pid,
                    "Persistent child cleanup during Drop is Unix-only; child may be orphaned"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helper methods (not part of the ProviderAdapter trait)
// ---------------------------------------------------------------------------

impl ClaudeCliProvider {
    /// Per-invocation execute: spawns a new child process for this call.
    ///
    /// Uses `--session-id` on first call and `--resume` on subsequent calls
    /// for conversation continuity. Stdin is closed after writing the prompt.
    async fn execute_per_invocation(
        &self,
        session: &ManagedSessionHandle,
        op_id: HostOperationId,
        prompt_text: String,
    ) -> HostResult<HostEventStream> {
        // Look up session state to determine session-continuity flags
        let (claude_session_id, is_resume) = {
            let mut sessions = self.sessions.write().await;
            let native_session = sessions.get_mut(&session.session_id).ok_or_else(|| {
                HostError::internal(format!(
                    "session {} not found in native CLI provider",
                    session.session_id
                ))
            })?;

            let id = native_session.claude_session_id.clone();
            let resume = native_session.first_exec_done;
            native_session.first_exec_done = true;
            drop(sessions);
            (id, resume)
        };

        // Build command arguments: base args + session-continuity flags
        let mut full_args = self.args.clone();

        if is_resume {
            // Subsequent calls: --resume <session-id>
            full_args.push("--resume".to_string());
            full_args.push(claude_session_id.clone());
        } else {
            // First call: --session-id <uuid>
            full_args.push("--session-id".to_string());
            full_args.push(claude_session_id);
        }

        // Spawn the subprocess with prompt_ms timeout for the setup phase
        // (spawn + stdin write). The streaming phase runs until EOF.
        let prompt_dur = self.timeouts.prompt_duration();

        let spawn_result = tokio::time::timeout(
            prompt_dur,
            self.spawn_and_write_stdin(&full_args, &prompt_text),
        )
        .await
        .map_err(|_| {
            HostError::timeout(
                "prompt",
                format!(
                    "CLI process setup timed out after {}ms",
                    self.timeouts.prompt_ms
                ),
            )
            .with_provider(self.provider_id.clone())
            .with_session(session.session_id.clone())
            .with_op(op_id.clone())
        })??;

        let (stdout, stderr, mut child) = spawn_result;

        let stream = Self::build_event_stream(stdout, op_id, session.session_id.clone());

        // Spawn a background task to drain stderr and log warnings
        if let Some(stderr) = stderr {
            let provider_id = self.provider_id.clone();
            tokio::spawn(async move {
                let reader = tokio::io::BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::warn!(
                        provider_id = %provider_id,
                        stderr = %line,
                        "Native CLI stderr output"
                    );
                }
            });
        }

        // Wait for the child process in the background to prevent zombies
        tokio::spawn(async move {
            let _ = child.wait().await;
        });

        Ok(stream)
    }

    /// Persistent-mode execute: spawns a new child process and stores handles
    /// for reuse on subsequent calls. Stdin is flushed but NOT closed.
    async fn execute_spawn_persistent(
        &self,
        session: &ManagedSessionHandle,
        op_id: HostOperationId,
        prompt_text: String,
    ) -> HostResult<HostEventStream> {
        // Spawn child with piped stdio
        let mut cmd = tokio::process::Command::new(&self.command);
        cmd.args(&self.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .envs(&self.env);

        // cmd.spawn() is synchronous (returns io::Result, not a Future),
        // so we cannot wrap it in tokio::time::timeout. The spawn itself is
        // fast; the expensive part (child execution) is covered by the
        // delimited-stream reader timeout.
        let mut child = cmd.spawn().map_err(|e| {
            HostError::launch_failed(
                self.provider_id.clone(),
                format!("failed to spawn '{}'", self.command),
                Some(e.to_string()),
            )
        })?;

        let pid = child.id().unwrap_or(0);

        // Take stdin and stdout
        let raw_stdin = child.stdin.take();
        let raw_stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let (Some(raw_stdin), Some(raw_stdout)) = (raw_stdin, raw_stdout) else {
            // Can't do persistent mode without both pipes
            let _ = child.kill().await;
            return Err(HostError::launch_failed(
                self.provider_id.clone(),
                "failed to capture stdin/stdout for persistent mode",
                None,
            ));
        };

        let stdin: Arc<tokio::sync::Mutex<tokio::process::ChildStdin>> =
            Arc::new(tokio::sync::Mutex::new(raw_stdin));
        let stdout: Arc<tokio::sync::Mutex<tokio::io::BufReader<tokio::process::ChildStdout>>> =
            Arc::new(tokio::sync::Mutex::new(tokio::io::BufReader::new(
                raw_stdout,
            )));

        // Write initial prompt to stdin (flush, don't close)
        {
            let mut stdin_guard = stdin.lock().await;
            stdin_guard
                .write_all(prompt_text.as_bytes())
                .await
                .map_err(|e| {
                    HostError::protocol_error(
                        "failed to write prompt to persistent stdin",
                        Some(e.to_string()),
                    )
                })?;
            stdin_guard.write_all(b"\n").await.map_err(|e| {
                HostError::protocol_error("failed to write newline delimiter", Some(e.to_string()))
            })?;
            stdin_guard.flush().await.map_err(|e| {
                HostError::protocol_error("failed to flush persistent stdin", Some(e.to_string()))
            })?;
            drop(stdin_guard);
        }

        // Spawn a background task to drain stderr
        if let Some(stderr) = stderr {
            let provider_id = self.provider_id.clone();
            tokio::spawn(async move {
                let reader = tokio::io::BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::warn!(
                        provider_id = %provider_id,
                        stderr = %line,
                        "Native CLI stderr output (persistent)"
                    );
                }
            });
        }

        // Build the delimited event stream
        let stream =
            Self::build_delimited_event_stream(stdout.clone(), op_id, session.session_id.clone());

        // Store persistent handles in the session and track PID for Drop cleanup (R-011).
        {
            let mut sessions = self.sessions.write().await;
            if let Some(ns) = sessions.get_mut(&session.session_id) {
                ns.persistent_handles = Some(PersistentHandles {
                    child,
                    stdin,
                    stdout,
                    pid,
                });
            }
        }
        {
            let mut pids = self.persistent_pids.lock().expect("persistent_pids lock");
            pids.push(pid);
        }

        tracing::info!(
            session_id = %session.session_id,
            pid,
            "Spawned persistent native CLI child"
        );

        Ok(stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_descriptor() {
        let provider = ClaudeCliProvider::default_config();
        let desc = provider.descriptor();

        assert_eq!(desc.provider_id.0, "claude-native");
        assert_eq!(desc.protocol_kind, ProtocolKind::NativeCli);
        assert!(desc.capabilities.text_prompt);
        assert!(desc.capabilities.streaming);
        assert!(desc.capabilities.cancellation);
        assert!(!desc.capabilities.structured_tool_calls);
        assert!(
            desc.capabilities.session_restore,
            "native CLI now supports session_restore via --resume"
        );
        assert!(!desc.capabilities.mcp_http);
    }

    #[test]
    fn default_config_has_print_flag() {
        let provider = ClaudeCliProvider::default_config();
        assert_eq!(provider.command, "claude");
        assert_eq!(provider.args, vec!["--print".to_string()]);
    }

    #[tokio::test]
    async fn probe_unavailable_when_command_not_found() {
        let provider = ClaudeCliProvider::new(
            ProviderId::new("nonexistent-cli-xyz"),
            "Fake".to_string(),
            "nonexistent_cli_xyz_12345".to_string(),
            vec![],
            HashMap::new(),
            TimeoutConfig::default(),
        );

        let health = provider
            .probe(crate::capability::model::ProbeRequest { timeout_ms: 5000 })
            .await
            .expect("probe should succeed");

        assert!(!health.available);
        assert!(health.message.unwrap().contains("not found"));
    }

    #[test]
    fn custom_config_overrides() {
        let provider = ClaudeCliProvider::new(
            ProviderId::new("my-claude"),
            "My Claude".to_string(),
            "/opt/claude/bin/claude".to_string(),
            vec!["-p".to_string(), "--verbose".to_string()],
            HashMap::from([("ANTHROPIC_API_KEY".to_string(), "sk-test".to_string())]),
            TimeoutConfig::default(),
        );

        assert_eq!(provider.provider_id.0, "my-claude");
        assert_eq!(provider.command, "/opt/claude/bin/claude");
        assert_eq!(provider.args.len(), 2);
        assert_eq!(provider.env.get("ANTHROPIC_API_KEY").unwrap(), "sk-test");
    }

    #[test]
    fn native_cli_limited_descriptor_session_restore() {
        let caps = CapabilityDescriptor::native_cli_limited();
        assert!(
            caps.session_restore,
            "native_cli_limited should claim session_restore since --resume is supported"
        );
    }

    #[tokio::test]
    async fn launch_generates_session_id_and_registers_state() {
        let provider = ClaudeCliProvider::default_config();

        let handle = provider
            .launch(crate::capability::model::LaunchSpec {
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
            })
            .await
            .expect("launch should succeed");

        // Verify the session was registered
        let sessions = provider.sessions.read().await;
        let native_session = sessions.get(&handle.session_id);
        assert!(native_session.is_some(), "session should be registered");

        let ns = native_session.unwrap();
        assert!(
            !ns.claude_session_id.is_empty(),
            "claude_session_id should be set"
        );
        assert!(
            !ns.first_exec_done,
            "first_exec_done should be false initially"
        );
    }

    #[tokio::test]
    async fn execute_uses_session_id_flag_on_first_call() {
        let provider = ClaudeCliProvider::new(
            ProviderId::new("test-native"),
            "Test".to_string(),
            "echo".to_string(), // `echo` is available on all platforms
            vec!["--print".to_string()],
            HashMap::new(),
            TimeoutConfig::default(),
        );

        let handle = provider
            .launch(crate::capability::model::LaunchSpec {
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
            })
            .await
            .expect("launch");

        // First execute should succeed (echo just prints the prompt)
        let result = provider
            .execute(
                &handle,
                crate::capability::model::HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "hello".to_string(),
                    }],
                },
            )
            .await;

        assert!(result.is_ok(), "first execute should succeed");

        // Verify first_exec_done is now true
        let sessions = provider.sessions.read().await;
        let ns = sessions.get(&handle.session_id).expect("session exists");
        assert!(
            ns.first_exec_done,
            "first_exec_done should be true after first execute"
        );
    }

    #[test]
    fn acp_full_descriptor_set_model_false_set_mode_true() {
        let caps = CapabilityDescriptor::acp_full();
        assert!(
            !caps.set_model,
            "acp_full should claim set_model = false (depends on dynamic discovery)"
        );
        assert!(
            caps.set_mode,
            "acp_full should claim set_mode = true (stable RPC available)"
        );
    }

    // --- NT1.1: same PID across executes in persistent mode ---

    /// Helper: build a persistent-mode provider backed by a mock shell that
    /// echoes each stdin line as `R:<line>` followed by an empty-line delimiter.
    /// The shell stays alive across multiple prompts.
    fn persistent_mock_provider() -> ClaudeCliProvider {
        // macOS `/bin/sh` `while read` loop: stays alive, echoes with delimiter
        let mock_command = "sh";
        let mock_args = vec![
            "-c".to_string(),
            "while IFS= read -r line; do printf 'R:%s\n\n' \"$line\"; done".to_string(),
        ];
        ClaudeCliProvider::new_persistent(
            ProviderId::new("mock-persistent"),
            "Mock Persistent".to_string(),
            mock_command.to_string(),
            mock_args,
            HashMap::new(),
            TimeoutConfig::default(),
        )
    }

    /// Helper: fully consume a HostEventStream and return all unwrapped events.
    /// Collects `Result<HostEvent, HostError>` items, unwrapping Oks and
    /// failing the test on any error.
    async fn collect_events(stream: HostEventStream) -> Vec<HostEvent> {
        let results: Vec<_> = stream.collect().await;
        results
            .into_iter()
            .map(|r| r.expect("stream item should be Ok"))
            .collect()
    }

    #[tokio::test]
    async fn nt1_1_persistent_mode_same_pid_across_executes() {
        let provider = persistent_mock_provider();

        let handle = provider
            .launch(crate::capability::model::LaunchSpec {
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
            })
            .await
            .expect("launch");

        // First execute — should spawn persistent child
        let stream1 = provider
            .execute(
                &handle,
                crate::capability::model::HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "hello".to_string(),
                    }],
                },
            )
            .await
            .expect("first execute");

        let events1 = collect_events(stream1).await;
        // Should have at least Started + MessageDelta + Finished
        assert!(!events1.is_empty(), "first execute should produce events");

        // Capture PID from session state
        let pid1 = {
            let sessions = provider.sessions.read().await;
            let ns = sessions.get(&handle.session_id).expect("session exists");
            ns.persistent_handles
                .as_ref()
                .expect("should have persistent handles")
                .pid
        };

        // Second execute — should reuse same child (same PID)
        let stream2 = provider
            .execute(
                &handle,
                crate::capability::model::HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "world".to_string(),
                    }],
                },
            )
            .await
            .expect("second execute");

        let events2 = collect_events(stream2).await;
        assert!(!events2.is_empty(), "second execute should produce events");

        // Verify same PID
        let pid2 = {
            let sessions = provider.sessions.read().await;
            let ns = sessions.get(&handle.session_id).expect("session exists");
            ns.persistent_handles
                .as_ref()
                .expect("should have persistent handles after second execute")
                .pid
        };

        assert_eq!(
            pid1, pid2,
            "NT1.1: persistent mode must reuse the same PID across executes"
        );

        // Clean up
        provider
            .shutdown(handle)
            .await
            .expect("shutdown should succeed");
    }

    // --- NT1.2: shutdown kills child, no zombie ---

    #[tokio::test]
    async fn nt1_2_shutdown_kills_persistent_child() {
        let provider = persistent_mock_provider();

        let handle = provider
            .launch(crate::capability::model::LaunchSpec {
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
            })
            .await
            .expect("launch");

        // Execute to spawn the persistent child
        let stream = provider
            .execute(
                &handle,
                crate::capability::model::HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "ping".to_string(),
                    }],
                },
            )
            .await
            .expect("execute");

        // Drain the stream so the child isn't blocked on write
        collect_events(stream).await;

        // Capture PID before shutdown
        let pid_before = {
            let sessions = provider.sessions.read().await;
            let ns = sessions.get(&handle.session_id).expect("session exists");
            ns.persistent_handles
                .as_ref()
                .expect("should have persistent handles")
                .pid
        };
        assert_ne!(pid_before, 0, "PID should be nonzero");

        // Shut down — should kill the child
        provider
            .shutdown(handle)
            .await
            .expect("shutdown should succeed");

        // After shutdown, the session is removed from the map.
        // Verify the child process is no longer alive using `kill -0`.
        // Give a brief moment for the process to fully exit.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Use `kill -0 <pid>` to check if the process still exists.
        // This is safe (no signals sent) and avoids needing libc.
        let check = tokio::process::Command::new("kill")
            .arg("-0")
            .arg(pid_before.to_string())
            .output()
            .await
            .expect("kill -0 check");

        assert!(
            !check.status.success(),
            "NT1.2: shutdown must kill the persistent child — PID {pid_before} is still alive"
        );
    }

    // --- NT2.1: two executes share state (observable continuity) ---

    #[tokio::test]
    async fn nt2_1_two_executes_observable_state_continuity() {
        let provider = persistent_mock_provider();

        let handle = provider
            .launch(crate::capability::model::LaunchSpec {
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
            })
            .await
            .expect("launch");

        // First execute
        let stream1 = provider
            .execute(
                &handle,
                crate::capability::model::HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "alpha".to_string(),
                    }],
                },
            )
            .await
            .expect("first execute");

        let events1 = collect_events(stream1).await;

        // Verify first response content includes our echo
        let text1: String = events1
            .iter()
            .filter_map(|e| match e {
                HostEvent::MessageDelta(d) => Some(d.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");
        assert!(
            text1.contains("alpha"),
            "NT2.1: first response should echo 'alpha', got: {text1}"
        );

        // Second execute on same session — must reuse same PID
        let stream2 = provider
            .execute(
                &handle,
                crate::capability::model::HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "beta".to_string(),
                    }],
                },
            )
            .await
            .expect("second execute");

        let events2 = collect_events(stream2).await;

        // Verify second response content includes the second prompt
        let text2: String = events2
            .iter()
            .filter_map(|e| match e {
                HostEvent::MessageDelta(d) => Some(d.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");
        assert!(
            text2.contains("beta"),
            "NT2.1: second response should echo 'beta', got: {text2}"
        );

        // Verify persistent handles are populated with same PID
        {
            let sessions = provider.sessions.read().await;
            let ns = sessions.get(&handle.session_id).expect("session exists");
            let handles = ns
                .persistent_handles
                .as_ref()
                .expect("should have persistent handles");
            assert!(handles.pid != 0, "NT2.1: persistent PID should be nonzero");
        }

        // Clean up
        provider
            .shutdown(handle)
            .await
            .expect("shutdown should succeed");
    }
}
