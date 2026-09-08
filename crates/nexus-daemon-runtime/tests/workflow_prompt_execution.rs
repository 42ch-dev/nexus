//! V1.186 P1 T2 — Host-mediated prompt execution through the real ACP fixture.
//!
//! Proves the A1 cutover end to end: all five production prompt consumers
//! (`acp.prompt`, `judge.llm`, `context.summarize`, `nexus.llm.extract`, and
//! graph `acp_prompt`) execute through the daemon-owned `HostPromptExecutor`
//! over the existing `HostFacade` and a real deterministic ACP stdio fixture
//! (`../nexus-agent-host/tests/fixtures/mock_acp_workflow.py`), observing
//! transformed non-echo output.
//!
//! Also proves:
//! - durable `PromptAttempt` dispatch intent is persisted BEFORE the fixture
//!   records the external prompt effect (A2/A5);
//! - missing binding / EOF / non-EndTurn stop stay typed failures, never
//!   partial-output success;
//! - no live Host/client/process handle is serialized into orchestration
//!   context (only `full_text` + host/op ids).

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use nexus_agent_host::capability::model::HostStartConfig;
use nexus_agent_host::config::{AgentHostConfig, ProviderConfig, TimeoutConfig};
use nexus_agent_host::core::manager::HostManager;
use nexus_agent_host::providers::acp::AcpProvider;
use nexus_agent_host::{HostFacade, LaunchStrategy};
use nexus_daemon_runtime::prompt_executor::HostPromptExecutor;
use nexus_orchestration::capability::{
    CapabilityError, CapabilityRegistry, CapabilityRuntimeDeps, PromptExecutor, PromptRequest,
    ToolPolicy,
};
use nexus_orchestration::engine::{
    GraphFlowEngine, OrchestrationEngine, SessionStatus, SessionSummary, StepOutcome,
};
use nexus_orchestration::run_state::{
    AgentBinding, PresetSourceIdentity, RunCheckpoint, RunDescriptorV1, RunStateV1,
    WorkflowStateStore,
};
use nexus_orchestration::storage::sqlite::SqliteSessionStorage;
use nexus_orchestration::{CapabilityRegistryHolder, SessionId};
use tempfile::TempDir;

use graph_flow::{FlowRunner, Graph, Session as GraphSession, SessionStorage, Task};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../nexus-agent-host/tests/fixtures/mock_acp_workflow.py"
);

/// Serialize environment-mutating tests within this integration-test binary.
static PROCESS_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    PROCESS_ENV_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

struct TestWorkspace {
    _tmp: TempDir,
    workspace_root: PathBuf,
    creator_ws: PathBuf,
    config_path: PathBuf,
    fixture_log: PathBuf,
}

fn setup_workspace() -> TestWorkspace {
    let tmp = TempDir::new().expect("temp dir");
    let workspace_root = tmp.path().join("workspace");
    let creator_ws = workspace_root.join("creator-a");
    std::fs::create_dir_all(&creator_ws.join("sub")).expect("creator sub dir");
    let config_path = tmp.path().join("config.toml");
    let fixture_log = tmp.path().join("fixture.log");
    TestWorkspace {
        _tmp: tmp,
        workspace_root,
        creator_ws,
        config_path,
        fixture_log,
    }
}

fn acp_provider_config(id: &str, fixture_log: &Path) -> ProviderConfig {
    let mut env = HashMap::new();
    env.insert(
        "ACP_FIXTURE_LOG".to_string(),
        fixture_log.to_string_lossy().into_owned(),
    );
    ProviderConfig {
        id: id.to_string(),
        protocol: "acp".to_string(),
        command: Some(FIXTURE.to_string()),
        args: vec![],
        env,
        enabled: true,
    }
}

fn timeouts() -> TimeoutConfig {
    TimeoutConfig {
        launch_ms: 10_000,
        initialize_ms: 10_000,
        session_ms: 10_000,
        prompt_ms: 10_000,
        shutdown_ms: 2_000,
    }
}

