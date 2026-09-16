//! Embedded MCP server (Model B): the daemon's `WorkspaceState`-backed
//! composition over the transport-neutral core shell
//! ([`nexus_core::connect::mcp_embedded`], P4-T3). The backend adapts the
//! same spine the stdio child reaches over loopback HTTP as direct function
//! calls: the catalog builder `GET /v1/daemon/tools` uses
//! ([`crate::api::handlers::tools::build_catalog`]) and the
//! `ToolExecuteRequest` dispatch path
//! ([`crate::api::handlers::host_tool_executor::HostToolExecutor`]).
//!
//! Session budget, session establishment and the rmcp lifecycle live in the
//! core shell (process-global budget, server-side slot lifetime, shutdown
//! coordination). This file is the daemon adapter only.

use std::future::Future;
use std::sync::Arc;

use nexus_contracts::generated::daemon_api::agent_host as _;
use nexus_core::connect::mcp_bridge::{
    is_unroutable, CatalogRow, McpBackend, ToolCallOutcome,
};
use nexus_core::connect::mcp_embedded::{
    start_embedded_mcp_server as core_start_embedded_mcp_server, EmbeddedMcpError,
    EmbeddedMcpServer as CoreEmbeddedMcpServer, EmbeddedShutdown,
};
use nexus_core::connect::visibility::VisibilityPolicy;
use rmcp::ErrorData as McpError;

use crate::api::errors::NexusApiError;
use crate::api::handlers::host_tool_executor::{HostToolExecutor, ToolExecuteRequest};
use crate::api::handlers::tools::build_catalog;
use crate::workspace::WorkspaceState;

/// The daemon-backed embedded MCP server: the core generic shell over the
/// spine backend. Clone is cheap; the session budget is process-global.
pub type EmbeddedMcpServer = CoreEmbeddedMcpServer<EmbeddedMcpBackend>;

pub use nexus_core::connect::mcp_embedded::{EmbeddedSession, EMBEDDED_MCP_MAX_SESSIONS};

/// The embedded spine backend: direct function calls into the same catalog
/// builder and `ToolExecuteRequest` dispatch path the HTTP routes use.
#[derive(Clone)]
pub struct EmbeddedMcpBackend {
    state: WorkspaceState,
}

impl McpBackend for EmbeddedMcpBackend {
    fn list_tools(&self) -> impl Future<Output = Result<Vec<CatalogRow>, McpError>> + Send {
        let rows = build_catalog(&self.state)
            .into_iter()
            .map(|t| CatalogRow {
                id: t.id,
                description: t.description,
                input_schema: t.input_schema,
                output_schema: t.output_schema,
            })
            .collect();
        std::future::ready(Ok(rows))
    }

