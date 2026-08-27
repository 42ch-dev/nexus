//! Embedded MCP server (V1.179 P0 T1, DF-88 Model B) — in-process rmcp
//! over sink/stream.
//!
//! The in-daemon counterpart to the stdio child (AR-71 Model A): a
//! nexus-hosted consumer (embedded ACP agent or an in-daemon capability
//! that wants MCP semantics) establishes a session over an rmcp
//! [`SinkStreamTransport`] pair — two in-process `mpsc` channels, no
//! sockets, no bind, no TLS (that is DF-87's territory). The backend
//! adapts the SAME spine the stdio child reaches over loopback HTTP, as
//! direct function calls: the catalog builder `GET /v1/daemon/tools` uses
//! ([`crate::api::handlers::tools::build_catalog`]) and the
//! `ToolExecuteRequest` dispatch path
//! ([`crate::api::handlers::host_tool_executor::HostToolExecutor`]).
//!
//! # Session bounds (GC #8, architect-locked)
//!
//! Model B carries a **documented exemption** from AR-67's
//! [`PeerSessionManager::max_sessions`] (default 8). That bound guards the
//! remote WS accept path against dial floods (in-handshake + registered
//! connections, `accept.rs` `reserve_in_flight`); an in-process consumer
//! presents no remote dial surface and MUST NOT consume or perturb
//! peer-session counters. The embedded surface carries its own compile-time
//! bound [`EMBEDDED_MCP_MAX_SESSIONS`] (in-process consumers are
//! daemon-internal and few; the bound keeps the surface finite, it does not
//! shape remote load): the (N+1)-th concurrent embedded session establish
//! is refused with the honest discriminator `embedded_mcp_session_limit`.
//! The budget is **process-global** (I-1): every [`EmbeddedMcpServer`]
//! handle in the process shares ONE registry, so the boot instance and any
//! consumer-constructed handle count against the same cap.
//!
//! # Lifecycle
//!
//! The server is created at daemon boot (`boot.rs` §8.5, the peer-tools
//! lane block, via [`boot_embedded_mcp_server`]) and the ONE boot-scoped
//! instance is stored on [`WorkspaceState`] — the handle in-daemon
//! consumers `establish()` on. It lives until daemon shutdown
//! (`state.shutdown_notify()`) — restart-scoped per AR-67, same class as
//! `host`/`port`/`max_sessions`.
//! Enablement is the union of the `PeerToolsConfig.embedded_mcp` key and
//! the `nexus42 daemon start --embedded-mcp` flag (GC #9); the cargo
//! `embedded-mcp` feature is the hard gate (feature off + enablement
//! requested ⇒ warn-and-skip at boot, never an abort — PR #229 F-1
//! posture).
//!
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use futures_channel::mpsc;
use rmcp::model::{
    ClientNotification, ClientRequest, ClientResult, JsonRpcMessage, ServerNotification,
    ServerRequest, ServerResult,
};
use rmcp::transport::sink_stream::SinkStreamTransport;
use rmcp::{serve_server, ErrorData as McpError};

use crate::api::errors::NexusApiError;
use crate::api::handlers::host_tool_executor::{HostToolExecutor, ToolExecuteRequest};
use crate::api::handlers::tools::build_catalog;
use crate::connect::mcp_bridge::{
    is_unroutable, CatalogRow, McpBackend, McpBridgeHandler, ToolCallOutcome,
};
use crate::workspace::WorkspaceState;

/// Compile-time bound on concurrent embedded MCP sessions (GC #8).
///
/// In-process consumers are daemon-internal and few; the bound keeps the
/// surface finite. It does NOT shape remote load — the remote WS accept
/// path keeps its own `PeerSessionManager::max_sessions` bound, and the
/// embedded surface never touches those counters.
pub const EMBEDDED_MCP_MAX_SESSIONS: usize = 4;

/// Client→server wire messages (the client's sink / the server's stream).
type ClientToServer = JsonRpcMessage<ClientRequest, ClientResult, ClientNotification>;
/// Server→client wire messages (the server's sink / the client's stream).
type ServerToClient = JsonRpcMessage<ServerRequest, ServerResult, ServerNotification>;

