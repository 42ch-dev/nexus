//! V1.174 P0 T4 (AR-69) — outbound authz + hello derivation.
//!
//! Drives the real accept loop over a real `TcpListener` on `127.0.0.1:0`
//! with real spoke dialers, plus the boot path (`start_peer_tools_lane`)
//! with on-disk `daemon.json` / `peer_keys.json` config.
//!
//! `DoD` coverage:
//! - derivation pin: hello tool capabilities == allowlist-derived exact ids
//!   (set equality, machine-checked);
//! - negotiation journey: integrator-side negotiated set = both-hello
//!   intersection; non-advertised id ⇒ integrator adapter denies
//!   (`op_unsupported`, `wire_code` preserved) — against a spoke dialer;
//! - boot wiring: `start_peer_tools_lane` derives the hello from the
//!   config allowlist (admission proves negotiation), Layer 0 `peer_ids` +
//!   `peer_keys.json` gate the handshake, default-deny (empty allowlist ⇒
//!   zero admitted);
//! - config-load negatives (umbrella / malformed / reserved-ns ⇒ named
//!   `InvalidAllowlist` error) live in `connect::config` unit tests.

#![cfg(feature = "connect-client")]
// Justification (repo convention, cf. tests/works_api.rs): integration-test
// assertions operate on fixed local fixtures where a panic IS the failure
// signal; `.unwrap()`/`.expect()` keep the tests linear and readable.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use futures_util::future::BoxFuture;
use nexus_daemon_runtime::connect::{
    daemon_manifest, peer_tool_table, spawn_accept_loop, start_peer_tools_lane, ws_config,
    PeerConfigHolder, PeerConfigSnapshot, PeerResponderOptions, PeerSessionManager,
    PeerToolsConfig, WsTransport, DEFAULT_MAX_ENVELOPE_BYTES,
};
use serde_json::{json, Value};
use serial_test::serial;
use spoke_connect::core::derive_peer_id_from_ed25519_pubkey;
use spoke_connect::remote::{
    connect_remote_adapter, RemoteAdapter, RemoteAdapterError, RemoteAdapterOptions,
    RemoteIdentity, ToolHandler, Transport,
};
use spoke_operations::{spoke_ok, SpokeRejectCode, SpokeResult};
use spoke_schemas::HostCapabilityManifest;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

/// Fixed seeds. T4 uses `[0x50+n; 32]` dialers and `[0xc0; 32]` daemon so
/// the process-global table never collides with T2's `[0x10+n; 32]` /
/// `tools.t2.*` or T3's `[0x40+n; 32]` / `tools.t3.*` fixtures.
const fn seed_host() -> [u8; 32] {
    [0xc0; 32]
}
const fn seed_peer(n: u8) -> [u8; 32] {
    [0x50 + n; 32]
}

