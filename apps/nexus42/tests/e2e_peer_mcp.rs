//! V1.174 P1 T4 (AR-77) — three-process E2E harness + eviction journey.
//!
//! Three processes at full fidelity:
//! 1. **Integrator** — this test process, speaking spoke `RemoteAdapter`
//!    over a REAL WS socket pair to the daemon's accept loop (no
//!    loopback-pair shims: the WS leg is a real `TcpStream` +
//!    `tokio-tungstenite` upgrade on `127.0.0.1:0`).
//! 2. **nexus daemon** — in-process test boot: accept loop + real axum
//!    HTTP listener on `127.0.0.1:0` (the spawned MCP child reaches
//!    `GET /v1/daemon/tools` + `POST …/tool-executions` over real
//!    loopback HTTP).
//! 3. **MCP client** — the real spawned `nexus42 mcp serve` child
//!    (`CARGO_BIN_EXE_nexus42`) driven by an in-process rmcp client
//!    (P0 form) and by a scripted ACP agent via `newSession.mcp_servers`
//!    (AC-closing form).
//!
//! Journey A (happy): dial → hello → admission → `tools/list` contains
//! the peer tool with advertised schemas verbatim + builtin `nexus.*`
//! rows + ≥1 user capability (one catalog) → `tools/call` with
//! schema-valid args → `structured_content` matches the advertised
//! output schema. Supplementary: scripted `agent_tool_request` for the
//! same id through the worker spine.
//!
//! Journey B (honest refusals): (i) never-admitted id ⇒
//! `METHOD_NOT_FOUND`; (ii) default-deny config variant (allowlist
//! absent) ⇒ empty peer catalog; (iii) integrator disconnect ⇒ evicted id
//! absent from `tools/list` and refused, bounded; (iv) daemon stopped ⇒
//! bounded `INTERNAL_ERROR`, no hang.
//!
//! Determinism: fixed Ed25519 seeds, `127.0.0.1:0`, deadline-polling
//! readiness waits (admission/eviction handshakes polled every 20 ms with
//! a deadline — S-e/QC3 S-1: bounded polling, not a true event), per-suite
//! serialization via `#[serial]`.
//!
//! Placement: behind `connect-client` (`required-features`); default
//! feature CI never compiles it.

#![cfg(feature = "connect-client")]
// Justification (repo convention, cf. tests/works_api.rs): integration-test
// assertions operate on fixed local fixtures where a panic IS the failure
// signal; `.unwrap()`/`.expect()` keep the tests linear and readable.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use agent_client_protocol::schema::{
    AgentCapabilities, InitializeRequest, InitializeResponse, McpServer, NewSessionRequest,
    NewSessionResponse, ProtocolVersion, SessionId,
};
use agent_client_protocol::{Agent, Channel, Client};
use ed25519_dalek::SigningKey;
use futures_util::future::BoxFuture;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::api::create_router;
use nexus_daemon_runtime::api::handlers::host_tool_executor::HostToolExecutor;
use nexus_daemon_runtime::connect::{
    daemon_manifest, peer_tool_table, spawn_accept_loop, ws_config, PeerResponderOptions,
    PeerSessionManager, PeerToolsConfig, WsTransport, DEFAULT_MAX_ENVELOPE_BYTES,
};
use nexus_daemon_runtime::test_utils::{self, TestTempRoot};
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_orchestration::{CapabilityRegistry, CapabilityRuntimeDeps};
use rmcp::model::{CallToolRequestParams, ClientInfo, ErrorCode};
use rmcp::serve_client;
use rmcp::service::RunningService;
use rmcp::ServiceError;
use serde_json::{json, Value};
use serial_test::serial;
use sha2::{Digest, Sha256};
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

/// Fixed seeds — `0xc0+` host / `0xd0+` peers keep this binary's
/// process-global table disjoint from the other `connect-client` test
/// binaries (`0x40+` `peer_tool.rs`, `0x50+` `worker_spine_peer.rs`).
const fn seed_host() -> [u8; 32] {
    [0xc0; 32]
}
const fn seed_peer(n: u8) -> [u8; 32] {
    [0xd0 + n; 32]
}

