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
#![allow(clippy::await_holding_lock)] // tests hold PROCESS_ENV_LOCK (std sync mutex) across awaits to serialize env mutation

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use nexus_agent_host::capability::model::{HostStartConfig, SessionOwner};
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

use graph_flow::{FlowRunner, Session as GraphSession, SessionStorage, Task};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../nexus-agent-host/tests/fixtures/mock_acp_workflow.py"
);

/// Serialize environment-mutating tests within this integration-test binary.
static PROCESS_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    PROCESS_ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct TestWorkspace {
    _tmp: TempDir,
    workspace_root: PathBuf,
    creator_ws: PathBuf,
    config_path: PathBuf,
    fixture_log: PathBuf,
    /// Keepalive for the SQLite workflow-store file: the pool holds the
    /// path open lazily, so the backing temp file must outlive every test.
    db_file: tempfile::NamedTempFile,
}

fn setup_workspace() -> TestWorkspace {
    let tmp = TempDir::new().expect("temp dir");
    let workspace_root = tmp.path().join("workspace");
    let creator_ws = workspace_root.join("creator-a");
    std::fs::create_dir_all(creator_ws.join("sub")).expect("creator sub dir");
    let config_path = tmp.path().join("config.toml");
    let fixture_log = tmp.path().join("fixture.log");
    let db_file = tempfile::NamedTempFile::new().expect("db temp file");
    TestWorkspace {
        _tmp: tmp,
        workspace_root,
        creator_ws,
        config_path,
        fixture_log,
        db_file,
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

const fn timeouts() -> TimeoutConfig {
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
            host_config: None,
            // Verified probe owner bound to the fixture workspace boundary:
            // the ready-path journeys below admit real sessions, which require
            // a probed-available provider. Without an owner every entry stays
            // unavailable by contract (see the no-context case in the manager
            // tests, which asserts unavailability).
            probe_owner: Some(SessionOwner {
                creator_id: "ctr_probe".to_string(),
                workspace_root: ws.workspace_root.clone(),
                orchestration_run_id: None,
            }),
        })
        .await
        .expect("host start");
    let host: Arc<dyn HostFacade> = manager.clone();
    (manager, host)
}

/// Start events for SESSION launches only (the readiness probe is excluded).
///
/// The host runs one bounded ACP probe in the boundary root before any session
/// exists, so per-session spawn assertions must not count that probe child.
/// Assert the refusal produced NO session-level external effect.
///
/// The readiness probe legitimately spawns once in the boundary root at host
/// start, so "no external effect" for a refused request means: no SESSION
/// launch and no prompt ever reached the fixture — a strictly stronger check
/// than the old "the log file does not exist".
fn assert_no_session_effect(ws: &TestWorkspace, context: &str) {
    let log = read_fixture_log(&ws.fixture_log);
    assert!(
        session_start_events(&log, ws).is_empty(),
        "{context}: no session launch may be spawned, log: {log:?}"
    );
    assert_eq!(
        log.iter().filter(|e| e["event"] == "prompt").count(),
        0,
        "{context}: no prompt may reach the fixture, log: {log:?}"
    );
}

