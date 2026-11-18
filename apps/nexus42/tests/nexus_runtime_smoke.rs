//! P2 T1 — spawned-process smoke test for the headless `nexus-runtime` bin.
//!
//! Builds the REAL binary (`env!("CARGO_BIN_EXE_nexus-runtime")`) and runs
//! it against a hermetic temp `NEXUS42_HOME`, proving the headless boot
//! contract (P2 spec § Subsystem profile / AC-2):
//!
//! 1. **stdout readiness** — the process prints the readiness block
//!    (`peer_id` / `host_id` / `listen` / served invokes) on stdout;
//! 2. **serves Connect** — the `runtime_smoke_probe` example (a reference
//!    spoke-connect peer, built with the same `connect-host` feature)
//!    completes the signed-hello handshake against the spawned process and
//!    reads the advertised manifest (`extensions.nexus.served_ops` =
//!    upsert/promote/relate/check/assemble/compute — the invoke surface,
//!    honest by the machine-check);
//! 3. **no HTTP/SPA listener** — every TCP listener of the runtime process is
//!    either a printed Connect multiaddr or the Connect peer-tools WS lane's
//!    own loopback endpoint, and that lane endpoint answers no HTTP request
//!    (the daemon router and the embedded SPA are not even in this cohort's
//!    graph — v1.193 P2-T13 deleted the daemon-runtime crate).
//!
//! Compiled only with `--features connect-host` (same gate as the bin);
//! the test itself only spawns processes, so the default test graph stays
//! libp2p-free — the dialing lives in the probe example.

#![cfg(feature = "connect-host")]

use std::io::{BufRead, BufReader};
#[cfg(unix)]
use std::io::{Read, Write};
#[cfg(unix)]
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Peer id of the probe's fixed Ed25519 seed (7) — deterministic, derived
/// once via `runtime_smoke_probe --print-peer-only` (libp2p peer-id
/// derivation is stable). Allowlisted in the seeded `allowlist.json`
/// BEFORE the host boots (fail-closed allowlist: the handshake only
/// succeeds for listed peers).
const DIALER_PEER_ID: &str = "12D3KooWRawPbxPtP1eZaJpumGnyWX2DcUyd3RQnydr3eAto4Az7";

/// Canonical nexus home dir name (the layout join used by every home
/// helper; kept literal so the test does not need the layout crate).
const NEXUS_DIR: &str = ".nexus42";

/// How long the spawned runtime may take to print readiness.
const READY_TIMEOUT: Duration = Duration::from_secs(30);

/// The retained fixture the opt-in probe half drives: one stored scoped
/// World, one staged compute session and the deterministic `basic-combat`
/// module installed host-locally (module install only where the wasm32 target
/// exists). The allowlist row below scopes the probe peer to this
/// World/module and grants the stored Creator, with an op scope of EXACTLY
/// `upsert` + `compute` — so the probe's `check` invoke still produces the
/// locked `op_unsupported` refusal while the follow-up `upsert` proves the
/// session stays usable.
const COMPUTE_CREATOR: &str = "ctr_smoke";
const COMPUTE_WORLD: &str = "wld_smoke";
const COMPUTE_MODULE: &str = "basic-combat";
const COMPUTE_SESSION: &str = "ses_smoke";

/// Seed a hermetic `~/.nexus42` home for the spawned runtime: the CLI
/// config keys the boot resolves the active workspace from (same keys the
/// creator app writes), plus the scoped Connect allowlist holding the
/// fixed-seed probe peer.
fn seed_home(tmp: &Path) {
    let nexus = tmp.join(NEXUS_DIR);
    std::fs::create_dir_all(&nexus).expect("create nexus home");

    std::fs::write(
        nexus.join("config.toml"),
        format!(
            "active_creator_id = \"{COMPUTE_CREATOR}\"\n\
         [active_workspace_slug_by_creator]\n\
         {COMPUTE_CREATOR} = \"default\"\n"
        ),
    )
    .expect("write config.toml");

    let connect_dir = nexus.join("connect");
    std::fs::create_dir_all(&connect_dir).expect("create connect dir");
    std::fs::write(
        connect_dir.join("allowlist.json"),
        format!(
            r#"{{"peer_ids": [{{
                "peer_id": "{DIALER_PEER_ID}",
                "world_scope": ["{COMPUTE_WORLD}"],
                "op_scope": ["upsert", "compute"],
                "module_scope": ["{COMPUTE_MODULE}"],
                "grant": {{
                    "creator_id": "{COMPUTE_CREATOR}",
                    "actor": {{ "kind": "creator", "id": "{COMPUTE_CREATOR}" }},
                    "world_id": "{COMPUTE_WORLD}"
                }}
            }}]}}"#
        ),
    )
    .expect("write allowlist.json");

    // v1.210 P3: pin the peer-tools event lane to an OS-assigned ephemeral
    // port. The lane then always binds (no machine-wide 8425 collision
    // dependence), and the printed `event_lane:` readiness line must name
    // the REAL bound port — never the configured `0` (the config echo the
    // locked contract forbids).
    std::fs::write(connect_dir.join("daemon.json"), "{\"port\": 0}").expect("write daemon.json");
}

/// Install the embedded deterministic module into the hermetic home's
/// host-local store (`~/.nexus42/modules/<id>/`) — the operator-install step
/// the compute route requires (bytes are never peer-supplied).
#[cfg(not(nexus42_no_wasm_target))]
fn install_compute_module(home: &Path) {
    let dir = nexus_home_layout::user_modules_dir(home).join(COMPUTE_MODULE);
    std::fs::create_dir_all(&dir).expect("mkdir module store dir");
    let bytes = nexus_wasm_host::embedded_module_bytes(COMPUTE_MODULE)
        .unwrap_or_else(|| panic!("embedded module {COMPUTE_MODULE:?} must ship bytes"));
    let manifest = nexus_wasm_host::embedded_module_manifest(COMPUTE_MODULE)
        .unwrap_or_else(|| panic!("embedded module {COMPUTE_MODULE:?} must ship a manifest"));
    std::fs::write(dir.join(format!("{COMPUTE_MODULE}.wasm")), bytes).expect("write module wasm");
    std::fs::write(dir.join("manifest.json"), manifest).expect("write module manifest");
}

