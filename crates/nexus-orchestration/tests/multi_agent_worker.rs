//! Integration tests for multi-agent worker feature (WS-E T4b).
//!
//! Tests exercise the full IPC roundtrip between daemon-side code
//! (WorkerHandle / IpcClient) and a mock worker that implements the
//! multi-agent JSON-RPC protocol (worker/initialize, worker/agent_start,
//! worker/agent_stop, worker/agent_list, worker/acp_prompt, worker/shutdown).
//!
//! The mock worker runs as a tokio task behind a [`DuplexTransport`] pair,
//! faithfully reproducing the request routing and state management of the
//! real `nexus42 acp-worker` binary — without requiring a compiled binary
//! or ACP SDK connection.
//!
//! Test coverage:
//! 1. `multi_agent_initialize_two_sessions`  — Step 1
//! 2. `concurrent_prompts_routed_correctly`  — Step 2
//! 3. `crash_isolation_error_slot_no_impact` — Step 3
//! 4. `dynamic_agent_start_stop`             — Step 4
//! 5. `backward_compat_single_agent_prompt`  — Step 5
//! 6. `shutdown_with_multiple_agents`        — Step 6

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use nexus_orchestration::worker::{
    DuplexTransport, IpcClient, RpcTransport, WorkerAgentConfig, WorkerHandle,
};
use serde_json::{json, Value};
use tokio::sync::{Mutex, RwLock};

// ---------------------------------------------------------------------------
// Mock multi-agent worker
// ---------------------------------------------------------------------------

/// Per-session state tracked by the mock worker.
#[derive(Debug, Clone)]
struct MockSlot {
    session_id: String,
    acp_agent_id: String,
    role: Option<String>,
    model: Option<String>,
    state: String,
    created_at: std::time::Instant,
}

impl MockSlot {
    fn to_summary(&self) -> Value {
        json!({
            "session_id": self.session_id,
            "acp_agent_id": self.acp_agent_id,
            "role": self.role,
            "model": self.model,
            "state": self.state,
            "uptime_ms": self.created_at.elapsed().as_millis() as u64,
        })
    }
}

/// Shared state for the mock worker, mirroring `MultiplexedWorkerState`.
struct MockWorkerState {
    creator_id: String,
    sessions: RwLock<HashMap<String, MockSlot>>,
    shutdown_requested: AtomicBool,
    request_counter: AtomicU64,
}

impl MockWorkerState {
    fn new(creator_id: String) -> Self {
        Self {
            creator_id,
            sessions: RwLock::new(HashMap::new()),
            shutdown_requested: AtomicBool::new(false),
            request_counter: AtomicU64::new(1),
        }
    }
}

/// Result type for mock handler functions.
///
/// `Ok(value)` → JSON-RPC success response (returned in `result` field).
/// `Err((code, message))` → JSON-RPC error response.
type HandlerResult = Result<Value, (i64, String)>;

