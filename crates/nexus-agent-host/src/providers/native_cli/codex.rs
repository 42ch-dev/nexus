//! Codex native provider adapter (`codex-codes` app-server client).
//!
//! Implements a native provider for `OpenAI`'s `codex` CLI through the
//! `codex-codes` async client (PD-2): a long-lived `codex app-server`
//! process speaking JSON-RPC over stdio. Nexus owns only the `HostEvent`
//! normalization (`map_codex`) and the approval auto-policy (AR-4); the
//! crate owns the wire parser, the process lifecycle, and teardown.
//!
//! # Session Model
//!
//! Each `launch()` registers a host session with no client yet. The first
//! `execute()` lazily starts the app-server client
//! ([`codex_codes::AsyncClient::start_with`]), creates a codex thread
//! (`thread/start`), and runs `turn/start` per prompt. The thread id is
//! reused while the app-server lives; if the app-server connection is lost
//! between turns, the client is restarted and the thread is restored via
//! `thread/resume` (AR-5 — replaces the old `exec resume <id>`).
//!
//! # Approvals
//!
//! Every `turn/start` carries `approval_policy: AskForApproval::Never` and
//! `sandbox_policy: SandboxPolicy::ReadOnly { network_access: None }` — the
//! app-server equivalent of today's headless `codex exec -s read-only`
//! (AR-4). Any residual approval server-request is auto-answered from the
//! native permission classification without surfacing to the author:
//! read-only command actions (`Allow`) are accepted, everything else
//! (`Ask` / `Deny`) is denied (headless fail-safe, logged).

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use async_trait::async_trait;
use codex_codes::messages::{Notification, ServerRequest};
use codex_codes::protocol::{
    AskForApproval, CommandAction, SandboxPolicy, TurnInterruptParams, TurnStartParams, UserInput,
};
use codex_codes::{AsyncClient, Error as CodexError, RequestId, ServerMessage};
use futures_util::StreamExt;
use tokio::sync::{Mutex, RwLock};

use crate::capability::model::{
    CapabilityDescriptor, HostContentBlock, HostEvent, HostEventStream, ManagedSessionHandle,
    OperationFailedEvent, ProtocolKind, ProviderDescriptor, ProviderHealth,
};
use crate::config::TimeoutConfig;
use crate::error::{HostError, HostResult};
use crate::ids::{HostOperationId, HostSessionId, ProviderId};
use crate::providers::native_cli::map_codex::{classify_stream_error, map_codex};
use crate::ProviderAdapter;

/// Crate-client-scoped state for a managed codex session, guarded by the
/// per-session mutex (B-2): only this session's operations contend on it —
/// cancel/shutdown of other sessions never wait on this session's frame
/// reads or setup RPCs. The provider-global registry `RwLock` is only for
/// short lookups.
struct ClientState {
    /// The codex app-server async client, started lazily on the first
    /// execute. Dropping it kills the app-server process — no raw `Child`
    /// handles are tracked by the provider.
    client: Option<AsyncClient>,
    /// The codex thread (conversation) id. Created on the first execute and
    /// reused while the app-server lives; restored via `thread/resume` if
    /// the app-server restarts (AR-5).
    thread_id: Option<String>,
    /// The turn currently streaming on this session, used by `cancel()` /
    /// stream-timeout `turn/interrupt` and to filter stale notifications
    /// (B-1). Cleared when the stream ends.
    active_turn_id: Option<String>,
}

/// Internal state for a managed codex native session.
struct NativeSession {
    /// Per-session lock around the crate client and turn state (B-2).
    state: Arc<Mutex<ClientState>>,
    /// Working directory for the app-server process, retained from
    /// `LaunchSpec::cwd`.
    cwd: std::path::PathBuf,
}

impl std::fmt::Debug for NativeSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("NativeSession");
        debug.field("cwd", &self.cwd.display());
        match self.state.try_lock() {
            Ok(state) => {
                debug
                    .field("client_started", &state.client.is_some())
                    .field("thread_id", &state.thread_id.as_deref())
                    .field("active_turn_id", &state.active_turn_id.as_deref());
            }
            Err(_) => {
                debug.field("client_state", &"<locked>");
            }
        }
        debug.finish()
    }
}

/// Codex native provider.
///
/// Spawns `codex app-server` (via the `codex-codes` crate) and normalizes
/// its JSON-RPC notifications into `HostEvent` items. Multi-turn continuity
/// is the app-server thread: `thread/start` on first execute, the thread id
/// reused while the app-server lives, `thread/resume` after a restart
/// (AR-5).
pub struct CodexNativeProvider {
    /// Provider ID (typically `codex-native` to avoid collision with ACP registry).
    provider_id: ProviderId,
    /// Display name.
    display_name: String,
    /// Command to execute (e.g., `codex`).
    command: String,
    /// Environment variables to inject into the app-server process.
    env: HashMap<String, String>,
    /// Active sessions: host session ID → native session state.
    sessions: Arc<RwLock<HashMap<HostSessionId, NativeSession>>>,
    /// Timeout configuration for stage-level enforcement.
    timeouts: TimeoutConfig,
}

