//! Claude Code CLI native provider adapter (`claude-codes` stream-json client).
//!
//! Implements a native provider for the `claude` CLI through the
//! `claude-codes` async client (PD-2): per-invocation, one builder-spawned
//! child per execute, drained to the CLI's `Result` frame and killed on
//! drop. Nexus owns only the `HostEvent` normalization (`map_claude`); the
//! crate owns the wire parser, the `--print` / `--output-format
//! stream-json` flags, the `--session-id` / `--resume` argv, and the child
//! teardown.
//!
//! # Session Model
//!
//! Each `launch()` registers a host session with no client yet and generates
//! a host-side UUID (the Claude CLI session ID). The first `execute()`
//! passes it to the crate via [`claude_codes::ClaudeCliBuilder::session_id`]
//! (the crate emits `--session-id` itself); later executes use
//! [`claude_codes::ClaudeCliBuilder::resume`] (AR-5) — Nexus never assembles
//! `--session-id`/`--resume` argv. Each execute spawns a fresh child that is
//! killed when the turn's stream ends (terminal `Result` frame, stream
//! error, EOF, or cancel/shutdown). The stream has no inter-frame timeout
//! (B-3: the old rail read until EOF); a silent-but-alive child keeps the
//! turn waiting, and cancel/shutdown stay prompt by signalling the child by
//! PID when an in-flight frame read holds the per-session lock (B-2).

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use claude_codes::{AsyncClient, ClaudeCliBuilder, ClaudeInput, Error as ClaudeError};
use futures_util::StreamExt;
use tokio::sync::{Mutex, RwLock};

use crate::capability::model::{
    CapabilityDescriptor, HostContentBlock, HostEvent, HostEventStream, ManagedSessionHandle,
    OperationFailedEvent, ProtocolKind, ProviderDescriptor, ProviderHealth,
};
use crate::config::TimeoutConfig;
use crate::error::{HostError, HostResult};
use crate::ids::{HostOperationId, HostSessionId, ProviderId};
use crate::providers::native_cli::map_claude::{classify_stream_error, map_claude};
use crate::ProviderAdapter;

/// Crate-client-scoped state for a managed claude session, guarded by the
/// per-session mutex (B-2): only this session's operations contend on it —
/// cancel/shutdown of other sessions never wait on this session's frame
/// reads or setup I/O. The provider-global registry `RwLock` is only for
/// short lookups.
struct ClientState {
    /// Host-generated Claude session UUID (AR-5): the first execute passes
    /// it via `ClaudeCliBuilder::session_id`; later executes use
    /// `.resume(uuid)`. The crate emits the `--session-id`/`--resume` argv.
    claude_session_id: uuid::Uuid,
    /// Whether the first execute has been performed for this session.
    /// `false` → `.session_id(uuid)`, `true` → `.resume(uuid)`.
    first_exec_done: bool,
    /// The crate client for the in-flight turn. Dropping it kills the child
    /// (crate-owned teardown — no raw `Child` map, no PID killer).
    client: Option<AsyncClient>,
}

/// Internal state for a managed claude native session.
struct NativeSession {
    /// Per-session lock around the crate client and session metadata (B-2).
    state: Arc<Mutex<ClientState>>,
    /// PID of the spawned CLI child, for the cancel/shutdown fallback when
    /// the per-session lock is held by an in-flight frame read (the crate
    /// client is unreachable without `&mut`). 0 = no live child recorded.
    child_pid: AtomicU32,
    /// Working directory for the CLI child, retained from `LaunchSpec::cwd`
    /// (N-1).
    cwd: std::path::PathBuf,
}

impl std::fmt::Debug for NativeSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("NativeSession");
        debug
            .field("child_pid", &self.child_pid.load(Ordering::Relaxed))
            .field("cwd", &self.cwd.display());
        match self.state.try_lock() {
            Ok(state) => {
                debug
                    .field("claude_session_id", &state.claude_session_id.to_string())
                    .field("first_exec_done", &state.first_exec_done)
                    .field("client_started", &state.client.is_some());
            }
            Err(_) => {
                debug.field("client_state", &"<locked>");
            }
        }
        debug.finish()
    }
}

/// Claude Code CLI native provider.
///
/// Spawns `claude` (or a configured command) via the `claude-codes` crate
/// and normalizes its stream-json frames into `HostEvent` items. Each
/// `execute()` spawns one fresh child; multi-turn continuity is the host
/// session UUID carried by the crate's `--session-id` / `--resume` flags
/// (AR-5).
pub struct ClaudeCliProvider {
    /// Provider ID (typically `claude-native` to avoid collision with ACP registry).
    provider_id: ProviderId,
    /// Display name.
    display_name: String,
    /// Command to execute (e.g., `claude`).
    command: String,
    /// Environment variables to inject into the CLI child process.
    env: HashMap<String, String>,
    /// Active sessions: host session ID → native session state.
    sessions: Arc<RwLock<HashMap<HostSessionId, NativeSession>>>,
    /// Timeout configuration for stage-level enforcement.
    timeouts: TimeoutConfig,
}

