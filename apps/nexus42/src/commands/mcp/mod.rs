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
//! The rmcp `ServerHandler` surface, the catalog-row→`Tool` mapping, and the
//! AR-70 #4 refusal vocabulary live in the shared bridge core
//! (`nexus_daemon_runtime::connect::mcp_bridge`, V1.179 P0 T1 DF-88); this
//! module supplies the Model A [`DaemonClientBackend`] adapter (loopback
//! HTTP) and the AR-79 catalog-watch loop (Model-A-only, not shared — the
//! embedded Model B server has no watcher).
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

use std::future::Future;
use std::time::Duration;

use nexus_daemon_runtime::connect::mcp_bridge::{
    is_unroutable, CatalogResponse, CatalogRow, McpBackend, McpBridgeHandler, ToolCallOutcome,
};
use rmcp::service::Peer;
use rmcp::transport::stdio;
use rmcp::{serve_server, ErrorData as McpError, RoleServer};

use crate::api::daemon_client::DaemonClient;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};
/// MCP child per-request timeout, strictly above the user-capability
/// sandbox wall so the sandbox wall-clock limit fires first.
///
/// `nexus-wasm-host`'s `DEFAULT_WALL_TIME` is 30 s; keeping this deadline at
/// 45 s makes the timeout ordering deterministic (QC-fix S-b): a user
/// capability that consumes its full wall budget is cut off by the
/// daemon-side sandbox, and the child's HTTP request then completes with the
/// daemon's honest timeout error well before this deadline.
///
/// Side-effect-after-timeout semantics: when the sandbox wall fires, the
/// module's `run()` is aborted mid-execution — the caller observes a bounded
/// failure while PARTIAL side effects of that run may already have landed
/// (state-change ambiguity is inherent to wall-clock aborts, AR-70 #4).
/// Keeping this deadline above the sandbox wall guarantees the child is
/// never the party that cuts off a running user capability first.
pub const MCP_REQUEST_TIMEOUT: Duration = Duration::from_secs(45);
/// Catalog watch interval (AR-79, DF-90): the child polls
/// `GET /v1/daemon/tools` every 2 s and sends `notifications/tools/list_changed`
/// when the digest changes.
///
/// A const, not configurable this iteration (AR-79 #1); the delivery bound
/// is interval + one request timeout.
pub const MCP_CATALOG_WATCH_INTERVAL: Duration = Duration::from_secs(2);

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
    // QC-fix S-b: use the MCP-specific request timeout (strictly above the
    // user-cap sandbox wall) instead of the default 30 s request timeout,
    // which RACED the sandbox wall (both could fire at ~30 s, making the
    // timeout ordering nondeterministic).
    let client = DaemonClient::with_timeouts(
        &config.daemon_url,
        crate::api::daemon_client::DEFAULT_CONNECT_TIMEOUT,
        MCP_REQUEST_TIMEOUT,
    )?;
    let handler = McpBridgeHandler {
        backend: DaemonClientBackend { client },
    };

    let service = serve_server(handler, stdio())
        .await
        .map_err(|e| CliError::Other(format!("mcp server init failed: {e}")))?;
    // AR-79 (DF-90): spawn the catalog watcher AFTER the initialize
    // handshake completes, so notifications are only ever sent inside an
    // initialized session (protocol-correct). The peer handle is cloned
    // from the RunningService (F-8) before `waiting()` consumes it; the
    // [`WatcherGuard`] aborts the task on every exit path (F-6).
    let watcher_peer = service.peer().clone();
    let watcher_client = DaemonClient::with_timeouts(
        &config.daemon_url,
        crate::api::daemon_client::DEFAULT_CONNECT_TIMEOUT,
        MCP_REQUEST_TIMEOUT,
    )?;
    let watcher = tokio::spawn(catalog_watch_loop(watcher_client, watcher_peer));
    let _watcher_guard = WatcherGuard(watcher);
    service
        .waiting()
        .await
        .map_err(|e| CliError::Other(format!("mcp server failed: {e}")))?;
    Ok(())
}