/// Seed the workspace the spawned runtime opens with the stored authority the
/// retained half reads: a Creator + holder row, the scoped World, and the
/// staged compute session. The module is installed only when the wasm32 target
/// exists; the World/session rows are safe (and unused) on a no-wasm host and
/// are what make the session-alive `upsert` possible there. Staged through an
/// engine-owned pool (the `compute_sessions` table is engine-writer-fenced)
/// which is closed and its OS admission released before the runtime starts —
/// the runtime keeps its own direct (cooperative) pool.
fn seed_scoped_workspace(home: &Path) {
    #[cfg(not(nexus42_no_wasm_target))]
    install_compute_module(home);
    let db_path = nexus42::paths::state_db_path(home, COMPUTE_CREATOR, "default");
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir workspace state dir");
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("compute staging runtime");

    runtime.block_on(async {
        let guarded = nexus_local_db::init_engine_pool(&db_path)
            .await
            .expect("engine-owned workspace DB initializes");
        let pool = guarded.clone_pool();
        nexus_local_db::creators::ensure_creator_row(&pool, COMPUTE_CREATOR, "smoke creator")
            .await
            .expect("seed the acting Creator and its holder");
        nexus_local_db::kb_store::seed::world(
            &pool,
            COMPUTE_WORLD,
            COMPUTE_CREATOR,
            "Smoke World",
            "smoke",
            "private",
            "manual",
        )
        .await;
        let state = serde_json::json!({
            "module_id": COMPUTE_MODULE,
            "attacker_id": "kb_atk",
            "defender_id": "kb_def",
            "character": { "current_hp": 30, "max_hp": 30 },
        });
        nexus_local_db::compute_session::insert_compute_session(
            &pool,
            COMPUTE_SESSION,
            "kb_atk",
            &state.to_string(),
        )
        .await
        .expect("stage the compute session");
        pool.close().await;
        drop(guarded);
        nexus_local_db::writer_protocol::release_retained_writer_guards(&db_path);
    });
}

/// The `runtime_smoke_probe` example binary — compiled by cargo (with
/// `connect-host` on) when the test target builds; resolved through the
/// standard workspace target layout.
fn probe_binary() -> PathBuf {
    // Same profile inference as the shipped host artifact: a custom profile's
    // probe lives under its own directory, never under `debug`.
    let (target, profile) = running_profile().unwrap_or_else(|error| panic!("{error}"));
    let exe = if cfg!(windows) {
        "runtime_smoke_probe.exe"
    } else {
        "runtime_smoke_probe"
    };
    target.join(profile).join("examples").join(exe)
}

/// Infer `<target>/<profile>` from the running test binary's path
/// (`<target>/<profile>/deps/<binary>`).
///
/// A compile-time `debug_assertions` switch is NOT enough: a custom profile
/// (e.g. `--profile ci`) that inherits `dev` keeps debug assertions on while
/// building into `target/ci/`, so the boolean would resolve another profile's
/// — possibly stale — artifact. The profile directory name is the truth.
fn profile_from_exe(exe: &Path) -> Result<(PathBuf, String), String> {
    let shown = exe.display();
    let deps = exe.parent().ok_or_else(|| {
        format!("cannot infer the cargo profile from {shown}: no parent directory")
    })?;
    if deps.file_name().and_then(|name| name.to_str()) != Some("deps") {
        return Err(format!(
            "cannot infer the cargo profile from {shown}: expected the cargo test layout \
             <target>/<profile>/deps/<binary>; set NEXUS42_RUNTIME_BIN to name the shipped artifact"
        ));
    }
    let profile_dir = deps.parent().ok_or_else(|| {
        format!("cannot infer the cargo profile from {shown}: the deps directory has no parent")
    })?;
    let profile = profile_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            format!(
                "cannot infer the cargo profile from {shown}: the profile directory has no name"
            )
        })?;
    let target_root = profile_dir.parent().ok_or_else(|| {
        format!(
            "cannot infer the cargo target directory from {shown}: the profile directory has no parent"
        )
    })?;
    Ok((target_root.to_path_buf(), profile.to_string()))
}

/// The `<target>/<profile>` of the binary that is actually running.
fn running_profile() -> Result<(PathBuf, String), String> {
    let exe = std::env::current_exe()
        .map_err(|error| format!("cannot read the running test binary path: {error}"))?;
    profile_from_exe(&exe)
}

/// The cargo arguments that build into `profile`'s directory.
fn profile_build_args(profile: &str) -> Vec<String> {
    match profile {
        "debug" => Vec::new(),
        "release" => vec!["--release".to_string()],
        other => vec!["--profile".to_string(), other.to_string()],
    }
}

/// The SHIPPED Connect-only runtime artifact for `profile`, or a named error.
///
/// Never falls back to another profile's binary: a release test that silently
/// spawned a stale debug artifact would prove behaviour of code this run never
/// compiled.
fn profile_artifact(target_dir: &Path, profile: &str) -> Result<PathBuf, String> {
    let artifact = target_dir
        .join(profile)
        .join(format!("nexus-runtime{}", std::env::consts::EXE_SUFFIX));
    if artifact.is_file() {
        Ok(artifact)
    } else {
        Err(format!(
            "shipped Connect-only runtime artifact missing for the {profile} profile at {}; \
             the smoke was built as {profile} and refuses to fall back to another profile's binary",
            artifact.display()
        ))
    }
}