impl CodexNativeProvider {
    /// Create a new Codex provider with the given configuration.
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

    /// Create with default configuration for the `codex` command.
    #[must_use]
    pub fn default_config() -> Self {
        Self::new(
            ProviderId::new("codex-native"),
            "Codex CLI (native)".to_string(),
            "codex".to_string(),
            HashMap::new(),
            TimeoutConfig::default(),
        )
    }

    /// Ensure the app-server client and thread exist, then start a turn.
    ///
    /// Mirrors the old spawn+stdin-write setup phase: it runs under the
    /// caller's prompt timeout and returns an error (no stream) on failure.
    ///
    /// If the app-server connection was lost between turns
    /// (`ServerClosed` / `ConnectionClosed` on `turn/start`), the dead
    /// client is dropped, a fresh app-server is started, and the thread is
    /// restored with `thread/resume` before retrying once (AR-5).
    // The per-session lock is deliberately held across the whole setup
    // (client + thread + turn/start) so a concurrent execute/cancel/shutdown
    // cannot race a half-initialized session — and, unlike the old
    // provider-global write lock (B-2), only this session's operations are
    // serialized by it. too_many_lines: the restart/retry loop reads
    // linearly and splitting it would obscure it.
    #[allow(clippy::significant_drop_tightening, clippy::too_many_lines)]
    async fn ensure_client_and_start_turn(
        &self,
        session_id: &HostSessionId,
        prompt_text: &str,
    ) -> HostResult<String> {
        // Clone the per-session state under a short registry read (B-2);
        // every setup RPC then runs under the per-session lock only.
        let (state, cwd) = {
            let sessions = self.sessions.read().await;
            let native_session = sessions.get(session_id).ok_or_else(|| {
                HostError::internal(format!(
                    "session {session_id} not found in native CLI provider"
                ))
            })?;
            let state = Arc::clone(&native_session.state);
            let cwd = native_session.cwd.clone();
            drop(sessions);
            (state, cwd)
        };

        let mut guard = state.lock().await;
        let mut attempts = 0;
        loop {
            attempts += 1;

            if guard.client.is_none() {
                let builder = codex_codes::AppServerBuilder::new()
                    .command(self.command.clone())
                    .working_directory(cwd.clone())
                    .envs(self.env.clone());
                let client = codex_codes::AsyncClient::start_with(builder)
                    .await
                    .map_err(|e| {
                        HostError::launch_failed(
                            self.provider_id.clone(),
                            format!("failed to start '{}' app-server", self.command),
                            Some(e.to_string()),
                        )
                    })?;
                guard.client = Some(client);
            }

            let thread_id = if let Some(thread_id) = guard.thread_id.clone() {
                if attempts > 1 {
                    // The app-server restarted: replay thread history so
                    // turns continue where they left off (AR-5).
                    guard
                        .client
                        .as_mut()
                        .expect("client was just ensured")
                        .thread_resume(&codex_codes::ThreadResumeParams {
                            thread_id: thread_id.clone(),
                            ..codex_codes::ThreadResumeParams::default()
                        })
                        .await
                        .map_err(|e| {
                            HostError::protocol_error(
                                "codex thread resume failed",
                                Some(e.to_string()),
                            )
                        })?;
                    tracing::info!(
                        session_id = %session_id,
                        provider_id = %self.provider_id,
                        thread_id = %thread_id,
                        "Codex app-server restarted; thread resumed"
                    );
                }
                thread_id
            } else {
                let response = guard
                    .client
                    .as_mut()
                    .expect("client was just ensured")
                    .thread_start(&codex_codes::ThreadStartParams::default())
                    .await
                    .map_err(|e| {
                        HostError::protocol_error("codex thread start failed", Some(e.to_string()))
                    })?;
                let thread_id = response.thread.id.clone();
                guard.thread_id = Some(thread_id.clone());
                tracing::info!(
                    session_id = %session_id,
                    provider_id = %self.provider_id,
                    thread_id = %thread_id,
                    "Codex thread started"
                );
                thread_id
            };

            // AR-4: every turn/start runs inside the read-only sandbox with
            // no approval prompts — never more writable than today's
            // headless `-s read-only`.
            match guard
                .client
                .as_mut()
                .expect("client was just ensured")
                .turn_start(&TurnStartParams {
                    thread_id,
                    input: vec![UserInput::Text {
                        text: prompt_text.to_string(),
                        text_elements: None,
                    }],
                    approval_policy: Some(AskForApproval::Never),
                    sandbox_policy: Some(SandboxPolicy::ReadOnly {
                        network_access: None,
                    }),
                    ..TurnStartParams::default()
                })
                .await
            {
                Ok(response) => {
                    guard.active_turn_id = Some(response.turn.id.clone());
                    return Ok(response.turn.id);
                }
                // The app-server died between turns — reconnect once and
                // resume the thread (AR-5). Only when a thread already
                // exists; a fresh session re-attempts from thread/start.
                Err(CodexError::ServerClosed | CodexError::ConnectionClosed)
                    if attempts == 1 && guard.thread_id.is_some() =>
                {
                    tracing::warn!(
                        session_id = %session_id,
                        provider_id = %self.provider_id,
                        "Codex app-server connection lost; restarting client",
                    );
                    guard.client = None;
                }
                Err(error) => {
                    return Err(HostError::protocol_error(
                        "codex turn start failed",
                        Some(error.to_string()),
                    ));
                }
            }
        }
    }

