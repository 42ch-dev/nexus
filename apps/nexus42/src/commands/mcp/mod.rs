//! MCP server bridge — `nexus42 mcp serve` (V1.174 P0 T5, AR-70/71/72).
//!
//! A tools-only MCP server over stdio. The MCP client (e.g. a desktop app
//! or agent host) spawns `nexus42 mcp serve` as its OWN child (AR-71
//! Model A): the child is STATELESS — no registry, no allowlist, no policy,
//! no cache. Every `tools/list` is a live `GET /v1/daemon/tools` and every
//! `tools/call` is a live `POST /v1/daemon/agent-host/internal/tool-executions`
//! over daemon loopback HTTP (reusing the CLI daemon-client resolution
//! exactly — config `daemon_url`, default `http://127.0.0.1:8420`).
//!
//! Error mapping (AR-70 #4):
//! - Unroutable (never-admitted / evicted / allowlist-missing /
//!   non-exposable id) → `Err(ErrorData)` `METHOD_NOT_FOUND` naming the
//!   refusal class.
//! - Daemon unreachable / auth rejected → `Err(ErrorData)` `INTERNAL_ERROR`
//!   bounded by the daemon client's connect/request timeouts (never hangs).
//! - Executed-but-failed (invalid args, peer deny with its wire code,
//!   invoke timeout, transport closed, user-cap `run()` error) →
//!   `Ok(CallToolResult::error(...))` naming the spine code.
//! - Success → text content + `structured_content` when the spine result is
//!   a JSON object (matching the advertised output schema), else text-only.
//!
//! Boundary (AR-72): the handler implements ONLY `get_info` / `list_tools` /
//! `call_tool`. `prompts/list` and `resources/list` keep the rmcp defaults
//! (empty lists — SDK reality, documented in the T5 report) and
//! `prompts/get` / `resources/read` return `METHOD_NOT_FOUND`; no other
//! handler overrides exist beyond the tools family + server info.

use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, ErrorCode, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::transport::stdio;
use rmcp::{serve_server, ErrorData as McpError, RoleServer, ServerHandler};

use crate::api::daemon_client::DaemonClient;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};

/// `nexus42 mcp` subcommands (hidden group; V1.35 CLI surface lock).
#[derive(Debug, clap::Subcommand)]
pub enum McpCommand {
    /// Run the MCP stdio bridge server (AR-71 Model A — stateless child).
    Serve,
}

/// Run an `mcp` subcommand.
///
/// # Errors
///
/// Returns the CLI error surfaced by the subcommand.
pub async fn run(command: McpCommand, config: &CliConfig) -> Result<()> {
    match command {
        McpCommand::Serve => serve(config).await,
    }
}

/// Serve the MCP stdio bridge until the client closes the transport.
///
/// The child never returns until the transport ends: `serve_server`
/// completes the initialize handshake and then the background loop runs
/// until stdin EOF / stdout close, at which point `waiting()` resolves and
/// the process exits 0.
///
/// # Errors
///
/// Returns a server-init error when the rmcp handshake cannot complete, or
/// a join error if the service loop panics.
async fn serve(config: &CliConfig) -> Result<()> {
    let client = DaemonClient::from_config(config);
    let handler = McpBridgeHandler { client };

    let service = serve_server(handler, stdio())
        .await
        .map_err(|e| CliError::Other(format!("mcp server init failed: {e}")))?;
    service
        .waiting()
        .await
        .map_err(|e| CliError::Other(format!("mcp server failed: {e}")))?;
    Ok(())
}

/// The stateless MCP bridge handler (AR-70).
struct McpBridgeHandler {
    client: DaemonClient,
}