/// Build (once per run) and return the SHIPPED Connect-only runtime binary.
///
/// `CARGO_BIN_EXE_nexus-runtime` is not that artifact inside `cargo test`:
/// resolver v2 unifies this package's dev-dependency features
/// (`nexus-core[execution,test-hooks]`) into the test build, so the spawned
/// binary could link a wider graph than the product ships. This runs a
/// bin-only `--no-default-features --features connect-host` build in its own
/// cargo invocation — with `--release` when the test itself is release — and
/// returns that exact profile's artifact (`profile_artifact` fails closed
/// otherwise). `NEXUS42_RUNTIME_BIN` overrides it (e.g. a CI-packaged
/// artifact).
fn shipped_runtime_binary() -> PathBuf {
    if let Some(path) = std::env::var_os("NEXUS42_RUNTIME_BIN") {
        let path = PathBuf::from(path);
        assert!(
            path.is_file(),
            "NEXUS42_RUNTIME_BIN does not name a file: {}",
            path.display()
        );
        return path;
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let (target, profile) = running_profile().unwrap_or_else(|error| panic!("{error}"));
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut build = Command::new(cargo);
    build
        .args([
            "build",
            "--locked",
            "-p",
            "nexus42",
            "--bin",
            "nexus-runtime",
            "--no-default-features",
            "--features",
            "connect-host",
        ])
        .args(profile_build_args(&profile))
        // Pin the target root the inference resolved, so a relative
        // `CARGO_TARGET_DIR` cannot land the artifact elsewhere.
        .env("CARGO_TARGET_DIR", &target)
        .current_dir(&manifest_dir);
    let build = build
        .output()
        .expect("run cargo build for the shipped Connect-only runtime");
    assert!(
        build.status.success(),
        "shipped Connect-only runtime build failed ({})\nstdout:\n{}\nstderr:\n{}",
        build.status,
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    profile_artifact(&target, &profile).unwrap_or_else(|error| panic!("{error}"))
}

/// The fail-closed profile controls: the required profile's artifact is used,
/// and another profile's existing binary is never substituted.
#[test]
fn profile_artifact_refuses_to_fall_back_to_another_profile() {
    let dir = tempfile::tempdir().expect("temp dir");
    let exe = format!("nexus-runtime{}", std::env::consts::EXE_SUFFIX);
    std::fs::create_dir_all(dir.path().join("debug")).expect("mkdir debug");
    std::fs::write(dir.path().join("debug").join(&exe), b"debug").expect("write debug");

    let error = profile_artifact(dir.path(), "release")
        .expect_err("a release run must not resolve the debug artifact");
    assert!(
        error.contains("missing for the release profile") && error.contains("refuses to fall back"),
        "{error}"
    );
    assert_eq!(
        profile_artifact(dir.path(), "debug").unwrap(),
        dir.path().join("debug").join(&exe)
    );

    let custom_error = profile_artifact(dir.path(), "ci")
        .expect_err("a custom profile must not resolve the debug artifact");
    assert!(
        custom_error.contains("missing for the ci profile")
            && custom_error.contains("refuses to fall back"),
        "{custom_error}"
    );

    std::fs::create_dir_all(dir.path().join("release")).expect("mkdir release");
    std::fs::write(dir.path().join("release").join(&exe), b"release").expect("write release");
    assert_eq!(
        profile_artifact(dir.path(), "release").unwrap(),
        dir.path().join("release").join(&exe)
    );
}

/// The inference the smoke uses: the profile directory name comes from the
/// running binary's path, custom profiles included, and a layout that does not
/// name it fails closed instead of guessing a profile.
#[test]
fn profile_from_exe_infers_the_profile_directory_and_rejects_other_layouts() {
    for (exe, target, profile) in [
        (
            "/w/target/debug/deps/nexus_runtime_smoke-abc",
            "/w/target",
            "debug",
        ),
        (
            "/w/target/release/deps/nexus_runtime_smoke-abc",
            "/w/target",
            "release",
        ),
        (
            "/w/target/ci/deps/nexus_runtime_smoke-abc",
            "/w/target",
            "ci",
        ),
    ] {
        assert_eq!(
            profile_from_exe(Path::new(exe)),
            Ok((PathBuf::from(target), profile.to_string()))
        );
    }
    for layout in [
        "/w/target/debug/nexus_runtime_smoke",
        "/w/nexus_runtime_smoke",
    ] {
        let error =
            profile_from_exe(Path::new(layout)).expect_err("an ambiguous layout must fail closed");
        assert!(error.contains("expected the cargo test layout"), "{error}");
    }
    assert_eq!(profile_build_args("debug"), Vec::<String>::new());
    assert_eq!(profile_build_args("release"), vec!["--release".to_string()]);
    assert_eq!(
        profile_build_args("ci"),
        vec!["--profile".to_string(), "ci".to_string()]
    );
}

/// Parse the probe's `COMPUTE_OK session=<id> entry=kb_atk current_hp=<n>`
/// line and return the reported HP. Requires exactly ONE matching line and a
/// complete integer token: a neighbouring value (`150`, `1500`) therefore
/// never compares equal to `15`.
fn compute_current_hp(stdout: &str, session: &str) -> Result<i64, String> {
    let prefix = format!("COMPUTE_OK session={session} entry=kb_atk current_hp=");
    let mut values = stdout
        .lines()
        .filter_map(|line| line.trim().strip_prefix(&prefix));
    let raw = values
        .next()
        .ok_or_else(|| format!("missing one {prefix}<n> line"))?;
    if values.next().is_some() {
        return Err(format!("duplicate {prefix}<n> lines"));
    }
    raw.parse::<i64>()
        .map_err(|e| format!("current_hp is not a complete integer ({raw:?}): {e}"))
}

/// The retained-compute assertion the smoke test makes: exactly one result
/// line for `session`, and the value must be the deterministic `15`.
///
/// Shared with the neighbouring-value control below, so a regression of the
/// smoke assertion to a substring/prefix match fails the control too.
fn assert_compute_result(stdout: &str, session: &str) -> Result<(), String> {
    match compute_current_hp(stdout, session)? {
        15 => Ok(()),
        other => Err(format!(
            "deterministic compute result must be exactly 15, got {other}"
        )),
    }
}

/// The retained-compute assertion's negative control: a neighbouring HP value
/// must never satisfy the exact comparison the smoke test makes.
#[test]
fn compute_result_control_rejects_a_neighbouring_value() {
    let line = |hp: &str| format!("COMPUTE_OK session=ses_smoke entry=kb_atk current_hp={hp}");

    assert_eq!(assert_compute_result(&line("15"), "ses_smoke"), Ok(()));
    for neighbour in ["150", "1500", "1500 ", "15x", "-15"] {
        assert!(
            assert_compute_result(&line(neighbour), "ses_smoke").is_err(),
            "{neighbour:?} must fail the exact current_hp=15 assertion"
        );
    }
    assert!(
        assert_compute_result(&line("15"), "ses_other").is_err(),
        "another session id must not match"
    );
    assert!(
        assert_compute_result(&format!("{}\n{}", line("15"), line("15")), "ses_smoke").is_err(),
        "duplicate compute lines must be rejected"
    );
}

/// One `<key>=<value>` line printed by the probe, requiring exactly one.
fn probe_value<'a>(stdout: &'a str, key: &str) -> Result<&'a str, String> {
    let prefix = format!("{key}=");
    let mut values = stdout
        .lines()
        .filter_map(|line| line.trim().strip_prefix(&prefix));
    let value = values
        .next()
        .ok_or_else(|| format!("missing {key}= line"))?;
    if values.next().is_some() {
        return Err(format!("duplicate {key}= lines"));
    }
    Ok(value)
}