    /// Build the event stream for an active turn.
    ///
    /// Pumps `AsyncClient::next_message` under the per-session lock (B-2;
    /// only this session's operations contend on it) with a per-frame
    /// timeout (parity with the old stdout line timeout) and maps every
    /// frame through the T1 `map_codex` mapper. Emits at most one terminal
    /// event:
    ///
    /// - `TurnCompleted` with a mapped terminal ends the stream.
    /// - `Ok(None)` at EOF (or the session being torn down by `shutdown()`)
    ///   before a terminal → one `OpFailed(stream_closed)` (PD-3 stream
    ///   abort / stream-abort backstop).
    /// - A crate stream error (`Error::Deserialization` and kin) interrupts
    ///   the server-side turn, drains its leftover terminal (B-1), and
    ///   emits one `OpFailed` from `classify_stream_error` (AR-7).
    /// - A per-frame read timeout interrupts the turn, drains its leftover
    ///   terminal (B-1), and emits one `OpFailed(timeout)`.
    ///
    /// Turn-scoped notifications whose turn id does not match the session's
    /// `active_turn_id` are skipped (B-1): a stale terminal left in the pipe
    /// by an abandoned stream must not end the next turn empty-success.
    /// Frames that map to no host event are skipped and the loop keeps
    /// reading. Residual approval server-requests are auto-answered (AR-4)
    /// and never surface as host events.
    #[allow(clippy::too_many_lines)]
    fn build_event_stream(
        &self,
        op_id: HostOperationId,
        session_id: HostSessionId,
    ) -> HostEventStream {
        let sessions = Arc::clone(&self.sessions);
        let read_timeout = self.timeouts.prompt_duration();

        futures_util::stream::unfold(
            (sessions, op_id, session_id, read_timeout, VecDeque::new(), false),
            |(sessions, op_id, session_id, read_timeout, mut pending, finished)| async move {
                // Drain events mapped from a previous frame first: one
                // frame can carry multiple host events.
                if let Some(event) = pending.pop_front() {
                    return Some((
                        Ok(event),
                        (sessions, op_id, session_id, read_timeout, pending, finished),
                    ));
                }
                if finished {
                    return None;
                }

                // Read frames until one maps to host events or the turn
                // ends (terminal event / stream error / timeout). The read
                // runs under the session's own lock (B-2): other sessions'
                // cancel/shutdown never wait on this frame read.
                loop {
                    let read = async {
                        let state = {
                            let guard = sessions.read().await;
                            guard.get(&session_id).map(|ns| Arc::clone(&ns.state))
                        };
                        let Some(state) = state else {
                            // Session removed by shutdown(): the client was
                            // dropped (app-server killed) — backstop below.
                            return (Ok(None), None);
                        };
                        let mut guard = state.lock().await;
                        let active_turn_id = guard.active_turn_id.clone();
                        let message = match guard.client.as_mut() {
                            Some(client) => client.next_message().await,
                            None => Ok(None),
                        };
                        drop(guard);
                        (message, active_turn_id)
                    };

                    let outcome = tokio::time::timeout(read_timeout, read).await;

                    match outcome {
                        Ok((Ok(Some(ServerMessage::Notification(notification))), active_turn_id)) => {
                            // B-1: a turn-scoped notification from a
                            // previous turn (stale terminal/deltas left in
                            // the pipe by an abandoned stream) must not
                            // drive this stream — keep reading.
                            let stale = notification.turn_id().is_some_and(|turn| {
                                active_turn_id.as_deref() != Some(turn)
                            });
                            if stale {
                                tracing::debug!(
                                    session_id = %session_id,
                                    op_id = %op_id,
                                    turn_id = notification.turn_id(),
                                    active_turn_id = active_turn_id.as_deref(),
                                    "skipping stale codex notification from a previous turn",
                                );
                                continue;
                            }

                            let events = map_codex(
                                &[ServerMessage::Notification(notification)],
                                &session_id,
                                &op_id,
                            );
                            if events.is_empty() {
                                // Skipped frame (unknown method, unmapped
                                // notification) — keep reading.
                                continue;
                            }
                            // map_codex puts the terminal (if any) last.
                            let terminal = matches!(
                                events.last(),
                                Some(HostEvent::OpFinished(_) | HostEvent::OpFailed(_))
                            );
                            if terminal {
                                clear_active_turn(&sessions, &session_id).await;
                            }
                            let mut iter = events.into_iter();
                            let first = iter.next().expect("events is non-empty");
                            pending.extend(iter);
                            return Some((
                                Ok(first),
                                (
                                    sessions,
                                    op_id,
                                    session_id,
                                    read_timeout,
                                    pending,
                                    terminal,
                                ),
                            ));
                        }
                        Ok((Ok(Some(ServerMessage::Request { id, request })), _)) => {
                            // AR-4: never surfaced to the author.
                            let state = {
                                let guard = sessions.read().await;
                                guard.get(&session_id).map(|ns| Arc::clone(&ns.state))
                            };
                            if let Some(state) = state {
                                let mut guard = state.lock().await;
                                if let Some(client) = guard.client.as_mut() {
                                    auto_answer_approval(client, id, &request).await;
                                }
                            }
                        }
                        Ok((Ok(None), _)) => {
                            // Stream abort (EOF or session torn down) before
                            // a terminal frame: exactly one OpFailed (PD-3).
                            clear_active_turn(&sessions, &session_id).await;
                            return Some((
                                Ok(HostEvent::OpFailed(OperationFailedEvent {
                                    session_id: session_id.clone(),
                                    op_id: op_id.clone(),
                                    error_category: "stream_closed".to_string(),
                                    error_message:
                                        "codex app-server closed the stream before the turn completed"
                                            .to_string(),
                                })),
                                (
                                    sessions,
                                    op_id,
                                    session_id,
                                    read_timeout,
                                    pending,
                                    true,
                                ),
                            ));
                        }
                        Ok((Err(error), _)) => {
                            // Typed-decode failure / connection error: the
                            // frame is lost, so the turn fails once (PD-3,
                            // AR-7). The server-side turn is interrupted and
                            // its leftover terminal drained so the next
                            // execute cannot consume it (B-1).
                            interrupt_turn(&sessions, &session_id).await;
                            drain_turn_terminal(&sessions, &session_id).await;
                            clear_active_turn(&sessions, &session_id).await;
                            let failed = classify_stream_error(&error, &session_id, &op_id)
                                .expect("stream error always classifies to a terminal");
                            return Some((
                                Ok(HostEvent::OpFailed(failed)),
                                (sessions, op_id, session_id, read_timeout, pending, true),
                            ));
                        }
                        Err(_elapsed) => {
                            // Per-frame read timeout: stop the turn
                            // server-side (best effort), drain its leftover
                            // terminal (B-1), then fail once with the AR-7
                            // `timeout` token (parity with the old stream
                            // read timeout).
                            interrupt_turn(&sessions, &session_id).await;
                            drain_turn_terminal(&sessions, &session_id).await;
                            clear_active_turn(&sessions, &session_id).await;
                            return Some((
                                Ok(HostEvent::OpFailed(OperationFailedEvent {
                                    session_id: session_id.clone(),
                                    op_id: op_id.clone(),
                                    error_category: "timeout".to_string(),
                                    error_message: format!(
                                        "codex stream read timed out after {read_timeout:?}"
                                    ),
                                })),
                                (sessions, op_id, session_id, read_timeout, pending, true),
                            ));
                        }
                    }
                }
            },
        )
        .boxed()
    }
}