fn pubkey(seed: [u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(&seed).verifying_key().to_bytes()
}

fn peer_id_of(seed: [u8; 32]) -> String {
    derive_peer_id_from_ed25519_pubkey(&pubkey(seed))
}

/// 64-hex-char encoding of a 32-byte Ed25519 public key (`peer_keys.json`
/// format).
fn hex32(bytes: [u8; 32]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(64);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Dialer hello manifest advertising the given tools.
fn dialer_manifest(host_id: &str, tool_ids: &[&str]) -> HostCapabilityManifest {
    let mut capabilities: Vec<String> = vec!["spoke-baseline".to_owned()];
    capabilities.extend(tool_ids.iter().map(|s| (*s).to_owned()));
    let tool_objs: Vec<Value> = tool_ids
        .iter()
        .map(|id| {
            json!({
                "schema_version": 1,
                "capability_id": id,
                "op": id,
                "description": format!("{id} test tool"),
                "input": { "type": "object" },
                "output": { "type": "object" },
            })
        })
        .collect();
    let namespaces: Vec<String> = tool_ids
        .iter()
        .filter_map(|id| id.split('.').nth(1))
        .map(ToOwned::to_owned)
        .collect();
    serde_json::from_value(json!({
        "schema_version": 1,
        "host_id": host_id,
        "roles": ["data-store"],
        "capabilities": capabilities,
        "namespaces": namespaces,
        "extensions": {},
        "tools": tool_objs,
    }))
    .expect("valid dialer manifest")
}

/// Test harness: one accept loop on an ephemeral loopback port.
struct PeerTestServer {
    addr: std::net::SocketAddr,
    sessions: Arc<PeerSessionManager>,
    task: JoinHandle<()>,
    shutdown: Arc<Notify>,
}

async fn start_server(
    allowlist: Vec<String>,
    peer_keys: HashMap<String, [u8; 32]>,
    tool_ids: &[&str],
) -> PeerTestServer {
    let config = Arc::new(PeerToolsConfig {
        host: "127.0.0.1".to_owned(),
        port: 0,
        max_sessions: 8,
        invoke_timeout_ms: 2000,
        max_envelope_bytes: DEFAULT_MAX_ENVELOPE_BYTES,
        tool_allowlist: tool_ids.iter().map(|s| (*s).to_owned()).collect(),
        peer_ids: allowlist.clone(),
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
    let task = spawn_accept_loop(
        listener,
        config,
        Arc::clone(&sessions),
        options,
        Arc::clone(&shutdown),
    );
    PeerTestServer {
        addr,
        sessions,
        task,
        shutdown,
    }
}

/// Dial a spoke adapter against a server whose daemon key is `daemon_pubkey`.
async fn dial_with_daemon_key(
    addr: std::net::SocketAddr,
    seed: [u8; 32],
    tool_ids: &[&str],
    daemon_pubkey: [u8; 32],
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
    let daemon_peer_id = derive_peer_id_from_ed25519_pubkey(&daemon_pubkey);
    connect_remote_adapter(RemoteAdapterOptions {
        transport,
        local_identity: RemoteIdentity { seed },
        local_manifest: dialer_manifest("dialer", tool_ids),
        remote_pubkey: daemon_pubkey,
        allowlist: vec![daemon_peer_id],
        invoke_timeout_ms: Some(5000),
        capability_token: None,
    })
    .await
}

/// Dial against the in-test harness daemon (fixed `seed_host()`).
async fn dial(
    addr: std::net::SocketAddr,
    seed: [u8; 32],
    tool_ids: &[&str],
) -> Result<Arc<RemoteAdapter>, RemoteAdapterError> {
    dial_with_daemon_key(addr, seed, tool_ids, pubkey(seed_host())).await
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

/// An echo tool handler: answers with the arguments echoed back.
fn echo_handler() -> ToolHandler {
    Arc::new(|args: Value| {
        Box::pin(async move { spoke_ok(json!({ "echo": args })) })
            as BoxFuture<'static, SpokeResult<Value>>
    })
}

// ── DoD: derivation pin (hello tool capabilities == allowlist exact set) ──

#[test]
fn hello_tool_capabilities_equal_allowlist_exact_set() {
    let allowlist = vec![
        "tools.t4.echo".to_owned(),
        "tools.t4.ping".to_owned(),
        "tools.acme.alpha".to_owned(),
    ];
    let manifest = daemon_manifest("device-1", &allowlist);
    let tool_caps: HashSet<&str> = manifest
        .capabilities
        .iter()
        .map(String::as_str)
        .filter(|c| *c != "spoke-baseline")
        .collect();
    let expected: HashSet<&str> = allowlist.iter().map(String::as_str).collect();
    assert_eq!(
        tool_caps, expected,
        "hello tool capabilities == allowlist exact ids (set equality, both directions)"
    );
    // Baseline is always present alongside the derived ids.
    assert!(manifest.capabilities.contains(&"spoke-baseline".to_owned()));
    // Namespaces derived from the ids, deduplicated + order-stable.
    let namespaces: Vec<&str> = manifest.namespaces.iter().map(|n| n.as_str()).collect();
    assert_eq!(namespaces, vec!["acme", "t4"]);
}

#[test]
fn hello_namespaces_deduped_when_allowlist_shares_namespace() {
    let allowlist = vec!["tools.t4.echo".to_owned(), "tools.t4.ping".to_owned()];
    let manifest = daemon_manifest("device-1", &allowlist);
    let namespaces: Vec<&str> = manifest.namespaces.iter().map(|n| n.as_str()).collect();
    assert_eq!(namespaces, vec!["t4"], "single namespace, no duplicates");
}

#[test]
fn hello_empty_allowlist_advertises_baseline_only() {
    let manifest = daemon_manifest("device-1", &[]);
    assert_eq!(manifest.capabilities, vec!["spoke-baseline".to_owned()]);
    assert!(manifest.namespaces.is_empty());
    assert!(manifest.tools.is_empty());
}

// ── DoD: negotiation journey (intersection + non-advertised deny) ─────────

#[tokio::test]
#[serial]
async fn negotiation_journey_intersection_and_non_advertised_denied() {
    let peer_id = peer_id_of(seed_peer(1));
    // Daemon hello advertises BOTH ids (allowlist); the dialer advertises
    // only echo. Negotiated = intersection = {spoke-baseline, echo}.
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(1)))]),
        &["tools.t4.echo", "tools.t4.ghost"],
    )
    .await;
    let adapter = dial(server.addr, seed_peer(1), &["tools.t4.echo"])
        .await
        .expect("dial succeeds");
    adapter.register_tool_handler("tools.t4.echo", echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get("tools.t4.echo").is_some(),
            Duration::from_secs(5)
        )
        .await,
        "echo admitted (negotiated)"
    );
    assert!(
        peer_tool_table().get("tools.t4.ghost").is_none(),
        "ghost never admitted (absent from the dialer manifest)"
    );

    // Integrator-side negotiated set = both-hello intersection, observed
    // through the daemon's reverse-invoke face (the session responder).
    let record = server.sessions.get(&peer_id).expect("session record");

    // (a) A negotiated id dispatches to the dialer's registered handler.
    match record
        .responder
        .invoke_tool("tools.t4.echo", json!({"n": 1}))
        .await
    {
        SpokeResult::Ok(value) => assert_eq!(value, json!({"echo": {"n": 1}})),
        SpokeResult::Reject(reject) => panic!("negotiated id must dispatch: {reject:?}"),
    }

    // (b) A non-advertised id (daemon hello only, absent from the dialer's
    // hello) ⇒ the integrator adapter denies with `op_unsupported`, wire
    // code preserved (D7 row: CAPABILITY_PORT_MISSING + details.wire_code).
    match record
        .responder
        .invoke_tool("tools.t4.ghost", json!({}))
        .await
    {
        SpokeResult::Reject(reject) => {
            assert_eq!(
                reject.code,
                SpokeRejectCode::CapabilityPortMissing,
                "deny maps to CAPABILITY_PORT_MISSING: {reject:?}"
            );
            let wire = reject
                .details
                .as_ref()
                .and_then(|d| d.get("wire_code"))
                .and_then(Value::as_str);
            assert_eq!(
                wire,
                Some("op_unsupported"),
                "wire_code preserved: {reject:?}"
            );
        }
        SpokeResult::Ok(value) => panic!("non-advertised id must be denied, got {value:?}"),
    }

    peer_tool_table().evict_peer(&peer_id, None);
    server.shutdown.notify_one();
    let _ = server.task.await;
}

