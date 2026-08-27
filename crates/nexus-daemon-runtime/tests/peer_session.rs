//! V1.174 P0 T2 (AR-67) — peer-tools accept loop + session manager suite.
//!
//! Drives the real accept loop over a real `TcpListener` on `127.0.0.1:0`
//! and real spoke dialers (`connect_remote_adapter` over a WS client
//! transport). Each test is self-contained (own listener + session
//! manager); no shared global state.

#![cfg(feature = "connect-client")]
// Justification (repo convention, cf. tests/works_api.rs): integration-test
// assertions operate on fixed local fixtures where a panic IS the failure
// signal; `.unwrap()`/`.expect()` keep the tests linear and readable.
#![allow(clippy::unwrap_used)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use futures_util::future::BoxFuture;
use spoke_connect::core::derive_peer_id_from_ed25519_pubkey;
use spoke_connect::remote::{
    connect_remote_adapter, RemoteAdapter, RemoteAdapterError, RemoteAdapterOptions,
    RemoteAdapterState, RemoteIdentity, ToolHandler, Transport,
};
use spoke_operations::{spoke_ok, SpokeResult};
use spoke_schemas::HostCapabilityManifest;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use nexus_daemon_runtime::connect::peer_tool_table;
use nexus_daemon_runtime::connect::{
    spawn_accept_loop, ws_config, PeerConfigHolder, PeerConfigSnapshot, PeerResponderOptions,
    PeerSessionManager, PeerToolsConfig, WsTransport, DEFAULT_MAX_ENVELOPE_BYTES,
};
use serial_test::serial;

/// Fixed seeds (daemon + dialers).
const fn seed_host() -> [u8; 32] {
    [0xa0; 32]
}
const fn seed_peer(n: u8) -> [u8; 32] {
    [0x10 + n; 32]
}

fn pubkey(seed: [u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(&seed).verifying_key().to_bytes()
}

fn peer_id_of(seed: [u8; 32]) -> String {
    derive_peer_id_from_ed25519_pubkey(&pubkey(seed))
}

/// Dialer hello manifest advertising one tool.
fn dialer_manifest(host_id: &str, tool_id: &str) -> HostCapabilityManifest {
    let namespace = tool_id
        .split('.')
        .nth(1)
        .expect("tools.<ns>.<id>")
        .to_owned();
    serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "host_id": host_id,
        "roles": ["data-store"],
        "capabilities": ["spoke-baseline", tool_id],
        "namespaces": [namespace],
        "extensions": {},
        "tools": [{
            "schema_version": 1,
            "capability_id": tool_id,
            "op": tool_id,
            "description": format!("{tool_id} test tool"),
            "input": { "type": "object" },
            "output": { "type": "object" },
        }],
    }))
    .expect("valid dialer manifest")
}

/// Test harness: one accept loop on an ephemeral loopback port.
struct TestServer {
    addr: std::net::SocketAddr,
    sessions: Arc<PeerSessionManager>,
    task: JoinHandle<()>,
}