/// Auto-answer a residual codex approval server-request (AR-4).
///
/// Uses the native permission classification: read-only command actions
/// classify `Allow` → accept; everything else (`Ask` / `Deny`) is denied —
/// the headless fail-safe, logged, never surfaced to the author. Failures
/// to respond are logged at warn (the turn already carries
/// `approval_policy: Never`, so these are unexpected belt-and-braces).
async fn auto_answer_approval(client: &mut AsyncClient, id: RequestId, request: &ServerRequest) {
    match request {
        ServerRequest::CmdExecApproval(params) => {
            let read_only = params.command_actions.as_ref().is_some_and(|actions| {
                actions.iter().all(|action| {
                    matches!(
                        action,
                        CommandAction::Read { .. }
                            | CommandAction::ListFiles { .. }
                            | CommandAction::Search { .. }
                    )
                })
            });
            let response = if read_only {
                codex_codes::CommandExecutionRequestApprovalResponse::accept()
            } else {
                codex_codes::CommandExecutionRequestApprovalResponse::decline()
            };
            let decision = if read_only { "accept" } else { "decline" };
            tracing::info!(
                method = request.method(),
                decision,
                "Auto-answering codex command approval (AR-4 native permission policy)"
            );
            if let Err(error) = client.respond(id, &response).await {
                tracing::warn!(error = %error, "Failed to answer codex approval request");
            }
        }
        ServerRequest::FileChangeApproval(_) => {
            tracing::info!(
                method = request.method(),
                decision = "decline",
                "Auto-denying codex file-change approval (AR-4 read-only sandbox)"
            );
            if let Err(error) = client
                .respond(
                    id,
                    &codex_codes::FileChangeRequestApprovalResponse::decline(),
                )
                .await
            {
                tracing::warn!(error = %error, "Failed to answer codex approval request");
            }
        }
        ServerRequest::ApplyPatchApproval(_) => {
            tracing::info!(
                method = request.method(),
                decision = "deny",
                "Auto-denying codex apply-patch approval (AR-4 read-only sandbox)"
            );
            if let Err(error) = client
                .respond(
                    id,
                    &codex_codes::ApplyPatchApprovalResponse::denied(
                        "denied by nexus read-only policy",
                    ),
                )
                .await
            {
                tracing::warn!(error = %error, "Failed to answer codex approval request");
            }
        }
        ServerRequest::ExecCommandApproval(_) => {
            tracing::info!(
                method = request.method(),
                decision = "deny",
                "Auto-denying codex exec approval (AR-4 read-only sandbox)"
            );
            if let Err(error) = client
                .respond(
                    id,
                    &codex_codes::ExecCommandApprovalResponse::denied(
                        "denied by nexus read-only policy",
                    ),
                )
                .await
            {
                tracing::warn!(error = %error, "Failed to answer codex approval request");
            }
        }
        // Permission/user-input/tool/auth requests are not approvals we can
        // classify — deny with a JSON-RPC error (headless fail-safe).
        _ => {
            tracing::info!(
                method = request.method(),
                decision = "deny",
                "Auto-denying unclassified codex server request (AR-4 headless fail-safe)"
            );
            if let Err(error) = client
                .respond_error(id, -32000, "denied by nexus headless policy")
                .await
            {
                tracing::warn!(error = %error, "Failed to answer codex server request");
            }
        }
    }
}

