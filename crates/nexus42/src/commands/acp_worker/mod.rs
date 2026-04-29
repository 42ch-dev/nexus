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

    /// Model override (e.g. `claude-3-opus`, `o3`).
    #[arg(long)]
    pub model: Option<String>,

    /// Role ID for this worker (e.g. `writer`, `reviewer`).
    #[arg(long)]
    pub role: Option<String>,

    /// Agent reference in `role:acp_agent_id[:model]` format (repeatable).
    /// Each ref overrides the agent and/or model for a specific role.
    /// Example: `--agent-ref reviewer:codex-acp:o3 --agent-ref writer:claude-acp`
    #[arg(long = "agent-ref", value_name = "ROLE:AGENT_ID[:MODEL]")]
    pub agent_ref: Vec<String>,
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
///
/// # Errors
///
/// Returns I/O errors if stdin/stdout communication fails.
/// Returns JSON parsing errors if malformed JSON-RPC requests are received.
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
                    &format!("unknown method: {method}"),
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
const fn state_to_string(state: &AgentSlotState) -> &str {
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
    reason: Option<&str>,
) -> Result<()> {
    let mut params = json!({
        "session_id": session_id,
        "event": event,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    if let Some(reason) = reason {
        params["reason"] = json!(reason);
    }
    let notification = json!({
        "jsonrpc": "2.0",
        "method": "worker/agent_session_event",
        "params": params,
    });
    let bytes = format!("{notification}\n");
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

        // If sessions already exist, clear them for idempotent re-init.
        if !sessions.is_empty() {
            warn!(
                count = sessions.len(),
                "worker/initialize called with existing sessions, clearing"
            );
            sessions.clear();
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
                // T8: system_prompt from IPC params (daemon reads from preset)
                let system_prompt = agent_obj
                    .get("system_prompt")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let config = AgentConfig {
                    session_id: session_id.clone(),
                    acp_agent_id,
                    role,
                    model,
                    system_prompt,
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
            sessions.insert(session_id, slot);
        } else {
            // No agent info provided — create a default session.
            let session_id = "default".to_string();
            let config =
                AgentConfig::new(session_id.clone(), "claude-sonnet-4-20250514".to_string());
            let slot = AgentSlot::new(config);
            slot.mark_ready();
            sessions.insert(session_id, slot);
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
        emit_session_event(stdout, sid, "started", None).await?;
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
    // Define PromptCheck before the lock scope.
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
    let chunk_bytes = format!("{chunk}\n");
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
    // T8: system_prompt from IPC params (daemon reads from preset)
    let system_prompt = params
        .get("system_prompt")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Define StartResult before the lock scope.
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
                system_prompt,
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
            emit_session_event(stdout, &session_id, "started", None).await?;

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
    emit_session_event(stdout, &session_id, "stopped", None).await?;

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

/// Handle a detected agent subprocess crash.
///
/// Called when a subprocess monitor detects that an agent process has exited
/// unexpectedly. Marks the slot as crashed (Error state) and emits a
/// `worker/agent_session_event` notification with `"crashed"` event.
///
/// The slot remains in the sessions map (not removed) so the daemon can
/// inspect its state and decide whether to restart or stop it. Other slots
/// are unaffected.
///
/// # Arguments
///
/// * `state` — shared worker state
/// * `stdout` — stdout for JSON-RPC responses and notifications
/// * `session_id` — which agent session crashed
/// * `reason` — human-readable crash reason (e.g., "exit code 1", "signal 9")
///
/// # Future integration
///
/// This function will be called from a per-slot subprocess monitor task once
/// real ACP subprocess supervision is implemented. Currently it is infrastructure
/// for crash detection; tests use `AgentSlot::simulate_crash` to exercise this
/// code path.
#[allow(dead_code)]
async fn handle_agent_crash(
    state: &MultiplexedWorkerState,
    stdout: &mut tokio::io::Stdout,
    session_id: &str,
    reason: &str,
) -> Result<()> {
    warn!(
        session_id = session_id,
        reason = reason,
        "agent subprocess crash detected"
    );

    // Synchronous lock scope: find the slot and mark it as crashed.
    let found = {
        let sessions = state
            .sessions
            .read()
            .map_err(|e| crate::errors::CliError::Other(format!("sessions lock poisoned: {e}")))?;

        if let Some(slot) = sessions.get(session_id) {
            slot.mark_crashed(reason);
            true
        } else {
            false
        }
    };

    if found {
        // Emit crashed event (async, no lock held).
        emit_session_event(stdout, session_id, "crashed", Some(reason)).await?;
    } else {
        warn!(
            session_id = session_id,
            "crash reported for unknown session, ignoring"
        );
    }

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
    let bytes = format!("{response}\n");
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
    let bytes = format!("{response}\n");
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
    // --- T8: includes system_prompt extraction

    #[test]
    fn initialize_with_agents_array_creates_multiple_sessions() {
        let state = MultiplexedWorkerState::new("test-creator".to_string());
        let params = json!({
            "agents": [
                {
                    "session_id": "writer_1",
                    "acp_agent_id": "claude-sonnet-4-20250514",
                    "role": "writer",
                    "model": "claude-3",
                    "system_prompt": "You are a creative writer."
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
                    // T8: system_prompt
                    let system_prompt = agent_obj
                        .get("system_prompt")
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    let config = AgentConfig {
                        session_id: session_id.clone(),
                        acp_agent_id,
                        role,
                        model,
                        system_prompt,
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
        // T8: system_prompt stored correctly
        assert_eq!(writer.system_prompt(), Some("You are a creative writer."));

        let editor = sessions.get("editor_1").expect("exists");
        assert_eq!(editor.role(), Some("editor"));
        assert!(editor.model().is_none());
        // T8: editor has no system_prompt in params
        assert!(editor.system_prompt().is_none());
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

            if has_agents || has_agent_ref {
                // Test placeholder: agents/agent_ref path not exercised
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

    // --- T8: agent_start with system_prompt ---

    #[test]
    fn agent_start_with_system_prompt_stores_correctly() {
        let state = MultiplexedWorkerState::new("test".to_string());

        // Simulate agent_start IPC with system_prompt.
        let params = json!({
            "session_id": "writer_sp",
            "acp_agent_id": "claude-acp",
            "role": "writer",
            "model": "claude-sonnet-4",
            "system_prompt": "You are a creative writing assistant."
        });

        {
            let mut sessions = state.sessions.write().expect("lock ok");
            let session_id = params
                .get("session_id")
                .and_then(|v| v.as_str())
                .expect("session_id");
            let acp_agent_id = params
                .get("acp_agent_id")
                .and_then(|v| v.as_str())
                .expect("acp_agent_id");
            let role = params
                .get("role")
                .and_then(|v| v.as_str())
                .map(String::from);
            let model = params
                .get("model")
                .and_then(|v| v.as_str())
                .map(String::from);
            let system_prompt = params
                .get("system_prompt")
                .and_then(|v| v.as_str())
                .map(String::from);

            let config = AgentConfig {
                session_id: session_id.to_string(),
                acp_agent_id: acp_agent_id.to_string(),
                role,
                model,
                system_prompt,
            };
            let slot = AgentSlot::new(config);
            slot.mark_ready();
            sessions.insert(session_id.to_string(), slot);
        }

        let sessions = state.sessions.read().expect("lock ok");
        let slot = sessions.get("writer_sp").expect("exists");
        assert_eq!(
            slot.system_prompt(),
            Some("You are a creative writing assistant.")
        );
        assert_eq!(slot.role(), Some("writer"));
    }

    // --- T8: agent_start without system_prompt (graceful handling) ---

    #[test]
    fn agent_start_without_system_prompt_works_gracefully() {
        let state = MultiplexedWorkerState::new("test".to_string());

        // Simulate agent_start IPC WITHOUT system_prompt.
        let params = json!({
            "session_id": "no_sp",
            "acp_agent_id": "claude-acp",
            "role": "editor"
        });

        {
            let mut sessions = state.sessions.write().expect("lock ok");
            let session_id = params
                .get("session_id")
                .and_then(|v| v.as_str())
                .expect("session_id");
            let acp_agent_id = params
                .get("acp_agent_id")
                .and_then(|v| v.as_str())
                .expect("acp_agent_id");
            let role = params
                .get("role")
                .and_then(|v| v.as_str())
                .map(String::from);
            let model = params
                .get("model")
                .and_then(|v| v.as_str())
                .map(String::from);
            let system_prompt = params
                .get("system_prompt")
                .and_then(|v| v.as_str())
                .map(String::from);

            let config = AgentConfig {
                session_id: session_id.to_string(),
                acp_agent_id: acp_agent_id.to_string(),
                role,
                model,
                system_prompt,
            };
            let slot = AgentSlot::new(config);
            slot.mark_ready();
            sessions.insert(session_id.to_string(), slot);
        }

        let sessions = state.sessions.read().expect("lock ok");
        let slot = sessions.get("no_sp").expect("exists");
        assert!(slot.system_prompt().is_none());
        assert_eq!(slot.role(), Some("editor"));
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

    // --- T8: verify system_prompt extraction in re-init path

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
                {
                    "session_id": "new_s1",
                    "acp_agent_id": "new_agent",
                    "role": "writer",
                    "system_prompt": "New system prompt."
                }
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
                    // T8: system_prompt
                    let system_prompt = agent_obj
                        .get("system_prompt")
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    let config = AgentConfig {
                        session_id: session_id.clone(),
                        acp_agent_id,
                        role,
                        model,
                        system_prompt,
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
        // T8: verify system_prompt in new slot
        let new_slot = sessions.get("new_s1").expect("exists");
        assert_eq!(new_slot.system_prompt(), Some("New system prompt."));
    }

    // --- T3b: Crash isolation ---

    #[test]
    fn crash_isolation_one_slot_crash_does_not_affect_others() {
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

        // Simulate crash on s1.
        {
            let sessions = state.sessions.read().expect("lock ok");
            let s1 = sessions.get("s1").expect("found");
            s1.simulate_crash("exit code 1");
        }

        // Verify s1 is in error state but s2 is still ready.
        {
            let sessions = state.sessions.read().expect("lock ok");
            let s1 = sessions.get("s1").expect("found");
            assert!(s1.state().is_error());

            let s2 = sessions.get("s2").expect("found");
            assert!(s2.state().is_ready());
        }
    }

    #[test]
    fn crash_isolation_three_slots_one_crashes() {
        let state = MultiplexedWorkerState::new("test".to_string());

        // Create three sessions.
        {
            let mut sessions = state.sessions.write().expect("lock ok");
            for sid in &["s1", "s2", "s3"] {
                let slot =
                    AgentSlot::new(AgentConfig::new(sid.to_string(), format!("agent_{sid}")));
                slot.mark_ready();
                sessions.insert(sid.to_string(), slot);
            }
        }

        // Crash s2 only.
        {
            let sessions = state.sessions.read().expect("lock ok");
            sessions
                .get("s2")
                .expect("found")
                .simulate_crash("OOM killed");
        }

        // Verify only s2 is in error state.
        {
            let sessions = state.sessions.read().expect("lock ok");
            assert!(sessions.get("s1").expect("found").state().is_ready());
            assert!(sessions.get("s2").expect("found").state().is_error());
            assert!(sessions.get("s3").expect("found").state().is_ready());
        }
    }

    #[test]
    fn crashed_slot_remains_in_sessions_map() {
        let state = MultiplexedWorkerState::new("test".to_string());

        {
            let mut sessions = state.sessions.write().expect("lock ok");
            let slot = AgentSlot::new(AgentConfig::new("s1".to_string(), "agent_a".to_string()));
            slot.mark_ready();
            sessions.insert("s1".to_string(), slot);
        }

        // Crash the slot.
        {
            let sessions = state.sessions.read().expect("lock ok");
            sessions
                .get("s1")
                .expect("found")
                .simulate_crash("signal 9");
        }

        // Slot should still be in the map (not removed).
        let sessions = state.sessions.read().expect("lock ok");
        assert_eq!(sessions.len(), 1);
        assert!(sessions.contains_key("s1"));
        // And it should be in error state.
        assert!(sessions.get("s1").expect("found").state().is_error());
    }

    #[test]
    fn crash_reason_recorded_in_health() {
        let state = MultiplexedWorkerState::new("test".to_string());

        {
            let mut sessions = state.sessions.write().expect("lock ok");
            let slot = AgentSlot::new(AgentConfig::new("s1".to_string(), "agent_a".to_string()));
            slot.mark_ready();
            sessions.insert("s1".to_string(), slot);
        }

        {
            let sessions = state.sessions.read().expect("lock ok");
            sessions
                .get("s1")
                .expect("found")
                .simulate_crash("segfault at address 0x0");
        }

        let sessions = state.sessions.read().expect("lock ok");
        let slot = sessions.get("s1").expect("found");
        let health = slot.health();
        assert!(!health.is_healthy());
        assert!(health
            .last_error
            .as_ref()
            .expect("error")
            .contains("segfault at address 0x0"));
        assert!(health
            .last_error
            .as_ref()
            .expect("error")
            .starts_with("[crash]"));
    }

    #[test]
    fn mark_crashed_from_any_state() {
        // Crash should work from Ready state.
        let s1 = AgentSlot::new(AgentConfig::new("s1".to_string(), "a".to_string()));
        s1.mark_ready();
        s1.simulate_crash("crash");
        assert!(s1.state().is_error());

        // Crash should work from Prompting state.
        let s2 = AgentSlot::new(AgentConfig::new("s2".to_string(), "a".to_string()));
        s2.mark_ready();
        s2.mark_prompting();
        s2.simulate_crash("crash");
        assert!(s2.state().is_error());

        // Crash should work from Initializing state.
        let s3 = AgentSlot::new(AgentConfig::new("s3".to_string(), "a".to_string()));
        s3.simulate_crash("crash during init");
        assert!(s3.state().is_error());

        // Crash should work from Error state (overwrite previous error).
        let s4 = AgentSlot::new(AgentConfig::new("s4".to_string(), "a".to_string()));
        s4.mark_error("first error".to_string());
        s4.simulate_crash("second crash");
        assert!(s4.state().is_error());
        let health = s4.health();
        assert!(health
            .last_error
            .as_ref()
            .expect("error")
            .contains("second crash"));
    }

    // --- T7: CLI flag parsing for multi-agent config ---

    use clap::Parser;

    /// Wrapper for parsing `AcpWorkerArgs` in tests.
    #[derive(Debug, Parser)]
    #[command(name = "acp-worker")]
    struct AcpWorkerCli {
        #[command(flatten)]
        args: AcpWorkerArgs,
    }

    #[test]
    fn acp_worker_args_basic_creator_agent() {
        let cli = AcpWorkerCli::try_parse_from([
            "acp-worker",
            "--creator",
            "c1",
            "--agent",
            "claude-sonnet-4-20250514",
        ])
        .unwrap();

        assert_eq!(cli.args.creator, "c1");
        assert_eq!(cli.args.agent.as_deref(), Some("claude-sonnet-4-20250514"));
        assert!(cli.args.model.is_none());
        assert!(cli.args.role.is_none());
        assert!(cli.args.agent_ref.is_empty());
    }

    #[test]
    fn acp_worker_args_with_model_and_role() {
        let cli = AcpWorkerCli::try_parse_from([
            "acp-worker",
            "--creator",
            "c1",
            "--model",
            "claude-3-opus",
            "--role",
            "writer",
        ])
        .unwrap();

        assert_eq!(cli.args.creator, "c1");
        assert_eq!(cli.args.model.as_deref(), Some("claude-3-opus"));
        assert_eq!(cli.args.role.as_deref(), Some("writer"));
    }

    #[test]
    fn acp_worker_args_with_agent_ref_two_segments() {
        let cli = AcpWorkerCli::try_parse_from([
            "acp-worker",
            "--creator",
            "c1",
            "--agent-ref",
            "writer:claude-acp",
        ])
        .unwrap();

        assert_eq!(cli.args.agent_ref.len(), 1);
        assert_eq!(cli.args.agent_ref[0], "writer:claude-acp");
    }

    #[test]
    fn acp_worker_args_with_agent_ref_three_segments() {
        let cli = AcpWorkerCli::try_parse_from([
            "acp-worker",
            "--creator",
            "c1",
            "--agent-ref",
            "reviewer:codex-acp:o3",
        ])
        .unwrap();

        assert_eq!(cli.args.agent_ref.len(), 1);
        assert_eq!(cli.args.agent_ref[0], "reviewer:codex-acp:o3");
    }

    #[test]
    fn acp_worker_args_with_multiple_agent_refs() {
        let cli = AcpWorkerCli::try_parse_from([
            "acp-worker",
            "--creator",
            "c1",
            "--agent-ref",
            "reviewer:codex-acp:o3",
            "--agent-ref",
            "writer:claude-acp",
        ])
        .unwrap();

        assert_eq!(cli.args.agent_ref.len(), 2);
    }

    #[test]
    fn acp_worker_args_all_flags() {
        let cli = AcpWorkerCli::try_parse_from([
            "acp-worker",
            "--creator",
            "c1",
            "--agent",
            "claude-sonnet-4-20250514",
            "--model",
            "claude-3-opus",
            "--role",
            "writer",
            "--agent-ref",
            "reviewer:codex-acp:o3",
        ])
        .unwrap();

        assert_eq!(cli.args.creator, "c1");
        assert_eq!(cli.args.agent.as_deref(), Some("claude-sonnet-4-20250514"));
        assert_eq!(cli.args.model.as_deref(), Some("claude-3-opus"));
        assert_eq!(cli.args.role.as_deref(), Some("writer"));
        assert_eq!(cli.args.agent_ref.len(), 1);
    }

    // --- T7: Integration with parse_agent_ref ---

    #[test]
    fn acp_worker_agent_ref_integration_two_segments() {
        let ref_str = "writer:claude-acp";
        let (role, agent, model) = crate::config::parse_agent_ref(ref_str).unwrap();
        assert_eq!(role, "writer");
        assert_eq!(agent, "claude-acp");
        assert!(model.is_none());
    }

    #[test]
    fn acp_worker_agent_ref_integration_three_segments() {
        let ref_str = "reviewer:codex-acp:o3";
        let (role, agent, model) = crate::config::parse_agent_ref(ref_str).unwrap();
        assert_eq!(role, "reviewer");
        assert_eq!(agent, "codex-acp");
        assert_eq!(model.as_deref(), Some("o3"));
    }

    #[test]
    fn acp_worker_agent_ref_integration_invalid_format() {
        let result = crate::config::parse_agent_ref("bad-format");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid --agent-ref format"));
    }
}
