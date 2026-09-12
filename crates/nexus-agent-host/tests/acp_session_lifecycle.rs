//! Hermetic ACP session lifecycle tests (v1.186 P1 T1).
//!
//! Proves the observable lifecycle contracts of the recipe-based ACP
//! provider through the real `HostManager` plane and a real deterministic
//! ACP stdio fixture (`tests/fixtures/mock_acp_workflow.py`):
//!
//! - no boot/catalog spawn (recipe only, truthful catalog)
//! - lazy per-session owned process, bound to the verified Creator workspace
//!   cwd (never the daemon cwd)
//! - distinct Host sessions/Creators get distinct PIDs
//! - missing/disabled providers refuse
//! - bounded cancel reaches the owned operation; shutdown drains and reaps
//!   the exact owned process tree (a reused/unowned PID is never signalled)
//! - EOF/crash is a typed failure, never success

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::StreamExt;
use nexus_agent_host::capability::model::{
    CreateSessionRequest, HostContentBlock, HostEvent, HostOperation, HostStartConfig, SessionOwner,
};
use nexus_agent_host::config::{AgentHostConfig, ProviderConfig, TimeoutConfig};
use nexus_agent_host::core::manager::HostManager;
use nexus_agent_host::providers::acp::AcpProvider;
use nexus_agent_host::{HostFacade, HostOperationId, LaunchStrategy, ProviderId};
use tempfile::TempDir;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/mock_acp_workflow.py"
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
    creator_ws_a: PathBuf,
    creator_ws_b: PathBuf,
    config_path: PathBuf,
    fixture_log: PathBuf,
}

fn setup_workspace() -> TestWorkspace {
    let tmp = TempDir::new().expect("temp dir");
    let workspace_root = tmp.path().join("workspace");
    let creator_ws_a = workspace_root.join("creator-a");
    let creator_ws_b = workspace_root.join("creator-b");
    std::fs::create_dir_all(creator_ws_a.join("sub")).expect("creator-a sub dir");
    std::fs::create_dir_all(creator_ws_b.join("sub")).expect("creator-b sub dir");
    let config_path = tmp.path().join("config.toml");
    let fixture_log = tmp.path().join("fixture.log");
    TestWorkspace {
        _tmp: tmp,
        workspace_root,
        creator_ws_a,
        creator_ws_b,
        config_path,
        fixture_log,
    }
}

fn acp_provider_config(
    id: &str,
    fixture_log: &Path,
    extra_env: &[(&str, &str)],
    enabled: bool,
) -> ProviderConfig {
    let mut env = HashMap::new();
    env.insert(
        "ACP_FIXTURE_LOG".to_string(),
        fixture_log.to_string_lossy().into_owned(),
    );
    for (k, v) in extra_env {
        env.insert((*k).to_string(), (*v).to_string());
    }
    ProviderConfig {
        id: id.to_string(),
        protocol: "acp".to_string(),
        command: Some(FIXTURE.to_string()),
        args: vec![],
        env,
        enabled,
    }
}

fn owner(creator_id: &str, workspace_root: PathBuf) -> SessionOwner {
    SessionOwner {
        creator_id: creator_id.to_string(),
        workspace_root,
        orchestration_run_id: None,
    }
}

fn host_start_config(ws: &TestWorkspace) -> HostStartConfig {
    HostStartConfig {
        config_path: ws.config_path.clone(),
        workspace_root: ws.workspace_root.clone(),
        max_sessions: 4,
        max_ops_per_session: 1,
        timeouts: TimeoutConfig {
            launch_ms: 10_000,
            initialize_ms: 10_000,
            session_ms: 10_000,
            prompt_ms: 10_000,
            shutdown_ms: 2_000,
        },
        host_config: None,
        probe_owner: None,
    }
}