/// The refusal the smoke's authorization claim requires: the op is SERVED but
/// excluded by this peer's `op_scope`. The product keeps one `op_unsupported`
/// code for every denial (no cross-peer scope leak), so the wire reason is the
/// observable discriminator — request validation or an unserved op must NOT
/// satisfy this assertion.
fn assert_out_of_scope_refusal(code: &str, message: &str) -> Result<(), String> {
    if code != "op_unsupported" {
        return Err(format!(
            "refusal code {code:?} is not the op_unsupported family"
        ));
    }
    if message != "op denied: op check is not in this peer's op_scope" {
        return Err(format!(
            "refusal message {message:?} is not the op-scope denial"
        ));
    }
    Ok(())
}

/// The refusal-attribution control: an unserved-op or payload denial must not
/// pass as the authorization-boundary refusal the smoke claims.
#[test]
fn refusal_control_rejects_an_unserved_or_payload_denial() {
    assert_eq!(
        assert_out_of_scope_refusal(
            "op_unsupported",
            "op denied: op check is not in this peer's op_scope"
        ),
        Ok(())
    );
    for (code, message) in [
        (
            "op_unsupported",
            "op project is not supported: this host serves the core connect ops",
        ),
        (
            "op_unsupported",
            "op denied: invoke payload carries no verifiable world scope (writes need \
             extensions.nexus.world_id on every entry/relation; check/assemble need \
             scope.scope_id); cannot verify world scope",
        ),
        (
            "invalid_input",
            "op denied: op check is not in this peer's op_scope",
        ),
    ] {
        assert!(
            assert_out_of_scope_refusal(code, message).is_err(),
            "{message:?} must not satisfy the out-of-scope refusal assertion"
        );
    }
}

/// A no-wasm build cannot run the retained-compute half. Keep that visible as
/// an IGNORED, named test instead of letting the half silently disappear from
/// a green run; CI additionally sets `NEXUS42_REQUIRE_WASM_TARGET=1`, which
/// turns the same condition into a build failure.
#[cfg(nexus42_no_wasm_target)]
#[test]
#[ignore = "retained compute half not executed: wasm32-unknown-unknown target missing"]
fn skipped_retained_compute_half_requires_wasm32_unknown_unknown() {}

