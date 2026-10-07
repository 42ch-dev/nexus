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
//!   logged refusal. The gate counts registered sessions PLUS in-flight
//!   (in-handshake) connections (QC-fix W-A): a dial flood of incomplete
//!   handshakes cannot exceed the cap or spawn unbounded handshake tasks.
//! - **Close observation** (AR-67 #4, no spoke API changes): the
//!   nexus-owned [`ObservedTransport`] wrapper sets a flag + fires a
//!   `Notify` on the first transport error/close; the monitor awaits it and
//!   evicts in the same tick, with a `responder.state()` poll as the
//!   documented fallback.
//! - **Zero session state on handshake failure**: a non-allowlisted peer /
//!   missing key is rejected by the responder's fail-closed handshake; the
//!   session manager never sees it.
//!
//! The daemon hello manifest derives PER CONNECTION from the live config
//! allowlist (AR-69: baseline ∪ allowlist exact ids; DF-92 — a hot-added
//! allowlist entry is negotiable for the next handshake without a
//! restart; tests seed the holder with the ids directly).

use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use nexus_spoke_adapter::HostCapabilityManifest;
use spoke_connect::remote::{
    connect_responder, ConnectResponder, ConnectResponderOptions, ConnectResponderState,
    RemoteIdentity, Transport, TransportError,
};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::connect::config::PeerToolsConfig;
use crate::connect::identity::{self};
use crate::connect::session::PeerSessionManager;
use crate::connect::table::live_reserved_tool_ids;
use crate::connect::watch::{
    peer_config_digest, spawn_peer_config_watch, supervise_peer_config_watch, PeerConfigHolder,
    PeerConfigSnapshot,
};
use crate::connect::ws_transport::{ws_config, WsTransport};
use crate::error::{CoreError, CoreResult};
use nexus_orchestration::CapabilityRegistryHolder;
const SUBSCRIBE_TOOL: &str = "tools.nexus.subscribe";
const DELIVER_TOOL: &str = "tools.nexus.deliver_events";

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
    pending_subscribe: Mutex<HashMap<String, String>>,
    session: Mutex<Option<String>>,
}