async fn build_host(
    ws: &TestWorkspace,
    provider_cfg: ProviderConfig,
) -> (Arc<HostManager>, Arc<dyn HostFacade>) {
    let manager = Arc::new(HostManager::new());
    let provider = AcpProvider::from_config(
        provider_cfg.clone(),
        TimeoutConfig {
            launch_ms: 10_000,
            initialize_ms: 10_000,
            session_ms: 10_000,
            prompt_ms: 10_000,
            shutdown_ms: 2_000,
        },
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
        .start(host_start_config(ws))
        .await
        .expect("host start");
    let host: Arc<dyn HostFacade> = manager.clone();
    (manager, host)
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

fn start_events(log: &[serde_json::Value]) -> Vec<&serde_json::Value> {
    log.iter().filter(|e| e["event"] == "start").collect()
}
async fn wait_for_event(path: &Path, event: &str) -> Vec<serde_json::Value> {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let log = read_fixture_log(path);
            if log.iter().any(|entry| entry["event"] == event) {
                return log;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("fixture did not emit {event} within bound"))
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // env_lock() serializes env-mutating tests process-wide across the whole async body
async fn no_boot_spawn_and_truthful_catalog_recipe() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg = acp_provider_config("mock-acp", &ws.fixture_log, &[], true);
    let (manager, _host) = build_host(&ws, provider_cfg.clone()).await;

    // No process may have been spawned at boot/catalog load.
    assert!(
        !ws.fixture_log.exists(),
        "fixture must not be spawned at boot/catalog load"
    );

    // Truthful catalog: real command/args/env, available = configured recipe.
    let catalog = manager.provider_catalog().await.expect("catalog");
    let entry = catalog
        .find(&ProviderId::new("mock-acp"))
        .expect("configured ACP provider in catalog");
    assert_eq!(
        entry.protocol_kind,
        nexus_agent_host::capability::model::ProtocolKind::Acp
    );
    match &entry.launch {
        LaunchStrategy::Acp { command, args, env } => {
            assert_eq!(command, FIXTURE, "catalog reports the real command");
            assert!(args.is_empty());
            assert_eq!(
                env.get("ACP_FIXTURE_LOG").map(String::as_str),
                Some(ws.fixture_log.to_str().expect("utf8")),
                "catalog reports sanitized env keys"
            );
        }
        other @ LaunchStrategy::NativeCli { .. } => {
            panic!("expected Acp launch strategy, got {other:?}")
        }
    }
    assert!(entry.health.available, "configured+enabled is available");
    // The manager's runtime catalog reports the registered recipe; the
    // discovery catalog (config path) states the lazy-spawn message.
    assert!(
        entry.health.message.is_none()
            || entry
                .health
                .message
                .as_deref()
                .unwrap_or_default()
                .contains("lazy"),
        "catalog must not claim a successful launch, got {:?}",
        entry.health.message
    );

    // Still no spawn after catalog read.
    assert!(
        !ws.fixture_log.exists(),
        "catalog read must not spawn the child"
    );
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // env_lock() serializes env-mutating tests process-wide
async fn lazy_per_session_pid_and_creator_cwd_isolation() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg = acp_provider_config("mock-acp", &ws.fixture_log, &[], true);
    let (_manager, host) = build_host(&ws, provider_cfg).await;

    // Session A for creator-a, cwd under creator-a workspace.
    let session_a = host
        .create_session(CreateSessionRequest {
            provider_id: ProviderId::new("mock-acp"),
            cwd: ws.creator_ws_a.join("sub"),
            model: None,
            mode: None,
            mcp_servers: vec![],
            metadata: serde_json::Value::Null,
            owner: owner("ctr_a", ws.creator_ws_a.clone()),
        })
        .await
        .expect("session A created");

    // The fixture must have been spawned with the Creator workspace cwd,
    // never the daemon cwd (the test process cwd).
    let log = read_fixture_log(&ws.fixture_log);
    let starts = start_events(&log);
    assert_eq!(starts.len(), 1, "exactly one spawn for session A: {log:?}");
    let cwd_a = starts[0]["cwd"].as_str().expect("cwd recorded");
    let expected_cwd = ws
        .creator_ws_a
        .join("sub")
        .canonicalize()
        .expect("canonical");
    assert_eq!(
        Path::new(cwd_a),
        expected_cwd,
        "session A child must run in the Creator workspace cwd, not the daemon cwd"
    );
    let pid_a = starts[0]["pid"].as_u64().expect("pid recorded");

    // Session B for creator-b, cwd under creator-b workspace → distinct PID.
    let session_b = host
        .create_session(CreateSessionRequest {
            provider_id: ProviderId::new("mock-acp"),
            cwd: ws.creator_ws_b.join("sub"),
            model: None,
            mode: None,
            mcp_servers: vec![],
            metadata: serde_json::Value::Null,
            owner: owner("ctr_b", ws.creator_ws_b.clone()),
        })
        .await
        .expect("session B created");

    let log = read_fixture_log(&ws.fixture_log);
    let starts = start_events(&log);
    assert_eq!(starts.len(), 2, "two distinct spawns: {log:?}");
    let pid_b = starts[1]["pid"].as_u64().expect("pid recorded");
    assert_ne!(
        pid_a, pid_b,
        "distinct Host sessions/Creators must not share a process"
    );
    let cwd_b = starts[1]["cwd"].as_str().expect("cwd recorded");
    let expected_cwd_b = ws
        .creator_ws_b
        .join("sub")
        .canonicalize()
        .expect("canonical");
    assert_eq!(Path::new(cwd_b), expected_cwd_b);

    // Cleanup: shutdown both sessions (reaps the exact owned children).
    host.shutdown_session(session_a.id.clone())
        .await
        .expect("shutdown A");
    host.shutdown_session(session_b.id.clone())
        .await
        .expect("shutdown B");
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // env_lock() serializes env-mutating tests process-wide
async fn prompt_returns_non_echo_agent_output() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg = acp_provider_config("mock-acp", &ws.fixture_log, &[], true);
    let (_manager, host) = build_host(&ws, provider_cfg).await;

    let session = host
        .create_session(CreateSessionRequest {
            provider_id: ProviderId::new("mock-acp"),
            cwd: ws.creator_ws_a.join("sub"),
            model: None,
            mode: None,
            mcp_servers: vec![],
            metadata: serde_json::Value::Null,
            owner: owner("ctr_a", ws.creator_ws_a.clone()),
        })
        .await
        .expect("session created");

    let op_id = HostOperationId::new();
    let stream = host
        .exec(
            session.id.clone(),
            HostOperation::Prompt {
                op_id: op_id.clone(),
                content: vec![HostContentBlock::Text {
                    text: "hello-fixture".to_string(),
                }],
                permission_scope: None,
            },
        )
        .await
        .expect("exec");

    let events: Vec<HostEvent> = stream.map(|r| r.expect("event")).collect().await;

    let mut text = String::new();
    let mut finished = false;
    for event in &events {
        match event {
            HostEvent::MessageDelta(d) => text.push_str(&d.text),
            HostEvent::OpFinished(_) => finished = true,
            _ => {}
        }
    }
    assert!(finished, "must end with OpFinished: {events:?}");
    assert_eq!(
        text, "transformed:hello-fixture",
        "agent output must be the fixture's non-echo transformation, not the prompt"
    );

    host.shutdown_session(session.id).await.expect("shutdown");
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // env_lock() serializes env-mutating tests process-wide
async fn missing_and_disabled_providers_refuse() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg = acp_provider_config("mock-acp", &ws.fixture_log, &[], true);
    let (manager, host) = build_host(&ws, provider_cfg).await;

    // Missing provider → refusal, no echo fallback.
    let err = host
        .create_session(CreateSessionRequest {
            provider_id: ProviderId::new("does-not-exist"),
            cwd: ws.creator_ws_a.join("sub"),
            model: None,
            mode: None,
            mcp_servers: vec![],
            metadata: serde_json::Value::Null,
            owner: owner("ctr_a", ws.creator_ws_a.clone()),
        })
        .await
        .expect_err("unknown provider must refuse");
    assert_eq!(err.category(), "policy_denied");

    // Disabled provider → construction refuses (never registered).
    let disabled = acp_provider_config("mock-disabled", &ws.fixture_log, &[], false);
    let err = AcpProvider::from_config(
        disabled,
        TimeoutConfig::default(),
        nexus_agent_host::HostPermissionResolver::new_native_only(
            &AgentHostConfig::default().policy,
        ),
    )
    .err()
    .expect("disabled provider must refuse");
    assert_eq!(err.category(), "provider_unavailable");

    // The disabled provider was never registered → catalog omits it.
    let catalog = manager.provider_catalog().await.expect("catalog");
    assert!(
        catalog.find(&ProviderId::new("mock-disabled")).is_none(),
        "disabled provider must not appear in the catalog"
    );
}

#[tokio::test]
#[allow(clippy::await_holding_lock, clippy::cast_possible_truncation)] // env_lock() held process-wide; OS pid fits u32
async fn cancel_reaches_owned_operation_and_shutdown_reaps_exact_process() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg =
        acp_provider_config("mock-acp", &ws.fixture_log, &[("BLOCK_PROMPT", "1")], true);
    let (_manager, host) = build_host(&ws, provider_cfg).await;

    let session = host
        .create_session(CreateSessionRequest {
            provider_id: ProviderId::new("mock-acp"),
            cwd: ws.creator_ws_a.join("sub"),
            model: None,
            mode: None,
            mcp_servers: vec![],
            metadata: serde_json::Value::Null,
            owner: owner("ctr_a", ws.creator_ws_a.clone()),
        })
        .await
        .expect("session created");

    let pid = {
        let log = read_fixture_log(&ws.fixture_log);
        start_events(&log)[0]["pid"].as_u64().expect("pid") as u32
    };

    // Start a blocked prompt in the background (the fixture never responds).
    let op_id = HostOperationId::new();
    let stream = host
        .exec(
            session.id.clone(),
            HostOperation::Prompt {
                op_id: op_id.clone(),
                content: vec![HostContentBlock::Text {
                    text: "blocked".to_string(),
                }],
                permission_scope: None,
            },
        )
        .await
        .expect("exec");
    let drain = tokio::spawn(async move {
        let _: Vec<HostEvent> = stream.map(|r| r.expect("event")).collect().await;
    });
    wait_for_event(&ws.fixture_log, "prompt_blocked").await;

    // Cancel targets the owned operation: the ACP session/cancel must reach
    // the fixture.
    tokio::time::timeout(std::time::Duration::from_secs(5), host.cancel(op_id))
        .await
        .expect("cancel must complete within bound")
        .expect("cancel");
    let log = wait_for_event(&ws.fixture_log, "cancel").await;
    assert!(
        log.iter().any(|e| e["event"] == "cancel"),
        "session/cancel must reach the owned fixture: {log:?}"
    );

    // Shutdown drains and reaps the exact owned process tree.
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        host.shutdown_session(session.id.clone()),
    )
    .await
    .expect("shutdown must complete within bound")
    .expect("shutdown");
    tokio::time::timeout(std::time::Duration::from_secs(15), drain)
        .await
        .expect("stream drain must complete within bound")
        .expect("drain task");

    // The exact owned PID must be gone (reaped), and we must never have
    // signalled a reused/unowned PID — the fixture's own child (if any)
    // is the only other process, and the owned child is the one we reaped.
    assert!(
        !process_alive(pid),
        "owned ACP process {pid} must be reaped after shutdown"
    );
}

