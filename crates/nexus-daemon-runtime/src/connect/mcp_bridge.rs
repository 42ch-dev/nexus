//! Shared MCP bridge core (V1.179 P0 T1, DF-88) — the rmcp `ServerHandler`
//! surface extracted from `apps/nexus42/src/commands/mcp/mod.rs` (V1.174 P0
//! T5, AR-70/71/72), generic over an [`McpBackend`].
//!
//! Two concrete backends exist:
//! - **Model A** (stdio child, `apps/nexus42`): a `DaemonClient`-loopback-HTTP
//!   adapter — every `tools/list` is a live `GET /v1/daemon/tools` and every
//!   `tools/call` is a live `POST …/tool-executions` (AR-71).
//! - **Model B** (embedded, `connect/mcp_embedded.rs`): an in-process adapter
//!   over the same spine as direct function calls (DF-88).
//!
//! This module owns the catalog-row→`Tool` mapping, the permissive fallback
//! schema, and the full AR-70 #4 refusal vocabulary (the [`ToolCallOutcome`]
//! classification + the MCP result mapping). The extraction is justified by
//! the 80-line duplication cap: the shareable surface (`ServerHandler` impl +
//! call/list logic + mapping + error vocabulary) exceeds it; `nexus-contracts`
//! is REJECTED as the home — it is the schema SSOT and must stay
//! transport-free (rmcp server types must not enter the contracts graph).
//!
//! Error mapping (AR-70 #4):
//! - Unroutable (never-admitted / evicted / allowlist-missing /
//!   non-exposable id) → `Err(ErrorData)` `METHOD_NOT_FOUND` naming the
//!   refusal class.
//! - Backend transport failure (daemon unreachable / auth rejected) →
//!   `Err(ErrorData)` `INTERNAL_ERROR` — the backend maps its own errors;
//!   the message never reaches the client.
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

use std::future::Future;
use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, ErrorCode, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};

/// One catalog row from the spine catalog (wire shape mirrors
/// `catalog-tool.schema.json`).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CatalogRow {
    pub id: String,
    pub description: String,
    pub input_schema: String,
    #[serde(default)]
    pub output_schema: Option<String>,
}

/// `{ "items": [...] }` wrapper of the spine catalog route.
#[derive(Debug, serde::Deserialize)]
pub struct CatalogResponse {
    pub items: Vec<CatalogRow>,
}

/// Structured outcome of one spine tool execution, preserving the daemon's
/// wire code for the protocol mapping (AR-70 #4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCallOutcome {
    /// Spine success `{ success: true, result: <value> }`.
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
    /// The spine refuses the caller itself (auth rejected) — an opaque
    /// `INTERNAL_ERROR` per AR-70 #4, the message never reaches the client.
    DaemonRefused { message: String },
}

/// The spine face a concrete MCP backend adapts (AR-71 Model A loopback
/// HTTP; DF-88 Model B in-process function calls).
///
/// The backend is responsible for resolving the live catalog and executing
/// one tool through the spine, mapping the outcome into the
/// [`ToolCallOutcome`] vocabulary. Transport-level failures (daemon
/// unreachable, auth rejected) are returned as `McpError` — the handler
/// surfaces them as bounded `INTERNAL_ERROR`s.
///
/// `advertise_tool_list_changed` reports whether the backend runs the AR-79
/// catalog-watch loop and can deliver `notifications/tools/list_changed`.
/// Model A (stdio child) overrides to `true`; the embedded server has no
/// watcher (Model-A-only, AR-79 #5) and must not advertise a capability it
/// never delivers.
pub trait McpBackend: Send + Sync + 'static {
    /// Resolve the live catalog rows (same shape as `GET /v1/daemon/tools`).
    fn list_tools(&self) -> impl Future<Output = Result<Vec<CatalogRow>, McpError>> + Send;

    /// Execute one tool through the spine, mapping the outcome into the
    /// AR-70 #4 vocabulary.
    fn call_tool(
        &self,
        tool_name: &str,
        parameters: serde_json::Value,
    ) -> impl Future<Output = Result<ToolCallOutcome, McpError>> + Send;

    /// Whether this backend delivers `notifications/tools/list_changed`
    /// (AR-79). Default `false` — only the stdio child's watcher overrides.
    fn advertise_tool_list_changed(&self) -> bool {
        false
    }
}

