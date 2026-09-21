//! Retained peer-transport and embedded Model B (in-process MCP) contracts.
//!
//! v1.193 P2-T10: the domain regressions of the retired daemon-runtime
//! fixtures (`peer_tool.rs`, `peer_session.rs`, `embedded_mcp_e2e.rs`,
//! `cross_caller_e2e.rs`, `transport_conformance.rs`, `authz_hello.rs`) whose
//! behavior had no existing core owner. Every case below is labeled
//! **MIGRATED** with its source file/case names; nothing here reproduces the
//! retired HTTP composition — the daemon router, `WorkspaceState`-backed
//! catalog, `ApiState` and the Model A child are neither imported nor
//! emulated.
//!
//! What lives here (all against the retained core library features
//! `connect-client` / `embedded-mcp`):
//! - the core-owned WS [`WsTransport`] conformance bar (opaque envelopes,
//!   order, close semantics, the 2 MiB / non-Binary fail-closed framing);
//! - the accept-loop session lifecycle (`PeerSessionManager` replace/evict,
//!   fail-fast handshake refusals, the registered + in-flight budget);
//! - the embedded Model B shell (`mcp_embedded`) end to end, including that a
//!   disconnect REVOKES a peer tool from the in-process MCP surface;
//! - the boot lane's config-derived hello, its Layer-0 refusals, and the
//!   grant-at-establish session boundary under a live config reload;
//! - the core peer port's honest-refusal mapping (peer wire code preserved).

#![cfg(feature = "connect-client")]
// Justification (repo convention, cf. tests/peer_control.rs): integration-test
// assertions operate on fixed local fixtures where a panic IS the failure
// signal; `.unwrap()`/`.expect()` keep the tests linear and readable.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
#[cfg(feature = "embedded-mcp")]
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use futures_util::future::BoxFuture;
use futures_util::{SinkExt, StreamExt};
use nexus_core::connect::table::ConnectResponderAdapter;
use nexus_core::connect::{
    peer_tool_table, spawn_accept_loop, start_peer_tools_lane, ws_config, PeerConfigHolder,
    PeerConfigSnapshot, PeerResponderOptions, PeerSessionManager, PeerToolsConfig, WsTransport,
    DEFAULT_MAX_ENVELOPE_BYTES,
};
#[cfg(feature = "embedded-mcp")]
use nexus_core::execution::peer_tools::invoke_peer_tool;
use nexus_core::execution::peer_tools::{PeerInvokeError, PeerResponder};
use nexus_spoke_adapter::{HostCapabilityManifest, SpokeResult};
use serde_json::{json, Value};
use serial_test::serial;
use spoke_connect::core::derive_peer_id_from_ed25519_pubkey;
use spoke_connect::remote::{
    connect_remote_adapter, connect_responder, ConnectResponderOptions, RemoteAdapter,
    RemoteAdapterError, RemoteAdapterOptions, RemoteAdapterState, RemoteIdentity, ToolHandler,
    Transport, TransportError,
};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

// ── Fixed seeds and tool ids ──────────────────────────────────────────────
//
// Distinct from every other fixture in this crate (and from the retired
// daemon fixtures' `[0x10..0x50]` / `tools.t2..t4.*` ranges) so the
// process-global peer registry never mixes fixtures.

const fn seed_host() -> [u8; 32] {
    [0xd0; 32]
}
const fn seed_peer(n: u8) -> [u8; 32] {
    [0x70 + n; 32]
}

