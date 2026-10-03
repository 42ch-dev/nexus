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
    CreateSessionRequest, HostEvent, HostOperation, HostStartConfig, ProbeRequest, ProviderHealth,
    SessionOwner,
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

/// Resolve an absolute interpreter before tests isolate the process PATH.
fn python3_path() -> String {
    static PYTHON3: LazyLock<String> = LazyLock::new(|| {
        #[cfg(unix)]
        {
            let from_env = std::process::Command::new("/usr/bin/env")
                .args(["python3", "-c", "import sys; print(sys.executable)"])
                .output()
                .ok()
                .filter(|out| out.status.success())
                .and_then(|out| String::from_utf8(out.stdout).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            from_env.unwrap_or_else(|| "python3".to_string())
        }
        #[cfg(windows)]
        {
            for (program, args) in [
                ("py", vec!["-3", "-c", "import sys; print(sys.executable)"]),
                ("python", vec!["-c", "import sys; print(sys.executable)"]),
            ] {
                if let Ok(output) = std::process::Command::new(program).args(args).output() {
                    if output.status.success() {
                        if let Ok(path) = String::from_utf8(output.stdout) {
                            let path = path.trim();
                            if !path.is_empty() && Path::new(path).is_absolute() {
                                return path.to_string();
                            }
                        }
                    }
                }
            }
            panic!("Python is unavailable through `py -3` and `python`");
        }
    });
    PYTHON3.clone()
}

/// Make a platform-appropriate shim using an absolute interpreter.
fn write_fixture_shim(dir: &Path, name: &str, fixture: &str) -> PathBuf {
    let path = dir.join(if cfg!(windows) {
        format!("{name}.cmd")
    } else {
        name.to_string()
    });
    #[cfg(unix)]
    let body = format!("#!/bin/sh\nexec {} {fixture} \"$@\"\n", python3_path());
    #[cfg(windows)]
    let body = format!("@echo off\r\n\"{}\" \"{fixture}\" %*\r\n", python3_path());
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

/// Every fixture PROCESS SPAWN receipt in a log, regardless of cwd.
///
/// The ACP fixture writes one `{"event": "start", "pid": ...}` record per
/// process it creates, so this counts real spawns (probe AND session). A
/// cwd-filtered session-only count would miss a child spawned in the boundary
/// cwd, which is exactly what a denied admission must not do.
fn spawn_events(log: &[serde_json::Value]) -> Vec<&serde_json::Value> {
    log.iter()
        .filter(|e| e["event"] == "start" && e["pid"].is_u64())
        .collect()
}

/// Every fixture pid recorded in a log.
fn fixture_pids(log: &[serde_json::Value]) -> Vec<u32> {
    log.iter()
        .filter_map(|e| e["pid"].as_u64())
        .map(|pid| u32::try_from(pid).expect("fixture PID must fit u32"))
        .collect()
}

// Liveness probe for the fixture children this binary spawns. The definition is
// split by platform; the call sites stay shared. The Windows form is a REAL
// probe, never a constant stub: a `false` stub would let every `!process_alive`
// leak assertion below pass vacuously, and a `true` stub would false-fail the
// un-gated post-ready test.
#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    // `kill -0 <pid>` returns Ok while the process exists (or is a zombie
    // awaiting reap); non-zero (ESRCH) means it is gone.
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .output()
        .is_ok_and(|out| out.status.success())
}

#[cfg(windows)]
fn process_alive(pid: u32) -> bool {
    // `tasklist /FI "PID eq <pid>" /FO CSV /NH` prints one quoted CSV row per
    // match; a no-match run prints a localized INFO line instead, which is
    // never CSV-shaped, so comparing the CSV PID column is
    // localization-independent. `tasklist` exits 0 either way, so the OUTPUT
    // is parsed, never the exit status.
    let output = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .unwrap_or_else(|error| {
            panic!("`tasklist /FI \"PID eq {pid}\" /FO CSV /NH` failed to spawn: {error}")
        });
    assert!(
        output.status.success(),
        "`tasklist /FI \"PID eq {pid}\" /FO CSV /NH` exited with {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    tasklist_reports_live_pid(&String::from_utf8_lossy(&output.stdout), pid)
}

// ── Windows tasklist CSV parsing (pure; unit-tested cross-platform) ─

/// Extract the PID column (the SECOND CSV field) from one `tasklist /FO CSV`
/// row, honoring the CSV quoting rules `tasklist` actually emits.
///
/// Every field is double-quoted and an embedded quote is escaped by doubling
/// (`""`), so a comma inside a quoted image name (for example `"py,thon.exe"`)
/// must NOT split the row and shift the PID out of field two. A line that is
/// not CSV-shaped — the localized "no tasks match" INFO line printed on a
/// no-match run, or empty output — yields `None`. Kept pure so the known-live
/// semantics are unit-testable without Windows.
fn tasklist_csv_pid(line: &str) -> Option<String> {
    // tasklist CSV rows always open with the quoted image-name field; the
    // localized INFO line does not, so reject anything else before splitting.
    let line = line.trim_start();
    if !line.starts_with('"') {
        return None;
    }
    let mut fields: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if in_quotes => {
                if chars.peek() == Some(&'"') {
                    // `""` is one literal `"`; consume both.
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            }
            '"' => in_quotes = true,
            ',' if !in_quotes => fields.push(std::mem::take(&mut field)),
            _ => field.push(ch),
        }
    }
    fields.push(field);
    fields.get(1).map(|pid| pid.trim().to_string())
}

/// Whether a `tasklist /FI "PID eq <pid>" /FO CSV /NH` run's stdout reports
/// `expected_pid` alive.
///
/// The PID is the second CSV field of a real row, so a live child is
/// recognized even when its image name contains a comma. A no-match run still
/// exits 0 and prints a localized INFO line, which `tasklist_csv_pid` rejects.
fn tasklist_reports_live_pid(stdout: &str, expected_pid: u32) -> bool {
    let expected = expected_pid.to_string();
    stdout
        .lines()
        .filter_map(tasklist_csv_pid)
        .any(|pid| pid == expected)
}

#[test]
fn tasklist_probe_recognizes_a_live_pid_in_an_ordinary_row() {
    let stdout = "\"python.exe\",\"4242\",\"Console\",\"1\",\"12,000 K\"\r\n";
    assert!(tasklist_reports_live_pid(stdout, 4242));
    // A PID the row does not carry is NOT reported alive (guards a true stub).
    assert!(!tasklist_reports_live_pid(stdout, 4243));
}

#[test]
fn tasklist_probe_handles_a_comma_inside_a_quoted_image_name() {
    // The comma lives INSIDE the quoted image name; the PID stays field two.
    let row = r#""py,thon.exe","4242","Console","1","12,000 K""#;
    assert_eq!(tasklist_csv_pid(row).as_deref(), Some("4242"));
    assert!(tasklist_reports_live_pid(row, 4242));
}

#[test]
fn tasklist_probe_handles_an_escaped_quote_in_a_field() {
    // `""` is one literal quote and must not end the quoted field early.
    let row = r#""we""ird,na""me.exe","4242","Console","1","12,000 K""#;
    assert_eq!(tasklist_csv_pid(row).as_deref(), Some("4242"));
    assert!(tasklist_reports_live_pid(row, 4242));
}

#[test]
fn tasklist_probe_ignores_a_localized_no_match_info_line() {
    let stdout = "INFO: No tasks are running which match the specified criteria.\r\n";
    assert_eq!(tasklist_csv_pid(stdout.trim()), None);
    assert!(!tasklist_reports_live_pid(stdout, 4242));
}

#[test]
fn tasklist_probe_reports_no_pid_for_empty_output() {
    assert_eq!(tasklist_csv_pid(""), None);
    assert!(!tasklist_reports_live_pid("", 4242));
}

// ── dsh: routes, cwd binding, timeout close ────────────────────────

/// Platform-specific expectation for one resolved dsh route's probe result.
#[derive(Clone, Copy, Debug)]
enum RouteExpectation {
    /// Unix: ordinary AND sealed recipes initialize and close, so the probe
    /// reports available.
    Available,
    /// Windows: sealed provisioning is unsupported, so the probe reports
    /// unavailable ONLY after the ordinary recipe's confirmed start + close
    /// (register R-V1202-P1T3-001).
    UnsupportedSealed,
}

/// The Windows sealed-provisioning fail-closed reason fragment. The provider
/// emits it only AFTER the ordinary recipe's confirmed start and cooperative
/// close (`dsh.rs` probe ordering: an ordinary failure returns an
/// "ordinary dsh recipe …" message instead), so matching it proves the
/// ordinary handshake completed.
const SEALED_UNSUPPORTED: &str =
    "sealed deny_all home provisioning is unsupported on this platform";

/// Pure predicate for one resolved dsh route's probe outcome; `Err` carries
/// the violation reason.
///
/// The fixture logs `_spawn` BEFORE reading its first request, so a nonempty
/// log proves nothing: the route log must carry BOTH the `initialize` request
/// AND the cooperative `shutdown` close. Then the platform expectation applies:
/// unix must report `available`; Windows must report unavailable for the
/// SPECIFIC unsupported-sealed-provisioning reason — an ordinary close
/// error/timeout (which also yields an unavailable health) is rejected.
fn check_route_handshake(
    methods: &[String],
    available: bool,
    message: Option<&str>,
    expectation: RouteExpectation,
    route: &str,
) -> Result<(), String> {
    if !methods.iter().any(|method| method == "initialize") {
        return Err(format!(
            "the {route} route must complete a real ordinary `initialize` handshake: {methods:?}"
        ));
    }
    if !methods.iter().any(|method| method == "shutdown") {
        return Err(format!(
            "the {route} route's ordinary recipe must be closed cooperatively: {methods:?}"
        ));
    }
    match expectation {
        RouteExpectation::Available => {
            if !available {
                return Err(format!(
                    "the {route} route must probe available, got message {message:?}"
                ));
            }
        }
        RouteExpectation::UnsupportedSealed => {
            if available {
                return Err(format!(
                    "the {route} route must be unavailable on Windows (sealed recipe unsupported)"
                ));
            }
            if !message.is_some_and(|text| text.contains(SEALED_UNSUPPORTED)) {
                return Err(format!(
                    "the {route} route may only fail via the unsupported sealed provisioning \
                     reached after a confirmed ordinary close, got message {message:?}"
                ));
            }
        }
    }
    Ok(())
}

/// Assert one resolved dsh route reached a REAL ordinary handshake.
fn assert_route_reached_a_handshake(health: &ProviderHealth, req_log: &Path, route: &str) {
    let methods: Vec<String> = read_log(req_log)
        .iter()
        .filter_map(|entry| entry["method"].as_str().map(str::to_string))
        .collect();
    #[cfg(unix)]
    let expectation = RouteExpectation::Available;
    #[cfg(not(unix))]
    let expectation = RouteExpectation::UnsupportedSealed;
    if let Err(reason) = check_route_handshake(
        &methods,
        health.available,
        health.message.as_deref(),
        expectation,
        route,
    ) {
        panic!("{reason}");
    }
}

/// QC3-F001 proof: with `initialize` + `shutdown` receipts present, an ordinary
/// recipe failure (initialization or close) must be rejected under BOTH
/// platform expectations, while the Windows unsupported-sealed outcome — only
/// reachable after a confirmed ordinary close — is accepted. A startup-only
/// receipt (`_spawn` only) is always rejected.
#[test]
fn route_handshake_rejects_ordinary_failure_despite_receipts() {
    let receipts = vec![
        "_spawn".to_string(),
        "initialize".to_string(),
        "shutdown".to_string(),
    ];
    let expectations = [
        RouteExpectation::Available,
        RouteExpectation::UnsupportedSealed,
    ];
    // The provider's two ordinary-recipe failure messages (dsh.rs probe).
    for message in [
        "ordinary dsh recipe failed to initialize: dsh runtime could not be launched",
        "ordinary dsh recipe close was not confirmed: cooperative shutdown failed",
    ] {
        for expectation in expectations {
            assert!(
                check_route_handshake(&receipts, false, Some(message), expectation, "configured")
                    .is_err(),
                "an ordinary failure must be rejected despite receipts \
                 ({expectation:?}, {message:?})"
            );
        }
    }
    // A startup-only log (no handshake receipt) is rejected even with the
    // accepted Windows reason.
    assert!(
        check_route_handshake(
            &["_spawn".to_string()],
            false,
            Some(SEALED_UNSUPPORTED),
            RouteExpectation::UnsupportedSealed,
            "configured"
        )
        .is_err(),
        "a startup-only receipt must be rejected"
    );
    // The Windows unsupported-sealed outcome (ordinary confirmed close, then
    // sealed provisioning fails closed) is accepted.
    assert!(
        check_route_handshake(
            &receipts,
            false,
            Some(
                "sealed dsh recipe failed to initialize: sealed deny_all home provisioning is \
                 unsupported on this platform: descriptor-relative no-follow filesystem \
                 primitives are required"
            ),
            RouteExpectation::UnsupportedSealed,
            "configured"
        )
        .is_ok(),
        "the unsupported-sealed outcome after a confirmed ordinary close must be accepted"
    );
    // An available result satisfies the unix expectation.
    assert!(
        check_route_handshake(
            &receipts,
            true,
            None,
            RouteExpectation::Available,
            "configured"
        )
        .is_ok(),
        "an available probe with receipts must be accepted on unix"
    );
}

/// The three documented resolution routes each produce a bounded, real
/// handshake; a missing runtime stays unavailable.
// Keep the three resolution-route handshakes in one acceptance scenario.
#[allow(clippy::too_many_lines)]
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

    let path_dir = tmp.path().join("path-bin");
    std::fs::create_dir_all(&path_dir).expect("path bin");
    let configured_bin = write_fixture_shim(&path_dir, "dsh-configured", MOCK_DSH);
    let explicit = DshNativeProvider::new(
        ProviderId::new("dsh-native"),
        "Configured".to_string(),
        Some(configured_bin.to_string_lossy().into_owned()),
        &[],
        env.clone(),
        timeouts(),
    )
    .expect("empty native args accepted");
    let health = explicit
        .probe(probe_request(&cwd, 10_000))
        .await
        .expect("probe runs");
    assert_route_reached_a_handshake(&health, &req_log, "configured");

    std::fs::remove_file(&req_log).ok();
    write_fixture_shim(&path_dir, "dsh", MOCK_DSH);
    {
        let _path = PathGuard::isolate(&path_dir);
        let via_path = DshNativeProvider::new(
            ProviderId::new("dsh-native"),
            "Path".to_string(),
            None,
            &[],
            env.clone(),
            timeouts(),
        )
        .expect("constructor ok");
        let health = via_path
            .probe(probe_request(&cwd, 10_000))
            .await
            .expect("probe runs");
        assert_route_reached_a_handshake(&health, &req_log, "PATH");
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
            None,
            &[],
            env.clone(),
            timeouts(),
        )
        .expect("constructor ok");
        let health = via_env
            .probe(probe_request(&cwd, 10_000))
            .await
            .expect("probe runs");
        assert_route_reached_a_handshake(&health, &req_log, "DSH_RUNTIME_BIN");
    }

    // (4) Nothing resolvable ⇒ unavailable, never a false ready.
    {
        let _empty_path = PathGuard::isolate(&tmp.path().join("no-such-dir"));
        // `DSH_RUNTIME_BIN` is unset here (guard dropped above).
        let missing = DshNativeProvider::new(
            ProviderId::new("dsh-native"),
            "Missing".to_string(),
            None,
            &[],
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

/// Build the verified-cwd dsh probe fixture: a real interpreter shim plus an
/// isolated `REQ_LOG`/`DSH_HOME`. The `cwd` dir is created.
fn dsh_cwd_probe_provider(
    tmp: &Path,
    cwd: &Path,
    req_log: &Path,
    dsh_home: &Path,
) -> DshNativeProvider {
    std::fs::create_dir_all(cwd).expect("cwd");
    let fixture = write_fixture_shim(tmp, "dsh-cwd", MOCK_DSH);
    DshNativeProvider::new(
        ProviderId::new("dsh-native"),
        "Cwd".to_string(),
        Some(fixture.to_string_lossy().into_owned()),
        &[],
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
    .expect("constructor ok")
}

/// The `_spawn` cwd as the probe child recorded it (`os.getcwd()`),
/// canonicalized so Windows 8.3/long-name and verbatim-prefix forms compare
/// equal to the verified request cwd.
fn recorded_spawn_cwd(spawn: &serde_json::Value, log: &[serde_json::Value]) -> PathBuf {
    let recorded = spawn["cwd"].as_str().expect("cwd recorded");
    std::fs::canonicalize(recorded).unwrap_or_else(|e| {
        panic!("recorded cwd {recorded:?} must canonicalize: {e}; log: {log:?}")
    })
}

/// F4 (ordinary half): the bounded probe runs in the VERIFIED request cwd,
/// never the daemon's ambient working directory.
///
/// The sealed recipe is asserted by its own `#[cfg(unix)]` test below.
#[tokio::test]
#[allow(clippy::await_holding_lock)] // one process-heavy fixture at a time
async fn dsh_probe_binds_verified_cwd_for_ordinary_recipe() {
    let _lock = ENV_LOCK.lock().await;
    let tmp = tempfile::tempdir().expect("temp dir");
    let cwd = tmp.path().join("creator-ws");
    let req_log = tmp.path().join("dsh.jsonl");
    let dsh_home = tmp.path().join("dsh-home");

    // Read (never mutate) the process cwd: the request cwd is a fresh temp dir,
    // so a probe that fell back to the ambient directory would be detectable
    // without changing process-global state (mutating cwd corrupts sibling
    // tests running in the same binary).
    let ambient = std::env::current_dir().expect("cwd");
    let provider = dsh_cwd_probe_provider(tmp.path(), &cwd, &req_log, &dsh_home);

    let _health = provider
        .probe(probe_request(&cwd, 15_000))
        .await
        .expect("probe runs");

    let log = read_log(&req_log);
    let spawns = spawn_records(&log);
    assert!(
        !spawns.is_empty(),
        "the ordinary recipe must spawn for the probe: {log:?}"
    );
    let expected = std::fs::canonicalize(&cwd).expect("canonical cwd");
    let ambient = std::fs::canonicalize(&ambient).expect("canonical ambient");
    let recorded = recorded_spawn_cwd(spawns[0], &log);
    assert_eq!(
        recorded, expected,
        "the probe must run in the verified request cwd: {log:?}"
    );
    assert_ne!(
        recorded, ambient,
        "the probe must never fall back to the ambient cwd: {log:?}"
    );
}

/// F4 (sealed half): the bounded probe initializes and closes BOTH recipes and
/// EVERY recipe binds the VERIFIED request cwd, never the ambient directory.
///
/// Sealed `deny_all` home provisioning needs descriptor-relative no-follow
/// filesystem primitives; Windows is unsupported and fails that recipe closed
/// (register R-V1202-P1T3-001 — trigger: Windows job-object/process and
/// descriptor-relative filesystem capability evidence re-opening the sealed
/// cohort), so only this unix half asserts both recipes.
#[cfg(unix)]
#[tokio::test]
#[allow(clippy::await_holding_lock)] // one process-heavy fixture at a time
async fn dsh_probe_binds_verified_cwd_for_sealed_recipe() {
    let _lock = ENV_LOCK.lock().await;
    let tmp = tempfile::tempdir().expect("temp dir");
    let cwd = tmp.path().join("creator-ws");
    let req_log = tmp.path().join("dsh.jsonl");
    let dsh_home = tmp.path().join("dsh-home");
    let ambient = std::env::current_dir().expect("cwd");
    let provider = dsh_cwd_probe_provider(tmp.path(), &cwd, &req_log, &dsh_home);

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
    let ambient = std::fs::canonicalize(&ambient).expect("canonical ambient");
    for spawn in &spawns {
        let recorded = recorded_spawn_cwd(spawn, &log);
        assert_eq!(
            recorded, expected,
            "every probe recipe must run in the verified request cwd: {log:?}"
        );
        assert_ne!(
            recorded, ambient,
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
        Some(MOCK_DSH.to_string()),
        &[],
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
            .filter_map(|s| s["pid"].as_u64())
            .map(|pid| u32::try_from(pid).expect("fixture PID must fit u32"))
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

    let fixture = write_fixture_shim(tmp.path(), "claude", MOCK_CLAUDE);
    let provider = ClaudeCliProvider::new(
        ProviderId::new("claude-native"),
        "Claude".to_string(),
        fixture.to_string_lossy().into_owned(),
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
    // Canonicalize the child's recorded cwd too: Windows may report the 8.3
    // short form (RUNNER~1) or a `\\?\` verbatim form, which both canonicalize
    // to the long absolute path (register row R-V1202-P1T3-001).
    let recorded_cwd = std::fs::canonicalize(log[0]["cwd"].as_str().expect("cwd recorded"))
        .expect("the recorded cwd must canonicalize");
    let expected_cwd = std::fs::canonicalize(&cwd).expect("canonical cwd");
    assert_eq!(
        recorded_cwd, expected_cwd,
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

    let fixture = write_fixture_shim(tmp.path(), "acp", MOCK_ACP);
    let provider = AcpProvider::from_config(
        ProviderConfig {
            id: "mock-acp".to_string(),
            protocol: "acp".to_string(),
            command: Some(fixture.to_string_lossy().into_owned()),
            args: vec![],
            env: HashMap::from([(
                "ACP_FIXTURE_LOG".to_string(),
                fixture_log.to_string_lossy().into_owned(),
            )]),
            enabled: true,
        },
        timeouts(),
        HostPermissionResolver::new_native_only(&AgentHostConfig::default().policy),
        nexus_acp_host::LocalSetBridge::new(),
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
    // Canonicalize the recorded cwd too: Windows may report a `\\?\` verbatim
    // form that only matches after canonicalization (register row
    // R-V1202-P1T3-001).
    let recorded = std::fs::canonicalize(starts[0]["cwd"].as_str().expect("cwd recorded"))
        .expect("the recorded cwd must canonicalize");
    assert_eq!(
        recorded, expected,
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
        admitted_catalog: None,
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
#[allow(clippy::too_many_lines)] // Keep the two lifecycle transitions and their observations together.
async fn post_ready_launch_failure_invalidates_while_prompt_timeout_stays_ready() {
    let _lock = ENV_LOCK.lock().await;
    let tmp = tempfile::tempdir().expect("temp dir");
    let workspace_root = tmp.path().join("workspace");
    let creator_ws = workspace_root.join("creator-a");
    std::fs::create_dir_all(&creator_ws).expect("creator ws");
    let acp_fixture = write_fixture_shim(tmp.path(), "acp-lifecycle", MOCK_ACP);

    // ── Case A: the probe PASSES (run 1), the SESSION LAUNCH EOFs (run 2).
    let launch_log = tmp.path().join("acp-launch.jsonl");
    let manager = HostManager::new();
    let mut config = host_start_config(&workspace_root, Some(owner(&workspace_root)));
    config.host_config = Some(AgentHostConfig {
        providers: vec![ProviderConfig {
            id: "mock-acp".to_string(),
            protocol: "acp".to_string(),
            command: Some(acp_fixture.to_string_lossy().into_owned()),
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
    let spawns_before = spawn_events(&read_log(&launch_log)).len();
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
    let denied_err = denied.expect_err("admission must agree with the now-unavailable catalog");
    assert_eq!(
        denied_err.category(),
        "provider_unavailable",
        "the denial must be the TYPED ProviderUnavailable category, got: {denied_err}"
    );
    assert_eq!(
        spawn_events(&read_log(&launch_log)).len(),
        spawns_before,
        "the denied admission must not spawn another fixture process"
    );

    // ── Case B: an ordinary prompt timeout must NOT invalidate.
    let prompt_log = tmp.path().join("acp-prompt.jsonl");
    let manager2 = HostManager::new();
    let mut config2 = host_start_config(&workspace_root, Some(owner(&workspace_root)));
    // The 500ms prompt deadline must be configured on the authoritative
    // `AgentHostConfig.timeouts`: `HostManager::start` materializes the
    // discovered provider adapters from the EMBEDDED host config's timeouts,
    // while the `HostStartConfig.timeouts` copy only feeds the start probe
    // budget (`initialize_ms`). Configuring the deadline on the start config
    // alone would leave the provider on the 180s default, and the blocked
    // prompt would outlive the 3s drain bound below.
    let prompt_deadline = TimeoutConfig {
        prompt_ms: 500,
        ..TimeoutConfig::default()
    };
    config2.host_config = Some(AgentHostConfig {
        providers: vec![ProviderConfig {
            id: "mock-acp".to_string(),
            protocol: "acp".to_string(),
            command: Some(acp_fixture.to_string_lossy().into_owned()),
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
        timeouts: prompt_deadline.clone(),
        ..AgentHostConfig::default()
    });
    // Keep the derived start-config copy in lockstep with the host config,
    // mirroring the daemon/core-node constructors so the two timeout sources
    // never silently diverge.
    config2.timeouts = prompt_deadline;
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
        let drain = async {
            while let Some(item) = stream.next().await {
                if let Ok(HostEvent::OpFailed(failed)) = item {
                    saw_timeout = Some(failed.error_category);
                    break;
                }
            }
        };
        // Discriminating bound: 3s is far above the configured 500ms prompt
        // deadline and far below the 180s crate default, so reaching the
        // terminal event here is itself the evidence that the CONFIGURED
        // deadline was the one enforced — an ignored `prompt_ms` would still be
        // pending. (No assertion on the diagnostic's wording: that would pin
        // source text rather than behavior.)
        tokio::time::timeout(std::time::Duration::from_secs(3), drain)
            .await
            .expect("the configured 500ms prompt deadline must elapse within 3s (default is 180s)");
        saw_timeout
    };
    let category = observed_timeout.expect("the blocked prompt must emit OpFailed");
    assert!(
        category.contains("timeout"),
        "the terminal event must be a REAL timeout (got category '{category}'), \
proving the configured prompt deadline elapsed"
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