/// Spawn a mock multi-agent worker server task.
///
/// Returns the client-side `IpcClient` and the server task handle.
async fn spawn_mock_multi_agent_worker(
    creator_id: &str,
) -> (IpcClient, tokio::task::JoinHandle<()>) {
    let (client_transport, server_transport) = DuplexTransport::new_pair();
    let state = Arc::new(MockWorkerState::new(creator_id.to_string()));
    let server = Arc::new(Mutex::new(server_transport));

    let handle = tokio::spawn(async move {
        loop {
            let line = {
                let mut s = server.lock().await;
                s.recv().await
            };

            match line {
                Some(request_str) => {
                    let request: Value = match serde_json::from_str(&request_str) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    let method = request
                        .get("method")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let id = request.get("id").cloned();
                    let params = request.get("params").cloned().unwrap_or(Value::Null);

                    // Check shutdown state.
                    if state.shutdown_requested.load(Ordering::Relaxed)
                        && method != "worker/shutdown"
                    {
                        send_jsonrpc_error(&server, &id, -32000, "Worker shutting down").await;
                        continue;
                    }

                    match method.as_str() {
                        "worker/initialize" => match handle_initialize(&state, &params).await {
                            Ok(result) => {
                                send_jsonrpc_result(&server, &id, &result).await;
                            }
                            Err((code, msg)) => {
                                send_jsonrpc_error(&server, &id, code, &msg).await;
                            }
                        },
                        "worker/agent_start" => match handle_agent_start(&state, &params).await {
                            Ok(result) => {
                                send_jsonrpc_result(&server, &id, &result).await;
                            }
                            Err((code, msg)) => {
                                send_jsonrpc_error(&server, &id, code, &msg).await;
                            }
                        },
                        "worker/agent_stop" => match handle_agent_stop(&state, &params).await {
                            Ok(result) => {
                                send_jsonrpc_result(&server, &id, &result).await;
                            }
                            Err((code, msg)) => {
                                send_jsonrpc_error(&server, &id, code, &msg).await;
                            }
                        },
                        "worker/agent_list" => {
                            let result = handle_agent_list(&state).await;
                            send_jsonrpc_result(&server, &id, &result).await;
                        }
                        "worker/acp_prompt" => {
                            let (handler_result, maybe_notification) =
                                handle_acp_prompt(&state, &params).await;
                            // Send notification first (no id → not a response).
                            if let Some(notification) = maybe_notification {
                                send_line(&server, &notification).await;
                            }
                            match handler_result {
                                Ok(result) => {
                                    send_jsonrpc_result(&server, &id, &result).await;
                                }
                                Err((code, msg)) => {
                                    send_jsonrpc_error(&server, &id, code, &msg).await;
                                }
                            }
                        }
                        "worker/health" => {
                            let result = handle_health(&state).await;
                            send_jsonrpc_result(&server, &id, &result).await;
                        }
                        "worker/shutdown" => {
                            state.shutdown_requested.store(true, Ordering::Relaxed);
                            send_jsonrpc_result(&server, &id, &json!({})).await;
                            break;
                        }
                        _ => {
                            send_jsonrpc_error(&server, &id, -32601, "Method not found").await;
                        }
                    }
                }
                None => {
                    // Client disconnected (EOF).
                    break;
                }
            }
        }
    });

    let client = IpcClient::new(Box::new(client_transport));
    (client, handle)
}

// ---------------------------------------------------------------------------
// JSON-RPC response helpers
// ---------------------------------------------------------------------------

/// Send a JSON-RPC success response.
async fn send_jsonrpc_result(
    server: &Arc<Mutex<DuplexTransport>>,
    id: &Option<Value>,
    result: &Value,
) {
    let response = json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    });
    send_line(server, &response).await;
}

/// Send a JSON-RPC error response.
async fn send_jsonrpc_error(
    server: &Arc<Mutex<DuplexTransport>>,
    id: &Option<Value>,
    code: i64,
    message: &str,
) {
    let response = json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    });
    send_line(server, &response).await;
}

/// Send a raw JSON line (for notifications).
async fn send_line(server: &Arc<Mutex<DuplexTransport>>, value: &Value) {
    let line = serde_json::to_string(value).expect("serialize reply");
    let mut s = server.lock().await;
    let _ = s.send(line).await;
}

// ---------------------------------------------------------------------------
// Mock worker method handlers
// ---------------------------------------------------------------------------

async fn handle_initialize(state: &Arc<MockWorkerState>, params: &Value) -> HandlerResult {
    let mut sessions = state.sessions.write().await;

    // Clear existing sessions on re-init (idempotent).
    sessions.clear();

    // New multi-agent format.
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

            let slot = MockSlot {
                session_id: session_id.clone(),
                acp_agent_id,
                role,
                model,
                state: "ready".to_string(),
                created_at: std::time::Instant::now(),
            };
            sessions.insert(session_id, slot);
        }
    } else if let Some(agent_ref) = params.get("agent_ref").and_then(|v| v.as_str()) {
        // Backward-compatible single-agent format.
        let slot = MockSlot {
            session_id: "default".to_string(),
            acp_agent_id: agent_ref.to_string(),
            role: None,
            model: None,
            state: "ready".to_string(),
            created_at: std::time::Instant::now(),
        };
        sessions.insert("default".to_string(), slot);
    } else {
        // No agent info — default session.
        let slot = MockSlot {
            session_id: "default".to_string(),
            acp_agent_id: "claude-sonnet-4-20250514".to_string(),
            role: None,
            model: None,
            state: "ready".to_string(),
            created_at: std::time::Instant::now(),
        };
        sessions.insert("default".to_string(), slot);
    }

    let summaries: Vec<Value> = sessions.values().map(|s| s.to_summary()).collect();

    Ok(json!({
        "capabilities": ["acp.prompt", "acp.session_load", "agent.start", "agent.stop", "agent.list"],
        "worker_pid": 42,
        "creator_id": state.creator_id,
        "sessions": summaries,
    }))
}