fn pubkey(seed: [u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(&seed).verifying_key().to_bytes()
}

fn peer_id_of(seed: [u8; 32]) -> String {
    derive_peer_id_from_ed25519_pubkey(&pubkey(seed))
}

// One tool-id namespace per case: the registry is process-global, so a case
// that aborts (an assertion failure) must not be able to collide with, or be
// blocked by, another case's leftover rows.
const TOOL_RECONNECT_REPLACED: &str = "tools.t10rc.replaced";
const TOOL_RECONNECT_ECHO: &str = "tools.t10rc.echo";
const TOOL_HANDSHAKE: &str = "tools.t10hs.echo";
const TOOL_SLOW: &str = "tools.t10sd.slow";
const TOOL_CAP: &str = "tools.t10cap.echo";
#[cfg(feature = "embedded-mcp")]
const TOOL_EMBED_ECHO: &str = "tools.t10em.echo";
#[cfg(feature = "embedded-mcp")]
const TOOL_VIS_ECHO: &str = "tools.t10vi.echo";
#[cfg(feature = "embedded-mcp")]
const TOOL_VIS_PING: &str = "tools.t10vi.ping";
#[cfg(feature = "embedded-mcp")]
const TOOL_OPAQUE: &str = "tools.t10op.opaque";
const TOOL_PROBE_ECHO: &str = "tools.t10pr.echo";
const TOOL_PROBE_GHOST: &str = "tools.t10pr.ghost";
const TOOL_BOOT: &str = "tools.t10bt.echo";
const TOOL_BOOT_EMPTY: &str = "tools.t10be.echo";
const TOOL_ROTATED_OLD: &str = "tools.t10rl.echo";
const TOOL_ROTATED_NEW: &str = "tools.t10rl.rotated";

// ── Manifests ─────────────────────────────────────────────────────────────

/// A dialer hello manifest advertising `tools` with the given `(input,
/// output)` schemas.
fn dialer_manifest_with_schemas(
    host_id: &str,
    tools: &[(&str, Value, Value)],
) -> HostCapabilityManifest {
    let mut capabilities: Vec<String> = vec!["spoke-baseline".to_owned()];
    let mut namespaces: Vec<String> = Vec::new();
    let mut descriptors: Vec<Value> = Vec::new();
    for (id, input, output) in tools {
        capabilities.push((*id).to_owned());
        if let Some(namespace) = id.split('.').nth(1) {
            namespaces.push(namespace.to_owned());
        }
        descriptors.push(json!({
            "schema_version": 1,
            "capability_id": id,
            "op": id,
            "description": format!("{id} test tool"),
            "input": input,
            "output": output,
        }));
    }
    serde_json::from_value(json!({
        "schema_version": 1,
        "host_id": host_id,
        "roles": ["data-store"],
        "capabilities": capabilities,
        "namespaces": namespaces,
        "extensions": {},
        "tools": descriptors,
    }))
    .expect("valid dialer manifest")
}

fn dialer_manifest(host_id: &str, tool_ids: &[&str]) -> HostCapabilityManifest {
    let object = json!({ "type": "object" });
    let tools: Vec<(&str, Value, Value)> = tool_ids
        .iter()
        .map(|id| (*id, object.clone(), object.clone()))
        .collect();
    dialer_manifest_with_schemas(host_id, &tools)
}

// ── Handlers ──────────────────────────────────────────────────────────────

/// An echo tool handler: answers with the arguments echoed back.
fn echo_handler() -> ToolHandler {
    Arc::new(|args: Value| {
        Box::pin(async move { SpokeResult::Ok(json!({ "echo": args })) })
            as BoxFuture<'static, SpokeResult<Value>>
    })
}

/// A parked handler: holds the reverse invoke until `park` is notified.
fn parked_handler(park: Arc<Notify>) -> ToolHandler {
    Arc::new(move |_args: Value| {
        let park = Arc::clone(&park);
        Box::pin(async move {
            park.notified().await;
            SpokeResult::Ok(json!({ "parked": true }))
        }) as BoxFuture<'static, SpokeResult<Value>>
    })
}

// ── Accept-loop harness ───────────────────────────────────────────────────

struct PeerHarness {
    addr: std::net::SocketAddr,
    sessions: Arc<PeerSessionManager>,
    shutdown: Arc<Notify>,
    task: JoinHandle<()>,
}

impl PeerHarness {
    /// Stop the accept loop and drop every row the fixture admitted.
    async fn stop(self, peers: &[String]) {
        self.shutdown.notify_one();
        let _ = self.task.await;
        for peer_id in peers {
            peer_tool_table().evict_peer(peer_id, None);
        }
    }
}

/// Bind an accept loop on an ephemeral loopback port with the given admission
/// policy (MIGRATED harness from `peer_session.rs`/`peer_tool.rs`, minus the
/// daemon router/`WorkspaceState` the retired fixtures booted around it).
async fn start_server(
    max_sessions: usize,
    peer_ids: Vec<String>,
    peer_keys: HashMap<String, [u8; 32]>,
    tool_allowlist: Vec<String>,
) -> PeerHarness {
    let config = Arc::new(PeerToolsConfig {
        host: "127.0.0.1".to_owned(),
        port: 0,
        max_sessions,
        invoke_timeout_ms: 2000,
        max_envelope_bytes: DEFAULT_MAX_ENVELOPE_BYTES,
        tool_allowlist,
        peer_ids,
        ..PeerToolsConfig::default()
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let sessions = Arc::new(PeerSessionManager::new());
    let shutdown = Arc::new(Notify::new());
    // The hello derives per connection from the live holder — the harness
    // seeds it with the boot generation (allowlist + keys).
    let options = PeerResponderOptions {
        identity_seed: seed_host(),
        host_id: "nexus-core-test".to_owned(),
        config: PeerConfigHolder::new(PeerConfigSnapshot {
            config: Arc::clone(&config),
            peer_keys: Arc::new(peer_keys),
        }),
        capability_registry: None,
    };
    let task = spawn_accept_loop(
        listener,
        config,
        Arc::clone(&sessions),
        options,
        Arc::clone(&shutdown),
    );
    PeerHarness {
        addr,
        sessions,
        shutdown,
        task,
    }
}

fn ws_config_for_tests() -> tokio_tungstenite::tungstenite::protocol::WebSocketConfig {
    ws_config(DEFAULT_MAX_ENVELOPE_BYTES)
}

/// Dial the harness daemon (fixed `seed_host()` identity) with a spoke adapter
/// over the core [`WsTransport`].
async fn dial(
    addr: std::net::SocketAddr,
    seed: [u8; 32],
    tool_ids: &[&str],
) -> Result<Arc<RemoteAdapter>, RemoteAdapterError> {
    dial_with_manifest(addr, seed, dialer_manifest("dialer", tool_ids)).await
}

/// Dial the core accept loop with an explicit manifest.
async fn dial_with_manifest(
    addr: std::net::SocketAddr,
    seed: [u8; 32],
    manifest: HostCapabilityManifest,
) -> Result<Arc<RemoteAdapter>, RemoteAdapterError> {
    let url = format!("ws://{addr}/connect");
    let stream = TcpStream::connect(addr)
        .await
        .map_err(|e| RemoteAdapterError::Handshake(format!("tcp connect failed: {e}")))?;
    let (ws, _) =
        tokio_tungstenite::client_async_with_config(url, stream, Some(ws_config_for_tests()))
            .await
            .map_err(|e| RemoteAdapterError::Handshake(format!("ws upgrade failed: {e}")))?;
    let transport: Arc<dyn Transport> = Arc::new(WsTransport::new(ws));
    connect_remote_adapter(RemoteAdapterOptions {
        transport,
        local_identity: RemoteIdentity { seed },
        local_manifest: manifest,
        remote_pubkey: pubkey(seed_host()),
        allowlist: vec![derive_peer_id_from_ed25519_pubkey(&pubkey(seed_host()))],
        invoke_timeout_ms: Some(5000),
        capability_token: None,
    })
    .await
}

/// Await a condition until it holds or the deadline elapses.
async fn wait_until(mut cond: impl FnMut() -> bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    cond()
}

// ── Core-owned WS transport conformance ───────────────────────────────────

/// A (client, server) pair of core [`WsTransport`]s over a real `TcpListener`
/// on `127.0.0.1:0`.
async fn ws_pair() -> (Arc<dyn Transport>, Arc<dyn Transport>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let config = ws_config_for_tests();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let ws = tokio_tungstenite::accept_async_with_config(stream, Some(config))
            .await
            .unwrap();
        Arc::new(WsTransport::new(ws)) as Arc<dyn Transport>
    });
    let url = format!("ws://127.0.0.1:{}/connect", addr.port());
    let (client_ws, _resp) = tokio_tungstenite::client_async_with_config(
        url,
        TcpStream::connect(addr).await.unwrap(),
        Some(config),
    )
    .await
    .unwrap();
    let client = Arc::new(WsTransport::new(client_ws)) as Arc<dyn Transport>;
    (client, server.await.unwrap())
}

/// MIGRATED from `transport_conformance.rs::conformance_roundtrip_integrity`
/// and `::conformance_order_preservation`.
///
/// Preserved: one connect envelope == one WS message (payloads carrying
/// newlines / NUL / invalid UTF-8 / the full byte range round-trip
/// byte-identical in BOTH directions), and message order is preserved in both
/// directions under interleaved writers.
///
/// Narrowed: only the core-owned [`WsTransport`] is exercised. The retired
/// fixture parametrized the same bar over spoke-connect's
/// `loopback_transport_pair()`; that transport's semantics belong to the
/// spoke-connect crate's own `tests/remote_loopback.rs` family (Duplicate).
#[tokio::test]
async fn ws_transport_roundtrips_opaque_envelopes_and_preserves_order() {
    let (client, server) = ws_pair().await;

    let vectors: Vec<Vec<u8>> = vec![
        b"plain-ascii".to_vec(),
        b"line1\nline2\r\nline3".to_vec(),
        b"nul\x00inside\x00".to_vec(),
        vec![0xff, 0xfe, 0x00, 0x01], // invalid UTF-8
        (0..=255u8).collect(),        // full byte range
    ];
    for payload in &vectors {
        client.send(payload).await.unwrap();
        assert_eq!(
            server.recv().await.unwrap(),
            *payload,
            "client→server envelope must round-trip byte-identical"
        );
    }
    for payload in &vectors {
        server.send(payload).await.unwrap();
        assert_eq!(
            client.recv().await.unwrap(),
            *payload,
            "server→client envelope must round-trip byte-identical"
        );
    }

    // Order preservation, both directions, interleaved.
    let n: u8 = 32;
    for i in 0..n {
        client.send(&[i]).await.unwrap();
        server.send(&[0xff - i]).await.unwrap();
    }
    for i in 0..n {
        assert_eq!(
            server.recv().await.unwrap(),
            vec![i],
            "client→server order broken at {i}"
        );
        assert_eq!(
            client.recv().await.unwrap(),
            vec![0xff - i],
            "server→client order broken at {i}"
        );
    }

    client.close().await.unwrap();
    server.close().await.unwrap();
}

/// MIGRATED from `transport_conformance.rs::conformance_close_semantics`.
///
/// Preserved: a pending `recv` fails fast with `Closed` once the peer closes,
/// a `recv` after close fails fast rather than parking, and `close()` is
/// idempotent on both ends.
#[tokio::test]
async fn ws_transport_close_semantics_fail_fast_and_are_idempotent() {
    let (client, server) = ws_pair().await;

    let recv_task = tokio::spawn({
        let client = Arc::clone(&client);
        async move { client.recv().await }
    });
    // Let the recv park before the peer closes.
    tokio::task::yield_now().await;
    server.close().await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), recv_task)
        .await
        .expect("pending recv must resolve after peer close")
        .expect("recv task must not panic");
    assert_eq!(
        result,
        Err(TransportError::Closed),
        "pending recv must fail with Closed after peer close"
    );

    assert_eq!(
        client.recv().await,
        Err(TransportError::Closed),
        "recv after close must fail fast (Closed)"
    );

    client.close().await.unwrap();
    client.close().await.unwrap();
    server.close().await.unwrap();
    server.close().await.unwrap();
}

