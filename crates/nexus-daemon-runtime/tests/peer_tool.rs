//! V1.174 P0 T3 (AR-68) — `PeerToolTable` + spine extension + catalog route.
//!
//! Drives the real accept loop over a real `TcpListener` on `127.0.0.1:0`
//! with real spoke dialers, then exercises the single dispatch spine
//! (worker `HostToolExecutor` + HTTP `POST …/tool-executions` + catalog
//! `GET /v1/daemon/tools`) against admitted peer tools and user caps.
//!
//! `DoD` coverage:
//! - admission matrix (grammar / reserved-ns / non-negotiated /
//!   not-allowlisted / whole-manifest-invalid each refused with its named
//!   refusal; valid manifest admits exact-id set with schemas verbatim);
//! - default deny (empty allowlist ⇒ zero rows, table + catalog);
//! - single-table proof (unknown peer id through worker spine, HTTP
//!   tool-executions, AND catalog lookup all yield `not_supported` /
//!   absence identically to unknown builtin);
//! - user-cap branch (≥1 scanned user cap dispatchable via
//!   `POST …/tool-executions` with `run()` result; `nexus.*`-named and
//!   `tools.*`-grammar user caps refused at catalog admission; builtin
//!   orchestration caps absent from catalog);
//! - catalog ⇄ spine lockstep (`GET /v1/daemon/tools` ids == dispatchable
//!   set, both directions);
//! - eviction (disconnect ⇒ zero rows, table + catalog, same tick);
//! - duplicate-id two-peer collision (later refused, first stays).

#![cfg(feature = "connect-client")]
// Justification (repo convention, cf. tests/works_api.rs): integration-test
// assertions operate on fixed local fixtures where a panic IS the failure
// signal; `.unwrap()`/`.expect()` keep the tests linear and readable.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use axum_test::TestServer;
use ed25519_dalek::SigningKey;
use futures_util::future::BoxFuture;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
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
use spoke_operations::{spoke_ok, spoke_reject, SpokeRejectCode, SpokeResult};
use spoke_schemas::HostCapabilityManifest;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

/// Fixed seeds (daemon + dialers). T3 uses distinct seeds `[0x40+n; 32]`
/// and tool ids `tools.t3.*` so the process-global table never collides
/// with T2's `[0x10+n; 32]` / `tools.t2.*` fixtures.
const fn seed_host() -> [u8; 32] {
    [0xb0; 32]
}
const fn seed_peer(n: u8) -> [u8; 32] {
    [0x40 + n; 32]
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

/// Test harness: one accept loop on an ephemeral loopback port + a daemon
/// router whose capability registry scans `scan_dir` (user caps).
struct PeerTestServer {
    addr: std::net::SocketAddr,
    task: JoinHandle<()>,
    shutdown: Arc<Notify>,
    http: axum_test::TestServer,
    _tmp: TestTempRoot,
}

async fn start_server(
    allowlist: Vec<String>,
    peer_keys: HashMap<String, [u8; 32]>,
    tool_ids: &[&str],
    scan_dir: Option<&std::path::Path>,
) -> PeerTestServer {
    let config = Arc::new(PeerToolsConfig {
        host: "127.0.0.1".to_owned(),
        port: 0,
        max_sessions: 8,
        invoke_timeout_ms: 2000,
        max_envelope_bytes: DEFAULT_MAX_ENVELOPE_BYTES,
        tool_allowlist: tool_ids.iter().map(|s| (*s).to_owned()).collect(),
        // T4 (AR-69): the dialer handshake allowlist (Layer 0). The tests
        // pass the peer allowlist separately via `PeerResponderOptions`.
        peer_ids: Vec::new(),
        embedded_mcp: false,
        // DF-91: the new collision-policy fields default to first_stays +
        // empty rank (AR-68 #3 behavior preserved for every fixture).
        ..PeerToolsConfig::default()
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

    // Daemon router with an optional user-cap scan dir.
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
    let app = api::create_router(state, DaemonApiConfig::keyless());
    let http = TestServer::new(app).expect("test server");

    PeerTestServer {
        addr,
        task,
        shutdown,
        http,
        _tmp: tmp,
    }
}

/// Dial a spoke adapter against the test server.
async fn dial(
    addr: std::net::SocketAddr,
    seed: [u8; 32],
    tool_ids: &[&str],
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
        local_manifest: dialer_manifest("dialer", tool_ids),
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

/// Write an admitted `<name>/capability.json` trio at
/// `<root>/capabilities/<name>/` (AR-35 layout) using the embedded
/// basic-combat module (real hash pairing so the scan admits it).
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

/// Like [`write_capability_dir`] but with a non-object output schema
/// (`{"type":"string"}`) — exercises the AR-70 §3 inclusion rule for user
/// capabilities (`output_schema` omitted from the catalog, never wrapped).
fn write_capability_dir_string_output(root: &std::path::Path, name: &str) {
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
            "outputSchema": "{{\"type\":\"string\"}}",
            "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{sha}" }}
        }}"#
    );
    std::fs::write(dir.join("capability.json"), descriptor).unwrap();
    std::fs::write(dir.join("manifest.json"), manifest_json).unwrap();
    std::fs::write(dir.join("basic-combat.wasm"), wasm).unwrap();
}