fn pubkey(seed: [u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(&seed).verifying_key().to_bytes()
}

fn peer_id_of(seed: [u8; 32]) -> String {
    derive_peer_id_from_ed25519_pubkey(&pubkey(seed))
}

/// The harness advertises one deterministic tool.
const TOOL_ID: &str = "tools.demo-harness.echo";
/// The user capability the daemon scan admits.
const USER_CAP: &str = "t4.wcap";

/// Advertised schemas, verbatim.
fn echo_input_schema() -> Value {
    json!({ "type": "object", "properties": { "msg": { "type": "string" } } })
}
fn echo_output_schema() -> Value {
    json!({ "type": "object", "properties": { "echo": { "type": "object" } } })
}

// ── Daemon harness ────────────────────────────────────────────────────────

/// Test daemon: accept loop over a real WS listener + real axum HTTP
/// listener on `127.0.0.1:0`, with a scanned user-capability registry.
struct E2eDaemon {
    ws_addr: std::net::SocketAddr,
    http_url: String,
    shutdown: Arc<Notify>,
    accept_task: JoinHandle<()>,
    http_task: JoinHandle<()>,
    sessions: Arc<PeerSessionManager>,
    _scan: tempfile::TempDir,
    _tmp: TestTempRoot,
}

impl E2eDaemon {
    /// Start a daemon. `operator_allowlist` is the config
    /// `tool_allowlist` (empty = default-deny); `peer_allowlist` /
    /// `peer_keys` gate the dialer handshake (Layer 0).
    async fn start(
        operator_allowlist: Vec<String>,
        peer_allowlist: Vec<String>,
        peer_keys: HashMap<String, [u8; 32]>,
        with_user_cap: bool,
    ) -> Self {
        // ── Accept loop on a REAL ephemeral loopback socket ──
        let config = Arc::new(PeerToolsConfig {
            host: "127.0.0.1".to_owned(),
            port: 0,
            max_sessions: 8,
            invoke_timeout_ms: 2000,
            max_envelope_bytes: DEFAULT_MAX_ENVELOPE_BYTES,
            tool_allowlist: operator_allowlist.clone(),
            peer_ids: Vec::new(),
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws_addr = listener.local_addr().unwrap();
        let sessions = Arc::new(PeerSessionManager::new());
        let manifest = Arc::new(daemon_manifest("daemon-test", &operator_allowlist));
        let shutdown = Arc::new(Notify::new());
        let options = PeerResponderOptions {
            identity_seed: seed_host(),
            manifest,
            allowlist: peer_allowlist,
            peer_keys,
            reserved_tool_ids: std::collections::HashSet::new(),
        };
        let accept_task = spawn_accept_loop(
            listener,
            config,
            Arc::clone(&sessions),
            options,
            Arc::clone(&shutdown),
        );

        // ── Workspace + user-cap scan ──
        let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
        let scan = tempfile::tempdir().expect("scan dir");
        if with_user_cap {
            write_capability_dir(scan.path(), USER_CAP);
        }
        let scan_dir = scan.path().join("capabilities");
        let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let deps = CapabilityRuntimeDeps {
            pool: None,
            worker_provider: None,
            daemon_tool_dispatch: None,
            cdn_config: None,
        };
        let (registry, outcome) =
            CapabilityRegistry::with_runtime_deps_and_user_caps(&deps, &scan_dir);
        assert!(
            outcome.skipped.is_empty(),
            "no skips expected: {:?}",
            outcome.skipped
        );
        state.set_capability_registry(Arc::new(registry));

        // ── Real axum HTTP listener on 127.0.0.1:0 ──
        let http_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let http_port = http_listener.local_addr().unwrap().port();
        let app = create_router(
            state,
            DaemonApiConfig::keyless().with_resolved_listen_addr(http_port, "127.0.0.1"),
        );
        let http_task = tokio::spawn(async move {
            axum::serve(http_listener, app).await.expect("http serve");
        });

        Self {
            ws_addr,
            http_url: format!("http://127.0.0.1:{http_port}"),
            shutdown,
            accept_task,
            http_task,
            sessions,
            _scan: scan,
            _tmp: tmp,
        }
    }

    /// Stop the daemon: fire the accept-loop shutdown and abort the HTTP
    /// server.
    fn stop(&self) {
        self.shutdown.notify_one();
        self.accept_task.abort();
        self.http_task.abort();
    }
}

impl Drop for E2eDaemon {
    fn drop(&mut self) {
        self.shutdown.notify_one();
        self.http_task.abort();
    }
}

/// Dial a spoke adapter against the daemon's real WS listener.
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

/// Dialer hello advertising one deterministic tool with its real schema.
fn dialer_manifest(host_id: &str, tool_id: &str) -> HostCapabilityManifest {
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
            "description": format!("{tool_id} deterministic E2E tool"),
            "input": echo_input_schema(),
            "output": echo_output_schema(),
        }],
    }))
    .expect("valid dialer manifest")
}

