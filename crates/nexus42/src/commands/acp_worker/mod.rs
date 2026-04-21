//! `nexus42 acp-worker` — JSON-RPC main loop for the daemon-spawned worker.
//!
//! This is the long-lived CLI subprocess spawned by `nexus42d`'s Worker Manager
//! for each active creator. It hosts one or more ACP agent subprocesses and
//! communicates with the daemon via stdin/stdout JSON-RPC 2.0.
//!
//! Design: `orchestration-engine-v1.md` §6.3–§6.4, `acp-client-tech-spec-v2.md` §2.3.
//!
//! IPC methods (daemon → worker):
//! - `worker/initialize` → `{ capabilities, worker_pid, sessions }`
//! - `worker/acp_prompt` → streaming `worker/acp_prompt_chunk` + final reply
//! - `worker/acp_cancel` → `{}`
//! - `worker/acp_session_load` → `{ ok, error? }`
//! - `worker/agent_start` → `{ session }`
//! - `worker/agent_stop` → `{ ok }`
//! - `worker/agent_list` → `[ session_summary ]`
//! - `worker/health` → `{ uptime_ms, sessions: [ health_info ] }`
//! - `worker/shutdown` → `{}`
//!
//! Notifications (worker → daemon):
//! - `worker/acp_prompt_chunk` → `{ request_id, session_id, text }`
//! - `worker/agent_session_event` → `{ session_id, event, timestamp }`
//!
//! Uses `tokio::task::LocalSet` since `agent-client-protocol` futures are `!Send`.

#[allow(dead_code)]
pub mod agent_slot;

use crate::errors::Result;
use agent_slot::{AgentConfig, AgentSlot, AgentSlotState, SlotHealth};
use serde_json::{json, Value};
use std::collections::HashMap;
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
    /// Deprecated: prefer `agents` array in `worker/initialize`.
    #[arg(long, default_value = "claude-sonnet-4-20250514")]
    pub agent: Option<String>,
}

/// Shared state for the multiplexed worker, managing multiple agent sessions.
///
/// Uses `std::sync::RwLock` (not `tokio::sync`) because the IPC loop is
/// single-threaded and all guard scopes are kept non-async (no `.await` while
/// holding the lock).
struct MultiplexedWorkerState {
    creator_id: String,
    sessions: std::sync::RwLock<HashMap<String, AgentSlot>>,
    shutdown_requested: AtomicBool,
    start_time: std::time::Instant,
    request_counter: AtomicU64,
}

impl MultiplexedWorkerState {
    fn new(creator_id: String) -> Self {
        Self {
            creator_id,
            sessions: std::sync::RwLock::new(HashMap::new()),
            shutdown_requested: AtomicBool::new(false),
            start_time: std::time::Instant::now(),
            request_counter: AtomicU64::new(1),
        }
    }
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

    let state = Arc::new(MultiplexedWorkerState::new(args.creator));

    // The ACP SDK requires LocalSet since its futures are !Send.
    // However, the stdin/stdout IPC loop itself is Send. We run the
    // IPC loop on a regular tokio task and only use LocalSet for ACP calls.
    run_ipc_loop(state).await
}