/// POST a tool execution through the HTTP spine.
// axum_test's AutoFuture is not Send; this helper is awaited directly by #[tokio::test], never spawned
#[allow(clippy::future_not_send)]
async fn post_tool_execution(
    server: &PeerTestServer,
    tool_name: &str,
    args: Value,
) -> (StatusCode, Value) {
    let resp = server
        .http
        .post("/v1/daemon/agent-host/internal/tool-executions")
        .json(&json!({
            "tool_name": tool_name,
            "parameters": args,
        }))
        .await;
    let status = resp.status_code();
    let body: Value = resp.json();
    (status, body)
}

// ── DoD: admission matrix ─────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn valid_manifest_admits_exact_id_set_with_schemas_verbatim() {
    let peer_id = peer_id_of(seed_peer(1));
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(1)))]),
        &["tools.t3.echo"],
        None,
    )
    .await;
    let adapter = dial(server.addr, seed_peer(1), &["tools.t3.echo"])
        .await
        .expect("dial succeeds");
    adapter.register_tool_handler("tools.t3.echo", echo_handler());

    assert!(
        wait_until(
            || peer_tool_table().get("tools.t3.echo").is_some(),
            Duration::from_secs(5)
        )
        .await,
        "peer tool admitted"
    );
    let entry = peer_tool_table().get("tools.t3.echo").expect("entry");
    assert_eq!(entry.peer_id, peer_id);
    assert_eq!(
        String::from(entry.descriptor.capability_id.clone()),
        "tools.t3.echo"
    );
    assert_eq!(
        entry.descriptor.input,
        json!({"type": "object"}).as_object().cloned().unwrap()
    );
    assert_eq!(
        entry.descriptor.output,
        json!({"type": "object"}).as_object().cloned().unwrap()
    );
    assert_eq!(
        peer_tool_table().peer_tool_ids(&peer_id),
        vec!["tools.t3.echo".to_owned()]
    );

    // Cleanup: evict so the global table stays clean for other tests.
    peer_tool_table().evict_peer(&peer_id, None);
    server.shutdown.notify_one();
    let _ = server.task.await;
}

