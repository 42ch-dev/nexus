//! V1.174 P1 T0 (AR-75 C-1) — ACP stdio-injection consumption probe.
//!
//! STOP-gate reproduction: a scripted (no-live-LLM) ACP **agent** session
//! created with `newSession.mcp_servers` carrying
//! `McpServer::Stdio { name, command, args, env }` — the exact SDK request
//! shape `nexus-acp-host::client::sdk_new_session_request_from_nexus` emits
//! for `NexusMcpServer::Stdio` (pinned by that crate's unit test
//! `new_session_request_propagates_mcp_servers`). The agent-side session
//! handler acts as its own MCP client: it spawns the stdio child
//! (`nexus42 mcp serve`, real subprocess, hermetic `$HOME` pointing at a
//! stub daemon via wiremock), completes MCP initialize, `tools/list`
//! (full-registry catalog incl. an admitted peer tool), and `tools/call`
//! of the peer tool with the schema-matching result.
//!
//! Both ACP roles run in-process over the SDK's `Channel::duplex()` — the
//! SDK owns the JSON-RPC framing; the process boundary lives in the
//! spawned MCP child (which is the point of the gate: P0's raw rmcp client
//! alone does not close AC-V174-1).
//!
//! Gated: `--features connect-client` (same gate as `mcp_serve_e2e.rs`).

#![cfg(feature = "connect-client")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::process::Stdio;

use agent_client_protocol::schema::{
    AgentCapabilities, EnvVariable, InitializeRequest, InitializeResponse, McpServer,
    McpServerStdio, NewSessionRequest, NewSessionResponse, ProtocolVersion, SessionId,
};
use agent_client_protocol::{Agent, Channel, Client};
use rmcp::model::{CallToolRequestParams, ClientInfo};
use rmcp::serve_client;
use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Evidence collected by the scripted agent's `session/new` handler and
/// returned to the test via the evidence channel.
#[derive(Debug)]
struct McpProbeEvidence {
    /// Catalog ids returned by the child's `tools/list`, sorted.
    listed_tools: Vec<String>,
    /// `structured_content` returned by `tools/call` of the peer tool.
    call_structured: serde_json::Value,
    /// The stdio command/name the agent actually spawned (proves the
    /// `McpServerStdio { name, command }` reached the spawn site).
    spawned_name: String,
    spawned_command: PathBuf,
}