/// An echo tool handler: answers with the arguments echoed back.
fn echo_handler() -> ToolHandler {
    Arc::new(|args: Value| {
        Box::pin(async move { spoke_ok(json!({ "echo": args })) })
            as BoxFuture<'static, SpokeResult<Value>>
    })
}

/// Evict any rows a previously panicked test left in the process-global
/// table, so every test starts hermetic (a panic skips teardown; the next
/// test must not observe the leaked rows).
fn clear_peer_table() {
    let peer_ids: std::collections::HashSet<String> = peer_tool_table()
        .entries()
        .iter()
        .map(|e| e.peer_id.clone())
        .collect();
    for peer_id in peer_ids {
        peer_tool_table().evict_peer(&peer_id, None);
    }
}

/// Await a condition until it holds or the deadline elapses
/// (deadline-polling: 20 ms poll interval, S-e — not a readiness event).
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

/// Write an admitted `<name>/capability.json` trio at
/// `<root>/capabilities/<name>/` using the embedded basic-combat module
/// (real hash pairing so the scan admits it).
fn write_capability_dir(root: &std::path::Path, name: &str) {
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
            "inputSchema": "{{\"type\":\"object\",\"properties\":{{\"topic\":{{\"type\":\"string\"}}}}}}",
            "outputSchema": "{{\"type\":\"object\"}}",
            "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{sha}" }}
        }}"#
    );
    std::fs::write(dir.join("capability.json"), descriptor).unwrap();
    std::fs::write(dir.join("manifest.json"), manifest_json).unwrap();
    std::fs::write(dir.join("basic-combat.wasm"), wasm).unwrap();
}

/// Shutdown a test daemon and clean the process-global table.
#[allow(clippy::needless_pass_by_value)] // Drop keeps the temp dirs alive
fn teardown(server: E2eDaemon, peer_ids: &[String]) {
    for peer_id in peer_ids {
        peer_tool_table().evict_peer(peer_id, None);
    }
    server.stop();
}

// ── Spawned MCP child (P0 form) ───────────────────────────────────────────

/// Real spawned `nexus42 mcp serve` child with a hermetic `$HOME` whose
/// `.nexus42/config.toml` points at the harness daemon. Killed on drop.
struct McpChild {
    child: tokio::process::Child,
    _home: tempfile::TempDir,
}

impl McpChild {
    fn spawn(daemon_url: &str) -> Self {
        let home = tempfile::tempdir().expect("temp home");
        let nexus_dir = home.path().join(".nexus42");
        std::fs::create_dir_all(&nexus_dir).expect("nexus dir");
        std::fs::write(
            nexus_dir.join("config.toml"),
            format!("daemon_url = \"{daemon_url}\"\n"),
        )
        .expect("config.toml");

        let child = tokio::process::Command::new(env!("CARGO_BIN_EXE_nexus42"))
            .args(["mcp", "serve"])
            .env("HOME", home.path())
            .env("RUST_LOG", "off")
            .env("NO_PROXY", "127.0.0.1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn nexus42 mcp serve");
        Self { child, _home: home }
    }

    const fn take_transport(
        &mut self,
    ) -> (tokio::process::ChildStdout, tokio::process::ChildStdin) {
        (
            self.child.stdout.take().expect("child stdout"),
            self.child.stdin.take().expect("child stdin"),
        )
    }

