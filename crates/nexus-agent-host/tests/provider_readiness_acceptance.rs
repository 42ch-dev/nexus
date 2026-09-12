//! v1.188 P2 — provider readiness acceptance proof (runtime, real components).
//!
//! Exercises the REAL `HostManager`/factory plane against spawnable protocol
//! fixtures (`mock_dsh_agent.py`, `mock_claude_cli.py`,
//! `mock_acp_workflow.py`) so the locked acceptance cases are proven by
//! observed subprocess behaviour rather than mock-only health echoes:
//!
//! - configured / PATH / `DSH_RUNTIME_BIN` dsh routes, and missing ⇒ unavailable
//! - the bounded dsh probe binds the VERIFIED request cwd for BOTH the ordinary
//!   and the sealed recipe (never the daemon's ambient cwd)
//! - dsh initialize timeout ⇒ unavailable, with the owned direct child reaped
//! - Claude version probe runs the CONFIGURED environment; timeout ⇒
//!   unavailable without leaving a live child
//! - generic ACP probe runs in the verified owner workspace
//! - `HostManager` catalog health and admission agree (disabled suppression,
//!   missing/non-executable omission)
//! - a POST-READY launch failure invalidates the candidate, while an ordinary
//!   prompt/content timeout leaves a probed-ready provider healthy
//!
//! Hermetic: every test either mutates `PATH`/`DSH_RUNTIME_BIN` under a
//! process-wide lock with RAII restore, or passes an explicit executable.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use nexus_agent_host::capability::model::{
    CreateSessionRequest, HostEvent, HostOperation, HostStartConfig, ProbeRequest, SessionOwner,
};
use nexus_agent_host::config::{AgentHostConfig, ProviderConfig, TimeoutConfig};
use nexus_agent_host::core::manager::HostManager;
use nexus_agent_host::providers::acp::AcpProvider;
use nexus_agent_host::providers::native_cli::claude::ClaudeCliProvider;
use nexus_agent_host::providers::native_cli::dsh::DshNativeProvider;
use nexus_agent_host::{
    HostFacade, HostOperationId, HostPermissionResolver, ProviderAdapter, ProviderId,
};
use tokio::sync::Mutex;

// ── Fixtures (reused; no new protocol mocks) ───────────────────────

const MOCK_DSH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/native_protocol/mock_dsh_agent.py"
);
const MOCK_CLAUDE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/native_protocol/mock_claude_cli.py"
);
const MOCK_ACP: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/mock_acp_workflow.py"
);

/// Serializes the process-heavy tests within this test binary.
///
/// Every test here drives real python subprocesses, and two of them also mutate
/// process-global `PATH`/`DSH_RUNTIME_BIN`. Running them concurrently makes the
/// fixtures contend for CPU (cold interpreter starts blow SDK handshake
/// deadlines) and lets one test's env mutation land inside another's probe.
/// One lock for the whole file keeps the fixtures hermetic — the same
/// convention the ACP lifecycle and dsh/claude fixtures already use.
static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Replace `PATH` for the test scope; restore on drop.
struct PathGuard {
    previous: Option<std::ffi::OsString>,
}

impl PathGuard {
    fn isolate(dir: &Path) -> Self {
        let previous = std::env::var_os("PATH");
        std::env::set_var("PATH", dir);
        Self { previous }
    }
}

impl Drop for PathGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
    }
}

/// Set/remove `DSH_RUNTIME_BIN`; restore on drop.
struct DshBinGuard {
    previous: Option<String>,
}

impl DshBinGuard {
    fn set(value: &Path) -> Self {
        let previous = std::env::var("DSH_RUNTIME_BIN").ok();
        std::env::set_var("DSH_RUNTIME_BIN", value);
        Self { previous }
    }
}

impl Drop for DshBinGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var("DSH_RUNTIME_BIN", value),
            None => std::env::remove_var("DSH_RUNTIME_BIN"),
        }
    }
}

#[cfg(unix)]
fn set_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).expect("stat").permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).expect("chmod +x");
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) {}