/// Interrupt the session's active turn via the crate client, if any.
///
/// Best-effort: a turn that already completed server-side makes
/// `turn/interrupt` fail with a JSON-RPC error, which is logged only. Runs
/// under the per-session lock (B-2); only this session's in-flight frame
/// read (bounded by the read timeout) can delay it.
async fn interrupt_turn(
    sessions: &Arc<RwLock<HashMap<HostSessionId, NativeSession>>>,
    session_id: &HostSessionId,
) {
    let state = {
        let guard = sessions.read().await;
        guard.get(session_id).map(|ns| Arc::clone(&ns.state))
    };
    let Some(state) = state else { return };
    let mut guard = state.lock().await;
    let (Some(thread_id), Some(turn_id)) = (guard.thread_id.clone(), guard.active_turn_id.clone())
    else {
        return;
    };
    if let Some(client) = guard.client.as_mut() {
        if let Err(error) = client
            .turn_interrupt(&TurnInterruptParams { thread_id, turn_id })
            .await
        {
            tracing::warn!(
                session_id = %session_id,
                error = %error,
                "Codex turn interrupt failed (turn may have completed)"
            );
        }
    }
}

/// Clear the session's active-turn marker (stream ended).
async fn clear_active_turn(
    sessions: &Arc<RwLock<HashMap<HostSessionId, NativeSession>>>,
    session_id: &HostSessionId,
) {
    let state = {
        let guard = sessions.read().await;
        guard.get(session_id).map(|ns| Arc::clone(&ns.state))
    };
    let Some(state) = state else { return };
    let mut guard = state.lock().await;
    guard.active_turn_id = None;
}

/// Budget for draining an interrupted turn's leftover terminal (B-1).
///
/// The app-server answers `turn/interrupt` with `turn/completed`; that frame
/// must not linger in the stream for the next execute to consume. Distinct
/// from the prompt-setup budget: it bounds only the interrupt response wait.
const TURN_DRAIN_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);