    fn kill(&mut self) {
        let _ = self.child.start_kill();
    }
}

impl Drop for McpChild {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

type McpClient = RunningService<rmcp::RoleClient, ClientInfo>;

/// Spawn the child and complete the MCP initialize handshake (bounded —
/// S-c/QC3 S-2: a stalled child must fail this test, never hang the whole
/// `#[serial(e2e_peer_mcp)]` group).
async fn mcp_client(daemon_url: &str) -> (McpClient, McpChild) {
    let mut child = McpChild::spawn(daemon_url);
    let transport = child.take_transport();
    let running = tokio::time::timeout(
        Duration::from_secs(15),
        serve_client(ClientInfo::default(), transport),
    )
    .await
    .expect("initialize handshake is bounded (no hang)")
    .expect("initialize handshake completes");
    (running, child)
}

/// Sorted `tools/list` names from the MCP client.
async fn list_tool_ids(running: &McpClient) -> Vec<String> {
    let list = running.list_tools(None).await.expect("tools/list succeeds");
    let mut ids: Vec<String> = list.tools.iter().map(|t| t.name.to_string()).collect();
    ids.sort();
    ids
}

/// tools/call of a peer tool with schema-valid arguments; returns the
/// structured content.
async fn call_peer(running: &McpClient, id: &str, args: Value) -> Value {
    let call = running
        .call_tool(
            CallToolRequestParams::new(id.to_owned())
                .with_arguments(args.as_object().expect("object args").clone()),
        )
        .await
        .expect("tools/call succeeds");
    assert_ne!(call.is_error, Some(true), "{id} call not an error");
    call.structured_content.expect("structured result present")
}

/// tools/call that must be refused as `METHOD_NOT_FOUND`, bounded.
async fn expect_method_not_found(running: &McpClient, id: &str) {
    let err = tokio::time::timeout(
        Duration::from_secs(15),
        running.call_tool(CallToolRequestParams::new(id.to_owned())),
    )
    .await
    .expect("refusal is bounded (no hang)");
    let err = err.expect_err("unroutable id -> protocol error");
    let ServiceError::McpError(data) = err else {
        panic!("expected McpError, got {err:?}");
    };
    assert_eq!(data.code, ErrorCode::METHOD_NOT_FOUND, "{id} -> -32601");
}

/// Assert the ONE-catalog shape: ≥1 builtin `nexus.*` row, the user cap,
/// exactly our peer tool, and no other `tools.*` ids.
fn assert_one_catalog(ids: &[String]) {
    assert!(
        ids.iter().any(|id| id.starts_with("nexus.")),
        "builtin rows present: {ids:?}"
    );
    assert!(
        ids.iter().any(|id| id == USER_CAP),
        "user capability present: {ids:?}"
    );
    let peer_ids: Vec<&String> = ids.iter().filter(|id| id.starts_with("tools.")).collect();
    assert_eq!(
        peer_ids,
        vec![&TOOL_ID.to_string()],
        "exactly the admitted peer tool in the catalog: {ids:?}"
    );
}

// ── Journey A: happy path (P0 rmcp form) ──────────────────────────────────

#[tokio::test]
#[serial(e2e_peer_mcp)]
async fn journey_a_rmcp_one_catalog_schema_matching_call() {
    clear_peer_table();
    let peer_id = peer_id_of(seed_peer(1));
    let server = E2eDaemon::start(
        vec![TOOL_ID.to_owned()],
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(1)))]),
        true,
    )
    .await;

    let adapter = dial(server.ws_addr, seed_peer(1), TOOL_ID)
        .await
        .expect("dial succeeds");
    adapter.register_tool_handler(TOOL_ID, echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get(TOOL_ID).is_some(),
            Duration::from_secs(5)
        )
        .await,
        "admitted"
    );

    // The spawned MCP child reads the REAL daemon over loopback HTTP.
    let (running, mut child) = mcp_client(&server.http_url).await;

    // ONE catalog: builtin + user cap + peer tool.
    let ids = list_tool_ids(&running).await;
    assert_one_catalog(&ids);

    // Schema mapping, verbatim (AR-70 §3): peer rows carry the advertised
    // input/output schemas.
    let list = running.list_tools(None).await.expect("tools/list");
    let by_name: std::collections::HashMap<_, _> =
        list.tools.iter().map(|t| (t.name.to_string(), t)).collect();
    let peer = by_name.get(TOOL_ID).expect("peer row");
    assert_eq!(
        serde_json::Value::Object((*peer.input_schema).clone()),
        echo_input_schema(),
        "peer input schema advertised verbatim"
    );
    assert_eq!(
        peer.output_schema
            .as_ref()
            .map(|s| serde_json::Value::Object((**s).clone())),
        Some(echo_output_schema()),
        "peer output schema advertised verbatim"
    );

    // tools/call with schema-valid args -> structured_content matches the
    // advertised output schema.
    let result = call_peer(&running, TOOL_ID, json!({ "msg": "hello" })).await;
    assert_eq!(
        result,
        json!({ "echo": { "msg": "hello" } }),
        "schema-matching structured result"
    );

    drop(running);
    child.kill();
    teardown(server, &[peer_id]);
}

