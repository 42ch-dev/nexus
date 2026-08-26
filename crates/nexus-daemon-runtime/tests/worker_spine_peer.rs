//! V1.174 P1 T3 (AR-76) — Worker-spine peer + user-cap dispatch proof.
//!
//! **SUPPLEMENTARY (PL-4):** this suite proves the worker `agent_tool_request`
//! journeys for peer ids and user-cap ids ride the SAME single dispatch
//! registry as the MCP lane (AR-68 #4/#6). It does NOT substitute for the
//! MCP consumption journey (AR-75/AR-77) — that remains the consumer face
//! of record.
//!
//! Coverage (AR-76 #2/#3/#4 DoD):
//! - happy path: admitted peer tool dispatched via `dispatch_from_worker`
//!   with `WorkerToolResult { grant: true, output }` unchanged;
//! - structural argument gate (spoke `validate_tool_arguments`, PRE-I/O):
//!   non-object args and missing declared top-level `required` keys ⇒
//!   `invalid_input`, zero adapter I/O observable;
//! - refusal matrix: unknown/evicted ⇒ `not_supported`; peer deny ⇒
//!   `not_supported` + `wire_code` preserved (worker message channel);
//!   transport closed mid-invoke ⇒ fail-fast `internal` disconnect-named;
//!   timeout ⇒ `internal` timeout-named; user-cap `run()` error ⇒ honest
//!   failure code (`service_unavailable` on engine-less boot).

#![cfg(feature = "connect-client")]
// Justification (repo convention, cf. tests/works_api.rs): integration-test
// assertions operate on fixed local fixtures where a panic IS the failure
// signal; `.unwrap()`/`.expect()` keep the tests linear and readable.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use futures_util::future::BoxFuture;
use nexus_daemon_runtime::api::handlers::host_tool_executor::HostToolExecutor;
use nexus_daemon_runtime::connect::{
    daemon_manifest, peer_tool_table, spawn_accept_loop, ws_config, PeerResponderOptions,
    PeerSessionManager, PeerToolsConfig, WsTransport, DEFAULT_MAX_ENVELOPE_BYTES,
};
use nexus_daemon_runtime::test_utils::{self, TestTempRoot};
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_orchestration::{CapabilityRegistry, CapabilityRuntimeDeps};
use serde_json::{json, Value};
use serial_test::serial;
use spoke_connect::core::derive_peer_id_from_ed25519_pubkey;
use spoke_connect::remote::{
    connect_remote_adapter, RemoteAdapter, RemoteAdapterError, RemoteAdapterOptions,
    RemoteIdentity, ToolHandler, Transport,
};
use spoke_operations::{spoke_ok, SpokeResult};
use spoke_schemas::HostCapabilityManifest;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