#[tokio::test]
#[serial]
async fn grammar_reserved_negotiated_allowlist_refusals_are_named() {
    let peer_id = peer_id_of(seed_peer(2));
    // Daemon hello advertises only tools.t3.echo; allowlist only that id.
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(2)))]),
        &["tools.t3.echo"],
        None,
    )
    .await;

    // Not-allowlisted: manifest advertises tools.t3.other (negotiated? no —
    // the daemon hello does not list it either) → refused.
    let adapter = dial(server.addr, seed_peer(2), &["tools.t3.other"])
        .await
        .expect("dial succeeds");
    adapter.register_tool_handler("tools.t3.other", echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get("tools.t3.other").is_none(),
            Duration::from_secs(5)
        )
        .await,
        "non-allowlisted tool refused"
    );
    assert!(
        peer_tool_table().get("tools.t3.echo").is_none(),
        "no accidental admission"
    );

    // Reserved namespace: tools.nexus.* refused even when allowlisted.
    let peer_id2 = peer_id_of(seed_peer(3));
    let server2 = start_server(
        vec![peer_id2.clone()],
        HashMap::from([(peer_id2.clone(), pubkey(seed_peer(3)))]),
        &["tools.nexus.evil"],
        None,
    )
    .await;
    let adapter2 = dial(server2.addr, seed_peer(3), &["tools.nexus.evil"])
        .await
        .expect("dial succeeds");
    adapter2.register_tool_handler("tools.nexus.evil", echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get("tools.nexus.evil").is_none(),
            Duration::from_secs(5)
        )
        .await,
        "reserved-ns tool refused"
    );

    peer_tool_table().evict_peer(&peer_id, None);
    peer_tool_table().evict_peer(&peer_id2, None);
    server.shutdown.notify_one();
    server2.shutdown.notify_one();
    let _ = server.task.await;
    let _ = server2.task.await;
}

// ── DoD: default deny ─────────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn empty_allowlist_yields_zero_rows_table_and_catalog() {
    let peer_id = peer_id_of(seed_peer(4));
    // tool_ids = [] ⇒ config.tool_allowlist = [] ⇒ default deny.
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(4)))]),
        &[],
        None,
    )
    .await;
    let adapter = dial(server.addr, seed_peer(4), &["tools.t3.echo"])
        .await
        .expect("dial succeeds");
    adapter.register_tool_handler("tools.t3.echo", echo_handler());
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(peer_tool_table().is_empty(), "zero rows in table");

    let resp = server.http.get("/v1/daemon/tools").await;
    resp.assert_status(StatusCode::OK);
    let body: Value = resp.json();
    let items = body["items"].as_array().expect("items array");
    assert!(
        items.iter().all(|t| t["origin"] != "peer"),
        "no peer rows in catalog: {body}"
    );

    peer_tool_table().evict_peer(&peer_id, None);
    server.shutdown.notify_one();
    let _ = server.task.await;
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
            spoke_reject(
                SpokeRejectCode::CapabilityPortMissing,
                "tool is not supported by this peer",
                Some(details),
            )
        }) as BoxFuture<'static, SpokeResult<Value>>
    })
}

// ── DoD: single-table proof ────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn peer_deny_wire_code_survives_to_http_error_details_verbatim() {
    let peer_id = peer_id_of(seed_peer(9));
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(9)))]),
        &["tools.t3.reject"],
        None,
    )
    .await;
    let adapter = dial(server.addr, seed_peer(9), &["tools.t3.reject"])
        .await
        .expect("dial succeeds");
    adapter.register_tool_handler("tools.t3.reject", rejecting_handler());
    assert!(
        wait_until(
            || peer_tool_table().get("tools.t3.reject").is_some(),
            Duration::from_secs(5)
        )
        .await,
        "reject tool admitted"
    );

    // AR-70 #4: the spine threads the ORIGINAL lowercase spoke wire code in
    // `details.wire_code` — never uppercased, never re-parsed from text.
    let (status, body) = post_tool_execution(&server, "tools.t3.reject", json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "not_supported");
    assert_eq!(
        body["error"]["details"]["wire_code"], "op_unsupported",
        "lowercase wire code preserved verbatim: {body}"
    );
    assert_eq!(
        body["error"]["message"], "tool is not supported by this peer",
        "message carries no uppercase prefix: {body}"
    );

    peer_tool_table().evict_peer(&peer_id, None);
    server.shutdown.notify_one();
    let _ = server.task.await;
}

