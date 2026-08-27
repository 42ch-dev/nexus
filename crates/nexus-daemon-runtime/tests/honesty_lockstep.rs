//! V1.174 P0 T6 (AR-74) — honesty lockstep pin module (consolidated).
//!
//! Every bullet below is MACHINE-CHECKED (set-equality / named-refusal /
//! exact-count assertions — never hand-inspected sets) and cites its
//! covering tests. Tests that already exist in `peer_tool.rs` / `authz_hello.rs`
//! / `host_tool_executor_tests.rs` / `config.rs` are NOT duplicated here — the
//! coverage map below is the citation; this file only adds the gaps.
//!
//! Coverage map (lockstep per spec §8):
//! - admitted ⇄ derivation (admission chain == hello derivation):
//!   - `peer_tool::valid_manifest_admits_exact_id_set_with_schemas_verbatim`
//!   - `authz_hello::boot_derives_hello_from_config_allowlist`
//! - catalog ⇄ spine (single dispatch spine; every catalog row dispatchable):
//!   - `peer_tool::catalog_ids_equal_dispatchable_set_both_directions`
//!   - [`catalog_spine_listing_set_equality_with_user_and_peer_origins`] (new)
//! - tools/list ⇄ catalog (MCP bridge is a live read of `GET /v1/daemon/tools`):
//!   - `apps/nexus42/tests/mcp_serve_e2e.rs::initialize_handshake_and_tools_list_mirror_catalog`
//! - listing ⇄ table (`/orchestration/capabilities` peer rows == PeerToolTable):
//!   - [`catalog_spine_listing_set_equality_with_user_and_peer_origins`] (new)
//! - hello ⇄ allowlist (daemon hello `capabilities[]` derives ONLY from the
//!   operator allowlist):
//!   - `authz_hello::hello_tool_capabilities_equal_allowlist_exact_set`
//! - default deny (empty allowlist ⇒ zero rows):
//!   - `peer_tool::empty_allowlist_yields_zero_rows_table_and_catalog`
//!   - `authz_hello::boot_default_deny_empty_allowlist_zero_admitted`
//! - reserved / grammar / umbrella + user-cap ns negatives:
//!   - `peer_tool::grammar_reserved_negotiated_allowlist_refusals_are_named`
//!   - `peer_tool::nexus_named_and_tools_grammar_user_caps_refused_at_catalog_admission`
//!   - `config.rs` unit tests (13 — allowlist negatives / umbrella / grammar)
//! - duplicate-peer (first stays):
//!   - `peer_tool::duplicate_id_two_peer_collision_later_refused_first_stays`
//! - eviction (disconnect ⇒ table + catalog zero rows, same tick):
//!   - `peer_tool::disconnect_evicts_rows_table_and_catalog_same_tick`
//! - reconnect (same-peer reconnect deterministic last-wins):
//!   - [`reconnect_same_peer_evicts_then_admits_new_allowlist_set`] (new)
//! - static `TOOL_ALLOWLIST` pin untouched by peer/user traffic:
//!   - `host_tool_executor_tests::tool_allowlist_matches_registry_ids`
//!   - [`static_allowlist_ids_never_shadowed_by_peer_or_user`] (new)
//! - MCP catalog refusal mapping (catalog-layer only, registration untouched):
//!   - [`peer_row_non_object_input_registered_but_refused_from_mcp_catalog`] (new)
//!
//! All tests use `#[serial]` — the process-global `PeerToolTable` is shared
//! with `peer_tool.rs` (distinct seeds `[0x60+n; 32]` / `tools.t6.*` ids so
//! the fixtures never collide). `peer_session.rs` likewise uses `#[serial]`
//! and explicit `evict_peer` teardown for every test that admits table rows
//! (QC-fix W-B — the earlier claim was incomplete without it).