/// Fixed seeds — the T3 worker suite uses `[0x50+n; 32]` / `tools.t3w.*` so
/// the process-global table never collides with `peer_tool.rs` T3's
/// `[0x40+n; 32]` / `tools.t3.*` fixtures (separate binaries, but keep the
/// convention for greppability).
const fn seed_host() -> [u8; 32] {
    [0xb1; 32]
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

/// Dialer manifest for the structural-gate tool: input schema declares one
/// top-level required key (`topic`).
fn dialer_manifest_required(host_id: &str, tool_id: &str) -> HostCapabilityManifest {
    serde_json::from_value(json!({
        "schema_version": 1,
        "host_id": host_id,
        "roles": ["data-store"],
        "capabilities": ["spoke-baseline", tool_id],
        "namespaces": [tool_id.split('.').nth(1).unwrap_or(tool_id)],
        "extensions": {},
        "tools": [{
            "schema_version": 1,
            "capability_id": tool_id,
            "op": tool_id,
            "description": format!("{tool_id} test tool"),
            "input": { "type": "object", "required": ["topic"] },
            "output": { "type": "object" },
        }],
    }))
    .expect("valid dialer manifest")
}

/// Test harness: one accept loop on an ephemeral loopback port + a daemon
/// router whose capability registry scans `scan_dir` (user caps).
struct WorkerSpineServer {
    addr: std::net::SocketAddr,
    task: JoinHandle<()>,
    shutdown: Arc<Notify>,
    sessions: Arc<PeerSessionManager>,
    _tmp: TestTempRoot,
}

async fn start_server(
    allowlist: Vec<String>,
    peer_keys: HashMap<String, [u8; 32]>,
    tool_ids: &[&str],
    scan_dir: Option<&std::path::Path>,
    invoke_timeout_ms: u64,
) -> WorkerSpineServer {
    let config = Arc::new(PeerToolsConfig {
        host: "127.0.0.1".to_owned(),
        port: 0,
        max_sessions: 8,
        invoke_timeout_ms,
        max_envelope_bytes: DEFAULT_MAX_ENVELOPE_BYTES,
        tool_allowlist: tool_ids.iter().map(|s| (*s).to_owned()).collect(),
        // Layer 0 dialer allowlist: tests pass the peer allowlist via
        // `PeerResponderOptions`.
        peer_ids: Vec::new(),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let sessions = Arc::new(PeerSessionManager::new());
    let manifest = Arc::new(daemon_manifest(
        "daemon-test",
        &tool_ids.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
    ));
    let shutdown = Arc::new(Notify::new());
    let options = PeerResponderOptions {
        identity_seed: seed_host(),
        manifest,
        allowlist,
        peer_keys,
        capability_registry: None,
    };
    let task = spawn_accept_loop(
        listener,
        config,
        Arc::clone(&sessions),
        options,
        Arc::clone(&shutdown),
    );

    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    if let Some(dir) = scan_dir {
        let deps = CapabilityRuntimeDeps {
            pool: None,
            worker_provider: None,
            daemon_tool_dispatch: None,
            cdn_config: None,
        };
        let (registry, outcome) = CapabilityRegistry::with_runtime_deps_and_user_caps(&deps, dir);
        assert!(
            outcome.skipped.is_empty(),
            "no skips expected: {:?}",
            outcome.skipped
        );
        state.set_capability_registry(
            nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(registry)),
        );
    }

    WorkerSpineServer {
        addr,
        task,
        shutdown,
        sessions,
        _tmp: tmp,
    }
}

/// Dial a spoke adapter against the test server.
async fn dial(
    addr: std::net::SocketAddr,
    seed: [u8; 32],
    tool_ids: &[&str],
) -> Result<Arc<RemoteAdapter>, RemoteAdapterError> {
    dial_with_manifest(addr, seed, dialer_manifest("dialer", tool_ids)).await
}

/// Dial with a custom manifest.
async fn dial_with_manifest(
    addr: std::net::SocketAddr,
    seed: [u8; 32],
    manifest: HostCapabilityManifest,
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
        local_manifest: manifest,
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

/// An echo tool handler: answers with the arguments echoed back.
fn echo_handler() -> ToolHandler {
    Arc::new(|args: Value| {
        Box::pin(async move { spoke_ok(json!({ "echo": args })) })
            as BoxFuture<'static, SpokeResult<Value>>
    })
}

/// A handler that never completes — the reverse invoke must time out or
/// fail on session close. Records entry so the test can synchronize on the
/// in-flight window.
fn blocking_handler(started: Arc<AtomicBool>) -> ToolHandler {
    Arc::new(move |_args: Value| {
        let started = Arc::clone(&started);
        Box::pin(async move {
            started.store(true, Ordering::SeqCst);
            std::future::pending::<SpokeResult<Value>>().await
        }) as BoxFuture<'static, SpokeResult<Value>>
    })
}

/// A handler that counts invocations — proves zero adapter I/O when the
/// structural gate rejects before dispatch.
fn counting_handler(calls: Arc<AtomicUsize>) -> ToolHandler {
    Arc::new(move |_args: Value| {
        let calls = Arc::clone(&calls);
        Box::pin(async move {
            calls.fetch_add(1, Ordering::SeqCst);
            spoke_ok(json!({}))
        }) as BoxFuture<'static, SpokeResult<Value>>
    })
}

/// A peer tool handler that rejects with a typed spoke reject carrying the
/// lowercase wire code `op_unsupported` in `details.wire_code` — the exact
/// shape the integrator adapter emits for a non-advertised id (AR-70 #4).
fn rejecting_handler() -> ToolHandler {
    Arc::new(|_args: Value| {
        let mut details = serde_json::Map::new();
        details.insert(
            "wire_code".to_string(),
            Value::String("op_unsupported".to_string()),
        );
        Box::pin(async move {
            spoke_operations::spoke_reject(
                spoke_operations::SpokeRejectCode::CapabilityPortMissing,
                "tool is not supported by this peer",
                Some(details),
            )
        }) as BoxFuture<'static, SpokeResult<Value>>
    })
}

/// Write an admitted `<name>/capability.json` trio at
/// `<root>/capabilities/<name>/` (AR-35 layout) using the embedded
/// basic-combat module (real hash pairing so the scan admits it).
/// The descriptor's `inputSchema` declares `{"type":"object"}` (no
/// required keys) — matching the engine-less-arm fixtures below.
fn write_capability_dir(root: &std::path::Path, name: &str) {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    let wasm = nexus_wasm_host::embedded_module_bytes("basic-combat")
        .expect("embedded basic-combat available");
    let manifest_json = nexus_wasm_host::embedded_module_manifest("basic-combat")
        .expect("embedded manifest available");
    let sha: String = {
        let mut hex = String::with_capacity(64);
        for b in Sha256::digest(wasm) {
            let _ = write!(hex, "{b:02x}");
        }
        hex
    };
    let dir = root.join("capabilities").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let descriptor = format!(
        r#"{{
            "name": "{name}",
            "inputSchema": "{{\"type\":\"object\"}}",
            "outputSchema": "{{\"type\":\"object\"}}",
            "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{sha}" }}
        }}"#
    );
    std::fs::write(dir.join("capability.json"), descriptor).unwrap();
    std::fs::write(dir.join("manifest.json"), manifest_json).unwrap();
    std::fs::write(dir.join("basic-combat.wasm"), wasm).unwrap();
}