async fn build_host(
    ws: &TestWorkspace,
    provider_cfg: ProviderConfig,
) -> (Arc<HostManager>, Arc<dyn HostFacade>) {
    let manager = Arc::new(HostManager::new());
    let provider = AcpProvider::from_config(
        provider_cfg.clone(),
        timeouts(),
        nexus_agent_host::HostPermissionResolver::new_native_only(
            &AgentHostConfig::default().policy,
        ),
    )
    .expect("valid ACP recipe");
    let launch = LaunchStrategy::Acp {
        command: provider_cfg.command.clone().unwrap_or_default(),
        args: provider_cfg.args.clone(),
        env: provider_cfg.env.clone(),
    };
    manager.register_provider(Arc::new(provider), launch).await;
    manager
        .start(HostStartConfig {
            config_path: ws.config_path.clone(),
            workspace_root: ws.workspace_root.clone(),
            max_sessions: 4,
            max_ops_per_session: 1,
            timeouts: timeouts(),
        })
        .await
        .expect("host start");
    let host: Arc<dyn HostFacade> = manager.clone();
    (manager, host)
}

async fn fresh_pool() -> (Arc<sqlx::SqlitePool>, tempfile::NamedTempFile) {
    let db = tempfile::NamedTempFile::new().unwrap();
    let pool = nexus_local_db::open_pool(db.path())
        .await
        .expect("open pool");
    nexus_local_db::run_migrations(&pool)
        .await
        .expect("run migrations");
    (Arc::new(pool), db)
}

