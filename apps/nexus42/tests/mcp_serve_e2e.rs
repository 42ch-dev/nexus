//! V1.174 P0 T6 (AR-74) — E2E MCP harness over a real spawned child.
//!
//! Drives `nexus42 mcp serve` (the T5 stdio bridge child, AR-71 Model A) as
//! a REAL subprocess with a hermetic `$HOME` whose `.nexus42/config.toml`
//! points `daemon_url` at a stub daemon (`wiremock`, the nexus42 dev-dep —
//! the stub approach keeps the DEFAULT graph free of spoke-connect/libp2p;
//! the daemon-side peer admission is covered by `peer_tool.rs` +
//! `honesty_lockstep.rs`).
//!
//! The rmcp client speaks JSON-RPC over `(child.stdout, child.stdin)` via
//! the `transport-async-rw` tuple transport. Scope (spec §8 + AR-70/71/72):
//! - initialize handshake completes (`serve_client` returns Ok);
//! - `tools/list` mirrors the daemon catalog BOTH directions (ids equal);
//! - `tools/call` success (structured result) + every error class:
//!   peer deny carries the ORIGINAL lowercase wire code verbatim
//!   (`details.wire_code` typed path, AR-70 #4), `invalid_input` names the
//!   spine code, unroutable (`unsupported tool:` prefix) → `METHOD_NOT_FOUND`,
//!   daemon-down → bounded `INTERNAL_ERROR`;
//! - `prompts/list` + `resources/list` assert EMPTY LISTS (rmcp 1.8.0
//!   reality — the bridge keeps SDK defaults; NOT protocol errors).

#![cfg(feature = "connect-client")]
// Justification (repo convention, cf. tests/works_api.rs): integration-test
// assertions operate on fixed local fixtures where a panic IS the failure
// signal; `.unwrap()`/`.expect()` keep the tests linear and readable.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::future::Future;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nexus_daemon_runtime::capability_registry::host_tool_registry;
use rmcp::model::{CallToolRequestParams, ClientInfo, ErrorCode};
use rmcp::serve_client;
use rmcp::ServiceError;
use serde_json::{json, Value};
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Spawned `nexus mcp serve` child with a hermetic HOME pointing at a
/// stub daemon. Kills the child on drop.
struct McpChild {
    child: tokio::process::Child,
    #[allow(dead_code)]
    home: tempfile::TempDir,
}

impl McpChild {
    fn spawn(mock: &MockServer) -> Self {
        Self::spawn_with(mock, None)
    }