/// Write an admitted user capability whose declared `inputSchema` requires
/// one top-level key (`topic`) — the structural-refusal fixtures (W-A).
fn write_capability_dir_required(root: &std::path::Path, name: &str) {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    let wasm = nexus_wasm_host::embedded_module_bytes("basic-combat")
        .expect("embedded basic-combat available");
    let manifest_json = nexus_wasm_host::embedded_module_manifest("basic-combat")
        .expect("embedded manifest available");
    let sha: String = {
        let mut hex = String::with_capacity(64);
        for b in Sha256::digest(wasm) {
            let _ = write!(hex, "{b:02x}");
        }
        hex
    };
    let dir = root.join("capabilities").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let descriptor = format!(
        r#"{{
            "name": "{name}",
            "inputSchema": "{{\"type\":\"object\",\"properties\":{{\"topic\":{{\"type\":\"string\"}}}},\"required\":[\"topic\"]}}",
            "outputSchema": "{{\"type\":\"object\"}}",
            "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{sha}" }}
        }}"#
    );
    std::fs::write(dir.join("capability.json"), descriptor).unwrap();
    std::fs::write(dir.join("manifest.json"), manifest_json).unwrap();
    std::fs::write(dir.join("basic-combat.wasm"), wasm).unwrap();
}

/// Shutdown a test server and clean the process-global table.
async fn teardown(server: WorkerSpineServer, peer_ids: &[String]) {
    for peer_id in peer_ids {
        peer_tool_table().evict_peer(peer_id, None);
    }
    server.shutdown.notify_one();
    let _ = server.task.await;
}

// ── DoD: happy path ───────────────────────────────────────────────────────

#[tokio::test]
#[serial(worker_spine_peer)]
async fn worker_spine_peer_happy_path_echoes_ok() {
    let peer_id = peer_id_of(seed_peer(1));
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(1)))]),
        &["tools.t3w.echo"],
        None,
        2000,
    )
    .await;
    let adapter = dial(server.addr, seed_peer(1), &["tools.t3w.echo"])
        .await
        .expect("dial succeeds");
    adapter.register_tool_handler("tools.t3w.echo", echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get("tools.t3w.echo").is_some(),
            Duration::from_secs(5)
        )
        .await,
        "admitted"
    );

    let (_tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let result = HostToolExecutor::dispatch_from_worker(
        "tools.t3w.echo",
        &json!({ "msg": "hello from worker" }),
        "req-ws-echo",
        &state,
    )
    .await;
    assert_eq!(result.request_id, "req-ws-echo");
    assert!(result.grant, "echo must grant: {result:?}");
    assert_eq!(
        result.output,
        Some(json!({ "echo": { "msg": "hello from worker" } })),
        "spoke result passes through verbatim"
    );
    assert!(result.error.is_none());

    teardown(server, &[peer_id]).await;
}

