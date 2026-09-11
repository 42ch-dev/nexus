//! Hermetic actual-upstream-`dsh` integration proof (v1.188 P0 Task 3).
//!
//! Unlike the SDK-wire fixture (`tests/fixtures/native_protocol/
//! mock_dsh_agent.py`, exercised by the in-module `dsh::tests`), these tests
//! drive the INSTALLED upstream `dsh` runtime (resolved through the
//! production chain: explicit override → parent `DSH_RUNTIME_BIN` → PATH
//! `dsh`) through the real `DshNativeProvider`, with a loopback
//! deterministic **LLM protocol** proxy standing in for the DeepSeek API
//! (`DEEPSEEK_BASE_URL` + a non-secret placeholder `DEEPSEEK_API_KEY`; no
//! credential is read, copied, or logged, and no paid/model call leaves
//! loopback). The proxy scripts a bounded completion and records only
//! non-secret request structure/counts.
//!
//! Proofs:
//!
//! - sealed `deny_all`: every model request advertises NO tools (the
//!   `tools` key is absent), unsolicited `shell` / `str_replace_editor` /
//!   `run_code` calls are rejected by dispatch (`unknown tool`) with no
//!   marker file/process/tool-body side effect, and the scripted final
//!   completion bounds the turn (empty advertised tools alone would not
//!   prove enforcement — the hostile calls are the enforcement arm);
//! - a poisoned ordinary home/profile (marker plugin package + persona
//!   override) observably enters the ORDINARY runtime but never the sealed
//!   runtime;
//! - a missing required plugin package fails launch closed (typed error,
//!   zero model calls) — every incompatible/failed denial is a STOP, never
//!   an ordinary fallback;
//! - v1.188 P1: message-level streaming timing on the installed runtime
//!   (first `MessageDelta` observed before the terminal on a loopback LLM
//!   proxy turn; non-secret monotonic Instant evidence is logged);
//! - safe runtime identity (CLI version, npm package identity, wire
//!   protocol server identity) is recorded without touching credentials.
//!
//! When no real `dsh` resolves on the host, each test skips with a loud
//! note (never a fake proof); with `dsh` installed they are deterministic,
//! bounded, isolated (own temp HOME/DSH_HOME, loopback ports), and mutate
//! no process-global environment.

use std::collections::HashMap;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::{FutureExt as _, StreamExt};
use nexus_agent_host::capability::model::{
    FinishReason, HostContentBlock, HostEvent, HostOperation, LaunchSpec, ManagedSessionHandle,
    PromptPermissionScope, SessionOwner,
};
use nexus_agent_host::config::TimeoutConfig;
use nexus_agent_host::error::HostError;
use nexus_agent_host::providers::native_cli::dsh::{DshNativeProvider, resolve_dsh_executable};
use nexus_agent_host::{HostOperationId, ProviderAdapter, ProviderId};
use tempfile::TempDir;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

/// The persona token planted in the poisoned home patch layer. Its
/// presence in a model request's system message proves the poisoned layer
/// entered that runtime's composition; its absence from the sealed
/// runtime's requests proves exclusion.
const POISON_TOKEN: &str = "POISON-PERSONA-7f3a9c-via-home";

/// The scripted final assistant text (the bounded completion).
const FINAL_TEXT: &str = "deterministic proxy final answer";

/// Non-secret placeholder API key: the loopback proxy ignores it; no real
/// credential is ever read or passed.
const DUMMY_API_KEY: &str = "dsh-test-nonsecret-loopback-key";

/// Serialize every test in this binary that inspects the process table:
/// all tests share ONE test process, so provider-owned dsh children of
/// concurrently running tests are indistinguishable by parent pid. The
/// lock makes reaping/hostile-token evidence unambiguous. Poison-tolerant:
/// a panicked test never blocks the rest.
static REAL_RUNTIME_SERIAL: Mutex<()> = Mutex::new(());

fn serial_real_runtime() -> std::sync::MutexGuard<'static, ()> {
    REAL_RUNTIME_SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

/// Resolve the installed/resolved real `dsh` through the production chain
/// (parent `DSH_RUNTIME_BIN` → PATH `dsh`; no explicit override here so the
/// ambient chain is what a deployment would use). `None` = skip gate.
fn real_dsh() -> Option<PathBuf> {
    match resolve_dsh_executable(None) {
        Ok(bin) => Some(bin),
        Err(reason) => {
            eprintln!("SKIP: no real upstream dsh runtime resolves on this host: {reason}");
            None
        }
    }
}

/// The all-false deny_all scope (architecture §3.4 selection).
const fn deny_all_scope() -> PromptPermissionScope {
    PromptPermissionScope {
        allow_read: false,
        allow_write: false,
        allow_destructive: false,
    }
}

fn launch_spec(cwd: &Path) -> LaunchSpec {
    LaunchSpec {
        cwd: cwd.to_path_buf(),
        model: None,
        mode: None,
        owner: SessionOwner {
            creator_id: "ctr_dsh_real_test".to_string(),
            workspace_root: cwd.to_path_buf(),
            orchestration_run_id: None,
        },
        mcp_servers: vec![],
    }
}

fn real_timeouts() -> TimeoutConfig {
    TimeoutConfig {
        initialize_ms: 30_000,
        prompt_ms: 90_000,
        shutdown_ms: 30_000,
        ..TimeoutConfig::default()
    }
}

/// The isolated child environment handed to the provider (the SDK merges
/// it into the spawn environment): isolated parent HOME and DSH_HOME, the
/// loopback model endpoint, the non-secret placeholder key, the poison
/// marker log, and disabled telemetry so the proof never egresses.
fn isolated_env(root: &TempDir, proxy_port: u16) -> HashMap<String, String> {
    HashMap::from([
        (
            "HOME".to_string(),
            root.path().join("home").to_string_lossy().into_owned(),
        ),
        (
            "DSH_HOME".to_string(),
            root.path().join("dsh-home").to_string_lossy().into_owned(),
        ),
        (
            "DEEPSEEK_BASE_URL".to_string(),
            format!("http://127.0.0.1:{proxy_port}"),
        ),
        ("DEEPSEEK_API_KEY".to_string(), DUMMY_API_KEY.to_string()),
        (
            "POISON_LOG".to_string(),
            root.path().join("poison.jsonl").to_string_lossy().into_owned(),
        ),
        ("DSH_TELEMETRY_DISABLED".to_string(), "1".to_string()),
    ])
}