// ── DoD: boot wiring (config → hello → admission) ─────────────────────────

/// Write a `daemon.json` + `peer_keys.json` + fixed daemon identity under
/// `home` (the RAW user home; `.nexus42` is joined internally).
fn write_boot_config(
    home: &std::path::Path,
    tool_allowlist: &[&str],
    peer_ids: &[&str],
    peer_keys_json: Option<&str>,
) {
    use std::fs;
    let connect_dir = nexus_home_layout::connect_dir(home);
    fs::create_dir_all(&connect_dir).unwrap();
    // Fixed daemon identity so the dialer can preconfigure the pubkey.
    fs::write(connect_dir.join("daemon_identity.key"), [0xc0u8; 32]).unwrap();
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
    fs::write(
        nexus_home_layout::connect_daemon_config_path(home),
        format!(
            r#"{{"host":"127.0.0.1","port":0,"tool_allowlist":[{allowlist_json}],"peer_ids":[{peers_json}]}}"#
        ),
    )
    .unwrap();
    if let Some(keys) = peer_keys_json {
        fs::write(nexus_home_layout::connect_peer_keys_path(home), keys).unwrap();
    }
}

#[tokio::test]
#[serial]
async fn boot_derives_hello_from_config_allowlist() {
    let tmp = tempfile::TempDir::new().unwrap();
    let peer_id = peer_id_of(seed_peer(1));
    write_boot_config(
        tmp.path(),
        &["tools.t4.echo"],
        &[&peer_id],
        Some(&format!(
            r#"{{"peer_keys":{{"{peer_id}":"{}"}}}}"#,
            hex32(pubkey(seed_peer(1)))
        )),
    );
    let shutdown = Arc::new(Notify::new());
    let handle = start_peer_tools_lane(tmp.path(), Arc::clone(&shutdown), None)
        .await
        .expect("lane starts");
    let adapter = dial_with_daemon_key(
        handle.addr,
        seed_peer(1),
        &["tools.t4.echo"],
        pubkey(seed_host()),
    )
    .await
    .expect("dial succeeds (peer allowlisted + key preconfigured)");
    adapter.register_tool_handler("tools.t4.echo", echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get("tools.t4.echo").is_some(),
            Duration::from_secs(5)
        )
        .await,
        "tool admitted ⇒ the boot hello advertised it (derived from the config allowlist)"
    );
    peer_tool_table().evict_peer(&peer_id, None);
    shutdown.notify_one();
    let _ = handle.task.await;
}