// ── Structural argument gate (schema, PRE-I/O) ────────────────────────────

#[tokio::test]
#[serial(worker_spine_peer)]
async fn worker_spine_peer_structural_gate_rejects_non_object_pre_io() {
    let peer_id = peer_id_of(seed_peer(2));
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(2)))]),
        &["tools.t3w.required"],
        None,
        2000,
    )
    .await;
    let adapter = dial_with_manifest(
        server.addr,
        seed_peer(2),
        dialer_manifest_required("dialer", "tools.t3w.required"),
    )
    .await
    .expect("dial succeeds");
    let calls = Arc::new(AtomicUsize::new(0));
    adapter.register_tool_handler("tools.t3w.required", counting_handler(Arc::clone(&calls)));
    assert!(
        wait_until(
            || peer_tool_table().get("tools.t3w.required").is_some(),
            Duration::from_secs(5)
        )
        .await,
        "admitted"
    );

    let (_tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    // Non-object arguments — rejected before any adapter I/O.
    let result = HostToolExecutor::dispatch_from_worker(
        "tools.t3w.required",
        &json!([1, 2, 3]),
        "req-ws-struct-arr",
        &state,
    )
    .await;
    let err = result.error.expect("structural reject must set error");
    assert_eq!(err.code, "invalid_input");
    assert!(
        err.message.contains("must be a JSON object"),
        "message: {err:?}"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "adapter handler must never be invoked (zero peer bytes observable)"
    );

    teardown(server, &[peer_id]).await;
}

#[tokio::test]
#[serial(worker_spine_peer)]
async fn worker_spine_peer_structural_gate_rejects_missing_required_pre_io() {
    let peer_id = peer_id_of(seed_peer(3));
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(3)))]),
        &["tools.t3w.anchored"],
        None,
        2000,
    )
    .await;
    let adapter = dial_with_manifest(
        server.addr,
        seed_peer(3),
        dialer_manifest_required("dialer", "tools.t3w.anchored"),
    )
    .await
    .expect("dial succeeds");
    let calls = Arc::new(AtomicUsize::new(0));
    adapter.register_tool_handler("tools.t3w.anchored", counting_handler(Arc::clone(&calls)));
    assert!(
        wait_until(
            || peer_tool_table().get("tools.t3w.anchored").is_some(),
            Duration::from_secs(5)
        )
        .await,
        "admitted"
    );

    let (_tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    // Object but missing the declared top-level required key `topic`.
    let result = HostToolExecutor::dispatch_from_worker(
        "tools.t3w.anchored",
        &json!({ "other": 1 }),
        "req-ws-struct-miss",
        &state,
    )
    .await;
    let err = result.error.expect("structural reject must set error");
    assert_eq!(err.code, "invalid_input");
    assert!(
        err.message.contains("Missing required tool arguments"),
        "message: {err:?}"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "adapter handler must never be invoked (zero peer bytes observable)"
    );

    teardown(server, &[peer_id]).await;
}

// ── Peer deny ⇒ not_supported + wire_code preserved ───────────────────────

#[tokio::test]
#[serial(worker_spine_peer)]
async fn worker_spine_peer_deny_wire_code_survives_verbatim() {
    let peer_id = peer_id_of(seed_peer(4));
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(4)))]),
        &["tools.t3w.reject"],
        None,
        2000,
    )
    .await;
    let adapter = dial(server.addr, seed_peer(4), &["tools.t3w.reject"])
        .await
        .expect("dial succeeds");
    adapter.register_tool_handler("tools.t3w.reject", rejecting_handler());
    assert!(
        wait_until(
            || peer_tool_table().get("tools.t3w.reject").is_some(),
            Duration::from_secs(5)
        )
        .await,
        "admitted"
    );

    let (_tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let result = HostToolExecutor::dispatch_from_worker(
        "tools.t3w.reject",
        &json!({}),
        "req-ws-deny",
        &state,
    )
    .await;
    let err = result.error.expect("deny must set error");
    assert_eq!(err.code, "not_supported", "peer deny stays not_supported");
    assert_eq!(
        err.message, "tool is not supported by this peer (wire_code: op_unsupported)",
        "lowercase wire code preserved verbatim on the worker path: {err:?}"
    );

    teardown(server, &[peer_id]).await;
}