fn real_provider(root: &TempDir, proxy_port: u16, dsh_bin: &Path) -> DshNativeProvider {
    DshNativeProvider::new(
        ProviderId::new("dsh-real-test"),
        "Real upstream dsh (Task 3 proof)".to_string(),
        Some(dsh_bin.to_string_lossy().into_owned()),
        Vec::new(),
        isolated_env(root, proxy_port),
        real_timeouts(),
    )
    .expect("empty native args are accepted")
}

/// Plant the poison in the ordinary isolated home: a marker plugin
/// package inserted through the profile patch layer (its module-load and
/// apply side effects append their runtime's DSH_HOME to POISON_LOG), and
/// a persona override through the home patch layer (reaches the composed
/// system prompt). Both channels are verified effective against the
/// ordinary profile by `real_dsh_poisoned_layers_reach_ordinary_runtime`.
fn poison_ordinary_home(root: &TempDir) {
    let dsh_home = root.path().join("dsh-home");
    let profile = dsh_home.join("profiles").join("sdk");
    let pkg = profile.join("node_modules").join("dsh-poison-marker");
    std::fs::create_dir_all(&pkg).expect("poison package dir");
    std::fs::write(
        pkg.join("package.json"),
        r#"{"name":"dsh-poison-marker","version":"0.0.0","type":"module","main":"index.js"}"#,
    )
    .expect("poison package manifest");
    std::fs::write(
        pkg.join("index.js"),
        r#"import fs from "node:fs";
const log = process.env.POISON_LOG;
if (log) fs.appendFileSync(log, JSON.stringify({event: "plugin-module-loaded", dshHome: process.env.DSH_HOME ?? null}) + "\n");
export const name = "dsh-poison-marker";
export function apply(ctx) {
  if (log) fs.appendFileSync(log, JSON.stringify({event: "plugin-applied", dshHome: process.env.DSH_HOME ?? null}) + "\n");
}
"#,
    )
    .expect("poison plugin module");
    // The profile's own patch layer: insert the marker plugin service.
    std::fs::write(
        profile.join("cordis.patch.yml"),
        "- insert:\n    - id: poison-marker\n      name: dsh-poison-marker\n",
    )
    .expect("poison profile patch");
    // The home patch layer: persona override reaching the system prompt.
    std::fs::write(
        dsh_home.join("cordis.patch.yml"),
        format!("- id: system-prompt\n  config:\n    personaPrefix: \"{POISON_TOKEN}\"\n"),
    )
    .expect("poison home patch");
}

/// Marker paths the unsolicited tool calls are scripted to create, plus a
/// per-process unique token embedded in the scripted background process's
/// OWN command line (the tracked process itself is token-bearing — no
/// nested untracked sleeper). The marker PARENT directory is created up
/// front, so a hostile body cannot fail for a missing parent: any marker
/// absence after the proof is attributable to dispatch rejection, not to
/// a failed body. `hostile_bodies_create_their_evidence_when_directly_run`
/// is the positive control proving every body is effective when actually
/// run. POSIX-only (process-table proof tooling).
#[cfg(unix)]
struct HostileMarkers {
    shell: PathBuf,
    shell_pid: PathBuf,
    editor: PathBuf,
    run_code: PathBuf,
    process_token: String,
}

#[cfg(unix)]
impl HostileMarkers {
    fn under(root: &TempDir) -> Self {
        let dir = root.path().join("markers");
        std::fs::create_dir_all(&dir).expect("controlled marker parent directory");
        Self {
            shell: dir.join("shell-marker"),
            shell_pid: dir.join("shell-marker.pid"),
            editor: dir.join("editor-marker"),
            run_code: dir.join("run-code-marker"),
            process_token: format!("dsh-t3-hostile-{}", std::process::id()),
        }
    }

    fn assert_all_absent(&self) {
        for (what, path) in [
            ("shell", &self.shell),
            ("shell background pid", &self.shell_pid),
            ("editor", &self.editor),
            ("run_code", &self.run_code),
        ] {
            assert!(
                !path.exists(),
                "the unsolicited {what} tool call must have NO side effect, but {path:?} exists"
            );
        }
        assert!(
            !hostile_token_alive(&self.process_token),
            "no hostile background process may survive (token {:?})",
            self.process_token
        );
    }
}

/// Explicit capability check for the POSIX process-inspection proof
/// tooling. Absence is a loud NON-PROOF skip, never silent success; the
/// local Unix proof (recorded identity below) is unaffected.
#[cfg(unix)]
fn proof_tool_capability() -> bool {
    for (tool, present) in [
        ("ps", which::which("ps").is_ok()),
        ("kill", which::which("kill").is_ok()),
        ("python3", which::which("python3").is_ok()),
        ("/bin/sh", Path::new("/bin/sh").exists()),
    ] {
        if !present {
            eprintln!("SKIP (non-proof): required proof tool {tool} unavailable on this host");
            return false;
        }
    }
    true
}

/// Whether a process with `pid` currently exists (`ps`-based).
#[cfg(unix)]
fn pid_alive(pid: i32) -> bool {
    Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "pid="])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Whether any live process command line carries the unique hostile token.
#[cfg(unix)]
fn hostile_token_alive(token: &str) -> bool {
    let out = Command::new("ps")
        .args(["-eo", "args="])
        .output()
        .expect("ps process scan");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|line| line.contains(token))
}

/// Live (non-zombie) `dsh` runtime processes parented to THIS test
/// process — the provider-owned children whose reaping the cleanup arms
/// must observe.
#[cfg(unix)]
fn live_dsh_children() -> Vec<i32> {
    let out = Command::new("ps")
        .args(["-eo", "pid=,ppid=,stat=,args="])
        .output()
        .expect("ps process scan");
    let me = std::process::id().to_string();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let pid: i32 = fields.next()?.parse().ok()?;
            let ppid = fields.next()?;
            let stat = fields.next()?;
            let args: String = fields.collect::<Vec<_>>().join(" ");
            (ppid == me && !stat.starts_with('Z') && args.contains("dsh")).then_some(pid)
        })
        .collect()
}

