//! Peer-tools Connect accept loop (V1.174 P0, AR-67 §3.1-§3.4).
//!
//! The daemon-side listening face for spoke dialers: one
//! `TcpListener` (config-gated host/port), one WebSocket upgrade per
//! connection, one `connect_responder` per connection, and a per-connection
//! monitor task that registers sessions with the [`PeerSessionManager`] and
//! evicts them on close observation.
//!
//! Invariants (AR-67 #4):
//! - **Accept-loop independence**: the accept loop NEVER awaits session work
//!   — every connection is handed to a spawned task. The loop body is only
//!   `accept()` + the session-limit gate + `spawn`.
//! - **Session limit**: excess connections are refused at accept with a
//!   logged refusal (the gate counts registered sessions; a 9th concurrent
//!   session is closed before any session work).
//! - **Close observation** (AR-67 #4, no spoke API changes): the
//!   nexus-owned [`ObservedTransport`] wrapper sets a flag + fires a
//!   `Notify` on the first transport error/close; the monitor awaits it and
//!   evicts in the same tick, with a `responder.state()` poll as the
//!   documented fallback.
//! - **Zero session state on handshake failure**: a non-allowlisted peer /
//!   missing key is rejected by the responder's fail-closed handshake; the
//!   session manager never sees it.
//!
//! The daemon hello manifest is an input parameter (T4 owns the
//! AR-69 allowlist-derived hello; boot builds the baseline).

use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use spoke_connect::remote::{
    connect_responder, ConnectResponder, ConnectResponderOptions, ConnectResponderState,
    RemoteIdentity, Transport, TransportError,
};
use spoke_schemas::HostCapabilityManifest;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::connect::config::PeerToolsConfig;
use crate::connect::identity::{self, IdentityError};
use crate::connect::session::PeerSessionManager;
use crate::connect::ws_transport::{ws_config, WsTransport};

/// Poll interval for the close-observation fallback (`responder.state()`).
const CLOSE_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Handshake state poll interval while waiting for establishment.
const HANDSHAKE_POLL_INTERVAL: Duration = Duration::from_millis(25);

/// The nexus-owned transport wrapper (AR-67 #4).
///
/// Delegates to the inner transport and sets a flag + fires a `Notify` on
/// the first error/close observed through it. This is how the session
/// monitor observes a peer drop WITHOUT any spoke API change — the wrapper
/// IS the transport handed to `connect_responder`.
pub struct ObservedTransport {
    inner: Arc<dyn Transport>,
    closed: AtomicBool,
    closed_notify: Notify,
}

impl ObservedTransport {
    /// Wrap an inner transport.
    #[must_use]
    pub fn new(inner: Arc<dyn Transport>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            closed: AtomicBool::new(false),
            closed_notify: Notify::new(),
        })
    }

    /// True once the transport has reported an error/close.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// A future that completes when the transport reports its first
    /// error/close. Re-created per wait so the flag re-check between waits
    /// is never missed. `tokio::select!` pins the future internally, so no
    /// `Unpin` bound is needed.
    fn closed_notified(&self) -> impl Future<Output = ()> + '_ {
        self.closed_notify.notified()
    }

    /// Latch the closed flag once; wakes every waiting `notified()` future
    /// created before the call.
    fn mark_closed(&self) {
        if !self.closed.swap(true, Ordering::SeqCst) {
            self.closed_notify.notify_waiters();
        }
    }
}

#[async_trait]
impl Transport for ObservedTransport {
    async fn send(&self, envelope: &[u8]) -> Result<(), TransportError> {
        let result = self.inner.send(envelope).await;
        if result.is_err() {
            self.mark_closed();
        }
        result
    }

    async fn recv(&self) -> Result<Vec<u8>, TransportError> {
        let result = self.inner.recv().await;
        if result.is_err() {
            self.mark_closed();
        }
        result
    }

    async fn close(&self) -> Result<(), TransportError> {
        self.mark_closed();
        self.inner.close().await
    }
}

/// Daemon hello manifest: baseline capabilities (+ any tool ids the test /
/// T4 wiring chooses to advertise). `host_id` is the installation device id.
///
/// # Panics
/// Panics if the static JSON shape fails to deserialize (programmer error —
/// the shape is fixed at authoring time).
#[must_use]
pub fn daemon_manifest(host_id: &str, tool_ids: &[String]) -> HostCapabilityManifest {
    let mut capabilities = vec!["spoke-baseline".to_owned()];
    capabilities.extend(tool_ids.iter().cloned());
    let namespaces: Vec<String> = tool_ids
        .iter()
        .filter_map(|id| id.split('.').nth(1))
        .map(ToOwned::to_owned)
        .collect();
    serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "host_id": host_id,
        "roles": ["daemon"],
        "capabilities": capabilities,
        "namespaces": namespaces,
        "extensions": {},
        "tools": [],
    }))
    .expect("static daemon manifest is valid")
}