    /// Spawn the child with an optional `~/.nexus42/connect/daemon.json`
    /// body (V1.180 P1, RN-OGA-2): the child reads the daemon-local
    /// `mcp_visibility` subset from it. `None` = no file ⇒ absent policy
    /// (byte-identical current behavior).
    fn spawn_with(mock: &MockServer, daemon_json: Option<&str>) -> Self {
        let home = tempfile::tempdir().expect("temp home");
        let nexus_dir = home.path().join(".nexus42");
        std::fs::create_dir_all(&nexus_dir).expect("nexus dir");
        std::fs::write(
            nexus_dir.join("config.toml"),
            format!("daemon_url = \"{}\"\n", mock.uri()),
        )
        .expect("config.toml");
        if let Some(body) = daemon_json {
            std::fs::create_dir_all(nexus_dir.join("connect")).expect("connect dir");
            std::fs::write(nexus_dir.join("connect").join("daemon.json"), body)
                .expect("daemon.json");
        }

        let child = tokio::process::Command::new(env!("CARGO_BIN_EXE_nexus42"))
            .args(["mcp", "serve"])
            .env("HOME", home.path())
            .env("RUST_LOG", "off")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn nexus42 mcp serve");
        Self { child, home }
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

/// The builtin rows are derived from the REAL registry
/// (`host_tool_registry()`) so the fixture can never drift from the
/// authored `CatalogDescriptor` — the AR-80 #1 schema-equality pin below
/// compares the child's parsed `tools/list` schemas against the same
/// registry source for ALL 30 builtin ids (F-4, qc1 W-002). Peer/user
/// rows stay on the V1.174 placeholder shape (AR-80 #3: those lanes are
/// untouched).
fn catalog_body() -> Value {
    let registry = host_tool_registry();
    let mut items: Vec<Value> = registry
        .ids()
        .map(|id| {
            let row = registry.lookup(id).expect("builtin row exists");
            json!({
                "id": id,
                "description": row.catalog.description,
                "input_schema": row.catalog.input_schema.expect("authored input"),
                "output_schema": row.catalog.output_schema,
                "origin": "builtin"
            })
        })
        .collect();
    items.push(json!({
        "id": "t6.wcap",
        "description": "t6.wcap",
        "input_schema": "{\"type\":\"object\"}",
        "output_schema": "{\"type\":\"object\"}",
        "origin": "user"
    }));
    items.push(json!({
        "id": "tools.t6.echo",
        "description": "tools.t6.echo test tool",
        "input_schema": "{\"type\":\"object\"}",
        "output_schema": "{\"type\":\"object\"}",
        "origin": "peer"
    }));
    json!({ "items": items })
}

/// Mount the catalog mock; returns the sorted catalog id set.
async fn mount_catalog(mock: &MockServer) -> Vec<String> {
    let body = catalog_body();
    let ids: Vec<String> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_owned())
        .collect();
    Mock::given(method("GET"))
        .and(path("/v1/daemon/tools"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(mock)
        .await;
    ids
}

/// Mount one tool-execution response for the given tool.
async fn mount_execution(mock: &MockServer, tool_name: &str, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/v1/daemon/agent-host/internal/tool-executions"))
        .and(body_partial_json(json!({ "tool_name": tool_name })))
        .respond_with(response)
        .mount(mock)
        .await;
}

/// Spawn a child against a fresh stub daemon, complete the initialize
/// handshake, and return (running client, child, catalog ids).
async fn start_bridge() -> (
    rmcp::service::RunningService<rmcp::RoleClient, ClientInfo>,
    McpChild,
    Vec<String>,
) {
    let mock = MockServer::start().await;
    let ids = mount_catalog(&mock).await;
    let mut child = McpChild::spawn(&mock);
    let transport = child.take_transport();
    // serve_client sends `initialize` and waits for the response — Ok here
    // proves the child completed the handshake.
    let running = serve_client(ClientInfo::default(), transport)
        .await
        .expect("initialize handshake completes");
    (running, child, ids)
}

// ── initialize + tools/list ⇄ catalog ─────────────────────────────────────

#[tokio::test]
async fn initialize_handshake_and_tools_list_mirror_catalog() {
    let (running, mut child, catalog_ids) = start_bridge().await;
    assert!(!running.is_closed(), "client alive after handshake");

    let result = running.list_tools(None).await.expect("tools/list succeeds");
    // Both directions: every catalog id is listed, and nothing else is.
    let mut listed: Vec<String> = result.tools.iter().map(|t| t.name.to_string()).collect();
    listed.sort();
    let mut expected = catalog_ids.clone();
    expected.sort();
    assert_eq!(
        listed, expected,
        "tools/list == catalog ids (both directions)"
    );

    // Schema mapping pin (AR-70 §3 + AR-80 #3): builtin rows carry their
    // authored real schema (input + output when pinned); peer rows carry
    // output_schema when the catalog emitted one.
    let by_name: std::collections::HashMap<_, _> = result
        .tools
        .iter()
        .map(|t| (t.name.to_string(), t))
        .collect();
    let peer = by_name.get("tools.t6.echo").expect("peer row");
    assert_eq!(
        peer.output_schema.as_ref().map(|s| s.get("type")),
        Some(Some(&json!("object")))
    );

    // AR-80 #1 (schema-equality, catalog ⇄ tools/list, ALL 30 builtin
    // rows — F-4, qc1 W-002): the child parses the catalog's
    // `input_schema`/`output_schema` strings and carries them on the
    // `Tool` — the parsed `tools/list` schemas must equal the registry
    // `CatalogDescriptor` text parsed, per row, both directions. The
    // registry ⇄ catalog route leg is pinned in
    // `honesty_lockstep::builtin_catalog_schema_equality_registry_to_route`;
    // this leg closes the route ⇄ `tools/list` hop for every builtin id.
    let registry = host_tool_registry();
    for id in registry.ids() {
        let row = registry.lookup(id).expect("builtin row exists");
        let tool = by_name
            .get(id)
            .unwrap_or_else(|| panic!("builtin row '{id}' must be listed by tools/list"));
        let registry_input: Value =
            serde_json::from_str(row.catalog.input_schema.expect("authored input schema"))
                .expect("registry input parses");
        assert_eq!(
            tool.input_schema.as_ref(),
            registry_input
                .as_object()
                .expect("registry input is an object"),
            "tools/list input schema for '{id}' == registry descriptor (parsed)"
        );
        match (tool.output_schema.as_ref(), row.catalog.output_schema) {
            (Some(listed), Some(authored)) => {
                let registry_output: Value =
                    serde_json::from_str(authored).expect("registry output parses");
                assert_eq!(
                    listed.as_ref(),
                    registry_output
                        .as_object()
                        .expect("registry output is an object"),
                    "tools/list output schema for '{id}' == registry descriptor (parsed)"
                );
            }
            (None, None) => {}
            (Some(_), None) => {
                panic!("'{id}': tools/list carries an output schema the registry does not pin")
            }
            (None, Some(_)) => panic!("'{id}': registry pins an output schema tools/list dropped"),
        }
    }

    drop(running);
    child.kill();
}

// ── tools/call success + error classes ────────────────────────────────────

#[tokio::test]
async fn call_tool_success_returns_structured_content() {
    let mock = MockServer::start().await;
    mount_catalog(&mock).await;
    mount_execution(
        &mock,
        "tools.t6.echo",
        ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "result": { "echo": { "k": "v" } }
        })),
    )
    .await;
    let mut child = McpChild::spawn(&mock);
    let running = serve_client(ClientInfo::default(), child.take_transport())
        .await
        .expect("handshake");

