//! `DeepSeek Harness` native provider adapter (`deepseek-harness-sdk`).
//!
//! Implements a native provider for the `dsh` runtime through the SDK's
//! `Python`-parity high-level surface (AR-2): ONE [`DeepSeekHarness`] per
//! provider session, started lazily on the first `execute()` and closed on
//! `shutdown()`; each execute runs `start_session(Some(host_generated_id))`
//! and `Session::run(Input::Text)` wrapped in `tokio::time::timeout` (the
//! SDK's inbox-receipt / root-idle waits are unbounded by design). Nexus
//! owns only the `HostEvent` normalization (`map_dsh`); the crate owns the
//! runtime spawn, the stdio JSON-RPC wire parser, and the close ladder.
//!
//! # Session Model
//!
//! Each `launch()` registers a host session with a host-generated DSH
//! session id and no harness yet. The first `execute()` lazily starts the
//! harness and runs on `start_session(Some(id))`; a session id unknown to
//! the runtime lazily creates the agent+session pair, and reusing the id
//! across executes resumes the conversation (AR-2/AR-5 —
//! `session_restore` stays honest). The harness lives until `shutdown()`,
//! which runs the SDK close ladder ([`DeepSeekHarness::close`]).
//!
//! # No streaming, no cancel (AR-6)
//!
//! `Session::run` returns only after the root session reports idle and
//! `final_response` is derived from the last `assistant/message` event —
//! the SDK has no incremental delta API and no cancel / session-close RPC.
//! `dsh-native` therefore ships the documented narrower descriptor
//! [`CapabilityDescriptor::dsh_limited`] (`streaming: false`,
//! `cancellation: false`) and `cancel()` is an honest no-op: killing the
//! runtime mid-turn would abandon the turn, not cancel it.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use async_trait::async_trait;
use deepseek_harness_sdk::{Config, DeepSeekHarness, Error as DshError, Input};
use futures_util::StreamExt;
use tokio::sync::{Mutex, RwLock};

use crate::capability::model::{
    CapabilityDescriptor, HostContentBlock, HostEvent, HostEventStream, ManagedSessionHandle,
    OperationFailedEvent, ProtocolKind, ProviderDescriptor, ProviderHealth,
};
use crate::config::TimeoutConfig;
use crate::error::{HostError, HostResult};
use crate::ids::{HostOperationId, HostSessionId, ProviderId};
use crate::providers::native_cli::map_dsh::{classify_run_error, map_run_result};
use crate::ProviderAdapter;

/// Crate-client-scoped state for a managed dsh session, guarded by the
/// per-session mutex (B-2): only this session's operations contend on it —
/// cancel/shutdown of other sessions never wait on this session's run. The
/// provider-global registry `RwLock` is only for short lookups.
struct ClientState {
    /// Host-generated DSH session id (AR-2): `start_session(Some(id))`
    /// reuses it across executes — the runtime lazily creates the
    /// agent+session pair on the first run and resumes it afterwards.
    dsh_session_id: String,
    /// The lazily-started harness (one per provider session); `None` until
    /// the first execute. Closed by `shutdown()` via the SDK close ladder.
    harness: Option<DeepSeekHarness>,
    /// Set by `shutdown()` under the lock so a run that already cloned the
    /// state can never start a fresh harness after the close.
    closed: bool,
}

/// Internal state for a managed dsh native session.
struct NativeSession {
    /// Per-session lock around the crate harness and session metadata (B-2).
    state: Arc<Mutex<ClientState>>,
    /// Working directory for the runtime, retained from `LaunchSpec::cwd`
    /// (N-1).
    cwd: std::path::PathBuf,
}

impl std::fmt::Debug for NativeSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("NativeSession");
        debug.field("cwd", &self.cwd.display());
        match self.state.try_lock() {
            Ok(state) => {
                debug
                    .field("dsh_session_id", &state.dsh_session_id)
                    .field("harness_started", &state.harness.is_some())
                    .field("closed", &state.closed);
            }
            Err(_) => {
                debug.field("client_state", &"<locked>");
            }
        }
        debug.finish()
    }
}