// ── Journey A: AC-closing form (scripted ACP agent) ──────────────────────

/// Evidence collected by the scripted agent's `session/new` handler.
#[derive(Debug)]
struct AcpEvidence {
    listed: Vec<String>,
    structured: Value,
    spawned_command: PathBuf,
    spawned_args: Vec<String>,
}

/// The scripted ACP agent: handles `session/new` by spawning the stdio
/// child named in `mcp_servers[0]`, running the MCP lifecycle against the
/// REAL daemon, and replying with the session id once evidence is
/// collected.
async fn run_scripted_acp_agent(
    agent_channel: Channel,
    daemon_url: String,
    evidence_tx: tokio::sync::mpsc::UnboundedSender<AcpEvidence>,
) -> agent_client_protocol::Result<()> {
    Agent
        .builder()
        .name("e2e-acp-agent")
        .on_receive_request(
            async move |initialize: InitializeRequest, responder, _connection| {
                responder.respond(
                    InitializeResponse::new(initialize.protocol_version)
                        .agent_capabilities(AgentCapabilities::new()),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: NewSessionRequest, responder, _connection| {
                assert_eq!(request.mcp_servers.len(), 1, "one MCP server injected");
                let McpServer::Stdio(stdio) = &request.mcp_servers[0] else {
                    panic!("expected McpServer::Stdio");
                };
                assert_eq!(stdio.name, "nexus", "server name preserved");
                assert!(
                    stdio.command.ends_with("nexus42"),
                    "command = CARGO_BIN_EXE_nexus42, got {}",
                    stdio.command.display()
                );
                assert_eq!(
                    stdio.args,
                    vec!["mcp".to_string(), "serve".to_string()],
                    "args = [mcp, serve]"
                );

                // The agent's own MCP client spawns the child with a
                // hermetic HOME pointed at the REAL daemon.
                let (running, mut child) = mcp_client(&daemon_url).await;

                let listed = list_tool_ids(&running).await;
                let structured = call_peer(&running, TOOL_ID, json!({ "msg": "hi" })).await;

                let _ = evidence_tx.send(AcpEvidence {
                    listed,
                    structured,
                    spawned_command: stdio.command.clone(),
                    spawned_args: stdio.args.clone(),
                });

                drop(running);
                child.kill();

                responder.respond(NewSessionResponse::new(SessionId::new("e2e-acp-session")))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_to(agent_channel)
        .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial(e2e_peer_mcp)]
async fn journey_a_acp_agent_via_new_session_closes() {
    clear_peer_table();
    let peer_id = peer_id_of(seed_peer(2));
    let server = E2eDaemon::start(
        vec![TOOL_ID.to_owned()],
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(2)))]),
        true,
    )
    .await;

    let adapter = dial(server.ws_addr, seed_peer(2), TOOL_ID)
        .await
        .expect("dial succeeds");
    adapter.register_tool_handler(TOOL_ID, echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get(TOOL_ID).is_some(),
            Duration::from_secs(5)
        )
        .await,
        "admitted"
    );

    let (agent_channel, client_channel) = Channel::duplex();
    let (evidence_tx, mut evidence_rx) = tokio::sync::mpsc::unbounded_channel();
    let agent_task = tokio::spawn(run_scripted_acp_agent(
        agent_channel,
        server.http_url.clone(),
        evidence_tx,
    ));

    let client_result = Client
        .builder()
        .name("e2e-acp-client")
        .connect_with(client_channel, async move |connection| {
            connection
                .send_request(InitializeRequest::new(ProtocolVersion::V1))
                .block_task()
                .await?;

            let session_req = NewSessionRequest::new("/tmp").mcp_servers(vec![
                nexus_acp_host::nexus_mcp_stdio_server(env!("CARGO_BIN_EXE_nexus42")),
            ]);
            let resp = connection.send_request(session_req).block_task().await?;
            assert_eq!(resp.session_id.to_string(), "e2e-acp-session");
            Ok(())
        })
        .await;
    assert!(client_result.is_ok(), "client result: {client_result:?}");

    let evidence = tokio::time::timeout(Duration::from_secs(60), evidence_rx.recv())
        .await
        .expect("evidence arrives within 60s")
        .expect("agent sent evidence");

    agent_task.abort();

    assert_one_catalog(&evidence.listed);
    assert_eq!(
        evidence.structured,
        json!({ "echo": { "msg": "hi" } }),
        "AC-closing schema-matching call"
    );
    assert!(
        evidence.spawned_command.ends_with("nexus42"),
        "spawned command is the nexus42 binary"
    );
    assert_eq!(evidence.spawned_args, vec!["mcp", "serve"]);

    teardown(server, &[peer_id]);
}