    let result = running
        .call_tool(
            CallToolRequestParams::new("tools.t6.echo")
                .with_arguments(json!({ "k": "v" }).as_object().expect("object").clone()),
        )
        .await
        .expect("successful call");
    assert_ne!(result.is_error, Some(true), "not an error result");
    assert_eq!(
        result.structured_content,
        Some(json!({ "echo": { "k": "v" } }))
    );
    drop(running);
    child.kill();
}

#[tokio::test]
async fn call_tool_peer_deny_carries_original_lowercase_wirecode() {
    let mock = MockServer::start().await;
    mount_catalog(&mock).await;
    mount_execution(
        &mock,
        "tools.t6.echo",
        ResponseTemplate::new(400).set_body_json(json!({
            "success": false,
            "error": {
                "code": "not_supported",
                "message": "tool is not supported by this peer",
                "details": { "wire_code": "op_unsupported" }
            }
        })),
    )
    .await;
    let mut child = McpChild::spawn(&mock);
    let running = serve_client(ClientInfo::default(), child.take_transport())
        .await
        .expect("handshake");

    let result = running
        .call_tool(CallToolRequestParams::new("tools.t6.echo"))
        .await
        .expect("executed-but-failed is an Ok(CallToolResult::error)");
    assert_eq!(
        result.is_error,
        Some(true),
        "is_error set for executed failure"
    );
    let text = result
        .content
        .iter()
        .find_map(|c| c.as_text().map(|t| t.text.clone()))
        .expect("text content");
    assert_eq!(
        text, "op_unsupported: tool is not supported by this peer",
        "typed lowercase wire_code surfaces verbatim ahead of the message"
    );
    drop(running);
    child.kill();
}