/// Best-effort drain: after interrupting the active turn, keep reading
/// frames until that turn's terminal arrives (or the budget / EOF / stream
/// error ends the drain), so the next execute cannot consume a leftover
/// terminal from this turn (B-1). Runs under the per-session lock; a
/// concurrent execute's setup is serialized behind the drain.
async fn drain_turn_terminal(
    sessions: &Arc<RwLock<HashMap<HostSessionId, NativeSession>>>,
    session_id: &HostSessionId,
) {
    let state = {
        let guard = sessions.read().await;
        guard.get(session_id).map(|ns| Arc::clone(&ns.state))
    };
    let Some(state) = state else { return };
    let mut guard = state.lock().await;
    let Some(active_turn_id) = guard.active_turn_id.clone() else {
        return;
    };
    let deadline = tokio::time::Instant::now() + TURN_DRAIN_BUDGET;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return;
        }
        let outcome = tokio::time::timeout(remaining, async {
            match guard.client.as_mut() {
                Some(client) => client.next_message().await,
                None => Ok(None),
            }
        })
        .await;
        match outcome {
            Ok(Ok(Some(ServerMessage::Notification(notification)))) => {
                // The interrupted turn's terminal ends the drain.
                if matches!(
                    &notification,
                    Notification::TurnCompleted(completed)
                        if completed.turn.id == active_turn_id
                ) {
                    return;
                }
                // Any other leftover frame from the interrupted turn is
                // noise — keep draining.
            }
            // Leftover approval request: the turn is being torn down, the
            // interrupt response follows; nothing to answer.
            Ok(Ok(Some(ServerMessage::Request { .. }))) => {}
            // EOF / stream error / budget elapsed: nothing more to drain.
            Ok(Ok(None) | Err(_)) | Err(_) => {
                drop(guard);
                return;
            }
        }
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
        // Native provider launch only registers session state — the
        // app-server process starts lazily on the first execute().
        let host_session_id = HostSessionId::new();

        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(
                host_session_id.clone(),
                NativeSession {
                    state: Arc::new(Mutex::new(ClientState {
                        client: None,
                        thread_id: None,
                        active_turn_id: None,
                    })),
                    cwd: spec.cwd.clone(),
                },
            );
        }

        tracing::info!(
            session_id = %host_session_id,
            provider_id = %self.provider_id,
            cwd = %spec.cwd.display(),
            "Native CLI session registered (app-server starts on first execute)"
        );

        Ok(ManagedSessionHandle {
            provider_id: self.provider_id.clone(),
            session_id: host_session_id,
            capabilities: CapabilityDescriptor::native_cli_limited(),
        })
    }

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

        // Setup phase (client + thread + turn/start) runs under the prompt
        // timeout, mirroring the old spawn + stdin-write enforcement. A
        // failure returns an error, not a stream.
        let turn_id = tokio::time::timeout(
            self.timeouts.prompt_duration(),
            self.ensure_client_and_start_turn(&session.session_id, &prompt_text),
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
        })?
        .map_err(|e| {
            e.with_provider(self.provider_id.clone())
                .with_session(session.session_id.clone())
                .with_op(op_id.clone())
        })?;

        tracing::info!(
            session_id = %session.session_id,
            provider_id = %self.provider_id,
            op_id = %op_id,
            turn_id = %turn_id,
            "Codex turn started"
        );

        Ok(self.build_event_stream(op_id, session.session_id.clone()))
    }

    async fn cancel(
        &self,
        session: &ManagedSessionHandle,
        op_id: HostOperationId,
    ) -> HostResult<()> {
        // The app-server owns the turn; interrupt it through the crate
        // client under the per-session lock (B-2: only this session's frame
        // read can delay it). The app-server reports `Interrupted`, which
        // maps to a clean `OpFinished(EndTurn)` on the stream (AR-1).
        interrupt_turn(&self.sessions, &session.session_id).await;
        tracing::info!(
            session_id = %session.session_id,
            op_id = %op_id,
            provider_id = %self.provider_id,
            "Native CLI cancel: turn interrupt requested"
        );
        Ok(())
    }

    async fn shutdown(&self, session: ManagedSessionHandle) -> HostResult<()> {
        // Removing the session drops the crate client once no in-flight
        // frame read holds it, which kills the app-server process (AsyncClient
        // teardown owns the child — no raw Child map, no ChildReaper). An
        // in-flight read releases the per-session lock when it returns
        // (bounded by the read timeout), then the last state Arc drops the
        // client.
        let mut sessions = self.sessions.write().await;
        sessions.remove(&session.session_id);
        drop(sessions);

        tracing::info!(
            session_id = %session.session_id,
            provider_id = %self.provider_id,
            "Native CLI session shut down (app-server killed via crate client teardown)"
        );
        Ok(())
    }

    fn capabilities(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::native_cli_limited()
    }
}

#[cfg(test)]
mod tests {
    // Lock guards (session registry / client) are intentionally held to the
    // end of the visible test scope for readability; the nursery
    // significant_drop_tightening suggestion to drop them earlier is noise
    // here.
    #![allow(clippy::significant_drop_tightening)]

    use super::*;
    use crate::capability::model::{FinishReason, HostOperation, LaunchSpec};

    const MOCK_APP_SERVER: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_protocol/mock_codex_app_server.py"
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

    /// Provider wired to the fixture mock app-server (a small Python
    /// JSON-RPC speaker; see `MOCK_APP_SERVER`).
    fn mock_provider(env: HashMap<String, String>) -> CodexNativeProvider {
        CodexNativeProvider::new(
            ProviderId::new("test-codex-app-server"),
            "Test".to_string(),
            MOCK_APP_SERVER.to_string(),
            env,
            TimeoutConfig::default(),
        )
    }