async fn start_server(
    max_sessions: usize,
    allowlist: Vec<String>,
    peer_keys: HashMap<String, [u8; 32]>,
    tool_ids: &[&str],
) -> TestServer {
    let config = Arc::new(PeerToolsConfig {
        host: "127.0.0.1".to_owned(),
        port: 0,
        max_sessions,
        invoke_timeout_ms: 2000,
        max_envelope_bytes: DEFAULT_MAX_ENVELOPE_BYTES,
        // T3 (AR-68): the operator tool allowlist — tests admit exactly the
        // tool ids they advertise (empty = default deny).
        tool_allowlist: tool_ids.iter().map(|s| (*s).to_owned()).collect(),
        // T4 (AR-69): the dialer handshake allowlist (Layer 0) — DF-92: it
        // lives in the config generation the holder serves.
        peer_ids: allowlist,
        embedded_mcp: false,
        // DF-91: the new collision-policy fields default to first_stays +
        // empty rank (AR-68 #3 behavior preserved for every fixture).
        ..PeerToolsConfig::default()
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let sessions = Arc::new(PeerSessionManager::new());
    let shutdown = Arc::new(Notify::new());
    // DF-92: the hello derives per connection from the live holder — the
    // harness seeds it with the fixed boot generation (allowlist + keys).
    let options = PeerResponderOptions {
        identity_seed: seed_host(),
        host_id: "daemon-test".to_owned(),
        config: PeerConfigHolder::new(PeerConfigSnapshot {
            config: Arc::clone(&config),
            peer_keys: Arc::new(peer_keys),
        }),
        capability_registry: None,
    };
    let task = spawn_accept_loop(listener, config, Arc::clone(&sessions), options, shutdown);
    TestServer {
        addr,
        sessions,
        task,
    }
}

/// Dial a spoke adapter against the test server. Fails fast when the server
/// rejects (handshake rejection closes the transport).
async fn dial(
    addr: std::net::SocketAddr,
    seed: [u8; 32],
    tool_id: &str,
) -> Result<Arc<RemoteAdapter>, RemoteAdapterError> {
    let url = format!("ws://{addr}/connect");
    let stream = TcpStream::connect(addr)
        .await
        .map_err(|e| RemoteAdapterError::Handshake(format!("tcp connect failed: {e}")))?;
    let (ws, _) = tokio_tungstenite::client_async_with_config(
        url,
        stream,
        Some(ws_config(DEFAULT_MAX_ENVELOPE_BYTES)),
    )
    .await
    .map_err(|e| RemoteAdapterError::Handshake(format!("ws upgrade failed: {e}")))?;
    let transport: Arc<dyn Transport> = Arc::new(WsTransport::new(ws));
    let daemon_pubkey = pubkey(seed_host());
    let daemon_peer_id = derive_peer_id_from_ed25519_pubkey(&daemon_pubkey);
    connect_remote_adapter(RemoteAdapterOptions {
        transport,
        local_identity: RemoteIdentity { seed },
        local_manifest: dialer_manifest("dialer", tool_id),
        remote_pubkey: daemon_pubkey,
        allowlist: vec![daemon_peer_id],
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

/// A slow tool handler: parks until a parked flag is set, then answers.
fn parked_handler(park: Arc<Notify>) -> ToolHandler {
    Arc::new(move |_args: serde_json::Value| {
        let park = Arc::clone(&park);
        Box::pin(async move {
            park.notified().await;
            spoke_ok(serde_json::json!({ "parked": true }))
        }) as BoxFuture<'static, SpokeResult<serde_json::Value>>
    })
}

// ── DoD ①: two-connection replace ─────────────────────────────────────────

#[tokio::test]
#[serial]
async fn second_session_same_peer_replaces_first() {
    let peer_a = peer_id_of(seed_peer(0));
    let mut keys = HashMap::new();
    keys.insert(peer_a.clone(), pubkey(seed_peer(0)));
    let server = start_server(
        8,
        vec![peer_a.clone()],
        keys,
        &["tools.t2.old", "tools.t2.new"],
    )
    .await;

    // First dial: same peer id, manifest tool A.
    let adapter_a = dial(server.addr, seed_peer(0), "tools.t2.old")
        .await
        .unwrap();
    assert!(
        wait_until(
            || server.sessions.get(&peer_a).is_some(),
            Duration::from_secs(5)
        )
        .await
    );
    let first = server.sessions.get(&peer_a).expect("first session");
    assert_eq!(first.admitted_ids, vec!["tools.t2.old".to_owned()]);
    assert_eq!(server.sessions.session_count(), 1);

    // Second dial: SAME peer id, different manifest — deterministic
    // last-wins replace.
    let adapter_b = dial(server.addr, seed_peer(0), "tools.t2.new")
        .await
        .unwrap();
    assert!(
        wait_until(
            || {
                server
                    .sessions
                    .get(&peer_a)
                    .is_some_and(|rec| rec.admitted_ids == vec!["tools.t2.new".to_owned()])
            },
            Duration::from_secs(5)
        )
        .await
    );

    // Exactly one live session for the peer; the old session's responder was
    // closed and its table entries evicted; the fresh session's entries were
    // admitted. The process-global PeerToolTable is the single authority for
    // "which peer owns a tool id" (QC-fix S-c — the session-scoped reverse
    // index was removed).
    assert_eq!(server.sessions.session_count(), 1);
    assert!(
        peer_tool_table().get("tools.t2.old").is_none(),
        "old entries must be evicted from the table"
    );
    assert!(
        peer_tool_table().get("tools.t2.new").is_some(),
        "fresh admission must be in the table"
    );
    assert_eq!(adapter_b.state(), RemoteAdapterState::Established);
    assert!(
        wait_until(
            || adapter_a.state() == RemoteAdapterState::Closed,
            Duration::from_secs(5)
        )
        .await,
        "the replaced dialer must observe its session close"
    );

    adapter_a.close();
    adapter_b.close();
    server.task.abort();
    // QC-fix W-B: explicit teardown — the process-global PeerToolTable is
    // shared across test binaries; leave no rows behind.
    peer_tool_table().evict_peer(&peer_a, None);
}

// ── DoD ②: handshake failure ⇒ immediate close, zero session state ────────

#[tokio::test]
async fn non_allowlisted_peer_is_rejected_at_handshake() {
    let peer_a = peer_id_of(seed_peer(0));
    let mut keys = HashMap::new();
    keys.insert(peer_a.clone(), pubkey(seed_peer(0)));
    let server = start_server(8, vec![peer_a.clone()], keys, &[]).await;

    // Dialer B is NOT in the allowlist → the responder rejects the hello and
    // closes the transport → the dialer fails fast with zero session state.
    let started = Instant::now();
    let result = dial(server.addr, seed_peer(1), "tools.t2.other").await;
    assert!(result.is_err(), "non-allowlisted dial must fail fast");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "handshake rejection must be immediate (no timeout park)"
    );
    assert_eq!(server.sessions.session_count(), 0);
    assert!(server.sessions.peer_ids().is_empty());
    server.task.abort();
}

#[tokio::test]
async fn allowlisted_peer_without_key_is_rejected_at_handshake() {
    let peer_a = peer_id_of(seed_peer(0));
    // Peer A is on the allowlist but has NO preconfigured key → the responder
    // rejects at the peer_keys lookup and closes (fail-closed).
    let server = start_server(8, vec![peer_a.clone()], HashMap::new(), &[]).await;

    let started = Instant::now();
    let result = dial(server.addr, seed_peer(0), "tools.t2.a").await;
    assert!(result.is_err(), "missing peer key must fail the handshake");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "handshake rejection must be immediate (no timeout park)"
    );
    assert_eq!(server.sessions.session_count(), 0);
    assert!(server.sessions.peer_ids().is_empty());
    server.task.abort();
}

// ── DoD ③: close observation → eviction + bounded in-flight invoke ────────

#[tokio::test]
#[serial]
async fn transport_drop_evicts_session_and_resolves_in_flight_invoke() {
    let peer_a = peer_id_of(seed_peer(0));
    let mut keys = HashMap::new();
    keys.insert(peer_a.clone(), pubkey(seed_peer(0)));
    let server = start_server(8, vec![peer_a.clone()], keys, &["tools.t2.slow"]).await;

    let adapter = dial(server.addr, seed_peer(0), "tools.t2.slow")
        .await
        .unwrap();
    assert!(
        wait_until(
            || server.sessions.get(&peer_a).is_some(),
            Duration::from_secs(5)
        )
        .await
    );

    // A parked dialer handler keeps the daemon's reverse invoke in flight.
    let park = Arc::new(Notify::new());
    adapter.register_tool_handler("tools.t2.slow", parked_handler(Arc::clone(&park)));

    let responder = server.sessions.get(&peer_a).expect("session").responder;
    let invoke = tokio::spawn({
        let responder = Arc::clone(&responder);
        async move {
            responder
                .invoke_tool("tools.t2.slow", serde_json::json!({}))
                .await
        }
    });

    // Drop the peer's transport: the wrapper observes the close and the
    // monitor evicts; the in-flight invoke resolves bounded (session_closed).
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
            || server.sessions.session_count() == 0,
            Duration::from_secs(5)
        )
        .await,
        "transport drop must evict the session (same tick as observed close)"
    );
    server.task.abort();
    // QC-fix W-B: explicit teardown — the global table row must be gone.
    peer_tool_table().evict_peer(&peer_a, None);
}

// ── DoD ④: accept-loop independence (load) ────────────────────────────────

#[tokio::test]
#[serial]
async fn accept_loop_stays_responsive_under_session_load() {
    // 3 concurrent established sessions + a refused excess dial: the accept
    // loop never awaits session work, so all dials settle within the bound.
    let mut allowlist = Vec::new();
    let mut keys = HashMap::new();
    // Seeds 0..2 are the concurrent sessions; seeds 3 (refused excess) and
    // 4 (rejoin after eviction) are also allowlisted so the accept/limit
    // gates — not the handshake allowlist — decide their fate.
    for i in 0..5 {
        let pid = peer_id_of(seed_peer(i));
        allowlist.push(pid.clone());
        keys.insert(pid, pubkey(seed_peer(i)));
    }
    let server = start_server(3, allowlist, keys, &[]).await;

    let mut adapters = Vec::new();
    for i in 0..3 {
        adapters.push(
            dial(server.addr, seed_peer(i), "tools.t2.other")
                .await
                .unwrap(),
        );
    }
    assert!(
        wait_until(
            || server.sessions.session_count() == 3,
            Duration::from_secs(5)
        )
        .await
    );

    // While all 3 sessions are live, a 4th distinct peer is refused at accept
    // with a logged refusal and the dialer fails fast — the accept loop is
    // not blocked by the established sessions' work.
    let started = Instant::now();
    let refused = dial(server.addr, seed_peer(3), "tools.t2.refused").await;
    assert!(refused.is_err(), "excess session must be refused at accept");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "refusal must be immediate (no park on session work)"
    );
    assert_eq!(server.sessions.session_count(), 3);

    // Evict one live session → a new distinct peer is admitted again.
    adapters[0].close();
    assert!(
        wait_until(
            || server.sessions.session_count() == 2,
            Duration::from_secs(5)
        )
        .await
    );
    let readmitted = dial(server.addr, seed_peer(4), "tools.t2.rejoin")
        .await
        .unwrap();
    assert!(
        wait_until(
            || server.sessions.session_count() == 3,
            Duration::from_secs(5)
        )
        .await
    );
    readmitted.close();
    for a in &adapters {
        a.close();
    }
    server.task.abort();
    // QC-fix W-B: explicit teardown for every peer admitted to the table.
    for i in 0..5 {
        peer_tool_table().evict_peer(&peer_id_of(seed_peer(i)), None);
    }
}

