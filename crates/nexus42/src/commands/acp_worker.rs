//! `nexus42 acp-worker` — JSON-RPC main loop for the daemon-spawned worker.
//!
//! This is the long-lived CLI subprocess spawned by `nexus42d`'s Worker Manager
//! for each active creator. It hosts an ACP agent and communicates with the
//! daemon via stdin/stdout JSON-RPC 2.0.
//!
//! Design: `orchestration-engine-v1.md` §6.3–§6.4, `acp-client-tech-spec-v2.md` §2.3.
//!
//! IPC methods (daemon → worker):
//! - `worker/initialize` → `{ capabilities, worker_pid }`
//! - `worker/acp_prompt` → streaming `worker/acp_prompt_chunk` + final reply
//! - `worker/acp_cancel` → `{}`
//! - `worker/acp_session_load` → `{ ok, error? }`
//! - `worker/health` → `{ uptime_ms, acp_session_state, last_error? }`
//! - `worker/shutdown` → `{}`
//!
//! Uses `tokio::task::LocalSet` since `agent-client-protocol` futures are `!Send`.

use crate::errors::Result;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as AsyncBufReader};
use tracing::{debug, error, info, warn};

#[derive(clap::Args, Debug)]
pub struct AcpWorkerArgs {
    /// Creator ID this worker is associated with.
    #[arg(long)]
    pub creator: String,

    /// Agent reference (e.g. `claude-sonnet-4-20250514`).
    #[arg(long, default_value = "claude-sonnet-4-20250514")]
    pub agent: Option<String>,
}

/// Shared state between the main loop and signal handlers.
struct WorkerState {
    creator_id: String,
    agent_ref: String,
    initialized: AtomicBool,
    shutdown_requested: AtomicBool,
    start_time: std::time::Instant,
    request_counter: AtomicU64,
}

/// Run the acp-worker JSON-RPC main loop.
///
/// Reads newline-delimited JSON from stdin, dispatches to handler functions,
/// writes responses to stdout. Uses `tokio::task::LocalSet` for `!Send` compat.
pub async fn run(args: AcpWorkerArgs) -> Result<()> {
    let agent_ref = args
        .agent
        .unwrap_or_else(|| "claude-sonnet-4-20250514".to_string());

    info!(
        creator_id = %args.creator,
        agent_ref = %agent_ref,
        pid = std::process::id(),
        "acp-worker starting"
    );

    let state = Arc::new(WorkerState {
        creator_id: args.creator,
        agent_ref,
        initialized: AtomicBool::new(false),
        shutdown_requested: AtomicBool::new(false),
        start_time: std::time::Instant::now(),
        request_counter: AtomicU64::new(1),
    });

    // The ACP SDK requires LocalSet since its futures are !Send.
    // However, the stdin/stdout IPC loop itself is Send. We run the
    // IPC loop on a regular tokio task and only use LocalSet for ACP calls.
    run_ipc_loop(state).await
}

/// Main IPC loop: read JSON-RPC from stdin, dispatch, write responses to stdout.
async fn run_ipc_loop(state: Arc<WorkerState>) -> Result<()> {
    let stdin = tokio::io::stdin();
    let mut reader = AsyncBufReader::new(stdin);
    let mut stdout = tokio::io::stdout();

    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            // EOF — daemon pipe closed.
            info!("stdin EOF, worker exiting");
            break;
        }

        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        debug!(line = %line, "received JSON-RPC request");

        // Parse the JSON-RPC request.
        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                error!(error = %e, "failed to parse JSON-RPC request");
                write_jsonrpc_error(&mut stdout, None, -32700, "Parse error", &line).await?;
                continue;
            }
        };

        let method = request
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let id = request.get("id").cloned();
        let params = request.get("params").cloned().unwrap_or(Value::Null);

        // Check shutdown state before processing.
        if state.shutdown_requested.load(Ordering::Relaxed) {
            info!("shutdown already requested, skipping request: {}", method);
            write_jsonrpc_error(&mut stdout, id.as_ref(), -32000, "Worker shutting down", &method)
                .await?;
            continue;
        }

        match method.as_str() {
            "worker/initialize" => {
                handle_initialize(&state, &mut stdout, id.as_ref(), &params).await?;
            }
            "worker/acp_prompt" => {
                handle_acp_prompt(&state, &mut stdout, id.as_ref(), &params).await?;
            }
            "worker/acp_cancel" => {
                handle_cancel(&mut stdout, id.as_ref(), &params).await?;
            }
            "worker/acp_session_load" => {
                handle_session_load(&mut stdout, id.as_ref(), &params).await?;
            }
            "worker/health" => {
                handle_health(&state, &mut stdout, id.as_ref(), &params).await?;
            }
            "worker/shutdown" => {
                handle_shutdown(&state, &mut stdout, id.as_ref(), &params).await?;
                break;
            }
            _ => {
                write_jsonrpc_error(
                    &mut stdout,
                    id.as_ref(),
                    -32601,
                    "Method not found",
                    &format!("unknown method: {}", method),
                )
                .await?;
            }
        }
    }

    info!("acp-worker exiting");
    Ok(())
}

// ---------------------------------------------------------------------------
// Method handlers
// ---------------------------------------------------------------------------