/// MIGRATED from `transport_conformance.rs::conformance_envelope_cap_exceeds_2mib`
/// and `::conformance_non_binary_rejected`.
///
/// Preserved: an inbound message over the 2 MiB envelope cap fails the
/// receiver closed (`TransportError::Io`) and leaves the session unusable; a
/// non-Binary (Text) inbound frame fails the same way. Both are WS-framing
/// properties of the core transport (`max_message_size` + the Binary-only
/// envelope contract) and therefore apply to the WS pair only.
#[tokio::test]
async fn ws_transport_fails_closed_on_over_cap_and_non_binary_frames() {
    // Over-cap: the write side is unbounded, so the send may complete or fail
    // with a transport error if the receiver's fail-closed teardown resets
    // mid-write. The cap is enforced on the receiver.
    let (client, server) = ws_pair().await;
    let oversized = vec![0xab; DEFAULT_MAX_ENVELOPE_BYTES + 1];
    let send_task = tokio::spawn({
        let client = Arc::clone(&client);
        async move { client.send(&oversized).await }
    });
    let result = server.recv().await;
    match send_task.await.unwrap() {
        Ok(()) | Err(TransportError::Io(_)) => {}
        Err(other) => panic!("sender must complete or fail with Io, got {other:?}"),
    }
    assert!(
        matches!(result, Err(TransportError::Io(_))),
        "inbound message over the 2 MiB cap must fail with Io (fail-closed), got {result:?}"
    );
    assert!(
        matches!(
            server.send(&[0u8; 16]).await,
            Err(TransportError::Io(_) | TransportError::Closed)
        ),
        "the session must be unusable after the cap rejection"
    );
    client.close().await.unwrap();
    server.close().await.unwrap();

    // Non-Binary frame: needs a raw tungstenite sink (the `Transport::send`
    // seam only emits Binary).
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let config = ws_config_for_tests();
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let ws = tokio_tungstenite::accept_async_with_config(stream, Some(config))
            .await
            .unwrap();
        Arc::new(WsTransport::new(ws)) as Arc<dyn Transport>
    });
    let url = format!("ws://127.0.0.1:{}/connect", addr.port());
    let (client_ws, _resp) = tokio_tungstenite::client_async_with_config(
        url,
        TcpStream::connect(addr).await.unwrap(),
        Some(config),
    )
    .await
    .unwrap();
    let (mut sink, _stream) = client_ws.split();
    let server = server_task.await.unwrap();
    sink.send(tokio_tungstenite::tungstenite::protocol::Message::Text(
        "not-an-envelope".into(),
    ))
    .await
    .unwrap();
    assert!(
        matches!(server.recv().await, Err(TransportError::Io(_))),
        "a non-Binary inbound message must fail the receiver closed"
    );
    assert!(
        matches!(
            server.send(&[0u8; 8]).await,
            Err(TransportError::Io(_) | TransportError::Closed)
        ),
        "the session must be unusable after the non-Binary rejection"
    );
    server.close().await.unwrap();
}

/// MIGRATED from `transport_conformance.rs::conformance_golden_path_handshake_and_tool_invoke`.
///
/// Preserved: over the core [`WsTransport`] a full spoke handshake completes on
/// BOTH ends (adapter `Established` + the responder's observed peer manifest)
/// and one reverse tool invoke round-trips to the registered handler exactly
/// once. Narrowed to the WS half (see the round-trip case).
#[tokio::test]
async fn ws_transport_golden_path_handshake_and_reverse_invoke() {
    const SEED_CLIENT: [u8; 32] = [0x11; 32];
    let (client, server) = ws_pair().await;

    let peer_id_client = derive_peer_id_from_ed25519_pubkey(&pubkey(SEED_CLIENT));
    let responder = connect_responder(ConnectResponderOptions {
        transport: server,
        identity: RemoteIdentity { seed: seed_host() },
        manifest: dialer_manifest("test-responder", &[TOOL_PROBE_ECHO]),
        allowlist: vec![peer_id_client.clone()],
        peer_keys: HashMap::from([(peer_id_client, pubkey(SEED_CLIENT))]),
        ports: None,
        invoke_timeout_ms: None,
    })
    .await;
    let invocations = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    responder.register_tool_handler(
        TOOL_PROBE_ECHO,
        Arc::new({
            let invocations = Arc::clone(&invocations);
            move |args: Value| {
                let invocations = Arc::clone(&invocations);
                Box::pin(async move {
                    invocations.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    SpokeResult::Ok(json!({ "echo": args }))
                }) as BoxFuture<'static, SpokeResult<Value>>
            }
        }),
    );

    let adapter = connect_remote_adapter(RemoteAdapterOptions {
        transport: client,
        local_identity: RemoteIdentity { seed: SEED_CLIENT },
        local_manifest: dialer_manifest("test-client", &[TOOL_PROBE_ECHO]),
        remote_pubkey: pubkey(seed_host()),
        allowlist: vec![derive_peer_id_from_ed25519_pubkey(&pubkey(seed_host()))],
        invoke_timeout_ms: None,
        capability_token: None,
    })
    .await
    .unwrap();

    assert_eq!(adapter.state(), RemoteAdapterState::Established);
    assert_eq!(responder.state(), RemoteAdapterState::Established);
    assert_eq!(
        responder
            .remote_manifest()
            .expect("peer manifest")
            .host_id
            .to_string(),
        "test-client"
    );

    let result = adapter
        .invoke_tool(TOOL_PROBE_ECHO, json!({ "n": 1 }))
        .await;
    assert_eq!(
        result,
        SpokeResult::Ok(json!({ "echo": { "n": 1 } })),
        "one reverse invoke round-trips through the core WS transport"
    );
    assert_eq!(
        invocations.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "the handler ran exactly once"
    );

    adapter.close();
    responder.close();
}

// ── Accept-loop session lifecycle ─────────────────────────────────────────

/// MIGRATED from `peer_session.rs::second_session_same_peer_replaces_first`.
///
/// Preserved: a second session for the SAME peer id is deterministic
/// last-wins — exactly one live session remains, the replaced dialer observes
/// its close, and the prior generation's admitted tool row is evicted from the
/// process-global table while the fresh admission's row appears.
#[tokio::test]
#[serial]
async fn reconnect_for_a_peer_replaces_the_session_and_its_tool_rows() {
    let peer_id = peer_id_of(seed_peer(0));
    let harness = start_server(
        8,
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(0)))]),
        vec![
            TOOL_RECONNECT_REPLACED.to_owned(),
            TOOL_RECONNECT_ECHO.to_owned(),
        ],
    )
    .await;

    let first = dial(harness.addr, seed_peer(0), &[TOOL_RECONNECT_REPLACED])
        .await
        .expect("first dial");
    first.register_tool_handler(TOOL_RECONNECT_REPLACED, echo_handler());
    assert!(
        wait_until(
            || harness.sessions.get(&peer_id).is_some(),
            Duration::from_secs(5)
        )
        .await
    );
    assert_eq!(
        harness
            .sessions
            .get(&peer_id)
            .expect("session")
            .admitted_ids,
        vec![TOOL_RECONNECT_REPLACED.to_owned()]
    );
    assert_eq!(harness.sessions.session_count(), 1);
    assert!(peer_tool_table().get(TOOL_RECONNECT_REPLACED).is_some());

    // Same peer id, different manifest: last-wins replace.
    let second = dial(harness.addr, seed_peer(0), &[TOOL_RECONNECT_ECHO])
        .await
        .expect("reconnect");
    second.register_tool_handler(TOOL_RECONNECT_ECHO, echo_handler());
    assert!(
        wait_until(
            || harness
                .sessions
                .get(&peer_id)
                .is_some_and(|rec| rec.admitted_ids == vec![TOOL_RECONNECT_ECHO.to_owned()]),
            Duration::from_secs(5)
        )
        .await,
        "the reconnect admits the fresh manifest"
    );
    assert_eq!(
        harness.sessions.session_count(),
        1,
        "exactly one live session per peer id"
    );
    assert!(
        peer_tool_table().get(TOOL_RECONNECT_REPLACED).is_none(),
        "the replaced generation's rows are evicted"
    );
    assert!(
        peer_tool_table().get(TOOL_RECONNECT_ECHO).is_some(),
        "the replacement generation's rows are admitted"
    );
    assert_eq!(second.state(), RemoteAdapterState::Established);
    assert!(
        wait_until(
            || first.state() == RemoteAdapterState::Closed,
            Duration::from_secs(5)
        )
        .await,
        "the replaced dialer observes its session close"
    );

    first.close();
    second.close();
    harness.stop(&[peer_id]).await;
}