#[tokio::test]
#[serial]
async fn boot_default_deny_empty_allowlist_zero_admitted() {
    let tmp = tempfile::TempDir::new().unwrap();
    let peer_id = peer_id_of(seed_peer(2));
    // No tool_allowlist in config ⇒ the boot hello advertises only the
    // baseline ⇒ the dialer's tools are not negotiated ⇒ zero admitted
    // (default deny), even though the peer itself is allowlisted.
    write_boot_config(
        tmp.path(),
        &[],
        &[&peer_id],
        Some(&format!(
            r#"{{"peer_keys":{{"{peer_id}":"{}"}}}}"#,
            hex32(pubkey(seed_peer(2)))
        )),
    );
    let shutdown = Arc::new(Notify::new());
    let handle = start_peer_tools_lane(tmp.path(), Arc::clone(&shutdown), None)
        .await
        .expect("lane starts");
    let adapter = dial_with_daemon_key(
        handle.addr,
        seed_peer(2),
        &["tools.t4.echo"],
        pubkey(seed_host()),
    )
    .await
    .expect("dial succeeds (peer allowlisted)");
    adapter.register_tool_handler("tools.t4.echo", echo_handler());
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        peer_tool_table().is_empty(),
        "zero admitted with an empty operator allowlist"
    );
    shutdown.notify_one();
    let _ = handle.task.await;
}