/// `DeepSeek Harness` native provider.
///
/// Spawns the `dsh` runtime (bring-your-own; PATH command
/// `dsh-jsonrpc-agent` or `DSH_RUNTIME_BIN`) via the
/// `deepseek-harness-sdk` crate and normalizes each `Session::run` outcome
/// into `HostEvent` items. Multi-turn continuity is the host-generated DSH
/// session id reused via `start_session(Some(id))` (AR-2/AR-5).
pub struct DshNativeProvider {
    /// Provider ID (typically `dsh-native` to avoid collision with ACP registry).
    provider_id: ProviderId,
    /// Display name.
    display_name: String,
    /// Runtime binary handed to the SDK as `Config::runtime_bin`. `Some`
    /// with a resolved absolute path when discovery found the command on
    /// PATH; `Some` with a bare command name spawns it via PATH; `None`
    /// leaves the SDK's own resolution to `DSH_RUNTIME_BIN` (its
    /// `resolve_runtime` order 3: `launch_args_override` → `runtime_bin` →
    /// `DSH_RUNTIME_BIN` → `RuntimeNotFound`).
    runtime_bin: Option<String>,
    /// Environment variables to inject into the runtime process.
    env: HashMap<String, String>,
    /// Active sessions: host session ID → native session state.
    sessions: Arc<RwLock<HashMap<HostSessionId, NativeSession>>>,
    /// Timeout configuration for stage-level enforcement.
    timeouts: TimeoutConfig,
}

impl DshNativeProvider {
    /// Create a new `DeepSeek Harness` provider with the given configuration.
    #[must_use]
    pub fn new(
        provider_id: ProviderId,
        display_name: String,
        runtime_bin: Option<String>,
        env: HashMap<String, String>,
        timeouts: TimeoutConfig,
    ) -> Self {
        Self {
            provider_id,
            display_name,
            runtime_bin,
            env,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            timeouts,
        }
    }

    /// Create with default configuration for the `dsh-jsonrpc-agent` runtime.
    ///
    /// The bare command name is handed to the SDK as `Config::runtime_bin`,
    /// which spawns it via PATH resolution.
    #[must_use]
    pub fn default_config() -> Self {
        Self::new(
            ProviderId::new("dsh-native"),
            "DeepSeek Harness (native)".to_string(),
            Some("dsh-jsonrpc-agent".to_string()),
            HashMap::new(),
            TimeoutConfig::default(),
        )
    }

    /// Register-time constructor used by daemon boot (T2): `runtime_bin` is
    /// the boot-resolved absolute path when discovery found
    /// `dsh-jsonrpc-agent` on PATH, or `None` when discovery was via the
    /// `DSH_RUNTIME_BIN` env var only — the SDK then resolves the env var
    /// itself (resolution order 3 of its `resolve_runtime` chain), which
    /// keeps the two discovery routes consistent at spawn time.
    #[must_use]
    pub fn with_runtime_bin(runtime_bin: Option<String>) -> Self {
        Self::new(
            ProviderId::new("dsh-native"),
            "DeepSeek Harness (native)".to_string(),
            runtime_bin,
            HashMap::new(),
            TimeoutConfig::default(),
        )
    }