/// Per-connection responder options (identity + hello + trust material).
#[derive(Clone)]
pub struct PeerResponderOptions {
    /// Daemon Ed25519 seed (persistent identity).
    pub identity_seed: [u8; 32],
    /// Daemon hello manifest (T4 derives it from the allowlist).
    pub manifest: Arc<HostCapabilityManifest>,
    /// Dialer peer ids allowed at the handshake (fail-closed).
    pub allowlist: Vec<String>,
    /// Preconfigured dialer Ed25519 public keys by peer id (fail-closed).
    pub peer_keys: HashMap<String, [u8; 32]>,
}

/// Spawn the peer-tools accept loop over an already-bound listener.
///
/// The accept loop is detached: it runs until `shutdown` fires. Each
/// accepted connection is processed in its own spawned task; the loop never
/// awaits session work.
#[must_use]
pub fn spawn_accept_loop(
    listener: TcpListener,
    config: Arc<PeerToolsConfig>,
    sessions: Arc<PeerSessionManager>,
    responder_options: PeerResponderOptions,
    shutdown: Arc<Notify>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let accepted = tokio::select! {
                result = listener.accept() => result,
                () = shutdown.notified() => break,
            };
            let (stream, _peer_addr) = match accepted {
                Ok(pair) => pair,
                Err(e) => {
                    tracing::warn!(error = %e, "peer-tools accept error");
                    continue;
                }
            };
            // Session limit gate at accept (AR-67 #4): excess closed at
            // accept with a logged refusal — the dialer fails fast.
            if sessions.session_count() >= config.max_sessions {
                tracing::warn!(
                    limit = config.max_sessions,
                    "peer session refused at accept: session limit reached"
                );
                drop(stream);
                continue;
            }
            let config = Arc::clone(&config);
            let sessions = Arc::clone(&sessions);
            let responder_options = responder_options.clone();
            tokio::spawn(async move {
                handle_connection(stream, config, sessions, responder_options).await;
            });
        }
        tracing::info!("peer-tools accept loop stopped");
    })
}

/// One accepted connection: WS upgrade → responder → session registration →
/// close-observation monitor (all in the caller's spawned task).
async fn handle_connection(
    stream: TcpStream,
    config: Arc<PeerToolsConfig>,
    sessions: Arc<PeerSessionManager>,
    options: PeerResponderOptions,
) {
    let ws = match tokio_tungstenite::accept_async_with_config(
        stream,
        Some(ws_config(config.max_envelope_bytes)),
    )
    .await
    {
        Ok(ws) => ws,
        Err(e) => {
            tracing::debug!(error = %e, "peer-tools WS upgrade failed");
            return;
        }
    };
    let observed = ObservedTransport::new(Arc::new(WsTransport::new(ws)));
    let responder = connect_responder(ConnectResponderOptions {
        transport: Arc::clone(&observed) as Arc<dyn Transport>,
        identity: RemoteIdentity { seed: options.identity_seed },
        manifest: (*options.manifest).clone(),
        allowlist: options.allowlist.clone(),
        peer_keys: options.peer_keys.clone(),
        ports: None,
        invoke_timeout_ms: Some(config.invoke_timeout_ms),
    })
    .await;
    monitor_session(responder, observed, sessions, config.invoke_timeout_ms).await;
}

/// Establish / register / observe-close for one session.
///
/// Phase 1: bounded handshake wait. Phase 2: register (last-wins replace).
/// Phase 3: close observation → eviction in the same tick as the observed
/// close; `responder.state()` poll as the documented fallback (AR-67 #4).
async fn monitor_session(
    responder: Arc<ConnectResponder>,
    observed: Arc<ObservedTransport>,
    sessions: Arc<PeerSessionManager>,
    invoke_timeout_ms: u64,
) {
    // Phase 1: bounded handshake outcome (a dialer that never sends its
    // hello is closed by us after the bound — the responder's own recv would
    // otherwise park forever).
    let handshake_timeout = Duration::from_millis(invoke_timeout_ms.max(1000));
    let established = tokio::time::timeout(handshake_timeout, wait_until_established(&responder))
        .await
        .ok()
        .flatten();
    let Some(peer_id) = established else {
        // Handshake failed (rejection → responder closed itself) or timed
        // out: close the responder so the dialer fails fast. Zero session
        // state — the manager never saw this peer.
        responder.close();
        return;
    };

    // Phase 2: admit. The admitted ids are the authenticated manifest's tool
    // ids (T2 granularity; T3's admission filter chain narrows this set).
    let admitted_ids: Vec<String> = responder
        .remote_manifest()
        .map(|manifest| {
            manifest
                .tools
                .iter()
                .map(|tool| String::from(tool.capability_id.clone()))
                .collect()
        })
        .unwrap_or_default();
    let replaced = sessions.register(&peer_id, Arc::clone(&responder), &admitted_ids);
    tracing::info!(%peer_id, replaced, "peer session established");

    // Phase 3: close observation. Primary path = the wrapper's Notify (fires
    // the same tick the transport reports an error/close); fallback = the
    // responder state poll (catches a close the wrapper missed, e.g. a
    // local `close_session` without a transport error). The flag is
    // re-checked after the future is created to close the notify-counter
    // race; the poll tick is the belt-and-braces fallback.
    loop {
        if observed.is_closed() || responder.state() == ConnectResponderState::Closed {
            break;
        }
        let notified = observed.closed_notified();
        if observed.is_closed() || responder.state() == ConnectResponderState::Closed {
            break;
        }
        tokio::select! {
            () = notified => {}
            () = tokio::time::sleep(CLOSE_POLL_INTERVAL) => {}
        }
    }
    let evicted = sessions.evict(&peer_id, Some(&responder));
    if evicted {
        tracing::info!(%peer_id, "peer session evicted after close observation");
    }
}