    async fn call_tool(
        &self,
        tool_name: &str,
        parameters: serde_json::Value,
    ) -> Result<ToolCallOutcome, McpError> {
        // Embedded-lane audit honesty (QC S-005): session/request identifiers
        // are unavailable at this seam — the in-process sink/stream transport
        // carries no session identity and none is invented for Model B.
        let req = ToolExecuteRequest {
            tool_name: tool_name.to_owned(),
            parameters,
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        match HostToolExecutor::execute(&req, &self.state).await {
            Ok(result) => Ok(ToolCallOutcome::Success(result)),
            Err(e) => Ok(map_spine_error(e)),
        }
    }
}

/// Map one spine `NexusApiError` into the AR-70 #4 outcome vocabulary —
/// wire-parity with the stdio child's HTTP mapping.
fn map_spine_error(e: NexusApiError) -> ToolCallOutcome {
    match e {
        // Peer deny: `not_supported` + typed wire code — executed-but-failed.
        NexusApiError::PeerToolDenied {
            code,
            message,
            wire_code,
        } => ToolCallOutcome::ExecutedError {
            code,
            message,
            wire_code: Some(wire_code),
        },
        // Unroutable: `not_supported` with NO wire code (the daemon
        // `BadRequest` path never supplies one).
        NexusApiError::BadRequest { code, message } if is_unroutable(&code, None) => {
            ToolCallOutcome::Unroutable {
                code,
                message: format!("Bad request: {message}"),
            }
        }
        // Auth rejected → the spine refuses the caller (INTERNAL_ERROR
        // bounded per AR-70 #4; the message never reaches the client).
        NexusApiError::AuthRequired => ToolCallOutcome::DaemonRefused {
            message: "Authentication required".to_owned(),
        },
        // Everything else is an executed-but-failed spine outcome; the
        // public `error_code()` names the failure (same code the HTTP wire
        // carries).
        other => ToolCallOutcome::ExecutedError {
            code: other.error_code().to_owned(),
            message: other.to_string(),
            wire_code: None,
        },
    }
}

/// Start the embedded MCP server (Model B), honoring the GC #9 enablement
/// gate. Returns `None` when enablement was not requested. The shutdown
/// signal bridges `WorkspaceState::request_shutdown` onto the core shell's
/// watch-based gate: a forwarder task flips the channel once and the gate
/// stays up (a fired broadcast is state, not a permit — the same semantics
/// the Notify path reproduced with the pre-broadcast gate).
#[must_use]
pub fn start_embedded_mcp_server(
    state: WorkspaceState,
    embedded_enabled: bool,
    policy: VisibilityPolicy,
) -> Option<EmbeddedMcpServer> {
    if !embedded_enabled {
        tracing::info!(
            "embedded MCP not enabled (config key `embedded_mcp` and --embedded-mcp \
             both unset); no server created"
        );
        return None;
    }
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(state.shutdown_requested());
    let notify = state.shutdown_notify();
    tokio::spawn(async move {
        notify.notified().await;
        let _ = shutdown_tx.send(true);
    });
    let server = core_start_embedded_mcp_server(
        EmbeddedMcpBackend {
            state: state.clone(),
        },
        true,
        policy,
        EmbeddedShutdown::new(shutdown_rx),
    )?;
    tracing::info!(
        max_sessions = EMBEDDED_MCP_MAX_SESSIONS,
        "embedded MCP server ready (Model B, in-process sink/stream; exempt from \
         PeerSessionManager::max_sessions per GC #8; process-global session budget)"
    );
    Some(server)
}

/// Boot-wire the embedded MCP server (DF-88 Model B) per GC #9.
///
/// Enablement is the union of the `PeerToolsConfig.embedded_mcp` key (read
/// from `~/.nexus42/connect/daemon.json`) and the `--embedded-mcp` CLI flag.
/// When enabled, ONE boot-scoped server instance is created and stored on
/// `state` (I-1). The cargo `embedded-mcp` feature remains the hard gate.
pub async fn boot_embedded_mcp_server(
    state: &mut WorkspaceState,
    raw_home: &std::path::Path,
    cli_embedded_mcp: bool,
) {
    let (embedded_enabled, visibility) = match crate::connect::PeerToolsConfig::load(raw_home) {
        Ok(cfg) => (
            cfg.embedded_mcp || cli_embedded_mcp,
            VisibilityPolicy::from_visible(cfg.mcp_visibility),
        ),
        // V1.180 P1 (RN-OGA-2) T1-minor decision: a semantically invalid
        // `mcp_visibility` entry is a fail-closed CONSTRUCTION refusal — the
        // embedded server is not started rather than silently widening the
        // surface to all-visible.
        Err(nexus_core::connect::config::ConnectConfigError::InvalidVisibility { entry, reason }) => {
            tracing::error!(
                entry = %entry,
                reason = %reason,
                "embedded MCP refused: invalid mcp_visibility entry (fail-closed — a \
                 broken visibility config must not silently widen the surface)"
            );
            return;
        }
        Err(e) => {
            // QC W-002: warn-and-continue keeps the CLI flag's enablement
            // (GC #9 union); on a malformed config the visibility policy
            // falls back to ABSENT (all visible — byte-identical current
            // behavior).
            if cli_embedded_mcp {
                tracing::warn!(
                    error = %e,
                    "embedded MCP config load failed; continuing with embedded MCP \
                     enabled via the --embedded-mcp flag (GC #9 union)"
                );
            } else {
                tracing::warn!(
                    error = %e,
                    "embedded MCP config load failed; continuing without embedded MCP \
                     (no config key and no --embedded-mcp flag)"
                );
            }
            (cli_embedded_mcp, VisibilityPolicy::absent())
        }
    };
    if let Some(server) = start_embedded_mcp_server(state.clone(), embedded_enabled, visibility) {
        state.set_embedded_mcp_server(Arc::new(server));
        tracing::info!(
            "embedded MCP server started (Model B, in-process sink/stream; boot instance \
             stored on WorkspaceState for in-daemon consumers)"
        );
    }
}