/// Absolute interpreter path resolved BEFORE any PATH isolation.
///
/// The route tests replace `PATH` with a single directory, so a shim that said
/// bare `python3` would lose its interpreter and the child would exit
/// immediately (observed as "closed the transport before the turn completed").
fn python3_path() -> String {
    static PYTHON3: LazyLock<String> = LazyLock::new(|| {
        let from_env = std::process::Command::new("/usr/bin/env")
            .args(["python3", "-c", "import sys; print(sys.executable)"])
            .output()
            .ok()
            .filter(|out| out.status.success())
            .and_then(|out| String::from_utf8(out.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        from_env.unwrap_or_else(|| "python3".to_string())
    });
    PYTHON3.clone()
}

/// Make an executable shim named `name` that runs the python fixture with an
/// ABSOLUTE interpreter, so it survives PATH isolation.
fn write_fixture_shim(dir: &Path, name: &str, fixture: &str) -> PathBuf {
    let path = dir.join(name);
    let body = format!("#!/bin/sh\nexec {} {fixture} \"$@\"\n", python3_path());
    std::fs::write(&path, body).expect("write shim");
    set_executable(&path);
    path
}

/// Crate-default budgets (see `TimeoutConfig`), not tightened values.
///
/// These tests drive REAL python subprocesses; under parallel test load a cold
/// interpreter start plus the SDK handshake can exceed a few seconds, so a
/// tightened initialize/launch budget makes the fixtures flaky rather than
/// proving anything. Tests that need a SHORT deadline (the prompt-timeout case)
/// override the specific field explicitly.
fn timeouts() -> TimeoutConfig {
    TimeoutConfig::default()
}

fn owner(workspace_root: &Path) -> SessionOwner {
    SessionOwner {
        creator_id: "ctr_acceptance".to_string(),
        workspace_root: workspace_root.to_path_buf(),
        orchestration_run_id: None,
    }
}

fn probe_request(cwd: &Path, timeout_ms: u64) -> ProbeRequest {
    ProbeRequest {
        timeout_ms,
        cwd: cwd.to_path_buf(),
        owner: owner(cwd),
    }
}

fn read_log(path: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .map(|content| {
            content
                .lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn spawn_records(log: &[serde_json::Value]) -> Vec<&serde_json::Value> {
    log.iter().filter(|e| e["method"] == "_spawn").collect()
}

/// Start events for SESSION launches only (the readiness probe runs in the
/// host workspace boundary, so its cwd is excluded).
fn session_start_events<'a>(
    log: &'a [serde_json::Value],
    workspace_root: &Path,
) -> Vec<&'a serde_json::Value> {
    let probe_cwd =
        std::fs::canonicalize(workspace_root).unwrap_or_else(|_| workspace_root.to_path_buf());
    log.iter()
        .filter(|e| e["event"] == "start")
        .filter(|e| {
            e["cwd"]
                .as_str()
                .is_none_or(|cwd| Path::new(cwd) != probe_cwd.as_path())
        })
        .collect()
}

/// Every fixture pid recorded in a log.
fn fixture_pids(log: &[serde_json::Value]) -> Vec<u32> {
    log.iter()
        .filter_map(|e| e["pid"].as_u64().map(|p| p as u32))
        .collect()
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .output()
        .is_ok_and(|out| out.status.success())
}

// ── dsh: routes, cwd binding, timeout close ────────────────────────

/// The three documented resolution routes each produce a bounded, real
/// handshake; a missing runtime stays unavailable.
#[tokio::test]
async fn dsh_routes_configured_path_and_env_all_reach_a_real_handshake() {
    let _lock = ENV_LOCK.lock().await;
    let tmp = tempfile::tempdir().expect("temp dir");
    let cwd = tmp.path().join("creator-ws");
    std::fs::create_dir_all(&cwd).expect("cwd");
    let req_log = tmp.path().join("dsh.jsonl");
    let dsh_home = tmp.path().join("dsh-home");

    let env = HashMap::from([
        (
            "REQ_LOG".to_string(),
            req_log.to_string_lossy().into_owned(),
        ),
        (
            "DSH_HOME".to_string(),
            dsh_home.to_string_lossy().into_owned(),
        ),
    ]);

    // (1) configured explicit executable.
    let explicit = DshNativeProvider::new(
        ProviderId::new("dsh-native"),
        "Configured".to_string(),
        Some(MOCK_DSH.to_string()), &[],
        env.clone(),
        timeouts(),
    )
    .expect("empty native args accepted");
    let health = explicit
        .probe(probe_request(&cwd, 10_000))
        .await
        .expect("probe runs");
    assert!(
        health.available,
        "a configured runtime must probe available, got {health:?}"
    );

    // (2) PATH route: a `dsh` shim on the isolated PATH.
    std::fs::remove_file(&req_log).ok();
    let path_dir = tmp.path().join("path-bin");
    std::fs::create_dir_all(&path_dir).expect("path bin");
    write_fixture_shim(&path_dir, "dsh", MOCK_DSH);
    {
        let _path = PathGuard::isolate(&path_dir);
        let via_path = DshNativeProvider::new(
            ProviderId::new("dsh-native"),
            "Path".to_string(),
            None, &[],
            env.clone(),
            timeouts(),
        )
        .expect("constructor ok");
        let health = via_path
            .probe(probe_request(&cwd, 10_000))
            .await
            .expect("probe runs");
        assert!(
            health.available,
            "a PATH-found runtime must probe available, got {health:?}"
        );
    }

    // (3) DSH_RUNTIME_BIN route (no PATH entry).
    std::fs::remove_file(&req_log).ok();
    let env_bin = write_fixture_shim(&path_dir, "dsh-env", MOCK_DSH);
    {
        let _empty_path = PathGuard::isolate(&tmp.path().join("no-such-dir"));
        let _dsh_bin = DshBinGuard::set(&env_bin);
        let via_env = DshNativeProvider::new(
            ProviderId::new("dsh-native"),
            "Env".to_string(),
            None, &[],
            env.clone(),
            timeouts(),
        )
        .expect("constructor ok");
        let health = via_env
            .probe(probe_request(&cwd, 10_000))
            .await
            .expect("probe runs");
        assert!(
            health.available,
            "a DSH_RUNTIME_BIN runtime must probe available, got {health:?}"
        );
    }

    // (4) Nothing resolvable ⇒ unavailable, never a false ready.
    {
        let _empty_path = PathGuard::isolate(&tmp.path().join("no-such-dir"));
        // `DSH_RUNTIME_BIN` is unset here (guard dropped above).
        let missing = DshNativeProvider::new(
            ProviderId::new("dsh-native"),
            "Missing".to_string(),
            None, &[],
            env,
            timeouts(),
        )
        .expect("constructor ok");
        let health = missing
            .probe(probe_request(&cwd, 5_000))
            .await
            .expect("probe returns health, not an error");
        assert!(
            !health.available,
            "an unresolvable runtime must be unavailable, got {health:?}"
        );
    }
}

/// F4: the bounded probe runs in the VERIFIED request cwd for BOTH recipes,
/// never the daemon's ambient working directory.
#[tokio::test]
#[allow(clippy::await_holding_lock)] // one process-heavy fixture at a time
async fn dsh_probe_binds_verified_cwd_for_ordinary_and_sealed_recipes() {
    let _lock = ENV_LOCK.lock().await;
    let tmp = tempfile::tempdir().expect("temp dir");
    let cwd = tmp.path().join("creator-ws");
    std::fs::create_dir_all(&cwd).expect("cwd");
    let req_log = tmp.path().join("dsh.jsonl");
    let dsh_home = tmp.path().join("dsh-home");

    // Read (never mutate) the process cwd: the request cwd is a fresh temp dir,
    // so a probe that fell back to the ambient directory would be detectable
    // without changing process-global state (mutating cwd corrupts sibling
    // tests running in the same binary).
    let ambient = std::env::current_dir().expect("cwd");

    let provider = DshNativeProvider::new(
        ProviderId::new("dsh-native"),
        "Cwd".to_string(),
        Some(MOCK_DSH.to_string()), &[],
        HashMap::from([
            (
                "REQ_LOG".to_string(),
                req_log.to_string_lossy().into_owned(),
            ),
            (
                "DSH_HOME".to_string(),
                dsh_home.to_string_lossy().into_owned(),
            ),
        ]),
        timeouts(),
    )
    .expect("constructor ok");

    let health = provider
        .probe(probe_request(&cwd, 15_000))
        .await
        .expect("probe runs");

    assert!(
        health.available,
        "both recipes must initialize and close, got {health:?}"
    );

    let log = read_log(&req_log);
    let spawns = spawn_records(&log);
    assert_eq!(
        spawns.len(),
        2,
        "the ordinary AND sealed recipes each spawn once: {log:?}"
    );
    let expected = std::fs::canonicalize(&cwd).expect("canonical cwd");
    for spawn in &spawns {
        let recorded = spawn["cwd"].as_str().expect("cwd recorded");
        assert_eq!(
            Path::new(recorded),
            expected.as_path(),
            "every probe recipe must run in the verified request cwd: {log:?}"
        );
        assert_ne!(
            Path::new(recorded),
            ambient.as_path(),
            "the probe must never fall back to the ambient cwd: {log:?}"
        );
    }
}

/// A dsh initialize timeout reports unavailable and leaves no live child.
#[cfg(unix)]
#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn dsh_probe_initialize_timeout_is_unavailable_with_no_live_child() {
    let _lock = ENV_LOCK.lock().await;
    let tmp = tempfile::tempdir().expect("temp dir");
    let cwd = tmp.path().join("creator-ws");
    std::fs::create_dir_all(&cwd).expect("cwd");
    let req_log = tmp.path().join("dsh.jsonl");
    let dsh_home = tmp.path().join("dsh-home");

    let provider = DshNativeProvider::new(
        ProviderId::new("dsh-native"),
        "Timeout".to_string(),
        Some(MOCK_DSH.to_string()), &[],
        HashMap::from([
            (
                "REQ_LOG".to_string(),
                req_log.to_string_lossy().into_owned(),
            ),
            (
                "DSH_HOME".to_string(),
                dsh_home.to_string_lossy().into_owned(),
            ),
            // Initialize stalls past the probe budget.
            ("INIT_DELAY_MS".to_string(), "3000".to_string()),
        ]),
        TimeoutConfig {
            initialize_ms: 300,
            ..timeouts()
        },
    )
    .expect("constructor ok");

    // A probe deadline that fires is surfaced as the typed probe timeout (the
    // manager maps that to unavailable); a completed probe reports unavailable
    // health. Either way the provider must NOT be reported ready.
    match provider.probe(probe_request(&cwd, 1_200)).await {
        Ok(health) => assert!(
            !health.available,
            "a stalled initialize must be unavailable, got {health:?}"
        ),
        Err(error) => assert_eq!(
            error.category(),
            "operation_timeout",
            "a stalled initialize must surface the typed probe timeout, got: {error}"
        ),
    }

    // The retained close owners finish asynchronously; poll for the spawned
    // pids to appear and then for every one of them to be reaped, under a
    // generous bound. Asserting eventual reaping (not a fixed sleep) keeps this
    // decidable under parallel test load.
    let mut pids: Vec<u32> = Vec::new();
    for _ in 0..100 {
        pids = spawn_records(&read_log(&req_log))
            .iter()
            .filter_map(|s| s["pid"].as_u64().map(|p| p as u32))
            .collect();
        if !pids.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(
        !pids.is_empty(),
        "the stalled initialize must still have spawned the recipe"
    );
    let mut reaped = false;
    for _ in 0..150 {
        if pids.iter().all(|pid| !process_alive(*pid)) {
            reaped = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(
        reaped,
        "owned probe children {pids:?} must be reaped after the timeout"
    );
}

// ── Claude: configured env + timeout close ─────────────────────────

/// The version probe runs the CONFIGURED environment (evidence: the fixture
/// only writes `REQ_LOG` when that configured variable reached the child).
#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn claude_version_probe_applies_configured_environment() {
    let _lock = ENV_LOCK.lock().await;
    let tmp = tempfile::tempdir().expect("temp dir");
    let cwd = tmp.path().join("creator-ws");
    std::fs::create_dir_all(&cwd).expect("cwd");
    let req_log = tmp.path().join("claude.jsonl");

    let provider = ClaudeCliProvider::new(
        ProviderId::new("claude-native"),
        "Claude".to_string(),
        MOCK_CLAUDE.to_string(),
        HashMap::from([(
            "REQ_LOG".to_string(),
            req_log.to_string_lossy().into_owned(),
        )]),
        timeouts(),
    );

    let health = provider
        .probe(probe_request(&cwd, 10_000))
        .await
        .expect("probe runs");
    assert!(
        health.available,
        "the version handshake must succeed, got {health:?}"
    );

    let log = read_log(&req_log);
    assert!(
        !log.is_empty(),
        "the configured environment must reach the probe child \
         (no REQ_LOG evidence ⇒ the probe dropped it)"
    );
    assert_eq!(
        log[0]["argv"][0].as_str(),
        Some("--version"),
        "the probe must run the version handshake: {log:?}"
    );
    let expected_cwd = std::fs::canonicalize(&cwd).expect("canonical cwd");
    assert_eq!(
        log[0]["cwd"].as_str().map(Path::new),
        Some(expected_cwd.as_path()),
        "the probe must run in the verified workspace cwd"
    );
}

/// A hanging version probe is unavailable and leaves no live child.
#[cfg(unix)]
#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn claude_probe_timeout_is_unavailable_without_leaking_a_child() {
    let _lock = ENV_LOCK.lock().await;
    let tmp = tempfile::tempdir().expect("temp dir");
    let cwd = tmp.path().join("creator-ws");
    std::fs::create_dir_all(&cwd).expect("cwd");

    // A command that never answers `--version`, and records its pid. The child
    // is an ABSOLUTE python interpreter so the pid record cannot be lost to a
    // shell redirection race or an isolated PATH.
    let pid_file = tmp.path().join("hang.pid");
    let hang = tmp.path().join("hanging-claude");
    std::fs::write(
        &hang,
        format!(
            "#!/bin/sh\nexec {} -c 'import os,sys,time;open(sys.argv[1],\"w\").write(str(os.getpid()));sys.stdout.flush();time.sleep(60)' {}\n",
            python3_path(),
            pid_file.to_string_lossy()
        ),
    )
    .expect("write hanging command");
    set_executable(&hang);

    let provider = ClaudeCliProvider::new(
        ProviderId::new("claude-native"),
        "Claude".to_string(),
        hang.to_string_lossy().into_owned(),
        HashMap::new(),
        timeouts(),
    );

    // The budget must outlast a cold interpreter start so the child can record
    // its pid before the deadline kills it; the handshake itself stays hung.
    let health = provider
        .probe(probe_request(&cwd, 2_000))
        .await
        .expect("probe returns health");
    assert!(
        !health.available,
        "a hanging handshake must be unavailable, got {health:?}"
    );
    let message = health.message.unwrap_or_default();
    assert!(
        message.contains("timed out") || message.contains("cleanup unconfirmed"),
        "the failure must state the timeout (or unconfirmed close), got: {message}"
    );

    // The child records its pid immediately; poll briefly so the read cannot
    // race the child's own startup.
    let mut recorded = None;
    for _ in 0..40 {
        if let Ok(text) = std::fs::read_to_string(&pid_file) {
            if let Ok(pid) = text.trim().parse::<u32>() {
                recorded = Some(pid);
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let pid = recorded.expect("the probe child must record its pid");
    // The owned close is bounded; give it that window then assert reaped.
    tokio::time::sleep(std::time::Duration::from_millis(2_500)).await;
    assert!(
        !process_alive(pid),
        "the timed-out probe child {pid} must not outlive the bounded close"
    );
}

// ── ACP: verified owner workspace ──────────────────────────────────

/// The generic ACP probe connects inside the VERIFIED owner workspace.
#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn acp_probe_runs_in_the_verified_owner_workspace() {
    let _lock = ENV_LOCK.lock().await;
    let tmp = tempfile::tempdir().expect("temp dir");
    let workspace_root = tmp.path().join("workspace");
    let creator_ws = workspace_root.join("creator-a");
    std::fs::create_dir_all(&creator_ws).expect("creator ws");
    let fixture_log = tmp.path().join("acp.jsonl");

    let provider = AcpProvider::from_config(
        ProviderConfig {
            id: "mock-acp".to_string(),
            protocol: "acp".to_string(),
            command: Some(MOCK_ACP.to_string()),
            args: vec![],
            env: HashMap::from([(
                "ACP_FIXTURE_LOG".to_string(),
                fixture_log.to_string_lossy().into_owned(),
            )]),
            enabled: true,
        },
        timeouts(),
        HostPermissionResolver::new_native_only(&AgentHostConfig::default().policy),
    )
    .expect("valid ACP recipe");

    // `available` is the observable that fails if the probe spends its whole
    // budget on the owned close (the old defect reported a timeout here).
    let health = provider
        .probe(probe_request(&creator_ws, 10_000))
        .await
        .expect("probe runs");
    assert!(
        health.available,
        "the bounded ACP handshake must succeed, got {health:?}"
    );

    let log = read_log(&fixture_log);
    let starts: Vec<&serde_json::Value> = log.iter().filter(|e| e["event"] == "start").collect();
    assert_eq!(starts.len(), 1, "one probe child: {log:?}");
    let expected = std::fs::canonicalize(&creator_ws).expect("canonical");
    assert_eq!(
        Path::new(starts[0]["cwd"].as_str().expect("cwd recorded")),
        expected.as_path(),
        "the probe must run in the verified owner workspace: {log:?}"
    );
}

// ── HostManager: discovery ⇄ admission agreement ───────────────────

fn host_start_config(workspace_root: &Path, probe_owner: Option<SessionOwner>) -> HostStartConfig {
    HostStartConfig {
        config_path: workspace_root.join("absent-config.toml"),
        workspace_root: workspace_root.to_path_buf(),
        max_sessions: 4,
        max_ops_per_session: 1,
        timeouts: timeouts(),
        host_config: None,
        probe_owner,
    }
}

fn provider_row(id: &str, protocol: &str, command: Option<&str>, enabled: bool) -> ProviderConfig {
    ProviderConfig {
        id: id.to_string(),
        protocol: protocol.to_string(),
        command: command.map(str::to_string),
        args: vec![],
        env: HashMap::new(),
        enabled,
    }
}

/// A disabled provider is suppressed and never admissible; a non-executable
/// PATH candidate is omitted, and catalog health matches the admission result.
#[tokio::test]
async fn catalog_health_and_admission_agree_on_suppressed_and_missing_providers() {
    let _lock = ENV_LOCK.lock().await;
    let tmp = tempfile::tempdir().expect("temp dir");
    let workspace_root = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace_root).expect("workspace root");

    // A non-executable `claude` on PATH must NOT become a candidate.
    let path_dir = tmp.path().join("path-bin");
    std::fs::create_dir_all(&path_dir).expect("path bin");
    std::fs::write(path_dir.join("claude"), "not executable\n").expect("write non-exec");

    let manager = HostManager::new();
    let mut config = host_start_config(&workspace_root, Some(owner(&workspace_root)));
    config.host_config = Some(AgentHostConfig {
        providers: vec![
            // Explicitly disabled ⇒ suppresses any auto-discovery for this id.
            provider_row("claude-native", "native_cli", Some("claude"), false),
        ],
        ..AgentHostConfig::default()
    });

    {
        let _path = PathGuard::isolate(&path_dir);
        manager.start(config).await.expect("host start");
    }

    let catalog = manager.provider_catalog().await.expect("catalog");
    assert!(
        catalog.find(&ProviderId::new("claude-native")).is_none(),
        "a disabled provider must not appear: {:?}",
        catalog.entries
    );
    assert!(
        catalog
            .entries
            .iter()
            .all(|e| e.provider_id.0 != "dsh-native"),
        "a PATH-absent dsh must not appear: {:?}",
        catalog.entries
    );

    // Admission agrees with the catalog: no row ⇒ refused.
    let denied = manager
        .create_session(CreateSessionRequest {
            provider_id: ProviderId::new("claude-native"),
            cwd: workspace_root.clone(),
            model: None,
            mode: None,
            mcp_servers: vec![],
            metadata: serde_json::Value::Null,
            owner: owner(&workspace_root),
        })
        .await;
    assert!(
        denied.is_err(),
        "an absent provider must not be admissible from a catalog that omits it"
    );
}

/// A ready-path provider whose LATER launch fails is invalidated; an ordinary
/// prompt/content timeout leaves the same provider ready.
#[tokio::test]
async fn post_ready_launch_failure_invalidates_while_prompt_timeout_stays_ready() {
    let _lock = ENV_LOCK.lock().await;
    let tmp = tempfile::tempdir().expect("temp dir");
    let workspace_root = tmp.path().join("workspace");
    let creator_ws = workspace_root.join("creator-a");
    std::fs::create_dir_all(&creator_ws).expect("creator ws");

    // ── Case A: the probe PASSES (run 1), the SESSION LAUNCH EOFs (run 2).
    let launch_log = tmp.path().join("acp-launch.jsonl");
    let manager = HostManager::new();
    let mut config = host_start_config(&workspace_root, Some(owner(&workspace_root)));
    config.host_config = Some(AgentHostConfig {
        providers: vec![ProviderConfig {
            id: "mock-acp".to_string(),
            protocol: "acp".to_string(),
            command: Some(MOCK_ACP.to_string()),
            args: vec![],
            env: HashMap::from([
                (
                    "ACP_FIXTURE_LOG".to_string(),
                    launch_log.to_string_lossy().into_owned(),
                ),
                ("EOF_AFTER_INIT_FROM_RUN".to_string(), "2".to_string()),
            ]),
            enabled: true,
        }],
        ..AgentHostConfig::default()
    });
    manager.start(config).await.expect("host start");

    let before = manager.provider_catalog().await.expect("catalog");
    let ready_before = before
        .find(&ProviderId::new("mock-acp"))
        .expect("mock-acp in catalog")
        .health
        .available;
    assert!(
        ready_before,
        "the bounded probe must have proven readiness before the launch fails"
    );

    let launch = manager
        .create_session(CreateSessionRequest {
            provider_id: ProviderId::new("mock-acp"),
            cwd: creator_ws.clone(),
            model: None,
            mode: None,
            mcp_servers: vec![],
            metadata: serde_json::Value::Null,
            owner: owner(&workspace_root),
        })
        .await;
    let launch_err = launch.expect_err("a post-ready launch failure must surface");
    // The exact launch-class category varies with whether the owned close could
    // be confirmed once the child dies during session creation
    // (`launch_failed` vs `cleanup_unconfirmed`). The invariant that matters —
    // and that the manager acts on — is the launch-class classification, so
    // assert that directly via the library's own predicate.
    assert!(
        nexus_agent_host::core::readiness::is_launch_class_failure(&launch_err),
        "the failure must be a TYPED launch-class error, got category '{}': {launch_err}",
        launch_err.category()
    );

    // The candidate must now be reported unavailable by the catalog...
    let after_launch = manager.provider_catalog().await.expect("catalog");
    assert!(
        !after_launch
            .find(&ProviderId::new("mock-acp"))
            .expect("mock-acp in catalog")
            .health
            .available,
        "a launch-class exec/launch failure must invalidate the candidate"
    );

    // ...and the next admission must be DENIED, without spawning another child.
    let sessions_before = session_start_events(&read_log(&launch_log), &workspace_root).len();
    let denied = manager
        .create_session(CreateSessionRequest {
            provider_id: ProviderId::new("mock-acp"),
            cwd: creator_ws.clone(),
            model: None,
            mode: None,
            mcp_servers: vec![],
            metadata: serde_json::Value::Null,
            owner: owner(&workspace_root),
        })
        .await;
    assert!(
        denied.is_err(),
        "admission must agree with the now-unavailable catalog"
    );
    assert_eq!(
        session_start_events(&read_log(&launch_log), &workspace_root).len(),
        sessions_before,
        "the denied admission must not spawn another fixture child"
    );

    // ── Case B: an ordinary prompt timeout must NOT invalidate.
    let prompt_log = tmp.path().join("acp-prompt.jsonl");
    let manager2 = HostManager::new();
    let mut config2 = host_start_config(&workspace_root, Some(owner(&workspace_root)));
    config2.host_config = Some(AgentHostConfig {
        providers: vec![ProviderConfig {
            id: "mock-acp".to_string(),
            protocol: "acp".to_string(),
            command: Some(MOCK_ACP.to_string()),
            args: vec![],
            env: HashMap::from([
                (
                    "ACP_FIXTURE_LOG".to_string(),
                    prompt_log.to_string_lossy().into_owned(),
                ),
                ("BLOCK_PROMPT".to_string(), "1".to_string()),
            ]),
            enabled: true,
        }],
        ..AgentHostConfig::default()
    });
    // The prompt deadline is REAL and configured through the authoritative
    // `TimeoutConfig` (it flows into the discovered provider). A short deadline
    // makes a genuine streaming timeout observable; the outer wait below is
    // generous so the assertion is about the event, not about timing.
    config2.timeouts.prompt_ms = 500;
    manager2.start(config2).await.expect("host start");

    let session = manager2
        .create_session(CreateSessionRequest {
            provider_id: ProviderId::new("mock-acp"),
            cwd: creator_ws.clone(),
            model: None,
            mode: None,
            mcp_servers: vec![],
            metadata: serde_json::Value::Null,
            owner: owner(&workspace_root),
        })
        .await
        .expect("session created on the probed provider");

    // Drive one prompt whose content never completes. The fixture blocks
    // forever, so the provider's OWN prompt deadline must elapse and the
    // stream must terminate with a real timeout OpFailed.
    let mut stream = manager2
        .exec(
            session.id.clone(),
            HostOperation::Prompt {
                op_id: HostOperationId::new(),
                content: vec![
                    nexus_agent_host::capability::model::HostContentBlock::Text {
                        text: "blocked-prompt".to_string(),
                    },
                ],
                permission_scope: None,
            },
        )
        .await
        .expect("exec admitted");

    let observed_timeout = {
        use futures_util::StreamExt;
        let mut saw_timeout = None;
        // Generous outer bound: the assertion below is about the EVENT, not a
        // timing margin.
        let drain = async {
            while let Some(item) = stream.next().await {
                if let Ok(HostEvent::OpFailed(failed)) = item {
                    saw_timeout = Some((failed.error_category, failed.error_message));
                    break;
                }
            }
        };
        tokio::time::timeout(std::time::Duration::from_secs(20), drain)
            .await
            .expect("the prompt must reach a terminal event");
        saw_timeout
    };
    let (category, message) = observed_timeout.expect("the blocked prompt must emit OpFailed");
    assert!(
        category.contains("timeout"),
        "the terminal event must be a REAL timeout (got category '{category}'), \
proving the configured prompt deadline elapsed"
    );
    // Non-timing proof that the AUTHORITATIVE config reached the provider: the
    // provider reports the budget it actually enforced, so a `prompt_ms` that
    // was ignored would not match.
    assert!(
        message.contains("500ms"),
        "the terminal event must report the CONFIGURED prompt deadline of 500ms, \
got: {message}"
    );

    // A prompt/content timeout is not a launch-class failure: the provider
    // stays ready and remains admissible.
    let after = manager2.provider_catalog().await.expect("catalog");
    let still_ready = after
        .find(&ProviderId::new("mock-acp"))
        .expect("mock-acp in catalog")
        .health
        .available;
    assert!(
        still_ready,
        "an ordinary prompt/content timeout must leave a probed-ready provider \
         healthy (blanket timeout invalidation would break this)"
    );

    // Orderly teardown of BOTH managers, then confirm no fixture child
    // outlived them.
    manager.shutdown().await.expect("manager shutdown");
    manager2.shutdown().await.expect("manager2 shutdown");

    let mut leaked = fixture_pids(&read_log(&launch_log));
    leaked.extend(fixture_pids(&read_log(&prompt_log)));
    assert!(
        !leaked.is_empty(),
        "the run must have spawned fixture children to check for leaks"
    );
    // Give the owned teardown its bounded window before judging leaks.
    tokio::time::sleep(std::time::Duration::from_millis(1_500)).await;
    for pid in leaked {
        assert!(
            !process_alive(pid),
            "fixture child {pid} must not survive an orderly manager shutdown"
        );
    }
}