impl ObservedTransport {
    #[must_use]
    pub fn new(inner: Arc<dyn Transport>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            closed: AtomicBool::new(false),
            closed_notify: Notify::new(),
            pending_subscribe: Mutex::new(HashMap::new()),
            session: Mutex::new(None),
        })
    }

    fn bind_session(&self, peer_id: &str) {
        *self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(peer_id.to_owned());
    }

    fn observe_inbound(&self, envelope: &[u8]) {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(envelope) else {
            return;
        };
        if value.get("op").and_then(serde_json::Value::as_str) != Some(SUBSCRIBE_TOOL) {
            return;
        }
        let Some(request_id) = value.get("request_id").and_then(serde_json::Value::as_str) else {
            return;
        };
        let Some(stream) = value
            .pointer("/payload/arguments/stream")
            .and_then(serde_json::Value::as_str)
        else {
            return;
        };
        self.pending_subscribe
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(request_id.to_owned(), stream.to_owned());
    }

    fn observe_outbound(&self, envelope: &[u8]) {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(envelope) else {
            return;
        };
        let Some(request_id) = value.get("request_id").and_then(serde_json::Value::as_str) else {
            return;
        };
        let stream = self
            .pending_subscribe
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(request_id);
        let Some(stream) = stream else { return };
        if value.pointer("/payload/result").is_none() {
            return;
        }
        let session = self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(session) = session {
            crate::connect::events::connect_event_registry().activate(&session, &stream);
        }
    }

    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
    fn closed_notified(&self) -> impl Future<Output = ()> + '_ {
        self.closed_notify.notified()
    }
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
        } else {
            self.observe_outbound(envelope);
        }
        result
    }

    async fn recv(&self) -> Result<Vec<u8>, TransportError> {
        let result = self.inner.recv().await;
        if let Ok(envelope) = &result {
            self.observe_inbound(envelope);
        }
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

/// Whether `host` is a loopback bind target (`localhost` or a loopback IP).
///
/// Semantics verbatim from the daemon boot gate (P4-T3 moved into the core
/// lane so the refusal lives with the lane that enforces it).
#[must_use]
pub fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

/// Enforce the opt-in remote-bind gate for a non-loopback peer-tools bind.
///
/// A non-loopback bind is only permitted when both `NEXUS42_DAEMON_API_KEY`
/// and `NEXUS_DAEMON_REMOTE_BIND=1` are present. Loopback binds are
/// unaffected. Semantics verbatim from the daemon boot gate.
///
/// # Errors
/// Returns [`CoreError::Internal`] when the remote-bind gate is closed.
pub fn ensure_remote_bind_allowed(host: &str) -> CoreResult<()> {
    if is_loopback_host(host) {
        return Ok(());
    }
    let key_set = std::env::var("NEXUS42_DAEMON_API_KEY").is_ok();
    let remote_allowed = std::env::var("NEXUS_DAEMON_REMOTE_BIND").as_deref() == Ok("1");
    if !key_set || !remote_allowed {
        return Err(CoreError::Internal {
            category: format!(
                "Refusing to bind peer-tools lane to non-loopback address {host}: \
                 remote bind requires both NEXUS42_DAEMON_API_KEY and NEXUS_DAEMON_REMOTE_BIND=1"
            ),
        });
    }
    tracing::info!(host, "remote bind gate open for the peer-tools lane");
    Ok(())
}

/// Daemon hello manifest: baseline capabilities (+ any tool ids the test /
/// T4 wiring chooses to advertise). `host_id` is the installation device id.
///
/// AR-69 derivation lock: the tool `capabilities[]` derive ONLY from the
/// operator allowlist (the `tool_ids` argument — connections pass the live
/// config allowlist, DF-92; tests pass ids directly). No runtime discovery
/// ever feeds this manifest. `namespaces[]` is derived from the tool ids
/// (`tools.<ns>.<id>` ⇒ `ns`), deduplicated and order-stable.
///
/// # Panics
/// Panics if the static JSON shape fails to deserialize (programmer error —
/// the shape is fixed at authoring time).
#[must_use]
pub fn daemon_manifest(host_id: &str, tool_ids: &[String]) -> HostCapabilityManifest {
    // v1.191 P1 T14 (durable §9): the tools-only derivation lives with the
    // allowlist that feeds it (`config::tools_only_capabilities`) — the
    // baseline plus exact allowlisted tool ids, and never a KE family. This
    // responder is composed with `ports: None`, so it advertises no
    // `ke-ownership` / `ke-extraction`.
    let capabilities = crate::connect::config::tools_only_capabilities(tool_ids);
    // Tool grammar is exactly `tools.<ns>.<id>` (3 segments), so `nth(1)`
    // is the namespace. Dedup keeps the hello stable when the allowlist
    // names several tools in one namespace (T2 review M-1/M-2).
    let mut namespaces: Vec<String> = tool_ids
        .iter()
        .filter_map(|id| id.split('.').nth(1))
        .map(ToOwned::to_owned)
        .collect();
    namespaces.sort();
    namespaces.dedup();
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
/// Per-lane responder identity (AR-69 trust material minus the frozen
/// generation).
///
/// DF-92: the ADMISSION-affecting material — dialer allowlist, preconfigured
/// keys, allowlist-derived hello — is NOT frozen here. Each connection reads
/// ONE generation from `config`, the live [`PeerConfigHolder`] (see
/// [`handle_connection`]); only the fields below are lane-static.
#[derive(Clone)]
pub struct PeerResponderOptions {
    /// Daemon Ed25519 seed (persistent identity).
    pub identity_seed: [u8; 32],
    /// Installation device id — the hello `host_id`. The hello itself is
    /// derived per connection from the live allowlist.
    pub host_id: String,
    /// Live peer config holder (DF-92): one read per connection supplies
    /// the handshake allowlist (`peer_ids`), the handshake keys, and the
    /// allowlist the hello + admission derive from.
    pub config: PeerConfigHolder,
    /// Shared capability registry holder (AR-92) from which the
    /// AR-68 #2(iii) reserved set is derived **live** at each admission:
    /// builtin ids ∪ the current user-capability names. A user capability
    /// hot-added after the lane spawned stays reserved against peer
    /// admission (V1.176 P1 QC fix W-A). `None` reserves only the static
    /// builtin host-tool ids.
    pub capability_registry: Option<CapabilityRegistryHolder>,
}

/// The connection's grant-at-establish generation (DF-92): the config
/// snapshot the handshake validated against + the allowlist-derived hello it
/// negotiated with. Admission reads THIS generation — never a fresh holder
/// read — so a session's grant is internally coherent even if a reload lands
/// mid-handshake (plan risk table: the handshake clones the snapshot `Arc`
/// for its duration; the swap is RwLock-quick and never blocks readers).
struct ConnectionGeneration {
    snapshot: Arc<PeerConfigSnapshot>,
    manifest: Arc<HostCapabilityManifest>,
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
            // Session limit gate at accept (AR-67 #4, QC-fix W-A): excess
            // closed at accept with a logged refusal — the dialer fails
            // fast. The reservation is taken BEFORE the spawn so
            // in-handshake connections count against the cap (the old
            // registered-only count left a dial flood unbounded).
            if !sessions.reserve_in_flight(config.max_sessions) {
                tracing::warn!(
                    limit = config.max_sessions,
                    "peer session limit reached (registered + in-flight)"
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
    // DF-92: ONE holder read per connection. The handshake (dialer
    // allowlist + keys) and the admission (negotiation hello + operator
    // allowlist) validate against THIS generation — grant-at-establish for
    // the session it establishes. A reload landing mid-handshake applies
    // at the NEXT connection; a live session keeps its generation until
    // close/reconnect (AR-67 reconnect=replace, no mid-call yank).
    let generation = {
        let snapshot = options.config.get();
        ConnectionGeneration {
            manifest: Arc::new(daemon_manifest(
                &options.host_id,
                &snapshot.config.tool_allowlist,
            )),
            snapshot,
        }
    };
    let ws = match tokio_tungstenite::accept_async_with_config(
        stream,
        // Boot-scoped (GC #7): the envelope cap never hot-applies.
        Some(ws_config(config.max_envelope_bytes)),
    )
    .await
    {
        Ok(ws) => ws,
        Err(e) => {
            tracing::debug!(error = %e, "peer-tools WS upgrade failed");
            // QC-fix W-A: the accept reservation is released — the
            // connection never reaches the handshake.
            sessions.release_in_flight();
            return;
        }
    };
    let observed = ObservedTransport::new(Arc::new(WsTransport::new(ws)));
    let responder = connect_responder(ConnectResponderOptions {
        transport: Arc::clone(&observed) as Arc<dyn Transport>,
        identity: RemoteIdentity {
            seed: options.identity_seed,
        },
        // Handshake admission material from THIS connection's generation
        // (DF-92): a fresh snapshot's peer_ids + keys gate the dialer.
        manifest: (*generation.manifest).clone(),
        allowlist: generation.snapshot.config.peer_ids.clone(),
        peer_keys: (*generation.snapshot.peer_keys).clone(),
        ports: None,
        // Boot-scoped (GC #7): the invoke timeout never hot-applies.
        invoke_timeout_ms: Some(config.invoke_timeout_ms),
    })
    .await;
    monitor_session(responder, observed, sessions, config, generation, options).await;
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
    config: Arc<PeerToolsConfig>,
    generation: ConnectionGeneration,
    options: PeerResponderOptions,
) {
    // Phase 1: bounded handshake outcome (a dialer that never sends its
    // hello is closed by us after the bound — the responder's own recv would
    // otherwise park forever).
    let handshake_timeout = Duration::from_millis(config.invoke_timeout_ms.max(1000));
    let established = tokio::time::timeout(handshake_timeout, wait_until_established(&responder))
        .await
        .ok()
        .flatten();
    let Some(peer_id) = established else {
        // Handshake failed (rejection → responder closed itself) or timed
        // out: close the responder so the dialer fails fast. Zero session
        // state — the manager never saw this peer. The accept reservation
        // (QC-fix W-A) is released here.
        responder.close();
        sessions.release_in_flight();
        return;
    };

    // The registry takes the protocol-neutral port, so the wire responder is
    // wrapped EXACTLY ONCE per session. The SAME wrapper must be used for the
    // admission below and for the close-observation eviction in Phase 3:
    // `PeerToolRegistry::evict_peer` guards with `Arc::ptr_eq` against the
    // wrapper the admission stored, so a second wrapper never matches and the
    // peer's rows would survive a disconnect (AR-68 #8 honesty break).
    let port: Arc<dyn crate::execution::peer_tools::PeerResponder> =
        Arc::new(crate::connect::table::ConnectResponderAdapter::new(
            Arc::clone(&responder),
            peer_id.clone(),
        ));

    // Phase 2: admit. T3 (AR-68): the authenticated manifest's tool ids run
    // the full admission chain inside the process-global PeerToolTable
    // (whole-manifest validation → grammar → reserved-ns → negotiated →
    // allowlist → duplicate-peer). The session manager records the
    // admitted subset (T2 granularity preserved for eviction bookkeeping).
    let admitted_ids: Vec<String> = responder
        .remote_manifest()
        .map(|manifest| {
            // Negotiation (AR-69 #1): the daemon hello `capabilities[]` is
            // the negotiated-membership set (baseline ∪ operator-allowlisted
            // tool ids) — derived from THIS connection's generation (DF-92):
            // the same allowlist-derived hello the handshake negotiated
            // with, so admission stays coherent with it.
            let daemon_caps: std::collections::HashSet<String> =
                generation.manifest.capabilities.iter().cloned().collect();
            let allowlist: std::collections::HashSet<String> =
                generation.snapshot.config.tool_allowlist.iter().cloned().collect();
            // W-A (V1.176 P1 QC fix): the reserved set is derived LIVE from
            // the shared holder at admission time — hot-reloaded user-cap
            // names stay reserved against peer admission.
            let reserved = live_reserved_tool_ids(options.capability_registry.as_ref());
            match crate::connect::peer_tool_table().admit_and_register(
                &peer_id,
                &manifest,
                &port,
                &daemon_caps,
                &allowlist,
                &reserved,
            ) {
                crate::connect::AdmissionOutcome::Admitted { tool_ids } => tool_ids,
                crate::connect::AdmissionOutcome::ManifestInvalid { message } => {
                    tracing::warn!(%peer_id, error = %message, "peer manifest rejected (zero ingestion)");
                    Vec::new()
                }
            }
        })
        .unwrap_or_default();
    let event_session_id = uuid::Uuid::new_v4().to_string();
    observed.bind_session(&event_session_id);
    serve_subscribe_tool(&responder, event_session_id.clone());
    let replaced = sessions.register(&peer_id, Arc::clone(&responder), &admitted_ids);
    tracing::info!(%peer_id, replaced, "peer session established");

    // Phase 3: close observation (see `wait_for_close`).
    wait_for_close(&responder, &observed).await;
    crate::connect::events::connect_event_registry().remove_session(&event_session_id);
    let evicted = sessions.evict(&peer_id, Some(&responder));
    if evicted {
        // AR-68 #8: same tick as close observation — the PeerToolTable rows
        // for this peer disappear from the spine + catalog. The wrapper
        // hoisted above (the one the admission stored) is what the registry's
        // expected-responder guard compares against, so the eviction actually
        // lands.
        crate::connect::peer_tool_table().evict_peer(&peer_id, Some(&port));
        tracing::info!(%peer_id, "peer session evicted after close observation");
    }
}

/// Await close observation for one session. Primary path = the wrapper's
/// `Notify` (fires the same tick the transport reports an error/close);
/// fallback = the responder state poll (catches a close the wrapper missed,
/// e.g. a local `close_session` without a transport error). The flag is
/// re-checked after the future is created to close the notify-counter race;
/// the poll tick is the belt-and-braces fallback.
async fn wait_for_close(responder: &Arc<ConnectResponder>, observed: &ObservedTransport) {
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
}

/// Serve `tools.nexus.subscribe` for one session (§A.2a(f)1).
///
/// Registration is gated on the peer advertising the consumer-served
/// `tools.nexus.deliver_events` capability — the §A.2a(a) reverse-leg check:
/// without it the native registered-or-deny path answers `op_unsupported`
/// with zero side effects.
fn serve_subscribe_tool(responder: &Arc<ConnectResponder>, session_id: String) {
    let has_delivery_capability = responder.remote_manifest().is_some_and(|manifest| {
        manifest
            .capabilities
            .iter()
            .any(|capability| capability == DELIVER_TOOL)
    });
    if !has_delivery_capability {
        return;
    }
    register_subscribe_handler(
        responder,
        session_id,
        crate::connect::events::connect_event_registry().clone(),
        Arc::new(tokio::sync::Mutex::new(())),
    );
}

/// Register the async `tools.nexus.subscribe` handler on `responder`.
///
/// The handler performs the ring's atomic subscribe and drives ack-gated
/// delivery. Its success response carries `{stream, epoch, resumed_from}` and
/// no frames; the first delivery push is held back until the
/// `ObservedTransport` seam observes that response write (§A.2a(f)5).
///
/// The closure captures only a [`Weak`] handle to the responder: the handler
/// is stored inside that same responder, so a strong capture would form an
/// ownership cycle (`close_session` never clears the handler map) and retain
/// every eligible disconnected session.
fn register_subscribe_handler(
    responder: &Arc<ConnectResponder>,
    session_id: String,
    events: crate::connect::events::ConnectEventRegistry,
    delivery_lock: Arc<tokio::sync::Mutex<()>>,
) {
    let weak = Arc::downgrade(responder);
    let handler: spoke_connect::remote::ToolHandler = Arc::new(move |arguments| {
        let events = events.clone();
        let weak = weak.clone();
        let session = session_id.clone();
        let delivery_lock = Arc::clone(&delivery_lock);
        Box::pin(async move {
            let Some(responder) = weak.upgrade() else {
                return nexus_spoke_adapter::SpokeResult::Reject(
                    nexus_spoke_adapter::SpokeReject {
                        code: nexus_spoke_adapter::SpokeRejectCode::InternalError,
                        message: "connect session closed before the subscription was admitted"
                            .to_owned(),
                        details: None,
                    },
                );
            };
            let Some(stream) = arguments
                .get("stream")
                .and_then(serde_json::Value::as_str)
                .filter(|stream| !stream.is_empty())
                .map(str::to_owned)
            else {
                return nexus_spoke_adapter::SpokeResult::Reject(
                    nexus_spoke_adapter::SpokeReject {
                        code: nexus_spoke_adapter::SpokeRejectCode::InvalidInput,
                        message: "subscribe requires a non-empty stream".to_owned(),
                        details: None,
                    },
                );
            };
            let cursor = match arguments.get("last_event_id") {
                None => None,
                Some(serde_json::Value::String(cursor)) => Some(cursor.clone()),
                _ => {
                    return nexus_spoke_adapter::SpokeResult::Reject(
                        nexus_spoke_adapter::SpokeReject {
                            code: nexus_spoke_adapter::SpokeRejectCode::InvalidInput,
                            message: "last_event_id must be a string when provided".to_owned(),
                            details: None,
                        },
                    )
                }
            };
            let Ok((epoch, resumed_from, subscription)) =
                events.subscribe(&session, &stream, cursor.as_deref()).await
            else {
                return nexus_spoke_adapter::SpokeResult::Reject(
                    nexus_spoke_adapter::SpokeReject {
                        code: nexus_spoke_adapter::SpokeRejectCode::InvalidInput,
                        message: "invalid or unavailable Connect event cursor".to_owned(),
                        details: None,
                    },
                );
            };
            let cancelled = subscription.cancellation();
            let delivery_stream = stream.clone();
            tokio::spawn(drive_delivery(
                subscription,
                cancelled,
                Arc::clone(&delivery_lock),
                session.clone(),
                move |frames| {
                    let responder = Arc::clone(&responder);
                    let delivery_stream = delivery_stream.clone();
                    async move {
                        responder
                            .invoke_tool(
                                DELIVER_TOOL,
                                serde_json::json!({"stream": delivery_stream, "frames": frames}),
                            )
                            .await
                    }
                },
            ));
            nexus_spoke_adapter::SpokeResult::Ok(serde_json::json!({
                "stream": stream, "epoch": epoch, "resumed_from": resumed_from
            }))
        })
            as futures_util::future::BoxFuture<
                'static,
                nexus_spoke_adapter::SpokeResult<serde_json::Value>,
            >
    });
    let _ = responder.register_tool_handler(SUBSCRIBE_TOOL, handler);
}

/// Drive ack-gated delivery for one subscription until it is cancelled or a
/// reverse invoke fails. `push` performs one reverse invocation of
/// `tools.nexus.deliver_events`.
///
/// Every wait is cancellation-aware: the ack wait (inside
/// `EventSubscription::next_batch`), the per-session send-slot acquisition
/// and the reverse invoke itself. A replaced generation therefore stops
/// promptly, releasing the shared send slot for its replacement or for other
/// streams of the same session.
async fn drive_delivery<P, Fut>(
    mut subscription: crate::connect::events::EventSubscription,
    cancelled: Arc<Notify>,
    delivery_lock: Arc<tokio::sync::Mutex<()>>,
    session: String,
    push: P,
) where
    P: Fn(Vec<crate::connect::events::EventFrame>) -> Fut,
    Fut: std::future::Future<Output = nexus_spoke_adapter::SpokeResult<serde_json::Value>>,
{
    loop {
        let frames = subscription.next_batch().await;
        if frames.is_empty() {
            break;
        }
        // Send admission: interrupted by cancellation, so a replaced
        // generation never waits out another stream's send slot.
        let _guard = tokio::select! {
            biased;
            () = cancelled.notified() => break,
            guard = delivery_lock.lock() => guard,
        };
        if subscription.is_cancelled() {
            break;
        }
        let invoke = push(frames);
        tokio::select! {
            biased;
            () = cancelled.notified() => break,
            result = invoke => match result {
                nexus_spoke_adapter::SpokeResult::Ok(_) => subscription.ack(),
                nexus_spoke_adapter::SpokeResult::Reject(_) => break,
            },
        }
    }
    subscription.unregister(&session);
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

/// Boot helper: load config + persistent identity from `home`, bind the
/// listener, spawn the accept loop + the supervised config watcher.
///
/// AR-69 outbound authz (all fail-closed):
/// - Layer 0 (dialer identity): `config.peer_ids` (handshake allowlist) +
///   `peer_keys.json` (preconfigured Ed25519 keys). Missing/empty ⇒ every
///   dial is rejected at the handshake.
/// - Layer 1 (negotiation): the daemon hello `capabilities[]` = baseline ∪
///   operator-allowlisted tool ids (derived from config, validated at load
///   — never from runtime discovery).
///
/// V1.179 P1 (DF-92): the admission surface is no longer boot-frozen — a
/// digest-poll watcher (`connect/watch.rs`) swaps validated admission
/// fields (allowlist, peer ids, keys, collision policy, ranks) into the
/// live `PeerConfigHolder` for NEW admissions; the boot-scoped fields
/// (`host`, `port`, `max_sessions`, `invoke_timeout_ms`,
/// `max_envelope_bytes`, `embedded_mcp`) and every in-flight session stay
/// restart-scoped (grant-at-establish, AR-67 reconnect=replace).
///
/// `capability_registry` is the shared [`CapabilityRegistryHolder`] (AR-92)
/// the peer lane keeps for the AR-68 #2(ii) reserved-set check — derived
/// LIVE at each admission so hot-reloaded user-capability names stay
/// reserved against peer admission (V1.176 P1 QC fix W-A). `None` reserves
/// only the static builtin host-tool ids.
///
/// # Errors
/// Config load, identity persistence, or listener bind failures are
/// returned as errors — the caller decides whether to fail boot or keep the
/// daemon core running without the peer-tools lane.
pub async fn start_peer_tools_lane(
    home: &Path,
    shutdown: Arc<Notify>,
    capability_registry: Option<CapabilityRegistryHolder>,
) -> CoreResult<PeerToolsLaneHandle> {
    // DF-92: seed the digest baseline BEFORE the boot load — an edit
    // landing anywhere in the boot window diverges from the baseline and
    // the first poll reloads (never absorbed; the capability watcher's
    // W-B rule).
    let boot_digest = peer_config_digest(home);
    let config = Arc::new(
        PeerToolsConfig::load(home).map_err(|e| CoreError::Internal {
            category: format!("peer-tools config load: {e}"),
        })?,
    );
    // DF-91: wire the live config snapshot into the process-global table
    // so admission reads `collision_policy` + `peer_priority` at
    // admission time (live-derivation precedent: `live_reserved_tool_ids`
    // reads the capability holder the same way — p1's reload swaps the
    // Arc and NEW registrations pick up the new policy without further
    // table mutation).
    crate::connect::peer_tool_table().set_config(Some(Arc::new(config.registry_config())));
    // PR #229 F-2 (Cursor Security HIGH): the peer lane binds PLAINTEXT
    // (no WSS — `accept_async_with_config`), so a non-loopback bind must
    // FAIL CLOSED — mirroring the V1.92 daemon HTTP API posture
    // (`boot.rs` requires TLS for non-loopback binds). No TLS support is
    // added to this lane; the operator must keep the default loopback
    // host (`127.0.0.1`, `DEFAULT_CONNECT_HOST`) or the lane refuses to
    // start (warn-and-skip at boot — nothing is admitted). This check
    // runs BEFORE the remote-bind env gate so the lane always fails
    // closed for non-loopback binds, even when the gate is opened.
    if !is_loopback_host(&config.host) {
        return Err(CoreError::Internal {
            category: format!(
                "peer-tools lane refuses non-loopback bind {host}: the lane has no TLS support \
                 (plaintext only); set connect daemon.json host back to the loopback default \
                 127.0.0.1",
                host = config.host,
            ),
        });
    }
    ensure_remote_bind_allowed(&config.host)?;
    let identity_seed =
        identity::load_or_create_identity(home).map_err(|e| CoreError::Internal {
            category: format!("peer-tools identity: {e}"),
        })?;
    let device_id = nexus_home_layout::device_id::get_or_create_device_id(home).map_err(|e| {
        CoreError::Internal {
            category: format!("peer-tools device id resolution: {e}"),
        }
    })?;
    // DF-92: the live config holder is seeded with the boot generation;
    // the watcher (below) swaps validated reloads into it and every
    // connection reads it (see `handle_connection`) — handshake
    // allowlist + keys, the allowlist-derived hello, and the admission
    // allowlist are live per connection. The boot `config` Arc stays the
    // source for every boot-scoped field (GC #7).
    let config_holder = PeerConfigHolder::new(PeerConfigSnapshot {
        config: Arc::clone(&config),
        peer_keys: Arc::new(crate::connect::config::load_peer_keys(home).map_err(|e| {
            CoreError::Internal {
                category: format!("peer-tools key load: {e}"),
            }
        })?),
    });
    let listener = TcpListener::bind((config.host.as_str(), config.port))
        .await
        .map_err(|e| CoreError::Internal {
            category: format!("peer-tools bind {}: {e}", config.host),
        })?;
    let addr = listener.local_addr().map_err(|e| CoreError::Internal {
        category: format!("peer-tools local addr: {e}"),
    })?;
    let sessions = Arc::new(PeerSessionManager::new());
    let options = PeerResponderOptions {
        identity_seed,
        host_id: device_id,
        config: PeerConfigHolder::clone(&config_holder),
        capability_registry,
    };
    // DF-92: the config watcher runs alongside the accept loop. The
    // caller's shutdown Notify stays SINGLE-consumer — `notify_one` stores
    // exactly one permit, so a second direct consumer (the watcher) could
    // steal it and leave the accept loop un-woken (a hung lane, observed
    // by the p0 lane tests). A tiny relay owns the caller's notify and
    // fans out to per-child notifies; `notify_one` STORES permits, so
    // relay-before-registration cannot lose a wakeup.
    let accept_shutdown = Arc::new(Notify::new());
    let watch_shutdown = Arc::new(Notify::new());
    let _shutdown_relay = tokio::spawn({
        let accept_shutdown = Arc::clone(&accept_shutdown);
        let watch_shutdown = Arc::clone(&watch_shutdown);
        async move {
            shutdown.notified().await;
            accept_shutdown.notify_one();
            watch_shutdown.notify_one();
        }
    });
    // DF-92 supervision: the supervisor awaits the watcher task; a watcher
    // panic surfaces as a JoinError, logs ONE `peer config reload degraded`
    // warn, and the lane keeps serving the last-good snapshot until a
    // restart restores reload. `handle.watch_task` is the supervisor.
    //
    // Detachment semantics (p1 QC): dropping/aborting `watch_task`
    // cancels only the supervisor — the inner watcher `JoinHandle`
    // detaches on drop (deliberately NO abort-on-drop guard). The
    // loop's only exit path is the COOPERATIVE one: the lane must
    // consume the caller shutdown Notify — relayed above into
    // `watch_shutdown` — a dropped handle never aborts a mid-apply
    // reload.
    let watch_home = home.to_path_buf();
    let watch_holder = PeerConfigHolder::clone(&config_holder);
    let watch_task = tokio::spawn(async move {
        let _ = supervise_peer_config_watch(spawn_peer_config_watch(
            boot_digest,
            watch_home,
            watch_holder,
            Arc::clone(crate::connect::peer_tool_table()),
            watch_shutdown,
        ))
        .await;
    });
    let task = spawn_accept_loop(
        listener,
        Arc::clone(&config),
        Arc::clone(&sessions),
        options,
        accept_shutdown,
    );
    tracing::info!(
        %addr,
        max_sessions = config.max_sessions,
        allowlisted_tools = config.tool_allowlist.len(),
        allowlisted_peers = config.peer_ids.len(),
        "peer-tools Connect accept loop listening (AR-69: allowlist-derived hello, fail-closed)"
    );

    Ok(PeerToolsLaneHandle {
        addr,
        sessions,
        config: config_holder,
        task,
        watch_task,
    })
}

/// Running peer-tools lane (accept loop + its session registry).
pub struct PeerToolsLaneHandle {
    /// Bound listen address.
    pub addr: std::net::SocketAddr,
    /// Session registry shared by the accept loop (T3's dispatch arm reads
    /// this; the reverse index into the `PeerToolTable` lands with T3).
    pub sessions: Arc<PeerSessionManager>,
    /// Live peer config holder (DF-92): the watcher swaps validated
    /// reloads into it; every connection reads ONE generation from it for
    /// the handshake + admission (grant-at-establish — see
    /// [`handle_connection`]).
    pub config: PeerConfigHolder,
    /// Accept-loop task.
    pub task: JoinHandle<()>,
    /// SUPERVISED config-watcher task (DF-92): the supervisor awaits the
    /// inner watcher, so a watcher panic is logged as ONE `peer config
    /// reload degraded` warn while the lane keeps serving the last-good
    /// snapshot (restart restores reload). Exits when the watcher exits on
    /// the relayed shutdown Notify. Detached by boot (like `task`).
    pub watch_task: JoinHandle<()>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connect::config::CollisionPolicy;

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

    #[tokio::test]
    async fn subscribe_response_write_activates_pending_delivery() {
        let session = format!("ordering-{}", uuid::Uuid::new_v4());
        let stream = format!("stream-{}", uuid::Uuid::new_v4());
        let registry = crate::connect::events::connect_event_registry().clone();
        registry.publish(&stream, "event", serde_json::json!({"n": 1}));
        let (_, _, mut subscription) = registry.subscribe(&session, &stream, None).await.unwrap();
        let pair = spoke_connect::remote::loopback_transport_pair();
        let observed = ObservedTransport::new(Arc::new(pair.client) as Arc<dyn Transport>);
        observed.bind_session(&session);
        pair.server
            .send(
                serde_json::to_vec(&serde_json::json!({
                    "op": SUBSCRIBE_TOOL, "request_id": "subscribe-1",
                    "payload": {"arguments": {"stream": &stream}}
                }))
                .unwrap()
                .as_slice(),
            )
            .await
            .unwrap();
        observed.recv().await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(20), subscription.next_batch())
                .await
                .is_err()
        );
        observed
            .send(
                serde_json::to_vec(&serde_json::json!({
                    "request_id": "subscribe-1", "payload": {"result": {"stream": &stream}}
                }))
                .unwrap()
                .as_slice(),
            )
            .await
            .unwrap();
        let pushed = tokio::time::timeout(Duration::from_secs(1), subscription.next_batch())
            .await
            .unwrap();
        assert_eq!(pushed.len(), 1);
        registry.remove_session(&session);
        pair.server.close().await.unwrap();
    }

    /// C1: an older subscribe response must never activate a replacement
    /// generation. With interleaved invokes (two requests observed before
    /// either response), the first response enables the generation it was
    /// bound to; the replacement stays dark until its own response write.
    #[tokio::test]
    async fn overlapping_subscribe_response_never_activates_the_replacement() {
        let session = format!("overlap-{}", uuid::Uuid::new_v4());
        let stream = format!("stream-{}", uuid::Uuid::new_v4());
        let registry = crate::connect::events::connect_event_registry().clone();
        let first_frame = registry.publish(&stream, "event", serde_json::json!({"n": 1}));
        let pair = spoke_connect::remote::loopback_transport_pair();
        let observed = ObservedTransport::new(Arc::new(pair.client) as Arc<dyn Transport>);
        observed.bind_session(&session);

        // Request A observed, then its generation registered (awaiting).
        send_subscribe_request(&pair.server, &stream, "subscribe-a").await;
        observed.recv().await.unwrap();
        let (_, _, mut first) = registry.subscribe(&session, &stream, None).await.unwrap();
        // Request B observed while A's response is still outstanding.
        send_subscribe_request(&pair.server, &stream, "subscribe-b").await;
        observed.recv().await.unwrap();

        // A's response write enables A's generation.
        send_subscribe_response(observed.as_ref(), &stream, "subscribe-a").await;
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), first.next_batch())
                .await
                .expect("A's response must enable A's generation"),
            vec![first_frame]
        );

        // B's generation can now register, but B's response is unwritten.
        let (_, _, mut replacement) = registry.subscribe(&session, &stream, None).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(20), replacement.next_batch())
                .await
                .is_err(),
            "no replacement delivery may escape before B's own response"
        );
        // B's own response write enables it.
        send_subscribe_response(observed.as_ref(), &stream, "subscribe-b").await;
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), replacement.next_batch())
                .await
                .expect("B's response must enable B's generation")
                .len(),
            1
        );
        registry.remove_session(&session);
        pair.server.close().await.unwrap();
    }

    /// C2: a replacement must interrupt a reverse invoke that is blocked
    /// while holding the session send slot, freeing it for another stream's
    /// subscription already waiting on that slot.
    #[tokio::test]
    async fn replacement_releases_a_blocked_send_slot_for_another_stream() {
        let registry = crate::connect::events::connect_event_registry().clone();
        let session = format!("slot-{}", uuid::Uuid::new_v4());
        let stream_a = format!("stream-a-{}", uuid::Uuid::new_v4());
        let stream_b = format!("stream-b-{}", uuid::Uuid::new_v4());
        registry.publish(&stream_a, "event", serde_json::json!({"n": 1}));
        registry.publish(&stream_b, "event", serde_json::json!({"n": 2}));
        let delivery_lock = Arc::new(tokio::sync::Mutex::new(()));

        let (_, _, first) = registry.subscribe(&session, &stream_a, None).await.unwrap();
        registry.activate(&session, &stream_a);
        let (_, _, second) = registry.subscribe(&session, &stream_b, None).await.unwrap();
        registry.activate(&session, &stream_b);

        let invoke_started = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let started = Arc::clone(&invoke_started);
        let blocked = Arc::clone(&release);
        let first_cancelled = first.cancellation();
        let first_driver = tokio::spawn(drive_delivery(
            first,
            first_cancelled,
            Arc::clone(&delivery_lock),
            session.clone(),
            move |_frames| {
                let started = Arc::clone(&started);
                let blocked = Arc::clone(&blocked);
                async move {
                    started.notify_one();
                    blocked.notified().await;
                    nexus_spoke_adapter::SpokeResult::Ok(serde_json::json!({}))
                }
            },
        ));
        let second_pushed = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&second_pushed);
        let second_cancelled = second.cancellation();
        let second_driver = tokio::spawn(drive_delivery(
            second,
            second_cancelled,
            Arc::clone(&delivery_lock),
            session.clone(),
            move |_frames| {
                let flag = Arc::clone(&flag);
                async move {
                    flag.store(true, Ordering::SeqCst);
                    nexus_spoke_adapter::SpokeResult::Ok(serde_json::json!({}))
                }
            },
        ));

        // A is blocked inside its reverse invoke, holding the session slot.
        invoke_started.notified().await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !second_pushed.load(Ordering::SeqCst),
            "the second stream must wait for the session send slot"
        );

        // Replacing A cancels its blocked invoke and releases the slot.
        let _replacement = registry.subscribe(&session, &stream_a, None).await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            while !second_pushed.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("the second stream must push once A releases the send slot");

        first_driver.await.unwrap();
        registry.remove_session(&session);
        second_driver.await.unwrap();
    }

    async fn send_subscribe_request<T: Transport>(transport: &T, stream: &str, request_id: &str) {
        transport
            .send(
                serde_json::to_vec(&serde_json::json!({
                    "op": SUBSCRIBE_TOOL, "request_id": request_id,
                    "payload": {"arguments": {"stream": stream}}
                }))
                .unwrap()
                .as_slice(),
            )
            .await
            .unwrap();
    }

    async fn send_subscribe_response(
        transport: &ObservedTransport,
        stream: &str,
        request_id: &str,
    ) {
        transport
            .send(
                serde_json::to_vec(&serde_json::json!({
                    "request_id": request_id, "payload": {"result": {"stream": stream}}
                }))
                .unwrap()
                .as_slice(),
            )
            .await
            .unwrap();
    }

    /// I1: the registered handler must not form a strong cycle with its
    /// responder (it is stored inside that responder and `close_session`
    /// never clears the handler map). Registering the Weak-capturing handler
    /// must not raise the responder's strong count.
    #[tokio::test]
    async fn subscribe_handler_does_not_retain_the_responder() {
        let pair = spoke_connect::remote::loopback_transport_pair();
        let responder = connect_responder(ConnectResponderOptions {
            transport: Arc::new(pair.client) as Arc<dyn Transport>,
            identity: RemoteIdentity { seed: [7u8; 32] },
            manifest: daemon_manifest("host-under-test", &[]),
            allowlist: Vec::new(),
            peer_keys: std::collections::HashMap::new(),
            ports: None,
            invoke_timeout_ms: Some(1_000),
        })
        .await;
        let before = Arc::strong_count(&responder);
        register_subscribe_handler(
            &responder,
            "session-under-test".to_owned(),
            crate::connect::events::ConnectEventRegistry::default(),
            Arc::new(tokio::sync::Mutex::new(())),
        );
        assert_eq!(
            Arc::strong_count(&responder),
            before,
            "the registered subscribe handler must not retain the responder"
        );
        responder.close();
        drop(pair.server);
    }

    #[test]
    fn daemon_manifest_is_baseline() {
        let manifest = daemon_manifest("device-1", &[]);
        assert!(manifest.capabilities.contains(&"spoke-baseline".to_owned()));
        assert!(manifest.tools.is_empty());
        assert_eq!(manifest.host_id.as_str(), "device-1");
    }

    #[test]
    fn daemon_manifest_never_carries_collision_policy() {
        // DF-91 (AR-69 derivation lock): the peer-visible hello derives
        // ONLY from the operator allowlist. The collision policy + peer
        // priority are operator config consumed by the admission table —
        // they must never leak into the hello.
        let config = PeerToolsConfig {
            collision_policy: CollisionPolicy::PriorityOrder,
            peer_priority: vec!["peer-b".to_owned()],
            ..PeerToolsConfig::default()
        };
        let manifest = daemon_manifest("device-1", &config.tool_allowlist);
        let hello = serde_json::to_string(&manifest).expect("hello serializes");
        assert!(
            !hello.contains("collision_policy"),
            "collision policy must not leak into the peer-visible hello"
        );
        assert!(
            !hello.contains("peer_priority"),
            "peer priority must not leak into the peer-visible hello"
        );
        assert!(
            !hello.contains("priority_order"),
            "policy value must not leak into the peer-visible hello"
        );
    }

    /// PR #229 F-2: a non-loopback `daemon.json` host must fail closed —
    /// the lane has no TLS support (plaintext only), so `start_peer_tools_lane`
    /// refuses to start with the TLS-required message instead of binding
    /// cleartext off-loopback (V1.92 HTTP API posture). The check runs before
    /// the remote-bind env gate, so the refusal is unconditional.
    #[tokio::test]
    async fn non_loopback_bind_fails_closed_without_tls() {
        let home = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(nexus_home_layout::connect_dir(home.path())).expect("mkdir");
        std::fs::write(
            nexus_home_layout::connect_daemon_config_path(home.path()),
            r#"{"host":"0.0.0.0","port":0}"#,
        )
        .expect("write daemon.json");
        let shutdown = Arc::new(Notify::new());
        let Err(err) = start_peer_tools_lane(home.path(), shutdown, None).await else {
            panic!("non-loopback bind must fail closed without TLS")
        };
        let msg = format!("{err:#}");
        assert!(
            msg.contains("non-loopback bind")
                && msg.contains("no TLS support")
                && msg.contains("127.0.0.1"),
            "error must name the TLS refusal and the loopback default: {msg}"
        );
    }

    /// v1.191 P1 T14 (durable §9) — the daemon peer-tools responder is
    /// **tools-only**: its hello declares the spoke baseline plus the exact
    /// operator-allowlisted tool ids, never a KE family, and the operator
    /// allowlist can never name one (a KE capability/family name is refused
    /// at config load with the truthful reason).
    #[test]
    fn v1191_holder_connect_tools_only_hello_declares_no_ke_family() {
        let tool_ids = vec![
            "tools.acme.lookup".to_string(),
            "tools.other.ping".to_string(),
        ];
        let manifest = daemon_manifest("daemon-host-uuid-0000", &tool_ids);
        let manifest_json = serde_json::to_string(&manifest).expect("serializes");
        for forbidden in crate::connect::config::KE_CAPABILITIES
            .iter()
            .chain(crate::connect::config::KE_OPERATION_FAMILIES.iter())
        {
            assert!(
                !manifest.capabilities.iter().any(|c| c == forbidden),
                "a tools-only hello must never declare {forbidden}"
            );
            assert!(
                !manifest_json.contains(&format!("\"{forbidden}\"")),
                "a tools-only hello must never mention {forbidden}: {manifest_json}"
            );
        }
        assert_eq!(
            manifest.capabilities,
            vec![
                "spoke-baseline".to_string(),
                "tools.acme.lookup".to_string(),
                "tools.other.ping".to_string(),
            ],
            "the tools-only hello is the baseline plus the exact allowlisted tool ids"
        );
        assert!(
            manifest.tools.is_empty(),
            "the daemon hello serves no tools of its own"
        );
        assert_eq!(
            manifest.namespaces.len(),
            2,
            "namespaces derive from the tool ids only"
        );

        // An operator cannot allowlist a KE name: config load fails with the
        // truthful reason, so the hello above can never grow one.
        for entry in crate::connect::config::KE_CAPABILITIES
            .iter()
            .chain(crate::connect::config::KE_OPERATION_FAMILIES.iter())
        {
            let dir = tempfile::tempdir().expect("tempdir");
            std::fs::create_dir_all(nexus_home_layout::connect_dir(dir.path())).expect("mkdir");
            std::fs::write(
                nexus_home_layout::connect_daemon_config_path(dir.path()),
                serde_json::json!({ "tool_allowlist": [entry] }).to_string(),
            )
            .expect("write daemon.json");
            let err = crate::connect::config::PeerToolsConfig::load(dir.path())
                .expect_err("a KE name must not be allowlistable");
            assert!(
                matches!(
                    err,
                    crate::connect::config::ConnectConfigError::InvalidAllowlist { .. }
                ),
                "{entry} must be refused as a KE name, got {err:?}"
            );
        }
    }
}