#[tokio::test]
async fn call_tool_invalid_input_names_spine_code() {
    let mock = MockServer::start().await;
    mount_catalog(&mock).await;
    mount_execution(
        &mock,
        "tools.t6.echo",
        ResponseTemplate::new(400).set_body_json(json!({
            "success": false,
            "error": {
                "code": "invalid_input",
                "message": "Missing required tool arguments: x"
            }
        })),
    )
    .await;
    let mut child = McpChild::spawn(&mock);
    let running = serve_client(ClientInfo::default(), child.take_transport())
        .await
        .expect("handshake");

    let result = running
        .call_tool(CallToolRequestParams::new("tools.t6.echo"))
        .await
        .expect("executed failure");
    assert_eq!(result.is_error, Some(true));
    let text = result
        .content
        .iter()
        .find_map(|c| c.as_text().map(|t| t.text.clone()))
        .expect("content");
    assert_eq!(text, "invalid_input: Missing required tool arguments: x");
    drop(running);
    child.kill();
}

#[tokio::test]
async fn call_tool_unroutable_is_method_not_found() {
    let mock = MockServer::start().await;
    mount_catalog(&mock).await;
    mount_execution(
        &mock,
        "tools.t6.gone",
        ResponseTemplate::new(400).set_body_json(json!({
            "success": false,
            "error": {
                "code": "not_supported",
                "message": "unsupported tool: tools.t6.gone"
            }
        })),
    )
    .await;
    let mut child = McpChild::spawn(&mock);
    let running = serve_client(ClientInfo::default(), child.take_transport())
        .await
        .expect("handshake");

    let err = running
        .call_tool(CallToolRequestParams::new("tools.t6.gone"))
        .await
        .expect_err("unroutable id -> protocol error");
    let ServiceError::McpError(data) = err else {
        panic!("expected McpError, got {err:?}");
    };
    assert_eq!(
        data.code,
        ErrorCode::METHOD_NOT_FOUND,
        "unsupported tool: → -32601"
    );
    drop(running);
    child.kill();
}

#[tokio::test]
async fn call_tool_daemon_down_is_bounded_internal_error() {
    // Point the child at a VALID-format but unreachable daemon address: a
    // closed loopback port (127.0.0.1, port 1 — nothing listens there).
    // This pins the REAL bounded-timeout path (QC3 W-3): reqwest's connect
    // to a dead-end socket fails through the daemon client's
    // connect/request timeouts and the bridge maps it to INTERNAL_ERROR.
    // The previous pin used an out-of-range port (99999 > 65535), which
    // reqwest rejects at request-BUILD time — no socket involved — so a
    // timeout regression would have passed silently.
    //
    // NO_PROXY is set on the child because reqwest's system-proxy lookup
    // (macOS system-configuration) would otherwise route loopback through
    // the machine's HTTP proxy, which answers empty 200s and defeats the
    // dead-end-socket pin. With NO_PROXY=127.0.0.1 the connect is direct
    // and deterministic (immediate connect-refused; no proxy interception).
    let home = tempfile::tempdir().expect("temp home");
    let nexus_dir = home.path().join(".nexus42");
    std::fs::create_dir_all(&nexus_dir).expect("nexus dir");
    std::fs::write(
        nexus_dir.join("config.toml"),
        "daemon_url = \"http://127.0.0.1:1\"\n",
    )
    .expect("config.toml");
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_nexus42"))
        .args(["mcp", "serve"])
        .env("HOME", home.path())
        .env("RUST_LOG", "off")
        .env("NO_PROXY", "127.0.0.1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn nexus42 mcp serve");
    let transport = (
        child.stdout.take().expect("child stdout"),
        child.stdin.take().expect("child stdin"),
    );
    let running = serve_client(ClientInfo::default(), transport)
        .await
        .expect("handshake");

    let result = tokio::time::timeout(
        Duration::from_secs(15),
        running.call_tool(CallToolRequestParams::new("tools.t6.echo")),
    )
    .await
    .expect("daemon-down error is bounded (no hang)");
    let err = result.expect_err("daemon unreachable → INTERNAL_ERROR");
    let ServiceError::McpError(data) = err else {
        panic!("expected McpError, got {err:?}");
    };
    assert_eq!(data.code, ErrorCode::INTERNAL_ERROR, "daemon-down → -32603");
    drop(running);
    let _ = child.start_kill();
}

// ── prompts / resources: EMPTY lists (rmcp 1.8.0 reality) ─────────────────