#![cfg(feature = "connect-client")]
// Justification (repo convention, cf. tests/works_api.rs): integration-test
// assertions operate on fixed local fixtures where a panic IS the failure
// signal; `.unwrap()`/`.expect()` keep the tests linear and readable.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use axum_test::TestServer;
use ed25519_dalek::SigningKey;
use futures_util::future::BoxFuture;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::capability_registry::host_tool_registry;
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

/// T6 fixture seeds (`[0x60+n; 32]`) — disjoint from T2/T3/T4 seeds.
const fn seed_host() -> [u8; 32] {
    [0xc0; 32]
}
const fn seed_peer(n: u8) -> [u8; 32] {
    [0x60 + n; 32]
}

fn pubkey(seed: [u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(&seed).verifying_key().to_bytes()
}

fn peer_id_of(seed: [u8; 32]) -> String {
    derive_peer_id_from_ed25519_pubkey(&pubkey(seed))
}

/// Dialer hello manifest advertising the given tools (root-object inputs).
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

/// Manifest advertising one tool whose `input` is NOT a root object
/// (exercises the AR-70 §3 catalog projection gate).
fn dialer_manifest_with_input(input: &Value, tool_id: &str) -> HostCapabilityManifest {
    serde_json::from_value(json!({
        "schema_version": 1,
        "host_id": "dialer",
        "roles": ["data-store"],
        "capabilities": ["spoke-baseline", tool_id],
        "namespaces": [tool_id.split('.').nth(1).unwrap_or_default()],
        "extensions": {},
        "tools": [{
            "schema_version": 1,
            "capability_id": tool_id,
            "op": tool_id,
            "description": format!("{tool_id} test tool"),
            "input": input,
            "output": { "type": "object" },
        }],
    }))
    .expect("valid dialer manifest")
}

/// Write an admitted `<name>/capability.json` trio (AR-35 layout) using the
/// embedded basic-combat module (real hash pairing so the scan admits it).
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

/// Test harness: accept loop + daemon router (+ optional user-cap scan dir).
/// The peer lane holds no capability holder (`None`), so only the static
/// builtin host-tool ids are reserved — host ids cannot collide with the
/// `tools.<ns>.<id>` test grammar (W-A; default posture like `peer_tool.rs`).
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

/// Dial with a custom manifest (for the non-object input fixture).
async fn dial_manifest(
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

/// An echo tool handler.
fn echo_handler() -> ToolHandler {
    Arc::new(|args: Value| {
        Box::pin(async move { spoke_ok(json!({ "echo": args })) })
            as BoxFuture<'static, SpokeResult<Value>>
    })
}

/// POST a tool execution through the HTTP spine.
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

/// `GET /v1/daemon/tools` → sorted catalog ids.
#[allow(clippy::future_not_send)]
async fn catalog_ids(server: &PeerTestServer) -> Vec<String> {
    let resp = server.http.get("/v1/daemon/tools").await;
    assert_eq!(resp.status_code(), StatusCode::OK);
    let body: Value = resp.json();
    let mut ids: Vec<String> = body["items"]
        .as_array()
        .expect("items array")
        .iter()
        .map(|item| item["id"].as_str().expect("id string").to_owned())
        .collect();
    ids.sort();
    ids
}

/// `GET /v1/daemon/tools` → `{ id → (input_schema, output_schema) }` for
/// builtin-origin rows (AR-80 #1 schema-equality pin).
#[allow(clippy::future_not_send)]
async fn catalog_builtin_schemas(
    server: &PeerTestServer,
) -> std::collections::HashMap<String, (String, Option<String>)> {
    let resp = server.http.get("/v1/daemon/tools").await;
    assert_eq!(resp.status_code(), StatusCode::OK);
    let body: Value = resp.json();
    body["items"]
        .as_array()
        .expect("items array")
        .iter()
        .filter(|item| item["origin"].as_str() == Some("builtin"))
        .map(|item| {
            let id = item["id"].as_str().expect("id string").to_owned();
            let input = item["input_schema"]
                .as_str()
                .expect("input string")
                .to_owned();
            let output = item["output_schema"].as_str().map(ToOwned::to_owned);
            (id, (input, output))
        })
        .collect()
}

/// `GET /v1/daemon/orchestration/capabilities` → sorted peer-origin names.
#[allow(clippy::future_not_send)]
async fn listing_peer_names(server: &PeerTestServer) -> Vec<String> {
    let resp = server
        .http
        .get("/v1/daemon/orchestration/capabilities")
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK);
    let body: Value = resp.json();
    let mut names: Vec<String> = body["items"]
        .as_array()
        .expect("items array")
        .iter()
        .filter(|item| item["origin"].as_str() == Some("peer"))
        .map(|item| item["name"].as_str().expect("name").to_owned())
        .collect();
    names.sort();
    names
}

/// AR-80 #1 (schema-equality, registry ⇄ catalog route, both directions):
/// every builtin row's `CatalogDescriptor` input/output schema text is
/// emitted VERBATIM by `GET /v1/daemon/tools` — the route never rewrites,
/// re-serializes, or substitutes a placeholder for an authored schema, and
/// every emitted builtin row traces back to a registry row with the same
/// schema text. The MCP `tools/list` leg of the lockstep family is pinned
/// in `apps/nexus42/tests/mcp_serve_e2e.rs` (same registry-derived
/// fixture, per-row equality).
#[tokio::test]
#[serial]
async fn builtin_catalog_schema_equality_registry_to_route() {
    let server = start_server(Vec::new(), HashMap::new(), &[], None).await;

    let route = catalog_builtin_schemas(&server).await;
    let registry = host_tool_registry();

    // Registry → route: every builtin row's authored schema text appears
    // verbatim on the route (input always; output when pinned).
    for id in registry.ids() {
        let row = registry.lookup(id).expect("row must exist");
        let (route_input, route_output) = route
            .get(id)
            .unwrap_or_else(|| panic!("builtin row '{id}' must be emitted by the catalog route"));
        let emitted_input = row
            .catalog
            .input_schema
            .unwrap_or(nexus_daemon_runtime::capability_registry::NAMED_PLACEHOLDER_INPUT);
        assert_eq!(
            route_input, emitted_input,
            "route input schema for '{id}' must equal the registry descriptor verbatim"
        );
        assert_eq!(
            route_output.as_deref(),
            row.catalog.output_schema,
            "route output schema for '{id}' must equal the registry descriptor verbatim"
        );
    }

    // Route → registry: every builtin-origin route row names a registered
    // row (no invented ids) and carries the same schema text (both
    // directions complete).
    assert_eq!(
        route.len(),
        registry.ids().count(),
        "catalog builtin rows == registry rows (both directions)"
    );
    for (id, (route_input, route_output)) in &route {
        let row = registry
            .lookup(id)
            .unwrap_or_else(|| panic!("route row '{id}' must name a registered builtin row"));
        let emitted_input = row
            .catalog
            .input_schema
            .unwrap_or(nexus_daemon_runtime::capability_registry::NAMED_PLACEHOLDER_INPUT);
        assert_eq!(
            route_input, &emitted_input,
            "registry input schema for '{id}' must equal the route emission verbatim"
        );
        assert_eq!(
            route_output.as_deref(),
            row.catalog.output_schema,
            "registry output schema for '{id}' must equal the route emission verbatim"
        );
    }

    server.shutdown.notify_one();
    let _ = server.task.await;
}

/// Every admitted row (peer table + user registry + static builtin) is
/// listed on the MCP catalog EXACTLY once, and every catalog row is present
/// in exactly one of the three origin registries — both directions. The
/// orchestration capabilities listing carries the same peer rows
/// (listing ⇄ table). A live spine call proves the catalog row dispatches.
#[tokio::test]
#[serial]
async fn catalog_spine_listing_set_equality_with_user_and_peer_origins() {
    let peer_id = peer_id_of(seed_peer(1));
    let tmp_root = tempfile::tempdir().expect("scan dir");
    write_capability_dir(tmp_root.path(), "t6.wcap");
    let scan_dir = tmp_root.path().join("capabilities");
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(1)))]),
        &["tools.t6.echo"],
        Some(&scan_dir),
    )
    .await;
    let adapter = dial(server.addr, seed_peer(1), &["tools.t6.echo"])
        .await
        .expect("dial succeeds");
    adapter.register_tool_handler("tools.t6.echo", echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get("tools.t6.echo").is_some(),
            Duration::from_secs(5)
        )
        .await,
        "peer tool admitted"
    );

    // Expected catalog = static builtin ids ∪ user caps ∪ peer table.
    let mut expected: Vec<String> = host_tool_registry().ids().map(str::to_owned).collect();
    expected.push("t6.wcap".to_owned());
    expected.push("tools.t6.echo".to_owned());
    expected.sort();

    // catalog ⊇ table ∪ user ∪ builtin, and catalog ⊆ that union (both dirs).
    let catalog = catalog_ids(&server).await;
    assert_eq!(
        catalog, expected,
        "catalog == admitted set (both directions)"
    );

    // listing ⇄ table: peer-origin rows of the orchestration listing ==
    // PeerToolTable ids.
    let mut table_ids = peer_tool_table().ids();
    table_ids.sort();
    let listing_peers = listing_peer_names(&server).await;
    assert_eq!(
        listing_peers, table_ids,
        "orchestration listing peer rows == PeerToolTable ids"
    );

    // catalog row ⇄ dispatchable: the peer row really executes.
    let (status, body) = post_tool_execution(&server, "tools.t6.echo", json!({ "k": "v" })).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "spine dispatch of catalog row succeeds"
    );
    assert_eq!(body["success"], json!(true));
    assert_eq!(body["result"]["echo"]["k"], json!("v"));

    // Teardown: evict + stop the accept loop.
    peer_tool_table().evict_peer(&peer_id, None);
    server.shutdown.notify_one();
    let _ = server.task.await;
}