async fn handle_initialize(
    state: &WorkerState,
    stdout: &mut tokio::io::Stdout,
    id: Option<&Value>,
    _params: &Value,
) -> Result<()> {
    if state.initialized.swap(true, Ordering::Relaxed) {
        warn!("worker/initialize called but already initialized");
    }

    let result = json!({
        "capabilities": ["acp.prompt", "acp.session_load"],
        "worker_pid": std::process::id(),
        "creator_id": state.creator_id,
        "agent_ref": state.agent_ref,
    });

    write_jsonrpc_response(stdout, id, &result).await?;
    Ok(())
}

async fn handle_acp_prompt(
    state: &WorkerState,
    stdout: &mut tokio::io::Stdout,
    id: Option<&Value>,
    params: &Value,
) -> Result<()> {
    if !state.initialized.load(Ordering::Relaxed) {
        return write_jsonrpc_error(
            stdout,
            id,
            -32002,
            "Worker not initialized",
            "call worker/initialize first",
        )
        .await;
    }

    let prompt = params
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let _tool_policy = params
        .get("tool_policy")
        .and_then(|v| v.as_str())
        .unwrap_or("auto_grant_read_only");
    let _session_id = params
        .get("session_id")
        .and_then(|v| v.as_str());

    debug!(prompt_len = prompt.len(), "handling worker/acp_prompt");

    // In the full implementation, this would call into nexus-acp-host to
    // dispatch the prompt to the ACP agent and stream chunks back.
    // For WS3, we send a single chunk notification + final result.

    let request_id = state.request_counter.fetch_add(1, Ordering::Relaxed);

    // Stream a chunk notification (simulated — full impl streams real chunks).
    let chunk = json!({
        "jsonrpc": "2.0",
        "method": "worker/acp_prompt_chunk",
        "params": {
            "request_id": request_id,
            "text": prompt
        }
    });
    let chunk_bytes = format!("{}\n", chunk);
    stdout.write_all(chunk_bytes.as_bytes()).await?;
    stdout.flush().await?;

    // Send final result.
    let result = json!({
        "done": true,
        "full_text": prompt
    });

    write_jsonrpc_response(stdout, id, &result).await?;

    Ok(())
}

async fn handle_cancel(
    stdout: &mut tokio::io::Stdout,
    id: Option<&Value>,
    _params: &Value,
) -> Result<()> {
    debug!("handling worker/acp_cancel");
    let result = json!({});
    write_jsonrpc_response(stdout, id, &result).await?;
    Ok(())
}

async fn handle_session_load(
    stdout: &mut tokio::io::Stdout,
    id: Option<&Value>,
    params: &Value,
) -> Result<()> {
    let _session_id = params
        .get("session_id")
        .and_then(|v| v.as_str());

    debug!("handling worker/acp_session_load");

    // Full implementation calls into nexus-acp-host to load the session.
    // For WS3, return a stub success.
    let result = json!({ "ok": true });
    write_jsonrpc_response(stdout, id, &result).await?;
    Ok(())
}

async fn handle_health(
    state: &WorkerState,
    stdout: &mut tokio::io::Stdout,
    id: Option<&Value>,
    _params: &Value,
) -> Result<()> {
    let uptime_ms = state.start_time.elapsed().as_millis();
    let result = json!({
        "uptime_ms": uptime_ms,
        "acp_session_state": if state.initialized.load(Ordering::Relaxed) {
            "ready"
        } else {
            "not_initialized"
        },
        "creator_id": state.creator_id,
    });
    write_jsonrpc_response(stdout, id, &result).await?;
    Ok(())
}

async fn handle_shutdown(
    state: &WorkerState,
    stdout: &mut tokio::io::Stdout,
    id: Option<&Value>,
    _params: &Value,
) -> Result<()> {
    info!("worker/shutdown received, initiating graceful shutdown");
    state.shutdown_requested.store(true, Ordering::Relaxed);

    let result = json!({});
    write_jsonrpc_response(stdout, id, &result).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// JSON-RPC helpers
// ---------------------------------------------------------------------------

/// Write a JSON-RPC 2.0 response (with id) to stdout.
async fn write_jsonrpc_response(
    stdout: &mut tokio::io::Stdout,
    id: Option<&Value>,
    result: &Value,
) -> Result<()> {
    let response = json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    });
    let bytes = format!("{}\n", response);
    stdout.write_all(bytes.as_bytes()).await?;
    stdout.flush().await?;
    Ok(())
}

/// Write a JSON-RPC 2.0 error response to stdout.
async fn write_jsonrpc_error(
    stdout: &mut tokio::io::Stdout,
    id: Option<&Value>,
    code: i64,
    message: &str,
    _data: &str,
) -> Result<()> {
    let response = json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
        }
    });
    let bytes = format!("{}\n", response);
    stdout.write_all(bytes.as_bytes()).await?;
    stdout.flush().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_state_defaults() {
        let state = WorkerState {
            creator_id: "test".into(),
            agent_ref: "claude".into(),
            initialized: AtomicBool::new(false),
            shutdown_requested: AtomicBool::new(false),
            start_time: std::time::Instant::now(),
            request_counter: AtomicU64::new(1),
        };
        assert!(!state.initialized.load(Ordering::Relaxed));
        assert!(!state.shutdown_requested.load(Ordering::Relaxed));
        assert_eq!(state.creator_id, "test");
    }
}