/// MIGRATED from `peer_session.rs::non_allowlisted_peer_is_rejected_at_handshake`
/// and `::allowlisted_peer_without_key_is_rejected_at_handshake`.
///
/// Preserved: the Layer-0 handshake gate fails CLOSED and FAST — a dialer
/// outside the operator peer allowlist, and an allowlisted dialer with no
/// preconfigured key, both fail immediately with zero session state (never a
/// timeout park).
#[tokio::test]
#[serial]
async fn handshake_rejections_fail_fast_with_zero_session_state() {
    // (a) Peer not on the handshake allowlist.
    let allowed = peer_id_of(seed_peer(0));
    let harness = start_server(
        8,
        vec![allowed.clone()],
        HashMap::from([(allowed.clone(), pubkey(seed_peer(0)))]),
        vec![TOOL_HANDSHAKE.to_owned()],
    )
    .await;
    let started = Instant::now();
    let refused = dial(harness.addr, seed_peer(1), &[TOOL_HANDSHAKE]).await;
    assert!(refused.is_err(), "a non-allowlisted dial must fail fast");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "handshake rejection must be immediate (no timeout park)"
    );
    assert_eq!(harness.sessions.session_count(), 0);
    assert!(harness.sessions.peer_ids().is_empty());
    harness.stop(&[allowed]).await;

    // (b) Allowlisted peer with no preconfigured key: fail-closed.
    let keyless = peer_id_of(seed_peer(2));
    let harness = start_server(8, vec![keyless.clone()], HashMap::new(), Vec::new()).await;
    let started = Instant::now();
    let refused = dial(harness.addr, seed_peer(2), &[TOOL_HANDSHAKE]).await;
    assert!(
        refused.is_err(),
        "a missing peer key must fail the handshake"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "handshake rejection must be immediate (no timeout park)"
    );
    assert_eq!(harness.sessions.session_count(), 0);
    assert!(harness.sessions.peer_ids().is_empty());
    harness.stop(&[keyless]).await;
}

/// MIGRATED from `peer_session.rs::transport_drop_evicts_session_and_resolves_in_flight_invoke`.
///
/// Preserved: dropping the dialer's transport is observed by the session
/// monitor, which evicts the session AND resolves an in-flight reverse invoke
/// bounded (as a rejection) instead of parking.
#[tokio::test]
#[serial]
async fn transport_drop_evicts_the_session_and_resolves_the_in_flight_invoke() {
    let peer_id = peer_id_of(seed_peer(0));
    let harness = start_server(
        8,
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(0)))]),
        vec![TOOL_SLOW.to_owned()],
    )
    .await;

    let adapter = dial(harness.addr, seed_peer(0), &[TOOL_SLOW])
        .await
        .expect("dial");
    assert!(
        wait_until(
            || harness.sessions.get(&peer_id).is_some(),
            Duration::from_secs(5)
        )
        .await
    );

    // A parked dialer handler keeps the reverse invoke in flight.
    let park = Arc::new(Notify::new());
    adapter.register_tool_handler(TOOL_SLOW, parked_handler(Arc::clone(&park)));

    let responder = harness.sessions.get(&peer_id).expect("session").responder;
    let invoke = tokio::spawn({
        let responder = Arc::clone(&responder);
        async move { responder.invoke_tool(TOOL_SLOW, json!({})).await }
    });

    adapter.close();
    let result = tokio::time::timeout(Duration::from_secs(5), invoke)
        .await
        .expect("in-flight invoke must resolve bounded (no park)")
        .expect("invoke task must not panic");
    assert!(
        matches!(result, SpokeResult::Reject(_)),
        "dropped-transport invoke must fail with a rejection, got {result:?}"
    );
    assert!(
        wait_until(
            || harness.sessions.session_count() == 0,
            Duration::from_secs(5)
        )
        .await,
        "a transport drop evicts the session"
    );

    harness.stop(&[peer_id]).await;
}

/// MIGRATED from `peer_session.rs::dial_flood_of_incomplete_handshakes_cannot_exceed_session_cap`,
/// `::concurrent_dial_burst_never_exceeds_session_cap`,
/// `::ninth_concurrent_session_is_refused_at_accept` and
/// `::accept_loop_stays_responsive_under_session_load`.
///
/// Preserved: the accept gate bounds registered sessions PLUS in-flight
/// (accepted-but-not-registered) connections together, so a dial flood of
/// never-hello connections cannot exceed the cap (the pre-fix
/// registered-only gate left that window unbounded, QC-fix W-A); a
/// simultaneous dial burst admits exactly the cap and refuses the rest with no
/// in-flight residue (PR #229 F-3); the dial past the cap is refused
/// immediately (the accept loop never blocks on established session work) and
/// a slot freed by an eviction is reusable.
#[tokio::test]
#[serial]
async fn accept_gate_bounds_registered_and_in_flight_connections() {
    // (a) In-flight flood: no WS upgrade ever completes, yet the cap holds.
    let harness = start_server(2, Vec::new(), HashMap::new(), Vec::new()).await;
    let mut sockets = Vec::new();
    for _ in 0..6 {
        sockets.push(TcpStream::connect(harness.addr).await.expect("connect"));
    }
    assert_eq!(
        sockets.len(),
        6,
        "all six raw dials must be open before the gate is asserted"
    );
    assert!(
        wait_until(
            || harness.sessions.connection_count() == 2,
            Duration::from_secs(5)
        )
        .await,
        "the accept loop reserves exactly max_sessions in-flight slots"
    );
    assert_eq!(
        harness.sessions.connection_count(),
        2,
        "registered + in-flight stays at the cap while six dials are open"
    );
    assert_eq!(
        harness.sessions.session_count(),
        0,
        "no dial completes the handshake"
    );
    sockets.clear();
    assert!(
        wait_until(
            || harness.sessions.connection_count() == 0,
            Duration::from_secs(5)
        )
        .await,
        "dropping the dials releases all in-flight reservations"
    );
    harness.stop(&[]).await;

    // (b) Simultaneous dial burst: exactly max_sessions admitted.
    let mut allowlist = Vec::new();
    let mut keys = HashMap::new();
    for i in 0..9 {
        let pid = peer_id_of(seed_peer(i));
        allowlist.push(pid.clone());
        keys.insert(pid, pubkey(seed_peer(i)));
    }
    let harness = start_server(4, allowlist.clone(), keys, vec![TOOL_CAP.to_owned()]).await;
    let mut dials = Vec::new();
    for i in 0..8 {
        dials.push(tokio::spawn(dial(harness.addr, seed_peer(i), &[TOOL_CAP])));
    }
    let mut adapters = Vec::new();
    let mut refused = 0usize;
    for dial_task in dials {
        match dial_task.await.expect("dial task") {
            Ok(adapter) => adapters.push(adapter),
            Err(_) => refused += 1,
        }
    }
    assert_eq!(adapters.len(), 4, "exactly max_sessions dials admitted");
    assert_eq!(refused, 4, "the remaining dials refused at accept");
    assert!(
        wait_until(
            || harness.sessions.session_count() == 4,
            Duration::from_secs(5)
        )
        .await
    );
    assert_eq!(harness.sessions.session_count(), 4, "the cap holds");
    assert_eq!(
        harness.sessions.connection_count(),
        4,
        "no in-flight residue after settlement (slot counted exactly once)"
    );

    // (c) Sequential dial past the cap, then a freed slot.
    let started = Instant::now();
    let ninth = dial(harness.addr, seed_peer(8), &[TOOL_CAP]).await;
    assert!(ninth.is_err(), "the dial past the cap is refused at accept");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the refusal is immediate (the accept loop never parks on session work)"
    );
    assert_eq!(harness.sessions.session_count(), 4);

    adapters[0].close();
    assert!(
        wait_until(
            || harness.sessions.session_count() == 3,
            Duration::from_secs(5)
        )
        .await
    );
    let readmitted = dial(harness.addr, seed_peer(8), &[TOOL_CAP])
        .await
        .expect("the freed slot admits a new peer");
    assert!(
        wait_until(
            || harness.sessions.session_count() == 4,
            Duration::from_secs(5)
        )
        .await
    );
    readmitted.close();

    for adapter in &adapters {
        adapter.close();
    }
    harness.stop(&allowlist).await;
}