#[tokio::test]
#[serial]
async fn boot_rejects_peer_not_in_peer_ids() {
    let tmp = tempfile::TempDir::new().unwrap();
    let allowed = peer_id_of(seed_peer(3));
    let intruder = peer_id_of(seed_peer(4));
    // The intruder HAS a preconfigured key but is NOT on the handshake
    // allowlist (peer_ids) ⇒ the responder rejects at the allowlist-first
    // gate and closes immediately (Layer 0, fail-closed).
    write_boot_config(
        tmp.path(),
        &["tools.t4.echo"],
        &[&allowed],
        Some(&format!(
            r#"{{"peer_keys":{{"{allowed}":"{}","{intruder}":"{}"}}}}"#,
            hex32(pubkey(seed_peer(3))),
            hex32(pubkey(seed_peer(4)))
        )),
    );
    let shutdown = Arc::new(Notify::new());
    let handle = start_peer_tools_lane(tmp.path(), Arc::clone(&shutdown), None)
        .await
        .expect("lane starts");
    let result = dial_with_daemon_key(
        handle.addr,
        seed_peer(4),
        &["tools.t4.echo"],
        pubkey(seed_host()),
    )
    .await;
    assert!(
        result.is_err(),
        "intruder dial must fail at the handshake (peer not in peer_ids)"
    );
    assert!(
        peer_tool_table().is_empty(),
        "no session state for the rejected dialer"
    );
    shutdown.notify_one();
    let _ = handle.task.await;
}
// ── V1.179 P1 T2 (DF-92): session-consistent adoption points ───────────────

/// Rewrite `daemon.json` mid-lane (the reload trigger — full-byte digest
/// divergence) and `peer_keys.json` in the same operator edit.
fn rewrite_lane_config(home: &std::path::Path, daemon_json: &str, peer_keys_json: &str) {
    use std::fs;
    fs::write(
        nexus_home_layout::connect_daemon_config_path(home),
        daemon_json,
    )
    .unwrap();
    fs::write(
        nexus_home_layout::connect_peer_keys_path(home),
        peer_keys_json,
    )
    .unwrap();
}