fn session_start_events<'a>(
    log: &'a [serde_json::Value],
    ws: &TestWorkspace,
) -> Vec<&'a serde_json::Value> {
    let probe_cwd = ws
        .workspace_root
        .canonicalize()
        .unwrap_or_else(|_| ws.workspace_root.clone());
    log.iter()
        .filter(|e| e["event"] == "start")
        .filter(|e| {
            e["cwd"]
                .as_str()
                .is_none_or(|cwd| Path::new(cwd) != probe_cwd.as_path())
        })
        .collect()
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
/// store + `HostPromptExecutor` + capability registry with the executor.
async fn build_stack(
    ws: &TestWorkspace,
    provider_cfg: ProviderConfig,
) -> (
    Arc<dyn HostFacade>,
    Arc<SqliteSessionStorage>,
    Arc<dyn WorkflowStateStore>,
    Arc<HostPromptExecutor>,
    Arc<CapabilityRegistry>,
    Arc<std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>>,
) {
    let (_manager, host) = build_host(ws, provider_cfg).await;
    let pool = Arc::new(
        nexus_local_db::open_pool(ws.db_file.path())
            .await
            .expect("open pool"),
    );
    nexus_local_db::run_migrations(&pool)
        .await
        .expect("run migrations");
    let storage = Arc::new(SqliteSessionStorage::new(pool));
    let workflow_store: Arc<dyn WorkflowStateStore> = storage.clone();
    let executor = Arc::new(HostPromptExecutor::new(
        host.clone(),
        workflow_store.clone(),
        timeouts(),
    ));
    let session_cancels: Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    > = Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
    let deps = CapabilityRuntimeDeps {
        pool: None,
        prompt_executor: Some(executor.clone() as Arc<dyn PromptExecutor>),
        session_cancels: session_cancels.clone(),
        daemon_tool_dispatch: None,
        cdn_config: None,
        workspace_executor: None,
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
/// provider, and return the run id. The run's coordinator cancellation token
/// is registered in `session_cancels` at admission (the production engine
/// registrars do the same; without a registered token every prompt consumer
/// fails closed with `CancellationUnavailable`).
async fn start_v1_run(
    workflow_store: &Arc<dyn WorkflowStateStore>,
    ws: &TestWorkspace,
    provider_id: &str,
    session_cancels: &Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    >,
) -> String {
    let run_id = format!("run:{}", uuid::Uuid::new_v4());
    session_cancels
        .write()
        .expect("session cancels write")
        .insert(run_id.clone(), tokio_util::sync::CancellationToken::new());
    let session = GraphSession::new_from_task(run_id.clone(), "start");
    session.context.set("_session_id", run_id.clone()).unwrap();

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
#[allow(clippy::too_many_lines)] // all five consumers asserted in one integration scenario
async fn all_five_consumers_observe_non_echo_agent_output() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg = acp_provider_config("mock-acp", &ws.fixture_log);
    let (host, _storage, workflow_store, executor, registry, session_cancels) =
        build_stack(&ws, provider_cfg).await;
    let run_id = start_v1_run(&workflow_store, &ws, "mock-acp", &session_cancels).await;

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
    ctx.set("_session_id", run_id.clone()).unwrap();
    ctx.set("core_context.version", "7").unwrap();
    let result = task
        .run(ctx.clone())
        .await
        .expect("graph acp_prompt succeeds");
    assert_eq!(
        result.response.as_deref().unwrap_or(""),
        "transformed:graph prompt 7"
    );
    let stored: String = ctx.get("state.n1.output").unwrap();
    assert_eq!(stored, "transformed:graph prompt 7");

    // The fixture log proves real agent output flowed (non-echo).
    let log = read_fixture_log(&ws.fixture_log);
    assert!(
        log.iter().filter(|e| e["event"] == "prompt").count() >= 5,
        "all five consumers must reach the fixture: {log:?}"
    );

    // Durable intent: the executor persists Dispatching BEFORE the external
    // Host effect and Active once the Host ids are known, then clears the
    // marker at successful terminal (the operation completed and the result
    // was delivered). Mid-flight presence is proven deterministically by
    // `concurrent_same_key_second_prompt_reuses_session_not_spawn` (marker
    // naming the owning operation while the prompt is in flight) and
    // `cancellation_after_active_before_exec_no_effect_and_no_session_leak`
    // (Dispatching visible before the fence fires); here we assert the
    // post-terminal contract: no stale in_flight survives a completed prompt.
    let record = workflow_store
        .load_run(&SessionId(run_id.clone()))
        .await
        .expect("load run");
    let state = record.expect("record").state.expect("v1 state");
    assert!(
        state.in_flight.is_none(),
        "a successfully completed prompt must not leave a durable in_flight marker"
    );

    // No live handle in context: only text + ids.
    let ctx_json = serde_json::to_value(&ctx).unwrap();
    let s = ctx_json.to_string();
    assert!(
        !s.contains("AcpSdkAdapter"),
        "no SDK handle in context: {s}"
    );
    assert!(
        !s.contains("ManagedAcpProcess"),
        "no process handle in context: {s}"
    );

    host.shutdown().await.expect("host shutdown");
}

#[tokio::test]
async fn missing_binding_refuses_before_effect() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg = acp_provider_config("mock-acp", &ws.fixture_log);
    let (host, _storage, workflow_store, executor, _registry, session_cancels) =
        build_stack(&ws, provider_cfg).await;
    let run_id = start_v1_run(&workflow_store, &ws, "mock-acp", &session_cancels).await;

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
    assert_no_session_effect(&ws, "refusal must not spawn the fixture");

    host.shutdown().await.expect("host shutdown");
}

#[tokio::test]
async fn eof_after_initialize_is_typed_failure() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg =
        // Run 1 is the bounded readiness probe (passes); run 2 is the session
        // launch that must fail — post-ready launch failure, not a broken recipe.
        acp_provider_config("mock-acp", &ws.fixture_log).with_env("EOF_AFTER_INIT_FROM_RUN", "2");
    let (host, _storage, workflow_store, executor, _registry, session_cancels) =
        build_stack(&ws, provider_cfg).await;
    let run_id = start_v1_run(&workflow_store, &ws, "mock-acp", &session_cancels).await;

    // The fixture exits right after initialize; session creation fails with a
    // typed launch error — never a fake success.
    let result = executor
        .execute(request(&run_id, "hello", ToolPolicy::DenyAll))
        .await;
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
    let (host, _storage, workflow_store, executor, _registry, session_cancels) =
        build_stack(&ws, provider_cfg).await;
    let run_id = start_v1_run(&workflow_store, &ws, "mock-acp", &session_cancels).await;

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
    assert_no_session_effect(&ws, "pre-cancelled request must not spawn the fixture");

    host.shutdown().await.expect("host shutdown");
}