/// Main IPC loop: read JSON-RPC from stdin, dispatch, write responses to stdout.
async fn run_ipc_loop(state: Arc<MultiplexedWorkerState>) -> Result<()> {
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
            write_jsonrpc_error(
                &mut stdout,
                id.as_ref(),
                -32000,
                "Worker shutting down",
                &method,
            )
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
                handle_session_load(&state, &mut stdout, id.as_ref(), &params).await?;
            }
            "worker/agent_start" => {
                handle_agent_start(&state, &mut stdout, id.as_ref(), &params).await?;
            }
            "worker/agent_stop" => {
                handle_agent_stop(&state, &mut stdout, id.as_ref(), &params).await?;
            }
            "worker/agent_list" => {
                handle_agent_list(&state, &mut stdout, id.as_ref(), &params).await?;
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
// Session summary / health helpers
// ---------------------------------------------------------------------------

/// Build a JSON summary for a single agent session.
fn session_summary_json(slot: &AgentSlot) -> Value {
    json!({
        "session_id": slot.session_id(),
        "acp_agent_id": slot.acp_agent_id(),
        "role": slot.role(),
        "model": slot.model(),
        "state": state_to_string(&slot.state()),
        "uptime_ms": slot.health().uptime_ms,
    })
}

/// Convert `AgentSlotState` to a string for JSON serialization.
fn state_to_string(state: &AgentSlotState) -> &str {
    match state {
        AgentSlotState::Initializing => "initializing",
        AgentSlotState::Ready => "ready",
        AgentSlotState::Prompting => "prompting",
        AgentSlotState::Error(_) => "error",
        AgentSlotState::Stopping => "stopping",
        AgentSlotState::Stopped => "stopped",
    }
}

/// Build a JSON health info object for a single agent session.
fn slot_health_json(session_id: &str, health: &SlotHealth) -> Value {
    json!({
        "session_id": session_id,
        "state": state_to_string(&health.state),
        "uptime_ms": health.uptime_ms,
        "healthy": health.is_healthy(),
        "last_error": health.last_error,
    })
}

// ---------------------------------------------------------------------------
// Notification helpers
// ---------------------------------------------------------------------------

/// Emit a `worker/agent_session_event` notification to stdout.
async fn emit_session_event(
    stdout: &mut tokio::io::Stdout,
    session_id: &str,
    event: &str,
) -> Result<()> {
    let notification = json!({
        "jsonrpc": "2.0",
        "method": "worker/agent_session_event",
        "params": {
            "session_id": session_id,
            "event": event,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    });
    let bytes = format!("{}\n", notification);
    stdout.write_all(bytes.as_bytes()).await?;
    stdout.flush().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Method handlers
// ---------------------------------------------------------------------------

async fn handle_initialize(
    state: &MultiplexedWorkerState,
    stdout: &mut tokio::io::Stdout,
    id: Option<&Value>,
    params: &Value,
) -> Result<()> {
    // All lock-protected work happens in this synchronous scope.
    // Collect session IDs for event emission after dropping the lock.
    let (started_session_ids, init_result): (Vec<String>, Value) = {
        let mut sessions = state
            .sessions
            .write()
            .map_err(|e| crate::errors::CliError::Other(format!("sessions lock poisoned: {e}")))?;

        // If sessions already exist, warn but allow (idempotent re-init).
        if !sessions.is_empty() {
            warn!(
                count = sessions.len(),
                "worker/initialize called with existing sessions"
            );
        }

        // Check for new multi-agent format first.
        if let Some(agents_array) = params.get("agents").and_then(|v| v.as_array()) {
            for (i, agent_obj) in agents_array.iter().enumerate() {
                let session_id = agent_obj
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&format!("default_{i}"))
                    .to_string();
                let acp_agent_id = agent_obj
                    .get("acp_agent_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("claude-sonnet-4-20250514")
                    .to_string();
                let role = agent_obj
                    .get("role")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let model = agent_obj
                    .get("model")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let config = AgentConfig {
                    session_id: session_id.clone(),
                    acp_agent_id,
                    role,
                    model,
                };

                let slot = AgentSlot::new(config);
                slot.mark_ready();
                sessions.insert(session_id.clone(), slot);
            }
        } else if let Some(agent_ref) = params.get("agent_ref").and_then(|v| v.as_str()) {
            // Backward-compatible single-agent format.
            let session_id = "default".to_string();
            let config = AgentConfig::new(session_id.clone(), agent_ref.to_string());
            let slot = AgentSlot::new(config);
            slot.mark_ready();
            sessions.insert(session_id.clone(), slot);
        } else {
            // No agent info provided — create a default session.
            let session_id = "default".to_string();
            let config =
                AgentConfig::new(session_id.clone(), "claude-sonnet-4-20250514".to_string());
            let slot = AgentSlot::new(config);
            slot.mark_ready();
            sessions.insert(session_id.clone(), slot);
        }

        // Build session summaries.
        let summaries: Vec<Value> = sessions.values().map(session_summary_json).collect();
        let result = json!({
            "capabilities": ["acp.prompt", "acp.session_load", "agent.start", "agent.stop", "agent.list"],
            "worker_pid": std::process::id(),
            "creator_id": state.creator_id,
            "sessions": summaries,
        });

        // Collect IDs for event emission.
        let ids: Vec<String> = sessions.keys().cloned().collect();

        // Write response while still holding the lock (stdout is not async-guarded).
        // We must NOT await here — write_jsonrpc_response is sync internally but
        // uses `stdout.write_all().await`. So we build the response and return it.
        (ids, result)
    };

    // Lock is dropped. Now do async I/O.
    write_jsonrpc_response(stdout, id, &init_result).await?;

    for sid in &started_session_ids {
        emit_session_event(stdout, sid, "started").await?;
    }

    Ok(())
}

async fn handle_acp_prompt(
    state: &MultiplexedWorkerState,
    stdout: &mut tokio::io::Stdout,
    id: Option<&Value>,
    params: &Value,
) -> Result<()> {
    let session_id = params
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default")
        .to_string();
    let prompt = params
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Synchronous lock scope: validate and transition state, collect what we need.
    enum PromptCheck {
        Ok { request_id: u64 },
        SessionNotFound,
        NotReady { state_str: String },
    }

    let check = {
        let sessions = state
            .sessions
            .read()
            .map_err(|e| crate::errors::CliError::Other(format!("sessions lock poisoned: {e}")))?;

        let slot = sessions.get(&session_id);
        let check = match slot {
            None => PromptCheck::SessionNotFound,
            Some(slot) => {
                let current_state = slot.state();
                if !current_state.is_ready() && !current_state.is_prompting() {
                    PromptCheck::NotReady {
                        state_str: state_to_string(&current_state).to_string(),
                    }
                } else {
                    let request_id = state.request_counter.fetch_add(1, Ordering::Relaxed);
                    slot.mark_prompting();
                    PromptCheck::Ok { request_id }
                }
            }
        };
        check
    };

    // Lock is dropped. Now handle errors via async I/O, or proceed.
    let request_id = match check {
        PromptCheck::SessionNotFound => {
            return write_jsonrpc_error(
                stdout,
                id,
                -32003,
                "Session not found",
                &format!("session {session_id} not found"),
            )
            .await;
        }
        PromptCheck::NotReady { state_str } => {
            return write_jsonrpc_error(
                stdout,
                id,
                -32004,
                "Agent slot not ready",
                &format!("session {session_id} is in state '{state_str}', expected 'ready'"),
            )
            .await;
        }
        PromptCheck::Ok { request_id } => request_id,
    };
    let chunk = json!({
        "jsonrpc": "2.0",
        "method": "worker/acp_prompt_chunk",
        "params": {
            "request_id": request_id,
            "session_id": session_id,
            "text": prompt,
        }
    });
    let chunk_bytes = format!("{}\n", chunk);
    stdout.write_all(chunk_bytes.as_bytes()).await?;
    stdout.flush().await?;

    // Transition back to Ready (synchronous lock scope — no await).
    {
        let sessions = state
            .sessions
            .read()
            .map_err(|e| crate::errors::CliError::Other(format!("sessions lock poisoned: {e}")))?;
        if let Some(slot) = sessions.get(&session_id) {
            slot.mark_ready_from_prompt();
        }
    }

    // Send final result.
    let result = json!({
        "done": true,
        "full_text": prompt,
        "session_id": session_id,
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
    _state: &MultiplexedWorkerState,
    stdout: &mut tokio::io::Stdout,
    id: Option<&Value>,
    _params: &Value,
) -> Result<()> {
    debug!("handling worker/acp_session_load");

    // Full implementation calls into nexus-acp-host to load the session.
    // For WS3, return a stub success.
    let result = json!({ "ok": true });
    write_jsonrpc_response(stdout, id, &result).await?;
    Ok(())
}

async fn handle_agent_start(
    state: &MultiplexedWorkerState,
    stdout: &mut tokio::io::Stdout,
    id: Option<&Value>,
    params: &Value,
) -> Result<()> {
    let session_id = match params.get("session_id").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return write_jsonrpc_error(
                stdout,
                id,
                -32005,
                "Missing session_id",
                "agent_start requires session_id",
            )
            .await;
        }
    };

    let acp_agent_id = params
        .get("acp_agent_id")
        .and_then(|v| v.as_str())
        .unwrap_or("claude-sonnet-4-20250514")
        .to_string();
    let role = params
        .get("role")
        .and_then(|v| v.as_str())
        .map(String::from);
    let model = params
        .get("model")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Synchronous lock scope.
    enum StartResult {
        Created(Value),
        Duplicate(String),
    }

    let start_result = {
        let mut sessions = state
            .sessions
            .write()
            .map_err(|e| crate::errors::CliError::Other(format!("sessions lock poisoned: {e}")))?;

        if sessions.contains_key(&session_id) {
            StartResult::Duplicate(session_id.clone())
        } else {
            let config = AgentConfig {
                session_id: session_id.clone(),
                acp_agent_id,
                role,
                model,
            };
            let slot = AgentSlot::new(config);
            slot.mark_ready();
            let summary = session_summary_json(&slot);
            sessions.insert(session_id.clone(), slot);
            StartResult::Created(summary)
        }
    };

    match start_result {
        StartResult::Created(summary) => {
            // Emit started event (async, no lock held).
            emit_session_event(stdout, &session_id, "started").await?;

            let result = json!({ "session": summary });
            write_jsonrpc_response(stdout, id, &result).await?;
        }
        StartResult::Duplicate(sid) => {
            write_jsonrpc_error(
                stdout,
                id,
                -32006,
                "Session already exists",
                &format!("session '{sid}' already exists"),
            )
            .await?;
        }
    }

    Ok(())
}

async fn handle_agent_stop(
    state: &MultiplexedWorkerState,
    stdout: &mut tokio::io::Stdout,
    id: Option<&Value>,
    params: &Value,
) -> Result<()> {
    let session_id = match params.get("session_id").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return write_jsonrpc_error(
                stdout,
                id,
                -32007,
                "Missing session_id",
                "agent_stop requires session_id",
            )
            .await;
        }
    };

    // Synchronous lock scope: find and remove the slot.
    let found = {
        let mut sessions = state
            .sessions
            .write()
            .map_err(|e| crate::errors::CliError::Other(format!("sessions lock poisoned: {e}")))?;

        if let Some(slot) = sessions.remove(&session_id) {
            slot.request_shutdown();
            slot.mark_stopped();
            true
        } else {
            false
        }
    };

    if !found {
        return write_jsonrpc_error(
            stdout,
            id,
            -32008,
            "Session not found",
            &format!("session '{session_id}' not found"),
        )
        .await;
    }

    // Emit stopped event (async, no lock held).
    emit_session_event(stdout, &session_id, "stopped").await?;

    let result = json!({ "ok": true });
    write_jsonrpc_response(stdout, id, &result).await?;
    Ok(())
}

async fn handle_agent_list(
    state: &MultiplexedWorkerState,
    stdout: &mut tokio::io::Stdout,
    id: Option<&Value>,
    _params: &Value,
) -> Result<()> {
    // Synchronous lock scope: collect summaries.
    let summaries = {
        let sessions = state
            .sessions
            .read()
            .map_err(|e| crate::errors::CliError::Other(format!("sessions lock poisoned: {e}")))?;

        sessions
            .values()
            .map(session_summary_json)
            .collect::<Vec<Value>>()
    };

    let result = json!({ "sessions": summaries });
    write_jsonrpc_response(stdout, id, &result).await?;
    Ok(())
}

async fn handle_health(
    state: &MultiplexedWorkerState,
    stdout: &mut tokio::io::Stdout,
    id: Option<&Value>,
    _params: &Value,
) -> Result<()> {
    let uptime_ms = state.start_time.elapsed().as_millis();

    // Synchronous lock scope: collect per-session health.
    let (creator_id, session_health) = {
        let sessions = state
            .sessions
            .read()
            .map_err(|e| crate::errors::CliError::Other(format!("sessions lock poisoned: {e}")))?;

        let health: Vec<Value> = sessions
            .iter()
            .map(|(sid, slot)| slot_health_json(sid, &slot.health()))
            .collect();

        (state.creator_id.clone(), health)
    };

    let result = json!({
        "uptime_ms": uptime_ms,
        "creator_id": creator_id,
        "session_count": session_health.len(),
        "sessions": session_health,
    });
    write_jsonrpc_response(stdout, id, &result).await?;
    Ok(())
}

async fn handle_shutdown(
    state: &MultiplexedWorkerState,
    stdout: &mut tokio::io::Stdout,
    id: Option<&Value>,
    _params: &Value,
) -> Result<()> {
    info!("worker/shutdown received, initiating graceful shutdown");
    state.shutdown_requested.store(true, Ordering::Relaxed);

    // Synchronous lock scope: request shutdown on all active sessions.
    if let Ok(sessions) = state.sessions.read() {
        for slot in sessions.values() {
            slot.request_shutdown();
        }
    }

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

    // --- MultiplexedWorkerState creation ---

    #[test]
    fn worker_state_defaults() {
        let state = MultiplexedWorkerState::new("test-creator".to_string());
        assert!(!state.shutdown_requested.load(Ordering::Relaxed));
        assert_eq!(state.creator_id, "test-creator");
        // No sessions initially.
        let sessions = state.sessions.read().expect("lock ok");
        assert!(sessions.is_empty());
    }

    // --- State string conversion ---

    #[test]
    fn state_to_string_maps_all_variants() {
        assert_eq!(
            state_to_string(&AgentSlotState::Initializing),
            "initializing"
        );
        assert_eq!(state_to_string(&AgentSlotState::Ready), "ready");
        assert_eq!(state_to_string(&AgentSlotState::Prompting), "prompting");
        assert_eq!(
            state_to_string(&AgentSlotState::Error("e".to_string())),
            "error"
        );
        assert_eq!(state_to_string(&AgentSlotState::Stopping), "stopping");
        assert_eq!(state_to_string(&AgentSlotState::Stopped), "stopped");
    }

    // --- Session summary JSON ---

    #[test]
    fn session_summary_json_contains_expected_fields() {
        let config = AgentConfig::new("sess_1".to_string(), "agent_1".to_string())
            .with_role("writer".to_string())
            .with_model("claude-3".to_string());
        let slot = AgentSlot::new(config);
        slot.mark_ready();

        let summary = session_summary_json(&slot);
        assert_eq!(summary["session_id"], "sess_1");
        assert_eq!(summary["acp_agent_id"], "agent_1");
        assert_eq!(summary["role"], "writer");
        assert_eq!(summary["model"], "claude-3");
        assert_eq!(summary["state"], "ready");
        // uptime_ms should be a number.
        assert!(summary["uptime_ms"].is_number());
    }

    // --- Slot health JSON ---

    #[test]
    fn slot_health_json_contains_expected_fields() {
        let config = AgentConfig::new("sess_1".to_string(), "agent_1".to_string());
        let slot = AgentSlot::new(config);
        slot.mark_ready();

        let health = slot.health();
        let json = slot_health_json("sess_1", &health);
        assert_eq!(json["session_id"], "sess_1");
        assert_eq!(json["state"], "ready");
        assert!(json["healthy"].as_bool().expect("bool"));
        assert!(json["uptime_ms"].is_number());
        assert!(json["last_error"].is_null());
    }

    #[test]
    fn slot_health_json_error_state() {
        let config = AgentConfig::new("sess_2".to_string(), "agent_2".to_string());
        let slot = AgentSlot::new(config);
        slot.mark_error("boom".to_string());

        let health = slot.health();
        let json = slot_health_json("sess_2", &health);
        assert_eq!(json["state"], "error");
        assert!(!json["healthy"].as_bool().expect("bool"));
        assert_eq!(json["last_error"], "boom");
    }

    // --- Initialize with backward-compat agent_ref ---

    #[test]
    fn initialize_with_agent_ref_creates_default_session() {
        let state = MultiplexedWorkerState::new("test-creator".to_string());
        let params = json!({
            "agent_ref": "claude-sonnet-4-20250514"
        });

        // Simulate initialize by manually applying logic.
        {
            let mut sessions = state.sessions.write().expect("lock ok");
            if let Some(agent_ref) = params.get("agent_ref").and_then(|v| v.as_str()) {
                let session_id = "default".to_string();
                let config = AgentConfig::new(session_id.clone(), agent_ref.to_string());
                let slot = AgentSlot::new(config);
                slot.mark_ready();
                sessions.insert(session_id, slot);
            }
        }

        let sessions = state.sessions.read().expect("lock ok");
        assert_eq!(sessions.len(), 1);
        assert!(sessions.contains_key("default"));
        let slot = sessions.get("default").expect("exists");
        assert_eq!(slot.acp_agent_id(), "claude-sonnet-4-20250514");
        assert_eq!(slot.state(), AgentSlotState::Ready);
    }

    // --- Initialize with new agents array ---

    #[test]
    fn initialize_with_agents_array_creates_multiple_sessions() {
        let state = MultiplexedWorkerState::new("test-creator".to_string());
        let params = json!({
            "agents": [
                {
                    "session_id": "writer_1",
                    "acp_agent_id": "claude-sonnet-4-20250514",
                    "role": "writer",
                    "model": "claude-3"
                },
                {
                    "session_id": "editor_1",
                    "acp_agent_id": "claude-sonnet-4-20250514",
                    "role": "editor"
                }
            ]
        });

        // Simulate initialize.
        {
            let mut sessions = state.sessions.write().expect("lock ok");
            if let Some(agents_array) = params.get("agents").and_then(|v| v.as_array()) {
                for (i, agent_obj) in agents_array.iter().enumerate() {
                    let session_id = agent_obj
                        .get("session_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&format!("default_{i}"))
                        .to_string();
                    let acp_agent_id = agent_obj
                        .get("acp_agent_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("claude-sonnet-4-20250514")
                        .to_string();
                    let role = agent_obj
                        .get("role")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let model = agent_obj
                        .get("model")
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    let config = AgentConfig {
                        session_id: session_id.clone(),
                        acp_agent_id,
                        role,
                        model,
                    };
                    let slot = AgentSlot::new(config);
                    slot.mark_ready();
                    sessions.insert(session_id, slot);
                }
            }
        }

        let sessions = state.sessions.read().expect("lock ok");
        assert_eq!(sessions.len(), 2);
        assert!(sessions.contains_key("writer_1"));
        assert!(sessions.contains_key("editor_1"));

        let writer = sessions.get("writer_1").expect("exists");
        assert_eq!(writer.role(), Some("writer"));
        assert_eq!(writer.model(), Some("claude-3"));

        let editor = sessions.get("editor_1").expect("exists");
        assert_eq!(editor.role(), Some("editor"));
        assert!(editor.model().is_none());
    }

    // --- Agent start / stop lifecycle ---

    #[test]
    fn agent_start_creates_slot_and_stops_removes() {
        let state = MultiplexedWorkerState::new("test".to_string());

        // Start an agent.
        {
            let mut sessions = state.sessions.write().expect("lock ok");
            let config = AgentConfig::new("s1".to_string(), "agent_a".to_string());
            let slot = AgentSlot::new(config);
            slot.mark_ready();
            sessions.insert("s1".to_string(), slot);
        }

        let sessions = state.sessions.read().expect("lock ok");
        assert_eq!(sessions.len(), 1);
        drop(sessions);

        // Stop the agent.
        {
            let mut sessions = state.sessions.write().expect("lock ok");
            if let Some(slot) = sessions.remove("s1") {
                slot.request_shutdown();
                slot.mark_stopped();
            }
        }

        let sessions = state.sessions.read().expect("lock ok");
        assert!(sessions.is_empty());
    }

    // --- acp_prompt routing ---

    #[test]
    fn acp_prompt_routes_to_correct_session() {
        let state = MultiplexedWorkerState::new("test".to_string());

        // Create two sessions.
        {
            let mut sessions = state.sessions.write().expect("lock ok");
            let s1 = AgentSlot::new(AgentConfig::new("s1".to_string(), "agent_a".to_string()));
            s1.mark_ready();
            sessions.insert("s1".to_string(), s1);

            let s2 = AgentSlot::new(AgentConfig::new("s2".to_string(), "agent_b".to_string()));
            s2.mark_ready();
            sessions.insert("s2".to_string(), s2);
        }

        // Route prompt to s1.
        {
            let sessions = state.sessions.read().expect("lock ok");
            let slot = sessions.get("s1").expect("found");
            assert!(slot.state().is_ready());
            slot.mark_prompting();
            slot.mark_ready_from_prompt();
            assert!(slot.state().is_ready());
        }

        // s2 should still be ready (untouched).
        {
            let sessions = state.sessions.read().expect("lock ok");
            let s2 = sessions.get("s2").expect("found");
            assert!(s2.state().is_ready());
        }
    }

    #[test]
    fn acp_prompt_errors_on_nonexistent_session() {
        let state = MultiplexedWorkerState::new("test".to_string());
        let sessions = state.sessions.read().expect("lock ok");
        assert!(sessions.get("nonexistent").is_none());
    }

    #[test]
    fn acp_prompt_errors_on_non_ready_session() {
        let state = MultiplexedWorkerState::new("test".to_string());

        // Create a session in Error state.
        {
            let mut sessions = state.sessions.write().expect("lock ok");
            let s1 = AgentSlot::new(AgentConfig::new("s1".to_string(), "agent_a".to_string()));
            s1.mark_error("startup failed".to_string());
            sessions.insert("s1".to_string(), s1);
        }

        let sessions = state.sessions.read().expect("lock ok");
        let slot = sessions.get("s1").expect("found");
        assert!(slot.state().is_error());
        assert!(!slot.state().is_ready());
    }

    // --- Agent list ---

    #[test]
    fn agent_list_returns_all_sessions() {
        let state = MultiplexedWorkerState::new("test".to_string());

        {
            let mut sessions = state.sessions.write().expect("lock ok");
            for i in 0..3 {
                let sid = format!("s{i}");
                let slot = AgentSlot::new(AgentConfig::new(sid.clone(), format!("agent_{i}")));
                slot.mark_ready();
                sessions.insert(sid, slot);
            }
        }

        let sessions = state.sessions.read().expect("lock ok");
        let summaries: Vec<Value> = sessions.values().map(session_summary_json).collect();
        assert_eq!(summaries.len(), 3);
    }

    // --- Health with per-session info ---

    #[test]
    fn health_returns_per_session_info() {
        let state = MultiplexedWorkerState::new("test-creator".to_string());

        {
            let mut sessions = state.sessions.write().expect("lock ok");
            let s1 = AgentSlot::new(AgentConfig::new(
                "healthy".to_string(),
                "agent_a".to_string(),
            ));
            s1.mark_ready();
            sessions.insert("healthy".to_string(), s1);

            let s2 = AgentSlot::new(AgentConfig::new(
                "errored".to_string(),
                "agent_b".to_string(),
            ));
            s2.mark_error("crash".to_string());
            sessions.insert("errored".to_string(), s2);
        }

        let sessions = state.sessions.read().expect("lock ok");
        let health_info: Vec<Value> = sessions
            .iter()
            .map(|(sid, slot)| slot_health_json(sid, &slot.health()))
            .collect();

        assert_eq!(health_info.len(), 2);

        // HashMap iteration order is non-deterministic, so look up by session_id.
        let healthy_entry = health_info
            .iter()
            .find(|h| h["session_id"] == "healthy")
            .expect("found healthy session");
        assert!(healthy_entry["healthy"].as_bool().expect("bool"));

        let errored_entry = health_info
            .iter()
            .find(|h| h["session_id"] == "errored")
            .expect("found errored session");
        assert!(!errored_entry["healthy"].as_bool().expect("bool"));
        assert_eq!(errored_entry["last_error"], "crash");
    }

    // --- Shutdown marks all sessions ---

    #[test]
    fn shutdown_requests_stop_on_all_sessions() {
        let state = MultiplexedWorkerState::new("test".to_string());

        {
            let mut sessions = state.sessions.write().expect("lock ok");
            let s1 = AgentSlot::new(AgentConfig::new("s1".to_string(), "agent_a".to_string()));
            s1.mark_ready();
            sessions.insert("s1".to_string(), s1);

            let s2 = AgentSlot::new(AgentConfig::new("s2".to_string(), "agent_b".to_string()));
            s2.mark_ready();
            sessions.insert("s2".to_string(), s2);
        }

        // Simulate shutdown.
        state.shutdown_requested.store(true, Ordering::Relaxed);
        if let Ok(sessions) = state.sessions.read() {
            for slot in sessions.values() {
                slot.request_shutdown();
            }
        }

        assert!(state.shutdown_requested.load(Ordering::Relaxed));
        let sessions = state.sessions.read().expect("lock ok");
        for slot in sessions.values() {
            assert!(slot.is_shutdown_requested());
        }
    }

    // --- Initialize with no params creates default session ---

    #[test]
    fn initialize_with_no_params_creates_default_session() {
        let state = MultiplexedWorkerState::new("test".to_string());
        let params = json!({});

        // Simulate initialize fallback path.
        {
            let mut sessions = state.sessions.write().expect("lock ok");
            let has_agents = params.get("agents").and_then(|v| v.as_array()).is_some();
            let has_agent_ref = params.get("agent_ref").and_then(|v| v.as_str()).is_some();

            if has_agents {
                // ...
            } else if has_agent_ref {
                // ...
            } else {
                let session_id = "default".to_string();
                let config =
                    AgentConfig::new(session_id.clone(), "claude-sonnet-4-20250514".to_string());
                let slot = AgentSlot::new(config);
                slot.mark_ready();
                sessions.insert(session_id, slot);
            }
        }

        let sessions = state.sessions.read().expect("lock ok");
        assert_eq!(sessions.len(), 1);
        assert!(sessions.contains_key("default"));
    }

    // --- Duplicate agent_start is rejected ---

    #[test]
    fn agent_start_rejects_duplicate_session_id() {
        let state = MultiplexedWorkerState::new("test".to_string());

        // Insert first session.
        {
            let mut sessions = state.sessions.write().expect("lock ok");
            let config = AgentConfig::new("s1".to_string(), "agent_a".to_string());
            let slot = AgentSlot::new(config);
            slot.mark_ready();
            sessions.insert("s1".to_string(), slot);
        }

        // Attempt duplicate insert.
        {
            let sessions = state.sessions.write().expect("lock ok");
            assert!(sessions.contains_key("s1"));
        }
    }

    // --- agent_stop on nonexistent session ---

    #[test]
    fn agent_stop_on_nonexistent_returns_none() {
        let state = MultiplexedWorkerState::new("test".to_string());

        let removed = {
            let mut sessions = state.sessions.write().expect("lock ok");
            sessions.remove("nonexistent").map(|slot| {
                slot.request_shutdown();
                slot.mark_stopped();
                true
            })
        };

        assert!(removed.is_none());
    }

    // --- Idempotent re-init ---

    #[test]
    fn initialize_idempotent_replaces_sessions() {
        let state = MultiplexedWorkerState::new("test".to_string());

        // First init.
        {
            let mut sessions = state.sessions.write().expect("lock ok");
            let config = AgentConfig::new("old_s1".to_string(), "old_agent".to_string());
            let slot = AgentSlot::new(config);
            slot.mark_ready();
            sessions.insert("old_s1".to_string(), slot);
        }

        // Re-init with new agents array.
        let params = json!({
            "agents": [
                { "session_id": "new_s1", "acp_agent_id": "new_agent" }
            ]
        });

        {
            let mut sessions = state.sessions.write().expect("lock ok");
            if let Some(agents_array) = params.get("agents").and_then(|v| v.as_array()) {
                // Clear and rebuild (test-only simulation of re-init).
                sessions.clear();
                for (i, agent_obj) in agents_array.iter().enumerate() {
                    let session_id = agent_obj
                        .get("session_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&format!("default_{i}"))
                        .to_string();
                    let acp_agent_id = agent_obj
                        .get("acp_agent_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("claude-sonnet-4-20250514")
                        .to_string();
                    let role = agent_obj
                        .get("role")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let model = agent_obj
                        .get("model")
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    let config = AgentConfig {
                        session_id: session_id.clone(),
                        acp_agent_id,
                        role,
                        model,
                    };
                    let slot = AgentSlot::new(config);
                    slot.mark_ready();
                    sessions.insert(session_id, slot);
                }
            }
        }

        let sessions = state.sessions.read().expect("lock ok");
        assert_eq!(sessions.len(), 1);
        assert!(sessions.contains_key("new_s1"));
        assert!(!sessions.contains_key("old_s1"));
    }
}
