//! Codex CLI native provider adapter.
//!
//! Implements a native CLI provider for `OpenAI`'s `codex` command-line tool.
//! Codex is **subcommand-based** (`codex exec`), unlike Claude's flag-based
//! (`claude --print`) interface. The adapter parses the `--json` JSONL event
//! stream and captures codex-generated session IDs for multi-turn resume.
//!
//! # Session Model
//!
//! Each `launch()` registers a host session with `codex_session_id: None`.
//! The first `execute()` spawns `codex exec --json -s read-only` and writes the
//! prompt to stdin. While reading the JSONL stream, the adapter looks for a
//! `session_start` event and stores its `session_id` in the native session.
//! Subsequent `execute()` calls use `codex exec resume <id> --json` to continue
//! the same codex session across separate process invocations.
//!
//! # Fallback
//!
//! If codex `--json` output is not structured (e.g., older versions, custom builds,
//! or incompatible event shapes), the adapter falls back to plain stdout line
//! streaming: unparsable lines are emitted as [`HostEvent::MessageDelta`] text
//! and the session ID is never captured, so future invocations spawn fresh
//! rather than attempting `resume`.
//!
//! This adapter only supports **per-invocation mode**. Codex `exec` exits after
//! one prompt; there is no persistent child reuse like Claude's delimited mode.

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

/// Internal state for a managed codex native CLI session.
struct NativeSession {
    /// The codex-generated session ID used for `exec resume <id>`.
    /// Captured from the first `session_start` JSONL event.
    codex_session_id: Option<String>,
    /// Working directory for the CLI process, retained from `LaunchSpec::cwd`.
    cwd: std::path::PathBuf,
    /// Whether the first `execute()` has been performed for this session.
    first_exec_done: bool,
    /// Whether the first `execute()` successfully captured a codex session ID.
    /// If false after the first execute, future invocations drop `--json`.
    json_capable: bool,
    /// Child processes for in-flight operations, keyed by host operation ID.
    /// Tracked so `cancel()` and `shutdown()` can kill running children.
    operation_children: HashMap<HostOperationId, tokio::process::Child>,
}

impl std::fmt::Debug for NativeSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeSession")
            .field("codex_session_id", &self.codex_session_id.as_deref())
            .field("cwd", &self.cwd.display())
            .field("first_exec_done", &self.first_exec_done)
            .field("json_capable", &self.json_capable)
            .field("operation_children", &self.operation_children.keys())
            .finish()
    }
}

/// Codex CLI native provider.
///
/// Spawns `codex` (or a configured command) as a subprocess and normalizes its
/// stdout into `HostEvent` items. Supports per-invocation mode only: each
/// `execute()` spawns a new child, with multi-turn continuity via codex's
/// `exec resume <id>` subcommand.
pub struct CodexNativeProvider {
    /// Provider ID (typically `codex-native` to avoid collision with ACP registry).
    provider_id: ProviderId,
    /// Display name.
    display_name: String,
    /// Command to execute (e.g., `codex`).
    command: String,
    /// Default arguments for non-interactive JSONL mode.
    args: Vec<String>,
    /// Environment variables to inject.
    env: HashMap<String, String>,
    /// Active sessions: host session ID → native session state.
    sessions: Arc<RwLock<HashMap<HostSessionId, NativeSession>>>,
    /// Timeout configuration for stage-level enforcement.
    timeouts: TimeoutConfig,
}