#[tokio::test]
async fn in_stream_cancel_calls_host_cancel_and_bounds_cleanup() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg =
        acp_provider_config("mock-acp", &ws.fixture_log).with_env("BLOCK_PROMPT", "1");
    let (host, _storage, workflow_store, executor, _registry, session_cancels) =
        build_stack(&ws, provider_cfg).await;
    let run_id = start_v1_run(&workflow_store, &ws, "mock-acp", &session_cancels).await;

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
#[allow(clippy::too_many_lines)] // concurrent single-flight scenario needs setup + both request paths in one flow
async fn concurrent_same_key_second_prompt_reuses_session_not_spawn() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg =
        acp_provider_config("mock-acp", &ws.fixture_log).with_env("BLOCK_PROMPT", "1");
    let (host, _storage, workflow_store, executor, _registry, session_cancels) =
        build_stack(&ws, provider_cfg).await;
    let run_id = start_v1_run(&workflow_store, &ws, "mock-acp", &session_cancels).await;

    // First prompt blocks in-stream (fixture never answers): the session is
    // Busy while it runs. The second same-run prompt must serialize on the
    // executor's per-run operation lock — it can never overlap the first's
    // Active-CAS→exec window nor replace its durable marker.
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

    // Wait until the fixture observes the first prompt (session now Busy,
    // durable Active marker written).
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

    // Durable ownership, mid-overlap: the marker names the FIRST operation's
    // attempt (same-anchor concurrency can never replace in_flight while the
    // first is still admitted/active).
    let record = workflow_store
        .load_run(&SessionId(run_id.clone()))
        .await
        .expect("load run")
        .expect("record");
    let state = record.state.expect("v1 state");
    let in_flight = state.in_flight.as_ref().expect("in_flight persisted");
    assert_eq!(
        in_flight.task_id, "task-1",
        "first operation owns the marker"
    );
    assert!(
        in_flight.operation_id.is_some(),
        "first operation's Active write carries its Host op id"
    );
    let first_attempt_id = in_flight.attempt_id.clone();

    // The second same-key prompt uses the run's SHARED coordinator token:
    // it parks on the per-run operation lock while the first is in flight
    // and can never create a second owned session or subprocess.
    let shared_token = session_cancels
        .read()
        .expect("read cancel map")
        .get(&run_id)
        .cloned()
        .expect("shared run token");
    let executor_second = executor.clone();
    let run_second = run_id.clone();
    let second = tokio::spawn(async move {
        executor_second
            .execute(PromptRequest {
                run_id: run_second,
                task_id: "task-2".to_string(),
                agent_ref: None,
                prompt: "hello-2".to_string(),
                tool_policy: ToolPolicy::DenyAll,
                cancellation: shared_token,
            })
            .await
    });

    // Give the second request time to reach (and park on) the per-run lock.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Single-flight proof, mid-overlap: exactly ONE owned session and ONE
    // fixture subprocess exist; only the first prompt reached the fixture.
    let sessions = host.list_sessions().await.expect("list sessions");
    assert_eq!(
        sessions.len(),
        1,
        "single-flight must create exactly one session for one (run, role) key"
    );
    let log = read_fixture_log(&ws.fixture_log);
    // Session launches only: the readiness probe child is not a session.
    let start_pids: std::collections::HashSet<String> = session_start_events(&log, &ws)
        .into_iter()
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

    // Release the first prompt via its OWN token, and the second via the
    // run's shared token: both requests then fail closed with the typed
    // cancellation — the second, serialized behind the first, is refused at
    // its post-lock admission fence (the shared token is already cancelled),
    // never launching a prompt of its own.
    cancel_first.cancel();
    let first_result = first.await.expect("first task joins");
    match first_result {
        Err(CapabilityError::Cancelled) => {}
        other => panic!("expected Cancelled after release, got: {other:?}"),
    }
    session_cancels
        .read()
        .expect("read cancel map")
        .get(&run_id)
        .cloned()
        .expect("shared token")
        .cancel();
    let second_result = second.await.expect("second task joins");
    match second_result {
        Err(CapabilityError::Cancelled) => {}
        other => panic!("expected Cancelled for serialized second, got: {other:?}"),
    }

    // Durable ownership after cancellation: the marker was cleared by the
    // FIRST operation's terminal handling (classified correctly — the run
    // is not left with a stale in_flight); the second request never claimed
    // or replaced it (it failed at admission before any write).
    let record = workflow_store
        .load_run(&SessionId(run_id.clone()))
        .await
        .expect("load run")
        .expect("record");
    let state = record.state.expect("v1 state");
    match state.in_flight.as_ref() {
        None => {}
        Some(marker) => panic!(
            "the durable in_flight marker must be cleared after cancellation \
             (first attempt {first_attempt_id}, found {marker:?})"
        ),
    }

    // The second request never performed an external effect.
    let log = read_fixture_log(&ws.fixture_log);
    assert_eq!(
        log.iter().filter(|e| e["event"] == "prompt").count(),
        1,
        "the serialized second request must never reach the fixture: {log:?}"
    );

    host.shutdown().await.expect("host shutdown");
}