// ── Unknown/evicted ⇒ not_supported ───────────────────────────────────────

#[tokio::test]
#[serial(worker_spine_peer)]
async fn worker_spine_peer_unknown_and_evicted_ids_are_not_supported() {
    let peer_id = peer_id_of(seed_peer(5));
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(5)))]),
        &["tools.t3w.echo"],
        None,
        2000,
    )
    .await;
    let adapter = dial(server.addr, seed_peer(5), &["tools.t3w.echo"])
        .await
        .expect("dial succeeds");
    adapter.register_tool_handler("tools.t3w.echo", echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get("tools.t3w.echo").is_some(),
            Duration::from_secs(5)
        )
        .await,
        "admitted"
    );

    let (_tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    // Never-admitted id — not_supported identically to an unknown builtin.
    let unknown = HostToolExecutor::dispatch_from_worker(
        "tools.t3w.ghost",
        &json!({}),
        "req-ws-unknown",
        &state,
    )
    .await;
    assert_eq!(
        unknown.error.expect("unknown id errors").code,
        "not_supported"
    );

    // Evicted id — same.
    peer_tool_table().evict_peer(&peer_id, None);
    server.sessions.evict(&peer_id, None);
    let evicted = HostToolExecutor::dispatch_from_worker(
        "tools.t3w.echo",
        &json!({}),
        "req-ws-evicted",
        &state,
    )
    .await;
    assert_eq!(
        evicted.error.expect("evicted id errors").code,
        "not_supported"
    );

    teardown(server, &[peer_id]).await;
}

// ── Transport closed mid-invoke ⇒ fail-fast disconnect-named ──────────────

#[tokio::test]
#[serial(worker_spine_peer)]
async fn worker_spine_peer_transport_close_mid_invoke_fails_fast_disconnect_named() {
    let peer_id = peer_id_of(seed_peer(6));
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(6)))]),
        &["tools.t3w.pend"],
        None,
        5000,
    )
    .await;
    let adapter = dial(server.addr, seed_peer(6), &["tools.t3w.pend"])
        .await
        .expect("dial succeeds");
    let started = Arc::new(AtomicBool::new(false));
    adapter.register_tool_handler("tools.t3w.pend", blocking_handler(Arc::clone(&started)));
    assert!(
        wait_until(
            || peer_tool_table().get("tools.t3w.pend").is_some(),
            Duration::from_secs(5)
        )
        .await,
        "admitted"
    );

    let (_tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    // Dispatch while the invoke is in flight; the session tears down under it.
    let dispatch_state = state.clone();
    let dispatch_args = json!({ "topic": "x" });
    let dispatch = tokio::spawn(async move {
        HostToolExecutor::dispatch_from_worker(
            "tools.t3w.pend",
            &dispatch_args,
            "req-ws-mid",
            &dispatch_state,
        )
        .await
    });
    assert!(
        wait_until(|| started.load(Ordering::SeqCst), Duration::from_secs(5)).await,
        "handler observed the invoke (in flight)"
    );

    // Tear the session down mid-invoke (transport close observation path).
    server.sessions.evict(&peer_id, None);
    peer_tool_table().evict_peer(&peer_id, None);

    let result = tokio::time::timeout(Duration::from_secs(5), dispatch)
        .await
        .expect("disconnect-named refusal is fail-fast, bounded")
        .expect("worker dispatch task completes");
    let err = result.error.expect("close mid-invoke must error");
    assert_eq!(
        err.code, "internal",
        "transport-closed mid-invoke is an internal refusal, not a deny: {err:?}"
    );
    assert!(
        err.message.contains("disconnected"),
        "disconnect-named message: {err:?}"
    );

    teardown(server, &[peer_id]).await;
}

// ── Timeout ⇒ internal timeout-named ──────────────────────────────────────