// ── static TOOL_ALLOWLIST pin untouched by peer/user traffic ───────────────

#[tokio::test]
#[serial]
async fn static_allowlist_ids_never_shadowed_by_peer_or_user() {
    let peer_id = peer_id_of(seed_peer(2));
    let tmp_root = tempfile::tempdir().expect("scan dir");
    // A user-capability named with the peer grammar (`tools.*`) must be
    // refused at catalog admission (AR-68 #6 named refusal) — never emitted.
    write_capability_dir(tmp_root.path(), "tools.t6.steal");
    let scan_dir = tmp_root.path().join("capabilities");
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(2)))]),
        &["tools.t6.steal"],
        Some(&scan_dir),
    )
    .await;

    // A peer advertising `tools.nexus.*` (reserved umbrella) must be refused
    // at admission — it cannot land in the table, let alone shadow static ids.
    let adapter = dial_manifest(
        server.addr,
        seed_peer(2),
        dialer_manifest_with_input(&json!({"type": "object"}), "tools.nexus.evil"),
    )
    .await
    .expect("dial succeeds");
    adapter.register_tool_handler("tools.nexus.evil", echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get("tools.nexus.evil").is_none(),
            Duration::from_secs(5)
        )
        .await,
        "tools.nexus.* peer tool refused at admission"
    );

    // The static allowlist rows are untouched: catalog = builtin ids only
    // (peer row refused, user row `tools.t6.steal` refused at admission) —
    // machine-checked set equality, no hand inspection.
    let mut expected: Vec<String> = host_tool_registry().ids().map(str::to_owned).collect();
    expected.sort();
    let catalog = catalog_ids(&server).await;
    assert_eq!(
        catalog, expected,
        "static allowlist ids untouched by peer/user traffic"
    );
    // The builtin ids are still dispatchable (host registry lookup path).
    assert!(
        host_tool_registry()
            .ids()
            .any(|id| id == "nexus.workspace.info"),
        "static pin includes nexus.workspace.info"
    );

    peer_tool_table().evict_peer(&peer_id, None);
    server.shutdown.notify_one();
    let _ = server.task.await;
}

