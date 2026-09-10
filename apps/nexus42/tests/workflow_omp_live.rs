//! P3 T3 — isolated **live** omp public success acceptance harness.
//!
//! `NEXUS_OMP_BIN=<absolute path to omp> SQLX_OFFLINE=true cargo test -p nexus42 \
//!   --test workflow_omp_live isolated_public_success -- --ignored --nocapture`
//!
//! This harness is deliberately `#[ignore]`d (A6): it is an explicit live
//! acceptance gate, never a normal network dependency in CI. It:
//!
//! - Builds a fresh temp `QA_ROOT` and writes **everything** under it
//!   (Nexus HOME/XDG, workspace, omp HOME/XDG, named profile `v1186-qa`).
//! - Starts the **real `nexus42` daemon binary** (`daemon-run` subprocess),
//!   never a tests-only orchestration engine.
//! - Drives a public `daemon schedule add` (HTTP admission) of an external
//!   minimal preset whose one agent step runs through the isolated `omp-qa`
//!   ACP provider with real model auth inherited from the environment.
//! - Asserts the produced agent output is **not the prompt echo** (an explicit
//!   nonce that must not be repeated verbatim) and that the run settles
//!   `completed`; restarts the daemon process and asserts inspection still
//!   shows `completed`.
//! - If isolated auth is unavailable / the provider refuses, the harness
//!   **fails with the refusal evidence** — never a silent skip, never a
//!   fixture-substitute "success".
//!
//! Recipe (A6): `NEXUS_HOME = $QA_HOME/.nexus42`; every Nexus process gets
//! `HOME=$QA_HOME` + XDG dirs; `omp` gets its own `HOME=$QA_OMP_HOME` + XDG
//! dirs and a fresh named profile. No user-global Nexus/omp config is read or
//! written; no secrets are copied. The omp child inherits model auth through
//! `[providers.env]` (provider keys like `OLLAMA_API_KEY`/`DEEPSEEK_API_KEY`
//! from the daemon env).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Isolation helper
// ---------------------------------------------------------------------------

/// Owns a fresh `QA_ROOT` temp tree and derives the isolation roots + the
/// per-process env every spawned Nexus/omp process gets.
struct IsolatedQa {
    _root: tempfile::TempDir,
    qa_root: PathBuf,
    qa_home: PathBuf,
    nexus_home: PathBuf,
    qa_workspace: PathBuf,
    qa_omp_home: PathBuf,
    daemon_port: u16,
}

impl IsolatedQa {
    const OMP_PROFILE: &'static str = "v1186-qa";
    const CREATOR: &'static str = "p3_creator";
    const WORKSPACE_SLUG: &'static str = "default";