#[tokio::test]
async fn prompts_and_resources_are_empty_lists_not_errors() {
    let (running, mut child, _ids) = start_bridge().await;

    let prompts = running
        .list_prompts(None)
        .await
        .expect("list_prompts succeeds");
    assert!(
        prompts.prompts.is_empty(),
        "prompts list empty (SDK default)"
    );

    let resources = running
        .list_resources(None)
        .await
        .expect("list_resources succeeds");
    assert!(
        resources.resources.is_empty(),
        "resources list empty (SDK default)"
    );

    drop(running);
    child.kill();
}
// ── listChanged: advertisement + live-session delivery (AR-79, DF-90) ────

/// Counts `notifications/tools/list_changed` received by the client.
#[derive(Clone, Default)]
struct ListChangedCounter {
    count: Arc<std::sync::atomic::AtomicUsize>,
}

impl ListChangedCounter {
    fn count(&self) -> usize {
        self.count.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl rmcp::ClientHandler for ListChangedCounter {
    fn on_tool_list_changed(
        &self,
        _context: rmcp::service::NotificationContext<rmcp::RoleClient>,
    ) -> impl Future<Output = ()> + rmcp::service::MaybeSendFuture + '_ {
        self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        std::future::ready(())
    }
}

/// A catalog stub whose body can be mutated mid-session. The responder
/// closure reads the current body on every request, so the watcher's next
/// poll observes the mutation.
#[derive(Clone)]
struct MutableCatalog {
    body: Arc<Mutex<Value>>,
}

impl MutableCatalog {
    fn new(body: Value) -> Self {
        Self {
            body: Arc::new(Mutex::new(body)),
        }
    }

    fn set(&self, body: Value) {
        *self.body.lock().expect("catalog lock") = body;
    }