#[tokio::test]
#[serial(worker_spine_peer)]
async fn worker_spine_peer_timeout_is_internal_timeout_named() {
    let peer_id = peer_id_of(seed_peer(7));
    // Short invoke timeout so the pending waiter fires fast.
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(7)))]),
        &["tools.t3w.slow"],
        None,
        300,
    )
    .await;
    let adapter = dial(server.addr, seed_peer(7), &["tools.t3w.slow"])
        .await
        .expect("dial succeeds");
    let started = Arc::new(AtomicBool::new(false));
    adapter.register_tool_handler("tools.t3w.slow", blocking_handler(Arc::clone(&started)));
    assert!(
        wait_until(
            || peer_tool_table().get("tools.t3w.slow").is_some(),
            Duration::from_secs(5)
        )
        .await,
        "admitted"
    );

    let (_tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    // W-B (QC3 W-1): outer bound so a daemon-side timeout regression fails
    // loudly instead of hanging or passing via the adapter's 5 s backstop.
    // The daemon bound is 300 ms; assert the resolved elapsed is well under
    // the 5 s adapter bound so the daemon path — not the backstop — fired.
    let started = Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        HostToolExecutor::dispatch_from_worker(
            "tools.t3w.slow",
            &json!({}),
            "req-ws-timeout",
            &state,
        ),
    )
    .await
    .expect("timeout dispatch is bounded (no hang)");
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(2),
        "daemon 300 ms bound fired, not the 5 s adapter backstop: {elapsed:?}"
    );
    let err = result.error.expect("timeout must error");
    assert_eq!(
        err.code, "internal",
        "timeout is an internal fault: {err:?}"
    );
    assert!(
        err.message.contains("timed out"),
        "timeout-named message: {err:?}"
    );

    teardown(server, &[peer_id]).await;
}

// ── User-cap run() error ⇒ honest failure code ────────────────────────────

#[tokio::test]
#[serial(worker_spine_peer)]
async fn worker_spine_peer_user_cap_run_error_is_honest_failure() {
    let tmp_root = tempfile::TempDir::new().unwrap();
    write_capability_dir(tmp_root.path(), "ws.demo.cap");
    let scan_dir = tmp_root.path().join("capabilities");

    let peer_id = peer_id_of(seed_peer(8));
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(8)))]),
        &["tools.t3w.echo"],
        Some(&scan_dir),
        2000,
    )
    .await;

    // Engine-less boot arm: run() returns WorkerUnavailable → the honest
    // `service_unavailable` failure code (never fabricated success).
    let (_tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let deps = CapabilityRuntimeDeps {
        pool: None,
        worker_provider: None,
        daemon_tool_dispatch: None,
        cdn_config: None,
    };
    let (registry, outcome) = CapabilityRegistry::with_runtime_deps_and_user_caps(&deps, &scan_dir);
    assert!(outcome.skipped.is_empty(), "skips: {:?}", outcome.skipped);
    state.set_capability_registry(
        nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(registry)),
    );

    let result =
        HostToolExecutor::dispatch_from_worker("ws.demo.cap", &json!({}), "req-ws-ucap", &state)
            .await;
    let err = result.error.expect("user-cap run() error must surface");
    assert_eq!(
        err.code, "service_unavailable",
        "engine-less user cap run() maps to the honest failure code: {err:?}"
    );
    assert!(
        err.message.contains("no executor wired"),
        "honest message: {err:?}"
    );

    teardown(server, &[peer_id]).await;
}

// ── User-cap structural gate (AR-76 #2/#4, W-A) ───────────────────────────

#[tokio::test]
#[serial(worker_spine_peer)]
async fn worker_spine_peer_user_cap_structural_gate_rejects_non_object_pre_io() {
    let tmp_root = tempfile::TempDir::new().unwrap();
    write_capability_dir(tmp_root.path(), "ws.demo.cap");
    let scan_dir = tmp_root.path().join("capabilities");

    let peer_id = peer_id_of(seed_peer(9));
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(9)))]),
        &["tools.t3w.echo"],
        Some(&scan_dir),
        2000,
    )
    .await;

    let (_tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let deps = CapabilityRuntimeDeps {
        pool: None,
        worker_provider: None,
        daemon_tool_dispatch: None,
        cdn_config: None,
    };
    let (registry, outcome) = CapabilityRegistry::with_runtime_deps_and_user_caps(&deps, &scan_dir);
    assert!(outcome.skipped.is_empty(), "skips: {:?}", outcome.skipped);
    state.set_capability_registry(
        nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(registry)),
    );

    // Non-object arguments: the structural gate must refuse BEFORE
    // `run()` (which would load + execute the WASM module — adapter I/O).
    let result = HostToolExecutor::dispatch_from_worker(
        "ws.demo.cap",
        &json!([1, 2, 3]),
        "req-ws-ucap-struct-arr",
        &state,
    )
    .await;
    let err = result.error.expect("structural reject must set error");
    assert_eq!(
        err.code, "invalid_input",
        "user-cap structural refusal is invalid_input: {err:?}"
    );
    assert!(
        err.message.contains("must be a JSON object"),
        "message: {err:?}"
    );

    teardown(server, &[peer_id]).await;
}