impl CodexNativeProvider {
    /// Create a new Codex CLI provider with the given configuration.
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
        }
    }

    /// Create with default configuration for the `codex` command.
    #[must_use]
    pub fn default_config() -> Self {
        Self::new(
            ProviderId::new("codex-native"),
            "Codex CLI (native)".to_string(),
            "codex".to_string(),
            vec![
                "exec".to_string(),
                "--json".to_string(),
                "-s".to_string(),
                "read-only".to_string(),
            ],
            HashMap::new(),
            TimeoutConfig::default(),
        )
    }

    /// Build the event stream from stdout.
    ///
    /// In `--json` mode, parses each line as a JSONL event. If a line is not
    /// valid JSON, it is emitted as a plain-text `MessageDelta` (fallback).
    /// Unknown JSON event types are logged and skipped.
    ///
    /// Emits `OpStarted`, then `MessageDelta`/`ThoughtDelta` per event, and a
    /// terminal `OpFinished`/`OpFailed` when stdout reaches EOF or an I/O error
    /// occurs.
    #[allow(clippy::too_many_lines)]
    fn build_event_stream(
        &self,
        stdout: Option<tokio::process::ChildStdout>,
        op_id: HostOperationId,
        session_id: HostSessionId,
        read_timeout: std::time::Duration,
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
            let sessions = Arc::clone(&self.sessions);
            let provider_id = self.provider_id.clone();
            let reaper = ChildReaper {
                sessions: Arc::clone(&self.sessions),
                session_id: session_id.clone(),
                op_id: op_id.clone(),
            };
            let stdout_reader = tokio::io::BufReader::new(stdout);
            futures_util::stream::unfold(
                (stdout_reader, op_id, session_id, false, reaper),
                move |(mut stdout_reader, op_id, session_id, finished, reaper)| {
                    let sessions = Arc::clone(&sessions);
                    let provider_id = provider_id.clone();
                    async move {
                        if finished {
                            return None;
                        }

                        loop {
                            let mut line = String::new();
                            match tokio::time::timeout(
                                read_timeout,
                                stdout_reader.read_line(&mut line),
                            )
                            .await
                            {
                                Ok(Ok(0)) => {
                                    // EOF — emit terminal event and mark finished.
                                    return Some((
                                        Ok(HostEvent::OpFinished(OperationFinishedEvent {
                                            session_id,
                                            op_id: op_id.clone(),
                                            reason: FinishReason::EndTurn,
                                        })),
                                        (stdout_reader, op_id, HostSessionId::new(), true, reaper),
                                    ));
                                }
                                Ok(Ok(_)) => {
                                    let trimmed = line
                                        .trim_end_matches('\n')
                                        .trim_end_matches('\r')
                                        .to_string();
                                    if let Some(event) = parse_codex_jsonl_line(
                                        &trimmed,
                                        &session_id,
                                        &op_id,
                                        &sessions,
                                        &provider_id,
                                    )
                                    .await
                                    {
                                        return Some((
                                            Ok(event),
                                            (stdout_reader, op_id, session_id, false, reaper),
                                        ));
                                    }
                                    // Event was skipped (e.g., session_start
                                    // internal update) — continue reading.
                                }
                                Ok(Err(e)) => {
                                    return Some((
                                        Ok(HostEvent::OpFailed(OperationFailedEvent {
                                            session_id,
                                            op_id: op_id.clone(),
                                            error_category: "io_error".to_string(),
                                            error_message: e.to_string(),
                                        })),
                                        (stdout_reader, op_id, HostSessionId::new(), true, reaper),
                                    ));
                                }
                                Err(_) => {
                                    reaper.kill().await;
                                    return Some((
                                        Ok(HostEvent::OpFailed(OperationFailedEvent {
                                            session_id,
                                            op_id: op_id.clone(),
                                            error_category: "timeout".to_string(),
                                            error_message: format!(
                                                "codex stream read timed out after {read_timeout:?}"
                                            ),
                                        })),
                                        (stdout_reader, op_id, HostSessionId::new(), true, reaper),
                                    ));
                                }
                            }
                        }
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

    /// Spawn the CLI subprocess and write the prompt to stdin.
    ///
    /// Returns `(stdout, stderr, child)` ready for event stream construction.
    async fn spawn_and_write_stdin(
        &self,
        full_args: &[String],
        prompt_text: &str,
        cwd: &std::path::Path,
    ) -> HostResult<(
        Option<tokio::process::ChildStdout>,
        Option<tokio::process::ChildStderr>,
        tokio::process::Child,
    )> {
        let mut cmd = tokio::process::Command::new(&self.command);
        cmd.args(full_args)
            .current_dir(cwd)
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

        // Write prompt to stdin with a timeout and close it. If the write fails
        // or times out, reap the child before returning so it cannot outlive host
        // management.
        let prompt_dur = self.timeouts.prompt_duration();
        let stdin = child.stdin.take();
        if let Some(mut stdin) = stdin {
            let write_result =
                tokio::time::timeout(prompt_dur, stdin.write_all(prompt_text.as_bytes())).await;

            match write_result {
                Ok(Ok(())) => {
                    drop(stdin);
                }
                Ok(Err(e)) => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    return Err(HostError::protocol_error(
                        "failed to write prompt to stdin",
                        Some(e.to_string()),
                    ));
                }
                Err(_) => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    return Err(HostError::timeout(
                        "prompt",
                        format!(
                            "CLI process setup timed out after {}ms",
                            self.timeouts.prompt_ms
                        ),
                    ));
                }
            }
        }

        Ok((child.stdout.take(), child.stderr.take(), child))
    }
}

/// RAII guard that reaps a child process when the event stream ends.
///
/// Per-invocation codex children run until EOF. Once the stream consumer
/// reaches EOF (or drops the stream), this guard removes the child handle from
/// the session and waits on it, preventing zombies.
struct ChildReaper {
    sessions: Arc<RwLock<HashMap<HostSessionId, NativeSession>>>,
    session_id: HostSessionId,
    op_id: HostOperationId,
}

impl Drop for ChildReaper {
    fn drop(&mut self) {
        let sessions = Arc::clone(&self.sessions);
        let session_id = self.session_id.clone();
        let op_id = self.op_id.clone();
        tokio::spawn(async move {
            let mut sessions = sessions.write().await;
            if let Some(ns) = sessions.get_mut(&session_id) {
                if let Some(mut child) = ns.operation_children.remove(&op_id) {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                }
            }
        });
    }
}

impl ChildReaper {
    /// Kill the child process now (used for stream timeout).
    async fn kill(&self) {
        let mut sessions = self.sessions.write().await;
        if let Some(ns) = sessions.get_mut(&self.session_id) {
            if let Some(mut child) = ns.operation_children.remove(&self.op_id) {
                let _ = child.kill().await;
                let _ = child.wait().await;
            }
        }
    }
}

/// Parsed JSONL event shape from `codex --json`.
///
/// This is intentionally tolerant: only fields the adapter cares about are
/// required. Unknown fields are ignored by serde.
#[derive(Debug, serde::Deserialize)]
struct CodexJsonlEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    delta: Option<serde_json::Value>,
    #[serde(default, rename = "finish_reason")]
    finish_reason: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

/// Parse a single JSONL line from codex `--json` output.
///
/// Returns `Some(HostEvent)` when the line produces a host-visible event.
/// Returns `None` when the line is consumed internally (e.g., session ID
/// capture) and the caller should continue reading.
async fn parse_codex_jsonl_line(
    line: &str,
    session_id: &HostSessionId,
    op_id: &HostOperationId,
    sessions: &Arc<RwLock<HashMap<HostSessionId, NativeSession>>>,
    provider_id: &ProviderId,
) -> Option<HostEvent> {
    // Try to parse as JSON. If it fails, fall back to plain-text line streaming.
    let event: CodexJsonlEvent = match serde_json::from_str(line) {
        Ok(event) => event,
        Err(e) => {
            tracing::debug!(
                provider_id = %provider_id,
                line = %line,
                error = %e,
                "JSONL parse failed; falling back to plain text line"
            );
            return Some(HostEvent::MessageDelta(TextDeltaEvent {
                session_id: session_id.clone(),
                op_id: op_id.clone(),
                text: line.to_string(),
            }));
        }
    };

    // Capture codex session ID from any event that carries it.
    if let Some(codex_id) = event.session_id {
        let session_id_owned = session_id.clone();
        let sessions = Arc::clone(sessions);
        let codex_id_clone = codex_id.clone();
        let mut sessions_guard = sessions.write().await;
        if let Some(ns) = sessions_guard.get_mut(&session_id_owned) {
            ns.codex_session_id = Some(codex_id_clone);
            ns.json_capable = true;
            tracing::info!(
                session_id = %session_id_owned,
                codex_session_id = %codex_id,
                "Captured codex session ID from JSONL event"
            );
        }
    }

    match event.event_type.as_str() {
        "session_start" => {
            // Internal event used for session ID capture; do not emit a host event.
            None
        }
        "message_delta" => {
            let text = extract_delta_text(event.delta.as_ref()).unwrap_or_default();
            Some(HostEvent::MessageDelta(TextDeltaEvent {
                session_id: session_id.clone(),
                op_id: op_id.clone(),
                text,
            }))
        }
        "thought_delta" => {
            let text = extract_delta_text(event.delta.as_ref()).unwrap_or_default();
            Some(HostEvent::ThoughtDelta(TextDeltaEvent {
                session_id: session_id.clone(),
                op_id: op_id.clone(),
                text,
            }))
        }
        "tool_call" | "tool_call_update" => {
            // MVP: log and skip structured tool events; native CLI limited
            // capabilities do not claim structured tool calls.
            tracing::debug!(
                provider_id = %provider_id,
                event_type = %event.event_type,
                "Skipping structured tool event in native CLI provider"
            );
            None
        }
        "finish" => {
            let reason = parse_finish_reason(event.finish_reason.as_deref())
                .or_else(|| parse_finish_reason(event.reason.as_deref()))
                .unwrap_or(FinishReason::EndTurn);
            Some(HostEvent::OpFinished(OperationFinishedEvent {
                session_id: session_id.clone(),
                op_id: op_id.clone(),
                reason,
            }))
        }
        _ => {
            tracing::debug!(
                provider_id = %provider_id,
                event_type = %event.event_type,
                "Unknown codex JSONL event type; skipping"
            );
            None
        }
    }
}

/// Extract displayable text from a `delta` JSON value.
///
/// Handles several plausible codex shapes:
/// - `"delta": "hello"` (string)
/// - `"delta": { "content": "hello" }` (object with content)
/// - `"delta": { "text": "hello" }` (object with text)
fn extract_delta_text(delta: Option<&serde_json::Value>) -> Option<String> {
    let delta = delta?;
    match delta {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Object(obj) => obj
            .get("content")
            .or_else(|| obj.get("text"))
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string),
        _ => None,
    }
}