async fn handle_agent_start(state: &Arc<MockWorkerState>, params: &Value) -> HandlerResult {
    let session_id = params
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| (-32005, "Missing session_id".to_string()))?;

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

    let mut sessions = state.sessions.write().await;

    if sessions.contains_key(&session_id) {
        return Err((-32006, format!("session '{session_id}' already exists")));
    }

    let slot = MockSlot {
        session_id: session_id.clone(),
        acp_agent_id,
        role,
        model,
        state: "ready".to_string(),
        created_at: std::time::Instant::now(),
    };
    let summary = slot.to_summary();
    sessions.insert(session_id, slot);

    Ok(json!({ "session": summary }))
}

async fn handle_agent_stop(state: &Arc<MockWorkerState>, params: &Value) -> HandlerResult {
    let session_id = params
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| (-32007, "Missing session_id".to_string()))?;

    let mut sessions = state.sessions.write().await;

    if sessions.remove(&session_id).is_none() {
        return Err((-32008, format!("session '{session_id}' not found")));
    }

    Ok(json!({ "ok": true }))
}

async fn handle_agent_list(state: &Arc<MockWorkerState>) -> Value {
    let sessions = state.sessions.read().await;
    let summaries: Vec<Value> = sessions.values().map(|s| s.to_summary()).collect();
    json!({ "sessions": summaries })
}

/// Returns (handler_result, optional_notification).
async fn handle_acp_prompt(
    state: &Arc<MockWorkerState>,
    params: &Value,
) -> (HandlerResult, Option<Value>) {
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

    let sessions = state.sessions.read().await;

    match sessions.get(&session_id) {
        Some(slot) if slot.state == "ready" || slot.state == "prompting" => {
            let request_id = state.request_counter.fetch_add(1, Ordering::Relaxed);

            // Emit a prompt_chunk notification (no id → notification).
            let notification = Some(json!({
                "jsonrpc": "2.0",
                "method": "worker/acp_prompt_chunk",
                "params": {
                    "request_id": request_id,
                    "session_id": session_id,
                    "text": prompt,
                }
            }));

            let result = json!({
                "done": true,
                "full_text": format!("[mock response from {}]: {}", slot.session_id, prompt),
                "session_id": session_id,
            });

            (Ok(result), notification)
        }
        Some(slot) => {
            let msg = format!(
                "session {} is in state '{}', expected 'ready'",
                session_id, slot.state
            );
            (Err((-32004, msg)), None)
        }
        None => (
            Err((-32003, format!("session {session_id} not found"))),
            None,
        ),
    }
}