// ── DoD ⑤: session limit — 9th concurrent session refused at accept ───────

#[tokio::test]
#[serial]
async fn ninth_concurrent_session_is_refused_at_accept() {
    let mut allowlist = Vec::new();
    let mut keys = HashMap::new();
    for i in 0..9 {
        let pid = peer_id_of(seed_peer(i));
        allowlist.push(pid.clone());
        keys.insert(pid, pubkey(seed_peer(i)));
    }
    let server = start_server(8, allowlist, keys, &[]).await;

    let mut adapters = Vec::new();
    for i in 0..8 {
        adapters.push(
            dial(server.addr, seed_peer(i), "tools.t2.other")
                .await
                .unwrap(),
        );
    }
    assert!(
        wait_until(
            || server.sessions.session_count() == 8,
            Duration::from_secs(5)
        )
        .await
    );

    // The 9th concurrent session is refused at accept with a logged refusal;
    // the dialer fails fast and the count stays at 8.
    let started = Instant::now();
    let ninth = dial(server.addr, seed_peer(8), "tools.t2.ninth").await;
    assert!(
        ninth.is_err(),
        "9th concurrent session must be refused at accept"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "refusal must be immediate (no park)"
    );
    assert_eq!(server.sessions.session_count(), 8);

    for a in &adapters {
        a.close();
    }
    server.task.abort();
    // QC-fix W-B: explicit teardown for every peer admitted to the table.
    for i in 0..8 {
        peer_tool_table().evict_peer(&peer_id_of(seed_peer(i)), None);
    }
}