/// Embedded MCP session establish failure (honest discriminator).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EmbeddedMcpError {
    /// The (N+1)-th concurrent session was refused: at most
    /// [`EMBEDDED_MCP_MAX_SESSIONS`] in-process consumers may hold a
    /// session at once.
    #[error(
        "embedded_mcp_session_limit: at most {EMBEDDED_MCP_MAX_SESSIONS} concurrent \
         embedded MCP sessions"
    )]
    SessionLimit,
}

impl EmbeddedMcpError {
    /// The stable lowercase discriminator for this refusal.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::SessionLimit => "embedded_mcp_session_limit",
        }
    }
}

/// The embedded MCP server (Model B); clone is cheap.
///
/// Every clone shares the spine state, and the session budget is
/// PROCESS-GLOBAL: every handle in the process counts against the same
/// [`EMBEDDED_MCP_MAX_SESSIONS`] cap, so the boot instance and any
/// consumer-constructed handle cannot bypass the bound.
#[derive(Clone)]
pub struct EmbeddedMcpServer {
    state: WorkspaceState,
}

/// One established embedded session: the client-side transport for
/// `serve_client` plus the session-budget slot.
///
/// The consumer completes the handshake with
/// `rmcp::serve_client(ClientInfo::default(), session.transport)`; the slot
/// is released when the session is dropped (teardown frees the budget).
pub struct EmbeddedSession {
    /// Client-side transport: sink = client→server writes, stream =
    /// server→client reads. Named [`SinkStreamTransport`] explicitly (M-3)
    /// — the rmcp 1.8 `IntoTransport` blanket covers it for `serve_client`.
    pub transport:
        SinkStreamTransport<mpsc::Sender<ClientToServer>, mpsc::Receiver<ServerToClient>>,
    /// Session slot — releases the [`EMBEDDED_MCP_MAX_SESSIONS`] budget on
    /// drop.
    _slot: SessionSlot,
}

/// The embedded spine backend: direct function calls into the same catalog
/// builder and `ToolExecuteRequest` dispatch path the HTTP routes use.
#[derive(Clone)]
struct EmbeddedMcpBackend {
    state: WorkspaceState,
}

impl McpBackend for EmbeddedMcpBackend {
    fn list_tools(&self) -> impl Future<Output = Result<Vec<CatalogRow>, McpError>> + Send {
        let rows = build_catalog(&self.state)
            .into_iter()
            .map(|t| CatalogRow {
                id: t.id,
                description: t.description,
                input_schema: t.input_schema,
                output_schema: t.output_schema,
            })
            .collect();
        std::future::ready(Ok(rows))
    }

    async fn call_tool(
        &self,
        tool_name: &str,
        parameters: serde_json::Value,
    ) -> Result<ToolCallOutcome, McpError> {
        let req = ToolExecuteRequest {
            tool_name: tool_name.to_owned(),
            parameters,
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        match HostToolExecutor::execute(&req, &self.state).await {
            Ok(result) => Ok(ToolCallOutcome::Success(result)),
            Err(e) => Ok(map_spine_error(e)),
        }
    }
}

/// Map one spine `NexusApiError` into the AR-70 #4 outcome vocabulary —
/// wire-parity with the stdio child's HTTP mapping (the child reads
/// `error.code` / `error.message` / `error.details.wire_code` from the
/// response body, which is `error_code()` / `Display` / the typed wire
/// code).
fn map_spine_error(e: NexusApiError) -> ToolCallOutcome {
    match e {
        // Peer deny: `not_supported` + typed wire code — executed-but-failed.
        NexusApiError::PeerToolDenied {
            code,
            message,
            wire_code,
        } => ToolCallOutcome::ExecutedError {
            code,
            message,
            wire_code: Some(wire_code),
        },
        // Unroutable: `not_supported` with NO wire code (the daemon
        // `BadRequest` path never supplies one). The message matches the
        // HTTP wire (`Display` = "Bad request: {message}").
        NexusApiError::BadRequest { code, message } if is_unroutable(&code, None) => {
            ToolCallOutcome::Unroutable {
                code,
                message: format!("Bad request: {message}"),
            }
        }
        // Auth rejected → the spine refuses the caller (INTERNAL_ERROR
        // bounded per AR-70 #4; the message never reaches the client).
        NexusApiError::AuthRequired => ToolCallOutcome::DaemonRefused {
            message: "Authentication required".to_owned(),
        },
        // Everything else is an executed-but-failed spine outcome; the
        // public `error_code()` names the failure (same code the HTTP wire
        // carries).
        other => ToolCallOutcome::ExecutedError {
            code: other.error_code().to_owned(),
            message: other.to_string(),
            wire_code: None,
        },
    }
}

/// Process-global embedded session registry (shared by every server handle).
#[derive(Debug)]
struct SessionRegistry {
    active: AtomicUsize,
}
/// The ONE process-wide embedded session registry (lazy-initialized, I-1).
///
/// The session budget is process-global, not per-server-instance: every
/// [`EmbeddedMcpServer`] handle in the process — the boot-scoped instance
/// stored on `WorkspaceState` and any consumer-constructed handle — counts
/// against the same [`EMBEDDED_MCP_MAX_SESSIONS`] cap, so no second server
/// instance can bypass the bound.
fn process_registry() -> Arc<SessionRegistry> {
    static REGISTRY: std::sync::LazyLock<Arc<SessionRegistry>> = std::sync::LazyLock::new(|| {
        Arc::new(SessionRegistry {
            active: AtomicUsize::new(0),
        })
    });
    Arc::clone(&REGISTRY)
}

impl SessionRegistry {
    /// Acquire one session slot, refusing with
    /// [`EmbeddedMcpError::SessionLimit`] when the budget is exhausted.
    fn try_acquire(self: &Arc<Self>) -> Result<SessionSlot, EmbeddedMcpError> {
        let mut current = self.active.load(Ordering::SeqCst);
        loop {
            if current >= EMBEDDED_MCP_MAX_SESSIONS {
                return Err(EmbeddedMcpError::SessionLimit);
            }
            match self.active.compare_exchange_weak(
                current,
                current + 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => {
                    return Ok(SessionSlot {
                        registry: Arc::clone(self),
                    })
                }
                Err(observed) => current = observed,
            }
        }
    }