// ── Supplementary: worker spine leg (scripted agent_tool_request) ────────

#[tokio::test]
#[serial(e2e_peer_mcp)]
async fn journey_a_worker_spine_leg_dispatches_same_id() {
    clear_peer_table();
    let peer_id = peer_id_of(seed_peer(3));
    let server = E2eDaemon::start(
        vec![TOOL_ID.to_owned()],
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(3)))]),
        false,
    )
    .await;

    let adapter = dial(server.ws_addr, seed_peer(3), TOOL_ID)
        .await
        .expect("dial succeeds");
    adapter.register_tool_handler(TOOL_ID, echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get(TOOL_ID).is_some(),
            Duration::from_secs(5)
        )
        .await,
        "admitted"
    );

    let (_tmp, root, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(root, db_path, None).await;
    let result = HostToolExecutor::dispatch_from_worker(
        TOOL_ID,
        &json!({ "msg": "from worker" }),
        "req-e2e-ws",
        &state,
    )
    .await;
    assert_eq!(result.request_id, "req-e2e-ws");
    assert!(result.grant, "worker leg grants: {result:?}");
    assert_eq!(
        result.output,
        Some(json!({ "echo": { "msg": "from worker" } })),
        "spoke result passes through verbatim"
    );
    assert!(result.error.is_none());

    teardown(server, &[peer_id]);
}

// ── Journey B (i): never-admitted id ⇒ METHOD_NOT_FOUND ──────────────────

#[tokio::test]
#[serial(e2e_peer_mcp)]
async fn journey_b_never_admitted_is_method_not_found() {
    clear_peer_table();
    let peer_id = peer_id_of(seed_peer(4));
    let server = E2eDaemon::start(
        vec![TOOL_ID.to_owned()],
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(4)))]),
        true,
    )
    .await;

    let adapter = dial(server.ws_addr, seed_peer(4), TOOL_ID)
        .await
        .expect("dial succeeds");
    adapter.register_tool_handler(TOOL_ID, echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get(TOOL_ID).is_some(),
            Duration::from_secs(5)
        )
        .await,
        "admitted"
    );

    let (running, mut child) = mcp_client(&server.http_url).await;
    expect_method_not_found(&running, "tools.demo-harness.ghost").await;

    drop(running);
    child.kill();
    teardown(server, &[peer_id]);
}

// ── Journey B (ii): default-deny (empty allowlist) ⇒ empty peer catalog ──