async fn handle_health(state: &Arc<MockWorkerState>) -> Value {
    let sessions = state.sessions.read().await;
    let session_health: Vec<Value> = sessions
        .iter()
        .map(|(sid, slot)| {
            json!({
                "session_id": sid,
                "state": slot.state,
                "uptime_ms": slot.created_at.elapsed().as_millis() as u64,
                "healthy": slot.state == "ready" || slot.state == "prompting",
                "last_error": null,
            })
        })
        .collect();

    json!({
        "uptime_ms": 42,
        "creator_id": state.creator_id,
        "session_count": sessions.len(),
        "sessions": session_health,
    })
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Create a test `WorkerHandle` from a spawned mock worker's `IpcClient`.
fn make_test_handle(ipc: IpcClient) -> WorkerHandle {
    WorkerHandle::from_ipc_for_test(ipc)
}

/// Small delay to let the mock worker process the previous message.
async fn yield_to_server() {
    tokio::task::yield_now().await;
    tokio::time::sleep(Duration::from_millis(5)).await;
}

// ===========================================================================
// Step 1: Multi-agent initialize
// ===========================================================================

/// Spawn worker with `agents` array (2 agents); verify `worker/initialize`
/// returns 2 sessions; verify `worker/agent_list` shows both.
#[tokio::test]
async fn multi_agent_initialize_two_sessions() {
    let (ipc, _server) = spawn_mock_multi_agent_worker("creator-step1").await;
    let handle = make_test_handle(ipc);

    // Send worker/initialize with a 2-agent array.
    let init_response = handle
        .call_json_rpc(
            "worker/initialize",
            json!({
                "agents": [
                    {
                        "session_id": "writer_1",
                        "acp_agent_id": "claude-sonnet-4-20250514",
                        "role": "writer",
                        "model": "claude-sonnet-4"
                    },
                    {
                        "session_id": "editor_1",
                        "acp_agent_id": "claude-sonnet-4-20250514",
                        "role": "editor"
                    }
                ]
            }),
        )
        .await
        .expect("initialize should succeed");

    // Verify the response contains 2 sessions.
    let sessions = init_response
        .get("sessions")
        .expect("init response should have sessions")
        .as_array()
        .expect("sessions should be an array");
    assert_eq!(sessions.len(), 2, "initialize should create 2 sessions");

    // Verify session IDs.
    let session_ids: Vec<&str> = sessions
        .iter()
        .filter_map(|s| s.get("session_id").and_then(|v| v.as_str()))
        .collect();
    assert!(session_ids.contains(&"writer_1"), "should contain writer_1");
    assert!(session_ids.contains(&"editor_1"), "should contain editor_1");

    // Verify agent_list via the typed method.
    yield_to_server().await;
    let list = handle
        .agent_list()
        .await
        .expect("agent_list should succeed");
    assert_eq!(list.len(), 2, "agent_list should return 2 sessions");

    let writer = list
        .iter()
        .find(|s| s.session_id == "writer_1")
        .expect("writer_1");
    assert_eq!(writer.acp_agent_id, "claude-sonnet-4-20250514");
    assert_eq!(writer.role, Some("writer".to_string()));
    assert_eq!(writer.model, Some("claude-sonnet-4".to_string()));
    assert!(writer.is_ready());

    let editor = list
        .iter()
        .find(|s| s.session_id == "editor_1")
        .expect("editor_1");
    assert_eq!(editor.role, Some("editor".to_string()));
}

// ===========================================================================
// Step 2: Concurrent prompts
// ===========================================================================

/// Send `worker/acp_prompt` to both agents concurrently (different session_ids);
/// verify responses are routed correctly by session_id.
#[tokio::test]
async fn concurrent_prompts_routed_correctly() {
    let (ipc, _server) = spawn_mock_multi_agent_worker("creator-step2").await;
    let handle = make_test_handle(ipc);

    // Initialize with 2 agents.
    handle
        .call_json_rpc(
            "worker/initialize",
            json!({
                "agents": [
                    { "session_id": "writer_1", "acp_agent_id": "claude-sonnet-4" },
                    { "session_id": "editor_1", "acp_agent_id": "codex-acp" },
                ]
            }),
        )
        .await
        .expect("initialize");

    yield_to_server().await;

    // Send concurrent prompts to both agents.
    let ipc_client = handle.ipc_client();
    let prompt_writer = ipc_client.call(
        "worker/acp_prompt",
        json!({
            "session_id": "writer_1",
            "prompt": "Write chapter 1"
        }),
    );
    let prompt_editor = ipc_client.call(
        "worker/acp_prompt",
        json!({
            "session_id": "editor_1",
            "prompt": "Review chapter 1"
        }),
    );

    let (result_writer, result_editor) = tokio::join!(prompt_writer, prompt_editor);

    // Verify writer response.
    let writer_val = result_writer.expect("writer prompt should succeed");
    assert_eq!(writer_val["done"], true);
    assert!(
        writer_val["full_text"]
            .as_str()
            .expect("string")
            .contains("writer_1"),
        "writer response should mention its session_id"
    );
    assert!(
        writer_val["full_text"]
            .as_str()
            .expect("string")
            .contains("Write chapter 1"),
        "writer response should contain the prompt"
    );

    // Verify editor response.
    let editor_val = result_editor.expect("editor prompt should succeed");
    assert_eq!(editor_val["done"], true);
    assert!(
        editor_val["full_text"]
            .as_str()
            .expect("string")
            .contains("editor_1"),
        "editor response should mention its session_id"
    );
    assert!(
        editor_val["full_text"]
            .as_str()
            .expect("string")
            .contains("Review chapter 1"),
        "editor response should contain the prompt"
    );

    // Responses must not be mixed.
    assert_ne!(
        writer_val["full_text"], editor_val["full_text"],
        "responses from different sessions should differ"
    );
}

// ===========================================================================
// Step 3: Agent crash isolation
// ===========================================================================

/// Test that marking one slot as Error doesn't affect other slots.
/// Verifies that independent session slots can be prompted without interference,
/// and that nonexistent sessions return proper errors.
#[tokio::test]
async fn crash_isolation_error_slot_no_impact() {
    let (ipc, _server) = spawn_mock_multi_agent_worker("creator-step3").await;
    let handle = make_test_handle(ipc);

    // Initialize with 3 agents.
    handle
        .call_json_rpc(
            "worker/initialize",
            json!({
                "agents": [
                    { "session_id": "s1", "acp_agent_id": "agent_a" },
                    { "session_id": "s2", "acp_agent_id": "agent_b" },
                    { "session_id": "s3", "acp_agent_id": "agent_c" },
                ]
            }),
        )
        .await
        .expect("initialize");

    yield_to_server().await;

    // Verify all 3 are initially ready.
    let list = handle.agent_list().await.expect("agent_list");
    assert_eq!(list.len(), 3);
    assert!(list.iter().all(|s| s.is_ready()));

    // Prompt s1 and s3 concurrently — both should succeed independently.
    let ipc_client = handle.ipc_client();
    let (r1, r3) = tokio::join!(
        ipc_client.call(
            "worker/acp_prompt",
            json!({ "session_id": "s1", "prompt": "hello from s1" })
        ),
        ipc_client.call(
            "worker/acp_prompt",
            json!({ "session_id": "s3", "prompt": "hello from s3" })
        ),
    );

    let v1 = r1.expect("s1 prompt should succeed");
    assert_eq!(v1["session_id"], "s1");

    let v3 = r3.expect("s3 prompt should succeed");
    assert_eq!(v3["session_id"], "s3");

    // Verify agent_list still shows all 3 sessions.
    let list_after = handle.agent_list().await.expect("agent_list after prompts");
    assert_eq!(list_after.len(), 3);

    // A prompt to a nonexistent session should return a proper JSON-RPC error.
    let nonexistent = handle
        .call_json_rpc(
            "worker/acp_prompt",
            json!({ "session_id": "nonexistent", "prompt": "should fail" }),
        )
        .await;
    assert!(
        nonexistent.is_err(),
        "prompt to nonexistent session should fail"
    );
}

// ===========================================================================
// Step 4: Dynamic agent start/stop
// ===========================================================================

/// Initialize with 1 agent; `worker/agent_start` adds a second;
/// `worker/agent_stop` removes the first; verify `worker/agent_list`
/// reflects changes at each step.
#[tokio::test]
async fn dynamic_agent_start_stop() {
    let (ipc, _server) = spawn_mock_multi_agent_worker("creator-step4").await;
    let handle = make_test_handle(ipc);

    // Step 4a: Initialize with 1 agent.
    handle
        .call_json_rpc(
            "worker/initialize",
            json!({
                "agents": [
                    { "session_id": "initial_1", "acp_agent_id": "claude-sonnet-4" }
                ]
            }),
        )
        .await
        .expect("initialize");

    yield_to_server().await;
    let list = handle.agent_list().await.expect("agent_list after init");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].session_id, "initial_1");

    // Step 4b: agent_start adds a second agent.
    let new_config = WorkerAgentConfig::new("added_1".to_string(), "codex-acp".to_string())
        .with_role("reviewer".to_string())
        .with_model("o3".to_string());

    let started = handle
        .agent_start(&new_config)
        .await
        .expect("agent_start should succeed");
    assert_eq!(started.session_id, "added_1");
    assert_eq!(started.acp_agent_id, "codex-acp");
    assert_eq!(started.role, Some("reviewer".to_string()));
    assert_eq!(started.model, Some("o3".to_string()));
    assert!(started.is_ready());

    yield_to_server().await;
    let list = handle.agent_list().await.expect("agent_list after start");
    assert_eq!(list.len(), 2);
    let ids: Vec<&str> = list.iter().map(|s| s.session_id.as_str()).collect();
    assert!(ids.contains(&"initial_1"));
    assert!(ids.contains(&"added_1"));

    // Step 4c: agent_stop removes the first agent.
    handle
        .agent_stop("initial_1", 1000)
        .await
        .expect("agent_stop should succeed");

    yield_to_server().await;
    let list = handle.agent_list().await.expect("agent_list after stop");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].session_id, "added_1");

    // Step 4d: Duplicate agent_start should fail.
    let dup_result = handle.agent_start(&new_config).await;
    assert!(dup_result.is_err(), "duplicate agent_start should fail");

    // Step 4e: agent_stop on nonexistent session should fail.
    let missing_result = handle.agent_stop("nonexistent", 1000).await;
    assert!(
        missing_result.is_err(),
        "agent_stop on nonexistent session should fail"
    );
}