// ── Embedded Model B (in-process MCP; needs the nested feature) ───────────

// Embedded-only imports (the cases below are the `embedded-mcp` half of
// this target; the transport/session/boot cases above need `connect-client`
// alone).
#[cfg(feature = "embedded-mcp")]
use nexus_core::connect::mcp_bridge::{CatalogRow, McpBackend, ToolCallOutcome};
#[cfg(feature = "embedded-mcp")]
use nexus_core::connect::mcp_embedded::{
    start_embedded_mcp_server, EmbeddedMcpServer, EmbeddedShutdown,
};
#[cfg(feature = "embedded-mcp")]
use nexus_core::connect::table::{mcp_catalog_admission, mcp_catalog_output_root_object};
#[cfg(feature = "embedded-mcp")]
use nexus_core::connect::VisibilityPolicy;
#[cfg(feature = "embedded-mcp")]
use rmcp::model::{CallToolRequestParams, ClientInfo, ErrorCode};
#[cfg(feature = "embedded-mcp")]
use rmcp::{serve_client, ServiceError};

/// The in-process Model B consumer: the MCP surface over the
/// process-global peer registry, with no HTTP hop, no daemon router and no
/// child process.
///
/// The embedded shell is generic over [`McpBackend`] by design (the
/// retired daemon supplied the `WorkspaceState`-backed implementation), so
/// a core-side regression must supply its own consumer. This one reads the
/// SAME process-global registry the accept loop registers into and
/// projects rows through the core-owned AR-70 §3 catalog gates.
#[cfg(feature = "embedded-mcp")]
#[derive(Clone)]
struct PeerRegistryBackend;

#[cfg(feature = "embedded-mcp")]
fn peer_catalog_rows() -> Vec<CatalogRow> {
    peer_tool_table()
        .entries()
        .into_iter()
        .filter(|entry| mcp_catalog_admission(&entry.descriptor).is_ok())
        .map(|entry| CatalogRow {
            id: String::from(entry.descriptor.capability_id.clone()),
            description: String::from(entry.descriptor.description.clone()),
            input_schema: serde_json::to_string(&entry.descriptor.input)
                .unwrap_or_else(|_| "{}".to_owned()),
            output_schema: mcp_catalog_output_root_object(&entry.descriptor)
                .then(|| serde_json::to_string(&entry.descriptor.output).unwrap_or_default()),
        })
        .collect()
}

#[cfg(feature = "embedded-mcp")]
impl McpBackend for PeerRegistryBackend {
    fn list_tools(&self) -> impl Future<Output = Result<Vec<CatalogRow>, rmcp::ErrorData>> + Send {
        std::future::ready(Ok(peer_catalog_rows()))
    }

    fn call_tool(
        &self,
        tool_name: &str,
        parameters: Value,
    ) -> impl Future<Output = Result<ToolCallOutcome, rmcp::ErrorData>> + Send {
        let name = tool_name.to_owned();
        async move {
            let Some(entry) = peer_tool_table().get(&name) else {
                return Ok(ToolCallOutcome::Unroutable {
                    code: "not_supported".to_owned(),
                    message: format!("tool {name} is not registered by any peer"),
                });
            };
            match invoke_peer_tool(&entry, parameters).await {
                Ok(value) => Ok(ToolCallOutcome::Success(value)),
                Err(PeerInvokeError::Denied { wire_code, message }) => {
                    Ok(ToolCallOutcome::ExecutedError {
                        code: "not_supported".to_owned(),
                        message,
                        wire_code,
                    })
                }
                Err(PeerInvokeError::Timeout { message }) => Ok(ToolCallOutcome::ExecutedError {
                    code: "service_unavailable".to_owned(),
                    message,
                    wire_code: None,
                }),
                Err(PeerInvokeError::Disconnected { message })
                | Err(PeerInvokeError::Internal { message }) => {
                    Ok(ToolCallOutcome::ExecutedError {
                        code: "internal".to_owned(),
                        message,
                        wire_code: None,
                    })
                }
            }
        }
    }
}

/// Establish one embedded session and complete the initialize handshake.
#[cfg(feature = "embedded-mcp")]
async fn establish_session(
    server: &EmbeddedMcpServer<PeerRegistryBackend>,
) -> rmcp::service::RunningService<rmcp::RoleClient, ClientInfo> {
    let session = server.establish().expect("embedded session establish");
    serve_client(ClientInfo::default(), session.transport)
        .await
        .expect("initialize handshake completes")
}

#[cfg(feature = "embedded-mcp")]
async fn listed_tools(
    running: &rmcp::service::RunningService<rmcp::RoleClient, ClientInfo>,
) -> Vec<String> {
    running
        .list_tools(None)
        .await
        .expect("tools/list succeeds")
        .tools
        .iter()
        .map(|tool| tool.name.to_string())
        .collect()
}