#[tokio::test]
#[serial(e2e_peer_mcp)]
async fn journey_b_default_deny_empty_peer_catalog() {
    clear_peer_table();
    let peer_id = peer_id_of(seed_peer(5));
    // Operator allowlist ABSENT: the dialer still handshakes (Layer 0
    // peer allowlist present) but zero tools are admitted (default deny).
    let server = E2eDaemon::start(
        Vec::new(),
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(5)))]),
        true,
    )
    .await;

    let adapter = dial(server.ws_addr, seed_peer(5), TOOL_ID)
        .await
        .expect("dial succeeds");
    adapter.register_tool_handler(TOOL_ID, echo_handler());
    // The dialer session REGISTERS (Layer 0 allowlist present) but zero
    // tools are admitted (default-deny): wait on the session count, then
    // assert the table stayed empty.
    assert!(
        wait_until(
            || server.sessions.session_count() == 1,
            Duration::from_secs(5)
        )
        .await,
        "session established under default-deny"
    );
    assert!(
        peer_tool_table().entries().is_empty(),
        "default-deny leaves the peer table empty"
    );

    let (running, mut child) = mcp_client(&server.http_url).await;
    let ids = list_tool_ids(&running).await;
    assert!(
        !ids.iter().any(|id| id.starts_with("tools.")),
        "no peer tools in the catalog under default-deny: {ids:?}"
    );
    assert!(
        ids.iter().any(|id| id == USER_CAP),
        "user caps still listed: {ids:?}"
    );

    drop(running);
    child.kill();
    teardown(server, &[peer_id]);
}

// ── Journey B (iii): integrator disconnect ⇒ eviction + honest refusal ───

#[tokio::test]
#[serial(e2e_peer_mcp)]
async fn journey_b_disconnect_evicts_zero_stale_rows() {
    clear_peer_table();
    let peer_id = peer_id_of(seed_peer(6));
    let server = E2eDaemon::start(
        vec![TOOL_ID.to_owned()],
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(6)))]),
        true,
    )
    .await;

    let adapter = dial(server.ws_addr, seed_peer(6), TOOL_ID)
        .await
        .expect("dial succeeds");
    adapter.register_tool_handler(TOOL_ID, echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get(TOOL_ID).is_some(),
            Duration::from_secs(5)
        )
        .await,
        "admitted"
    );

    let (running, mut child) = mcp_client(&server.http_url).await;
    let ids = list_tool_ids(&running).await;
    assert!(
        ids.iter().any(|id| id == TOOL_ID),
        "peer visible before disconnect: {ids:?}"
    );

    // Disconnect the integrator.
    adapter.close();

    // Eviction readiness wait (deadline-polled): the id leaves the table.
    assert!(
        wait_until(
            || peer_tool_table().get(TOOL_ID).is_none(),
            Duration::from_secs(5)
        )
        .await,
        "evicted from the peer tool table"
    );
    assert!(
        peer_tool_table().entries().is_empty(),
        "zero stale table rows after eviction"
    );

    // Same suite: table + catalog + tools/list all consistent.
    let ids = list_tool_ids(&running).await;
    assert!(
        !ids.iter().any(|id| id == TOOL_ID),
        "evicted id absent from tools/list: {ids:?}"
    );

    // tools/call of the evicted id is refused, bounded.
    expect_method_not_found(&running, TOOL_ID).await;

    drop(running);
    child.kill();
    teardown(server, &[peer_id]);
}

// ── Journey B (iv): daemon stopped ⇒ bounded INTERNAL_ERROR ───────────────