/// Parse a codex finish reason into the host `FinishReason` enum.
fn parse_finish_reason(reason: Option<&str>) -> Option<FinishReason> {
    let reason = reason?;
    match reason.to_lowercase().as_str() {
        "max_tokens" | "length" => Some(FinishReason::MaxTokens),
        "max_turn_requests" | "max_turns" => Some(FinishReason::MaxTurnRequests),
        "refusal" | "content_filter" => Some(FinishReason::Refusal),
        // Default: normal end-of-turn for end_turn, stop, completed, or unknown reasons.
        _ => Some(FinishReason::EndTurn),
    }
}

#[async_trait]
impl ProviderAdapter for CodexNativeProvider {
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
        // spawn_blocking to keep the async runtime responsive, and enforced
        // with launch_ms timeout.
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

        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(
                host_session_id.clone(),
                NativeSession {
                    codex_session_id: None,
                    cwd: spec.cwd.clone(),
                    first_exec_done: false,
                    json_capable: true,
                    operation_children: HashMap::new(),
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

        // Build prompt text from content blocks.
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

        // Look up session state and determine invocation flags.
        let (full_args, is_first, cwd) = {
            let mut sessions = self.sessions.write().await;
            let native_session = sessions.get_mut(&session.session_id).ok_or_else(|| {
                HostError::internal(format!(
                    "session {} not found in native CLI provider",
                    session.session_id
                ))
            })?;

            let cwd = native_session.cwd.clone();
            let is_first = !native_session.first_exec_done;

            let mut full_args = Vec::new();
            full_args.push("exec".to_string());

            if is_first {
                // First invocation: `codex exec --json -s read-only`
                full_args.extend(self.args.iter().skip(1).cloned());
            } else if native_session.json_capable {
                if let Some(ref codex_id) = native_session.codex_session_id {
                    // Subsequent invocation with captured session ID:
                    // `codex exec resume <id> --json -s read-only`
                    full_args.push("resume".to_string());
                    full_args.push(codex_id.clone());
                    full_args.extend(self.args.iter().skip(1).cloned());
                } else {
                    // JSONL mode failed to capture an ID; spawn fresh without --json.
                    native_session.json_capable = false;
                    full_args.extend(plain_text_args(&self.args));
                }
            } else {
                // Confirmed fallback to plain stdout line streaming.
                full_args.extend(plain_text_args(&self.args));
            }

            drop(sessions);
            (full_args, is_first, cwd)
        };

        // Spawn the subprocess with prompt_ms timeout for the setup phase
        // (spawn + stdin write). The timeout is enforced inside
        // spawn_and_write_stdin so a timed-out or failed write reaps the child
        // before the error is returned. The streaming phase runs until EOF.
        let spawn_result = self
            .spawn_and_write_stdin(&full_args, &prompt_text, &cwd)
            .await
            .map_err(|e| {
                e.with_provider(self.provider_id.clone())
                    .with_session(session.session_id.clone())
                    .with_op(op_id.clone())
            })?;

        let (stdout, stderr, mut child) = spawn_result;

        // Mark the first execute as done only after the process was spawned and
        // its stdin written successfully. Track the child handle so cancel()/
        // shutdown() can kill it. If the session was removed while we were
        // spawning, kill the child so it is not orphaned.
        {
            let mut sessions = self.sessions.write().await;
            if let Some(ns) = sessions.get_mut(&session.session_id) {
                ns.first_exec_done = true;
                ns.operation_children.insert(op_id.clone(), child);
            } else {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(HostError::internal(format!(
                    "session {} was removed while spawning child",
                    session.session_id
                )));
            }
        }

        let stream = self.build_event_stream(
            stdout,
            op_id.clone(),
            session.session_id.clone(),
            self.timeouts.prompt_duration(),
        );

        // Spawn a background task to drain stderr and log warnings.
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

        if is_first {
            // After the first execute, if the stream fails to capture a session ID,
            // the stream closure will leave json_capable = true and codex_session_id
            // = None. The next execute will detect that and drop --json.
            tracing::info!(
                session_id = %session.session_id,
                provider_id = %self.provider_id,
                "Codex native CLI first execute spawned"
            );
        }

        Ok(stream)
    }

    async fn cancel(
        &self,
        session: &ManagedSessionHandle,
        op_id: HostOperationId,
    ) -> HostResult<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(ns) = sessions.get_mut(&session.session_id) {
            if let Some(mut child) = ns.operation_children.remove(&op_id) {
                tracing::info!(
                    session_id = %session.session_id,
                    op_id = %op_id,
                    provider_id = %self.provider_id,
                    "Native CLI cancel: killing child process"
                );
                let _ = child.kill().await;
                let _ = child.wait().await;
            }
        }
        drop(sessions);
        tracing::info!(
            provider_id = %self.provider_id,
            "Native CLI cancel requested"
        );
        Ok(())
    }