    fn current(&self) -> Value {
        self.body.lock().expect("catalog lock").clone()
    }
}

/// Mount a mutable catalog stub; returns the catalog handle.
async fn mount_mutable_catalog(mock: &MockServer) -> MutableCatalog {
    let catalog = MutableCatalog::new(catalog_body());
    let responder = catalog.clone();
    Mock::given(method("GET"))
        .and(path("/v1/daemon/tools"))
        .respond_with(move |_req: &wiremock::Request| {
            ResponseTemplate::new(200).set_body_json(responder.current())
        })
        .mount(mock)
        .await;
    catalog
}

/// Wait until the watcher has completed its first (baseline) poll: the
/// test's own `tools/list` plus the watcher's first poll make ≥ 2 GETs.
async fn wait_for_baseline_poll(mock: &MockServer) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        let n = mock
            .received_requests()
            .await
            .expect("recording enabled")
            .iter()
            .filter(|r| r.method == "GET" && r.url.path() == "/v1/daemon/tools")
            .count();
        if n >= 2 {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for the watcher's baseline poll"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Await the next `tools/list_changed` notification (count > `before`),
/// bounded by 2 × interval + margin (AR-79 #7).
async fn await_list_changed(counter: &ListChangedCounter, before: usize, what: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        if counter.count() > before {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for tools/list_changed after {what}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Sorted `tools/list` ids from the client.
async fn list_ids(
    running: &rmcp::service::RunningService<rmcp::RoleClient, ListChangedCounter>,
) -> Vec<String> {
    let list = running.list_tools(None).await.expect("tools/list succeeds");
    let mut ids: Vec<String> = list.tools.iter().map(|t| t.name.to_string()).collect();
    ids.sort();
    ids
}

#[tokio::test]
async fn list_changed_advertised_and_delivered() {
    let mock = MockServer::start().await;
    let catalog = mount_mutable_catalog(&mock).await;
    let mut child = McpChild::spawn(&mock);
    let transport = child.take_transport();
    let counter = ListChangedCounter::default();
    let running = serve_client(counter.clone(), transport)
        .await
        .expect("initialize handshake completes");

    // Advertisement pin (AR-79 #4): initialize capabilities include
    // `tools.listChanged`.
    let info = running.peer_info().expect("server info from initialize");
    let tools = info.capabilities.tools.as_ref().expect("tools capability");
    assert_eq!(
        tools.list_changed,
        Some(true),
        "initialize advertises tools.listChanged"
    );

    // Baseline tools/list.
    let baseline = list_ids(&running).await;
    assert_eq!(
        baseline.len(),
        32,
        "baseline catalog has 30 builtin + 2 overlay rows"
    );

    // Wait for the watcher's baseline poll so the first digest is set
    // (a mutation before that would be absorbed into the baseline).
    wait_for_baseline_poll(&mock).await;

    // ── Leg 1: peer admission ──
    let mut body = catalog.current();
    body["items"]
        .as_array_mut()
        .expect("items array")
        .push(json!({
            "id": "tools.t6.admitted",
            "description": "tools.t6.admitted test tool",
            "input_schema": "{\"type\":\"object\"}",
            "output_schema": "{\"type\":\"object\"}",
            "origin": "peer"
        }));
    catalog.set(body);
    let before = counter.count();
    await_list_changed(&counter, before, "peer admission").await;
    assert_eq!(counter.count(), before + 1, "exactly one notification");
    let after_admission = list_ids(&running).await;
    assert!(
        after_admission.contains(&"tools.t6.admitted".to_string()),
        "refreshed tools/list includes the admitted peer tool"
    );
    assert_eq!(after_admission.len(), 33, "one row added");

    // ── Leg 2: peer eviction ──
    let mut body = catalog.current();
    body["items"]
        .as_array_mut()
        .expect("items array")
        .retain(|item| item["id"] != "tools.t6.admitted");
    catalog.set(body);
    let before = counter.count();
    await_list_changed(&counter, before, "peer eviction").await;
    assert_eq!(counter.count(), before + 1, "exactly one notification");
    let after_eviction = list_ids(&running).await;
    assert!(
        !after_eviction.contains(&"tools.t6.admitted".to_string()),
        "refreshed tools/list drops the evicted peer tool"
    );
    assert_eq!(after_eviction.len(), 32, "back to 32 rows");

    // ── Leg 3: user-cap content change via mock-catalog mutation ──
    // The child watch is source-agnostic over the tools body (AR-79), so a
    // content-only change notifies without an id change. The live
    // add/remove journey (scan dir → daemon hot reload → listChanged) is
    // covered against the REAL daemon in e2e_peer_mcp.rs (V1.176 P1, AR-95
    // #4) — user-cap changes are no longer restart-scoped (RN-2).
    let mut body = catalog.current();
    for item in body["items"].as_array_mut().expect("items array") {
        if item["id"] == "t6.wcap" {
            item["description"] = json!("t6.wcap (updated)");
        }
    }
    catalog.set(body);
    let before = counter.count();
    await_list_changed(&counter, before, "user-cap catalog change").await;
    assert_eq!(counter.count(), before + 1, "exactly one notification");
    let after_user_cap = list_ids(&running).await;
    assert_eq!(
        after_user_cap, baseline,
        "ids unchanged for a content-only change"
    );
    let list = running.list_tools(None).await.expect("tools/list succeeds");
    let wcap = list
        .tools
        .iter()
        .find(|t| t.name == "t6.wcap")
        .expect("wcap row present");
    assert_eq!(
        wcap.description.as_deref(),
        Some("t6.wcap (updated)"),
        "refreshed tools/list carries the new description"
    );

    drop(running);
    child.kill();
}

#[tokio::test]
async fn no_list_changed_when_catalog_unchanged() {
    let mock = MockServer::start().await;
    mount_mutable_catalog(&mock).await;
    let mut child = McpChild::spawn(&mock);
    let transport = child.take_transport();
    let counter = ListChangedCounter::default();
    let running = serve_client(counter.clone(), transport)
        .await
        .expect("initialize handshake completes");

    // Baseline poll + ≥ 2 further intervals with an unchanged catalog.
    wait_for_baseline_poll(&mock).await;
    tokio::time::sleep(Duration::from_secs(5)).await; // > 2 × 2 s interval

    assert_eq!(
        counter.count(),
        0,
        "zero notifications when the catalog is unchanged"
    );

    drop(running);
    child.kill();
}

#[tokio::test]
async fn visibility_policy_filters_list_and_short_circuits_hidden_call() {
    // V1.180 P1 (RN-OGA-2): the child reads the daemon-local
    // `mcp_visibility` subset and applies it at the bridge seam —
    // `tools/list` is filtered to the configured subset and a hidden-tool
    // `tools/call` is refused with the `tool_not_authorized` discriminator (never
    // dispatched to the daemon), while a visible tool still dispatches.
    let mock = MockServer::start().await;
    mount_catalog(&mock).await;
    mount_execution(
        &mock,
        "tools.t6.echo",
        ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "result": { "echo": "ok" }
        })),
    )
    .await;
    let mut child = McpChild::spawn_with(&mock, Some(r#"{"mcp_visibility":["tools.t6.echo"]}"#));
    let transport = child.take_transport();
    let running = serve_client(ClientInfo::default(), transport)
        .await
        .expect("initialize handshake completes");

    // tools/list is filtered to the visible subset.
    let result = running.list_tools(None).await.expect("tools/list succeeds");
    let listed: Vec<String> = result.tools.iter().map(|t| t.name.to_string()).collect();
    assert_eq!(
        listed,
        vec!["tools.t6.echo".to_owned()],
        "only the visible tool is listed"
    );

    // A visible tool still dispatches through the daemon.
    let call = running
        .call_tool(CallToolRequestParams::new("tools.t6.echo"))
        .await
        .expect("visible tool call succeeds");
    assert_ne!(call.is_error, Some(true), "visible call not an error");

    // A hidden tool is refused at the seam (never reaches the daemon).
    let err = running
        .call_tool(CallToolRequestParams::new("t6.wcap"))
        .await
        .expect_err("hidden tool -> protocol error");
    let ServiceError::McpError(data) = err else {
        panic!("expected McpError, got {err:?}");
    };
    assert_eq!(
        data.code,
        ErrorCode::METHOD_NOT_FOUND,
        "hidden tool refused with METHOD_NOT_FOUND"
    );
    assert!(
        data.message.contains("tool_not_authorized"),
        "refusal names the visibility class: {}",
        data.message
    );

    drop(running);
    child.kill();
}

#[tokio::test]
async fn visible_but_denied_call_is_typed_refusal() {
    // V1.180 P1 (RN-OGA-2): a tool VISIBLE per policy but denied by
    // the authz spine (admission_pipeline Forbidden) is a typed
    // refusal — `Ok(CallToolResult::error)` naming the honest spine
    // code ahead of the message. Visibility never grants execute:
    // the spine's denial maps through the existing daemon error
    // mapping, unchanged.
    let mock = MockServer::start().await;
    mount_catalog(&mock).await;
    mount_execution(
        &mock,
        "tools.t6.echo",
        ResponseTemplate::new(403).set_body_json(json!({
            "success": false,
            "error": {
                "code": "forbidden",
                "message": "Forbidden: fs/* tools require an active workspace with defined bounds",
                "details": {
                    "resource": "tool_execution",
                    "reason": "fs/* tools require an active workspace with defined bounds"
                }
            }
        })),
    )
    .await;
    let mut child = McpChild::spawn_with(&mock, Some(r#"{"mcp_visibility":["tools.t6.echo"]}"#));
    let running = serve_client(ClientInfo::default(), child.take_transport())
        .await
        .expect("initialize handshake completes");

    let result = running
        .call_tool(CallToolRequestParams::new("tools.t6.echo"))
        .await
        .expect("visible-but-denied is an Ok(CallToolResult::error)");
    assert_eq!(
        result.is_error,
        Some(true),
        "is_error set for the spine-owned denial"
    );
    let text = result
        .content
        .iter()
        .find_map(|c| c.as_text().map(|t| t.text.clone()))
        .expect("text content");
    assert_eq!(
        text, "forbidden: Forbidden: fs/* tools require an active workspace with defined bounds",
        "spine-owned denial names the honest spine code ahead of the message"
    );

    drop(running);
    child.kill();
}