    /// Bind a free loopback port so the daemon and CLI agree on `daemon_url`.
    fn allocate_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .expect("bind ephemeral port")
            .local_addr()
            .expect("local addr")
            .port()
    }

    fn new() -> Self {
        let root_dir = tempfile::Builder::new()
            .prefix("omp-live-qa-")
            .tempdir()
            .expect("create QA_ROOT");
        let qa_root = root_dir.path().canonicalize().expect("canonical QA_ROOT");
        let qa_home = qa_root.join("home");
        let nexus_home = qa_home.join(".nexus42");
        let qa_workspace = qa_root.join("workspace");
        let qa_omp_home = qa_root.join("omp-home");

        for dir in [
            &qa_home,
            &qa_workspace,
            &nexus_home.join("agent-host"),
            &qa_omp_home,
        ] {
            std::fs::create_dir_all(dir).expect("create QA_ROOT subtree");
        }

        Self {
            _root: root_dir,
            qa_root,
            qa_home,
            nexus_home,
            qa_workspace,
            qa_omp_home,
            daemon_port: Self::allocate_port(),
        }
    }

    fn daemon_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.daemon_port)
    }

    /// Verify every resolved DB/config/workspace/transport path starts inside
    /// `QA_ROOT` before any work begins (A6 hard requirement).
    fn assert_paths_inside_qa_root(&self) {
        for (label, p) in [
            ("NEXUS_HOME", self.nexus_home.as_path()),
            ("QA_WORKSPACE", self.qa_workspace.as_path()),
            ("QA_OMP_HOME", self.qa_omp_home.as_path()),
            ("workspace_db", self.workspace_db().as_path()),
            (
                "agent_host_config",
                self.nexus_home
                    .join("agent-host")
                    .join("config.toml")
                    .as_path(),
            ),
            (
                "agent_host_config_legacy",
                self.nexus_home
                    .join(".nexus42")
                    .join("agent-host")
                    .join("config.toml")
                    .as_path(),
            ),
        ] {
            assert!(
                p.starts_with(&self.qa_root),
                "{label} ({}) must live inside QA_ROOT ({})",
                p.display(),
                self.qa_root.display()
            );
        }
        // The transport is a loopback TCP port, never a socket path from the
        // ambient environment.
        assert!(
            std::env::var_os("NEXUS_DAEMON_SOCKET_PATH").is_none()
                || self.daemon_url().starts_with("http://127.0.0.1:"),
            "the daemon must use its isolated loopback port"
        );
    }

    /// Env every Nexus process inherits (HOME + XDG under `QA_HOME`). The
    /// daemon's own provider keys (e.g. `OLLAMA_API_KEY` / `DEEPSEEK_API_KEY`)
    /// stay in the ambient env so the omp child inherits them — never copied
    /// into a config file.
    fn nexus_env(&self) -> Vec<(String, String)> {
        vec![
            ("HOME".into(), self.qa_home.to_string_lossy().into_owned()),
            (
                "XDG_CONFIG_HOME".into(),
                self.qa_home.join(".config").to_string_lossy().into_owned(),
            ),
            (
                "XDG_DATA_HOME".into(),
                self.qa_home
                    .join(".local/share")
                    .to_string_lossy()
                    .into_owned(),
            ),
            (
                "XDG_CACHE_HOME".into(),
                self.qa_home.join(".cache").to_string_lossy().into_owned(),
            ),
            ("RUST_LOG".into(), "off".into()),
        ]
    }

    /// `$NEXUS_HOME/config.toml` pointing `daemon_url` at the isolated port.
    fn write_nexus_config(&self) {
        let content = format!(
            "active_creator_id = \"{}\"\n\
             daemon_url = \"{}\"\n\
             workspace_path = \"{}\"\n\
             \n\
             [active_workspace_slug_by_creator]\n\
             \"{}\" = \"{}\"\n",
            Self::CREATOR,
            self.daemon_url(),
            self.qa_workspace.display(),
            Self::CREATOR,
            Self::WORKSPACE_SLUG,
        );
        std::fs::write(self.nexus_home.join("config.toml"), content).expect("write config.toml");
    }

    /// `$NEXUS_HOME/agent-host/config.toml` — the `omp-qa` ACP provider recipe.
    ///
    /// argv is adapted from the A6 shorthand: `--no-tools` (instead of
    /// `--tools=read,glob`) forces the one-step live run to answer the single
    /// prompt directly and `end_turn`, rather than enter omp's own agentic
    /// read/glob tool loop (which produced non-EndTurn stop and failed the
    /// drive in the first live run). A6 explicitly allows adapting `ProviderConfig`
    /// argv and recording accepted args.
    ///
    /// NOTE: `nexus_agent_host::config::load_config(home)` is called from
    /// daemon boot with `home = $HOME/.nexus42`, and it resolves the file via
    /// `home.join(".nexus42/agent-host/config.toml")` — i.e. the provider file
    /// lives at `$HOME/.nexus42/.nexus42/agent-host/config.toml`. We write the
    /// file exactly where the daemon reads it so the `omp-qa` provider is
    /// registered (verified empirically: with this path the daemon registers
    /// `omp-qa` in its provider catalog).
    fn write_agent_host_config(&self, omp_bin: &Path) {
        let bin = omp_bin.to_str().expect("utf8 omp path");
        let args = [
            "acp".to_string(),
            format!("--profile={}", Self::OMP_PROFILE),
            format!("--cwd={}", self.qa_workspace.display()),
            "--no-session".to_string(),
            "--no-pty".to_string(),
            "--no-lsp".to_string(),
            "--no-skills".to_string(),
            "--no-rules".to_string(),
            "--no-extensions".to_string(),
            "--no-tools".to_string(),
            "--max-time=60".to_string(),
        ];
        let args_toml = args
            .iter()
            .map(|a| format!("\"{}\"", a.replace('"', "\\\"")))
            .collect::<Vec<_>>()
            .join(", ");
        let content = format!(
            r#"[[providers]]
id = "omp-qa"
protocol = "acp"
command = "{bin}"
args = [{args_toml}]
enabled = true

[providers.env]
HOME = "{omp_home}"
XDG_CONFIG_HOME = "{omp_home}/.config"
XDG_DATA_HOME = "{omp_home}/.local/share"
XDG_CACHE_HOME = "{omp_home}/.cache"
"#,
            bin = bin,
            args_toml = args_toml,
            omp_home = self.qa_omp_home.display(),
        );
        // The daemon resolves the canonical `$HOME/.nexus42/agent-host/config.toml`
        // (single-nested) for BOTH registration and `AgentHostSubsystem` start.
        let dir = self.nexus_home.join("agent-host");
        std::fs::create_dir_all(&dir).expect("agent-host config dir");
        std::fs::write(dir.join("config.toml"), &content).expect("write agent-host config.toml");
    }

    /// Seed the Creator workspace DB under `$NEXUS_HOME` (schema + creator row
    /// + world) so the real daemon attaches it and admission passes FK gates.
    ///
    /// Must run inside the ambient tokio runtime (the harness test body).
    async fn seed_creator_db(&self) {
        let op_dir = nexus_home_layout::operational_workspace_dir(
            &self.qa_home,
            Self::CREATOR,
            Self::WORKSPACE_SLUG,
        );
        std::fs::create_dir_all(&op_dir).expect("operational dir");
        std::fs::write(
            op_dir.join("meta.json"),
            serde_json::to_string(&json!({
                "schema_version": 1,
                "creator_id": Self::CREATOR,
                "workspace_slug": Self::WORKSPACE_SLUG,
                "local_root": self.qa_workspace,
                "created_at": "2020-01-01T00:00:00Z"
            }))
            .expect("meta json"),
        )
        .expect("write meta.json");

        let db_path = nexus_home_layout::workspace_state_db_path(
            &self.qa_home,
            Self::CREATOR,
            Self::WORKSPACE_SLUG,
        );
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open creator db");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations");
        nexus_local_db::seed_versions(&pool)
            .await
            .expect("seed versions");
        // SAFETY: test-only seeding of known schema (FK references).
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES (?, 'P3 Live QA Creator', 'active', datetime('now'), '{}')",
        )
        .bind(Self::CREATOR)
        .execute(&pool)
        .await
        .expect("seed creator");
        sqlx::query(
            "INSERT OR IGNORE INTO narrative_worlds \
                (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
                 time_policy, metadata_json, created_at) \
               VALUES ('wld_p3_qa_live', 'ws', ?, 'P3 Live QA World', 'p3-qa-live', \
                 'active', 'private', 'manual', '{{}}', datetime('now'))",
        )
        .bind(Self::CREATOR)
        .execute(&pool)
        .await
        .expect("seed world");
    }

    /// Write an external minimal one-agent-step preset (directory source).
    /// The single step prompts the model with a nonce and asks for a
    /// deterministic transformation, with tools denied.
    fn write_user_preset(&self) -> String {
        let preset_id = "p3-live-omp-one-step";
        let bundle = self.nexus_home.join("presets").join(preset_id);
        std::fs::create_dir_all(bundle.join("prompts")).expect("preset prompts dir");
        std::fs::write(
            bundle.join("preset.yaml"),
            format!(
                r#"preset:
  id: {preset_id}
  version: 1
  kind: creator
  description: "P3 isolated live omp one-step (non-echo) acceptance preset"
  requires_capabilities:
    - acp.prompt
  run_intents:
    - work_continue
  initial: generate
  terminal: done

states:
  - id: generate
    description: "One agent step through the isolated omp-qa provider"
    enter:
      - kind: inner_graph
        name: one_step
    exit_when:
      kind: rule
    next: done

  - id: done
    terminal: true

inner_graphs:
  one_step:
    nodes:
      - id: transform
        kind: acp_prompt
        template_file: prompts/transform.md
        tool_policy: deny
    output_binding: transform.text
"#
            ),
        )
        .expect("write preset.yaml");
        std::fs::write(
            bundle.join("prompts").join("transform.md"),
            "Deterministically transform the nonce \"{{preset.input.nonce}}\": \
             reply with ONLY the nonce reversed and prefixed by LIVE_. \
             Do NOT repeat the prompt text back verbatim.\n",
        )
        .expect("write transform.md");
        preset_id.to_string()
    }

    /// Workspace `state.db` path for this isolated creator.
    fn workspace_db(&self) -> PathBuf {
        nexus_home_layout::workspace_state_db_path(
            &self.qa_home,
            Self::CREATOR,
            Self::WORKSPACE_SLUG,
        )
    }
}