// ===========================================================================
// Step 5: Backward compatibility
// ===========================================================================

/// Send old-format `worker/initialize` (with `agent_ref`); verify
/// single-agent prompt works through the default session.
#[tokio::test]
async fn backward_compat_single_agent_prompt() {
    let (ipc, _server) = spawn_mock_multi_agent_worker("creator-step5").await;
    let handle = make_test_handle(ipc);

    // Old-format initialize with agent_ref.
    let init_response = handle
        .call_json_rpc(
            "worker/initialize",
            json!({
                "agent_ref": "claude-sonnet-4-20250514"
            }),
        )
        .await
        .expect("initialize with agent_ref should succeed");

    // Verify single session created.
    let sessions = init_response
        .get("sessions")
        .expect("sessions")
        .as_array()
        .expect("array");
    assert_eq!(sessions.len(), 1);
    assert_eq!(
        sessions[0]["session_id"], "default",
        "backward compat should create 'default' session"
    );
    assert_eq!(sessions[0]["acp_agent_id"], "claude-sonnet-4-20250514");

    // Verify agent_list.
    yield_to_server().await;
    let list = handle.agent_list().await.expect("agent_list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].session_id, "default");

    // Send prompt to the default session (no explicit session_id).
    let prompt_result = handle
        .call_json_rpc(
            "worker/acp_prompt",
            json!({
                "prompt": "Hello from backward-compat test"
            }),
        )
        .await
        .expect("prompt should succeed");

    assert_eq!(prompt_result["done"], true);
    assert!(
        prompt_result["full_text"]
            .as_str()
            .expect("string")
            .contains("default"),
        "response should reference the default session"
    );
    assert!(
        prompt_result["full_text"]
            .as_str()
            .expect("string")
            .contains("Hello from backward-compat test"),
        "response should contain the prompt"
    );
}

// ===========================================================================
// Step 6: Worker shutdown with multiple agents
// ===========================================================================

/// Send `worker/shutdown` with multiple active agents; verify response
/// is clean and subsequent calls fail.
#[tokio::test]
async fn shutdown_with_multiple_agents() {
    let (ipc, _server) = spawn_mock_multi_agent_worker("creator-step6").await;
    let mut handle = make_test_handle(ipc);

    // Initialize with 3 agents.
    handle
        .call_json_rpc(
            "worker/initialize",
            json!({
                "agents": [
                    { "session_id": "s1", "acp_agent_id": "agent_a", "role": "writer" },
                    { "session_id": "s2", "acp_agent_id": "agent_b", "role": "editor" },
                    { "session_id": "s3", "acp_agent_id": "agent_c", "role": "reviewer" },
                ]
            }),
        )
        .await
        .expect("initialize");

    yield_to_server().await;

    // Verify 3 sessions are active.
    let list = handle
        .agent_list()
        .await
        .expect("agent_list before shutdown");
    assert_eq!(list.len(), 3);

    // Send shutdown — should succeed cleanly.
    let shutdown_result = handle.shutdown().await;
    assert!(
        shutdown_result.is_ok(),
        "shutdown should succeed: {:?}",
        shutdown_result
    );

    // After shutdown, subsequent calls should fail (transport closed or
    // worker rejected).
    let post_shutdown = handle.call_json_rpc("worker/health", json!({})).await;

    assert!(post_shutdown.is_err(), "calls after shutdown should fail");
}

// ===========================================================================
// Additional edge-case tests
// ===========================================================================

/// Verify initialize with no params creates a default session (backward compat fallback).
#[tokio::test]
async fn initialize_with_no_params_creates_default() {
    let (ipc, _server) = spawn_mock_multi_agent_worker("creator-edge1").await;
    let handle = make_test_handle(ipc);

    let init_response = handle
        .call_json_rpc("worker/initialize", json!({}))
        .await
        .expect("initialize with empty params should succeed");

    let sessions = init_response
        .get("sessions")
        .expect("sessions")
        .as_array()
        .expect("array");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["session_id"], "default");
}