#[tokio::test]
#[serial]
async fn unknown_peer_id_is_not_supported_identically_to_unknown_builtin() {
    let peer_id = peer_id_of(seed_peer(5));
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(5)))]),
        &["tools.t3.echo"],
        None,
    )
    .await;
    let adapter = dial(server.addr, seed_peer(5), &["tools.t3.echo"])
        .await
        .expect("dial succeeds");
    adapter.register_tool_handler("tools.t3.echo", echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get("tools.t3.echo").is_some(),
            Duration::from_secs(5)
        )
        .await,
        "admitted"
    );

    // Worker spine: unknown peer id → not_supported (same as unknown builtin).
    let (status, body) = post_tool_execution(&server, "tools.t3.ghost", json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"]["code"], "not_supported",
        "worker spine: {body}"
    );

    // HTTP tool-executions: same.
    let (status2, body2) = post_tool_execution(&server, "tools.t3.ghost", json!({})).await;
    assert_eq!(status2, StatusCode::BAD_REQUEST);
    assert_eq!(body2["error"]["code"], "not_supported");

    // Catalog: absent.
    let resp = server.http.get("/v1/daemon/tools").await;
    let items = resp.json::<Value>()["items"]
        .as_array()
        .expect("items")
        .clone();
    assert!(
        items.iter().all(|t| t["id"] != "tools.t3.ghost"),
        "ghost absent from catalog"
    );

    // Unknown builtin behaves identically.
    let (status3, body3) = post_tool_execution(&server, "nexus.does.not.exist", json!({})).await;
    assert_eq!(status3, StatusCode::BAD_REQUEST);
    assert_eq!(body3["error"]["code"], "not_supported");

    peer_tool_table().evict_peer(&peer_id, None);
    server.shutdown.notify_one();
    let _ = server.task.await;
}

// ── DoD: user-cap branch ───────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn user_cap_dispatchable_and_builtin_caps_absent_from_catalog() {
    let tmp_root = tempfile::TempDir::new().unwrap();
    write_capability_dir(tmp_root.path(), "demo.pull");
    let scan_dir = tmp_root.path().join("capabilities");

    let peer_id = peer_id_of(seed_peer(6));
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(6)))]),
        &["tools.t3.echo"],
        Some(&scan_dir),
    )
    .await;

    // User cap dispatchable via HTTP tool-executions with run() result.
    let (status, body) = post_tool_execution(&server, "demo.pull", json!({})).await;
    // Engine-less boot arm: run() returns WorkerUnavailable → 503.
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "user cap run: {body}"
    );

    // User cap appears in catalog with origin user.
    let resp = server.http.get("/v1/daemon/tools").await;
    resp.assert_status(StatusCode::OK);
    let items = resp.json::<Value>()["items"]
        .as_array()
        .expect("items")
        .clone();
    let user = items
        .iter()
        .find(|t| t["id"] == "demo.pull")
        .unwrap_or_else(|| panic!("demo.pull in catalog: {items:?}"));
    assert_eq!(user["origin"], "user");
    assert_eq!(user["input_schema"], "{\"type\":\"object\"}");

    // Builtin orchestration caps (sync.pull, judge.llm, …) absent.
    for builtin in ["sync.pull", "judge.llm", "narrative.compute"] {
        assert!(
            items.iter().all(|t| t["id"] != builtin),
            "builtin {builtin} absent from catalog"
        );
    }

    peer_tool_table().evict_peer(&peer_id, None);
    server.shutdown.notify_one();
    let _ = server.task.await;
}