// ── QC-fix W-A: in-flight dial flood — never-hello connections cannot
// exceed the cap ───────────────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn dial_flood_of_incomplete_handshakes_cannot_exceed_session_cap() {
    // Raw TCP connections that never send a WS upgrade: each holds an
    // accept slot (in-flight) but never registers. The cap must bound
    // in-flight + registered TOGETHER (QC2 F-001 / QC3 W-1) — the old
    // registered-only gate left this window unbounded.
    let server = start_server(2, Vec::new(), HashMap::new(), &[]).await;

    let mut sockets = Vec::new();
    for _ in 0..6 {
        sockets.push(TcpStream::connect(server.addr).await.expect("connect"));
    }
    assert_eq!(
        sockets.len(),
        6,
        "all six raw dials must be open before the gate is asserted"
    );
    assert!(
        wait_until(
            || server.sessions.connection_count() == 2,
            Duration::from_secs(5)
        )
        .await,
        "accept loop must reserve exactly max_sessions in-flight slots"
    );
    assert_eq!(
        server.sessions.connection_count(),
        2,
        "registered + in-flight stays at the cap while 6 dials are open"
    );
    assert_eq!(
        server.sessions.session_count(),
        0,
        "no dial completes the handshake"
    );

    // Dropping every dial releases its reservation (no counter leak).
    sockets.clear();
    assert!(
        wait_until(
            || server.sessions.connection_count() == 0,
            Duration::from_secs(5)
        )
        .await,
        "dropping the dials must release all in-flight reservations"
    );
    server.task.abort();
}