/// The headline DF-92 verify: a config rotation (A's tool retired, B added,
/// policy → `priority_order`, a boot-scoped `max_sessions` flip) is adopted
/// ONLY at session boundaries. A's LIVE session keeps grant-at-establish
/// (old catalog, old tool still dispatchable) even though the tool left the
/// allowlist; B's first hello after the reload registers cleanly against
/// the FRESH snapshot; A's reconnect admits the new grant (and preempts B's
/// row under the reloaded policy + rank). The boot-scoped flip is pinned.
#[tokio::test]
#[serial]
async fn reload_rotates_grants_at_session_boundaries_only() {
    let tmp = tempfile::TempDir::new().unwrap();
    let a = peer_id_of(seed_peer(1));
    let b = peer_id_of(seed_peer(2));
    write_boot_config(
        tmp.path(),
        &["tools.t4.echo"],
        &[&a],
        Some(&format!(
            r#"{{"peer_keys":{{"{a}":"{}"}}}}"#,
            hex32(pubkey(seed_peer(1)))
        )),
    );
    let shutdown = Arc::new(Notify::new());
    let handle = start_peer_tools_lane(tmp.path(), Arc::clone(&shutdown), None)
        .await
        .expect("lane starts");
    let boot_max_sessions = handle.config.get().config.max_sessions;

    // A establishes the LIVE session from the boot generation.
    let adapter_a = dial_with_daemon_key(
        handle.addr,
        seed_peer(1),
        &["tools.t4.echo"],
        pubkey(seed_host()),
    )
    .await
    .expect("dial A succeeds at boot generation");
    adapter_a.register_tool_handler("tools.t4.echo", echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get("tools.t4.echo").is_some(),
            Duration::from_secs(5)
        )
        .await,
        "A admitted at boot"
    );

    // Operator rotation: A's tool retired, B added (peer_ids + key),
    // policy → priority_order with A ranked above B, and a boot-scoped
    // max_sessions flip (must NOT hot-apply).
    rewrite_lane_config(
        tmp.path(),
        &format!(
            r#"{{"host":"127.0.0.1","port":0,"max_sessions":3,"tool_allowlist":["tools.t4.echo2"],"peer_ids":["{a}","{b}"],"collision_policy":"priority_order","peer_priority":["{a}","{b}"]}}"#
        ),
        &format!(
            r#"{{"peer_keys":{{"{a}":"{}","{b}":"{}"}}}}"#,
            hex32(pubkey(seed_peer(1))),
            hex32(pubkey(seed_peer(2)))
        ),
    );
    assert!(
        wait_until(
            || handle.config.get().config.tool_allowlist == vec!["tools.t4.echo2".to_owned()],
            Duration::from_secs(10)
        )
        .await,
        "the validated reload adopted the rotated allowlist (2s poll budget)"
    );
    // Boot-scoped field: pinned to boot, never hot-applied (GC #7).
    assert_eq!(
        handle.config.get().config.max_sessions,
        boot_max_sessions,
        "boot-scoped max_sessions must stay pinned to boot (restart required)"
    );

    // A's LIVE session keeps grant-at-establish: the old catalog row and
    // the session's admitted set survive even though the tool LEFT the
    // allowlist, and a live reverse-invoke on the OLD grant still works.
    assert!(
        peer_tool_table()
            .get("tools.t4.echo")
            .is_some_and(|e| e.peer_id == a),
        "live A row must survive the rotation (grant-at-establish)"
    );
    assert_eq!(
        handle.sessions.get(&a).expect("A session").admitted_ids,
        vec!["tools.t4.echo".to_owned()],
        "A live session keeps the old catalog"
    );
    let live_invoke = handle
        .sessions
        .get(&a)
        .expect("A session")
        .responder
        .invoke_tool("tools.t4.echo", serde_json::json!({}))
        .await;
    assert!(
        matches!(live_invoke, SpokeResult::Ok(_)),
        "A live session must still dispatch the OLD grant, got {live_invoke:?}"
    );

    // B's FIRST hello post-reload registers cleanly: fresh peer_ids, fresh
    // keys, fresh allowlist-derived hello (negotiation) — all from the
    // reloaded snapshot, no restart.
    let adapter_b = dial_with_daemon_key(
        handle.addr,
        seed_peer(2),
        &["tools.t4.echo2"],
        pubkey(seed_host()),
    )
    .await
    .expect("B dials post-reload (adopted peer_ids + keys)");
    adapter_b.register_tool_handler("tools.t4.echo2", echo_handler());
    assert!(
        wait_until(
            || peer_tool_table()
                .get("tools.t4.echo2")
                .is_some_and(|e| e.peer_id == b),
            Duration::from_secs(5)
        )
        .await,
        "B registered post-reload"
    );

    // A RECONNECTS advertising both tools: the handshake validates against
    // the FRESH snapshot; the new grant = echo2 only (echo is retired —
    // not negotiated, not allowlisted). The reloaded policy + rank apply:
    // A (ranked above B) PREEMPTS B's echo2 row.
    let adapter_a2 = dial_with_daemon_key(
        handle.addr,
        seed_peer(1),
        &["tools.t4.echo", "tools.t4.echo2"],
        pubkey(seed_host()),
    )
    .await
    .expect("A reconnect dial admitted against the fresh snapshot");
    adapter_a2.register_tool_handler("tools.t4.echo", echo_handler());
    adapter_a2.register_tool_handler("tools.t4.echo2", echo_handler());
    assert!(
        wait_until(
            || handle
                .sessions
                .get(&a)
                .is_some_and(|rec| rec.admitted_ids == vec!["tools.t4.echo2".to_owned()]),
            Duration::from_secs(5)
        )
        .await,
        "A reconnect admits the new grant (echo2)"
    );
    assert!(
        peer_tool_table().get("tools.t4.echo").is_none(),
        "retired tool row is gone after A's reconnect (evict-then-admit)"
    );
    assert!(
        peer_tool_table()
            .get("tools.t4.echo2")
            .is_some_and(|e| e.peer_id == a),
        "A's reconnect preempted B's row (priority_order + rank from the reloaded snapshot)"
    );

    peer_tool_table().evict_peer(&a, None);
    peer_tool_table().evict_peer(&b, None);
    peer_tool_table().set_config(None);
    shutdown.notify_one();
    let _ = handle.task.await;
}