#[tokio::test]
#[serial]
async fn user_cap_non_object_output_omitted_from_catalog() {
    let tmp_root = tempfile::TempDir::new().unwrap();
    // A user cap whose output schema is NOT a root object (`{"type":"string"}`).
    write_capability_dir_string_output(tmp_root.path(), "demo.stringout");
    let scan_dir = tmp_root.path().join("capabilities");

    let peer_id = peer_id_of(seed_peer(10));
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(10)))]),
        &["tools.t3.echo"],
        Some(&scan_dir),
    )
    .await;

    // AR-70 §3: the user output schema is carried iff it declares a root
    // `type: "object"` — a string output is omitted, never wrapped.
    let resp = server.http.get("/v1/daemon/tools").await;
    resp.assert_status(StatusCode::OK);
    let items = resp.json::<Value>()["items"]
        .as_array()
        .expect("items")
        .clone();
    let user = items
        .iter()
        .find(|t| t["id"] == "demo.stringout")
        .unwrap_or_else(|| panic!("demo.stringout in catalog: {items:?}"));
    assert_eq!(user["origin"], "user");
    assert_eq!(user["input_schema"], "{\"type\":\"object\"}");
    assert!(
        user.get("output_schema").is_none(),
        "non-object user output omitted from catalog: {user:?}"
    );

    peer_tool_table().evict_peer(&peer_id, None);
    server.shutdown.notify_one();
    let _ = server.task.await;
}

#[tokio::test]
#[serial]
async fn nexus_named_and_tools_grammar_user_caps_refused_at_catalog_admission() {
    let tmp_root = tempfile::TempDir::new().unwrap();
    // A `nexus.*`-named user cap and a `tools.*`-grammar user cap: the scan
    // admits them (no builtin collision), but catalog admission refuses.
    write_capability_dir(tmp_root.path(), "nexus.evil");
    write_capability_dir(tmp_root.path(), "tools.t3.sneaky");
    let scan_dir = tmp_root.path().join("capabilities");

    let peer_id = peer_id_of(seed_peer(7));
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(7)))]),
        &["tools.t3.echo"],
        Some(&scan_dir),
    )
    .await;

    let resp = server.http.get("/v1/daemon/tools").await;
    let items = resp.json::<Value>()["items"]
        .as_array()
        .expect("items")
        .clone();
    assert!(
        items.iter().all(|t| t["id"] != "nexus.evil"),
        "nexus.* user cap refused from catalog"
    );
    assert!(
        items.iter().all(|t| t["id"] != "tools.t3.sneaky"),
        "tools.*-grammar user cap refused from catalog"
    );

    // Spine refuses them too (not dispatchable).
    let (status, _) = post_tool_execution(&server, "nexus.evil", json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status2, _) = post_tool_execution(&server, "tools.t3.sneaky", json!({})).await;
    assert_eq!(status2, StatusCode::BAD_REQUEST);

    peer_tool_table().evict_peer(&peer_id, None);
    server.shutdown.notify_one();
    let _ = server.task.await;
}

// ── DoD: catalog ⇄ spine lockstep ─────────────────────────────────────────

#[tokio::test]
#[serial]
async fn catalog_ids_equal_dispatchable_set_both_directions() {
    let peer_id = peer_id_of(seed_peer(8));
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(8)))]),
        &["tools.t3.echo", "tools.t3.ping"],
        None,
    )
    .await;
    let adapter = dial(
        server.addr,
        seed_peer(8),
        &["tools.t3.echo", "tools.t3.ping"],
    )
    .await
    .expect("dial succeeds");
    adapter.register_tool_handler("tools.t3.echo", echo_handler());
    adapter.register_tool_handler("tools.t3.ping", echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get("tools.t3.echo").is_some()
                && peer_tool_table().get("tools.t3.ping").is_some(),
            Duration::from_secs(5)
        )
        .await,
        "both admitted"
    );

    // Catalog ids == dispatchable set (both directions).
    let resp = server.http.get("/v1/daemon/tools").await;
    let items = resp.json::<Value>()["items"]
        .as_array()
        .expect("items")
        .clone();
    let catalog_ids: HashSet<&str> = items.iter().filter_map(|t| t["id"].as_str()).collect();
    for id in ["tools.t3.echo", "tools.t3.ping"] {
        assert!(catalog_ids.contains(id), "catalog contains {id}: {items:?}");
    }
    // Every catalog peer row is dispatchable (spine resolves it).
    for item in &items {
        if item["origin"] == "peer" {
            let id = item["id"].as_str().expect("id");
            assert!(
                peer_tool_table().get(id).is_some(),
                "catalog peer row {id} resolves in spine"
            );
        }
    }

    peer_tool_table().evict_peer(&peer_id, None);
    server.shutdown.notify_one();
    let _ = server.task.await;
}

