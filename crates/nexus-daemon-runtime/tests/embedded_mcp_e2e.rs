//! V1.179 P0 T1 (DF-88) — Model B e2e: embedded rmcp over in-process
//! sink/stream.
//!
//! Drives the embedded MCP server (`connect/mcp_embedded.rs`) with a REAL
//! in-process rmcp client over a `SinkStreamTransport` pair — the same
//! `start_embedded_mcp_server` function the daemon boot block (`boot.rs`
//! §8.5) calls, plus the boot-path wiring itself. Scope (DF-88 verify):
//! - the BOOT PATH (`connect::boot_embedded_mcp_server`, the exact
//!   function `run_daemon`'s §8.5 block calls) stores ONE boot-scoped
//!   instance on `WorkspaceState`, gated by GC #9 enablement (config key
//!   OR `--embedded-mcp` CLI flag), and that stored instance is reachable
//!   for `establish()` (I-1 / M-2);
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
//!
//! Note (M-2): a full `run_daemon` e2e (real $HOME mutation + HTTP bind +
//! shutdown coordination) is out of scope for this task because `run_daemon`
//! never exposes its internal `WorkspaceState` — the boot-path coverage
//! below drives `boot_embedded_mcp_server`, the EXACT function `run_daemon`
//! calls for the embedded server, so the enablement gate, the store-on-state
//! wiring, and establish-on-the-boot-instance are all exercised end-to-end.

#![cfg(feature = "embedded-mcp")]
// Justification (repo convention, cf. tests/works_api.rs): integration-test
// assertions operate on fixed local fixtures where a panic IS the failure
// signal; `.unwrap()`/`.expect()` keep the tests linear and readable.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use nexus_daemon_runtime::connect::mcp_embedded::{
    boot_embedded_mcp_server, start_embedded_mcp_server,
};
use nexus_daemon_runtime::connect::VisibilityPolicy;
use nexus_daemon_runtime::test_utils::create_test_workspace;
use nexus_daemon_runtime::workspace::WorkspaceState;
use rmcp::model::{CallToolRequestParams, ClientInfo, ErrorCode};
use rmcp::serve_client;
use rmcp::ServiceError;
use serial_test::serial;

/// Establish one embedded session and complete the initialize handshake.
async fn establish_session(
    server: &nexus_daemon_runtime::connect::mcp_embedded::EmbeddedMcpServer,
) -> rmcp::service::RunningService<rmcp::RoleClient, ClientInfo> {
    let session = server.establish().expect("embedded session establish");
    serve_client(ClientInfo::default(), session.transport)
        .await
        .expect("initialize handshake completes")
}

async fn boot_server(
    daemon_json: Option<&str>,
    cli_embedded_mcp: bool,
) -> (
    nexus_daemon_runtime::test_utils::TestTempRoot,
    WorkspaceState,
) {
    let (tmp, nexus_home, db_path) = create_test_workspace().await;
    let raw_home = nexus_home
        .parent()
        .expect("temp root is the raw user home")
        .to_path_buf();
    if let Some(body) = daemon_json {
        std::fs::create_dir_all(nexus_home.join("connect")).expect("connect dir");
        std::fs::write(nexus_home.join("connect").join("daemon.json"), body)
            .expect("write daemon.json");
    }
    let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    boot_embedded_mcp_server(&mut state, &raw_home, cli_embedded_mcp).await;
    (tmp, state)
}

// `#[serial]`: each test below holds a LIVE embedded session (the budget
// slot lives in the session's server task, QC F-001), so the
// process-global registry is touched by one test at a time.
#[tokio::test]
#[serial]
async fn embedded_server_lists_and_calls_builtin_without_child_process() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let server = start_embedded_mcp_server(state, true, VisibilityPolicy::absent()).expect("enabled server");
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
#[serial]
async fn embedded_unroutable_id_is_method_not_found() {
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let server = start_embedded_mcp_server(state, true, VisibilityPolicy::absent()).expect("enabled server");
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

#[tokio::test]
#[serial]
async fn boot_path_config_key_stores_boot_server_reachable_for_establish() {
    // I-1/M-2: the BOOT WIRING (the exact function `run_daemon`'s §8.5
    // block calls) must retain ONE boot-scoped server instance on
    // `WorkspaceState` — reachable for `establish()` — gated by the GC #9
    // config-key SSOT (`~/.nexus42/connect/daemon.json` `"embedded_mcp":
    // true`; serde default false). CLI flag off: the key alone enables.
    let (_tmp, state) = boot_server(Some(r#"{"embedded_mcp": true}"#), false).await;

    let server = state
        .embedded_mcp_server()
        .expect("boot instance stored on WorkspaceState");
    let running = establish_session(&server).await;
    assert!(!running.is_closed(), "client alive after handshake");

    // The stored boot instance serves the spine catalog end-to-end.
    let result = running.list_tools(None).await.expect("tools/list succeeds");
    assert!(
        result
            .tools
            .iter()
            .any(|t| t.name.as_ref() == "nexus.context.whoami"),
        "boot-path server lists builtin tools"
    );
    let call = running
        .call_tool(CallToolRequestParams::new("nexus.context.whoami"))
        .await
        .expect("successful call");
    assert_ne!(call.is_error, Some(true), "boot-path call succeeds");
    drop(running);
}

#[tokio::test]
async fn boot_path_cli_flag_enables_and_no_enablement_leaves_no_server() {
    // CLI-flag path: no config file, `--embedded-mcp` alone enables (union
    // semantics, GC #9).
    let (_tmp, state) = boot_server(None, true).await;
    assert!(
        state.embedded_mcp_server().is_some(),
        "CLI flag alone enables the boot-scoped server"
    );

    // Neither key nor flag: no server is stored, no embedded surface.
    let (_tmp2, state2) = boot_server(None, false).await;
    assert!(
        state2.embedded_mcp_server().is_none(),
        "no enablement ⇒ no boot server stored"
    );
}

#[tokio::test]
#[serial]
async fn embedded_visibility_policy_filters_list_and_short_circuits_hidden_call() {
    // V1.180 P1 (RN-OGA-2): Model B respects the daemon-side visibility
    // policy — `tools/list` is filtered to the configured subset and a
    // hidden-tool `tools/call` is refused at the seam (never dispatched),
    // while a visible tool still dispatches through the spine.
    let (_tmp, nexus_home, db_path) = create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let server = start_embedded_mcp_server(
        state,
        true,
        VisibilityPolicy::from_visible(["nexus.context.whoami".to_owned()]),
    )
    .expect("enabled server");
    let running = establish_session(&server).await;

    // tools/list is filtered to the visible subset.
    let result = running.list_tools(None).await.expect("tools/list succeeds");
    let listed: Vec<String> = result.tools.iter().map(|t| t.name.to_string()).collect();
    assert_eq!(
        listed,
        vec!["nexus.context.whoami".to_owned()],
        "only the visible tool is listed"
    );

    // A visible tool still dispatches through the spine.
    let call = running
        .call_tool(CallToolRequestParams::new("nexus.context.whoami"))
        .await
        .expect("visible tool call succeeds");
    assert_ne!(call.is_error, Some(true), "visible call not an error");

    // A hidden tool is refused at the seam with the `hidden_tool`
    // discriminator (METHOD_NOT_FOUND), never dispatched.
    let err = running
        .call_tool(CallToolRequestParams::new("nexus.workspace.info"))
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
        data.message.contains("hidden_tool"),
        "refusal names the visibility class: {}",
        data.message
    );
    drop(running);
}