/// One catalog row from `GET /v1/daemon/tools` (wire shape mirrors
/// `catalog-tool.schema.json`).
#[derive(Debug, serde::Deserialize)]
struct CatalogRow {
    id: String,
    description: String,
    input_schema: String,
    #[serde(default)]
    output_schema: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct CatalogResponse {
    items: Vec<CatalogRow>,
}

/// Structured outcome of one spine tool execution, preserving the daemon's
/// wire code for the protocol mapping (AR-70 #4).
enum ToolCallOutcome {
    /// HTTP 200 `{ success: true, result: <value> }`.
    Success(serde_json::Value),
    /// Executed-but-failed: spine code preserved (`invalid_input`,
    /// `not_supported` + peer wire code, `service_unavailable`, ...).
    ExecutedError {
        code: String,
        message: String,
        wire_code: Option<String>,
    },
    /// Unroutable: never-admitted / evicted / allowlist-missing id.
    Unroutable { code: String, message: String },
    /// The daemon refuses the caller itself (auth rejected) — an opaque
    /// `INTERNAL_ERROR` per AR-70 #4, the message never reaches the client.
    DaemonRefused { message: String },
}

impl ServerHandler for McpBridgeHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("nexus42", env!("CARGO_PKG_VERSION")))
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, McpError> {
        let catalog: CatalogResponse = self.client.get("/v1/daemon/tools").await.map_err(|e| {
            McpError::internal_error(format!("daemon tools list failed: {e}"), None)
        })?;
        let tools: Vec<Tool> = catalog.items.into_iter().map(row_into_tool).collect();
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, McpError> {
        let name = request.name.to_string();
        let parameters = serde_json::Value::Object(request.arguments.unwrap_or_default());

        let outcome = self
            .call_tool_inner(&name, parameters)
            .await
            .map_err(|e| McpError::internal_error(format!("daemon tool call failed: {e}"), None))?;

        match outcome {
            ToolCallOutcome::Success(result) => Ok(success_result(result)),
            ToolCallOutcome::ExecutedError {
                code,
                message,
                wire_code,
            } => {
                // AR-70 #4: the daemon threads the peer wire code verbatim in
                // `details.wire_code` (lowercase, e.g. `op_unsupported`);
                // surface it exactly once, ahead of the message. Other
                // executed-but-failed outcomes name the spine `code`.
                Ok(CallToolResult::error(vec![Content::text(
                    executed_error_text(&code, &message, wire_code.as_deref()),
                )]))
            }
            ToolCallOutcome::Unroutable { code, message } => Err(McpError::new(
                ErrorCode::METHOD_NOT_FOUND,
                format!("unroutable: {code}: {message}"),
                None,
            )),
            ToolCallOutcome::DaemonRefused { message } => Err(McpError::internal_error(
                format!("daemon refused tool call: {message}"),
                None,
            )),
        }
    }
}

impl McpBridgeHandler {
    /// Call the daemon spine, mapping the wire outcome into the MCP result
    /// vocabulary. The daemon distinguishes:
    /// - unroutable: `not_supported` + `"unsupported tool: {id}"`;
    /// - peer deny: `not_supported` + `details.wire_code` (lowercase,
    ///   e.g. `op_unsupported`) + message;
    /// - executed failure: `invalid_input` / `forbidden` /
    ///   `policy_blocked` / `service_unavailable` / `internal`;
    /// - auth rejected: `auth_required`.
    async fn call_tool_inner(
        &self,
        tool_name: &str,
        parameters: serde_json::Value,
    ) -> std::result::Result<ToolCallOutcome, CliError> {
        let wire = self
            .client
            .post_execution_raw(tool_name, parameters)
            .await?;
        let error = wire
            .body
            .get("error")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let code = error
            .get("code")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("internal")
            .to_owned();
        let message = error
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown tool execution error")
            .to_owned();
        // AR-70 #4: the daemon carries the spoke wire code (lowercase,
        // e.g. `op_unsupported`) in `error.details.wire_code` — read it
        // typed, never re-parse it from the message.
        let wire_code = error
            .get("details")
            .and_then(|d| d.get("wire_code"))
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned);

        if wire.status == 200
            && wire
                .body
                .get("success")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
        {
            return Ok(ToolCallOutcome::Success(
                wire.body
                    .get("result")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            ));
        }

        match code.as_str() {
            // Unroutable: the spine cannot resolve the id at all.
            "not_supported" if message.starts_with("unsupported tool:") => {
                Ok(ToolCallOutcome::Unroutable { code, message })
            }
            // Auth rejected → the daemon refuses the caller (INTERNAL_ERROR
            // bounded per AR-70 #4; the message never reaches the client).
            "auth_required" => Ok(ToolCallOutcome::DaemonRefused { message }),
            // Everything else is an executed-but-failed spine outcome;
            // the typed peer wire code rides along when the daemon
            // supplied one.
            _ => Ok(ToolCallOutcome::ExecutedError {
                code,
                message,
                wire_code,
            }),
        }
    }
}