#[tokio::test]
async fn capability_route_cancel_uses_shared_coordinator_token() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg = acp_provider_config("mock-acp", &ws.fixture_log);
    let (host, _storage, workflow_store, _executor, registry, session_cancels) =
        build_stack(&ws, provider_cfg).await;
    let run_id = start_v1_run(&workflow_store, &ws, "mock-acp", &session_cancels).await;

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
    assert!(
        result.is_err(),
        "capability route must surface the cancellation"
    );
    match result.unwrap_err() {
        CapabilityError::Cancelled => {}
        other => panic!("expected Cancelled through capability route, got: {other:?}"),
    }

    assert_no_session_effect(&ws, "cancelled capability prompt must never spawn the fixture");

    host.shutdown().await.expect("host shutdown");
}

#[tokio::test]
async fn capability_route_fails_closed_when_run_token_missing() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg = acp_provider_config("mock-acp", &ws.fixture_log);
    let (host, _storage, workflow_store, _executor, registry, _session_cancels) =
        build_stack(&ws, provider_cfg).await;
    // The run is admitted but intentionally has NO registered coordinator
    // token — the registry map is left empty.
    let run_id = format!("run:{}", uuid::Uuid::new_v4());
    let session = GraphSession::new_from_task(run_id.clone(), "start");
    session.context.set("_session_id", run_id.clone()).unwrap();
    let mut agent_bindings = HashMap::new();
    agent_bindings.insert(
        "default".to_string(),
        AgentBinding {
            provider_id: "mock-acp".to_string(),
            model: None,
        },
    );
    workflow_store
        .start_run(
            &SessionId(run_id.clone()),
            &RunDescriptorV1 {
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
            },
            RunCheckpoint {
                root: &session,
                children: &[],
            },
            &RunStateV1::default(),
        )
        .await
        .expect("start_run");

    // Every LLM-backed capability route must FAIL CLOSED with the typed
    // `CancellationUnavailable` when the run has no registered coordinator
    // token — none may silently mint a fresh uncancellable token.
    for (name, input) in [
        (
            "acp.prompt",
            serde_json::json!({ "prompt": "hello", "tool_policy": "deny_all" }),
        ),
        (
            "judge.llm",
            serde_json::json!({ "prompt": "Is the task complete?" }),
        ),
        (
            "context.summarize",
            serde_json::json!({ "content": "The story is about a brave knight." }),
        ),
        (
            "nexus.llm.extract",
            serde_json::json!({ "prompt": "extract entities", "chapter_prose": "Lin Xia drew her blade." }),
        ),
    ] {
        let cap = registry.get(name).expect("capability registered");
        let mut payload = serde_json::json!({
            "_creator_id": "ctr_test",
            "_session_id": run_id,
        });
        for (k, v) in input.as_object().expect("input object") {
            payload[k.clone()] = v.clone();
        }
        let result = cap.run(payload).await;
        match result.unwrap_err() {
            CapabilityError::CancellationUnavailable(msg) => {
                assert!(
                    msg.contains(&run_id),
                    "typed missing-token refusal must name the run: {name}: {msg}"
                );
            }
            other => panic!("{name} must fail closed with CancellationUnavailable, got: {other:?}"),
        }
    }

    // No fixture process was ever spawned (the fail-closed refusal happens
    // before any external effect).
    assert_no_session_effect(&ws, "missing-token refusals must never spawn the fixture");

    host.shutdown().await.expect("host shutdown");
}