/// Scope guard that aborts the catalog watcher task on EVERY exit path —
/// success, `waiting()` error, or panic unwinding (F-6, qc1 S-003 ∩ qc2
/// S-004 ∩ qc3 S-005). The explicit `watcher.abort()` on the success path
/// alone left the Err path returning via `?` without aborting; the guard
/// makes the lifecycle contract self-evident on both paths.
struct WatcherGuard(tokio::task::JoinHandle<()>);

impl Drop for WatcherGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Model A backend (AR-71): loopback HTTP through the daemon client. Every
/// `tools/list` is a live `GET /v1/daemon/tools` and every `tools/call` is
/// a live `POST …/tool-executions` — the child holds a digest + interval,
/// not a registry, not an allowlist, not policy, not a read cache (AR-79
/// #5).
struct DaemonClientBackend {
    client: DaemonClient,
}

impl McpBackend for DaemonClientBackend {
    async fn list_tools(&self) -> std::result::Result<Vec<CatalogRow>, McpError> {
        let catalog: CatalogResponse = self.client.get("/v1/daemon/tools").await.map_err(|e| {
            McpError::internal_error(format!("daemon tools list failed: {e}"), None)
        })?;
        Ok(catalog.items)
    }

    async fn call_tool(
        &self,
        tool_name: &str,
        parameters: serde_json::Value,
    ) -> std::result::Result<ToolCallOutcome, McpError> {
        let wire = self
            .client
            .post_execution_raw(tool_name, parameters)
            .await
            .map_err(|e| McpError::internal_error(format!("daemon tool call failed: {e}"), None))?;
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
            // Unroutable: the spine cannot resolve the id at all. Typed
            // discriminator (T4 review M-1, 2026-08-24): an unroutable id
            // arrives as `not_supported` with NO `details.wire_code`
            // (the daemon's BadRequest path never supplies one), while a
            // peer-deny `not_supported` ALWAYS carries the spoke wire
            // code (lowercase, e.g. `op_unsupported`) via the T5 typed
            // path. Classify on the presence of the typed field — never
            // on message text (a future spoke reject message containing
            // a matching substring must not misclassify).
            _ if is_unroutable(&code, wire_code.as_deref()) => {
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

    fn advertise_tool_list_changed(&self) -> bool {
        // AR-79 (DF-90): this backend runs the catalog-watch loop, so the
        // advertisement carries `tools.listChanged: true`.
        true
    }
}

/// AR-79 (DF-90) catalog watch loop: poll `GET /v1/daemon/tools` every
/// [`MCP_CATALOG_WATCH_INTERVAL`], digest the deterministic (id-sorted)
/// body, and send `notifications/tools/list_changed` via the cloned peer
/// handle when the digest changes between successful polls.
///
/// Baseline semantics (AR-79 #3): the first successful poll establishes
/// the baseline digest WITHOUT notifying (no spurious `listChanged` at
/// session start); notify fires only on a change between successful
/// polls. Poll errors (daemon down / auth) keep the last digest, log to
/// stderr, and never notify — a daemon that comes back with a changed
/// catalog then notifies once, correctly.
///
/// Retry semantics (F-1, qc2 W-001): `last_digest` advances ONLY after a
/// successful notify (or on the baseline poll). A failed
/// `notify_tool_list_changed` keeps the previous digest, so the next
/// successful poll retries the notification. `listChanged` is idempotent
/// (the client re-lists) — duplicates are safe, loss is not.
///
/// Logging bound (F-5, qc3 S-001): poll errors log once per error-state
/// transition (previous poll succeeded, or the error message changed) —
/// never every 2 s during an outage.
///
/// Model A preserved (AR-79 #5): the child holds a digest + interval —
/// not a registry, not an allowlist, not policy, not a read cache. Every
/// `tools/list` remains a live daemon round trip.
async fn catalog_watch_loop(client: DaemonClient, peer: Peer<RoleServer>) {
    let _ = catalog_watch_loop_inner(
        MCP_CATALOG_WATCH_INTERVAL,
        || fetch_catalog_body(&client),
        || async {
            peer.notify_tool_list_changed()
                .await
                .map_err(|e| e.to_string())
        },
        || false,
    )
    .await;
}

/// Core watch loop, generic over the poll and notify operations so the
/// retry semantics (F-1) and the log bound (F-5) are unit-testable
/// without a real daemon or MCP transport.
///
/// Returns the number of stderr log lines emitted (poll-error
/// transitions + notify failures) — the production wrapper ignores it;
/// tests assert the F-5 log bound directly. `should_stop` lets tests
/// terminate the otherwise-infinite loop deterministically.
async fn catalog_watch_loop_inner<PF, NF, P, N>(
    interval: Duration,
    mut poll: P,
    mut notify: N,
    mut should_stop: impl FnMut() -> bool,
) -> usize
where
    P: FnMut() -> PF,
    PF: Future<Output = std::result::Result<serde_json::Value, CliError>> + Send,
    N: FnMut() -> NF,
    NF: Future<Output = std::result::Result<(), String>> + Send,
{
    let mut last_digest: Option<serde_json::Value> = None;
    let mut last_poll_error: Option<String> = None;
    let mut log_lines = 0usize;
    loop {
        if should_stop() {
            return log_lines;
        }
        tokio::time::sleep(interval).await;
        let digest = match poll().await {
            Ok(digest) => digest,
            Err(e) => {
                // Keep the last digest; never notify on a poll error.
                let message = e.to_string();
                if last_poll_error.as_deref() != Some(message.as_str()) {
                    eprintln!("mcp catalog watch: poll failed: {message}");
                    last_poll_error = Some(message);
                    log_lines += 1;
                }
                continue;
            }
        };
        last_poll_error = None;
        if last_digest.as_ref() == Some(&digest) {
            continue;
        }
        if last_digest.is_some() {
            // A change between successful polls — notify. The first
            // successful poll only establishes the baseline.
            if let Err(e) = notify().await {
                eprintln!("mcp catalog watch: notify_tool_list_changed failed: {e}");
                log_lines += 1;
                // F-1: keep the previous digest so the next successful
                // poll retries the notification (idempotent — duplicates
                // are safe, loss is not).
                continue;
            }
        }
        last_digest = Some(digest);
    }
}

/// Fetch the catalog body. The route already sorts rows by id (tools.rs
/// L126), so the raw body is stable for an unchanged catalog; object
/// keys are order-insensitive in the `Value` comparison, while the
/// `items` array is order-sensitive BY DESIGN — the route's id-sort is
/// the documented invariant that makes an unchanged catalog compare
/// equal (F-7, qc1 S-001 ∩ qc2 S-003).
async fn fetch_catalog_body(
    client: &DaemonClient,
) -> std::result::Result<serde_json::Value, CliError> {
    let body: serde_json::Value = client.get("/v1/daemon/tools").await?;
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::ServerHandler;
    use std::sync::Arc;

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
    fn mcp_request_timeout_strictly_exceeds_user_cap_sandbox_wall() {
        // QC-fix S-b: the child's request timeout must NEVER fire before
        // the user-capability sandbox wall — the wall aborts the module
        // first, so the timeout ordering is deterministic and a running
        // user capability is never cut off by the child's HTTP client.
        let sandbox_wall = nexus_wasm_host::SandboxConfig::default().wall_time;
        assert!(
            MCP_REQUEST_TIMEOUT > sandbox_wall,
            "MCP child request timeout ({MCP_REQUEST_TIMEOUT:?}) must strictly exceed \
             the user-cap sandbox wall ({sandbox_wall:?})"
        );
    }
    #[tokio::test]
    async fn notify_failure_retries_on_next_poll() {
        // F-1 (qc2 W-001): a failed `notify_tool_list_changed` must NOT
        // advance `last_digest` — the next successful poll retries the
        // notification. `listChanged` is idempotent (the client re-lists),
        // so duplicates are safe; a lost notification is not.
        let interval = Duration::from_millis(10);
        let polls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let notifies = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let notify_fail = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let body = serde_json::json!({ "items": [{ "id": "nexus.workspace.info" }] });
        let body2 =
            serde_json::json!({ "items": [{ "id": "nexus.workspace.info" }, { "id": "t6.wcap" }] });

        // Poll 1: baseline (no notify). Poll 2: change → notify FAILS.
        // Poll 3: same body → digest unchanged → would skip, but the
        // failed notify kept the OLD digest, so the change is still
        // pending → notify retried and succeeds.
        let poll_polls = polls.clone();
        let poll = move || {
            let n = poll_polls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            let body = if n == 1 { body.clone() } else { body2.clone() };
            async move { Ok::<_, CliError>(body) }
        };
        let notify_notifies = notifies.clone();
        let notify_fail_flag = notify_fail.clone();
        let notify = move || {
            notify_notifies.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let fail = notify_fail_flag.swap(false, std::sync::atomic::Ordering::SeqCst);
            async move {
                if fail {
                    Err::<(), _>("transport closed".to_string())
                } else {
                    Ok(())
                }
            }
        };
        let stop_polls = polls.clone();
        let stop = move || stop_polls.load(std::sync::atomic::Ordering::SeqCst) >= 3;

        catalog_watch_loop_inner(interval, poll, notify, stop).await;

        assert_eq!(
            notifies.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "failed notify retried on the next poll"
        );
    }

    #[tokio::test]
    async fn poll_error_logs_once_per_transition() {
        // F-5 (qc3 S-001): poll errors log once per error-state
        // transition (previous poll succeeded, or the error message
        // changed) — never every interval during an outage.
        let interval = Duration::from_millis(10);
        let polls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let notifies = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let poll_polls = polls.clone();
        let poll = move || {
            let n = poll_polls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            async move {
                if n <= 3 {
                    Err::<serde_json::Value, _>(CliError::DaemonNotRunning)
                } else {
                    Ok::<_, CliError>(serde_json::json!({ "items": [] }))
                }
            }
        };
        let notify_notifies = notifies.clone();
        let notify = move || {
            notify_notifies.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async move { Ok::<(), String>(()) }
        };
        let stop_polls = polls.clone();
        let stop = move || stop_polls.load(std::sync::atomic::Ordering::SeqCst) >= 4;

        let log_lines = catalog_watch_loop_inner(interval, poll, notify, stop).await;

        // 3 consecutive identical poll errors → exactly 1 stderr line
        // (the transition into the error state); the 4th poll succeeds
        // and establishes the baseline without notifying.
        assert_eq!(log_lines, 1, "identical poll errors log once");
        assert_eq!(
            notifies.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "baseline poll never notifies"
        );
    }

    #[test]
    fn get_info_advertises_tools_list_changed() {
        // AR-79 #4 (F-6): the Model A backend runs the catalog watcher, so
        // the advertisement must carry `tools.listChanged: true` — the wire
        // shape a long-lived MCP client checks before relying on
        // `notifications/tools/list_changed`. The builder order pin
        // (`enable_tools()` before `enable_tool_list_changed()`) is
        // exercised by the real `get_info` path.
        let handler = McpBridgeHandler {
            backend: DaemonClientBackend {
                client: DaemonClient::new("http://127.0.0.1:1"),
            },
        };
        let info = handler.get_info();
        let tools = info
            .capabilities
            .tools
            .as_ref()
            .expect("tools capability advertised");
        assert_eq!(
            tools.list_changed,
            Some(true),
            "tools.listChanged must be advertised (AR-79 #4)"
        );
        // The serialized initialize result carries the camelCase wire key.
        let wire = serde_json::to_value(&info).expect("ServerInfo serializes");
        assert_eq!(
            wire["capabilities"]["tools"]["listChanged"],
            serde_json::json!(true),
            "wire shape: capabilities.tools.listChanged == true"
        );
    }
}
