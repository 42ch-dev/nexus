//! V1.179 P0 T1 (DF-88) — Model B e2e: embedded rmcp over in-process
//! sink/stream.
//!
//! Drives the embedded MCP server (`connect/mcp_embedded.rs`) with a REAL
//! in-process rmcp client over a `SinkStreamTransport` pair — the same
//! `start_embedded_mcp_server` function the daemon boot block (`boot.rs`
//! §8.5) calls. Scope (DF-88 verify):
//! - `tools/list` mirrors the spine catalog (builtin rows from the real
//!   `host_tool_registry()`);
//! - `tools/call` against a registered builtin returns the structured
//!   result;
//! - unroutable ids map to `METHOD_NOT_FOUND` (same exact-id discriminator
//!   as the stdio path fixture, AR-70 #4);
//! - the fixture spawns NO `nexus42 mcp serve` child — the embedded path is
//!   in-process by construction (no process boundary, no loopback HTTP).
//!
//! Gated: `--features connect-client,embedded-mcp` (the nested feature
//! proof — the embedded server module compiles only under `embedded-mcp`).

#![cfg(feature = "embedded-mcp")]
// Justification (repo convention, cf. tests/works_api.rs): integration-test
// assertions operate on fixed local fixtures where a panic IS the failure
// signal; `.unwrap()`/`.expect()` keep the tests linear and readable.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use nexus_daemon_runtime::connect::mcp_embedded::start_embedded_mcp_server;
use nexus_daemon_runtime::test_utils::create_test_workspace;
use nexus_daemon_runtime::workspace::WorkspaceState;
use rmcp::model::{CallToolRequestParams, ClientInfo, ErrorCode};
use rmcp::serve_client;
use rmcp::ServiceError;

/// Establish one embedded session and complete the initialize handshake.
async fn establish_session(
    server: &nexus_daemon_runtime::connect::mcp_embedded::EmbeddedMcpServer,
) -> rmcp::service::RunningService<rmcp::RoleClient, ClientInfo> {
    let session = server.establish().expect("embedded session establish");
    serve_client(ClientInfo::default(), session.transport)
        .await
        .expect("initialize handshake completes")
}

#[tokio::test]
async fn embedded_server_lists_and_calls_builtin_without_child_process() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let server = start_embedded_mcp_server(state);
    let running = establish_session(&server).await;
    assert!(!running.is_closed(), "client alive after handshake");

    // tools/list mirrors the spine catalog: every builtin id is listed.
    let result = running.list_tools(None).await.expect("tools/list succeeds");
    let listed: Vec<String> = result.tools.iter().map(|t| t.name.to_string()).collect();
    assert!(
        listed.contains(&"nexus.context.whoami".to_string()),
        "builtin nexus.context.whoami must be listed"
    );
    assert!(
        listed.contains(&"nexus.workspace.info".to_string()),
        "builtin nexus.workspace.info must be listed"
    );

    // tools/call against a registered builtin — structured result.
    let call = running
        .call_tool(CallToolRequestParams::new("nexus.context.whoami"))
        .await
        .expect("successful call");
    assert_ne!(call.is_error, Some(true), "not an error result");
    let text = call
        .content
        .iter()
        .find_map(|c| c.as_text().map(|t| t.text.clone()))
        .expect("text content");
    assert!(
        text.contains("test_creator"),
        "whoami result names the test creator: {text}"
    );

    // No `nexus42 mcp serve` child: this fixture never spawns one — the
    // embedded path is in-process by construction (no process boundary, no
    // loopback HTTP). The stdio child path is covered by
    // `tests/mcp_serve_e2e.rs` (Model A).
    drop(running);
}

#[tokio::test]
async fn embedded_unroutable_id_is_method_not_found() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let server = start_embedded_mcp_server(state);
    let running = establish_session(&server).await;

    // Same exact-id discriminator as the stdio path fixture
    // (`call_tool_unroutable_is_method_not_found`): an id the spine cannot
    // resolve arrives as `not_supported` with NO wire code → unroutable →
    // METHOD_NOT_FOUND.
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
        "unsupported tool: → -32601 (AR-70 #4 discriminator parity)"
    );
    drop(running);
}