/// Direct-child identity recorded BEFORE provider launch (the ambient
/// baseline). Strays are provider-owned direct children spawned since the
/// capture. The baseline owns NO close path itself: it never calls
/// `provider.shutdown` (the retained cleanup owner performs that exactly
/// once) and never deletes any lease — it only OBSERVES and, as a
/// test-only last resort, signals stray direct children. It therefore
/// cannot race a second provider close or destroy evidence for a
/// possibly-live lease.
#[cfg(unix)]
struct ChildBaseline(Vec<i32>);

#[cfg(unix)]
impl ChildBaseline {
    fn capture() -> Self {
        Self(live_dsh_children())
    }

    fn strays(&self) -> Vec<i32> {
        live_dsh_children()
            .into_iter()
            .filter(|pid| !self.0.contains(pid))
            .collect()
    }

    /// Boundedly observe stray exit (5s), then TERM (5s), then KILL (5s).
    /// Returns `(needed_force, diagnostics)` — `needed_force` means the
    /// provider's own cleanup left a live child, i.e. cleanup was
    /// UNCONFIRMED. Zombies are excluded (dead, reaped by the OS at test
    /// process exit); only genuinely live processes count as leaked.
    fn force_reap_strays(&self) -> (bool, Vec<String>) {
        let mut diag = Vec::new();
        let observe = |secs: u64| {
            let deadline = Instant::now() + Duration::from_secs(secs);
            while Instant::now() < deadline && !self.strays().is_empty() {
                std::thread::sleep(Duration::from_millis(50));
            }
            self.strays()
        };
        let mut strays = observe(5);
        if strays.is_empty() {
            diag.push("all direct children exited within the observation window".to_string());
            return (false, diag);
        }
        diag.push(format!("strays alive after observation window: {strays:?}; TERM"));
        for pid in &strays {
            let _ = Command::new("kill").arg(pid.to_string()).status();
        }
        strays = observe(5);
        if strays.is_empty() {
            diag.push("TERM reaped the strays".to_string());
            return (true, diag);
        }
        diag.push(format!("TERM insufficient for {strays:?}; KILL"));
        for pid in &strays {
            let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
        }
        strays = observe(5);
        if strays.is_empty() {
            diag.push("KILL reaped the strays".to_string());
        } else {
            diag.push(format!("FAILED to reap strays: {strays:?}"));
        }
        (true, diag)
    }
}

/// Assert the provider left no live direct child; force-reap any straggler
/// so no detached process survives, then fail loudly — unconfirmed cleanup
/// is never reported as success.
#[cfg(unix)]
fn assert_no_stray_dsh_children(baseline: &ChildBaseline, what: &str) {
    let (needed_force, diag) = baseline.force_reap_strays();
    assert!(
        !needed_force,
        "{what}: unconfirmed child cleanup; diagnostics: {diag:?}"
    );
}

/// Non-secret request structure recorded by the proxy (never bodies,
/// headers, or credentials): endpoint shape, model id, tool advertisement
/// shape, message-role structure, and derived booleans against the KNOWN
/// test tokens (poison persona, `unknown tool "<name>"` rejections the
/// script itself provoked).
#[cfg(unix)]
#[derive(Debug, Clone)]
struct RequestRecord {
    path: String,
    model: String,
    has_tools_key: bool,
    tools_len: usize,
    message_count: usize,
    roles: Vec<String>,
    stream: bool,
    system_mentions_poison: bool,
    rejected_unknown_tools: Vec<String>,
}

/// One scripted proxy: each of the first `hostile_rounds` requests gets a
/// single unsolicited tool call (shell → editor → run_code) whose
/// arguments would create marker files / spawn a background process; every
/// later request gets the bounded final `stop` completion.
#[cfg(unix)]
struct Proxy {
    port: u16,
    records: Arc<Mutex<Vec<RequestRecord>>>,
}

#[cfg(unix)]
impl Proxy {
    async fn start(hostile_rounds: usize, markers: Arc<HostileMarkers>) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind loopback proxy");
        let port = listener.local_addr().expect("proxy addr").port();
        let records: Arc<Mutex<Vec<RequestRecord>>> = Arc::new(Mutex::new(Vec::new()));
        let task_records = Arc::clone(&records);
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _peer)) = listener.accept().await else {
                    return;
                };
                let conn_records = Arc::clone(&task_records);
                let conn_markers = Arc::clone(&markers);
                tokio::spawn(async move {
                    serve_connection(&mut socket, hostile_rounds, conn_records, conn_markers)
                        .await;
                });
            }
        });
        Self { port, records }
    }

    fn records(&self) -> Vec<RequestRecord> {
        self.records.lock().expect("records").clone()
    }
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

/// Read one HTTP request (head + content-length body) from a connection.
async fn read_request(socket: &mut tokio::net::TcpStream) -> (String, Vec<u8>) {
    let mut buf = Vec::with_capacity(8192);
    let mut chunk = [0u8; 8192];
    let header_end = loop {
        if let Some(pos) = find_header_end(&buf) {
            break pos;
        }
        let read = socket.read(&mut chunk).await.expect("read request head");
        assert!(read > 0, "connection closed before request headers");
        buf.extend_from_slice(&chunk[..read]);
        assert!(buf.len() < (1 << 20), "request head unbounded");
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let content_length = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().expect("content-length"))
        })
        .expect("the dsh LLM client sends a content-length request body");
    let mut body = buf[header_end + 4..].to_vec();
    while body.len() < content_length {
        let read = socket.read(&mut chunk).await.expect("read request body");
        assert!(read > 0, "connection closed mid-body");
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(content_length);
    (head, body)
}