#[tokio::test]
#[serial]
async fn peer_row_non_object_input_registered_but_refused_from_mcp_catalog() {
    let peer_id = peer_id_of(seed_peer(3));
    let server = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(3)))]),
        &["tools.t6.scalar"],
        None,
    )
    .await;

    // Dial with a NON-object input schema (`{"type":"string"}`).
    let adapter = dial_manifest(
        server.addr,
        seed_peer(3),
        dialer_manifest_with_input(&json!({"type": "string"}), "tools.t6.scalar"),
    )
    .await
    .expect("dial succeeds");
    adapter.register_tool_handler("tools.t6.scalar", echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get("tools.t6.scalar").is_some(),
            Duration::from_secs(5)
        )
        .await,
        "peer row admitted (registration lane untouched by the MCP gate)"
    );

    // MCP catalog refuses the row (input not root-object) — the row is
    // absent from `GET /v1/daemon/tools`.
    let catalog = catalog_ids(&server).await;
    assert!(
        !catalog.contains(&"tools.t6.scalar".to_owned()),
        "non-object-input peer row refused from MCP catalog"
    );

    // But it stays dispatchable through the spine (AR-70 §3 separation).
    let (status, body) = post_tool_execution(&server, "tools.t6.scalar", json!({})).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "registration lane untouched, dispatchable"
    );
    assert_eq!(body["success"], json!(true));

    peer_tool_table().evict_peer(&peer_id, None);
    server.shutdown.notify_one();
    let _ = server.task.await;
}