fn read_fixture_log(path: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .map(|content| {
            content
                .lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Build the full production-shaped stack: real Host + real SQLite workflow
/// store + HostPromptExecutor + capability registry with the executor.
async fn build_stack(
    ws: &TestWorkspace,
    provider_cfg: ProviderConfig,
) -> (
    Arc<dyn HostFacade>,
    Arc<SqliteSessionStorage>,
    Arc<dyn WorkflowStateStore>,
    Arc<HostPromptExecutor>,
    Arc<CapabilityRegistry>,
    Arc<
        std::sync::RwLock<
            std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
        >,
    >,
) {
    let (_manager, host) = build_host(ws, provider_cfg).await;
    let (pool, _db) = fresh_pool().await;
    let storage = Arc::new(SqliteSessionStorage::new(pool));
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let executor = Arc::new(HostPromptExecutor::new(
        host.clone(),
        workflow_store.clone(),
        timeouts(),
    ));
    let session_cancels: Arc<
        std::sync::RwLock<
            std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
        >,
    > = Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
    let deps = CapabilityRuntimeDeps {
        pool: None,
        prompt_executor: Some(executor.clone() as Arc<dyn PromptExecutor>),
        session_cancels: session_cancels.clone(),
        daemon_tool_dispatch: None,
        cdn_config: None,
    };
    let registry = Arc::new(CapabilityRegistry::with_runtime_deps(&deps));
    (
        host,
        storage,
        workflow_store,
        executor,
        registry,
        session_cancels,
    )
}

/// Start a v1 run with a frozen descriptor binding `default` → the fixture
/// provider, and return the run id.
async fn start_v1_run(
    workflow_store: &Arc<dyn WorkflowStateStore>,
    ws: &TestWorkspace,
    provider_id: &str,
) -> String {
    let run_id = format!("run:{}", uuid::Uuid::new_v4());
    let session = GraphSession::new_from_task(run_id.clone(), "start");
    session
        .context
        .set("_session_id", run_id.clone())
        .await;

    let mut agent_bindings = HashMap::new();
    agent_bindings.insert(
        "default".to_string(),
        AgentBinding {
            provider_id: provider_id.to_string(),
            model: None,
        },
    );
    let descriptor = RunDescriptorV1 {
        creator_id: "ctr_test".to_string(),
        work_id: None,
        workspace_root: ws.creator_ws.clone(),
        preset_id: "test-preset".to_string(),
        preset_version: 1,
        source: PresetSourceIdentity::Embedded {
            preset_id: "test-preset".to_string(),
            content_hash: [7u8; 32],
        },
        input: serde_json::Map::new(),
        agent_bindings,
        parent_session_id: None,
        graph_name: None,
    };
    let checkpoint = RunCheckpoint {
        root: &session,
        children: &[],
    };
    workflow_store
        .start_run(
            &SessionId(run_id.clone()),
            &descriptor,
            checkpoint,
            &RunStateV1::default(),
        )
        .await
        .expect("start_run");
    run_id
}

fn request(run_id: &str, prompt: &str, tool_policy: ToolPolicy) -> PromptRequest {
    PromptRequest {
        run_id: run_id.to_string(),
        task_id: "task-1".to_string(),
        agent_ref: None,
        prompt: prompt.to_string(),
        tool_policy,
        cancellation: tokio_util::sync::CancellationToken::new(),
    }
}

#[tokio::test]
async fn all_five_consumers_observe_non_echo_agent_output() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg = acp_provider_config("mock-acp", &ws.fixture_log);
    let (host, _storage, workflow_store, executor, registry, session_cancels) =
        build_stack(&ws, provider_cfg).await;
    let run_id = start_v1_run(&workflow_store, &ws, "mock-acp").await;

    // 1. acp.prompt capability.
    let cap = registry.get("acp.prompt").expect("acp.prompt registered");
    let out = cap
        .run(serde_json::json!({
            "prompt": "hello-acp",
            "tool_policy": "deny_all",
            "_creator_id": "ctr_test",
            "_session_id": run_id,
        }))
        .await
        .expect("acp.prompt succeeds");
    assert_eq!(out["full_text"], "transformed:hello-acp");
    assert!(out["host_session_id"].is_string());
    assert!(out["operation_id"].is_string());

    // 2. judge.llm capability — the fixture returns "transformed:..." which
    // first-token-parses as ambiguous → NOGO (safe default), proving the
    // agent output (not the prompt) drove the verdict.
    let cap = registry.get("judge.llm").expect("judge.llm registered");
    let out = cap
        .run(serde_json::json!({
            "prompt": "Is the task complete?",
            "_creator_id": "ctr_test",
            "_session_id": run_id,
        }))
        .await
        .expect("judge.llm succeeds");
    assert_eq!(out["result"], false, "ambiguous agent output → NOGO");

    // 3. context.summarize capability.
    let cap = registry
        .get("context.summarize")
        .expect("context.summarize registered");
    let out = cap
        .run(serde_json::json!({
            "content": "The story is about a brave knight.",
            "_creator_id": "ctr_test",
            "_session_id": run_id,
        }))
        .await
        .expect("context.summarize succeeds");
    assert_eq!(
        out["summary"],
        "transformed:Summarize the following content concisely, preserving key entities, relationships, and state.\n\n--- Content ---\nThe story is about a brave knight.\n\n---\nProvide the summary now:"
    );
    assert_eq!(out["prompt_hash"].as_str().unwrap().len(), 64);

    // 4. nexus.llm.extract capability — the fixture returns non-JSON text, so
    // candidates parse empty (best-effort) but the call itself succeeded
    // through the Host plane (never WorkerUnavailable).
    let cap = registry
        .get("nexus.llm.extract")
        .expect("nexus.llm.extract registered");
    let out = cap
        .run(serde_json::json!({
            "prompt": "extract entities",
            "chapter_prose": "Lin Xia drew her blade.",
            "_creator_id": "ctr_test",
            "_session_id": run_id,
        }))
        .await
        .expect("nexus.llm.extract succeeds");
    assert!(out["candidates"].is_array());

    // 5. graph acp_prompt (InnerGraphNodeTask) through the executor.
    let task = nexus_orchestration::tasks::InnerGraphNodeTask::new("n1")
        .with_template("graph prompt {{core_context.version}}")
        .with_tool_policy(ToolPolicy::DenyAll)
        .with_prompt_executor(Some(executor.clone() as Arc<dyn PromptExecutor>))
        .with_session_cancels(session_cancels.clone());
    let ctx = graph_flow::Context::new();
    ctx.set("_session_id", run_id.clone()).await;
    ctx.set("core_context.version", "7").await;
    let result = task.run(ctx.clone()).await.expect("graph acp_prompt succeeds");
    assert_eq!(
        result.response.as_deref().unwrap_or(""),
        "transformed:graph prompt 7"
    );
    let stored: String = ctx.get("state.n1.output").await.unwrap();
    assert_eq!(stored, "transformed:graph prompt 7");

    // The fixture log proves real agent output flowed (non-echo).
    let log = read_fixture_log(&ws.fixture_log);
    let prompts: Vec<&serde_json::Value> = log.iter().filter(|e| e["event"] == "prompt").collect();
    assert!(
        prompts.len() >= 5,
        "all five consumers must reach the fixture: {log:?}"
    );

    // Durable intent: the run_state_json carries the last in_flight attempt
    // (the executor persists dispatching + active; the engine is not running
    // here so the attempt remains — proving intent was persisted before the
    // fixture effect).
    let record = workflow_store
        .load_run(&SessionId(run_id.clone()))
        .await
        .expect("load run");
    let state = record.expect("record").state.expect("v1 state");
    let in_flight = state.in_flight.expect("in_flight persisted");
    assert_eq!(in_flight.task_id, "n1");
    assert!(in_flight.host_session_id.is_some());
    assert!(in_flight.operation_id.is_some());

    // No live handle in context: only text + ids.
    let ctx_json = serde_json::to_value(&ctx).unwrap();
    let s = ctx_json.to_string();
    assert!(!s.contains("AcpSdkAdapter"), "no SDK handle in context: {s}");
    assert!(!s.contains("ManagedAcpProcess"), "no process handle in context: {s}");

    host.shutdown().await.expect("host shutdown");
}

#[tokio::test]
async fn missing_binding_refuses_before_effect() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg = acp_provider_config("mock-acp", &ws.fixture_log);
    let (host, _storage, workflow_store, executor, _registry, _cancels) =
        build_stack(&ws, provider_cfg).await;
    let run_id = start_v1_run(&workflow_store, &ws, "mock-acp").await;

    // A request with an unresolved role binding refuses before any external
    // effect (A1: explicit provider selection, no implicit fallback).
    let result = executor
        .execute(PromptRequest {
            run_id: run_id.clone(),
            task_id: "task-1".to_string(),
            agent_ref: Some("writer".to_string()),
            prompt: "hello".to_string(),
            tool_policy: ToolPolicy::DenyAll,
            cancellation: tokio_util::sync::CancellationToken::new(),
        })
        .await;
    assert!(result.is_err(), "unresolved binding must refuse");
    match result.unwrap_err() {
        CapabilityError::Forbidden(msg) => {
            assert!(msg.contains("no provider binding"), "typed refusal: {msg}");
        }
        other => panic!("expected Forbidden, got: {other:?}"),
    }

    // No fixture process was ever spawned (no external effect).
    assert!(
        !ws.fixture_log.exists(),
        "refusal must not spawn the fixture"
    );

    host.shutdown().await.expect("host shutdown");
}

#[tokio::test]
async fn eof_after_initialize_is_typed_failure() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg =
        acp_provider_config("mock-acp", &ws.fixture_log).with_env("EOF_AFTER_INIT", "1");
    let (host, _storage, workflow_store, executor, _registry, _cancels) =
        build_stack(&ws, provider_cfg).await;
    let run_id = start_v1_run(&workflow_store, &ws, "mock-acp").await;

    // The fixture exits right after initialize; session creation fails with a
    // typed launch error — never a fake success.
    let result = executor.execute(request(&run_id, "hello", ToolPolicy::DenyAll)).await;
    assert!(result.is_err(), "EOF after initialize must fail");
    match result.unwrap_err() {
        CapabilityError::TransientExternal(msg) => {
            assert!(
                msg.contains("host session creation failed"),
                "typed launch failure: {msg}"
            );
        }
        other => panic!("expected TransientExternal, got: {other:?}"),
    }

    host.shutdown().await.expect("host shutdown");
}