/// One SSE chunk (`data: <json>\n\n`).
fn sse_chunk(delta: serde_json::Value, finish_reason: Option<&str>) -> String {
    let payload = serde_json::json!({
        "id": "chatcmpl-dsh-t3-proxy",
        "object": "chat.completion.chunk",
        "created": 1_700_000_000,
        "model": "deepseek-v4-flash",
        "choices": [{"index": 0, "delta": delta, "finish_reason": finish_reason}],
    });
    format!("data: {payload}\n\n")
}

fn sse_usage_chunk() -> String {
    let payload = serde_json::json!({
        "id": "chatcmpl-dsh-t3-proxy",
        "object": "chat.completion.chunk",
        "created": 1_700_000_000,
        "model": "deepseek-v4-flash",
        "choices": [],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15},
    });
    format!("data: {payload}\n\n")
}

/// The scripted unsolicited tool call for one hostile round: a tool that
/// the sealed composition does NOT register, with arguments whose
/// execution would leave observable evidence. The shell body creates a
/// marker file AND starts a bounded background process whose OWN command
/// line carries the unique token (`python3 -c '...' <token>` — the
/// recorded `$!` pid IS the token-bearing process; there is no wrapper
/// and no nested untracked sleeper, so killing that pid removes the
/// token). The marker parent exists, so any body WOULD succeed if
/// dispatched (see the positive-control test).
#[cfg(unix)]
fn hostile_call(round: usize, markers: &HostileMarkers) -> (&'static str, String) {
    match round {
        0 => (
            "shell",
            format!(
                "{{\"command\": \"touch '{}'; python3 -c 'import time; time.sleep(30)' '{}' & echo $! > '{}'\"}}",
                markers.shell.display(),
                markers.process_token,
                markers.shell_pid.display()
            ),
        ),
        1 => (
            "str_replace_editor",
            format!(
                "{{\"command\": \"create\", \"path\": \"{}\", \"file_text\": \"pwned\"}}",
                markers.editor.display()
            ),
        ),
        _ => (
            "run_code",
            format!(
                "{{\"code\": \"open(r'{}', 'w').write('pwned')\"}}",
                markers.run_code.display()
            ),
        ),
    }
}

#[cfg(unix)]
async fn serve_connection(
    socket: &mut tokio::net::TcpStream,
    hostile_rounds: usize,
    records: Arc<Mutex<Vec<RequestRecord>>>,
    markers: Arc<HostileMarkers>,
) {
    let (head, body) = read_request(socket).await;
    let request_path = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or_default()
        .to_string();
    let parsed: serde_json::Value = serde_json::from_slice(&body).expect("request JSON");
    let raw = String::from_utf8_lossy(&body).into_owned();
    let messages = parsed["messages"].as_array().expect("messages array");
    let request_index = {
        let mut guard = records.lock().expect("records");
        guard.push(RequestRecord {
            path: request_path,
            model: parsed["model"].as_str().unwrap_or_default().to_string(),
            has_tools_key: parsed.get("tools").is_some(),
            tools_len: parsed["tools"].as_array().map_or(0, Vec::len),
            message_count: messages.len(),
            roles: messages
                .iter()
                .filter_map(|m| m["role"].as_str().map(str::to_string))
                .collect(),
            stream: parsed["stream"].as_bool().unwrap_or(false),
            system_mentions_poison: messages
                .first()
                .is_some_and(|m| m["role"].as_str() == Some("system"))
                && raw.contains(POISON_TOKEN),
            rejected_unknown_tools: ["shell", "str_replace_editor", "run_code"]
                .into_iter()
                .filter(|name| raw.contains(&format!("unknown tool \\\"{name}\\\"")))
                .map(str::to_string)
                .collect(),
        });
        guard.len()
    };

    let mut out = String::new();
    if request_index <= hostile_rounds {
        let (name, arguments) = hostile_call(request_index - 1, &markers);
        out += &sse_chunk(
            serde_json::json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "index": 0,
                    "id": format!("proxy-call-{request_index}"),
                    "type": "function",
                    "function": {"name": name, "arguments": arguments},
                }],
            }),
            None,
        );
        out += &sse_chunk(serde_json::json!({}), Some("tool_calls"));
    } else {
        out += &sse_chunk(
            serde_json::json!({"role": "assistant", "content": FINAL_TEXT}),
            None,
        );
        out += &sse_chunk(serde_json::json!({}), Some("stop"));
    }
    out += &sse_usage_chunk();
    out += "data: [DONE]\n\n";

    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{}",
        out.len(),
        out
    );
    use tokio::io::AsyncWriteExt;
    socket
        .write_all(response.as_bytes())
        .await
        .expect("write SSE response");
    let _ = socket.shutdown().await;
}

/// Collect a turn's events, expecting a clean stream (no Err items).
#[cfg(unix)]
async fn collect_turn(stream: nexus_agent_host::capability::model::HostEventStream) -> Vec<HostEvent> {
    let results: Vec<_> = stream.collect().await;
    results
        .into_iter()
        .map(|r| r.expect("stream item should be Ok"))
        .collect()
}

#[cfg(unix)]
async fn run_prompt(
    provider: &DshNativeProvider,
    handle: &nexus_agent_host::capability::model::ManagedSessionHandle,
    text: &str,
    scope: Option<PromptPermissionScope>,
) -> Vec<HostEvent> {
    let stream = provider
        .execute(
            handle,
            HostOperation::Prompt {
                op_id: HostOperationId::new(),
                content: vec![HostContentBlock::Text {
                    text: text.to_string(),
                }],
                permission_scope: scope,
            },
        )
        .await
        .expect("prompt admission succeeds");
    collect_turn(stream).await
}

