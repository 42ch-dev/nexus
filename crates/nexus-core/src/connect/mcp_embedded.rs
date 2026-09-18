//! Embedded MCP server shell (V1.179 P0 T1, DF-88 Model B) — in-process
//! rmcp over sink/stream, transport-neutral (P4-T3 moved from the daemon).
//!
//! The shell is GENERIC over an [`McpBackend`]: a host composition supplies
//! the catalog/call backend and a shutdown signal; the session budget, the
//! sink/stream session establishment, and the rmcp server lifecycle live
//! here. The daemon's `WorkspaceState`-backed backend (catalog builder +
//! `ToolExecuteRequest` dispatch) stays in the daemon as a thin composition
//! over this shell.
//!
//! # Session bounds (GC #8, architect-locked)
//!
//! Model B carries a **documented exemption** from AR-67's
//! [`crate::connect::session::PeerSessionManager::max_sessions`] (default 8).
//! That bound guards the remote WS accept path against dial floods; an
//! in-process consumer presents no remote dial surface and MUST NOT consume
//! peer-session counters. The embedded surface carries its own compile-time
//! bound [`EMBEDDED_MCP_MAX_SESSIONS`]: the (N+1)-th concurrent embedded
//! session establish is refused with the honest discriminator
//! `embedded_mcp_session_limit`. The budget is **process-global** (I-1):
//! every server handle in the process shares ONE registry, so a second
//! server instance cannot bypass the bound. A budget slot is held by the
//! session's SERVER-side serve task for the whole CONNECTION LIFETIME —
//! from before the initialize handshake until the transport ends or the
//! owner shuts down — never by the client-side [`EmbeddedSession`] handle,
//! so the documented consumer pattern (moving `transport` into
//! `rmcp::serve_client`) cannot release the slot early.
//!
//! # Lifecycle
//!
//! Each session's serve task selects on the owner's shutdown signal, so
//! live sessions end with the run that spawned them and release their
//! budget slots. `establish` refuses with [`EmbeddedMcpError::Shutdown`]
//! once the shutdown signal is set: a shutdown wake reaches only waiters
//! registered at fire time, so a post-shutdown session would otherwise
//! await forever and leak its slot.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use futures_channel::mpsc;
use rmcp::model::{
    ClientNotification, ClientRequest, ClientResult, JsonRpcMessage, ServerNotification,
    ServerRequest, ServerResult,
};
use rmcp::serve_server;
use rmcp::transport::sink_stream::SinkStreamTransport;

use crate::connect::mcp_bridge::{McpBackend, McpBridgeHandler};
use crate::connect::visibility::VisibilityPolicy;

/// Client→server wire messages (the client's sink / the server's stream).
type ClientToServer = JsonRpcMessage<ClientRequest, ClientResult, ClientNotification>;
/// Server→client wire messages (the server's sink / the client's stream).
type ServerToClient = JsonRpcMessage<ServerRequest, ServerResult, ServerNotification>;

/// Compile-time bound on concurrent embedded MCP sessions (GC #8).
///
/// In-process consumers are few; the bound keeps the surface finite. It does
/// NOT shape remote load — the remote WS accept path keeps its own
/// `PeerSessionManager::max_sessions` bound, and the embedded surface never
/// touches those counters.
pub const EMBEDDED_MCP_MAX_SESSIONS: usize = 4;

/// Embedded MCP session establish failure (honest discriminator).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EmbeddedMcpError {
    /// The (N+1)-th concurrent session was refused: at most
    /// [`EMBEDDED_MCP_MAX_SESSIONS`] concurrent embedded sessions may be
    /// live at once.
    #[error(
        "embedded_mcp_session_limit: at most {EMBEDDED_MCP_MAX_SESSIONS} concurrent \
         embedded MCP sessions"
    )]
    SessionLimit,
    /// The owner has requested shutdown: new sessions are refused because a
    /// shutdown wake reaches only waiters registered at fire time — a
    /// session established afterwards would never end and would leak its
    /// budget slot.
    #[error("embedded_mcp_shutdown: shutdown requested; no new embedded MCP sessions")]
    Shutdown,
}

impl EmbeddedMcpError {
    /// The stable lowercase discriminator for this refusal.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::SessionLimit => "embedded_mcp_session_limit",
            Self::Shutdown => "embedded_mcp_shutdown",
        }
    }
}

/// Shutdown signal for the embedded server shell: a `watch` channel whose
/// boolean is set when the owner requests shutdown (read gate + per-session
/// select source).
#[derive(Clone)]
pub struct EmbeddedShutdown {
    rx: tokio::sync::watch::Receiver<bool>,
}

impl EmbeddedShutdown {
    /// Bind the shutdown signal to a watch receiver.
    #[must_use]
    pub fn new(rx: tokio::sync::watch::Receiver<bool>) -> Self {
        Self { rx }
    }

    /// True once shutdown has been requested.
    #[must_use]
    pub fn requested(&self) -> bool {
        *self.rx.borrow()
    }

    /// Resolves when shutdown is requested (immediately if already set).
    async fn notified(&self) {
        let mut rx = self.rx.clone();
        if *rx.borrow() {
            return;
        }
        let _ = rx.changed().await;
    }
}

/// The embedded MCP server (Model B) over an arbitrary [`McpBackend`].
///
/// The session budget is PROCESS-GLOBAL: every handle in the process counts
/// against the same [`EMBEDDED_MCP_MAX_SESSIONS`] cap.
pub struct EmbeddedMcpServer<B: McpBackend> {
    backend: B,
    /// Per-consumer MCP tool visibility policy (V1.180 P1, RN-OGA-2), fixed
    /// at server construction and injected into every session's
    /// [`McpBridgeHandler`]. Absent ⇒ byte-identical current behavior.
    policy: VisibilityPolicy,
    shutdown: EmbeddedShutdown,
}