    /// Build the event stream for one turn.
    ///
    /// One turn = one `Session::run` on the session's harness, driven
    /// lazily on the first stream poll under the per-session lock (B-2;
    /// only this session's operations contend on it). The run is wrapped in
    /// `tokio::time::timeout(prompt_duration)` because the SDK's
    /// inbox-receipt / root-idle waits are unbounded by design (P2 plan;
    /// the crate documents the wrapper as the caller's bound). Emits at
    /// most one terminal event:
    ///
    /// - `Ok(RunResult)` → exactly one `MessageDelta(final_response)` + one
    ///   `OpFinished` (AR-1 dsh table / AR-6).
    /// - `tokio::time::error::Elapsed` → one `OpFailed(timeout)`.
    /// - `Err(DshError)` → one `OpFailed` from `classify_run_error` (AR-7).
    /// - Session removed/closed by `shutdown()` before the run starts →
    ///   one `OpFailed(stream_closed)` (stream-abort backstop).
    ///
    /// The harness is started lazily on the first poll (and only then) and
    /// stays alive for the session until `shutdown()` closes it.
    #[allow(clippy::too_many_lines)]
    fn build_event_stream(
        &self,
        op_id: HostOperationId,
        session_id: HostSessionId,
        prompt_text: String,
        cwd: std::path::PathBuf,
    ) -> HostEventStream {
        let sessions = Arc::clone(&self.sessions);
        let provider_id = self.provider_id.clone();
        let runtime_bin = self.runtime_bin.clone();
        let env = self.env.clone();
        let run_timeout = self.timeouts.prompt_duration();
        let prompt_ms = self.timeouts.prompt_ms;

        futures_util::stream::unfold(
            (
                sessions,
                provider_id,
                runtime_bin,
                env,
                op_id,
                session_id,
                prompt_text,
                cwd,
                run_timeout,
                prompt_ms,
                VecDeque::new(),
                false,
            ),
            |(
                sessions,
                provider_id,
                runtime_bin,
                env,
                op_id,
                session_id,
                prompt_text,
                cwd,
                run_timeout,
                prompt_ms,
                mut pending,
                done,
            )| async move {
                // Drain mapped events from the turn first: one successful
                // run maps to a MessageDelta + a terminal.
                if let Some(event) = pending.pop_front() {
                    return Some((
                        Ok(event),
                        (
                            sessions,
                            provider_id,
                            runtime_bin,
                            env,
                            op_id,
                            session_id,
                            prompt_text,
                            cwd,
                            run_timeout,
                            prompt_ms,
                            pending,
                            done,
                        ),
                    ));
                }
                if done {
                    return None;
                }

                // One turn = one Session::run under the per-session lock
                // (B-2): other sessions' cancel/shutdown never wait on this
                // run. The run coroutine takes clones: the tuple state must
                // survive for the terminal-event return after the run.
                // The `Session` borrows the harness, which borrows the
                // guard, so the guard must live across the whole run await;
                // the significant_drop_tightening suggestion to drop it
                // early is a false positive here (the borrow checker
                // rejects it).
                #[allow(clippy::significant_drop_tightening)]
                let run = {
                    let sessions = Arc::clone(&sessions);
                    let session_id = session_id.clone();
                    let provider_id = provider_id.clone();
                    let prompt_text = prompt_text.clone();
                    let cwd = cwd.clone();
                    let runtime_bin = runtime_bin.clone();
                    let env = env.clone();
                    async move {
                        let state = {
                            let guard = sessions.read().await;
                            guard.get(&session_id).map(|ns| Arc::clone(&ns.state))
                        };
                        let Some(state) = state else {
                            // Session removed by shutdown(): no harness to
                            // run — the stream-abort backstop.
                            return Err(RunError::SessionGone);
                        };
                        let mut guard = state.lock().await;
                        if guard.closed {
                            // shutdown() closed this session between the
                            // registry read and the lock: never start a
                            // fresh harness.
                            return Err(RunError::SessionGone);
                        }
                        if guard.harness.is_none() {
                            // AR-2: lazy start, one harness per provider
                            // session. The runtime handoff (T2): a
                            // PATH-discovered provider carries the
                            // boot-resolved absolute path here; an
                            // env-only provider carries `None`, so the SDK
                            // resolves DSH_RUNTIME_BIN itself (order 3).
                            let config = Config {
                                cwd: Some(cwd),
                                runtime_bin: runtime_bin.clone(),
                                env: Some(env),
                                ..Config::default()
                            };
                            let harness = DeepSeekHarness::start(config)
                                .await
                                .map_err(RunError::Sdk)?;
                            tracing::info!(
                                session_id = %session_id,
                                provider_id = %provider_id,
                                "dsh harness started lazily",
                            );
                            guard.harness = Some(harness);
                        }
                        let dsh_session_id = guard.dsh_session_id.clone();
                        let harness = guard.harness.as_mut().expect("harness was just ensured");
                        let session = harness.start_session(Some(dsh_session_id));
                        session
                            .run(Input::Text(prompt_text))
                            .await
                            .map_err(RunError::Sdk)
                    }
                };

                let events =
                    match tokio::time::timeout(run_timeout, run).await {
                        Ok(Ok(result)) => map_run_result(&result, &session_id, &op_id),
                        Ok(Err(RunError::SessionGone)) => {
                            vec![HostEvent::OpFailed(OperationFailedEvent {
                                session_id: session_id.clone(),
                                op_id: op_id.clone(),
                                error_category: "stream_closed".to_string(),
                                error_message: "dsh session closed before the turn completed"
                                    .to_string(),
                            })]
                        }
                        Ok(Err(RunError::Sdk(error))) => vec![HostEvent::OpFailed(
                            classify_run_error(&error, &session_id, &op_id),
                        )],
                        Err(_elapsed) => vec![HostEvent::OpFailed(OperationFailedEvent {
                            session_id: session_id.clone(),
                            op_id: op_id.clone(),
                            error_category: "timeout".to_string(),
                            error_message: format!("dsh turn timed out after {prompt_ms}ms"),
                        })],
                    };
                let mut iter = events.into_iter();
                let first = iter.next().expect("the turn maps to at least one event");
                pending.extend(iter);
                Some((
                    Ok(first),
                    (
                        sessions,
                        provider_id,
                        runtime_bin,
                        env,
                        op_id,
                        session_id,
                        prompt_text,
                        cwd,
                        run_timeout,
                        prompt_ms,
                        pending,
                        true,
                    ),
                ))
            },
        )
        .boxed()
    }
}