/// MIGRATED from `embedded_mcp_e2e.rs::embedded_server_lists_and_calls_builtin_without_child_process`,
/// `::embedded_unroutable_id_is_method_not_found`,
/// `::boot_path_cli_flag_enables_and_no_enablement_leaves_no_server` (core
/// enablement gate), and `peer_tool.rs::disconnect_evicts_rows_table_and_catalog_same_tick`,
/// `::catalog_ids_equal_dispatchable_set_both_directions`,
/// `::unknown_peer_id_is_not_supported_identically_to_unknown_builtin`.
///
/// Preserved: the in-process Model B surface lists exactly what the
/// dispatch registry holds and dispatches a listed id through a REAL
/// reverse invoke to the dialer; on disconnect the row is evicted, the
/// surface stops listing it, and a call is an honest `METHOD_NOT_FOUND`
/// (unroutable) — a STALE admission can never keep dispatching. No
/// enablement means no surface.
///
/// Deleted with the retired host: the daemon boot-wiring assertions
/// (`boot_embedded_mcp_server` storing one instance on `WorkspaceState`,
/// the config-key/CLI-flag union) and the "spawns no child process"
/// expectation — that wiring is the daemon composition, and the Model A
/// child is gone.
#[cfg(feature = "embedded-mcp")]
#[tokio::test]
#[serial]
async fn disconnect_revokes_embedded_peer_tool() {
    let peer_id = peer_id_of(seed_peer(1));
    let harness = start_server(
        8,
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(1)))]),
        vec![TOOL_EMBED_ECHO.to_owned()],
    )
    .await;
    let adapter = dial(harness.addr, seed_peer(1), &[TOOL_EMBED_ECHO])
        .await
        .expect("dial");
    adapter.register_tool_handler(TOOL_EMBED_ECHO, echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get(TOOL_EMBED_ECHO).is_some(),
            Duration::from_secs(5)
        )
        .await,
        "peer tool admitted"
    );

    // Enablement gate: no enablement ⇒ no embedded surface.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    assert!(
        start_embedded_mcp_server(
            PeerRegistryBackend,
            false,
            VisibilityPolicy::absent(),
            EmbeddedShutdown::new(shutdown_rx.clone()),
        )
        .is_none(),
        "no enablement ⇒ no embedded surface"
    );
    let server = start_embedded_mcp_server(
        PeerRegistryBackend,
        true,
        VisibilityPolicy::absent(),
        EmbeddedShutdown::new(shutdown_rx),
    )
    .expect("enabled server");
    let running = establish_session(&server).await;

    // The listed set mirrors the dispatchable registry (lockstep).
    let listed = listed_tools(&running).await;
    assert!(
        listed.contains(&TOOL_EMBED_ECHO.to_owned()),
        "the embedded surface lists the admitted peer tool: {listed:?}"
    );

    // A listed id dispatches through the real in-process reverse invoke.
    let call = running
        .call_tool(CallToolRequestParams::new(TOOL_EMBED_ECHO))
        .await
        .expect("the listed peer tool dispatches");
    assert_eq!(call.is_error, Some(false), "not an error result");
    assert_eq!(
        call.structured_content,
        Some(json!({ "echo": {} })),
        "the peer's own response reached the embedded consumer"
    );

    // Disconnect: the session monitor evicts the row.
    adapter.close();
    assert!(
        wait_until(
            || peer_tool_table().get(TOOL_EMBED_ECHO).is_none(),
            Duration::from_secs(5)
        )
        .await,
        "disconnect evicts the peer row"
    );

    // The stale admission is revoked on the MCP surface: not listed, and a
    // call is unroutable rather than dispatched.
    let listed_after = listed_tools(&running).await;
    assert!(
        !listed_after.contains(&TOOL_EMBED_ECHO.to_owned()),
        "a revoked peer tool leaves the embedded listing: {listed_after:?}"
    );
    let err = running
        .call_tool(CallToolRequestParams::new(TOOL_EMBED_ECHO))
        .await
        .expect_err("a revoked tool is unroutable");
    let ServiceError::McpError(data) = err else {
        panic!("expected McpError, got {err:?}");
    };
    assert_eq!(
        data.code,
        ErrorCode::METHOD_NOT_FOUND,
        "the revoked id is refused as unroutable"
    );

    drop(running);
    drop(server);
    drop(shutdown_tx);
    harness.stop(&[peer_id]).await;
}

/// MIGRATED from `embedded_mcp_e2e.rs::embedded_visibility_policy_filters_list_and_short_circuits_hidden_call`.
///
/// Preserved at the SHELL level (the bridge seam itself is pinned by
/// `connect/mcp_bridge.rs` unit tests): a present visibility policy filters
/// the embedded `tools/list` to the configured subset, a visible peer tool
/// still dispatches, and a hidden one is refused at the seam with the
/// `tool_not_authorized` discriminator — never dispatched.
#[cfg(feature = "embedded-mcp")]
#[tokio::test]
#[serial]
async fn embedded_visibility_policy_filters_the_peer_surface() {
    let peer_id = peer_id_of(seed_peer(2));
    let harness = start_server(
        8,
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(2)))]),
        vec![TOOL_VIS_ECHO.to_owned(), TOOL_VIS_PING.to_owned()],
    )
    .await;
    let adapter = dial(harness.addr, seed_peer(2), &[TOOL_VIS_ECHO, TOOL_VIS_PING])
        .await
        .expect("dial");
    adapter.register_tool_handler(TOOL_VIS_ECHO, echo_handler());
    adapter.register_tool_handler(TOOL_VIS_PING, echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get(TOOL_VIS_ECHO).is_some()
                && peer_tool_table().get(TOOL_VIS_PING).is_some(),
            Duration::from_secs(5)
        )
        .await,
        "both peer tools admitted"
    );

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let server = start_embedded_mcp_server(
        PeerRegistryBackend,
        true,
        VisibilityPolicy::from_visible([TOOL_VIS_ECHO.to_owned()]),
        EmbeddedShutdown::new(shutdown_rx),
    )
    .expect("enabled server");
    let running = establish_session(&server).await;

    assert_eq!(
        listed_tools(&running).await,
        vec![TOOL_VIS_ECHO.to_owned()],
        "only the visible peer tool is listed"
    );
    let call = running
        .call_tool(CallToolRequestParams::new(TOOL_VIS_ECHO))
        .await
        .expect("the visible tool dispatches");
    assert_eq!(call.is_error, Some(false));

    let err = running
        .call_tool(CallToolRequestParams::new(TOOL_VIS_PING))
        .await
        .expect_err("a hidden tool is refused at the seam");
    let ServiceError::McpError(data) = err else {
        panic!("expected McpError, got {err:?}");
    };
    assert_eq!(data.code, ErrorCode::METHOD_NOT_FOUND);
    assert!(
        data.message.contains("tool_not_authorized"),
        "the refusal names the visibility class: {}",
        data.message
    );

    adapter.close();
    drop(running);
    drop(server);
    drop(shutdown_tx);
    harness.stop(&[peer_id]).await;
}

/// MIGRATED from `peer_tool.rs::user_cap_non_object_output_omitted_from_catalog`
/// (the AR-70 §3 inclusion rule, applied to the retained peer registry).
///
/// Preserved: a registered peer row whose input schema is NOT a root
/// `type: "object"` is omitted from the MCP catalog while its registration
/// lane is untouched — the tool stays dispatchable through the registry.
#[cfg(feature = "embedded-mcp")]
#[tokio::test]
#[serial]
async fn embedded_row_with_non_object_input_is_registered_but_unlisted() {
    let peer_id = peer_id_of(seed_peer(3));
    let harness = start_server(
        8,
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(3)))]),
        vec![TOOL_OPAQUE.to_owned()],
    )
    .await;
    let adapter = dial_with_manifest(
        harness.addr,
        seed_peer(3),
        dialer_manifest_with_schemas(
            "dialer",
            &[(
                TOOL_OPAQUE,
                json!({ "type": "string" }),
                json!({ "type": "object" }),
            )],
        ),
    )
    .await
    .expect("dial");
    adapter.register_tool_handler(TOOL_OPAQUE, echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get(TOOL_OPAQUE).is_some(),
            Duration::from_secs(5)
        )
        .await,
        "the non-object row is registered"
    );

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let server = start_embedded_mcp_server(
        PeerRegistryBackend,
        true,
        VisibilityPolicy::absent(),
        EmbeddedShutdown::new(shutdown_rx),
    )
    .expect("enabled server");
    let running = establish_session(&server).await;
    assert!(
        !listed_tools(&running)
            .await
            .contains(&TOOL_OPAQUE.to_owned()),
        "a non-root-object input row is omitted from the MCP catalog"
    );

    // Registration is untouched: the row is still dispatchable.
    let entry = peer_tool_table()
        .get(TOOL_OPAQUE)
        .expect("registration lane untouched");
    assert!(invoke_peer_tool(&entry, json!({})).await.is_ok());

    adapter.close();
    drop(running);
    drop(server);
    drop(shutdown_tx);
    harness.stop(&[peer_id]).await;
}

// ── Hello negotiation, boot lane and grant-at-establish ───────────────────