/// The stateless MCP bridge handler (AR-70), generic over the spine backend.
pub struct McpBridgeHandler<B> {
    pub backend: B,
}

impl<B: McpBackend> ServerHandler for McpBridgeHandler<B> {
    fn get_info(&self) -> ServerInfo {
        let mut capabilities = ServerCapabilities::builder().enable_tools();
        if self.backend.advertise_tool_list_changed() {
            // AR-79 #4 (F-6): order matters — `enable_tools()` must precede
            // `enable_tool_list_changed()` (the builder only touches an
            // existing `tools` capability).
            capabilities = capabilities.enable_tool_list_changed();
        }
        ServerInfo::new(capabilities.build())
            .with_server_info(Implementation::new("nexus42", env!("CARGO_PKG_VERSION")))
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, McpError> {
        let rows = self.backend.list_tools().await?;
        let tools: Vec<Tool> = rows.into_iter().map(row_into_tool).collect();
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, McpError> {
        let name = request.name.to_string();
        let parameters = serde_json::Value::Object(request.arguments.unwrap_or_default());

        let outcome = self.backend.call_tool(&name, parameters).await?;

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

/// Typed unroutable discrimination (T4 review M-1).
///
/// `not_supported` without `details.wire_code` is unroutable
/// (never-admitted / evicted / allowlist-missing — the daemon
/// `BadRequest` path never supplies a wire code); `not_supported` WITH a
/// wire code is always a peer deny (executed-but-failed). Pure so it can
/// be unit-pinned.
#[must_use]
pub fn is_unroutable(code: &str, wire_code: Option<&str>) -> bool {
    code == "not_supported" && wire_code.is_none()
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
    fn unroutable_classification_is_typed_not_textual() {
        // Unroutable: not_supported + NO wire code.
        assert!(is_unroutable("not_supported", None));
        // Peer deny: not_supported + typed wire code — executed failure,
        // even when the message text contains the unroutable substring.
        assert!(!is_unroutable("not_supported", Some("op_unsupported")));
        // Other codes never classify unroutable regardless of wire code.
        assert!(!is_unroutable("invalid_input", None));
        assert!(!is_unroutable("internal", None));
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

    /// Stub backend for the `get_info` advertisement pin.
    struct StubBackend {
        advertise: bool,
    }

    impl McpBackend for StubBackend {
        fn list_tools(&self) -> impl Future<Output = Result<Vec<CatalogRow>, McpError>> + Send {
            std::future::ready(Ok(Vec::new()))
        }

        fn call_tool(
            &self,
            _tool_name: &str,
            _parameters: serde_json::Value,
        ) -> impl Future<Output = Result<ToolCallOutcome, McpError>> + Send {
            std::future::ready(Ok(ToolCallOutcome::Success(serde_json::Value::Null)))
        }

        fn advertise_tool_list_changed(&self) -> bool {
            self.advertise
        }
    }

    #[test]
    fn get_info_advertises_list_changed_only_when_backend_delivers_it() {
        // AR-79 #4 (F-6): the advertisement must carry
        // `tools.listChanged: true` ONLY when the backend runs the catalog
        // watcher. Model A (stdio child) overrides to `true`; the embedded
        // server (no watcher) keeps the honest default `false`.
        let watcher = McpBridgeHandler {
            backend: StubBackend { advertise: true },
        };
        let info = watcher.get_info();
        let tools = info
            .capabilities
            .tools
            .as_ref()
            .expect("tools capability advertised");
        assert_eq!(
            tools.list_changed,
            Some(true),
            "watcher backend advertises tools.listChanged (AR-79 #4)"
        );
        let wire = serde_json::to_value(&info).expect("ServerInfo serializes");
        assert_eq!(
            wire["capabilities"]["tools"]["listChanged"],
            serde_json::json!(true),
            "wire shape: capabilities.tools.listChanged == true"
        );

        let embedded = McpBridgeHandler {
            backend: StubBackend { advertise: false },
        };
        let info = embedded.get_info();
        let tools = info
            .capabilities
            .tools
            .as_ref()
            .expect("tools capability advertised");
        assert_eq!(
            tools.list_changed, None,
            "embedded backend must not advertise a listChanged it never delivers"
        );
        let wire = serde_json::to_value(&info).expect("ServerInfo serializes");
        assert_eq!(
            wire["capabilities"]["tools"]["listChanged"],
            serde_json::Value::Null,
            "wire shape: no listChanged key for the embedded backend"
        );
    }
}