/// Spawn the stub daemon (wiremock) serving the P0 catalog shape.
async fn mount_catalog(mock: &MockServer) {
    let body = json!({
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
    });
    Mock::given(method("GET"))
        .and(path("/v1/daemon/tools"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(mock)
        .await;
}

/// Mount the daemon tool-execution response the peer tool call hits.
async fn mount_execution(mock: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/v1/daemon/agent-host/internal/tool-executions"))
        .and(body_partial_json(json!({ "tool_name": "tools.t6.echo" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "result": { "echo": { "k": "v" } }
        })))
        .mount(mock)
        .await;
}

/// The scripted agent: handles `session/new` by spawning the stdio child
/// named in `mcp_servers[0]`, running the MCP lifecycle, and replying with
/// the session id once evidence is collected.
async fn run_scripted_agent(
    agent_channel: Channel,
    mock: MockServer,
    evidence_tx: tokio::sync::mpsc::UnboundedSender<McpProbeEvidence>,
) -> agent_client_protocol::Result<()> {
    Agent
        .builder()
        .name("c1-scripted-agent")
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
                // The gate's contract: mcp_servers carried the Stdio server
                // with name/command/args/env intact.
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
                assert!(
                    stdio.args.contains(&"mcp".to_string())
                        && stdio.args.contains(&"serve".to_string()),
                    "args = [mcp, serve], got {:?}",
                    stdio.args
                );
                assert!(
                    stdio
                        .env
                        .iter()
                        .any(|e| e.name == "NEXUS_PROBE_MARKER" && e.value == "carried"),
                    "env carried through McpServerStdio.env"
                );

                // Build a hermetic $HOME for the child so the spawned
                // `nexus42 mcp serve` reads the stub daemon config.
                let home = tempfile::tempdir().expect("temp home");
                let nexus_dir = home.path().join(".nexus42");
                std::fs::create_dir_all(&nexus_dir).expect("nexus dir");
                std::fs::write(
                    nexus_dir.join("config.toml"),
                    format!("daemon_url = \"{}\"\n", mock.uri()),
                )
                .expect("config.toml");

                // The agent's own MCP client spawns the child: apply the
                // env carried by McpServerStdio.env, then override HOME
                // with the hermetic sandbox (deterministic config).
                let mut spawn = tokio::process::Command::new(&stdio.command);
                spawn
                    .args(&stdio.args)
                    .envs(stdio.env.iter().map(|e| (&e.name, &e.value)))
                    .env("HOME", home.path())
                    .env("RUST_LOG", "off")
                    // S-d (QC3 S-3): NO_PROXY aligns with the e2e child —
                    // keeps the child's loopback daemon connection direct.
                    .env("NO_PROXY", "127.0.0.1")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null());
                let mut child = spawn.spawn().expect("spawn nexus42 mcp serve");

                let transport = (
                    child.stdout.take().expect("child stdout"),
                    child.stdin.take().expect("child stdin"),
                );
                let running = serve_client(ClientInfo::default(), transport)
                    .await
                    .expect("MCP initialize handshake completes");

                // tools/list — full-registry catalog.
                let list = running.list_tools(None).await.expect("tools/list succeeds");
                let mut listed: Vec<String> =
                    list.tools.iter().map(|t| t.name.to_string()).collect();
                listed.sort();

                // tools/call of the admitted peer tool.
                let call = running
                    .call_tool(CallToolRequestParams::new("tools.t6.echo"))
                    .await
                    .expect("tools/call succeeds");
                assert_ne!(call.is_error, Some(true), "peer call not an error");
                let call_structured = call.structured_content.expect("structured result present");

                let _ = evidence_tx.send(McpProbeEvidence {
                    listed_tools: listed.clone(),
                    call_structured,
                    spawned_name: stdio.name.clone(),
                    spawned_command: stdio.command.clone(),
                });

                drop(running);
                let _ = child.kill().await;

                responder.respond(NewSessionResponse::new(SessionId::new("c1-probe-session")))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_to(agent_channel)
        .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn c1_acp_stdio_injection_spawns_child_and_calls_peer_tool() {
    let mock = MockServer::start().await;
    mount_catalog(&mock).await;
    mount_execution(&mock).await;

    let (agent_channel, client_channel) = Channel::duplex();
    let (evidence_tx, mut evidence_rx) = tokio::sync::mpsc::unbounded_channel();

    let agent_task = tokio::spawn(run_scripted_agent(agent_channel, mock, evidence_tx));

    // Client side: initialize, then newSession with mcp_servers = Stdio.
    // Env carries a marker the agent asserts reached the spawn site, plus
    // a HOME the agent overrides with its hermetic sandbox.
    let client_result = Client
        .builder()
        .name("c1-scripted-client")
        .connect_with(client_channel, async move |connection| {
            connection
                .send_request(InitializeRequest::new(ProtocolVersion::V1))
                .block_task()
                .await?;

            let session_req = NewSessionRequest::new("/tmp").mcp_servers(vec![McpServer::Stdio(
                McpServerStdio::new("nexus", PathBuf::from(env!("CARGO_BIN_EXE_nexus42")))
                    .args(vec!["mcp".to_string(), "serve".to_string()])
                    .env(vec![
                        EnvVariable::new("NEXUS_PROBE_MARKER", "carried"),
                        EnvVariable::new("HOME", "set-by-client"),
                    ]),
            )]);

            let resp = connection.send_request(session_req).block_task().await?;
            assert_eq!(resp.session_id.to_string(), "c1-probe-session");
            Ok(())
        })
        .await;

    assert!(client_result.is_ok(), "client result: {client_result:?}");

    let evidence = tokio::time::timeout(std::time::Duration::from_secs(60), evidence_rx.recv())
        .await
        .expect("evidence arrives within 60s")
        .expect("agent sent evidence");

    agent_task.abort();

    // ── Assertions on the probe evidence ──────────────────────────────────
    let mut expected = vec![
        "nexus.workspace.info".to_string(),
        "t6.wcap".to_string(),
        "tools.t6.echo".to_string(),
    ];
    expected.sort();
    assert_eq!(
        evidence.listed_tools, expected,
        "tools/list == full-registry catalog (incl. admitted peer tool)"
    );
    assert_eq!(
        evidence.call_structured,
        json!({ "echo": { "k": "v" } }),
        "peer tool call returns the schema-matching result"
    );
    assert_eq!(evidence.spawned_name, "nexus");
    assert!(
        evidence.spawned_command.ends_with("nexus42"),
        "spawned command is the nexus42 binary: {:?}",
        evidence.spawned_command
    );
}