/// MIGRATED from `authz_hello.rs::negotiation_journey_intersection_and_non_advertised_denied`
/// and `peer_tool.rs::peer_deny_wire_code_survives_to_http_error_details_verbatim`.
///
/// Preserved through the CORE port (`ConnectResponderAdapter`, the daemon's
/// replacement for the retired HTTP error envelope): the negotiated
/// intersection dispatches to the dialer's registered handler, and a
/// non-advertised id is denied with the peer's OWN lowercase wire code
/// preserved verbatim (`op_unsupported`, never re-derived from the message).
#[tokio::test]
#[serial]
async fn non_advertised_peer_id_is_denied_with_the_peers_own_wire_code() {
    let peer_id = peer_id_of(seed_peer(4));
    // The daemon hello advertises BOTH ids (operator allowlist); the dialer
    // advertises only echo — the negotiated set is the intersection.
    let harness = start_server(
        8,
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(4)))]),
        vec![TOOL_PROBE_ECHO.to_owned(), TOOL_PROBE_GHOST.to_owned()],
    )
    .await;
    let adapter = dial(harness.addr, seed_peer(4), &[TOOL_PROBE_ECHO])
        .await
        .expect("dial");
    adapter.register_tool_handler(TOOL_PROBE_ECHO, echo_handler());
    assert!(
        wait_until(
            || harness.sessions.get(&peer_id).is_some(),
            Duration::from_secs(5)
        )
        .await
    );
    assert!(
        peer_tool_table().get(TOOL_PROBE_GHOST).is_none(),
        "an id absent from the dialer's hello is never admitted"
    );

    let port = ConnectResponderAdapter::new(
        Arc::clone(&harness.sessions.get(&peer_id).expect("session").responder),
        peer_id.clone(),
    );
    let dispatched = port.invoke_tool(TOOL_PROBE_ECHO, json!({ "n": 1 })).await;
    assert_eq!(
        dispatched.expect("the negotiated id dispatches"),
        json!({ "echo": { "n": 1 } })
    );

    let denied = port
        .invoke_tool(TOOL_PROBE_GHOST, json!({}))
        .await
        .expect_err("a non-advertised id is denied by the peer");
    let PeerInvokeError::Denied { wire_code, .. } = denied else {
        panic!("a peer deny must map to the port's Denied class, got {denied:?}");
    };
    assert_eq!(
        wire_code.as_deref(),
        Some("op_unsupported"),
        "the peer's own lowercase wire code is preserved verbatim"
    );

    adapter.close();
    harness.stop(&[peer_id]).await;
}

/// Write `daemon.json` + `peer_keys.json` + a fixed daemon identity under
/// `home` (the RAW user home; `.nexus42` is joined internally).
fn write_boot_config(
    home: &std::path::Path,
    tool_allowlist: &[&str],
    peer_ids: &[&str],
    peer_keys_json: Option<&str>,
) {
    let connect_dir = nexus_home_layout::connect_dir(home);
    std::fs::create_dir_all(&connect_dir).unwrap();
    // Fixed daemon identity so the dialer can preconfigure the pubkey.
    std::fs::write(connect_dir.join("daemon_identity.key"), seed_host()).unwrap();
    let allowlist_json = tool_allowlist
        .iter()
        .map(|s| format!("\"{s}\""))
        .collect::<Vec<_>>()
        .join(",");
    let peers_json = peer_ids
        .iter()
        .map(|s| format!("\"{s}\""))
        .collect::<Vec<_>>()
        .join(",");
    std::fs::write(
        nexus_home_layout::connect_daemon_config_path(home),
        format!(
            r#"{{"host":"127.0.0.1","port":0,"tool_allowlist":[{allowlist_json}],"peer_ids":[{peers_json}]}}"#
        ),
    )
    .unwrap();
    if let Some(keys) = peer_keys_json {
        std::fs::write(nexus_home_layout::connect_peer_keys_path(home), keys).unwrap();
    }
}

fn hex32(bytes: [u8; 32]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// MIGRATED from `authz_hello.rs::boot_derives_hello_from_config_allowlist`
/// and `::boot_rejects_peer_not_in_peer_ids`.
///
/// Preserved: the boot lane derives its hello from the on-disk operator
/// allowlist (an allowlisted, key-carrying dialer is admitted through
/// negotiation), while Layer 0 still gates the handshake — a peer with a
/// preconfigured key but absent from `peer_ids` is rejected with zero session
/// state.
#[tokio::test]
#[serial]
async fn boot_lane_admits_the_config_allowlist_and_rejects_unlisted_peers() {
    let home = tempfile::TempDir::new().unwrap();
    let allowed = peer_id_of(seed_peer(5));
    let intruder = peer_id_of(seed_peer(6));
    write_boot_config(
        home.path(),
        &[TOOL_BOOT],
        &[&allowed],
        Some(&format!(
            r#"{{"peer_keys":{{"{allowed}":"{}","{intruder}":"{}"}}}}"#,
            hex32(pubkey(seed_peer(5))),
            hex32(pubkey(seed_peer(6)))
        )),
    );
    let shutdown = Arc::new(Notify::new());
    let handle = start_peer_tools_lane(home.path(), Arc::clone(&shutdown), None)
        .await
        .expect("lane starts");

    // Layer 0 first: the intruder HAS a key but is not on the allowlist.
    let refused = dial(handle.addr, seed_peer(6), &[TOOL_BOOT]).await;
    assert!(
        refused.is_err(),
        "a peer outside peer_ids is rejected at the handshake"
    );
    assert!(
        handle.sessions.get(&intruder).is_none() && peer_tool_table().get(TOOL_BOOT).is_none(),
        "the rejected dialer leaves zero session state"
    );

    // The allowlisted dialer negotiates the config-derived hello.
    let adapter = dial(handle.addr, seed_peer(5), &[TOOL_BOOT])
        .await
        .expect("allowlisted dialer with a key is admitted");
    adapter.register_tool_handler(TOOL_BOOT, echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get(TOOL_BOOT).is_some(),
            Duration::from_secs(5)
        )
        .await,
        "the admitted tool proves the boot hello advertised the config allowlist"
    );

    adapter.close();
    shutdown.notify_one();
    let _ = handle.task.await;
    peer_tool_table().evict_peer(&allowed, None);
    peer_tool_table().set_config(None);
}

/// MIGRATED from `authz_hello.rs::boot_default_deny_empty_allowlist_zero_admitted`.
///
/// Preserved: with an empty operator tool allowlist the boot hello advertises
/// only the spoke baseline, so an allowlisted peer's tools are never
/// negotiated and NOTHING is admitted — default deny survives the boot path.
#[tokio::test]
#[serial]
async fn boot_lane_with_an_empty_tool_allowlist_admits_nothing() {
    let home = tempfile::TempDir::new().unwrap();
    let peer_id = peer_id_of(seed_peer(7));
    write_boot_config(
        home.path(),
        &[],
        &[&peer_id],
        Some(&format!(
            r#"{{"peer_keys":{{"{peer_id}":"{}"}}}}"#,
            hex32(pubkey(seed_peer(7)))
        )),
    );
    let shutdown = Arc::new(Notify::new());
    let handle = start_peer_tools_lane(home.path(), Arc::clone(&shutdown), None)
        .await
        .expect("lane starts");
    let adapter = dial(handle.addr, seed_peer(7), &[TOOL_BOOT_EMPTY])
        .await
        .expect("the peer itself is allowlisted");
    adapter.register_tool_handler(TOOL_BOOT_EMPTY, echo_handler());
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        peer_tool_table().peer_tool_ids(&peer_id).is_empty()
            && peer_tool_table().get(TOOL_BOOT_EMPTY).is_none(),
        "an empty operator allowlist admits zero peer tools for the authenticated peer"
    );

    adapter.close();
    shutdown.notify_one();
    let _ = handle.task.await;
    peer_tool_table().set_config(None);
}