/// Failure of the turn's `Session::run` attempt.
enum RunError {
    /// Session removed or closed by `shutdown()` before the turn could run.
    SessionGone,
    /// SDK error from the lazy harness start or `Session::run`.
    Sdk(DshError),
}

#[async_trait]
impl ProviderAdapter for DshNativeProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            provider_id: self.provider_id.clone(),
            display_name: self.display_name.clone(),
            protocol_kind: ProtocolKind::NativeCli,
            capabilities: CapabilityDescriptor::dsh_limited(),
        }
    }

    async fn probe(
        &self,
        _request: crate::capability::model::ProbeRequest,
    ) -> HostResult<ProviderHealth> {
        // Discovery (PD-4): the PATH command `dsh-jsonrpc-agent` OR the
        // `DSH_RUNTIME_BIN` env var (non-empty; the SDK treats empty as
        // absent). Cross-platform command lookup via the `which` crate, in
        // spawn_blocking under the launch timeout (same shape as
        // claude/codex). The lookup runs against the configured runtime
        // binary — a resolved absolute path from PATH discovery, or a bare
        // command name — and is skipped entirely when the provider was
        // registered via `DSH_RUNTIME_BIN` only (`runtime_bin` is `None`;
        // the env check below decides availability).
        let runtime_bin = self.runtime_bin.clone();
        let runtime_bin_for_msg = runtime_bin.clone();
        let provider_id = self.provider_id.clone();
        let launch_dur = self.timeouts.launch_duration();

        let which_result = tokio::time::timeout(
            launch_dur,
            tokio::task::spawn_blocking(move || {
                runtime_bin
                    .as_ref()
                    .map_or(Ok(None), |bin| which::which(bin).map(Some))
            }),
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

        let env_route = std::env::var_os("DSH_RUNTIME_BIN").is_some_and(|value| !value.is_empty());

        let health = match (which_result, env_route) {
            (Ok(Ok(Some(resolved_path))), _) => ProviderHealth {
                provider_id,
                available: true,
                latency_ms: None,
                message: Some(resolved_path.to_string_lossy().into_owned()),
            },
            (_, true) => ProviderHealth {
                provider_id,
                available: true,
                latency_ms: None,
                message: Some("DSH_RUNTIME_BIN is set".to_string()),
            },
            _ => {
                let bin = runtime_bin_for_msg
                    .as_deref()
                    .unwrap_or("(none configured)");
                ProviderHealth {
                    provider_id,
                    available: false,
                    latency_ms: None,
                    message: Some(format!(
                        "runtime binary '{bin}' not found on PATH and DSH_RUNTIME_BIN unset"
                    )),
                }
            }
        };
        Ok(health)
    }

    async fn launch(
        &self,
        spec: crate::capability::model::LaunchSpec,
    ) -> HostResult<ManagedSessionHandle> {
        // Native provider launch only registers session state — the runtime
        // spawns lazily on the first execute() (AR-2).
        let host_session_id = HostSessionId::new();

        // Host-generated DSH session id: start_session(Some(id)) reuses it
        // across executes; the runtime lazily creates the agent+session
        // pair on the first run and resumes it afterwards (AR-2/AR-5).
        let dsh_session_id = uuid::Uuid::new_v4().to_string();

        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(
                host_session_id.clone(),
                NativeSession {
                    state: Arc::new(Mutex::new(ClientState {
                        dsh_session_id,
                        harness: None,
                        closed: false,
                    })),
                    cwd: spec.cwd.clone(),
                },
            );
        }

        tracing::info!(
            session_id = %host_session_id,
            provider_id = %self.provider_id,
            cwd = %spec.cwd.display(),
            "dsh session registered (runtime spawns on first execute)"
        );

        Ok(ManagedSessionHandle {
            provider_id: self.provider_id.clone(),
            session_id: host_session_id,
            capabilities: CapabilityDescriptor::dsh_limited(),
        })
    }

    // One turn is a single Session::run driven by the event stream — no
    // setup phase to split out; allow the line count here.
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

        // Clone the session working directory under a short registry read
        // (B-2, N-1); the run then proceeds under the per-session lock
        // only, never the provider-global one.
        let cwd = {
            let sessions = self.sessions.read().await;
            let native_session = sessions.get(&session.session_id).ok_or_else(|| {
                HostError::internal(format!(
                    "session {} not found in native CLI provider",
                    session.session_id
                ))
            })?;
            let cwd = native_session.cwd.clone();
            drop(sessions);
            cwd
        };

        tracing::info!(
            session_id = %session.session_id,
            provider_id = %self.provider_id,
            op_id = %op_id,
            "dsh turn started"
        );

        Ok(self.build_event_stream(op_id, session.session_id.clone(), prompt_text, cwd))
    }

    async fn cancel(
        &self,
        session: &ManagedSessionHandle,
        op_id: HostOperationId,
    ) -> HostResult<()> {
        // AR-6: the SDK exposes no cancel / session-close RPC, and killing
        // the runtime mid-turn would abandon the turn (documented crate
        // non-goal). Honest no-op: the turn runs to completion (or the
        // turn timeout) and the terminal still arrives on the stream.
        tracing::info!(
            session_id = %session.session_id,
            op_id = %op_id,
            provider_id = %self.provider_id,
            "dsh cancel: no-op (the SDK has no cancel RPC; AR-6)",
        );
        Ok(())
    }

    async fn shutdown(&self, session: ManagedSessionHandle) -> HostResult<()> {
        // Removing the session first stops new executes; the per-session
        // lock is then awaited (an in-flight run holds it — bounded by the
        // turn timeout) and the harness is closed via the SDK close ladder,
        // which reaps the child even when a ladder tier reports an error.
        let removed = {
            let mut sessions = self.sessions.write().await;
            sessions.remove(&session.session_id)
        };
        if let Some(native_session) = removed {
            let mut guard = native_session.state.lock().await;
            guard.closed = true;
            if let Some(mut harness) = guard.harness.take() {
                if let Err(error) = harness.close().await {
                    tracing::warn!(
                        session_id = %session.session_id,
                        provider_id = %self.provider_id,
                        error = %error,
                        "dsh close ladder reported an error; the runtime \
                         child is still reaped",
                    );
                }
            }
        }

        tracing::info!(
            session_id = %session.session_id,
            provider_id = %self.provider_id,
            "dsh session shut down (harness closed via the SDK close ladder)"
        );
        Ok(())
    }

    fn capabilities(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::dsh_limited()
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
    use crate::capability::model::{HostOperation, LaunchSpec};

    fn launch_spec() -> LaunchSpec {
        LaunchSpec {
            cwd: std::path::PathBuf::from("/tmp"),
            model: None,
            mode: None,
            mcp_servers: vec![],
        }
    }

    #[test]
    fn default_config_descriptor() {
        let provider = DshNativeProvider::default_config();
        let desc = provider.descriptor();

        assert_eq!(desc.provider_id.0, "dsh-native");
        assert_eq!(desc.protocol_kind, ProtocolKind::NativeCli);
        assert!(desc.capabilities.text_prompt);
        assert!(
            !desc.capabilities.streaming,
            "dsh-native must not claim streaming (AR-6)"
        );
        assert!(
            !desc.capabilities.cancellation,
            "dsh-native must not claim cancellation (AR-6)"
        );
        assert!(
            desc.capabilities.session_restore,
            "dsh-native claims session_restore via start_session(Some(id)) reuse (AR-2)"
        );
        assert!(!desc.capabilities.structured_tool_calls);
        assert!(!desc.capabilities.mcp_http);
    }

    #[test]
    fn default_config_runtime_bin() {
        let provider = DshNativeProvider::default_config();
        assert_eq!(
            provider.runtime_bin.as_deref(),
            Some("dsh-jsonrpc-agent"),
            "default config spawns the runtime by command name via PATH"
        );
        assert!(provider.env.is_empty());
    }

    #[tokio::test]
    async fn probe_unavailable_when_command_not_found_and_env_unset() {
        let provider = DshNativeProvider::new(
            ProviderId::new("nonexistent-dsh-xyz"),
            "Fake".to_string(),
            Some("nonexistent_dsh_runtime_xyz_12345".to_string()),
            HashMap::new(),
            TimeoutConfig::default(),
        );

        let health = provider
            .probe(crate::capability::model::ProbeRequest { timeout_ms: 5000 })
            .await
            .expect("probe should succeed");

        if std::env::var_os("DSH_RUNTIME_BIN").is_some_and(|value| !value.is_empty()) {
            // The env route (PD-4) counts as present even when PATH lacks
            // the name — the environment decides, not this test.
            assert!(
                health.available,
                "DSH_RUNTIME_BIN is set; probe must report available"
            );
        } else {
            assert!(!health.available);
            assert!(
                health.message.unwrap().contains("not found"),
                "unavailable message must name the missing command"
            );
        }
    }

    #[tokio::test]
    async fn probe_available_when_env_route_set() {
        // The env route is only asserted when it is actually set; the test
        // is a no-op otherwise (the unavailable case above covers the rest).
        if std::env::var_os("DSH_RUNTIME_BIN").is_some_and(|value| !value.is_empty()) {
            let provider = DshNativeProvider::new(
                ProviderId::new("env-dsh-xyz"),
                "Env".to_string(),
                Some("nonexistent_dsh_runtime_xyz_12345".to_string()),
                HashMap::new(),
                TimeoutConfig::default(),
            );
            let health = provider
                .probe(crate::capability::model::ProbeRequest { timeout_ms: 5000 })
                .await
                .expect("probe should succeed");
            assert!(health.available);
        }
    }

    #[tokio::test]
    async fn non_prompt_operation_is_capability_unsupported() {
        let provider = DshNativeProvider::default_config();
        let handle = provider.launch(launch_spec()).await.expect("launch");

        let result = provider
            .execute(
                &handle,
                HostOperation::SetModel {
                    model: "deepseek-v4".to_string(),
                },
            )
            .await;

        assert!(matches!(
            result,
            Err(HostError::CapabilityUnsupported { .. })
        ));
    }

    #[tokio::test]
    async fn empty_prompt_is_protocol_error() {
        let provider = DshNativeProvider::default_config();
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

        assert!(matches!(
            result,
            Err(HostError::ProviderProtocolError { .. })
        ));
    }

    #[tokio::test]
    async fn execute_unknown_session_is_internal_error() {
        let provider = DshNativeProvider::default_config();
        let handle = ManagedSessionHandle {
            provider_id: ProviderId::new("dsh-native"),
            session_id: HostSessionId::new(),
            capabilities: CapabilityDescriptor::dsh_limited(),
        };

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

        assert!(matches!(result, Err(HostError::InternalHostError { .. })));
    }

    #[tokio::test]
    async fn cancel_is_honest_noop() {
        let provider = DshNativeProvider::default_config();
        let handle = provider.launch(launch_spec()).await.expect("launch");

        provider
            .cancel(&handle, HostOperationId::new())
            .await
            .expect("cancel must be an honest Ok no-op (AR-6)");
    }

    #[tokio::test]
    async fn shutdown_without_harness_ok() {
        let provider = DshNativeProvider::default_config();
        let handle = provider.launch(launch_spec()).await.expect("launch");

        provider
            .shutdown(handle)
            .await
            .expect("shutdown of an unstarted session must succeed");
    }

    #[tokio::test]
    async fn shutdown_removes_session_and_blocks_new_execute() {
        let provider = DshNativeProvider::default_config();
        let handle = provider.launch(launch_spec()).await.expect("launch");

        provider
            .shutdown(handle.clone())
            .await
            .expect("shutdown must succeed");

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
            matches!(result, Err(HostError::InternalHostError { .. })),
            "execute after shutdown must fail"
        );
    }
}