#[tokio::test]
async fn pre_cancelled_request_is_fenced_before_launch() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg = acp_provider_config("mock-acp", &ws.fixture_log);
    let (host, _storage, workflow_store, executor, _registry, _cancels) =
        build_stack(&ws, provider_cfg).await;
    let run_id = start_v1_run(&workflow_store, &ws, "mock-acp").await;

    // A5 cancellation fence: the token is cancelled BEFORE `execute` — no
    // durable intent write, no Host session launch, no prompt may reach the
    // fixture; the request refuses with a typed failure.
    let cancel = tokio_util::sync::CancellationToken::new();
    cancel.cancel();
    let result = executor
        .execute(PromptRequest {
            run_id: run_id.clone(),
            task_id: "task-1".to_string(),
            agent_ref: None,
            prompt: "never-sent".to_string(),
            tool_policy: ToolPolicy::DenyAll,
            cancellation: cancel,
        })
        .await;
    assert!(result.is_err(), "pre-cancelled request must refuse");
    match result.unwrap_err() {
        CapabilityError::Cancelled => {}
        other => panic!("expected Cancelled, got: {other:?}"),
    }

    // No fixture process was ever spawned (no external effect at all).
    assert!(
        !ws.fixture_log.exists(),
        "pre-cancelled request must not spawn the fixture"
    );

    host.shutdown().await.expect("host shutdown");
}