#[tokio::test]
async fn cancellation_after_active_before_exec_no_effect_and_no_session_leak() {
    let _lock = env_lock();
    let ws = setup_workspace();
    // The fixture blocks after receiving the prompt, so the operation stays
    // admitted (Active CAS won) until the coordinator token fires.
    let provider_cfg =
        acp_provider_config("mock-acp", &ws.fixture_log).with_env("BLOCK_PROMPT", "1");
    let (host, _storage, workflow_store, executor, _registry, session_cancels) =
        build_stack(&ws, provider_cfg).await;
    let run_id = start_v1_run(&workflow_store, &ws, "mock-acp", &session_cancels).await;

    let cancel = session_cancels
        .read()
        .expect("read cancel map")
        .get(&run_id)
        .cloned()
        .expect("registered run token");
    let executor_for_task = executor.clone();
    let run_for_task = run_id.clone();
    let handle = tokio::spawn(async move {
        executor_for_task
            .execute(PromptRequest {
                run_id: run_for_task,
                task_id: "task-1".to_string(),
                agent_ref: None,
                prompt: "never-should-land".to_string(),
                tool_policy: ToolPolicy::DenyAll,
                cancellation: cancel,
            })
            .await
    });

    // Deterministic post-Active admission: wait until the durable Active
    // write is visible (session + op ids persisted, the Active CAS won),
    // THEN fire the coordinator token. The cancellation lands after the
    // Active admission boundary — the biased cancel-vs-exec arbitration at
    // the launch point (and the in-stream cancel loop) must refuse/stop the
    // operation with a typed failure and clean up the created session.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let record = workflow_store
            .load_run(&SessionId(run_id.clone()))
            .await
            .expect("load run");
        let state = record.and_then(|r| r.state);
        let active = state.as_ref().and_then(|s| s.in_flight.as_ref());
        if active.is_some_and(|a| a.phase == nexus_orchestration::run_state::PromptPhase::Active) {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "Active intent never became durable"
        );
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    session_cancels
        .read()
        .expect("read cancel map")
        .get(&run_id)
        .cloned()
        .expect("token")
        .cancel();

    let result = handle.await.expect("task joins");
    assert!(
        result.is_err(),
        "post-Active cancellation must be a typed failure"
    );
    match result.unwrap_err() {
        CapabilityError::Cancelled => {}
        other => panic!("expected Cancelled, got: {other:?}"),
    }

    // The launch either never started (biased exec-gate admission won, zero
    // prompt effect) or was cancelled in-stream (prompt arrived, then the
    // owned operation was cancelled). Either way the observable contract
    // holds: the fixture observed NO prompt beyond the blocked one, and no
    // Host session is left behind after the bounded cleanup.
    let log = read_fixture_log(&ws.fixture_log);
    let prompts = log.iter().filter(|e| e["event"] == "prompt").count();
    assert!(
        prompts <= 1,
        "the cancelled request must not launch additional prompts: {log:?}"
    );

    // Zero leaked Host session: the created owned session was shut down and
    // evicted by the executor's cancellation cleanup.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let sessions = host.list_sessions().await.expect("list sessions");
        if sessions.is_empty() {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "no Host session may leak from a post-fence cancelled request: {sessions:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    // The durable marker was cleared by the cancelled operation's terminal
    // handling — the run is not left pretending a prompt is in flight.
    let record = workflow_store
        .load_run(&SessionId(run_id.clone()))
        .await
        .expect("load run")
        .expect("record");
    let state = record.state.expect("v1 state");
    assert!(
        state.in_flight.is_none(),
        "the cancelled operation must not leave a durable in_flight marker"
    );

    host.shutdown().await.expect("host shutdown");
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // nested graph + prompt execution integration in one flow
async fn nested_inner_graph_prompt_executes_with_child_identity() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg = acp_provider_config("mock-acp", &ws.fixture_log);
    let (host, storage, workflow_store, executor, _registry, _session_cancels) =
        build_stack(&ws, provider_cfg).await;

    // Parent v1 run with the frozen default binding → mock-acp, positioned
    // at the `parent_state` task (mirrors `start_preset_run`). The run's
    // coordinator token is registered at admission in the engine's shared
    // per-run cancellation map — the SAME map the child-spawn registrar
    // (`EngineSharedState::register_cancellation`) writes and the graph
    // prompt nodes read. Production unifies the boot map with the engine's
    // shared map via `set_prompt_executor`; this test wires the node to the
    // engine's own shared map directly.
    let storage_arc: Arc<dyn graph_flow::SessionStorage> = storage.clone();
    let caps = CapabilityRegistryHolder::with_registry(Arc::new(CapabilityRegistry::empty()));
    let engine = GraphFlowEngine::new_with_storage_and_workflow_store(
        storage_arc.clone(),
        workflow_store.clone(),
        caps,
    );
    let node_cancels = engine.shared_state().session_cancels.clone();
    let parent_sid = format!("run:{}", uuid::Uuid::new_v4());
    node_cancels.write().expect("session cancels write").insert(
        parent_sid.clone(),
        tokio_util::sync::CancellationToken::new(),
    );
    let parent_session = GraphSession::new_from_task(parent_sid.clone(), "parent_state");
    parent_session
        .context
        .set("_session_id", parent_sid.clone())
        .unwrap();
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
    // context (`_session_id` seeded by `spawn_child_session_internal`). The
    // node reads the engine's shared cancellation map (production unifies
    // the boot map via `set_prompt_executor`).
    let storage_arc: Arc<dyn graph_flow::SessionStorage> = storage.clone();
    let prompt_node = nexus_orchestration::tasks::InnerGraphNodeTask::new("n1")
        .with_template("hello from outer")
        .with_tool_policy(ToolPolicy::DenyAll)
        .with_prompt_executor(Some(executor.clone() as Arc<dyn PromptExecutor>))
        .with_session_cancels(node_cancels.clone());
    let inner_graph = Arc::new(
        graph_flow::GraphBuilder::new("inner_graph")
            .add_task(Arc::new(prompt_node))
            .add_task(Arc::new(EndTask))
            .add_edge("n1", "end_task")
            .build()
            .expect("test graph build"),
    );

    // Parent graph whose start task is an InnerGraphTask over that graph.
    let engine_shared = engine.shared_state();
    let engine_arc: Arc<dyn OrchestrationEngine> = Arc::new(engine);
    let inner_task = nexus_orchestration::tasks::InnerGraphTask::new(
        engine_arc.clone(),
        inner_graph.clone(),
        "parent_state",
        "_session_id",
        None,
    );
    let parent_graph = Arc::new(
        graph_flow::GraphBuilder::new("parent_graph")
            .add_task(Arc::new(inner_task))
            .add_task(Arc::new(EndTask))
            .add_edge("parent_state", "end_task")
            .build()
            .expect("test graph build"),
    );

    // Register the parent runner AFTER the graph is fully wired (the runner
    // shares the same `Arc<Graph>`, so start-task resolution sees the final
    // graph), and register the parent in the in-memory tracker.
    {
        engine_shared.runners.write().await.insert(
            parent_sid.clone(),
            Arc::new(FlowRunner::new(parent_graph, storage_arc)),
        );
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
        log.iter()
            .any(|e| e["event"] == "prompt" && e["prompt"] == "hello from outer"),
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
        child_desc
            .parent_session_id
            .as_ref()
            .expect("parent link")
            .0,
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
    let child_ctx_id: String = child_session.context.get("_session_id").unwrap();
    assert_eq!(child_ctx_id, child.session_id.0);
    let child_output: String = child_session
        .context
        .get("state.n1.output")
        .expect("child prompt output");
    assert_eq!(child_output, "transformed:hello from outer");

    host.shutdown().await.expect("host shutdown");
}

struct EndTask;

#[async_trait::async_trait]
impl Task for EndTask {
    fn id(&self) -> &'static str {
        "end_task"
    }

    async fn run(&self, _ctx: graph_flow::Context) -> graph_flow::Result<graph_flow::TaskResult> {
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