// ── PR #229 F-3: concurrent accept burst never over-admits ────────────────

#[tokio::test]
#[serial]
async fn concurrent_dial_burst_never_exceeds_session_cap() {
    // Fresh daemon with max_sessions=4 and 7 simultaneous dials (cap+3).
    // PR #229 F-3 regression: `reserve_in_flight` reads the registered
    // count under the `sessions` lock and `register` converts the
    // in-flight reservation into the session inside that SAME lock — one
    // synchronization domain — so a slot is never counted twice nor
    // missed (previously a non-atomic snapshot could over-admit past
    // max). Exactly max_sessions dials must be admitted; the rest are
    // refused at accept.
    let mut allowlist = Vec::new();
    let mut keys = HashMap::new();
    for i in 0..8 {
        let pid = peer_id_of(seed_peer(i));
        allowlist.push(pid.clone());
        keys.insert(pid, pubkey(seed_peer(i)));
    }
    let server = start_server(4, allowlist, keys, &["tools.t2.other"]).await;

    let mut dials = Vec::new();
    for i in 0..8 {
        dials.push(tokio::spawn(dial(
            server.addr,
            seed_peer(i),
            "tools.t2.other",
        )));
    }
    let mut adapters = Vec::new();
    let mut refused = 0usize;
    for dial_task in dials {
        match dial_task.await.expect("dial task") {
            Ok(adapter) => adapters.push(adapter),
            Err(_) => refused += 1,
        }
    }
    assert!(
        wait_until(
            || server.sessions.session_count() == 4,
            Duration::from_secs(5)
        )
        .await,
        "all admitted dials must complete registration"
    );
    assert_eq!(adapters.len(), 4, "exactly max_sessions dials admitted");
    assert_eq!(refused, 4, "the remaining dials refused at accept");
    assert_eq!(
        server.sessions.session_count(),
        4,
        "registered sessions must stay at the cap"
    );
    assert_eq!(
        server.sessions.connection_count(),
        4,
        "no in-flight residue after settlement (slot counted exactly once)"
    );

    let admitted: Vec<String> = server.sessions.peer_ids();
    for a in &adapters {
        a.close();
    }
    server.task.abort();
    // QC-fix W-B: explicit teardown for every peer admitted to the table.
    for pid in admitted {
        peer_tool_table().evict_peer(&pid, None);
    }
}