#[tokio::test]
#[serial(worker_spine_peer)]
async fn worker_spine_peer_user_cap_structural_gate_rejects_missing_required_pre_io() {
    let tmp_root = tempfile::TempDir::new().unwrap();
    write_capability_dir_required(tmp_root.path(), "ws.demo.capreq");
    let scan_dir = tmp_root.path().join("capabilities");

    let peer_id = peer_id_of(seed_peer(10));
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(10)))]),
        &["tools.t3w.echo"],
        Some(&scan_dir),
        2000,
    )
    .await;

    let (_tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let deps = CapabilityRuntimeDeps {
        pool: None,
        worker_provider: None,
        daemon_tool_dispatch: None,
        cdn_config: None,
    };
    let (registry, outcome) = CapabilityRegistry::with_runtime_deps_and_user_caps(&deps, &scan_dir);
    assert!(outcome.skipped.is_empty(), "skips: {:?}", outcome.skipped);
    state.set_capability_registry(
        nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(registry)),
    );

    // Object but missing the declared top-level required key `topic`.
    let result = HostToolExecutor::dispatch_from_worker(
        "ws.demo.capreq",
        &json!({ "other": 1 }),
        "req-ws-ucap-struct-miss",
        &state,
    )
    .await;
    let err = result.error.expect("structural reject must set error");
    assert_eq!(err.code, "invalid_input");
    assert!(
        err.message.contains("Missing required tool arguments"),
        "message: {err:?}"
    );

    teardown(server, &[peer_id]).await;
}

#[tokio::test]
#[serial(worker_spine_peer)]
async fn worker_spine_peer_user_cap_structural_gate_rejects_http_lane_pre_io() {
    // HTTP-lane entry (the handler behind /v1/daemon/agent-host/internal/
    // tool-executions just wraps HostToolExecutor::execute): same structural
    // refusal, same pre-I/O guarantee.
    let tmp_root = tempfile::TempDir::new().unwrap();
    write_capability_dir_required(tmp_root.path(), "ws.demo.capreq");
    let scan_dir = tmp_root.path().join("capabilities");

    let peer_id = peer_id_of(seed_peer(11));
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(11)))]),
        &["tools.t3w.echo"],
        Some(&scan_dir),
        2000,
    )
    .await;

    let (_tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let deps = CapabilityRuntimeDeps {
        pool: None,
        worker_provider: None,
        daemon_tool_dispatch: None,
        cdn_config: None,
    };
    let (registry, outcome) = CapabilityRegistry::with_runtime_deps_and_user_caps(&deps, &scan_dir);
    assert!(outcome.skipped.is_empty(), "skips: {:?}", outcome.skipped);
    state.set_capability_registry(
        nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(registry)),
    );

    let req = nexus_daemon_runtime::api::handlers::host_tool_executor::ToolExecuteRequest {
        tool_name: "ws.demo.capreq".to_string(),
        parameters: json!({ "other": 1 }),
        session_id: None,
        request_id: Some("req-http-ucap-struct".to_string()),
        caller_kind: None,
    };
    let result = HostToolExecutor::execute(&req, &state).await;
    let err = result.expect_err("structural reject must error on the HTTP lane");
    assert_eq!(
        err.error_code().to_string(),
        "invalid_input",
        "HTTP-lane user-cap structural refusal: {err:?}"
    );
    assert!(
        err.to_string().contains("Missing required tool arguments"),
        "message: {err:?}"
    );

    teardown(server, &[peer_id]).await;
}