    /// Release one session slot (saturating — a defensive double-release
    /// can never underflow).
    fn release(&self) {
        let _ = self
            .active
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| {
                Some(v.saturating_sub(1))
            });
    }

    /// Live session count (tests).
    #[cfg(test)]
    fn active_count(&self) -> usize {
        self.active.load(Ordering::SeqCst)
    }
}

/// RAII session slot: releases the budget on drop.
#[derive(Debug)]
struct SessionSlot {
    registry: Arc<SessionRegistry>,
}

impl Drop for SessionSlot {
    fn drop(&mut self) {
        self.registry.release();
    }
}

impl EmbeddedMcpServer {
    /// Establish one embedded MCP session: spawns the in-process rmcp
    /// server over a fresh sink/stream pair and returns the client-side
    /// transport. The consumer completes the handshake with
    /// `rmcp::serve_client(ClientInfo::default(), session.transport)`.
    ///
    /// Refused with [`EmbeddedMcpError::SessionLimit`] (discriminator
    /// `embedded_mcp_session_limit`) when [`EMBEDDED_MCP_MAX_SESSIONS`]
    /// sessions are already live. The slot is freed when the returned
    /// session is dropped.
    ///
    /// # Errors
    ///
    /// Returns [`EmbeddedMcpError::SessionLimit`] when the session budget
    /// is exhausted.
    pub fn establish(&self) -> Result<EmbeddedSession, EmbeddedMcpError> {
        let slot = process_registry().try_acquire()?;
        // Channel A: client→server (the client's sink, the server's
        // stream). Channel B: server→client (the server's sink, the
        // client's stream). Both carry the concrete JSON-RPC wire message
        // types of their direction.
        let (client_tx, server_rx) = mpsc::channel::<ClientToServer>(16);
        let (server_tx, client_rx) = mpsc::channel::<ServerToClient>(16);
        let server_transport = SinkStreamTransport::new(server_tx, server_rx);
        let handler = McpBridgeHandler {
            backend: EmbeddedMcpBackend {
                state: self.state.clone(),
            },
        };
        // The server task completes the initialize handshake when the
        // consumer runs `serve_client`, then keeps the service loop alive
        // until the transport ends (the consumer drops the session or the
        // client service). A session torn down before the handshake ends
        // with a logged debug line — normal teardown, not an error.
        tokio::spawn(async move {
            match serve_server(handler, server_transport).await {
                Ok(service) => {
                    let _ = service.waiting().await;
                }
                Err(e) => {
                    tracing::debug!(error = %e, "embedded MCP session ended before handshake");
                }
            }
        });
        Ok(EmbeddedSession {
            transport: SinkStreamTransport::new(client_tx, client_rx),
            _slot: slot,
        })
    }
}