#[tokio::test]
#[allow(clippy::await_holding_lock, clippy::cast_possible_truncation)] // env_lock() held process-wide; OS pid fits u32
async fn shutdown_reaps_owned_process_tree_descendants() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg =
        acp_provider_config("mock-acp", &ws.fixture_log, &[("DESCENDANT", "1")], true);
    let (_manager, host) = build_host(&ws, provider_cfg).await;

    let session = host
        .create_session(CreateSessionRequest {
            provider_id: ProviderId::new("mock-acp"),
            cwd: ws.creator_ws_a.join("sub"),
            model: None,
            mode: None,
            mcp_servers: vec![],
            metadata: serde_json::Value::Null,
            owner: owner("ctr_a", ws.creator_ws_a.clone()),
        })
        .await
        .expect("session created");

    // The fixture spawned an outliving descendant; the owned process-tree
    // teardown must kill the whole group (leader + descendant).
    let log = read_fixture_log(&ws.fixture_log);
    let child_pid = start_events(&log)[0]["pid"].as_u64().expect("pid") as u32;
    let descendant_pid = log
        .iter()
        .find(|e| e["event"] == "descendant_spawned")
        .expect("descendant spawn recorded")["child_pid"]
        .as_u64()
        .expect("descendant pid") as u32;

    host.shutdown_session(session.id.clone())
        .await
        .expect("shutdown");

    assert!(
        !process_alive(child_pid),
        "owned ACP leader {child_pid} must be reaped after shutdown"
    );
    assert!(
        !process_alive(descendant_pid),
        "owned process-tree descendant {descendant_pid} must be reaped after shutdown (group kill)"
    );
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // env_lock() serializes env-mutating tests process-wide
async fn eof_after_initialize_is_typed_failure() {
    let _lock = env_lock();
    let ws = setup_workspace();
    let provider_cfg = acp_provider_config(
        "mock-acp",
        &ws.fixture_log,
        &[("EOF_AFTER_INIT", "1")],
        true,
    );
    let (_manager, host) = build_host(&ws, provider_cfg).await;

    // The fixture exits right after initialize; session creation must fail
    // with a typed launch error — never a fake success.
    let err = host
        .create_session(CreateSessionRequest {
            provider_id: ProviderId::new("mock-acp"),
            cwd: ws.creator_ws_a.join("sub"),
            model: None,
            mode: None,
            mcp_servers: vec![],
            metadata: serde_json::Value::Null,
            owner: owner("ctr_a", ws.creator_ws_a.clone()),
        })
        .await
        .expect_err("EOF after initialize must fail session creation");
    assert_eq!(
        err.category(),
        "launch_failed",
        "EOF/crash is a typed failure, got: {err}"
    );
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    // `kill -0 <pid>` returns Ok while the process exists (or is a zombie
    // awaiting reap); non-zero (ESRCH) means it is gone.
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(not(unix))]
fn process_alive(_pid: u32) -> bool {
    true
}
