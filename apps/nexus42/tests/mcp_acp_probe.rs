//! Generic ACP `newSession.mcp_servers` descriptor protocol probe.
//!
//! v1.193 P1-T6 removed the Model A nexus42 MCP stdio child
//! (`nexus42 mcp serve`) and its `nexus-acp-host::mcp` descriptor factory;
//! what remains is the generic ACP surface — a client may attach arbitrary
//! `mcp_servers` descriptors (`Stdio` / `Http` / `Sse`) to `newSession` and
//! the agent receives them verbatim across the protocol. This probe pins
//! that round trip so cutting Model A cannot silently drop the generic
//! descriptor path. The nexus DTO → SDK mapping itself stays pinned by the
//! `nexus-acp-host::client` unit tests
//! (`new_session_request_propagates_mcp_servers`,
//! `new_session_request_empty_mcp_servers`).
//!
//! Both ACP roles run in-process over the SDK's `Channel::duplex()`; the SDK
//! owns the JSON-RPC framing.

// Justification (repo convention, cf. tests/works_api.rs): integration-test
// assertions operate on fixed local fixtures where a panic IS the failure
// signal; `.unwrap()`/`.expect()` keep the tests linear and readable.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use agent_client_protocol::schema::v1::{
    AgentCapabilities, EnvVariable, InitializeRequest, InitializeResponse, McpServer,
    McpServerHttp, McpServerSse, McpServerStdio, NewSessionRequest, NewSessionResponse, SessionId,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{Agent, Channel, Client};

/// The three generic descriptor kinds a client may attach to `newSession`.
fn generic_descriptors() -> Vec<McpServer> {
    vec![
        McpServer::Stdio(
            McpServerStdio::new("probe-stdio", PathBuf::from("/usr/local/bin/mcp-child"))
                .args(vec!["--stdio".to_string(), "serve".to_string()])
                .env(vec![EnvVariable::new("PROBE_MARKER", "carried")]),
        ),
        McpServer::Http(McpServerHttp::new("probe-http", "https://mcp.example/mcp")),
        McpServer::Sse(McpServerSse::new("probe-sse", "https://mcp.example/sse")),
    ]
}

/// `(name, command, args, env pairs)` lifted off a `Stdio` descriptor.
type StdioEvidence = (String, PathBuf, Vec<String>, Vec<(String, String)>);

/// Descriptors observed by the scripted agent's `session/new` handler.
#[derive(Debug)]
struct DescriptorEvidence {
    count: usize,
    stdio: Option<StdioEvidence>,
    http: Option<(String, String)>,
    sse: Option<(String, String)>,
}

/// The scripted agent: records the `mcp_servers` it received and replies
/// with the session id. It never spawns a child — the generic ACP contract is
/// descriptor delivery, not a prescribed nexus42 process.
async fn run_scripted_agent(
    agent_channel: Channel,
    evidence_tx: tokio::sync::mpsc::UnboundedSender<DescriptorEvidence>,
) -> agent_client_protocol::Result<()> {
    Agent
        .builder()
        .name("descriptor-probe-agent")
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
                let mut evidence = DescriptorEvidence {
                    count: request.mcp_servers.len(),
                    stdio: None,
                    http: None,
                    sse: None,
                };
                for server in request.mcp_servers {
                    match server {
                        McpServer::Stdio(s) => {
                            evidence.stdio = Some((
                                s.name,
                                s.command,
                                s.args,
                                s.env.into_iter().map(|e| (e.name, e.value)).collect(),
                            ));
                        }
                        McpServer::Http(h) => evidence.http = Some((h.name, h.url)),
                        McpServer::Sse(s) => evidence.sse = Some((s.name, s.url)),
                        _ => {}
                    }
                }
                let _ = evidence_tx.send(evidence);
                responder.respond(NewSessionResponse::new(SessionId::new("probe-session")))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_to(agent_channel)
        .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn acp_session_delivers_generic_mcp_server_descriptors() {
    let (agent_channel, client_channel) = Channel::duplex();
    let (evidence_tx, mut evidence_rx) = tokio::sync::mpsc::unbounded_channel();

    let agent_task = tokio::spawn(run_scripted_agent(agent_channel, evidence_tx));

    // Client side: initialize, then newSession carrying all three generic
    // descriptor kinds.
    let client_result = Client
        .builder()
        .name("descriptor-probe-client")
        .connect_with(client_channel, async move |connection| {
            connection
                .send_request(InitializeRequest::new(ProtocolVersion::V1))
                .block_task()
                .await?;

            let session_req = NewSessionRequest::new("/tmp").mcp_servers(generic_descriptors());
            let resp = connection.send_request(session_req).block_task().await?;
            assert_eq!(resp.session_id.to_string(), "probe-session");
            Ok(())
        })
        .await;
    assert!(client_result.is_ok(), "client result: {client_result:?}");

    let evidence = tokio::time::timeout(std::time::Duration::from_secs(30), evidence_rx.recv())
        .await
        .expect("descriptor evidence arrives within 30s")
        .expect("agent sent the received descriptors");
    agent_task.abort();

    assert_eq!(evidence.count, 3, "all three descriptors were delivered");

    let (name, command, args, env) = evidence.stdio.expect("stdio descriptor delivered");
    assert_eq!(name, "probe-stdio", "stdio name preserved");
    assert_eq!(command, PathBuf::from("/usr/local/bin/mcp-child"));
    assert_eq!(args, vec!["--stdio".to_string(), "serve".to_string()]);
    assert!(
        env.iter()
            .any(|(k, v)| k == "PROBE_MARKER" && v == "carried"),
        "stdio env carried verbatim: {env:?}"
    );

    let (name, url) = evidence.http.expect("http descriptor delivered");
    assert_eq!(
        (name.as_str(), url.as_str()),
        ("probe-http", "https://mcp.example/mcp")
    );

    let (name, url) = evidence.sse.expect("sse descriptor delivered");
    assert_eq!(
        (name.as_str(), url.as_str()),
        ("probe-sse", "https://mcp.example/sse")
    );
}