/// One established embedded session: the client-side transport for
/// `serve_client`.
///
/// The consumer completes the handshake with
/// `rmcp::serve_client(ClientInfo::default(), session.transport)`. The
/// session-budget slot is NOT carried on this handle: it lives in the
/// session's server-side serve task (see
/// [`EmbeddedMcpServer::establish`]), so consuming the transport never
/// releases the budget early.
pub struct EmbeddedSession {
    /// Client-side transport: sink = client→server writes, stream =
    /// server→client reads (the rmcp 3.2 `IntoTransport` blanket covers it
    /// for `serve_client`).
    pub transport:
        SinkStreamTransport<mpsc::Sender<ClientToServer>, mpsc::Receiver<ServerToClient>>,
}

/// Process-global embedded session registry (shared by every server handle).
#[derive(Debug)]
struct SessionRegistry {
    active: AtomicUsize,
}

/// The ONE process-wide embedded session registry (lazy-initialized, I-1).
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

impl<B: McpBackend + Clone + 'static> EmbeddedMcpServer<B> {
    /// Build a server shell over a backend, visibility policy, and shutdown
    /// signal.
    #[must_use]
    pub fn new(backend: B, policy: VisibilityPolicy, shutdown: EmbeddedShutdown) -> Self {
        Self {
            backend,
            policy,
            shutdown,
        }
    }

    /// Establish one embedded MCP session: spawns the in-process rmcp
    /// server over a fresh sink/stream pair and returns the client-side
    /// transport. The consumer completes the handshake with
    /// `rmcp::serve_client(ClientInfo::default(), session.transport)`.
    ///
    /// Refused with [`EmbeddedMcpError::Shutdown`] once the shutdown signal
    /// is set, and with [`EmbeddedMcpError::SessionLimit`] when the budget
    /// is exhausted. The budget slot is held by the session's server-side
    /// task for the CONNECTION LIFETIME — never by the returned handle.
    ///
    /// # Errors
    ///
    /// Returns [`EmbeddedMcpError::Shutdown`] when shutdown was requested,
    /// and [`EmbeddedMcpError::SessionLimit`] when the budget is exhausted.
    pub fn establish(&self) -> Result<EmbeddedSession, EmbeddedMcpError> {
        // Check the shutdown gate BEFORE acquiring a slot: a refused
        // establish must not consume budget it can never hold a session in.
        if self.shutdown.requested() {
            return Err(EmbeddedMcpError::Shutdown);
        }
        let slot = process_registry().try_acquire()?;
        // Channel A: client→server (the client's sink, the server's stream).
        // Channel B: server→client (the server's sink, the client's stream).
        // Both carry the concrete JSON-RPC wire message types of their
        // direction.
        let (client_tx, server_rx) = mpsc::channel::<ClientToServer>(16);
        let (server_tx, client_rx) = mpsc::channel::<ServerToClient>(16);
        let server_transport = SinkStreamTransport::new(server_tx, server_rx);
        let handler = McpBridgeHandler {
            backend: self.backend.clone(),
            policy: self.policy.clone(),
        };
        // The server task completes the initialize handshake when the
        // consumer runs `serve_client`, then keeps the service loop alive
        // until the transport ends (the consumer drops the session or the
        // client service). A session torn down before the handshake ends
        // with a logged debug line — normal teardown, not an error.
        //
        // GC #8 (QC F-001): the budget slot MOVES INTO this task — held for
        // the connection lifetime and released only when the task ends.
        // Lifecycle (QC F-002): the task also selects on the owner's
        // shutdown signal, so a live session ends with the run that spawned
        // it instead of outliving it as a detached stray; the transport end
        // remains the primary exit.
        let shutdown = self.shutdown.clone();
        tokio::spawn(async move {
            let serve = async {
                match serve_server(handler, server_transport).await {
                    Ok(service) => {
                        let _ = service.waiting().await;
                    }
                    Err(e) => {
                        tracing::debug!(error = %e, "embedded MCP session ended before handshake");
                    }
                }
            };
            tokio::select! {
                () = shutdown.notified() => {
                    tracing::debug!("embedded MCP session ended by owner shutdown");
                }
                () = serve => {}
            }
            // The slot is owned by this task: dropped here — at connection
            // end or owner shutdown — the budget frees (GC #8).
            drop(slot);
        });
        Ok(EmbeddedSession {
            transport: SinkStreamTransport::new(client_tx, client_rx),
        })
    }
}

/// Start the embedded MCP server (Model B), honoring the GC #9 enablement
/// gate. Returns `None` when enablement was not requested — the caller then
/// stores nothing and consumers see no embedded surface.
///
/// The session budget is PROCESS-global: every server handle shares the
/// same [`EMBEDDED_MCP_MAX_SESSIONS`] registry, so the boot instance and
/// any consumer-constructed handle count against the same cap.
#[must_use]
pub fn start_embedded_mcp_server<B: McpBackend + Clone + 'static>(
    backend: B,
    embedded_enabled: bool,
    policy: VisibilityPolicy,
    shutdown: EmbeddedShutdown,
) -> Option<EmbeddedMcpServer<B>> {
    if !embedded_enabled {
        tracing::info!(
            "embedded MCP not enabled (config key `embedded_mcp` and --embedded-mcp \
             both unset); no server created"
        );
        return None;
    }
    let server = EmbeddedMcpServer::new(backend, policy, shutdown);
    tracing::info!(
        max_sessions = EMBEDDED_MCP_MAX_SESSIONS,
        "embedded MCP server ready (Model B, in-process sink/stream; exempt from \
         PeerSessionManager::max_sessions per GC #8; process-global session budget)"
    );
    Some(server)
}