impl ClaudeCliProvider {
    /// Create a new Claude CLI provider with the given configuration.
    #[must_use]
    pub fn new(
        provider_id: ProviderId,
        display_name: String,
        command: String,
        env: HashMap<String, String>,
        timeouts: TimeoutConfig,
    ) -> Self {
        Self {
            provider_id,
            display_name,
            command,
            env,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            timeouts,
        }
    }

    /// Create with default configuration for the `claude` command.
    #[must_use]
    pub fn default_config() -> Self {
        Self::new(
            ProviderId::new("claude-native"),
            "Claude Code CLI (native)".to_string(),
            "claude".to_string(),
            HashMap::new(),
            TimeoutConfig::default(),
        )
    }

    /// Build the event stream for an active turn.
    ///
    /// Pumps [`AsyncClient::receive`] under the per-session lock (B-2; only
    /// this session's operations contend on it) with NO inter-frame timeout
    /// (B-3: the old rail read until EOF — a turn that is silent for longer
    /// than the prompt-setup budget must keep waiting, not hard-fail) and
    /// maps every frame through the T1 [`map_claude`] mapper. Emits at most
    /// one terminal event:
    ///
    /// - The CLI `Result` frame maps to `OpFinished(EndTurn)` /
    ///   `OpFailed(provider_error)` and ends the stream.
    /// - `Err(Error::ConnectionClosed)` at EOF (or the client killed by
    ///   cancel/shutdown) before a `Result` frame → one
    ///   `OpFailed(stream_closed)` (PD-3 stream-abort backstop).
    /// - A crate stream error (`Error::Deserialization` and kin) → one
    ///   `OpFailed` from `classify_stream_error` (AR-7); the frame is lost,
    ///   so the turn fails once (PD-3 row 2).
    ///
    /// The child is killed (crate client dropped) when the turn's stream
    /// ends.
    #[allow(clippy::too_many_lines)]
    fn build_event_stream(
        &self,
        op_id: HostOperationId,
        session_id: HostSessionId,
    ) -> HostEventStream {
        let sessions = Arc::clone(&self.sessions);

        futures_util::stream::unfold(
            (sessions, op_id, session_id, VecDeque::new(), false),
            |(sessions, op_id, session_id, mut pending, finished)| async move {
                // Drain events mapped from a previous frame first: one
                // assistant frame can carry multiple content blocks.
                if let Some(event) = pending.pop_front() {
                    return Some((Ok(event), (sessions, op_id, session_id, pending, finished)));
                }
                if finished {
                    return None;
                }

                // Read frames until one maps to host events or the turn
                // ends (terminal event / stream error / EOF). The read runs
                // under the session's own lock (B-2): other sessions'
                // cancel/shutdown never wait on this frame read.
                loop {
                    let read = async {
                        let state = {
                            let guard = sessions.read().await;
                            guard.get(&session_id).map(|ns| Arc::clone(&ns.state))
                        };
                        let Some(state) = state else {
                            // Session removed by shutdown() / client killed
                            // by cancel(): EOF — the backstop below.
                            return Err(ClaudeError::ConnectionClosed);
                        };
                        let mut guard = state.lock().await;
                        match guard.client.as_mut() {
                            Some(client) => client.receive().await,
                            None => Err(ClaudeError::ConnectionClosed),
                        }
                    };
                    // B-3: no inter-frame timeout — the old rail read until
                    // EOF; a distinct frame-gap budget would reintroduce the
                    // hard-fail this removal fixes.
                    let outcome = read.await;

                    match outcome {
                        Ok(output) => {
                            let events = map_claude(&[output], &session_id, &op_id);
                            if events.is_empty() {
                                // Skipped frame (unknown nested variant,
                                // internal transcript event) — keep reading.
                                continue;
                            }
                            // map_claude puts the terminal (if any) last.
                            let terminal = matches!(
                                events.last(),
                                Some(HostEvent::OpFinished(_) | HostEvent::OpFailed(_))
                            );
                            if terminal {
                                // Turn over: kill the child.
                                drop_client(&sessions, &session_id).await;
                            }
                            let mut iter = events.into_iter();
                            let first = iter.next().expect("events is non-empty");
                            pending.extend(iter);
                            return Some((
                                Ok(first),
                                (sessions, op_id, session_id, pending, terminal),
                            ));
                        }
                        Err(ClaudeError::ConnectionClosed) => {
                            // Stream abort (EOF or client killed) before a
                            // terminal frame: exactly one OpFailed (PD-3).
                            drop_client(&sessions, &session_id).await;
                            return Some((
                                Ok(HostEvent::OpFailed(OperationFailedEvent {
                                    session_id: session_id.clone(),
                                    op_id: op_id.clone(),
                                    error_category: "stream_closed".to_string(),
                                    error_message: "claude stream closed before the turn completed"
                                        .to_string(),
                                })),
                                (sessions, op_id, session_id, pending, true),
                            ));
                        }
                        Err(error) => {
                            // Typed-decode failure / io error: the frame is
                            // lost, so the turn fails once (PD-3, AR-7).
                            drop_client(&sessions, &session_id).await;
                            let failed = classify_stream_error(&error, &session_id, &op_id)
                                .expect("stream error always classifies to a terminal");
                            return Some((
                                Ok(HostEvent::OpFailed(failed)),
                                (sessions, op_id, session_id, pending, true),
                            ));
                        }
                    }
                }
            },
        )
        .boxed()
    }
}