/// DF-92 failure mode: an INVALID rotation (unknown field — hard load
/// error) that would have added B keeps last-good: A's live grant is
/// untouched, B is NOT adopted (`peer_ids` stay boot — B's dial is rejected
/// at the Layer 0 gate), and the daemon stays up.
#[tokio::test]
#[serial]
async fn invalid_reload_keeps_last_good_and_does_not_apply_new_peer() {
    let tmp = tempfile::TempDir::new().unwrap();
    let a = peer_id_of(seed_peer(1));
    let b = peer_id_of(seed_peer(2));
    write_boot_config(
        tmp.path(),
        &["tools.t4.echo"],
        &[&a],
        Some(&format!(
            r#"{{"peer_keys":{{"{a}":"{}"}}}}"#,
            hex32(pubkey(seed_peer(1)))
        )),
    );
    let shutdown = Arc::new(Notify::new());
    let handle = start_peer_tools_lane(tmp.path(), Arc::clone(&shutdown), None)
        .await
        .expect("lane starts");
    let adapter_a = dial_with_daemon_key(
        handle.addr,
        seed_peer(1),
        &["tools.t4.echo"],
        pubkey(seed_host()),
    )
    .await
    .expect("dial A succeeds at boot generation");
    adapter_a.register_tool_handler("tools.t4.echo", echo_handler());
    assert!(
        wait_until(|| handle.sessions.get(&a).is_some(), Duration::from_secs(5)).await,
        "A established"
    );

    // INVALID edit: unknown field (deny_unknown_fields fails the load) AND
    // b smuggled into peer_ids + keys — neither may be adopted.
    rewrite_lane_config(
        tmp.path(),
        &format!(
            r#"{{"bogus_field":1,"tool_allowlist":["tools.t4.echo"],"peer_ids":["{a}","{b}"]}}"#
        ),
        &format!(
            r#"{{"peer_keys":{{"{a}":"{}","{b}":"{}"}}}}"#,
            hex32(pubkey(seed_peer(1))),
            hex32(pubkey(seed_peer(2)))
        ),
    );
    // ≥1 watcher tick (2s interval): the apply fails, last-good stands.
    tokio::time::sleep(Duration::from_millis(3500)).await;

    let snapshot = handle.config.get();
    assert_eq!(
        snapshot.config.peer_ids,
        vec![a.clone()],
        "invalid edit must not adopt B (last-good peer_ids)"
    );
    assert_eq!(
        snapshot.config.tool_allowlist,
        vec!["tools.t4.echo".to_owned()],
        "invalid edit must not change the live allowlist"
    );
    assert!(
        snapshot.peer_keys.contains_key(&a) && !snapshot.peer_keys.contains_key(&b),
        "invalid edit must not adopt B's key (the reload failed as a whole)"
    );
    assert!(
        peer_tool_table()
            .get("tools.t4.echo")
            .is_some_and(|e| e.peer_id == a)
            && handle.sessions.get(&a).is_some(),
        "A's live grant untouched by the failed reload"
    );
    let result = dial_with_daemon_key(
        handle.addr,
        seed_peer(2),
        &["tools.t4.echo"],
        pubkey(seed_host()),
    )
    .await;
    assert!(
        result.is_err(),
        "B must be rejected at the handshake — last-good peer_ids gate the dial (rewired read)"
    );

    peer_tool_table().evict_peer(&a, None);
    peer_tool_table().set_config(None);
    shutdown.notify_one();
    let _ = handle.task.await;
}