/// Spawn the real `nexus-runtime` binary against `home` (via the
/// `NEXUS42_HOME` override) and wait for the stdout readiness block.
/// Returns the child (still running), the readiness lines, and the printed
/// `listen:` multiaddrs.
fn spawn_runtime(home: &Path) -> (Child, Vec<String>, Vec<String>) {
    let mut child = Command::new(shipped_runtime_binary())
        .env("NEXUS42_HOME", home)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn nexus-runtime");

    // Drain both pipes through channels so the child can never block on a
    // full pipe; stderr lines are kept for failure diagnostics.
    let stdout = child.stdout.take().expect("runtime stdout");
    let stderr = child.stderr.take().expect("runtime stderr");
    let (line_tx, line_rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line_tx.send(line).is_err() {
                break;
            }
        }
    });
    let (stderr_tx, stderr_rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if stderr_tx.send(line).is_err() {
                break;
            }
        }
    });

    let deadline = Instant::now() + READY_TIMEOUT;
    let mut ready_lines = Vec::new();
    let mut listen_addrs = Vec::new();
    while Instant::now() < deadline {
        match line_rx.recv_timeout(Duration::from_millis(250)) {
            Ok(line) => {
                ready_lines.push(line.clone());
                if let Some(rest) = line.trim().strip_prefix("listen:") {
                    listen_addrs.push(rest.trim().to_string());
                }
                // The readiness block ends with the Ctrl-C hint line.
                if !listen_addrs.is_empty() && line.contains("press Ctrl-C to stop") {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    if listen_addrs.is_empty() {
        let _ = child.kill();
        let _ = child.wait();
        panic!(
            "nexus-runtime did not print a `listen:` readiness line.\n\
             stdout so far:\n{}\nstderr:\n{}",
            ready_lines.join("\n"),
            stderr_rx.try_iter().collect::<Vec<_>>().join("\n")
        );
    }

    // Keep the stderr drain alive for the caller's lifetime (detached).
    std::mem::forget(stderr_rx);
    (child, ready_lines, listen_addrs)
}

/// RAII guard that kills and reaps the spawned runtime child on drop —
/// including panic unwind — so a failed assertion never orphans the
/// process (the pre-fix test only killed on the success path).
struct RuntimeGuard {
    child: Option<Child>,
}

impl RuntimeGuard {
    const fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }
}

impl Drop for RuntimeGuard {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Assert the spawned runtime has NO HTTP / SPA listener.
///
/// Two independent checks, because a port number alone cannot tell a WS accept
/// loop from an HTTP router:
///
/// 1. **Endpoint allowance** — every TCP listener of the runtime process must
///    be one of the Connect listen multiaddrs it printed, or the Connect
///    **peer-tools event lane**'s own endpoint as disclosed by the v1.210 P3
///    `event_lane:` readiness line ([`event_lane_endpoint`]). The ENDPOINT
///    (host AND port) is compared, so a listener on an allowed port but a
///    different interface (`[::1]:8425`, `*:8425`) is not covered by the
///    allowance, and any other port — the daemon API default 8420 included —
///    still fails.
/// 2. **Protocol identity** — the disclosed lane endpoint must be one THIS
///    process actually holds, asserted positively (the port-0 fixture makes
///    the lane always bind, so a readiness line naming a config echo, the
///    configured `0`, or a foreign endpoint fails instead of skipping the
///    probe). That held endpoint must then not answer an HTTP/1.1 request
///    ([`answers_plain_http`]): the lane is a plaintext-WS accept loop, not
///    the daemon data router or the embedded SPA. This is what a port number
///    cannot express — an HTTP router sitting on the allowed endpoint would
///    answer the probe and fail the test. The probe is scoped to THIS PID:
///    only endpoints `lsof -p <pid>` attributes to this runtime are
///    considered, so a foreign process occupying `127.0.0.1:8425` is never
///    attributed to the runtime (spec coexistence).
///
/// (A well-known-port probe would false-fail under the spec's coexistence
/// model — a creator-facing `nexus42` daemon may legitimately occupy the
/// daemon port while the runtime runs; the guarantee is about THIS process,
/// which never boots the daemon router.)
///
/// Implemented with `lsof` on unix (present on macOS + Linux CI runners);
/// on Windows the check is a no-op — the property is structural (the
/// headless boot binds only `SpokeConnectNode` + the peer-tools WS lane;
/// there is no axum bind in the path), and the T2 Windows CI leg smoke-tests
/// `--version`.
fn assert_no_http_listener(child_pid: u32, listen_addrs: &[String], lane_endpoint: &Endpoint) {
    #[cfg(unix)]
    {
        let mut expected: Vec<Endpoint> = listen_addrs
            .iter()
            .filter_map(|addr| multiaddr_endpoint(addr))
            .collect();
        // The peer-tools event lane's WS endpoint (see the doc comment): the
        // seeded home pins `daemon.json` to port 0, so the lane always binds
        // an OS-assigned port and the readiness line discloses the real one.
        expected.push((lane_endpoint.0.clone(), lane_endpoint.1));

        let out = Command::new("lsof")
            .args([
                "-nP",
                "-iTCP",
                "-sTCP:LISTEN",
                "-a",
                "-p",
                &child_pid.to_string(),
            ])
            .output()
            .expect("run lsof");
        assert!(
            out.status.success(),
            "lsof failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8_lossy(&out.stdout);
        // THIS PID's own listeners, each already checked against the allowance.
        let mut listeners: Vec<Endpoint> = Vec::new();
        for line in text.lines().skip(1) {
            let Some((host, port)) = lsof_line_endpoint(line) else {
                continue;
            };
            assert!(
                expected.iter().any(|(h, p)| h == &host && *p == port),
                "runtime process holds an unexpected TCP listener on {host}:{port} \
                 (neither a printed Connect multiaddr nor the peer-tools WS lane \
                 endpoint {lane_host}:{lane_port} — a daemon HTTP / SPA listener?):\n{text}",
                lane_host = lane_endpoint.0,
                lane_port = lane_endpoint.1,
            );
            listeners.push((host, port));
        }

        // Protocol identity (check 2), attributed to THIS PID only: the
        // disclosed endpoint must be one the runtime actually holds. The
        // port-0 fixture guarantees the lane bound, so a readiness line that
        // named a config echo, the configured `0`, or a foreign endpoint
        // fails here instead of silently skipping the probe.
        assert!(
            listeners
                .iter()
                .any(|(h, p)| *h == lane_endpoint.0 && *p == lane_endpoint.1),
            "the runtime does not hold the event-lane endpoint {lane_host}:{lane_port} it \
             disclosed in its readiness line; listeners held: {listeners:?}",
            lane_host = lane_endpoint.0,
            lane_port = lane_endpoint.1,
        );
        let addr =
            lane_socket_addr(lane_endpoint).expect("the disclosed lane host is an IP literal");
        assert!(
            !answers_plain_http(addr, HTTP_PROBE_TIMEOUT),
            "the runtime's Connect lane endpoint {addr} answered an HTTP/1.1 request: an \
             HTTP / SPA listener would, while the peer-tools lane is a WS accept loop and \
             must close a non-upgrade request with no response"
        );
    }
    #[cfg(not(unix))]
    {
        // Structural no-op — see the doc comment.
        let _ = (child_pid, listen_addrs, lane_endpoint);
    }
}

/// One TCP endpoint: a listener's `(host, port)`, or a printed Connect
/// multiaddr's.
type Endpoint = (String, u16);

/// How long the HTTP protocol probe waits for a response.
#[cfg(unix)]
const HTTP_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// The most response bytes the protocol probe reads before giving up.
#[cfg(unix)]
const HTTP_PROBE_CAP: usize = 4096;

/// The Connect **peer-tools event lane**'s listen endpoint.
///
/// `nexus-runtime::boot` starts the lane with
/// `nexus_connect::start_peer_tools_lane`, whose plaintext-WS accept loop
/// binds the lane's configured `host:port`: the loopback
/// `DEFAULT_CONNECT_HOST:DEFAULT_CONNECT_PORT`
/// (`crates/nexus-connect/src/config.rs`) when no `connect/daemon.json`
/// overrides it — which is the hermetic seeded home. The bind is loopback-only
/// by construction: having no TLS, the lane refuses a non-loopback host.
fn peer_tools_lane_endpoint() -> Endpoint {
    (
        nexus_connect::config::DEFAULT_CONNECT_HOST.to_string(),
        nexus_connect::DEFAULT_CONNECT_PORT,
    )
}

/// `(host, port)` of a printed Connect listen multiaddr
/// (`/ip4/127.0.0.1/tcp/53843`, or the `/ip6/` form).
fn multiaddr_endpoint(addr: &str) -> Option<Endpoint> {
    let rest = addr
        .split_once("/ip4/")
        .or_else(|| addr.split_once("/ip6/"))?
        .1;
    let (host, rest) = rest.split_once("/tcp/")?;
    let port = rest.split('/').next()?.parse::<u16>().ok()?;
    Some((host.to_string(), port))
}

/// Parse the v1.210 P3 `event_lane: ws://<host>:<port>/connect` readiness
/// line into a `(host, port)` endpoint. `None` for any other shape.
///
/// An IPv6 host is unbracketed (`[::1]` → `::1`) so the endpoint matches
/// `lsof`'s host form (`lsof` never prints the brackets) and every consumer
/// can parse the host as a bare `IpAddr` — the bracketed literal would panic
/// an `IpAddr` parse.
fn event_lane_endpoint(line: &str) -> Option<Endpoint> {
    let url = line.trim().strip_prefix("event_lane: ")?;
    let rest = url.strip_prefix("ws://")?;
    let (host, rest) = rest.rsplit_once(':')?;
    let port = rest.strip_suffix("/connect")?.parse::<u16>().ok()?;
    let host = host
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(host);
    Some((host.to_string(), port))
}

/// `SocketAddr` of a disclosed lane endpoint.
///
/// The host is a bare IP literal (IPv4, or the unbracketed IPv6
/// [`event_lane_endpoint`] yields), so the parse returns `None` instead of
/// panicking on any unexpected host form (S-2).
#[cfg(unix)]
fn lane_socket_addr(endpoint: &Endpoint) -> Option<SocketAddr> {
    endpoint
        .0
        .parse::<std::net::IpAddr>()
        .ok()
        .map(|ip| SocketAddr::new(ip, endpoint.1))
}

/// Send one minimal HTTP/1.1 request to `addr` and report whether the peer
/// answers with a syntactically valid HTTP response.
///
/// This is the protocol-identity half a port number cannot make. An HTTP
/// router / SPA answers a status line; the Connect peer-tools WS accept loop
/// rejects a non-upgrade request by closing the connection with NO response
/// (observed against the booted lane: 0 bytes), so "answered HTTP" separates
/// the two. An endpoint with nothing listening, or one that closes (or times
/// out) without answering, is not an HTTP listener.
#[cfg(unix)]
fn answers_plain_http(addr: SocketAddr, timeout: Duration) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, timeout) else {
        // Nothing is listening there: whatever holds the port is not answering.
        return false;
    };
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    if stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut response = Vec::new();
    let mut chunk = [0u8; 256];
    while response.len() < HTTP_PROBE_CAP {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                response.extend_from_slice(&chunk[..read]);
                if response.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
        }
    }
    let text = String::from_utf8_lossy(&response);
    let status = text.split("\r\n").next().unwrap_or_default();
    status.starts_with("HTTP/1.1 ") || status.starts_with("HTTP/1.0 ")
}

/// `(host, port)` of a `lsof -sTCP:LISTEN` NAME column line.
///
/// Real lines end with the state suffix — `TCP 127.0.0.1:62488 (LISTEN)` —
/// so the address token is the SECOND-TO-LAST whitespace token
/// (`split_whitespace().rev().nth(1)` — `str` has no `rsplit_whitespace`),
/// NOT `.last()` (which is always `(LISTEN)` and never parses as an address —
/// the pre-fix parser skipped every line, making the no-HTTP assertion
/// vacuous). The HOST is kept, not just the port: the lane allowance is one
/// loopback ENDPOINT, so `[::1]:8425` / `*:8425` must not pass as
/// `127.0.0.1:8425`.
fn lsof_line_endpoint(line: &str) -> Option<Endpoint> {
    let addr = line.split_whitespace().rev().nth(1)?;
    let (host, port) = addr.rsplit_once(':')?;
    let port = port.parse::<u16>().ok()?;
    Some((
        host.trim_start_matches('[')
            .trim_end_matches(']')
            .to_string(),
        port,
    ))
}

#[test]
fn lsof_line_endpoint_and_multiaddr_parse_real_listener_lines() {
    // Real shapes from `lsof -nP -iTCP -sTCP:LISTEN -a -p <pid>`: IPv4
    // loopback (observed: `TCP 127.0.0.1:62488 (LISTEN)`), wildcard, and
    // bracketed IPv6 — the host travels with the port so the endpoint
    // allowance cannot be satisfied by a different interface.
    assert_eq!(
        lsof_line_endpoint(
            "nexus-runtime 5469 user 14u IPv4 0x8f7d2f5b5e0b4c8f 0t0 TCP 127.0.0.1:62488 (LISTEN)"
        ),
        Some(("127.0.0.1".to_string(), 62488))
    );
    assert_eq!(
        lsof_line_endpoint("TCP *:62086 (LISTEN)"),
        Some(("*".to_string(), 62086))
    );
    assert_eq!(
        lsof_line_endpoint("TCP [::1]:62086 (LISTEN)"),
        Some(("::1".to_string(), 62086))
    );

    // Regression guard: the vacuous pre-fix parser (`.last()` token, i.e.
    // always `(LISTEN)`) extracts no port from ANY real listener line, so
    // it could never fail the no-HTTP assertion. If this guard trips, the
    // assertion became vacuous again.
    for line in [
        "TCP 127.0.0.1:62488 (LISTEN)",
        "TCP *:62086 (LISTEN)",
        "TCP [::1]:62086 (LISTEN)",
    ] {
        assert_eq!(
            line.split_whitespace()
                .last()
                .and_then(|tok| tok.rsplit(':').next())
                .and_then(|p| p.parse::<u16>().ok()),
            None,
            "pre-fix parser must not extract a port from {line:?}"
        );
    }

    // Malformed / non-listen lines are skipped, never mis-parsed.
    assert_eq!(lsof_line_endpoint("TCP 127.0.0.1:62488"), None);
    assert_eq!(lsof_line_endpoint(""), None);

    // The printed Connect multiaddrs the allowance is derived from.
    assert_eq!(
        multiaddr_endpoint("/ip4/127.0.0.1/tcp/53843"),
        Some(("127.0.0.1".to_string(), 53843))
    );
    assert_eq!(
        multiaddr_endpoint("/ip6/::1/tcp/9"),
        Some(("::1".to_string(), 9))
    );
    assert_eq!(multiaddr_endpoint("/ip4/127.0.0.1/udp/1"), None);
    assert_eq!(multiaddr_endpoint("not-a-multiaddr"), None);

    // The lane endpoint is the documented loopback default — pinned as a
    // literal so a silent constant change cannot pass unnoticed.
    assert_eq!(peer_tools_lane_endpoint(), ("127.0.0.1".to_string(), 8425));
}

/// The v1.210 P3 readiness-line parser accepts the locked shape (IPv4 and
/// IPv6 hosts) and rejects anything else — it feeds the listener allowance,
/// so a lax parse would weaken the no-HTTP assertion. The readiness line
/// prints a bracketed IPv6 host (`[::1]`), normalized here to `lsof`'s
/// unbracketed form.
#[test]
fn event_lane_endpoint_parses_the_readiness_line() {
    assert_eq!(
        event_lane_endpoint("event_lane: ws://127.0.0.1:8425/connect"),
        Some(("127.0.0.1".to_string(), 8425))
    );
    assert_eq!(
        event_lane_endpoint("  event_lane: ws://127.0.0.1:53127/connect  "),
        Some(("127.0.0.1".to_string(), 53127))
    );
    assert_eq!(
        event_lane_endpoint("event_lane: ws://[::1]:8425/connect"),
        Some(("::1".to_string(), 8425))
    );
    assert_eq!(event_lane_endpoint("event_lane: ws://127.0.0.1:8425"), None);
    assert_eq!(
        event_lane_endpoint("event_lane: wss://127.0.0.1:1/connect"),
        None
    );
    assert_eq!(
        event_lane_endpoint("event_lane: ws://127.0.0.1:0/connect"),
        Some(("127.0.0.1".to_string(), 0))
    );
    assert_eq!(event_lane_endpoint("listen: /ip4/127.0.0.1/tcp/1"), None);
    assert_eq!(event_lane_endpoint(""), None);
}

/// The disclosed-endpoint socket parse (S-2) covers both IP families and
/// fails closed — never panics — on anything but a bare IP host.
#[cfg(unix)]
#[test]
fn lane_socket_addr_covers_both_ip_families() {
    assert_eq!(
        lane_socket_addr(&("127.0.0.1".to_string(), 8425)),
        Some("127.0.0.1:8425".parse::<SocketAddr>().unwrap())
    );
    assert_eq!(
        lane_socket_addr(&("::1".to_string(), 8425)),
        Some("[::1]:8425".parse::<SocketAddr>().unwrap())
    );
    assert_eq!(lane_socket_addr(&("[::1]".to_string(), 8425)), None);
    assert_eq!(lane_socket_addr(&("not-an-ip".to_string(), 1)), None);
}

/// What a local control server does with an incoming connection.
#[cfg(unix)]
#[derive(Clone, Copy)]
enum HttpControl {
    /// Answers every request with a plain HTTP/1.1 response.
    Http,
    /// Answers a WebSocket upgrade request with the handshake and drops every
    /// other request without a response — the shape a WS accept loop
    /// (tungstenite) takes for a plain GET, which the booted lane shows.
    Ws,
}

/// Spin a local control server on an ephemeral `127.0.0.1` port.
///
/// The WS arm answers the RFC 6455 §1.3 example pair (the same
/// `Sec-WebSocket-Accept` the booted lane returns for that key), so the control
/// really speaks the handshake rather than being a dead socket.
#[cfg(unix)]
fn spawn_control_server(behavior: HttpControl) -> SocketAddr {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind control server");
    let addr = listener.local_addr().expect("control server addr");
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buffer = [0u8; 1024];
            let read = stream.read(&mut buffer).unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..read]).to_ascii_lowercase();
            match behavior {
                HttpControl::Http => {
                    let _ = stream.write_all(
                        b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                    );
                }
                HttpControl::Ws if request.contains("upgrade: websocket") => {
                    let _ = stream.write_all(
                        b"HTTP/1.1 101 Switching Protocols\r\nconnection: Upgrade\r\n\
                          upgrade: websocket\r\nsec-websocket-accept: \
                          s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n\r\n",
                    );
                }
                // A WS accept loop rejects a non-upgrade request by closing
                // with no response.
                HttpControl::Ws => {}
            }
        }
    });
    addr
}