#[tokio::test]
#[serial(e2e_peer_mcp)]
async fn journey_b_daemon_stopped_is_bounded_internal_error() {
    clear_peer_table();
    let peer_id = peer_id_of(seed_peer(7));
    let server = E2eDaemon::start(
        vec![TOOL_ID.to_owned()],
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(7)))]),
        false,
    )
    .await;

    let adapter = dial(server.ws_addr, seed_peer(7), TOOL_ID)
        .await
        .expect("dial succeeds");
    adapter.register_tool_handler(TOOL_ID, echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get(TOOL_ID).is_some(),
            Duration::from_secs(5)
        )
        .await,
        "admitted"
    );

    // Child is up and talking to the daemon.
    let (running, mut child) = mcp_client(&server.http_url).await;
    let ids = list_tool_ids(&running).await;
    assert!(
        ids.iter().any(|id| id == TOOL_ID),
        "daemon up before stop: {ids:?}"
    );

    // Stop the daemon: accept loop + HTTP listener both go away, and the
    // integrator session is torn down (the daemon's dispatch face is
    // gone — a whole-process stop).
    server.stop();
    adapter.close();
    assert!(
        wait_until(
            || peer_tool_table().get(TOOL_ID).is_none(),
            Duration::from_secs(5)
        )
        .await,
        "peer rows evicted with the daemon"
    );

    // Port-closure readiness: a TCP connect to the HTTP port must be
    // REFUSED before we assert the refused call.
    let http_port = server.http_url.rsplit(':').next().unwrap().to_string();
    let port: u16 = http_port.parse().expect("port");
    assert!(
        wait_until(
            || std::net::TcpStream::connect(("127.0.0.1", port)).is_err(),
            Duration::from_secs(5)
        )
        .await,
        "daemon HTTP port closed after stop"
    );

    drop(running);
    child.kill();

    // A FRESH child (new process, no pooled connections to the dead
    // daemon) must fail fast with a bounded INTERNAL_ERROR: the child
    // maps the daemon-unreachable connect refusal; NO_PROXY keeps the
    // connection direct loopback, no proxy interception.
    let (running2, mut child2) = mcp_client(&server.http_url).await;
    let result = tokio::time::timeout(
        Duration::from_secs(15),
        running2.call_tool(CallToolRequestParams::new(TOOL_ID.to_owned())),
    )
    .await
    .expect("daemon-down error is bounded (no hang)");
    let err = result.expect_err("daemon unreachable -> INTERNAL_ERROR");
    let ServiceError::McpError(data) = err else {
        panic!("expected McpError, got {err:?}");
    };
    assert_eq!(
        data.code,
        ErrorCode::INTERNAL_ERROR,
        "daemon-down -> -32603"
    );

    drop(running2);
    child2.kill();
    teardown(server, &[peer_id]);
}

// ── Journey B (v): child exits mid-session ⇒ bounded failure (S-f) ───────

#[tokio::test]
#[serial(e2e_peer_mcp)]
async fn journey_b_child_exit_mid_session_is_bounded_failure() {
    clear_peer_table();
    let peer_id = peer_id_of(seed_peer(8));
    let server = E2eDaemon::start(
        vec![TOOL_ID.to_owned()],
        vec![peer_id.clone()],
        HashMap::from([(peer_id.clone(), pubkey(seed_peer(8)))]),
        true,
    )
    .await;

    let adapter = dial(server.ws_addr, seed_peer(8), TOOL_ID)
        .await
        .expect("dial succeeds");
    adapter.register_tool_handler(TOOL_ID, echo_handler());
    assert!(
        wait_until(
            || peer_tool_table().get(TOOL_ID).is_some(),
            Duration::from_secs(5)
        )
        .await,
        "admitted"
    );

    let (running, mut child) = mcp_client(&server.http_url).await;
    // Prove the session is live before the child exits.
    let ok = call_peer(&running, TOOL_ID, json!({ "msg": "before-exit" })).await;
    assert_eq!(ok, json!({ "echo": { "msg": "before-exit" } }));

    // Kill the child mid-session; the next tools/call must fail BOUNDED
    // (never hang the serialized group). The failure mode is SDK-owned
    // (AR-71 Model A — the child is the client's stdio process); this pins
    // the observable contract: a dead child ⇒ prompt, bounded error.
    child.kill();
    let result = tokio::time::timeout(
        Duration::from_secs(15),
        running.call_tool(CallToolRequestParams::new(TOOL_ID.to_owned())),
    )
    .await
    .expect("child-exit failure is bounded (no hang)");
    assert!(
        result.is_err(),
        "tools/call after child exit must fail: {result:?}"
    );

    drop(running);
    teardown(server, &[peer_id]);
}