/// Map one daemon catalog row to an rmcp `Tool` (AR-70 §3 schema mapping):
/// `input_schema` carried verbatim; `output_schema` carried only when
/// present (the catalog already omits non-root-object outputs).
fn row_into_tool(row: CatalogRow) -> Tool {
    let input = parse_schema(&row.input_schema).unwrap_or_else(default_object_schema);
    let mut tool = Tool::new(row.id, row.description, Arc::new(input));
    if let Some(out) = row.output_schema {
        if let Some(map) = parse_schema(&out) {
            tool = tool.with_raw_output_schema(Arc::new(map));
        }
    }
    tool
}

/// The documented permissive fallback input schema (AR-70 §3 placeholder).
fn default_object_schema() -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    map.insert("type".into(), serde_json::Value::String("object".into()));
    map
}

/// Parse a JSON-Schema string into an object, or `None` on failure.
fn parse_schema(raw: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|v| v.as_object().cloned())
}

/// Build the success result: `structured_content` when the spine result is
/// a JSON object (matches the advertised output schema), text-only else.
fn success_result(value: serde_json::Value) -> CallToolResult {
    if value.is_object() {
        CallToolResult::structured(value)
    } else {
        let text = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_owned());
        CallToolResult::success(vec![Content::text(text)])
    }
}

/// Format the executed-but-failed content (AR-70 #4): the typed peer wire
/// code (`details.wire_code`, lowercase e.g. `op_unsupported`) takes
/// precedence when present; otherwise the spine `code` names the failure.
fn executed_error_text(code: &str, message: &str, wire_code: Option<&str>) -> String {
    let effective = wire_code.unwrap_or(code);
    format!("{effective}: {message}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_into_tool_carries_schemas_verbatim() {
        let row = CatalogRow {
            id: "tools.t5.echo".to_owned(),
            description: "echo".to_owned(),
            input_schema: r#"{"type":"object","required":["x"]}"#.to_owned(),
            output_schema: Some(r#"{"type":"object","properties":{"echo":{}}}"#.to_owned()),
        };
        let tool = row_into_tool(row);
        assert_eq!(tool.name, "tools.t5.echo");
        assert_eq!(
            tool.input_schema.get("required"),
            Some(&serde_json::json!(["x"]))
        );
        let out = tool.output_schema.expect("output schema carried");
        assert_eq!(
            out.get("properties").and_then(|p| p.get("echo")),
            Some(&serde_json::Value::Object(serde_json::Map::new()))
        );
    }

    #[test]
    fn unparseable_input_schema_falls_back_to_permissive_object() {
        let row = CatalogRow {
            id: "nexus.workspace.info".to_owned(),
            description: "info".to_owned(),
            input_schema: "not-json".to_owned(),
            output_schema: None,
        };
        let tool = row_into_tool(row);
        assert_eq!(
            tool.input_schema.get("type"),
            Some(&serde_json::Value::String("object".into()))
        );
        assert!(tool.output_schema.is_none());
    }

    #[test]
    fn executed_error_text_surfaces_typed_wire_code_exactly_once() {
        // AR-70 #4 fidelity: the daemon threads the ORIGINAL lowercase spoke
        // wire code (`details.wire_code`, e.g. `op_unsupported`) — it must
        // appear exactly once in the caller-visible content, and the message
        // must not duplicate any uppercase re-derivation.
        let text = executed_error_text(
            "not_supported",
            "tool tools.t4.ghost is not supported by this peer",
            Some("op_unsupported"),
        );
        assert_eq!(
            text,
            "op_unsupported: tool tools.t4.ghost is not supported by this peer"
        );
        assert_eq!(text.matches("op_unsupported").count(), 1);
        assert!(
            !text.contains("OP_UNSUPPORTED"),
            "uppercase re-derivation must be gone: {text}"
        );
    }

    #[test]
    fn executed_error_text_falls_back_to_spine_code_without_wire_code() {
        let text = executed_error_text("invalid_input", "missing required argument", None);
        assert_eq!(text, "invalid_input: missing required argument");
    }

    #[test]
    fn success_result_uses_structured_content_for_objects() {
        let result = success_result(serde_json::json!({"ok": true}));
        assert_eq!(result.is_error, Some(false));
        assert_eq!(
            result.structured_content,
            Some(serde_json::json!({ "ok": true }))
        );
        let scalar = success_result(serde_json::json!("hello"));
        assert!(scalar.structured_content.is_none());
        assert_eq!(scalar.content.len(), 1);
    }
}