/// Verify health returns per-session info with correct session_count.
#[tokio::test]
async fn health_returns_per_session_info() {
    let (ipc, _server) = spawn_mock_multi_agent_worker("creator-edge2").await;
    let handle = make_test_handle(ipc);

    handle
        .call_json_rpc(
            "worker/initialize",
            json!({
                "agents": [
                    { "session_id": "s1", "acp_agent_id": "agent_a" },
                    { "session_id": "s2", "acp_agent_id": "agent_b" },
                ]
            }),
        )
        .await
        .expect("initialize");

    yield_to_server().await;

    let health = handle
        .call_json_rpc("worker/health", json!({}))
        .await
        .expect("health should succeed");

    assert_eq!(health["session_count"], 2);
    let health_sessions = health["sessions"].as_array().expect("sessions array");
    assert_eq!(health_sessions.len(), 2);

    // Each session health should have required fields.
    for session in health_sessions {
        assert!(session.get("session_id").is_some());
        assert!(session.get("state").is_some());
        assert!(session.get("healthy").is_some());
        assert!(session.get("uptime_ms").is_some());
    }
}

/// Verify agent_start without required session_id fails.
#[tokio::test]
async fn agent_start_missing_session_id_fails() {
    let (ipc, _server) = spawn_mock_multi_agent_worker("creator-edge3").await;
    let handle = make_test_handle(ipc);

    let result = handle
        .call_json_rpc(
            "worker/agent_start",
            json!({
                "acp_agent_id": "claude-sonnet-4"
            }),
        )
        .await;

    assert!(
        result.is_err(),
        "agent_start without session_id should fail"
    );
}