/// Kill the session's in-flight CLI child by dropping the crate client (the
/// crate's `Drop` kills the process). Runs under the per-session lock (B-2).
async fn drop_client(
    sessions: &Arc<RwLock<HashMap<HostSessionId, NativeSession>>>,
    session_id: &HostSessionId,
) {
    let state = {
        // Short registry write: also clear the recorded PID so a stale
        // cancel fallback can never signal a recycled pid.
        let mut guard = sessions.write().await;
        match guard.get_mut(session_id) {
            Some(ns) => {
                ns.child_pid.store(0, Ordering::Relaxed);
                Arc::clone(&ns.state)
            }
            None => return,
        }
    };
    let mut guard = state.lock().await;
    guard.client = None;
}

/// Best-effort SIGTERM for the spawned CLI child by PID.
///
/// Fallback for cancel/shutdown when the per-session lock is held by an
/// in-flight frame read (B-2): the crate client is unreachable without
/// `&mut`, so the child is signalled directly; the read returns EOF and the
/// stream backstop emits the one `OpFailed(stream_closed)`. The child is
/// our own direct descendant; the recorded PID is cleared whenever the
/// client is dropped, so the tiny pid-reuse window only exists while the
/// child is verifiably alive (the read is blocked on its stdout).
fn kill_child_by_pid(pid: u32) {
    if pid == 0 {
        return;
    }
    let status = std::process::Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status();
    match status {
        Ok(_) => {
            tracing::info!(pid, "Signalled claude CLI child (cancel/shutdown fallback)");
        }
        Err(error) => {
            tracing::warn!(pid, error = %error, "Failed to signal claude CLI child");
        }
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
        // Native provider launch only registers session state — the CLI
        // child spawns lazily on the first execute().
        let host_session_id = HostSessionId::new();

        // Generate a UUID for Claude CLI session continuity (AR-5). The
        // crate emits `--session-id <uuid>` on first execute and
        // `--resume <uuid>` on subsequent ones.
        let claude_session_id = uuid::Uuid::new_v4();

        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(
                host_session_id.clone(),
                NativeSession {
                    state: Arc::new(Mutex::new(ClientState {
                        claude_session_id,
                        first_exec_done: false,
                        client: None,
                    })),
                    child_pid: AtomicU32::new(0),
                    cwd: spec.cwd.clone(),
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

    // Multi-turn execute involves session-state lookup, crate client
    // spawn + stdin write, and the frame stream — splitting would reduce
    // clarity, so allow the line count here.
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

        // Clone the per-session state + working directory under a short
        // registry read (B-2, N-1); the setup I/O then runs lock-free or
        // under the per-session lock only, never the provider-global one.
        let (state, cwd) = {
            let sessions = self.sessions.read().await;
            let native_session = sessions.get(&session.session_id).ok_or_else(|| {
                HostError::internal(format!(
                    "session {} not found in native CLI provider",
                    session.session_id
                ))
            })?;
            let state = Arc::clone(&native_session.state);
            let cwd = native_session.cwd.clone();
            drop(sessions);
            (state, cwd)
        };

        // Session continuity state + stale-client cleanup under the
        // per-session lock so a concurrent execute/shutdown cannot race a
        // half-initialized session.
        let (claude_session_id, is_resume) = {
            let mut guard = state.lock().await;

            // A previous turn's stream that was abandoned before its
            // terminal (the consumer dropped it): kill the stale child.
            if guard.client.is_some() {
                tracing::info!(
                    session_id = %session.session_id,
                    provider_id = %self.provider_id,
                    "Dropping stale claude client from an abandoned turn"
                );
                guard.client = None;
            }

            let id = guard.claude_session_id;
            let resume = guard.first_exec_done;
            guard.first_exec_done = true;
            drop(guard);
            (id, resume)
        };

        // Spawn the CLI child via the crate builder and send the prompt
        // (crate owns all flags — `--print`, stream-json, `--session-id`,
        // `--resume`, `--working-directory`). The setup phase runs under the
        // prompt timeout, mirroring the old spawn + stdin-write enforcement;
        // a failure returns an error, not a stream.
        let provider_id = self.provider_id.clone();
        let command = self.command.clone();
        let env = self.env.clone();
        let prompt_dur = self.timeouts.prompt_duration();
        let (client, child_pid) = tokio::time::timeout(prompt_dur, async move {
            let builder = ClaudeCliBuilder::new()
                .command(command.clone())
                .working_directory(cwd)
                .session_id(claude_session_id);
            let builder = if is_resume {
                builder.resume(Some(claude_session_id.to_string()))
            } else {
                builder
            };

            let mut cmd = builder.build_command().map_err(|e| {
                HostError::launch_failed(
                    provider_id.clone(),
                    format!("failed to spawn '{command}'"),
                    Some(e.to_string()),
                )
            })?;
            cmd.envs(&env);
            let child = cmd.spawn().map_err(|e| {
                HostError::launch_failed(
                    provider_id.clone(),
                    format!("failed to spawn '{command}'"),
                    Some(e.to_string()),
                )
            })?;
            // Record the child PID before moving it into the crate client:
            // cancel/shutdown fall back to signalling it when the per-session
            // lock is held by an in-flight frame read (B-2).
            let child_pid = child.id().unwrap_or(0);
            let mut client = AsyncClient::new(child).map_err(|e| {
                HostError::launch_failed(
                    provider_id.clone(),
                    format!("failed to spawn '{command}'"),
                    Some(e.to_string()),
                )
            })?;
            client
                .send(&ClaudeInput::user_message(prompt_text, claude_session_id))
                .await
                .map_err(|e| {
                    HostError::protocol_error(
                        "failed to write prompt to stdin",
                        Some(e.to_string()),
                    )
                })?;
            Ok::<_, HostError>((client, child_pid))
        })
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

        {
            let mut guard = state.lock().await;
            guard.client = Some(client);
        }
        {
            let mut sessions = self.sessions.write().await;
            if let Some(native_session) = sessions.get_mut(&session.session_id) {
                native_session.child_pid.store(child_pid, Ordering::Relaxed);
            }
        }

        tracing::info!(
            session_id = %session.session_id,
            provider_id = %self.provider_id,
            op_id = %op_id,
            resume = is_resume,
            "Claude turn started"
        );

        Ok(self.build_event_stream(op_id, session.session_id.clone()))
    }

    async fn cancel(
        &self,
        session: &ManagedSessionHandle,
        op_id: HostOperationId,
    ) -> HostResult<()> {
        // Best-effort graceful stop request, then kill: the crate client is
        // dropped (its Drop kills the child) and the stream backstop emits
        // exactly one `OpFailed(stream_closed)` (AR-1 table). Runs under the
        // per-session lock (B-2); if an in-flight frame read holds it, the
        // child is signalled by PID instead so cancel stays prompt.
        let state_and_pid = {
            let guard = self.sessions.read().await;
            guard
                .get(&session.session_id)
                .map(|ns| (Arc::clone(&ns.state), ns.child_pid.load(Ordering::Relaxed)))
        };
        let Some((state, pid)) = state_and_pid else {
            return Ok(());
        };

        match state.try_lock() {
            Ok(mut guard) => {
                if let Some(client) = guard.client.as_mut() {
                    if let Err(error) = client.interrupt().await {
                        tracing::warn!(
                            session_id = %session.session_id,
                            error = %error,
                            "Claude interrupt request failed; killing child"
                        );
                    }
                }
                guard.client = None;
                drop(guard);
                // The client is dropped (child killed); clear the recorded
                // PID so a later fallback can never signal a recycled pid.
                if let Some(ns) = self.sessions.write().await.get_mut(&session.session_id) {
                    ns.child_pid.store(0, Ordering::Relaxed);
                }
            }
            Err(_) => {
                // An in-flight frame read holds the per-session lock: kill
                // the child by PID; the read returns EOF and the stream
                // backstop emits the one terminal.
                kill_child_by_pid(pid);
            }
        }

        tracing::info!(
            session_id = %session.session_id,
            op_id = %op_id,
            provider_id = %self.provider_id,
            "Native CLI cancel: interrupt sent, child killed"
        );
        Ok(())
    }

    async fn shutdown(&self, session: ManagedSessionHandle) -> HostResult<()> {
        // Removing the session drops the crate client (killing the CLI
        // child) once no in-flight frame read holds it; if one does, the
        // child is signalled by PID so shutdown stays prompt (B-2).
        let removed = {
            let mut sessions = self.sessions.write().await;
            sessions.remove(&session.session_id)
        };
        if let Some(native_session) = removed {
            match native_session.state.try_lock() {
                Ok(mut guard) => {
                    guard.client = None;
                }
                Err(_) => {
                    kill_child_by_pid(native_session.child_pid.load(Ordering::Relaxed));
                }
            }
        }

        tracing::info!(
            session_id = %session.session_id,
            provider_id = %self.provider_id,
            "Native CLI session shut down (claude child killed via crate client teardown)"
        );
        Ok(())
    }

    fn capabilities(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::native_cli_limited()
    }
}

#[cfg(test)]
mod tests {
    // Lock guards (session registry / crate client) are intentionally held
    // to the end of the visible test scope for readability; the nursery
    // significant_drop_tightening suggestion to drop them earlier is noise
    // here.
    #![allow(clippy::significant_drop_tightening)]

    use super::*;
    use crate::capability::model::{FinishReason, HostOperation, LaunchSpec};

    const MOCK_CLAUDE_CLI: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/mock_claude_cli.py"
    );

    fn launch_spec() -> LaunchSpec {
        LaunchSpec {
            cwd: std::path::PathBuf::from("/tmp"),
            model: None,
            mode: None,
            mcp_servers: vec![],
        }
    }

    async fn collect_events(stream: HostEventStream) -> Vec<HostEvent> {
        let results: Vec<_> = stream.collect().await;
        results
            .into_iter()
            .map(|r| r.expect("stream item should be Ok"))
            .collect()
    }

    fn terminal_count(events: &[HostEvent]) -> usize {
        events
            .iter()
            .filter(|e| matches!(e, HostEvent::OpFinished(_) | HostEvent::OpFailed(_)))
            .count()
    }

    /// Provider wired to the fixture mock CLI (a small Python stream-json
    /// speaker; see `MOCK_CLAUDE_CLI`).
    fn mock_provider(env: HashMap<String, String>) -> ClaudeCliProvider {
        ClaudeCliProvider::new(
            ProviderId::new("test-claude-stream-json"),
            "Test".to_string(),
            MOCK_CLAUDE_CLI.to_string(),
            env,
            TimeoutConfig::default(),
        )
    }

    async fn launch_and_execute(
        provider: &ClaudeCliProvider,
        text: &str,
    ) -> (ManagedSessionHandle, HostEventStream) {
        let handle = provider.launch(launch_spec()).await.expect("launch");
        let stream = provider
            .execute(
                &handle,
                HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: text.to_string(),
                    }],
                },
            )
            .await
            .expect("execute");
        (handle, stream)
    }

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
            "native CLI supports session_restore via the crate's --session-id/--resume (AR-5)"
        );
        assert!(!desc.capabilities.mcp_http);
    }

    #[test]
    fn default_config_command() {
        let provider = ClaudeCliProvider::default_config();
        assert_eq!(provider.command, "claude");
        assert!(provider.env.is_empty());
    }

    #[tokio::test]
    async fn probe_unavailable_when_command_not_found() {
        let provider = ClaudeCliProvider::new(
            ProviderId::new("nonexistent-cli-xyz"),
            "Fake".to_string(),
            "nonexistent_cli_xyz_12345".to_string(),
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
            HashMap::from([("ANTHROPIC_API_KEY".to_string(), "sk-test".to_string())]),
            TimeoutConfig::default(),
        );

        assert_eq!(provider.provider_id.0, "my-claude");
        assert_eq!(provider.command, "/opt/claude/bin/claude");
        assert_eq!(provider.env.get("ANTHROPIC_API_KEY").unwrap(), "sk-test");
    }

    #[test]
    fn native_cli_limited_descriptor_session_restore() {
        let caps = CapabilityDescriptor::native_cli_limited();
        assert!(
            caps.session_restore,
            "native_cli_limited should claim session_restore since the crate's --resume is used (AR-5)"
        );
    }

    #[tokio::test]
    async fn launch_registers_session_without_client() {
        let provider = ClaudeCliProvider::default_config();

        let handle = provider.launch(launch_spec()).await.expect("launch");

        let sessions = provider.sessions.read().await;
        let native_session = sessions.get(&handle.session_id);
        assert!(native_session.is_some(), "session should be registered");

        let ns = native_session.unwrap();
        let state = ns.state.lock().await;
        assert!(
            state.client.is_none(),
            "client should start lazily on execute"
        );
        assert!(
            !state.first_exec_done,
            "first_exec_done should be false initially"
        );
        assert_ne!(
            state.claude_session_id,
            uuid::Uuid::nil(),
            "claude_session_id should be a generated UUID"
        );
        assert_eq!(ns.child_pid.load(Ordering::Relaxed), 0);
    }

    /// A turn against the mock CLI maps through `map_claude`: one
    /// `MessageDelta` (whole text block), then one `OpFinished(EndTurn)`
    /// from the `Result` frame — exactly one terminal. The crate client is
    /// dropped when the turn ends, killing the child.
    #[tokio::test]
    async fn execute_maps_turn_events_and_kills_client() {
        let provider = mock_provider(HashMap::new());

        let (handle, stream) = launch_and_execute(&provider, "hi").await;
        let events = collect_events(stream).await;

        assert!(
            matches!(&events[0], HostEvent::MessageDelta(d) if d.text == "hello from mock claude"),
            "delta must map through the T1 mapper: {events:?}"
        );
        assert!(
            matches!(&events[1], HostEvent::OpFinished(f) if f.reason == FinishReason::EndTurn),
            "completed turn must end cleanly: {events:?}"
        );
        assert_eq!(events.len(), 2, "delta + terminal: {events:?}");
        assert_eq!(terminal_count(&events), 1);

        let sessions = provider.sessions.read().await;
        let ns = sessions.get(&handle.session_id).expect("session exists");
        let state = ns.state.lock().await;
        assert!(
            state.client.is_none(),
            "client must be dropped (child killed) when the turn ends"
        );
        assert!(
            state.first_exec_done,
            "first_exec_done must be set after the first execute"
        );
        assert_eq!(
            ns.child_pid.load(Ordering::Relaxed),
            0,
            "child pid must be cleared when the client is dropped"
        );
    }

    /// Session restore (AR-5): the first execute passes the host UUID via
    /// the crate's `--session-id` flag; the second execute spawns a fresh
    /// child and resumes the same session via `--resume <uuid>` — Nexus
    /// never assembles the argv itself (asserted on the mock's recorded
    /// argv per spawn).
    #[tokio::test]
    async fn second_execute_resumes_session() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let req_log = temp_dir.path().join("argv.jsonl");
        let req_log_path = req_log.to_string_lossy().into_owned();
        let provider = mock_provider(HashMap::from([("REQ_LOG".to_string(), req_log_path)]));

        let handle = provider.launch(launch_spec()).await.expect("launch");
        let session_uuid = {
            let sessions = provider.sessions.read().await;
            let ns = sessions.get(&handle.session_id).expect("session exists");
            let state = ns.state.lock().await;
            state.claude_session_id
        };

        let stream1 = provider
            .execute(
                &handle,
                HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "hi".to_string(),
                    }],
                },
            )
            .await
            .expect("first execute");
        let events1 = collect_events(stream1).await;
        assert_eq!(terminal_count(&events1), 1);

        let stream2 = provider
            .execute(
                &handle,
                HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "again".to_string(),
                    }],
                },
            )
            .await
            .expect("second execute");
        let events2 = collect_events(stream2).await;
        assert_eq!(terminal_count(&events2), 1);

        let log = std::fs::read_to_string(&req_log).expect("read argv log");
        let spawns: Vec<serde_json::Value> = log
            .lines()
            .map(|line| serde_json::from_str(line).expect("log line is JSON"))
            .collect();
        assert_eq!(spawns.len(), 2, "one child spawn per execute");

        let argv0: Vec<String> = spawns[0]["argv"]
            .as_array()
            .expect("argv array")
            .iter()
            .map(|arg| arg.as_str().expect("argv string").to_string())
            .collect();
        let argv1: Vec<String> = spawns[1]["argv"]
            .as_array()
            .expect("argv array")
            .iter()
            .map(|arg| arg.as_str().expect("argv string").to_string())
            .collect();

        let uuid_str = session_uuid.to_string();
        assert!(
            argv0.iter().any(|arg| arg == "--session-id"),
            "first spawn must carry --session-id: {argv0:?}"
        );
        assert!(
            argv0.iter().any(|arg| arg == &uuid_str),
            "first spawn must carry the host uuid: {argv0:?}"
        );
        assert!(
            argv1.iter().any(|arg| arg == "--resume"),
            "second spawn must carry --resume: {argv1:?}"
        );
        assert!(
            argv1.iter().any(|arg| arg == &uuid_str),
            "second spawn must resume the same host uuid: {argv1:?}"
        );
        assert!(
            !argv1.iter().any(|arg| arg == "--session-id"),
            "resume spawn must not pass --session-id: {argv1:?}"
        );
    }

    /// `cancel()` sends the crate's graceful interrupt, then kills the
    /// child; the in-flight stream backstop emits exactly one
    /// `OpFailed(stream_closed)` (AR-1 stream-abort row).
    #[tokio::test]
    async fn cancel_interrupts_then_kills_client() {
        let provider = mock_provider(HashMap::from([("BLOCK_TURN".to_string(), "1".to_string())]));

        let (handle, stream) = launch_and_execute(&provider, "hi").await;
        let mut stream = stream;

        // Pump the first frame so the turn is visibly streaming.
        let first = stream.next().await.expect("first event").expect("ok");
        assert!(matches!(
            first,
            HostEvent::MessageDelta(d) if d.text == "hello from mock claude"
        ));

        provider
            .cancel(&handle, HostOperationId::new())
            .await
            .expect("cancel");

        let rest = collect_events(stream).await;
        assert!(
            rest.last().is_some_and(|e| matches!(
                e,
                HostEvent::OpFailed(f) if f.error_category == "stream_closed"
            )),
            "cancel must end with one OpFailed(stream_closed): {rest:?}"
        );
        assert_eq!(terminal_count(&rest), 1);

        let sessions = provider.sessions.read().await;
        let ns = sessions.get(&handle.session_id).expect("session exists");
        assert!(
            ns.state.lock().await.client.is_none(),
            "client must be dropped after cancel"
        );
    }

    /// `shutdown()` tears down the crate client (killing the CLI child) and
    /// removes the session; the in-flight stream still emits exactly one
    /// terminal — `OpFailed(stream_closed)` (PD-3 backstop).
    #[tokio::test]
    async fn shutdown_tears_down_client_and_stream_terminates() {
        let provider = mock_provider(HashMap::from([("BLOCK_TURN".to_string(), "1".to_string())]));

        let (handle, stream) = launch_and_execute(&provider, "hi").await;
        let mut stream = stream;

        // Pump the first frame so the turn is visibly streaming.
        let first = stream.next().await.expect("first event").expect("ok");
        assert!(matches!(first, HostEvent::MessageDelta(_)));

        provider.shutdown(handle.clone()).await.expect("shutdown");

        let rest = collect_events(stream).await;
        assert!(
            rest.last().is_some_and(|e| matches!(
                e,
                HostEvent::OpFailed(f) if f.error_category == "stream_closed"
            )),
            "stream must end with one OpFailed(stream_closed): {rest:?}"
        );
        assert_eq!(terminal_count(&rest), 1);

        let sessions = provider.sessions.read().await;
        assert!(
            sessions.is_empty(),
            "session must be removed after shutdown"
        );
    }

    /// Typed-decode failure (PD-3 row 2): a frame with an unknown
    /// top-level `type` tag fails typed decode in the crate — the turn
    /// fails once with `OpFailed(decode_error)`, no per-item skip.
    #[tokio::test]
    async fn decode_error_fails_turn_once() {
        let provider = mock_provider(HashMap::from([("BAD_FRAME".to_string(), "1".to_string())]));

        let (handle, stream) = launch_and_execute(&provider, "hi").await;
        let events = collect_events(stream).await;

        assert!(
            matches!(&events[0], HostEvent::MessageDelta(d) if d.text == "hello from mock claude"),
            "delta before the bad frame: {events:?}"
        );
        assert!(
            matches!(
                events.last(),
                Some(HostEvent::OpFailed(f)) if f.error_category == "decode_error"
            ),
            "unknown top-level type must fail the turn once with decode_error: {events:?}"
        );
        assert_eq!(events.len(), 2, "delta + one terminal: {events:?}");
        assert_eq!(terminal_count(&events), 1);

        let sessions = provider.sessions.read().await;
        let ns = sessions.get(&handle.session_id).expect("session exists");
        assert!(
            ns.state.lock().await.client.is_none(),
            "client must be dropped after the failed turn"
        );
    }

    /// B-3: a silent-but-alive child must not hard-fail with
    /// `OpFailed(timeout)` after the prompt-setup budget. The mock holds
    /// the turn open (`BLOCK_TURN`); the stream must stay quiet well past a
    /// tiny prompt budget instead of emitting a timeout terminal.
    #[tokio::test]
    async fn silent_turn_does_not_time_out_on_prompt_budget() {
        let timeouts = TimeoutConfig {
            prompt_ms: 1000,
            ..TimeoutConfig::default()
        };
        let provider = ClaudeCliProvider::new(
            ProviderId::new("test-claude-silent"),
            "Test".to_string(),
            MOCK_CLAUDE_CLI.to_string(),
            HashMap::from([("BLOCK_TURN".to_string(), "1".to_string())]),
            timeouts,
        );

        let (handle, stream) = launch_and_execute(&provider, "hi").await;
        let mut stream = stream;

        // Pump the first frame so the turn is visibly streaming.
        let first = stream.next().await.expect("first event").expect("ok");
        assert!(matches!(
            first,
            HostEvent::MessageDelta(d) if d.text == "hello from mock claude"
        ));

        // The next frame never arrives (the child is silent). With the old
        // prompt_ms-based per-frame timeout this emitted OpFailed(timeout)
        // after ~1s; the fixed rail keeps waiting (old-rail EOF semantics).
        let next =
            tokio::time::timeout(std::time::Duration::from_millis(2500), stream.next()).await;
        assert!(
            next.is_err(),
            "silent turn must keep waiting, not emit a timeout terminal"
        );

        // Tear down so no child process leaks.
        provider.shutdown(handle).await.expect("shutdown");
    }

    /// B-2: cancel must stay prompt even when the session's OWN frame read
    /// is in flight (silent child). The per-session lock is held by the
    /// read, so the fallback signals the child by PID; the read returns
    /// EOF and the stream backstop emits exactly one `OpFailed(stream_closed)`.
    #[tokio::test]
    async fn cancel_is_prompt_when_own_frame_read_is_in_flight() {
        let provider = mock_provider(HashMap::from([("BLOCK_TURN".to_string(), "1".to_string())]));

        let (handle, stream) = launch_and_execute(&provider, "hi").await;
        let mut stream = stream;

        // Pump the first frame so the turn is visibly streaming.
        let first = stream.next().await.expect("first event").expect("ok");
        assert!(matches!(
            first,
            HostEvent::MessageDelta(d) if d.text == "hello from mock claude"
        ));

        // Poll in a background task so the next frame read is in flight.
        let pump = tokio::spawn(async move { stream.collect::<Vec<_>>().await });
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let cancel = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            provider.cancel(&handle, HostOperationId::new()),
        )
        .await;
        assert!(
            cancel.is_ok(),
            "cancel must not wait on the session's own in-flight frame read"
        );

        // The stream terminates with the backstop once the child is dead.
        let events = tokio::time::timeout(std::time::Duration::from_secs(2), pump)
            .await
            .expect("stream must terminate promptly after cancel")
            .expect("pump task");
        let results: Vec<HostEvent> = events
            .into_iter()
            .map(|r| r.expect("stream item should be Ok"))
            .collect();
        assert!(
            results.last().is_some_and(|e| matches!(
                e,
                HostEvent::OpFailed(f) if f.error_category == "stream_closed"
            )),
            "cancel must end with one OpFailed(stream_closed): {results:?}"
        );
        assert_eq!(terminal_count(&results), 1);
    }

    /// B-2: with session A's frame read in flight (`BLOCK_TURN`), cancel and
    /// shutdown of session B must complete promptly — the per-session lock
    /// replaces the provider-global write lock.
    #[tokio::test]
    async fn session_b_cancel_and_shutdown_do_not_wait_on_session_a_read() {
        let provider = mock_provider(HashMap::from([("BLOCK_TURN".to_string(), "1".to_string())]));

        let handle_a = provider.launch(launch_spec()).await.expect("launch a");
        let stream_a = provider
            .execute(
                &handle_a,
                HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "blocked".to_string(),
                    }],
                },
            )
            .await
            .expect("execute a");

        // Keep session A's stream polling so its frame read is in flight.
        let pump = tokio::spawn(async move {
            let _ = stream_a.collect::<Vec<_>>().await;
        });
        // Let the read settle into the blocked frame read.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let handle_b = provider.launch(launch_spec()).await.expect("launch b");

        let cancel = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            provider.cancel(&handle_b, HostOperationId::new()),
        )
        .await;
        assert!(
            cancel.is_ok(),
            "cancel of session B must not wait on session A's frame read"
        );

        let shutdown = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            provider.shutdown(handle_b),
        )
        .await;
        assert!(
            shutdown.is_ok(),
            "shutdown of session B must not wait on session A's frame read"
        );

        // Clean up session A: abort the pump (releasing the blocked read),
        // then tear the session down so no child process leaks.
        pump.abort();
        let _ = pump.await;
        provider.shutdown(handle_a).await.expect("shutdown a");
    }

    /// N-1: `LaunchSpec.cwd` is applied to the CLI child via the crate's
    /// `working_directory` (observed as the child's working directory).
    #[tokio::test]
    async fn launch_cwd_is_applied_to_cli_child() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let cwd = temp_dir.path().to_path_buf();
        let req_log = temp_dir.path().join("argv.jsonl");
        let req_log_path = req_log.to_string_lossy().into_owned();
        let provider = mock_provider(HashMap::from([("REQ_LOG".to_string(), req_log_path)]));

        let handle = provider
            .launch(LaunchSpec {
                cwd: cwd.clone(),
                model: None,
                mode: None,
                mcp_servers: vec![],
            })
            .await
            .expect("launch");

        let stream = provider
            .execute(
                &handle,
                HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "hi".to_string(),
                    }],
                },
            )
            .await
            .expect("execute");
        let events = collect_events(stream).await;
        assert_eq!(terminal_count(&events), 1);

        let log = std::fs::read_to_string(&req_log).expect("read argv log");
        let spawn: serde_json::Value = log
            .lines()
            .next()
            .map(|line| serde_json::from_str(line).expect("log line is JSON"))
            .expect("one spawn logged");
        let child_cwd = std::fs::canonicalize(spawn["cwd"].as_str().expect("cwd string"))
            .expect("child cwd exists");
        let launch_cwd = std::fs::canonicalize(&cwd).expect("launch cwd exists");
        assert_eq!(
            child_cwd, launch_cwd,
            "LaunchSpec.cwd must reach the CLI child (symlinks resolved)"
        );
    }

    #[tokio::test]
    async fn execute_fails_when_binary_missing() {
        let provider = ClaudeCliProvider::new(
            ProviderId::new("test-claude-missing"),
            "Test".to_string(),
            "nonexistent_claude_xyz_12345".to_string(),
            HashMap::new(),
            TimeoutConfig::default(),
        );

        let handle = provider.launch(launch_spec()).await.expect("launch");
        let result = provider
            .execute(
                &handle,
                HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "hi".to_string(),
                    }],
                },
            )
            .await;

        assert!(
            result.is_err(),
            "execute with a missing binary must error, not stream"
        );

        let sessions = provider.sessions.read().await;
        let ns = sessions.get(&handle.session_id).expect("session exists");
        assert!(
            ns.state.lock().await.client.is_none(),
            "no client should be retained after a failed start"
        );
    }

    #[tokio::test]
    async fn cancel_with_no_active_turn_is_noop() {
        let provider = mock_provider(HashMap::new());
        let handle = provider.launch(launch_spec()).await.expect("launch");

        provider
            .cancel(&handle, HostOperationId::new())
            .await
            .expect("cancel with no active turn must succeed");
    }

    #[tokio::test]
    async fn shutdown_removes_session() {
        let provider = ClaudeCliProvider::default_config();

        let handle = provider.launch(launch_spec()).await.expect("launch");

        provider.shutdown(handle).await.expect("shutdown");

        let sessions = provider.sessions.read().await;
        assert!(
            sessions.is_empty(),
            "session should be removed after shutdown"
        );
    }

    #[tokio::test]
    async fn empty_prompt_is_rejected() {
        let provider = mock_provider(HashMap::new());
        let handle = provider.launch(launch_spec()).await.expect("launch");

        let result = provider
            .execute(
                &handle,
                HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![],
                },
            )
            .await;

        assert!(result.is_err(), "empty prompt must be rejected");
    }
}
