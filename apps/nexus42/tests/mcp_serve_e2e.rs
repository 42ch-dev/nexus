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

use std::process::Stdio;
use std::time::Duration;

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
        let home = tempfile::tempdir().expect("temp home");
        let nexus_dir = home.path().join(".nexus42");
        std::fs::create_dir_all(&nexus_dir).expect("nexus dir");
        std::fs::write(
            nexus_dir.join("config.toml"),
            format!("daemon_url = \"{}\"\n", mock.uri()),
        )
        .expect("config.toml");

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

/// The catalog fixture served by the stub daemon (wire shape mirrors the
/// daemon's `GET /v1/daemon/tools`: `input_schema` is a JSON STRING).
fn catalog_body() -> Value {
    json!({
        "items": [
            {
                "id": "nexus.workspace.info",
                "description": "nexus.workspace.info (parameters: work.get.request)",
                "input_schema": "{\"type\":\"object\"}",
                "origin": "builtin"
            },
            {
                "id": "t6.wcap",
                "description": "t6.wcap",
                "input_schema": "{\"type\":\"object\"}",
                "output_schema": "{\"type\":\"object\"}",
                "origin": "user"
            },
            {
                "id": "tools.t6.echo",
                "description": "tools.t6.echo test tool",
                "input_schema": "{\"type\":\"object\"}",
                "output_schema": "{\"type\":\"object\"}",
                "origin": "peer"
            }
        ]
    })
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

    // Schema mapping pin (AR-70 §3): builtin rows carry the permissive
    // `{"type":"object"}` placeholder and NO output schema; peer rows carry
    // output_schema when the catalog emitted one.
    let by_name: std::collections::HashMap<_, _> = result
        .tools
        .iter()
        .map(|t| (t.name.to_string(), t))
        .collect();
    let builtin = by_name.get("nexus.workspace.info").expect("builtin row");
    assert_eq!(builtin.input_schema.get("type"), Some(&json!("object")));
    assert!(
        builtin.output_schema.is_none(),
        "builtin output schema omitted"
    );
    let peer = by_name.get("tools.t6.echo").expect("peer row");
    assert_eq!(
        peer.output_schema.as_ref().map(|s| s.get("type")),
        Some(Some(&json!("object")))
    );

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