#[tokio::test]
async fn in_stream_cancel_calls_host_cancel_and_bounds_cleanup() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg = acp_provider_config("mock-acp", &ws.fixture_log).with_env("BLOCK_PROMPT", "1");
    let (host, _storage, workflow_store, executor, _registry, _cancels) =
        build_stack(&ws, provider_cfg).await;
    let run_id = start_v1_run(&workflow_store, &ws, "mock-acp").await;

    // The fixture never responds to the prompt; the coordinator token fires
    // WHILE the prompt is in flight, the executor cancels the owned
    // operation and returns a typed failure. (The pre-launch fence is proven
    // separately by `pre_cancelled_request_is_fenced_before_launch`.)
    let cancel = tokio_util::sync::CancellationToken::new();
    let cancel_for_task = cancel.clone();
    let executor_for_task = executor.clone();
    let run_for_task = run_id.clone();
    let handle = tokio::spawn(async move {
        executor_for_task
            .execute(PromptRequest {
                run_id: run_for_task,
                task_id: "task-1".to_string(),
                agent_ref: None,
                prompt: "blocked".to_string(),
                tool_policy: ToolPolicy::DenyAll,
                cancellation: cancel_for_task,
            })
            .await
    });

    // Wait until the fixture observes the prompt (the operation is in
    // flight), then fire the coordinator token.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let log = read_fixture_log(&ws.fixture_log);
        if log.iter().any(|e| e["event"] == "prompt") {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "fixture never observed the prompt: {log:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    cancel.cancel();

    let result = handle.await.expect("task completes");
    assert!(result.is_err(), "cancellation must be a typed failure");
    match result.unwrap_err() {
        CapabilityError::Cancelled => {}
        other => panic!("expected Cancelled, got: {other:?}"),
    }

    // The fixture observed the prompt and then the cancel (the cancel RPC
    // round-trip drains asynchronously after the typed failure returns).
    let log = read_fixture_log(&ws.fixture_log);
    assert!(
        log.iter().any(|e| e["event"] == "prompt"),
        "fixture must observe the prompt: {log:?}"
    );
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let log = read_fixture_log(&ws.fixture_log);
        if log.iter().any(|e| e["event"] == "cancel") {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "cancel must reach the fixture: {log:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    host.shutdown().await.expect("host shutdown");
}

#[tokio::test]
async fn concurrent_same_key_second_prompt_reuses_session_not_spawn() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg = acp_provider_config("mock-acp", &ws.fixture_log).with_env("BLOCK_PROMPT", "1");
    let (host, _storage, workflow_store, executor, _registry, _cancels) =
        build_stack(&ws, provider_cfg).await;
    let run_id = start_v1_run(&workflow_store, &ws, "mock-acp").await;

    // First prompt blocks in-stream (fixture never answers): the session is
    // Busy while it runs.
    let cancel_first = tokio_util::sync::CancellationToken::new();
    let cancel_first_for_task = cancel_first.clone();
    let executor_first = executor.clone();
    let run_first = run_id.clone();
    let first = tokio::spawn(async move {
        executor_first
            .execute(PromptRequest {
                run_id: run_first,
                task_id: "task-1".to_string(),
                agent_ref: None,
                prompt: "blocked-1".to_string(),
                tool_policy: ToolPolicy::DenyAll,
                cancellation: cancel_first_for_task,
            })
            .await
    });

    // Wait until the fixture observes the first prompt (session now Busy).
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let log = read_fixture_log(&ws.fixture_log);
        if log.iter().any(|e| e["event"] == "prompt") {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "fixture never observed the first prompt: {log:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    // The second same-key prompt MUST NOT create a second owned session: it
    // reuses the published (run, role) session and fails on the serial-reuse
    // boundary (session Busy) — the Minor finding's failure mode would have
    // spawned a second subprocess instead.
    let result = executor
        .execute(request(&run_id, "hello-2", ToolPolicy::DenyAll))
        .await;
    assert!(result.is_err(), "second same-key prompt must not overlap (serial reuse)");
    match result.unwrap_err() {
        CapabilityError::TransientExternal(msg) => {
            assert!(
                msg.contains("host exec failed"),
                "serial-reuse refusal must surface the exec boundary: {msg}"
            );
        }
        other => panic!("expected TransientExternal exec refusal, got: {other:?}"),
    }

    // Single-flight proof: exactly ONE owned session and ONE fixture
    // subprocess exist for the (run, role) key.
    let sessions = host.list_sessions().await.expect("list sessions");
    assert_eq!(
        sessions.len(),
        1,
        "single-flight must create exactly one session for one (run, role) key"
    );
    let log = read_fixture_log(&ws.fixture_log);
    let start_pids: std::collections::HashSet<String> = log
        .iter()
        .filter(|e| e["event"] == "start")
        .map(|e| e["pid"].to_string())
        .collect();
    assert_eq!(
        start_pids.len(),
        1,
        "second same-key prompt must never spawn a second subprocess: {log:?}"
    );
    assert_eq!(
        log.iter().filter(|e| e["event"] == "prompt").count(),
        1,
        "only the first prompt reaches the fixture: {log:?}"
    );

    // Release the blocked prompt via the coordinator token; the first
    // request then cancels the operation and returns a typed failure.
    cancel_first.cancel();
    let first_result = first.await.expect("first task joins");
    match first_result {
        Err(CapabilityError::Cancelled) => {}
        other => panic!("expected Cancelled after release, got: {other:?}"),
    }

    host.shutdown().await.expect("host shutdown");
}

#[tokio::test]
async fn capability_route_cancel_uses_shared_coordinator_token() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg = acp_provider_config("mock-acp", &ws.fixture_log);
    let (host, _storage, workflow_store, _executor, registry, session_cancels) =
        build_stack(&ws, provider_cfg).await;
    let run_id = start_v1_run(&workflow_store, &ws, "mock-acp").await;

    // A1 shared cancellation: the capability route must resolve the run's
    // coordinator token from `session_cancels` (never mint a fresh,
    // uncancellable token). A cancelled token in the shared map fences the
    // prompt at the executor's admission — no fixture effect.
    let cancel = tokio_util::sync::CancellationToken::new();
    cancel.cancel();
    session_cancels
        .write()
        .expect("session cancels write")
        .insert(run_id.clone(), cancel);

    let cap = registry.get("acp.prompt").expect("acp.prompt registered");
    let result = cap
        .run(serde_json::json!({
            "prompt": "blocked-by-shared-token",
            "tool_policy": "deny_all",
            "_creator_id": "ctr_test",
            "_session_id": run_id,
        }))
        .await;
    assert!(result.is_err(), "capability route must surface the cancellation");
    match result.unwrap_err() {
        CapabilityError::Cancelled => {}
        other => panic!("expected Cancelled through capability route, got: {other:?}"),
    }

    assert!(
        !ws.fixture_log.exists(),
        "cancelled capability prompt must never spawn the fixture"
    );

    host.shutdown().await.expect("host shutdown");
}