/// Poll the responder state until it leaves `Handshaking`; returns the
/// dialer peer id on success, `None` on rejection.
async fn wait_until_established(responder: &Arc<ConnectResponder>) -> Option<String> {
    loop {
        match responder.state() {
            ConnectResponderState::Established => return responder.remote_peer_id(),
            ConnectResponderState::Closed | ConnectResponderState::Disconnected => return None,
            ConnectResponderState::Handshaking => {
                tokio::time::sleep(HANDSHAKE_POLL_INTERVAL).await;
            }
        }
    }
}

/// Boot helper: load config + persistent identity from `home`, build the
/// baseline daemon manifest, bind the listener, spawn the accept loop.
///
/// The peer allowlist / peer keys are T4's outbound-authz config surface
/// (AR-69); boot starts the lane fail-closed (empty allowlist ⇒ every dial
/// is rejected at the handshake) until that config lands.
///
/// # Errors
/// Config load, identity persistence, or listener bind failures are
/// returned as errors — the caller decides whether to fail boot or keep the
/// daemon core running without the peer-tools lane.
pub async fn start_peer_tools_lane(
    home: &Path,
    shutdown: Arc<Notify>,
) -> anyhow::Result<PeerToolsLaneHandle> {
    let config = Arc::new(PeerToolsConfig::load(home)?);
    crate::boot::ensure_remote_bind_allowed(&config.host)?;
    let identity_seed = identity::load_or_create_identity(home)?;
    let device_id = nexus_home_layout::device_id::get_or_create_device_id(home)
        .map_err(|e| IdentityError::Io {
            path: home.display().to_string(),
            source: std::io::Error::other(format!("device id resolution failed: {e}")),
        })?;
    let manifest = Arc::new(daemon_manifest(&device_id, &[]));
    let listener = TcpListener::bind((config.host.as_str(), config.port)).await?;
    let addr = listener.local_addr()?;
    let sessions = Arc::new(PeerSessionManager::new());
    let options = PeerResponderOptions {
        identity_seed,
        manifest,
        allowlist: Vec::new(),
        peer_keys: HashMap::new(),
    };
    let task = spawn_accept_loop(
        listener,
        Arc::clone(&config),
        Arc::clone(&sessions),
        options,
        shutdown,
    );
    tracing::info!(
        %addr,
        max_sessions = config.max_sessions,
        "peer-tools Connect accept loop listening (fail-closed: peer allowlist not yet configured)"
    );
    Ok(PeerToolsLaneHandle {
        addr,
        sessions,
        task,
    })
}

/// Running peer-tools lane (accept loop + its session registry).
pub struct PeerToolsLaneHandle {
    /// Bound listen address.
    pub addr: std::net::SocketAddr,
    /// Session registry shared by the accept loop (T3's dispatch arm reads
    /// this; the reverse index into the `PeerToolTable` lands with T3).
    pub sessions: Arc<PeerSessionManager>,
    /// Accept-loop task.
    pub task: JoinHandle<()>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observed_transport_latches_flag() {
        let pair = spoke_connect::remote::loopback_transport_pair();
        let observed = ObservedTransport::new(Arc::new(pair.client) as Arc<dyn Transport>);
        assert!(!observed.is_closed());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            // The peer close surfaces as a recv error on the wrapper.
            let observed_ref = Arc::clone(&observed);
            let recv = tokio::spawn(async move { observed_ref.recv().await });
            pair.server.close().await.expect("close");
            let result = tokio::time::timeout(Duration::from_secs(5), recv)
                .await
                .expect("recv must resolve")
                .expect("recv task must not panic");
            assert!(result.is_err(), "peer close must fail the recv");
        });
        assert!(observed.is_closed(), "wrapper must latch the close flag");
    }

    #[test]
    fn daemon_manifest_is_baseline() {
        let manifest = daemon_manifest("device-1", &[]);
        assert!(manifest.capabilities.contains(&"spoke-baseline".to_owned()));
        assert!(manifest.tools.is_empty());
        assert_eq!(manifest.host_id.as_str(), "device-1");
    }
}