/// The probe's own discrimination proof: it must report a plain HTTP server as
/// HTTP and a WS accept loop as NOT HTTP — the two halves a port number cannot
/// tell apart.
#[cfg(unix)]
#[test]
fn answers_plain_http_discriminates_an_http_router_from_a_ws_accept() {
    let http = spawn_control_server(HttpControl::Http);
    assert!(
        answers_plain_http(http, Duration::from_secs(5)),
        "an HTTP responder must be reported as HTTP"
    );
    let ws = spawn_control_server(HttpControl::Ws);
    assert!(
        !answers_plain_http(ws, Duration::from_secs(5)),
        "a WS accept loop must not be reported as HTTP"
    );

    // An endpoint with nothing listening is not an HTTP listener either.
    let free = TcpListener::bind(("127.0.0.1", 0)).expect("reserve a free port");
    let unused = free.local_addr().expect("reserved addr");
    drop(free);
    assert!(
        !answers_plain_http(unused, Duration::from_millis(500)),
        "an unbound endpoint must not be reported as HTTP"
    );
}

#[test]
fn version_flag_prints_crate_version() {
    let out = Command::new(env!("CARGO_BIN_EXE_nexus-runtime"))
        .arg("--version")
        .output()
        .expect("run nexus-runtime --version");
    assert!(out.status.success(), "status: {:?}", out.status);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.starts_with("nexus-runtime "),
        "unexpected --version output: {text:?}"
    );
    assert!(
        text.contains(env!("CARGO_PKG_VERSION")),
        "--version must print the crate version: {text:?}"
    );
}