    async fn shutdown(&self, session: ManagedSessionHandle) -> HostResult<()> {
        // Take the child handles out of the session before removing the session,
        // then kill them. This avoids dropping a live child handle when a
        // concurrent execute is between spawn and registration.
        let mut sessions = self.sessions.write().await;
        let children = if let Some(ns) = sessions.get_mut(&session.session_id) {
            std::mem::take(&mut ns.operation_children)
        } else {
            HashMap::new()
        };
        sessions.remove(&session.session_id);
        drop(sessions);

        for (op_id, mut child) in children {
            tracing::info!(
                session_id = %session.session_id,
                op_id = %op_id,
                provider_id = %self.provider_id,
                "Native CLI shutdown: killing child process"
            );
            let _ = child.kill().await;
            let _ = child.wait().await;
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

/// Build a plain-text argument list by removing the `--json` flag.
///
/// Used when JSONL mode fails so the adapter falls back to line-streaming
/// stdout like claude-native.
fn plain_text_args(json_args: &[String]) -> Vec<String> {
    json_args
        .iter()
        .skip(1)
        .filter(|arg| arg.as_str() != "--json")
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn collect_events(stream: HostEventStream) -> Vec<HostEvent> {
        let results: Vec<_> = stream.collect().await;
        results
            .into_iter()
            .map(|r| r.expect("stream item should be Ok"))
            .collect()
    }

    #[test]
    fn default_config_descriptor() {
        let provider = CodexNativeProvider::default_config();
        let desc = provider.descriptor();

        assert_eq!(desc.provider_id.0, "codex-native");
        assert_eq!(desc.protocol_kind, ProtocolKind::NativeCli);
        assert!(desc.capabilities.text_prompt);
        assert!(desc.capabilities.streaming);
        assert!(desc.capabilities.cancellation);
        assert!(!desc.capabilities.structured_tool_calls);
        assert!(
            desc.capabilities.session_restore,
            "native CLI now supports session_restore via exec resume"
        );
        assert!(!desc.capabilities.mcp_http);
    }

    #[test]
    fn default_config_has_exec_json_args() {
        let provider = CodexNativeProvider::default_config();
        assert_eq!(provider.command, "codex");
        assert_eq!(
            provider.args,
            vec![
                "exec".to_string(),
                "--json".to_string(),
                "-s".to_string(),
                "read-only".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn probe_unavailable_when_command_not_found() {
        let provider = CodexNativeProvider::new(
            ProviderId::new("nonexistent-codex-xyz"),
            "Fake".to_string(),
            "nonexistent_codex_xyz_12345".to_string(),
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
        let provider = CodexNativeProvider::new(
            ProviderId::new("my-codex"),
            "My Codex".to_string(),
            "/opt/codex/bin/codex".to_string(),
            vec!["exec".to_string(), "--json".to_string()],
            HashMap::from([("OPENAI_API_KEY".to_string(), "sk-test".to_string())]),
            TimeoutConfig::default(),
        );

        assert_eq!(provider.provider_id.0, "my-codex");
        assert_eq!(provider.command, "/opt/codex/bin/codex");
        assert_eq!(provider.args.len(), 2);
        assert_eq!(provider.env.get("OPENAI_API_KEY").unwrap(), "sk-test");
    }

    #[tokio::test]
    async fn launch_registers_session_without_session_id() {
        let provider = CodexNativeProvider::default_config();

        let handle = provider
            .launch(crate::capability::model::LaunchSpec {
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
            })
            .await
            .expect("launch should succeed");

        let sessions = provider.sessions.read().await;
        let native_session = sessions.get(&handle.session_id);
        assert!(native_session.is_some(), "session should be registered");

        let ns = native_session.unwrap();
        assert!(
            ns.codex_session_id.is_none(),
            "codex_session_id should be None initially"
        );
        assert!(
            !ns.first_exec_done,
            "first_exec_done should be false initially"
        );
        assert!(ns.json_capable, "json_capable should be true initially");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn execute_parses_jsonl_and_captures_session_id() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let script = r#"printf '%s\n' '{"type":"session_start","session_id":"sess_test_123"}' '{"type":"message_delta","delta":"hello"}' '{"type":"finish","reason":"end_turn"}'"#;
        let script_path = write_mock_codex_script(&temp_dir, script);

        let provider = CodexNativeProvider::new(
            ProviderId::new("test-codex-jsonl"),
            "Test".to_string(),
            script_path.to_string_lossy().into_owned(),
            vec![
                "exec".to_string(),
                "--json".to_string(),
                "-s".to_string(),
                "read-only".to_string(),
            ],
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

        let stream = provider
            .execute(
                &handle,
                crate::capability::model::HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "hi".to_string(),
                    }],
                },
            )
            .await
            .expect("execute");

        let events = collect_events(stream).await;

        let deltas: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                HostEvent::MessageDelta(d) => Some(d.text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(deltas, vec!["hello"]);

        let sessions = provider.sessions.read().await;
        let ns = sessions.get(&handle.session_id).expect("session exists");
        assert!(
            ns.first_exec_done,
            "first_exec_done should be true after execute"
        );
        assert_eq!(
            ns.codex_session_id.as_deref(),
            Some("sess_test_123"),
            "codex session ID should be captured from JSONL"
        );
        assert!(ns.json_capable, "json_capable should remain true");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn execute_second_call_uses_resume_subcommand() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let args_file = temp_dir.path().join("args.txt");
        let args_file_path = args_file.to_string_lossy().to_string();

        // Mock script: records argv to a file, then emits JSONL with session ID.
        let script = format!(
            r#"printf '%s\n' "$@" >> "{args_file_path}"; printf '%s\n' '{{"type":"session_start","session_id":"sess_test_123"}}' '{{"type":"message_delta","delta":"hello"}}' '{{"type":"finish","reason":"end_turn"}}'"#
        );
        let script_path = write_mock_codex_script(&temp_dir, &script);

        let provider = CodexNativeProvider::new(
            ProviderId::new("test-codex-resume"),
            "Test".to_string(),
            script_path.to_string_lossy().into_owned(),
            vec![
                "exec".to_string(),
                "--json".to_string(),
                "-s".to_string(),
                "read-only".to_string(),
            ],
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

        // First execute: captures session ID.
        let stream1 = provider
            .execute(
                &handle,
                crate::capability::model::HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "hi".to_string(),
                    }],
                },
            )
            .await
            .expect("first execute");
        let events1 = collect_events(stream1).await;

        let deltas1: Vec<String> = events1
            .iter()
            .filter_map(|e| match e {
                HostEvent::MessageDelta(d) => Some(d.text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(deltas1, vec!["hello"]);

        // Second execute: should use `resume <id>`.
        let stream2 = provider
            .execute(
                &handle,
                crate::capability::model::HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "again".to_string(),
                    }],
                },
            )
            .await
            .expect("second execute");
        let events2 = collect_events(stream2).await;

        let deltas2: Vec<String> = events2
            .iter()
            .filter_map(|e| match e {
                HostEvent::MessageDelta(d) => Some(d.text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(deltas2, vec!["hello"]);

        // Verify the second invocation recorded `resume <id>` in argv.
        let args_content = std::fs::read_to_string(&args_file).expect("read args file");
        let args_lines: Vec<&str> = args_content.lines().collect();

        // First execute should have recorded the base args once.
        assert!(
            args_lines.contains(&"exec"),
            "execute should pass 'exec' subcommand; args: {args_lines:?}"
        );
        // Second execute should have recorded resume and the captured session ID.
        let resume_positions: Vec<usize> = args_lines
            .iter()
            .enumerate()
            .filter_map(|(i, s)| if *s == "resume" { Some(i) } else { None })
            .collect();
        assert!(
            !resume_positions.is_empty(),
            "second execute should pass 'resume' subcommand; args: {args_lines:?}"
        );
        let resume_pos = resume_positions[0];
        assert!(
            resume_pos + 1 < args_lines.len() && args_lines[resume_pos + 1] == "sess_test_123",
            "second execute should pass captured session ID after 'resume'; args: {args_lines:?}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn execute_fallback_to_plain_text_when_jsonl_parse_fails() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let script = r"printf 'plain text line\nanother line\n'";
        let script_path = write_mock_codex_script(&temp_dir, script);

        let provider = CodexNativeProvider::new(
            ProviderId::new("test-codex-fallback"),
            "Test".to_string(),
            script_path.to_string_lossy().into_owned(),
            vec![
                "exec".to_string(),
                "--json".to_string(),
                "-s".to_string(),
                "read-only".to_string(),
            ],
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

        let stream = provider
            .execute(
                &handle,
                crate::capability::model::HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "hi".to_string(),
                    }],
                },
            )
            .await
            .expect("execute");

        let events = collect_events(stream).await;

        let deltas: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                HostEvent::MessageDelta(d) => Some(d.text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(deltas, vec!["plain text line", "another line"]);

        let sessions = provider.sessions.read().await;
        let ns = sessions.get(&handle.session_id).expect("session exists");
        assert!(
            ns.codex_session_id.is_none(),
            "session ID should not be captured in fallback mode"
        );
    }

    /// Helper to create an executable mock codex script in a temp directory.
    #[cfg(unix)]
    fn write_mock_codex_script(temp_dir: &tempfile::TempDir, body: &str) -> std::path::PathBuf {
        let script_path = temp_dir.path().join("mock_codex.sh");
        let script = format!("#!/bin/sh\n{body}\n");
        std::fs::write(&script_path, &script).expect("write script");
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms).unwrap();
        script_path
    }

    #[tokio::test]
    async fn shutdown_removes_session() {
        let provider = CodexNativeProvider::default_config();

        let handle = provider
            .launch(crate::capability::model::LaunchSpec {
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
            })
            .await
            .expect("launch");

        provider.shutdown(handle).await.expect("shutdown");

        let sessions = provider.sessions.read().await;
        assert!(
            sessions.is_empty(),
            "session should be removed after shutdown"
        );
    }

    #[test]
    fn parse_finish_reason_mapping() {
        assert_eq!(
            parse_finish_reason(Some("end_turn")),
            Some(FinishReason::EndTurn)
        );
        assert_eq!(
            parse_finish_reason(Some("max_tokens")),
            Some(FinishReason::MaxTokens)
        );
        assert_eq!(
            parse_finish_reason(Some("max_turn_requests")),
            Some(FinishReason::MaxTurnRequests)
        );
        assert_eq!(
            parse_finish_reason(Some("refusal")),
            Some(FinishReason::Refusal)
        );
        assert_eq!(
            parse_finish_reason(Some("unknown")),
            Some(FinishReason::EndTurn)
        );
        assert_eq!(parse_finish_reason(None), None);
    }

    #[test]
    fn extract_delta_text_handles_variants() {
        let string_delta = serde_json::json!("hello");
        assert_eq!(
            extract_delta_text(Some(&string_delta)),
            Some("hello".to_string())
        );

        let content_delta = serde_json::json!({"content": "world"});
        assert_eq!(
            extract_delta_text(Some(&content_delta)),
            Some("world".to_string())
        );

        let text_delta = serde_json::json!({"text": "foo"});
        assert_eq!(
            extract_delta_text(Some(&text_delta)),
            Some("foo".to_string())
        );
    }
}