    async fn launch_and_execute(
        provider: &CodexNativeProvider,
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
            "native CLI supports session_restore via the app-server thread (AR-5)"
        );
        assert!(!desc.capabilities.mcp_http);
    }

    #[test]
    fn default_config_command() {
        let provider = CodexNativeProvider::default_config();
        assert_eq!(provider.command, "codex");
    }

    #[tokio::test]
    async fn probe_unavailable_when_command_not_found() {
        let provider = CodexNativeProvider::new(
            ProviderId::new("nonexistent-codex-xyz"),
            "Fake".to_string(),
            "nonexistent_codex_xyz_12345".to_string(),
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
            HashMap::from([("OPENAI_API_KEY".to_string(), "sk-test".to_string())]),
            TimeoutConfig::default(),
        );

        assert_eq!(provider.provider_id.0, "my-codex");
        assert_eq!(provider.command, "/opt/codex/bin/codex");
        assert_eq!(provider.env.get("OPENAI_API_KEY").unwrap(), "sk-test");
    }

    #[tokio::test]
    async fn launch_registers_session_without_client() {
        let provider = CodexNativeProvider::default_config();

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
            state.thread_id.is_none(),
            "thread_id should be None before first execute"
        );
        assert!(
            state.active_turn_id.is_none(),
            "active_turn_id should be None before first execute"
        );
    }

    #[tokio::test]
    async fn execute_fails_when_binary_missing() {
        let provider = CodexNativeProvider::new(
            ProviderId::new("test-codex-missing"),
            "Test".to_string(),
            "nonexistent_codex_xyz_12345".to_string(),
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

    /// A turn against the mock app-server maps through `map_codex`:
    /// `OpStarted` (turn/started), `MessageDelta`, one `OpFinished(EndTurn)`
    /// (turn/completed), and the client/thread survive for later turns.
    #[tokio::test]
    async fn execute_maps_turn_events_and_keeps_client() {
        let provider = mock_provider(HashMap::new());

        let (handle, stream) = launch_and_execute(&provider, "hi").await;
        let events = collect_events(stream).await;

        assert!(
            matches!(&events[0], HostEvent::OpStarted(_)),
            "first event should be OpStarted: {events:?}"
        );
        assert!(
            matches!(&events[1], HostEvent::MessageDelta(d) if d.text == "hello from mock codex"),
            "delta must map through the T1 mapper: {events:?}"
        );
        assert!(
            matches!(&events[2], HostEvent::OpFinished(f) if f.reason == FinishReason::EndTurn),
            "completed turn must end cleanly: {events:?}"
        );
        assert_eq!(events.len(), 3, "started + delta + terminal: {events:?}");
        assert_eq!(terminal_count(&events), 1);

        let sessions = provider.sessions.read().await;
        let ns = sessions.get(&handle.session_id).expect("session exists");
        let state = ns.state.lock().await;
        assert!(
            state.client.is_some(),
            "client must stay alive after the turn"
        );
        assert_eq!(
            state.thread_id.as_deref(),
            Some("mock-thread-1"),
            "thread id must be captured from thread/start"
        );
        assert!(
            state.active_turn_id.is_none(),
            "active turn must be cleared when the stream ends"
        );
    }

    /// Session restore (AR-5): the second execute reuses the app-server
    /// thread — no second `thread/start`, and both turns run on
    /// `mock-thread-1`.
    #[tokio::test]
    async fn second_execute_reuses_thread() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let req_log = temp_dir.path().join("requests.jsonl");
        let req_log_path = req_log.to_string_lossy().into_owned();
        let provider = mock_provider(HashMap::from([("REQ_LOG".to_string(), req_log_path)]));

        let handle = provider.launch(launch_spec()).await.expect("launch");

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

        let log = std::fs::read_to_string(&req_log).expect("read request log");
        let requests: Vec<serde_json::Value> = log
            .lines()
            .map(|line| serde_json::from_str(line).expect("log line is JSON"))
            .collect();

        let thread_starts = requests
            .iter()
            .filter(|r| r["method"] == "thread/start")
            .count();
        assert_eq!(thread_starts, 1, "thread must be created exactly once");
        let turn_starts: Vec<&serde_json::Value> = requests
            .iter()
            .filter(|r| r["method"] == "turn/start")
            .collect();
        assert_eq!(turn_starts.len(), 2, "one turn/start per execute");
        assert!(
            turn_starts.iter().all(|r| r["threadId"] == "mock-thread-1"),
            "both turns must run on the reused thread: {requests:?}"
        );
    }

    /// `cancel()` interrupts the running turn through the crate client; the
    /// app-server reports `Interrupted`, which maps to a clean
    /// `OpFinished(EndTurn)` (AR-1).
    #[tokio::test]
    async fn cancel_interrupts_active_turn() {
        let provider = mock_provider(HashMap::from([("BLOCK_TURN".to_string(), "1".to_string())]));

        let (handle, stream) = launch_and_execute(&provider, "hi").await;
        let events_fut = Box::pin(collect_events(stream));

        provider
            .cancel(&handle, HostOperationId::new())
            .await
            .expect("cancel");

        let events = events_fut.await;
        assert!(
            matches!(&events[0], HostEvent::OpStarted(_)),
            "turn must have started: {events:?}"
        );
        assert!(
            matches!(
                &events.last().unwrap(),
                HostEvent::OpFinished(f) if f.reason == FinishReason::EndTurn
            ),
            "interrupted turn must end cleanly: {events:?}"
        );
        assert_eq!(terminal_count(&events), 1);
    }

    /// `shutdown()` tears down the crate client (killing the app-server) and
    /// removes the session; the in-flight stream still emits exactly one
    /// terminal — `OpFailed(stream_closed)` (PD-3 backstop).
    #[tokio::test]
    async fn shutdown_tears_down_client_and_stream_terminates() {
        let provider = mock_provider(HashMap::from([("BLOCK_TURN".to_string(), "1".to_string())]));

        let (handle, stream) = launch_and_execute(&provider, "hi").await;
        let mut stream = stream;

        // Pump the first event so the turn is visibly streaming.
        let first = stream.next().await.expect("first event").expect("ok");
        assert!(matches!(first, HostEvent::OpStarted(_)));

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

    #[tokio::test]
    async fn cancel_with_no_active_turn_is_noop() {
        let provider = mock_provider(HashMap::new());
        let handle = provider.launch(launch_spec()).await.expect("launch");

        provider
            .cancel(&handle, HostOperationId::new())
            .await
            .expect("cancel with no active turn must succeed");
    }

    /// B-1 regression: a `turn/completed` left in the pipe from a PREVIOUS
    /// turn must not terminate the new turn's stream (empty-success). The
    /// mock emits the stale terminal for `mock-turn-1` before the second
    /// turn's own frames; the stream must skip it and read only its own
    /// turn's events.
    #[tokio::test]
    async fn stale_turn_terminal_from_previous_turn_does_not_end_new_stream() {
        let provider = mock_provider(HashMap::from([(
            "STALE_TURN_COMPLETED".to_string(),
            "1".to_string(),
        )]));

        let handle = provider.launch(launch_spec()).await.expect("launch");

        let stream1 = provider
            .execute(
                &handle,
                HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "first".to_string(),
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
                        text: "second".to_string(),
                    }],
                },
            )
            .await
            .expect("second execute");
        let events2 = collect_events(stream2).await;

        assert!(
            matches!(&events2[0], HostEvent::OpStarted(_)),
            "new turn must start, not terminate on the stale terminal: {events2:?}"
        );
        assert!(
            matches!(&events2[1], HostEvent::MessageDelta(d) if d.text == "hello from mock codex"),
            "new turn must stream its own delta: {events2:?}"
        );
        assert!(
            matches!(&events2[2], HostEvent::OpFinished(f) if f.reason == FinishReason::EndTurn),
            "new turn must end on its own terminal: {events2:?}"
        );
        assert_eq!(events2.len(), 3, "started + delta + terminal: {events2:?}");
        assert_eq!(terminal_count(&events2), 1);
    }

    /// B-1: a typed-decode failure interrupts the server-side turn and
    /// drains its leftover terminal, so the next execute starts a clean
    /// turn instead of consuming the interrupted turn's `turn/completed`.
    #[tokio::test]
    async fn decode_error_interrupts_and_drains_turn() {
        let provider = mock_provider(HashMap::from([("BAD_FRAME".to_string(), "1".to_string())]));

        let handle = provider.launch(launch_spec()).await.expect("launch");

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
        assert!(
            matches!(
                events1.last(),
                Some(HostEvent::OpFailed(f)) if f.error_category == "decode_error"
            ),
            "bad frame must fail the turn once with decode_error: {events1:?}"
        );
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
        assert!(
            matches!(&events2[0], HostEvent::OpStarted(_)),
            "new turn must start cleanly: {events2:?}"
        );
        assert!(
            matches!(&events2[1], HostEvent::MessageDelta(d) if d.text == "hello from mock codex"),
            "new turn must stream its own delta: {events2:?}"
        );
        assert!(
            matches!(&events2[2], HostEvent::OpFinished(f) if f.reason == FinishReason::EndTurn),
            "new turn must end on its own terminal: {events2:?}"
        );
        assert_eq!(events2.len(), 3, "started + delta + terminal: {events2:?}");
        assert_eq!(terminal_count(&events2), 1);
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

    #[tokio::test]
    async fn shutdown_removes_session() {
        let provider = CodexNativeProvider::default_config();

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