/// Start the embedded MCP server (Model B), honoring the GC #9 enablement
/// gate.
///
/// `embedded_enabled` is the union of the `PeerToolsConfig.embedded_mcp`
/// key and the `--embedded-mcp` CLI flag (computed by
/// [`boot_embedded_mcp_server`] at daemon boot, or directly by tests).
/// Returns `None` when enablement was not requested — the caller then
/// stores nothing and in-daemon consumers see no embedded surface.
///
/// The session budget is PROCESS-global: every server handle shares the
/// same [`EMBEDDED_MCP_MAX_SESSIONS`] registry, so the boot instance and
/// any consumer-constructed handle count against the same cap.
#[must_use]
pub fn start_embedded_mcp_server(
    state: WorkspaceState,
    embedded_enabled: bool,
) -> Option<EmbeddedMcpServer> {
    if !embedded_enabled {
        tracing::info!(
            "embedded MCP not enabled (config key `embedded_mcp` and --embedded-mcp \
             both unset); no server created"
        );
        return None;
    }
    let server = EmbeddedMcpServer { state };
    tracing::info!(
        max_sessions = EMBEDDED_MCP_MAX_SESSIONS,
        "embedded MCP server ready (Model B, in-process sink/stream; exempt from \
         PeerSessionManager::max_sessions per GC #8; process-global session budget)"
    );
    Some(server)
}
/// Boot-wire the embedded MCP server (DF-88 Model B) per GC #9.
///
/// The exact function the daemon boot block (`boot.rs` §8.5) calls.
/// Enablement is the union of the `PeerToolsConfig.embedded_mcp` key (read
/// from `~/.nexus42/connect/daemon.json` via [`crate::connect::PeerToolsConfig::load`];
/// `raw_home` is the raw `$HOME`, the path helper joins `.nexus42`
/// internally) and the `--embedded-mcp` CLI flag. When enabled, ONE
/// boot-scoped server instance is created and stored on `state` — the
/// handle in-daemon consumers `establish()` on (I-1). The cargo
/// `embedded-mcp` feature remains the hard gate: with this module not
/// compiled, the feature-off warn-and-skip lives in `boot.rs` (the CLI
/// flag warns on every graph; the config-key path needs `connect-client`
/// to load `PeerToolsConfig`).
pub async fn boot_embedded_mcp_server(
    state: &mut WorkspaceState,
    raw_home: &std::path::Path,
    cli_embedded_mcp: bool,
) {
    let embedded_enabled = match crate::connect::PeerToolsConfig::load(raw_home) {
        Ok(cfg) => cfg.embedded_mcp || cli_embedded_mcp,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "embedded MCP config load failed; continuing without embedded MCP"
            );
            false
        }
    };
    if let Some(server) = start_embedded_mcp_server(state.clone(), embedded_enabled) {
        state.set_embedded_mcp_server(Arc::new(server));
        tracing::info!(
            "embedded MCP server started (Model B, in-process sink/stream; boot instance \
             stored on WorkspaceState for in-daemon consumers)"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connect::session::PeerSessionManager;
    use serial_test::serial;

    #[tokio::test]
    #[serial]
    async fn session_limit_refuses_fifth_concurrent_session() {
        // The budget is PROCESS-global (I-1): the registry is shared across
        // every `EmbeddedMcpServer` handle in this process, so the boot
        // instance and any consumer-constructed handle count against the
        // same cap. `#[serial]` keeps this test's 4-session hold isolated
        // from the other budget-touching unit test in this binary.
        let server = start_embedded_mcp_server(
            WorkspaceState::new_for_testing(
                std::env::temp_dir().join("embedded-mcp-test-home"),
                std::env::temp_dir().join("embedded-mcp-test.db"),
                None,
            )
            .await,
            true,
        )
        .expect("enabled server");
        let mut sessions = Vec::new();
        for _ in 0..EMBEDDED_MCP_MAX_SESSIONS {
            sessions.push(server.establish().expect("session within budget"));
        }
        let Err(err) = server.establish() else {
            panic!("5th concurrent session must be refused")
        };
        assert_eq!(err, EmbeddedMcpError::SessionLimit);
        assert_eq!(
            err.code(),
            "embedded_mcp_session_limit",
            "honest discriminator (GC #8)"
        );
        assert_eq!(
            process_registry().active_count(),
            EMBEDDED_MCP_MAX_SESSIONS,
            "budget fully consumed (process-global)"
        );

        // Teardown frees the slot: dropping one session lets the next
        // establish succeed.
        drop(sessions.pop().expect("session to drop"));
        assert_eq!(
            process_registry().active_count(),
            EMBEDDED_MCP_MAX_SESSIONS - 1,
            "teardown frees the slot"
        );
        let _replacement = server.establish().expect("slot freed by teardown");
        assert_eq!(
            process_registry().active_count(),
            EMBEDDED_MCP_MAX_SESSIONS,
            "freed slot re-acquired"
        );
    }

    #[tokio::test]
    #[serial]
    async fn embedded_sessions_never_touch_peer_session_counters() {
        // GC #8: the embedded surface is exempt from
        // `PeerSessionManager::max_sessions` and MUST NOT consume or
        // perturb peer-session counters. Non-tautological baseline (M-1):
        // the manager starts in a NONZERO state (one in-flight reservation
        // simulating an accepted-but-unregistered dial), so the assertions
        // prove embedded establish/teardown leaves the peer counters
        // untouched rather than asserting 0 == 0 on a fresh manager.
        let (_tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let server = start_embedded_mcp_server(
            WorkspaceState::new_for_testing(nexus_home, db_path, None).await,
            true,
        )
        .expect("enabled server");
        let peer_sessions = PeerSessionManager::new();
        assert!(
            peer_sessions.reserve_in_flight(8),
            "baseline in-flight reservation accepted"
        );
        assert_eq!(peer_sessions.session_count(), 0);
        assert_eq!(peer_sessions.connection_count(), 1, "nonzero baseline");

        let session = server.establish().expect("establish");
        assert_eq!(
            peer_sessions.session_count(),
            0,
            "embedded establish must not register a peer session"
        );
        assert_eq!(
            peer_sessions.connection_count(),
            1,
            "embedded establish must not consume the peer budget"
        );
        drop(session);
        assert_eq!(
            peer_sessions.session_count(),
            0,
            "embedded teardown must not perturb peer sessions"
        );
        assert_eq!(
            peer_sessions.connection_count(),
            1,
            "embedded teardown must not perturb the peer budget"
        );
        peer_sessions.release_in_flight();
        assert_eq!(peer_sessions.connection_count(), 0);
    }

    #[test]
    fn map_spine_error_matches_stdio_wire_discriminators() {
        // AR-70 #4 parity: the embedded path must classify spine errors
        // into the SAME `ToolCallOutcome` values the stdio child derives
        // from the HTTP wire body (`error.code` / `error.message` /
        // `error.details.wire_code`).
        let peer_deny = map_spine_error(NexusApiError::PeerToolDenied {
            code: "not_supported".to_owned(),
            message: "tool is not supported by this peer".to_owned(),
            wire_code: "op_unsupported".to_owned(),
        });
        assert_eq!(
            peer_deny,
            ToolCallOutcome::ExecutedError {
                code: "not_supported".to_owned(),
                message: "tool is not supported by this peer".to_owned(),
                wire_code: Some("op_unsupported".to_owned()),
            },
            "peer deny keeps the typed lowercase wire code verbatim"
        );

        let unroutable = map_spine_error(NexusApiError::BadRequest {
            code: "not_supported".to_owned(),
            message: "unsupported tool: tools.t6.gone".to_owned(),
        });
        assert_eq!(
            unroutable,
            ToolCallOutcome::Unroutable {
                code: "not_supported".to_owned(),
                message: "Bad request: unsupported tool: tools.t6.gone".to_owned(),
            },
            "unroutable matches the HTTP wire message (Display prefix)"
        );

        let auth = map_spine_error(NexusApiError::AuthRequired);
        assert!(matches!(auth, ToolCallOutcome::DaemonRefused { .. }));

        let invalid = map_spine_error(NexusApiError::InvalidInput {
            field: "x".to_owned(),
            reason: "missing".to_owned(),
        });
        assert_eq!(
            invalid,
            ToolCallOutcome::ExecutedError {
                code: "invalid_input".to_owned(),
                message: "Invalid input: missing".to_owned(),
                wire_code: None,
            },
            "executed failure names the spine code"
        );
    }
}