/// MIGRATED from `authz_hello.rs::reload_rotates_grants_at_session_boundaries_only`.
///
/// Preserved (the SESSION-boundary half): an operator rotation is adopted for
/// NEW sessions while a LIVE session keeps its grant-at-establish catalog —
/// the retired tool's row survives and still dispatches on the open session —
/// and a reconnect re-evaluates against the fresh snapshot, admitting the new
/// grant and dropping the retired row.
///
/// The policy/rank adoption and the boot-scoped `max_sessions` pin from the
/// same source case are Duplicates of `connect/watch.rs` unit tests
/// (`apply_reload_swaps_holder_and_feeds_table_collision_policy`,
/// `reload_adopts_admission_fields_and_pins_boot_fields`).
#[tokio::test]
#[serial]
async fn reload_adopts_grants_at_session_boundaries_only() {
    let home = tempfile::TempDir::new().unwrap();
    let peer_id = peer_id_of(seed_peer(8));
    write_boot_config(
        home.path(),
        &[TOOL_ROTATED_OLD],
        &[&peer_id],
        Some(&format!(
            r#"{{"peer_keys":{{"{peer_id}":"{}"}}}}"#,
            hex32(pubkey(seed_peer(8)))
        )),
    );
    let shutdown = Arc::new(Notify::new());
    let handle = start_peer_tools_lane(home.path(), Arc::clone(&shutdown), None)
        .await
        .expect("lane starts");
    let boot_max_sessions = handle.config.get().config.max_sessions;

    let live = dial(handle.addr, seed_peer(8), &[TOOL_ROTATED_OLD])
        .await
        .expect("dial at the boot generation");
    live.register_tool_handler(TOOL_ROTATED_OLD, echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get(TOOL_ROTATED_OLD).is_some(),
            Duration::from_secs(5)
        )
        .await,
        "the boot generation admits the tool"
    );

    // Operator rotation: the retired tool leaves the tool allowlist, a new one
    // arrives, and a boot-scoped field changes (full-byte digest divergence
    // triggers the reload).
    std::fs::write(
        nexus_home_layout::connect_daemon_config_path(home.path()),
        format!(
            r#"{{"host":"127.0.0.1","port":0,"max_sessions":3,"tool_allowlist":["{TOOL_ROTATED_NEW}"],"peer_ids":["{peer_id}"]}}"#
        ),
    )
    .unwrap();
    assert!(
        wait_until(
            || handle.config.get().config.tool_allowlist == vec![TOOL_ROTATED_NEW.to_owned()],
            Duration::from_secs(10)
        )
        .await,
        "the validated reload is adopted (2s poll budget)"
    );

    // A LIVE session keeps its grant-at-establish catalog.
    assert!(
        peer_tool_table()
            .get(TOOL_ROTATED_OLD)
            .is_some_and(|entry| entry.peer_id == peer_id),
        "the live session's row survives the rotation"
    );
    assert_eq!(
        handle
            .sessions
            .get(&peer_id)
            .expect("live session")
            .admitted_ids,
        vec![TOOL_ROTATED_OLD.to_owned()],
        "the live session keeps the catalog it established with"
    );
    let live_invoke = handle
        .sessions
        .get(&peer_id)
        .expect("live session")
        .responder
        .invoke_tool(TOOL_ROTATED_OLD, json!({}))
        .await;
    assert!(
        matches!(live_invoke, SpokeResult::Ok(_)),
        "the live session still dispatches its established grant, got {live_invoke:?}"
    );
    assert_eq!(
        handle.config.get().config.max_sessions,
        boot_max_sessions,
        "boot-scoped max_sessions stays pinned (GC #7)"
    );

    // A reconnect re-evaluates against the FRESH snapshot: the new grant is
    // admitted and the retired row is gone.
    live.close();
    let reconnected = dial(
        handle.addr,
        seed_peer(8),
        &[TOOL_ROTATED_OLD, TOOL_ROTATED_NEW],
    )
    .await
    .expect("reconnect against the reloaded snapshot");
    reconnected.register_tool_handler(TOOL_ROTATED_NEW, echo_handler());
    assert!(
        wait_until(
            || handle
                .sessions
                .get(&peer_id)
                .is_some_and(|rec| rec.admitted_ids == vec![TOOL_ROTATED_NEW.to_owned()]),
            Duration::from_secs(5)
        )
        .await,
        "the reconnect admits the rotated grant only"
    );
    assert!(
        peer_tool_table().get(TOOL_ROTATED_OLD).is_none(),
        "the retired tool's row is gone after the reconnect"
    );
    assert!(peer_tool_table()
        .get(TOOL_ROTATED_NEW)
        .is_some_and(|entry| entry.peer_id == peer_id));

    reconnected.close();
    shutdown.notify_one();
    let _ = handle.task.await;
    peer_tool_table().evict_peer(&peer_id, None);
    peer_tool_table().set_config(None);
}

// ── Retained core tool-registry integrity ─────────────────────────────────

/// The 28 shipped `nexus.*` host-tool ids (V1.59 P0 roster).
const NEXUS_TOOL_IDS: &[&str] = &[
    "nexus.context.whoami",
    "nexus.workspace.info",
    "nexus.work.get",
    "nexus.work.patch",
    "nexus.orchestration.schedule_status",
    "nexus.context.assemble",
    "nexus.world.snapshot.get",
    "nexus.timeline.recent.get",
    "nexus.kb_snapshot.read",
    "nexus.manuscript.chapter.get",
    "nexus.observability.daemon.health",
    "nexus.kb_snapshot.write",
    "nexus.manuscript.chapter.update",
    "nexus.world.configure",
    "nexus.work.schedule.set",
    "nexus.finding.resolve",
    "nexus.pool.entry.manage",
    "nexus.registry.refresh",
    "nexus.reference.refresh",
    // V1.59 P0: DF-47 manuscript & misc parity batch (9 tools)
    "nexus.manuscript.list",
    "nexus.manuscript.read_range",
    "nexus.manuscript.write",
    "nexus.manuscript.phase.get",
    "nexus.manuscript.phase.set",
    "nexus.workspace.paths",
    "nexus.research.query",
    "nexus.runtime.health",
    "nexus.trace.correlation",
];

/// MIGRATED from `cross_caller_e2e.rs::all_nexus_tool_ids_registered_in_capability_registry`
/// and `::test_profile_sets_are_not_action_capabilities`.
///
/// Preserved: the process-global host-tool registry holds exactly the declared
/// `nexus.*` action roster (plus the two `fs/*` ids), and the `nexus.profile.*`
/// grouping ids are metadata, never action capabilities with handler
/// bindings. This is the registry the peer admission chain derives its
/// reserved set from (`live_reserved_tool_ids`), so the peer surface can never
/// shadow a builtin id.
///
/// Deleted with the retired host: the other `cross_caller_e2e` cases drove
/// `HostToolExecutor` (`POST`-path and Schedule-path wrappers built from the
/// daemon `WorkspaceState`) and asserted caller-path equivalence; those
/// wrappers are retired, and the tool-spine behavior they dispatched into is
/// owned by `capability_compute.rs` (P2-T9).
#[test]
fn host_tool_registry_roster_is_the_declared_nexus_surface() {
    let registry = nexus_core::execution::capabilities::host_tool_registry();
    for tool_id in NEXUS_TOOL_IDS {
        assert!(
            registry.lookup(tool_id).is_some(),
            "Tool '{tool_id}' must be registered in the host-tool registry"
        );
    }
    assert_eq!(
        registry.len(),
        30,
        "the registry holds the 28 nexus.* ids plus the 2 fs/* ids"
    );

    for profile_id in [
        "nexus.profile.minimal",
        "nexus.profile.writer",
        "nexus.profile.publisher",
    ] {
        assert!(
            registry.lookup(profile_id).is_none(),
            "Profile-set id '{profile_id}' is grouping metadata, not an action capability"
        );
        assert!(
            !NEXUS_TOOL_IDS.contains(&profile_id),
            "Profile-set id '{profile_id}' must not appear in the action roster"
        );
    }
}
