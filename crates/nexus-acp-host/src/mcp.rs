//! First-class nexus MCP server wiring for ACP sessions.
//!
//! V1.174 P1 T1 (AR-75 C-1 → AC close): when the `connect-client` feature
//! is enabled, session construction can attach the nexus MCP stdio server
//! (the `nexus42 mcp serve` bridge child, AR-71 Model A) to
//! `newSession.mcp_servers` so the hosted ACP agent's own MCP client sees
//! the daemon's full-registry catalog — builtin `nexus.*` + user
//! capabilities + admitted peer tools (PL-5).
//!
//! Compiled only with `connect-client`: without the feature the descriptor
//! does not exist (compile-time absence, T1 `DoD`), and the default graph
//! is unchanged (no new dependencies — `agent-client-protocol` is already
//! in the shipped graph).

use std::path::PathBuf;

use agent_client_protocol::schema::{McpServer, McpServerStdio};

/// Session-facing name of the nexus MCP server (`newSession.mcp_servers`).
pub const NEXUS_MCP_SERVER_NAME: &str = "nexus";

/// Build the first-class stdio descriptor for the nexus MCP server.
///
/// Returns [`McpServer::Stdio`] carrying `nexus42 mcp serve` so the agent
/// binary (the MCP client, per AR-71 Model A) spawns `nexus42 mcp serve`
/// as its own stdio child and lists/calls the daemon's full-registry
/// catalog through its own MCP client.
///
/// `nexus42_bin` is the config/constructor argument: callers pass the
/// resolved path to the `nexus42` executable (integration tests use
/// `CARGO_BIN_EXE_nexus42`; the hosted session service passes the
/// operator-resolved binary).
#[must_use]
pub fn nexus_mcp_stdio_server(nexus42_bin: impl Into<PathBuf>) -> McpServer {
    McpServer::Stdio(
        McpServerStdio::new(NEXUS_MCP_SERVER_NAME, nexus42_bin)
            .args(vec!["mcp".to_string(), "serve".to_string()]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_session_stdio_descriptor_carries_nexus_mcp_serve() {
        let server = nexus_mcp_stdio_server("/usr/bin/nexus42");
        let McpServer::Stdio(stdio) = server else {
            panic!("expected McpServer::Stdio");
        };
        assert_eq!(stdio.name, NEXUS_MCP_SERVER_NAME);
        assert_eq!(stdio.command, PathBuf::from("/usr/bin/nexus42"));
        assert_eq!(
            stdio.args,
            vec!["mcp".to_string(), "serve".to_string()]
        );
        assert!(stdio.env.is_empty(), "no env carried by the surface");
        assert!(stdio.meta.is_none());
    }
}