// One end-to-end scenario: seed, spawn, probe, assert. Splitting it would let a
// later assertion pass against a runtime an earlier step never started.
#[allow(clippy::too_many_lines)]
#[test]
fn headless_runtime_prints_readiness_serves_connect_and_has_no_http_listener() {
    // The advertised list is an exact record, not a prefix: a trailing op
    // (e.g. an unexpected served route) fails this line comparison.
    const SERVED_OPS_LINE: &str = "SERVED_OPS=upsert,promote,relate,check,assemble,compute,\
tools.nexus.list_observed_peers,tools.nexus.list_modules";
    let tmp = tempfile::tempdir().expect("temp dir");
    seed_home(tmp.path());
    // The retained half: stored scoped World + staged session (and, where the
    // wasm32 target exists, the installed deterministic module), all before
    // the runtime opens the DB.
    seed_scoped_workspace(tmp.path());

    // Boot the real binary against the temp home. The guard kills the
    // child on EVERY exit path — success or panic — never orphaning it.
    let (child, ready_lines, listen_addrs) = spawn_runtime(tmp.path());
    let runtime_pid = child.id();
    let _guard = RuntimeGuard::new(child);

    // 1. Readiness block: the required lines are present on stdout.
    let ready = ready_lines.join("\n");
    for expected in [
        "Connect Host (N-C2 E2) ready",
        "peer_id:",
        "host_id:",
        "allowlisted peers: 1",
        "upsert/promote/relate/check/assemble/compute served",
        "tools.nexus.list_observed_peers / tools.nexus.list_modules (host-level reads)",
    ] {
        assert!(
            ready.contains(expected),
            "readiness block missing {expected:?}:\n{ready}"
        );
    }

    // 1b. Event-lane readiness (v1.210 P3 locked contract): the greppable
    //     `event_lane: ws://<host>:<port>/connect` line names the address
    //     the lane ACTUALLY bound. The hermetic home pins the lane to port
    //     0, so the printed port is the OS-assigned one — a config echo
    //     would print `0` and fail here.
    let lane_line = ready_lines
        .iter()
        .find(|line| line.trim_start().starts_with("event_lane: "))
        .unwrap_or_else(|| panic!("readiness block missing the event-lane line:\n{ready}"));
    let lane_endpoint = event_lane_endpoint(lane_line)
        .unwrap_or_else(|| panic!("unparseable event-lane readiness line: {lane_line:?}"));
    assert_eq!(lane_endpoint.0, "127.0.0.1", "the lane is loopback-only");
    assert_ne!(
        lane_endpoint.1, 0,
        "the readiness line must name the bound port, not the configured 0"
    );

    // 2. No HTTP/SPA listener: every TCP listener of the runtime process
    //    is one of its printed Connect listen addrs or the event lane
    //    endpoint the readiness line disclosed.
    assert_no_http_listener(runtime_pid, &listen_addrs, &lane_endpoint);

    // 3. The reference probe peer dials the host and completes the
    //    signed-hello handshake; the manifest advertises exactly the served
    //    ops (writes, reads, compute) and the session stays usable.
    let host_peer = ready_lines
        .iter()
        .find_map(|line| line.trim().strip_prefix("peer_id:"))
        .expect("peer_id line")
        .trim();

    let probe_path = probe_binary();
    assert!(
        probe_path.exists(),
        "probe example not built — expected at {}",
        probe_path.display()
    );
    let mut probe = Command::new(&probe_path);
    // `--world` is always supplied: the probe proves the session still works
    // after the refusal with an in-scope `upsert`. The compute session is
    // added only where the module can be installed.
    probe.args([
        "--addr",
        &listen_addrs[0],
        "--host-peer",
        host_peer,
        "--world",
        COMPUTE_WORLD,
    ]);
    #[cfg(not(nexus42_no_wasm_target))]
    probe.args(["--compute-session", COMPUTE_SESSION]);
    let out = probe.output().expect("run runtime_smoke_probe");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let status = out.status;
    assert!(
        status.success(),
        "probe failed (status {status})\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    // The advertised list is an exact record, not a prefix: a trailing op
    // (e.g. an unexpected served route) fails this line comparison.
    assert!(
        stdout.lines().any(|line| line.trim() == SERVED_OPS_LINE),
        "probe output missing the exact {SERVED_OPS_LINE:?} line:\n{stdout}\nstderr:\n{stderr}"
    );
    for expected in ["DIAL_OK", "SESSION_OK", "UPSERT_OK", "SESSION_ALIVE_OK"] {
        assert!(
            stdout.contains(expected),
            "probe output missing {expected:?}:\n{stdout}\nstderr:\n{stderr}"
        );
    }
    // Refusal attribution (QC1-02): the same `op_unsupported` code covers an
    // unserved op, an out-of-scope op and a malformed payload. The smoke only
    // claims the authorization boundary, so it must see the SERVED op refused
    // by this peer's `op_scope` reason — with the unserved control printed
    // alongside to show the two reasons are really different.
    assert_eq!(
        probe_value(&stdout, "UNSERVED_REFUSAL_CODE").unwrap_or_default(),
        "op_unsupported"
    );
    assert!(
        probe_value(&stdout, "UNSERVED_REFUSAL_MESSAGE")
            .unwrap_or_default()
            .starts_with("op project is not supported:"),
        "the unserved control must be the served-op gate (not a scope denial):\n{stdout}"
    );
    assert_eq!(
        assert_out_of_scope_refusal(
            probe_value(&stdout, "OP_SCOPE_REFUSAL_CODE").unwrap_or_default(),
            probe_value(&stdout, "OP_SCOPE_REFUSAL_MESSAGE").unwrap_or_default(),
        ),
        Ok(()),
        "the refusal must be the out-of-scope denial, not request validation:\n{stdout}\nstderr:\n{stderr}"
    );
    // 3b. A REAL retained compute op ran end-to-end in the spawned process:
    //     the probe seeded both combatants over the served `upsert` and the
    //     host's compute lane executed the installed deterministic module,
    //     returning the staged 30 HP − (ATK 20 − DEF 5) = 15. The value is
    //     parsed and compared exactly, so 150/1500 cannot satisfy it (the
    //     neighbouring-value control above covers the matcher itself).
    #[cfg(not(nexus42_no_wasm_target))]
    assert_eq!(
        assert_compute_result(&stdout, COMPUTE_SESSION),
        Ok(()),
        "the deterministic compute result must be exactly 15 HP:\n{stdout}\nstderr:\n{stderr}"
    );
    // On a no-wasm host the compute half is absent by construction, and the
    // ignored test above names that skip; this assertion keeps the absence
    // explicit rather than letting a stray COMPUTE_OK line imply it ran.
    #[cfg(nexus42_no_wasm_target)]
    assert!(
        !stdout.contains("COMPUTE_OK"),
        "compute cannot run without the wasm32 target:\n{stdout}"
    );
    // `_guard` drops here (and on any panic unwind above): the child is
    // killed and reaped on every path.
}