/// Read the poison marker log as parsed JSON lines (empty when absent).
#[cfg(unix)]
fn poison_log_entries(root: &TempDir) -> Vec<serde_json::Value> {
    let path = root.path().join("poison.jsonl");
    if !path.exists() {
        return Vec::new();
    }
    std::fs::read_to_string(path)
        .expect("poison log")
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// Run `body` (the assertion phase borrowing the provider and a CLONE of
/// the handle) with failure-safe teardown. `baseline` was captured before
/// launch and owns no close path (never a second provider shutdown, never
/// a lease deletion), so it cannot race the retained cleanup owner.
///
/// - Any panic/early assertion failure inside `body` triggers the retained
///   bounded provider shutdown; if that cleanup is UNCONFIRMED, the
///   baseline independently observes bounded child exit and, as a
///   test-only last resort, TERM/KILLs stray direct children (without
///   touching the possibly-live lease) before the ORIGINAL panic is
///   re-thrown with cleanup diagnostics attached — the integration binary
///   never continues with a leaked child.
/// - On the success path the shutdown must be confirmed AND no stray
///   direct child may remain, otherwise the test fails: unconfirmed
///   cleanup is never silently passed.
#[cfg(unix)]
async fn with_confirmed_teardown<Fut: Future<Output = ()>>(
    provider: &DshNativeProvider,
    handle: ManagedSessionHandle,
    baseline: &ChildBaseline,
    body: Fut,
) {
    let outcome = AssertUnwindSafe(body).catch_unwind().await;
    let cleanup =
        tokio::time::timeout(Duration::from_secs(30), provider.shutdown(handle)).await;
    match (outcome, cleanup) {
        (Ok(()), Ok(Ok(()))) => {
            let (needed_force, diag) = baseline.force_reap_strays();
            assert!(
                !needed_force,
                "unconfirmed child cleanup after a passing body; diagnostics: {diag:?}"
            );
        }
        (Ok(()), cleanup) => {
            let (_, diag) = baseline.force_reap_strays();
            panic!(
                "unconfirmed cleanup after a passing body: {cleanup:?}; child diagnostics: {diag:?}"
            );
        }
        (Err(panic), cleanup) => {
            let (_needed_force, diag) = baseline.force_reap_strays();
            let leaked = baseline.strays();
            eprintln!(
                "teardown after failure: provider shutdown = {cleanup:?}; child reap diagnostics = {diag:?}"
            );
            if leaked.is_empty() {
                // No leak: preserve the ORIGINAL failure exactly.
                std::panic::resume_unwind(panic);
            }
            // A leak the forced reap could not clear: fail (never mask,
            // never continue quietly) with the original panic message
            // attached to the cleanup diagnostics.
            let original = panic
                .downcast_ref::<&'static str>()
                .map(|s| (*s).to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic>".to_string());
            panic!(
                "a provider-owned child survived every teardown attempt: {leaked:?}; diagnostics: {diag:?}; original panic: {original}"
            );
        }
    }
}

/// RAII guard for the identity test's directly spawned child: kill + wait
/// on every early exit; disarmed only after the child is confirmed exited.
struct ChildGuard(Option<std::process::Child>);

impl ChildGuard {
    fn new(child: std::process::Child) -> Self {
        Self(Some(child))
    }

    fn child(&mut self) -> &mut std::process::Child {
        self.0.as_mut().expect("child guard armed")
    }

    fn disarm(mut self) {
        self.0.take();
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Positive control for the side-effect arms: every scripted hostile
/// body, executed directly, DOES produce its observable evidence (marker
/// file, and a recorded live pid that IS the token-bearing bounded
/// process — no wrapper, no nested untracked sleeper). This makes the
/// sealed proof's absence assertions meaningful: the bodies are
/// effective; they simply never ran. Cleans up safely (killing the
/// recorded pid removes the token; no descendant can survive it) and
/// re-verifies total absence. POSIX-only; never runs on non-Unix suites.
#[cfg(unix)]
#[test]
fn hostile_bodies_create_their_evidence_when_directly_run() {
    let _serial = serial_real_runtime();
    if !proof_tool_capability() {
        return;
    }
    let root = tempfile::tempdir().expect("temp root");
    let markers = HostileMarkers::under(&root);

    // shell body: marker file + uniquely identifiable bounded process.
    let (name, arguments) = hostile_call(0, &markers);
    assert_eq!(name, "shell");
    let arguments: serde_json::Value = serde_json::from_str(&arguments).expect("args json");
    let command = arguments["command"].as_str().expect("shell command");
    let status = Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .status()
        .expect("run shell body");
    assert!(status.success(), "the shell body is effective when run");
    assert!(markers.shell.exists(), "the shell body created its marker");
    let pid: i32 = std::fs::read_to_string(&markers.shell_pid)
        .expect("pid marker")
        .trim()
        .parse()
        .expect("numeric pid");
    assert!(pid_alive(pid), "the bounded background process is alive");
    assert!(hostile_token_alive(&markers.process_token));
    // Clean the control process safely: bounded TERM, KILL fallback, then
    // observed death.
    let _ = Command::new("kill").arg(pid.to_string()).status();
    let deadline = Instant::now() + Duration::from_secs(5);
    while pid_alive(pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    if pid_alive(pid) {
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while pid_alive(pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!pid_alive(pid), "the control process was reaped");
    assert!(!hostile_token_alive(&markers.process_token));

    // str_replace_editor body: creating the file IS the tool effect.
    let (name, arguments) = hostile_call(1, &markers);
    assert_eq!(name, "str_replace_editor");
    let arguments: serde_json::Value = serde_json::from_str(&arguments).expect("args json");
    assert_eq!(arguments["command"].as_str(), Some("create"));
    std::fs::write(
        arguments["path"].as_str().expect("editor path"),
        arguments["file_text"].as_str().expect("editor text"),
    )
    .expect("run editor body");
    assert_eq!(
        std::fs::read_to_string(&markers.editor).expect("editor marker"),
        "pwned",
        "the editor body created its marker"
    );

    // run_code body: execute the exact python payload.
    let (name, arguments) = hostile_call(2, &markers);
    assert_eq!(name, "run_code");
    let arguments: serde_json::Value = serde_json::from_str(&arguments).expect("args json");
    let status = Command::new("python3")
        .arg("-c")
        .arg(arguments["code"].as_str().expect("run_code payload"))
        .status()
        .expect("run run_code body");
    assert!(status.success(), "the run_code body is effective when run");
    assert!(markers.run_code.exists(), "the run_code body created its marker");

    // Safe cleanup: every marker removed; total absence re-verified.
    for path in [
        &markers.shell,
        &markers.shell_pid,
        &markers.editor,
        &markers.run_code,
    ] {
        std::fs::remove_file(path).expect("remove control marker");
    }
    markers.assert_all_absent();
}

/// The actual upstream dsh sealed `deny_all` composition enforces no-tools
/// at dispatch (architecture §3.4 / AC6): the model request advertises NO
/// tools, unsolicited shell/editor/run_code calls are rejected as unknown
/// tools, and no marker file/process/tool-body side effect occurs before
/// the scripted bounded completion.
#[cfg(unix)]
#[tokio::test]
async fn real_dsh_sealed_deny_all_rejects_unsolicited_tools_without_side_effects() {
    let Some(dsh_bin) = real_dsh() else { return };
    let _serial = serial_real_runtime();
    if !proof_tool_capability() {
        return;
    }
    let baseline = ChildBaseline::capture();
    let root = tempfile::tempdir().expect("temp root");
    std::fs::create_dir_all(root.path().join("home")).expect("isolated HOME");
    poison_ordinary_home(&root);
    let markers = Arc::new(HostileMarkers::under(&root));
    let proxy = Proxy::start(3, Arc::clone(&markers)).await;
    let provider = real_provider(&root, proxy.port, &dsh_bin);

    let handle = provider
        .launch(launch_spec(root.path()))
        .await
        .expect("the ordinary recipe initializes against the poisoned home");
    let body = {
        let handle = handle.clone();
        let provider = &provider;
        let proxy = &proxy;
        async move {
            let events =
                run_prompt(provider, &handle, "attempt no tool use", Some(deny_all_scope())).await;

        // The turn completes bounded through the scripted proxy: exactly
        // one OpStarted, the final text delta, one OpFinished(EndTurn).
        assert!(
            matches!(events.first(), Some(HostEvent::OpStarted(_))),
            "events: {events:?}"
        );
        let deltas: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                HostEvent::MessageDelta(delta) => Some(delta.text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(deltas, vec![FINAL_TEXT], "events: {events:?}");
        assert!(
            matches!(events.last(), Some(HostEvent::OpFinished(f)) if f.reason == FinishReason::EndTurn),
            "the bounded final completion ends the turn: {events:?}"
        );

        // Exactly the scripted model calls happened: three hostile
        // tool-call responses, then the bounded final completion.
        let records = proxy.records();
        assert_eq!(
            records.len(),
            4,
            "three rejected tool calls plus the final completion: {records:?}"
        );
        for (index, record) in records.iter().enumerate() {
            assert_eq!(record.path, "/chat/completions");
            assert!(
                !record.model.is_empty(),
                "the runtime names the model it calls: {record:?}"
            );
            assert!(
                record.message_count == record.roles.len()
                    && record.message_count
                        == records
                            .first()
                            .map_or(record.message_count, |first| {
                                first.message_count + 2 * index
                            }),
                "conversation history grows by one assistant+tool pair per rejection: {record:?}"
            );
            assert!(record.stream, "the dsh LLM client always streams");
            assert!(
                !record.has_tools_key && record.tools_len == 0,
                "request {} must advertise NO tools (the `tools` key is absent): {record:?}",
                index + 1
            );
            assert!(
                !record.system_mentions_poison,
                "request {} must not carry the poisoned home layer: {record:?}",
                index + 1
            );
        }
        assert_eq!(
            records[0].roles,
            vec!["system".to_string(), "user".to_string()],
            "the sealed composition sends a minimal first request: {:?}",
            records[0].roles
        );
        }
    };
    with_confirmed_teardown(&provider, handle, &baseline, body).await;

    // After the bounded model completion AND the confirmed provider
    // shutdown, separately prove the dispatch-level rejections and the
    // absence of every side effect (file, pid file, live process).
    let records = proxy.records();
    for (index, tool) in [
        (1usize, "shell"),
        (2usize, "str_replace_editor"),
        (3usize, "run_code"),
    ] {
        assert!(
            records[index]
                .rejected_unknown_tools
                .iter()
                .any(|name| name == tool),
            "the {tool} rejection is visible on request {}: {:?}",
            index + 1,
            records[index].rejected_unknown_tools
        );
    }
    markers.assert_all_absent();
    assert_no_stray_dsh_children(&baseline, "sealed deny_all shutdown");

    // The poisoned marker plugin booted ONLY in the ordinary launch (the
    // provider initializes the ordinary recipe at launch); the sealed
    // switch booted a runtime that never touched the poisoned layers.
    let dsh_home = std::fs::canonicalize(root.path().join("dsh-home")).expect("canonical home");
    let poison = poison_log_entries(&root);
    assert_eq!(
        poison.len(),
        2,
        "the poison plugin loads once (module + apply) in the ordinary boot only: {poison:?}"
    );
    for entry in &poison {
        let booted_home = std::fs::canonicalize(
            entry["dshHome"].as_str().expect("poison log carries dshHome"),
        )
        .expect("poison boot home resolves");
        assert_eq!(
            booted_home, dsh_home,
            "the poison plugin only ever booted against the ordinary home: {entry:?}"
        );
    }

    // Confirmed shutdown deleted the sealed child home lease (the `nexus`
    // subtree holds no remaining lease).
    let nexus_dir = dsh_home.join("nexus");
    let lease_remains = nexus_dir.exists()
        && std::fs::read_dir(&nexus_dir)
            .expect("read nexus dir")
            .next()
            .is_some();
    assert!(
        !lease_remains,
        "the confirmed close deleted the sealed child home lease"
    );
}

/// Non-vacuity arm for the sealed proof: the SAME poisoned layers
/// observably enter the ORDINARY runtime — the model request carries the
/// poison persona token and DOES advertise tools, and the marker plugin
/// boots against the ordinary home. Without this arm, "absent from the
/// sealed runtime" would prove nothing.
#[cfg(unix)]
#[tokio::test]
async fn real_dsh_poisoned_layers_reach_ordinary_runtime() {
    let Some(dsh_bin) = real_dsh() else { return };
    let _serial = serial_real_runtime();
    if !proof_tool_capability() {
        return;
    }
    let baseline = ChildBaseline::capture();
    let root = tempfile::tempdir().expect("temp root");
    std::fs::create_dir_all(root.path().join("home")).expect("isolated HOME");
    poison_ordinary_home(&root);
    let markers = Arc::new(HostileMarkers::under(&root));
    let proxy = Proxy::start(0, Arc::clone(&markers)).await;
    let provider = real_provider(&root, proxy.port, &dsh_bin);

    let handle = provider
        .launch(launch_spec(root.path()))
        .await
        .expect("the ordinary recipe initializes against the poisoned home");
    let body = {
        let handle = handle.clone();
        let provider = &provider;
        let proxy = &proxy;
        let root = &root;
        async move {
            let events = run_prompt(provider, &handle, "ordinary turn", None).await;
        assert!(
            matches!(events.last(), Some(HostEvent::OpFinished(f)) if f.reason == FinishReason::EndTurn),
            "the ordinary turn completes: {events:?}"
        );

        let records = proxy.records();
        assert_eq!(records.len(), 1, "one scripted completion: {records:?}");
        assert!(
            records[0].has_tools_key && records[0].tools_len > 0,
            "the ordinary runtime DOES advertise tools (contrast with the sealed proof): {:?}",
            records[0]
        );
        assert!(
            records[0].system_mentions_poison,
            "the poisoned home layer reached the ordinary model request: {:?}",
            records[0]
        );

        let poison = poison_log_entries(root);
        assert_eq!(
            poison.len(),
            2,
            "the poison plugin booted in the ordinary runtime: {poison:?}"
        );
        }
    };
    with_confirmed_teardown(&provider, handle, &baseline, body).await;
    assert_no_stray_dsh_children(&baseline, "ordinary poison shutdown");
}

/// A missing required plugin package is a STOP, never an ordinary
/// fallback: the poisoned profile patch names a package that does not
/// exist, the real runtime fails the initialize handshake, and the
/// provider launch fails closed with a typed error and zero model calls.
#[cfg(unix)]
#[tokio::test]
async fn real_dsh_missing_plugin_package_fails_closed() {
    let Some(dsh_bin) = real_dsh() else { return };
    let _serial = serial_real_runtime();
    if !proof_tool_capability() {
        return;
    }
    let baseline = ChildBaseline::capture();
    let root = tempfile::tempdir().expect("temp root");
    std::fs::create_dir_all(root.path().join("home")).expect("isolated HOME");
    let profile = root.path().join("dsh-home").join("profiles").join("sdk");
    std::fs::create_dir_all(&profile).expect("profile dir");
    std::fs::write(
        profile.join("cordis.patch.yml"),
        "- insert:\n    - id: missing-plugin\n      name: dsh-missing-plugin-package-zzz\n",
    )
    .expect("missing-package profile patch");
    let markers = Arc::new(HostileMarkers::under(&root));
    let proxy = Proxy::start(0, Arc::clone(&markers)).await;
    let provider = real_provider(&root, proxy.port, &dsh_bin);

    let result = provider.launch(launch_spec(root.path())).await;
    assert!(
        matches!(result, Err(HostError::LaunchFailed { .. })),
        "a missing plugin package fails the launch closed: {result:?}"
    );
    assert!(
        proxy.records().is_empty(),
        "no model call ever happened on the failed launch"
    );
    // Launch failure is not proven cleanup until the failed-init child is
    // observed reaped and no Nexus sealed lease survives; this arm fails
    // closed if cleanup is unconfirmed (strays are killed, then the test
    // fails loudly).
    assert_no_stray_dsh_children(&baseline, "missing plugin package launch");
    let nexus_dir = root.path().join("dsh-home").join("nexus");
    let lease_remains = nexus_dir.exists()
        && std::fs::read_dir(&nexus_dir)
            .expect("read nexus dir")
            .next()
            .is_some();
    assert!(
        !lease_remains,
        "no Nexus sealed lease remains after the failed launch"
    );
    markers.assert_all_absent();
}


/// Wall-clock capture for the P1 actual-dsh streaming timing proof.
#[cfg(unix)]
struct TurnTiming {
    first_message_delta: Option<Instant>,
    terminal: Option<Instant>,
    events: Vec<HostEvent>,
}

#[cfg(unix)]
async fn collect_turn_timed(
    stream: nexus_agent_host::capability::model::HostEventStream,
) -> TurnTiming {
    let mut first_message_delta = None;
    let mut terminal = None;
    let mut events = Vec::new();
    let mut stream = std::pin::pin!(stream);
    while let Some(item) = stream.next().await {
        let event = item.expect("stream item should be Ok");
        let now = Instant::now();
        if matches!(event, HostEvent::MessageDelta(_)) && first_message_delta.is_none() {
            first_message_delta = Some(now);
        }
        if matches!(event, HostEvent::OpFinished(_) | HostEvent::OpFailed(_)) && terminal.is_none()
        {
            terminal = Some(now);
        }
        events.push(event);
    }
    TurnTiming {
        first_message_delta,
        terminal,
        events,
    }
}

/// Actual installed `dsh` on a loopback LLM proxy: at least one committed
/// `MessageDelta` is observed before the terminal, with monotonic Instant
/// evidence logged for parent verification (P1 T3 / AC2).
#[cfg(unix)]
#[tokio::test]
async fn real_dsh_message_delta_precedes_terminal_with_timing_evidence() {
    let Some(dsh_bin) = real_dsh() else {
        return;
    };
    let _serial = serial_real_runtime();
    let baseline = ChildBaseline::capture();
    let root = tempfile::tempdir().expect("temp root");
    std::fs::create_dir_all(root.path().join("home")).expect("isolated HOME");
    let markers = Arc::new(HostileMarkers::under(&root));
    let proxy = Proxy::start(0, Arc::clone(&markers)).await;
    let provider = real_provider(&root, proxy.port, &dsh_bin);

    let handle = provider
        .launch(launch_spec(root.path()))
        .await
        .expect("ordinary launch against isolated home");
    let body = {
        let handle = handle.clone();
        let provider = &provider;
        async move {
            let stream = provider
                .execute(
                    &handle,
                    HostOperation::Prompt {
                        op_id: HostOperationId::new(),
                        content: vec![HostContentBlock::Text {
                            text: "prove message-level streaming timing".to_string(),
                        }],
                        permission_scope: None,
                    },
                )
                .await
                .expect("prompt admission succeeds");
            let timing = collect_turn_timed(stream).await;
            assert!(
                matches!(timing.events.first(), Some(HostEvent::OpStarted(_))),
                "events: {:?}",
                timing.events
            );
            let deltas: Vec<&str> = timing
                .events
                .iter()
                .filter_map(|e| match e {
                    HostEvent::MessageDelta(delta) => Some(delta.text.as_str()),
                    _ => None,
                })
                .collect();
            assert!(
                !deltas.is_empty(),
                "the runtime must emit at least one MessageDelta: {:?}",
                timing.events
            );
            assert!(
                matches!(
                    timing.events.last(),
                    Some(HostEvent::OpFinished(f)) if f.reason == FinishReason::EndTurn
                ),
                "bounded completion: {:?}",
                timing.events
            );
            let (Some(first_delta), Some(terminal_at)) =
                (timing.first_message_delta, timing.terminal)
            else {
                panic!("timing arms must be populated: {:?}", timing.events);
            };
            assert!(
                first_delta <= terminal_at,
                "MessageDelta must not follow the terminal instant"
            );
            let lead_ms = terminal_at
                .saturating_duration_since(first_delta)
                .as_millis();
            eprintln!(
                "P1 actual-dsh timing evidence: first MessageDelta observed {lead_ms}ms                  (monotonic Instant) before OpFinished/OpFailed on this host"
            );
            assert_eq!(proxy.records().len(), 1, "one scripted model call");
        }
    };
    with_confirmed_teardown(&provider, handle, &baseline, body).await;
}

/// Record the actual runtime identity without reading credentials: CLI
/// version output, the installed npm package identity (name/version), and
/// the wire protocol server identity from a direct `initialize` handshake
/// under an isolated HOME/DSH_HOME.
#[test]
fn real_dsh_runtime_identity_is_recorded() {
    let Some(dsh_bin) = real_dsh() else { return };
    let _serial = serial_real_runtime();

    // CLI version (non-secret).
    let version_out = Command::new(&dsh_bin)
        .arg("--version")
        .env("DSH_TELEMETRY_DISABLED", "1")
        .output()
        .expect("run dsh --version");
    assert!(version_out.status.success(), "dsh --version must succeed");
    let cli_version = String::from_utf8_lossy(&version_out.stdout).trim().to_string();
    assert!(!cli_version.is_empty(), "a version string is recorded");

    // Installed package identity: the resolved executable is the package's
    // `lib/bin.js`; its `package.json` is two directories up.
    let canonical = std::fs::canonicalize(&dsh_bin).expect("canonical dsh");
    let package_json_path = canonical
        .parent()
        .and_then(Path::parent)
        .map(|dir| dir.join("package.json"))
        .expect("package layout");
    let package_json: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&package_json_path).expect("read package.json"),
    )
    .expect("parse package.json");
    let package_name = package_json["name"].as_str().expect("package name");
    let package_version = package_json["version"].as_str().expect("package version");
    assert_eq!(package_name, "@deepseek-ai/dsh");

    // Wire protocol identity: direct initialize handshake under isolation.
    let root = tempfile::tempdir().expect("temp root");
    let home = root.path().join("home");
    let dsh_home = root.path().join("dsh-home");
    std::fs::create_dir_all(&home).expect("home");
    std::fs::create_dir_all(&dsh_home).expect("dsh home");
    let mut guard = ChildGuard::new(
        Command::new(&dsh_bin)
            .args(["--profile", "sdk"])
            .env("HOME", &home)
            .env("DSH_HOME", &dsh_home)
            .env("DSH_TELEMETRY_DISABLED", "1")
            .current_dir(root.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn dsh for identity handshake"),
    );
    let mut stdin = guard.child().stdin.take().expect("stdin");
    let mut stdout = guard.child().stdout.take().expect("stdout");
    let (reply_tx, reply_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::Read as _;
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            match stdout.read(&mut byte) {
                Ok(0) => break,
                Ok(_) => {
                    if byte[0] == b'\n' {
                        let _ = reply_tx.send(String::from_utf8_lossy(&line).into_owned());
                        line.clear();
                    } else {
                        line.push(byte[0]);
                    }
                }
                Err(_) => break,
            }
        }
    });
    let initialize = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "t3-identity",
        "method": "initialize",
        "params": {
            "cwd": root.path().to_string_lossy(),
            "provider": "deepseek-official",
            "model": "deepseek-v4-flash",
        },
    });
    writeln!(stdin, "{initialize}").expect("write initialize");
    stdin.flush().expect("flush");
    let deadline = Instant::now() + Duration::from_secs(60);
    let reply = loop {
        match reply_rx.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                if line.contains("\"t3-identity\"") {
                    break line;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                assert!(Instant::now() < deadline, "initialize reply deadline");
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                panic!("dsh exited before the initialize reply")
            }
        }
    };
    let reply: serde_json::Value = serde_json::from_str(&reply).expect("initialize reply JSON");
    let server_name = reply["result"]["serverInfo"]["name"]
        .as_str()
        .expect("server identity name");
    let protocol_version = reply["result"]["serverInfo"]["version"]
        .as_str()
        .expect("server identity version");
    assert_eq!(server_name, "deepseek-harness-sdk-runtime");
    assert!(!protocol_version.is_empty());

    // Cooperative shutdown + EOF; the child exits on its own.
    writeln!(stdin, r#"{{"jsonrpc":"2.0","id":"t3-bye","method":"shutdown"}}"#)
        .expect("write shutdown");
    stdin.flush().expect("flush");
    drop(stdin);
    let exit_deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(Some(_status)) = guard.child().try_wait() {
            break;
        }
        assert!(
            Instant::now() <= exit_deadline,
            "dsh did not exit after shutdown + stdin EOF"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    // The child is confirmed exited; disarm so the guard does not kill a
    // reaped pid.
    guard.disarm();

    eprintln!(
        "actual dsh runtime identity: cli_version={cli_version} package={package_name}@{package_version} protocol_server={server_name}/{protocol_version} bin={}",
        canonical.display()
    );
}