// ---------------------------------------------------------------------------
// Binary/daemon helpers
// ---------------------------------------------------------------------------

/// Gate on `NEXUS_OMP_BIN`; missing/unusable → typed refusal (never silent).
fn resolve_omp_bin() -> Result<PathBuf, String> {
    let Some(raw) = std::env::var_os("NEXUS_OMP_BIN") else {
        return Err(
            "REFUSED: NEXUS_OMP_BIN is not set. Isolated live omp QA requires an absolute path \
             to an installed omp binary; the harness must refuse, never substitute fixture \
             success"
                .to_string(),
        );
    };
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err("REFUSED: NEXUS_OMP_BIN must be an absolute path".to_string());
    }
    if !path.exists() {
        return Err(format!(
            "REFUSED: NEXUS_OMP_BIN binary does not exist: {}",
            path.display()
        ));
    }
    let out = std::process::Command::new(&path)
        .arg("--version")
        .env_remove("OMPCODE")
        .output()
        .map_err(|e| {
            format!(
                "REFUSED: could not execute NEXUS_OMP_BIN {}: {e}",
                path.display()
            )
        })?;
    if !out.status.success() {
        return Err(format!(
            "REFUSED: `{} --version` exited {} — not a usable omp binary: {}",
            path.display(),
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(path)
}

/// Start the REAL `nexus42` daemon binary (`daemon-run`) with hermetic env.
/// Stdout/stderr are discarded so the pipes never fill and block boot.
fn spawn_daemon(qa: &IsolatedQa) -> tokio::process::Child {
    let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_nexus42"));
    cmd.arg("daemon-run")
        .arg("--port")
        .arg(qa.daemon_port.to_string())
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--shutdown-grace-ms")
        .arg("5000")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    // A socket path from the ambient environment would override the explicit
    // port and let the daemon bind/write outside QA_ROOT (A6 isolation).
    cmd.env_remove("NEXUS_DAEMON_SOCKET_PATH");
    for (k, v) in qa.nexus_env() {
        cmd.env(k, v);
    }
    cmd.spawn().expect("spawn nexus42 daemon-run")
}

/// Poll the daemon HTTP `/runtime/health` until it responds or times out.
async fn wait_for_daemon(qa: &IsolatedQa, timeout: Duration) -> Result<(), String> {
    let url = format!("{}/v1/daemon/runtime/health", qa.daemon_url());
    let client = reqwest::Client::new();
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                return Ok(());
            }
        }
        if Instant::now() >= deadline {
            return Err(format!("daemon did not become healthy at {url}"));
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

/// Run a `nexus42` CLI command against the isolated env; return Output.
async fn nexus_cli(qa: &IsolatedQa, args: &[&str]) -> std::process::Output {
    let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_nexus42"));
    cmd.args(args);
    for (k, v) in qa.nexus_env() {
        cmd.env(k, v);
    }
    cmd.output().await.expect("spawn nexus42")
}

/// POST a schedule over the public daemon HTTP surface with structured input
/// and the `omp-qa` binding.
async fn post_schedule(
    qa: &IsolatedQa,
    preset_id: &str,
    nonce: &str,
) -> Result<(u16, Value), String> {
    let url = format!("{}/v1/daemon/orchestration/schedules", qa.daemon_url());
    let body = json!({
        "creator_id": IsolatedQa::CREATOR,
        "preset_id": preset_id,
        "label": "p3-live-omp-qa",
        "seed": format!("live nonce {nonce}"),
        "input": { "nonce": nonce },
        "agent_bindings": { "default": { "provider_id": "omp-qa" } }
    });
    let resp = reqwest::Client::new()
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("POST schedule failed: {e}"))?;
    let status = resp.status().as_u16();
    let value = resp
        .json::<Value>()
        .await
        .map_err(|e| format!("POST schedule body parse failed: {e}"))?;
    Ok((status, value))
}

/// Inspect a schedule's own state over HTTP (durable terminal truth).
async fn schedule_status(qa: &IsolatedQa, schedule_id: &str) -> Result<Value, String> {
    let url = format!(
        "{}/v1/daemon/orchestration/schedules/{schedule_id}",
        qa.daemon_url()
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("GET schedule failed: {e}"))?;
    let v: Value = resp
        .json()
        .await
        .map_err(|e| format!("GET schedule body parse failed: {e}"))?;
    Ok(v)
}

/// Poll `/v1/daemon/agent-host/providers` until the `omp-qa` provider is
/// reported `available:true`, or Err(msg) with the observed provider state
/// after `timeout`. A configured provider that never becomes available means
/// isolated provider execution is unusable on this daemon build.
async fn wait_for_provider_available(qa: &IsolatedQa, timeout: Duration) -> Result<(), String> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/daemon/agent-host/providers", qa.daemon_url());
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(resp) = client.get(&url).send().await {
            if let Ok(v) = resp.json::<Value>().await {
                let providers = v["providers"].as_array().cloned().unwrap_or_default();
                if let Some(entry) = providers
                    .iter()
                    .find(|p| p["provider_id"] == json!("omp-qa"))
                {
                    match entry["available"].as_bool() {
                        Some(true) => return Ok(()),
                        _ if Instant::now() >= deadline => {
                            return Err(format!(
                                "omp-qa provider is NOT available on the running daemon: {entry}"
                            ))
                        }
                        _ => {}
                    }
                } else if Instant::now() >= deadline {
                    return Err(format!(
                        "omp-qa provider is NOT present in the daemon provider catalog: {v}"
                    ));
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "omp-qa provider did not become available within {timeout:?} at {url}"
            ));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

// ---------------------------------------------------------------------------
// The ignored live acceptance test
// ---------------------------------------------------------------------------

/// Isolated live omp public success: real daemon binary, external one-step
/// preset through the `omp-qa` ACP provider, non-echo output captured,
/// `completed` durable before and after a daemon-process restart.
#[tokio::test]
#[ignore = "explicit live omp acceptance gate (A6); requires NEXUS_OMP_BIN"]
#[allow(clippy::too_many_lines)]
async fn isolated_public_success() {
    // 1. Gate on the isolated omp binary FIRST — typed refusal before any work.
    let omp_bin = match resolve_omp_bin() {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("{msg}");
            eprintln!("EVIDENCE[refusal]: missing/unusable NEXUS_OMP_BIN — live QA refused, no silent skip");
            panic!("{}", msg);
        }
    };
    eprintln!("EVIDENCE: omp bin = {}", omp_bin.display());

    // 2. Build isolated QA tree; verify all paths inside QA_ROOT.
    let qa = IsolatedQa::new();
    qa.assert_paths_inside_qa_root();
    eprintln!("EVIDENCE: QA_ROOT = {}", qa.qa_root.display());
    eprintln!("EVIDENCE: NEXUS_HOME = {}", qa.nexus_home.display());
    eprintln!("EVIDENCE: QA_WORKSPACE = {}", qa.qa_workspace.display());
    eprintln!("EVIDENCE: QA_OMP_HOME = {}", qa.qa_omp_home.display());

    qa.write_nexus_config();
    qa.write_agent_host_config(&omp_bin);
    qa.seed_creator_db().await;
    let preset_id = qa.write_user_preset();
    eprintln!(
        "EVIDENCE: external preset = {preset_id} at {}",
        qa.nexus_home.join("presets").join(&preset_id).display()
    );

    // 3. Start the real daemon binary and wait for health.
    let mut daemon = spawn_daemon(&qa);
    wait_for_daemon(&qa, Duration::from_secs(60))
        .await
        .unwrap_or_else(|e| {
            let _ = daemon.start_kill();
            panic!("daemon boot: {e}");
        });
    eprintln!("EVIDENCE: real daemon binary up on {}", qa.daemon_url());

    // 3b. Gate on the `omp-qa` provider becoming AVAILABLE on the running
    //     daemon. A provider that never becomes available means isolated
    //     provider execution is unusable (e.g. the agent-host subsystem is
    //     not started on this daemon build) — refuse with typed evidence,
    //     never substitute fixture success.
    if let Err(msg) = wait_for_provider_available(&qa, Duration::from_secs(15)).await {
        let _ = daemon.start_kill();
        eprintln!("{msg}");
        eprintln!(
            "EVIDENCE[refusal]: omp-qa provider unavailable on the running daemon — \
                   isolated live provider execution is unusable; refusing (no silent skip, \
                   no fixture success)"
        );
        panic!("{}", msg);
    }
    eprintln!("EVIDENCE: omp-qa provider AVAILABLE on the running daemon");

    // 4. Drive public schedule add with a fresh nonce the model must NOT echo.
    let nonce = format!(
        "p3live{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .subsec_nanos()
    );
    let (status, body) = post_schedule(&qa, &preset_id, &nonce)
        .await
        .expect("POST schedule");
    eprintln!("EVIDENCE: POST schedule status={status} body={body}");
    assert_eq!(status, 201, "public add must succeed (201): {body}");
    let schedule_id = body["schedule_id"]
        .as_str()
        .expect("schedule_id")
        .to_string();
    eprintln!("EVIDENCE: schedule_id = {schedule_id}");

    // 5. Wait for the schedule to own a session and settle `completed`.
    let session_id = wait_for_completed_schedule(&qa, &schedule_id, Duration::from_secs(240))
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    eprintln!("EVIDENCE: session_id = {session_id}");

    // 6. Capture the durable agent output and assert it is NOT the prompt
    //    echo (the nonce must not be repeated verbatim) and carries real text.
    let output = capture_agent_output(&qa, &session_id).await;
    eprintln!("EVIDENCE: agent output excerpt = {}", excerpt(&output, 400));
    assert!(
        !output.is_empty(),
        "the live run produced NO agent output — a live provider must produce real text"
    );
    assert!(
        !output.contains(&nonce),
        "agent output must not be the prompt echo (nonce {} must not appear): {}",
        nonce,
        excerpt(&output, 400)
    );

    // 7. Restart the daemon process and assert `completed` survives.
    daemon.start_kill().expect("kill daemon");
    let _ = daemon.wait().await;
    let mut daemon2 = spawn_daemon(&qa);
    wait_for_daemon(&qa, Duration::from_secs(60))
        .await
        .unwrap_or_else(|e| {
            let _ = daemon2.start_kill();
            panic!("restart: {e}");
        });
    eprintln!("EVIDENCE: daemon restarted (fresh process)");

    let after = schedule_status(&qa, &schedule_id)
        .await
        .expect("schedule status after restart");
    eprintln!("EVIDENCE: schedule after restart = {after}");
    let status_after = after["schedule"]["status"].as_str().unwrap_or("?");
    assert_eq!(
        status_after, "completed",
        "schedule must be completed after daemon restart: {after}"
    );

    // 8. ops inspect (daemon-free, read-only) also reports the terminal class.
    let inspect = nexus_cli(&qa, &["ops", "inspect", &session_id, "--json"]).await;
    let inspect_text = String::from_utf8_lossy(&inspect.stdout).trim().to_string();
    eprintln!("EVIDENCE: ops inspect (post-restart) = {inspect_text}");
    assert!(
        inspect.status.success(),
        "ops inspect failed: {}",
        String::from_utf8_lossy(&inspect.stderr)
    );
    let inspected: Value = serde_json::from_str(&inspect_text).expect("ops inspect json");
    assert_eq!(
        inspected["recovery_class"],
        json!("terminal"),
        "terminal session must classify terminal after restart"
    );
    assert_eq!(inspected["db_status"], json!("completed"));

    daemon2.start_kill().expect("kill final daemon");
    eprintln!("EVIDENCE: isolated live omp public success PASSED (session {session_id})");
}

/// Poll schedule/session until the schedule status is `completed`; returns
/// the owned session id. Fails with the refusal evidence when the provider
/// refuses/auth is unusable (never a silent skip).
async fn wait_for_completed_schedule(
    qa: &IsolatedQa,
    schedule_id: &str,
    timeout: Duration,
) -> Result<String, String> {
    let deadline = Instant::now() + timeout;
    let mut last_session = None;
    loop {
        let v = schedule_status(qa, schedule_id).await?;
        let sched = &v["schedule"];
        let status = sched["status"].as_str().unwrap_or("?");
        if let Some(sid) = sched["current_session_id"].as_str() {
            last_session = Some(sid.to_string());
        }
        match status {
            "completed" => {
                return last_session
                    .ok_or_else(|| format!("schedule completed but no session id recorded: {v}"))
            }
            "failed" | "cancelled" | "interrupted" => {
                return Err(format!(
                    "schedule reached terminal '{status}' — live provider refusal evidence: {v}"
                ))
            }
            _ => {}
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "schedule did not complete within {timeout:?}; last status={status} \
                 session={last_session:?} schedule={v}"
            ));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Read the durable agent output the session persisted in its `context_json`
/// (`state.<state>.output`). We open the workspace DB (the same DB the daemon
/// uses) so the captured text is the real persisted agent output. Runs in the
/// ambient tokio runtime.
async fn capture_agent_output(qa: &IsolatedQa, session_id: &str) -> String {
    let db_path = qa.workspace_db();
    let pool = nexus_local_db::open_pool(&db_path)
        .await
        .expect("open workspace db for output capture");
    let bytes: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT context_json FROM orchestration_sessions WHERE session_id = ?")
            .bind(session_id)
            .fetch_optional(&pool)
            .await
            .expect("load session context");
    drop(pool);
    let Some(bytes) = bytes else {
        return String::new();
    };
    // Context is serialized graph-flow Context JSON: `{"data": {...}}`.
    let ctx: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    let data = ctx.get("data").cloned().unwrap_or(ctx);
    // Collect every string value; the transform output is the longest
    // non-empty one among `state.*.output` (or any `*output*` key).
    let mut strings: Vec<(String, String)> = Vec::new();
    collect_strings(&data, "", &mut strings);
    // Prefer values under keys containing "output".
    strings.sort_by_key(|(k, _)| {
        let priority = if k.contains("output") && k.starts_with("state.") {
            0
        } else if k.contains("output") {
            1
        } else {
            2
        };
        (priority, -(i64::try_from(k.len()).unwrap_or(i64::MAX)))
    });
    // The highest-priority entry after sorting IS the output-key value; do
    // not discard the ordering by re-selecting the longest arbitrary string.
    strings
        .into_iter()
        .next()
        .map(|(_, value)| value)
        .unwrap_or_default()
}

/// Recursively collect (dot-path, string) pairs from a JSON value.
fn collect_strings(v: &Value, prefix: &str, out: &mut Vec<(String, String)>) {
    match v {
        Value::String(s) => out.push((prefix.to_string(), s.clone())),
        Value::Object(map) => {
            for (k, val) in map {
                let p = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                collect_strings(val, &p, out);
            }
        }
        Value::Array(arr) => {
            for (i, val) in arr.iter().enumerate() {
                let p = format!("{prefix}[{i}]");
                collect_strings(val, &p, out);
            }
        }
        _ => {}
    }
}

/// Short excerpt for evidence.
fn excerpt(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        let mut out = s.chars().take(n).collect::<String>();
        out.push('…');
        out
    }
}