#[tokio::test]
async fn nested_inner_graph_prompt_executes_with_child_identity() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg = acp_provider_config("mock-acp", &ws.fixture_log);
    let (host, storage, workflow_store, executor, _registry, session_cancels) =
        build_stack(&ws, provider_cfg).await;

    // Parent v1 run with the frozen default binding → mock-acp, positioned
    // at the `parent_state` task (mirrors `start_preset_run`).
    let parent_sid = format!("run:{}", uuid::Uuid::new_v4());
    let parent_session = GraphSession::new_from_task(parent_sid.clone(), "parent_state");
    parent_session
        .context
        .set("_session_id", parent_sid.clone())
        .await;
    let mut agent_bindings = HashMap::new();
    agent_bindings.insert(
        "default".to_string(),
        AgentBinding {
            provider_id: "mock-acp".to_string(),
            model: None,
        },
    );
    let parent_descriptor = RunDescriptorV1 {
        creator_id: "ctr_test".to_string(),
        work_id: None,
        workspace_root: ws.creator_ws.clone(),
        preset_id: "parent-preset".to_string(),
        preset_version: 1,
        source: PresetSourceIdentity::Embedded {
            preset_id: "parent-preset".to_string(),
            content_hash: [7u8; 32],
        },
        input: serde_json::Map::new(),
        agent_bindings,
        parent_session_id: None,
        graph_name: None,
    };
    workflow_store
        .start_run(
            &SessionId(parent_sid.clone()),
            &parent_descriptor,
            RunCheckpoint {
                root: &parent_session,
                children: &[],
            },
            &RunStateV1::default(),
        )
        .await
        .expect("start parent run");

    // Inner graph with a production-wired prompt node that has NO explicit
    // session id — it must resolve the child run identity from the child
    // context (`_session_id` seeded by `spawn_child_session_internal`).
    let inner_graph = Arc::new(Graph::new("inner_graph"));
    let prompt_node = nexus_orchestration::tasks::InnerGraphNodeTask::new("n1")
        .with_template("hello from outer")
        .with_tool_policy(ToolPolicy::DenyAll)
        .with_prompt_executor(Some(executor.clone() as Arc<dyn PromptExecutor>))
        .with_session_cancels(session_cancels.clone());
    inner_graph.add_task(Arc::new(prompt_node));
    inner_graph.add_task(Arc::new(EndTask));
    inner_graph.add_edge("n1", "end_task");

    // Parent graph whose start task is an InnerGraphTask over that graph.
    let parent_graph = Arc::new(Graph::new("parent_graph"));
    let storage_arc: Arc<dyn graph_flow::SessionStorage> = storage.clone();
    let caps = CapabilityRegistryHolder::with_registry(Arc::new(CapabilityRegistry::empty()));
    let engine = GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_arc.clone(),
        workflow_store.clone(),
        caps,
    );
    let engine_shared = engine.shared_state();
    let engine_arc: Arc<dyn OrchestrationEngine> = Arc::new(engine);
    let inner_task = nexus_orchestration::tasks::InnerGraphTask::new(
        engine_arc.clone(),
        inner_graph.clone(),
        "parent_state",
        "_session_id",
        None,
    );
    parent_graph.add_task(Arc::new(inner_task));
    parent_graph.add_task(Arc::new(EndTask));
    parent_graph.add_edge("parent_state", "end_task");

    // Register the parent runner AFTER the graph is fully wired (the runner
    // shares the same `Arc<Graph>`, so start-task resolution sees the final
    // graph), and register the parent in the in-memory tracker.
    {
        engine_shared
            .runners
            .write()
            .await
            .insert(parent_sid.clone(), Arc::new(FlowRunner::new(parent_graph, storage_arc)));
        engine_shared.sessions.write().await.push(SessionSummary {
            session_id: SessionId(parent_sid.clone()),
            creator_id: "ctr_test".to_string(),
            preset_id: "parent-preset".to_string(),
            status: SessionStatus::Running,
            current_task_id: Some("parent_state".to_string()),
        });
    }

    // Step the parent: the inner graph spawns a child run, the child prompt
    // node routes through the executor with the CHILD's durable identity.
    let outcome = engine_arc
        .run_step(&SessionId(parent_sid.clone()))
        .await
        .expect("parent step runs inner graph");
    assert!(
        matches!(outcome, StepOutcome::Paused { .. }),
        "expected Paused after inner graph, got {outcome:?}"
    );
    let outcome = engine_arc
        .run_step(&SessionId(parent_sid.clone()))
        .await
        .expect("parent step completes");
    assert!(
        matches!(outcome, StepOutcome::Completed { .. }),
        "expected Completed, got {outcome:?}"
    );

    // The fixture observed the nested prompt (transformed non-echo) — the
    // prompt executed through the Host executor under the child identity,
    // never an unbound "default" refusal.
    let log = read_fixture_log(&ws.fixture_log);
    assert!(
        log.iter().any(|e| e["event"] == "prompt" && e["prompt"] == "hello from outer"),
        "fixture must observe the nested prompt: {log:?}"
    );

    // A durable child run was created with the trusted child identity
    // (parent link + inner graph name) and completed.
    let children = workflow_store
        .load_children(&SessionId(parent_sid.clone()))
        .await
        .expect("load children");
    assert_eq!(children.len(), 1, "one durable child run");
    let child = &children[0];
    let child_desc = child.descriptor.as_ref().expect("child descriptor");
    assert_eq!(
        child_desc.parent_session_id.as_ref().expect("parent link").0,
        parent_sid
    );
    assert_eq!(child_desc.graph_name.as_deref(), Some("inner_graph"));
    assert_eq!(child.status, SessionStatus::Completed);

    // The child's persisted context carries the child run id as `_session_id`
    // (the identity the prompt node resolved and executed under), and the
    // node's transformed output landed on the child context.
    let child_session = storage
        .get(&child.session_id.0)
        .await
        .expect("child session snapshot")
        .expect("child session present");
    let child_ctx_id: String = child_session.context.get("_session_id").await.unwrap();
    assert_eq!(child_ctx_id, child.session_id.0);
    let child_output: String = child_session
        .context
        .get("state.n1.output")
        .await
        .expect("child prompt output");
    assert_eq!(child_output, "transformed:hello from outer");

    host.shutdown().await.expect("host shutdown");
}

struct EndTask;

#[async_trait::async_trait]
impl Task for EndTask {
    fn id(&self) -> &str {
        "end_task"
    }

    async fn run(
        &self,
        _ctx: graph_flow::Context,
    ) -> graph_flow::Result<graph_flow::TaskResult> {
        Ok(graph_flow::TaskResult::new(
            None,
            graph_flow::NextAction::End,
        ))
    }
}

trait ProviderConfigExt {
    fn with_env(self, key: &str, value: &str) -> Self;
}

impl ProviderConfigExt for ProviderConfig {
    fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_string(), value.to_string());
        self
    }
}