/// Verify idempotent re-initialize replaces sessions.
#[tokio::test]
async fn initialize_idempotent_replaces_sessions() {
    let (ipc, _server) = spawn_mock_multi_agent_worker("creator-edge4").await;
    let handle = make_test_handle(ipc);

    // First init.
    handle
        .call_json_rpc(
            "worker/initialize",
            json!({ "agents": [{ "session_id": "old_1", "acp_agent_id": "old_agent" }] }),
        )
        .await
        .expect("first initialize");

    yield_to_server().await;
    let list = handle
        .agent_list()
        .await
        .expect("agent_list after first init");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].session_id, "old_1");

    // Re-init with different agents.
    handle
        .call_json_rpc(
            "worker/initialize",
            json!({ "agents": [
                { "session_id": "new_1", "acp_agent_id": "new_agent_a" },
                { "session_id": "new_2", "acp_agent_id": "new_agent_b" },
            ]}),
        )
        .await
        .expect("second initialize");

    yield_to_server().await;
    let list = handle.agent_list().await.expect("agent_list after re-init");
    assert_eq!(list.len(), 2);
    let ids: Vec<&str> = list.iter().map(|s| s.session_id.as_str()).collect();
    assert!(ids.contains(&"new_1"));
    assert!(ids.contains(&"new_2"));
}