// ── same-peer reconnect: deterministic last-wins across allowlist change ──

#[tokio::test]
#[serial]
async fn reconnect_same_peer_evicts_then_admits_new_allowlist_set() {
    let peer_id = peer_id_of(seed_peer(4));
    let server_a = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(4)))]),
        &["tools.t6.old"],
        None,
    )
    .await;
    let adapter_a = dial(server_a.addr, seed_peer(4), &["tools.t6.old"])
        .await
        .expect("dial A");
    adapter_a.register_tool_handler("tools.t6.old", echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get("tools.t6.old").is_some(),
            Duration::from_secs(5)
        )
        .await,
        "first admission landed"
    );

    // Same peer identity reconnects to a daemon whose allowlist changed
    // (restart-scoped config edit): the prior rows are evicted, only the new
    // allowlisted set is admitted — deterministic last-wins (AR-68 #3).
    let server_b = start_server(
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(4)))]),
        &["tools.t6.new"],
        None,
    )
    .await;
    let adapter_b = dial(server_b.addr, seed_peer(4), &["tools.t6.new"])
        .await
        .expect("dial B");
    adapter_b.register_tool_handler("tools.t6.new", echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get("tools.t6.new").is_some(),
            Duration::from_secs(5)
        )
        .await,
        "reconnect admitted new allowlisted set"
    );
    assert!(
        wait_until(
            || peer_tool_table().get("tools.t6.old").is_none(),
            Duration::from_secs(5)
        )
        .await,
        "prior rows evicted on same-peer reconnect"
    );

    let mut ids = peer_tool_table().ids();
    ids.sort();
    assert_eq!(
        ids,
        vec!["tools.t6.new".to_owned()],
        "last-wins set equality"
    );

    // Catalog follows the table (both directions).
    let mut expected: Vec<String> = host_tool_registry().ids().map(str::to_owned).collect();
    expected.push("tools.t6.new".to_owned());
    expected.sort();
    let catalog = catalog_ids(&server_b).await;
    assert_eq!(catalog, expected, "catalog == table after reconnect");

    // Teardown on the latest server (evict + stop both accept loops).
    peer_tool_table().evict_peer(&peer_id, None);
    server_a.shutdown.notify_one();
    let _ = server_a.task.await;
    server_b.shutdown.notify_one();
    let _ = server_b.task.await;
}