// ── DoD: eviction ─────────────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn disconnect_evicts_rows_table_and_catalog_same_tick() {
    let peer_id = peer_id_of(seed_peer(9));
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(9)))]),
        &["tools.t3.echo"],
        None,
    )
    .await;
    let adapter = dial(server.addr, seed_peer(9), &["tools.t3.echo"])
        .await
        .expect("dial succeeds");
    adapter.register_tool_handler("tools.t3.echo", echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get("tools.t3.echo").is_some(),
            Duration::from_secs(5)
        )
        .await,
        "admitted"
    );

    // Drop the transport → close observation → eviction.
    adapter.close();
    assert!(
        wait_until(
            || peer_tool_table().get("tools.t3.echo").is_none(),
            Duration::from_secs(5)
        )
        .await,
        "table row evicted same tick"
    );
    assert!(peer_tool_table().is_empty(), "zero rows after eviction");

    // Catalog shows no peer rows.
    let resp = server.http.get("/v1/daemon/tools").await;
    let items = resp.json::<Value>()["items"]
        .as_array()
        .expect("items")
        .clone();
    assert!(
        items.iter().all(|t| t["origin"] != "peer"),
        "no peer rows in catalog after eviction: {items:?}"
    );

    server.shutdown.notify_one();
    let _ = server.task.await;
}

// ── DoD: duplicate-id two-peer collision ───────────────────────────────────

#[tokio::test]
#[serial]
async fn duplicate_id_two_peer_collision_later_refused_first_stays() {
    let peer_a = peer_id_of(seed_peer(10));
    let peer_b = peer_id_of(seed_peer(11));
    let server = start_server(
        vec![peer_a.clone(), peer_b.clone()],
        HashMap::from([
            (peer_a.clone(), pubkey(seed_peer(10))),
            (peer_b.clone(), pubkey(seed_peer(11))),
        ]),
        &["tools.t3.echo"],
        None,
    )
    .await;
    let adapter_a = dial(server.addr, seed_peer(10), &["tools.t3.echo"])
        .await
        .expect("dial a");
    adapter_a.register_tool_handler("tools.t3.echo", echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get("tools.t3.echo").is_some(),
            Duration::from_secs(5)
        )
        .await,
        "first admitted"
    );

    let adapter_b = dial(server.addr, seed_peer(11), &["tools.t3.echo"])
        .await
        .expect("dial b");
    adapter_b.register_tool_handler("tools.t3.echo", echo_handler());
    tokio::time::sleep(Duration::from_millis(300)).await;

    // First stays bound to peer_a; peer_b's duplicate refused.
    let entry = peer_tool_table().get("tools.t3.echo").expect("first stays");
    assert_eq!(entry.peer_id, peer_a, "first peer keeps the id");
    assert!(
        peer_tool_table().peer_tool_ids(&peer_b).is_empty(),
        "peer_b admitted nothing"
    );

    peer_tool_table().evict_peer(&peer_a, None);
    peer_tool_table().evict_peer(&peer_b, None);
    server.shutdown.notify_one();
    let _ = server.task.await;
}
