//! Hermetic CLI surface tests for `creator works use` and `creator works
//! completion-lock` (V1.44 P3 — R-V141P0-04).
//!
//! Plan: `2026-06-13-v1.44-author-desk-residual-convergence`
//!
//! These tests verify the CLI subcommand surface, help text, and argument
//! validation for `creator works use` and `creator works completion-lock`
//! without requiring a running daemon. Daemon handler tests for pool and
//! completion-lock behavior are in `nexus-daemon-runtime/tests/works_api.rs`.
//!
//! Run with: cargo test -p nexus42 --test `creator_works`

use assert_cmd::Command;

// =============================================================================
// `creator works use` — CLI surface
// =============================================================================

/// `creator works use --help` documents the subcommand.
#[test]
fn works_use_help_shows_expected_text() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "works", "use", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    assert!(
        help_text.contains("<WORK_ID>"),
        "works use --help must show WORK_ID argument"
    );
    assert!(
        help_text.contains("active"),
        "works use --help must mention 'active' (pool active row)"
    );
    assert!(
        help_text.contains("default"),
        "works use --help must mention 'default' (CLI default work_id)"
    );
}

/// `creator works use` requires a `work_id` positional argument.
#[test]
fn works_use_requires_work_id() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "works", "use"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("WORK_ID"));
}

/// `creator works --help` lists `use` as a subcommand.
#[test]
fn works_help_lists_use_subcommand() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "works", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    assert!(
        help_text.contains("use"),
        "creator works --help must list 'use' subcommand"
    );
}

// =============================================================================
// `creator works completion-lock` — CLI surface
// =============================================================================

/// `creator works completion-lock --help` shows subcommands.
#[test]
fn works_completion_lock_help_shows_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "works", "completion-lock", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    assert!(
        help_text.contains("release"),
        "completion-lock --help must list 'release' subcommand"
    );
}

/// `creator works completion-lock release --help` documents flags.
#[test]
fn works_completion_lock_release_help_shows_expected_text() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "works", "completion-lock", "release", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    assert!(
        help_text.contains("<WORK_ID>"),
        "completion-lock release --help must show WORK_ID argument"
    );
    assert!(
        help_text.contains("--json"),
        "completion-lock release --help must list --json flag"
    );
}

/// `creator works completion-lock release` requires a `work_id` argument.
#[test]
fn works_completion_lock_release_requires_work_id() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "works", "completion-lock", "release"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("WORK_ID"));
}

// =============================================================================
// Cross-reference: `creator works` subcommand enumeration
// =============================================================================

/// Verify that `creator works --help` lists all expected subcommands.
#[test]
fn works_help_lists_all_expected_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "works", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    for subcmd in &["list", "status", "use", "completion-lock", "pool"] {
        assert!(
            help_text.contains(subcmd),
            "creator works --help must list '{subcmd}' subcommand"
        );
    }
}

// =============================================================================
// `creator works intake` — CLI surface (V1.49 P2, R-V147P1-01)
// =============================================================================

/// `creator works intake --help` documents the subcommand and flags.
#[test]
fn works_intake_help_shows_expected_text() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "works", "intake", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    assert!(
        help_text.contains("[<WORK_ID>]") || help_text.contains("WORK_ID"),
        "works intake --help must show the optional WORK_ID argument: {help_text}"
    );
    assert!(
        help_text.contains("--json"),
        "works intake --help must list --json flag: {help_text}"
    );
    assert!(
        help_text.contains("creative-brief-intake"),
        "works intake --help must mention the creative-brief-intake preset: {help_text}"
    );
}

/// `creator works --help` lists the `intake` subcommand.
#[test]
fn works_help_lists_intake_subcommand() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "works", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    assert!(
        help_text.contains("intake"),
        "creator works --help must list 'intake' subcommand: {help_text}"
    );
}

// =============================================================================
// `creator works reconcile-chapters` — dry-run / --yes flags (V1.49 P2, R-V148P4-W2)
// =============================================================================

/// `creator works reconcile-chapters --help` documents the new safety flags.
#[test]
fn works_reconcile_chapters_help_lists_dry_run_and_yes() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "works", "reconcile-chapters", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    assert!(
        help_text.contains("--dry-run"),
        "reconcile-chapters --help must list --dry-run: {help_text}"
    );
    assert!(
        help_text.contains("--yes"),
        "reconcile-chapters --help must list --yes: {help_text}"
    );
    assert!(
        help_text.contains("-y"),
        "reconcile-chapters --help must list the -y short form: {help_text}"
    );
}

/// `--yes` help text must not over-promise an inline preview (V1.49 P2 fix,
/// R-V149P2-01 / qc1 W-1). `confirm_reconcile_interactive` only prompts; the
/// preview lives behind `--dry-run`, which the help text must point to.
#[test]
fn works_reconcile_chapters_help_yes_does_not_promise_inline_preview() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "works", "reconcile-chapters", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    // The over-promising phrase from qc1 W-1 must be gone.
    assert!(
        !help_text.contains("prints a preview"),
        "reconcile-chapters --help for --yes must not promise an inline preview: {help_text}"
    );
    // The preview is accurately routed to --dry-run.
    assert!(
        help_text.contains("--dry-run") && help_text.contains("preview"),
        "reconcile-chapters --help must point to --dry-run for the preview: {help_text}"
    );
}

// =============================================================================
// `creator workspace` init/list/use, `creator demo-seed` and `creator status`
// — direct local path (v1.193 P0-T2)
// =============================================================================

/// Run the real `nexus42` binary against a hermetic `HOME` from a working
/// directory that contains no workspace marker.
fn hermetic_cli(
    home: &std::path::Path,
    cwd: &std::path::Path,
    args: &[&str],
) -> std::process::Output {
    Command::cargo_bin("nexus42")
        .unwrap()
        .env("HOME", home)
        .env("RUST_LOG", "off")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run nexus42")
}

/// Combined `stdout` + `stderr` of a child run.
///
/// Next-step instructions are printed on `stdout`, so an assertion on
/// `stderr` alone cannot see a retired-command instruction
/// (v1.193 P0-T2 fix 1).
fn combined_output(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Display name seeded into the core-owned identity cache by
/// [`patch_core_identity`].
const CORE_OWNED_DISPLAY_NAME: &str = "Core Owned Author";

/// Seed the core-owned identity projection through its production writer
/// (v1.193 P0-T2 fix 2).
///
/// `CoreHomeService::patch_creator` is the writer the daemon PATCH route and
/// the Node bridge call, so the status regression drives that same entry point
/// instead of hand-writing cache bytes no production caller would create. The
/// creator must already be selected with a materialized workspace state DB:
/// the writer upserts that workspace's `creators` row on the way through.
fn patch_core_identity(home: &std::path::Path, creator_id: &str, display_name: &str) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime for the in-process core writer");
    runtime.block_on(async {
        let service = nexus_core::CoreHomeService::open(home.to_path_buf())
            .expect("the core home entry opens on the raw home");
        service
            .patch_creator(creator_id, Some(display_name.to_string()))
            .await
            .expect("the production identity writer patches the core cache");
    });
}

/// NEW (v1.193 P0-T2): workspace initialization, selection and the demo seed
/// are local core/filesystem work — no daemon is consulted.
///
/// Defends the removed daemon-first branch: `creator workspace init` must
/// materialize the ADR-014 layout and commit the selection locally, the
/// retained `creator demo-seed` must stay real and idempotent without
/// `force`), the committed selection must stay usable, and the removed
/// workspace leaves must be unknown. The fixture points `daemon_url` at a port
/// nothing listens on, and every command asserts on the combined
/// `stdout`+`stderr` stream, so a reintroduced daemon instruction (or a health
/// probe warning) fails the case on whichever stream it appears.
#[allow(clippy::too_many_lines)] // single local-home lifecycle proof
#[test]
fn workspace_init_and_demo_seed_are_local_and_idempotent() {
    let home = tempfile::tempdir().expect("temp home");
    // Initialization refuses a directory tree that already contains a
    // workspace marker (`.nexus42`), so the child runs from a clean tree.
    let cwd = tempfile::tempdir().expect("temp cwd");
    // Explicit creative root: the whole flow stays inside the temp home on
    // every platform instead of resolving a documents directory.
    let creative_root = home.path().join("creative");

    let nexus_dir = home.path().join(".nexus42");
    std::fs::create_dir_all(&nexus_dir).expect("create .nexus42");
    std::fs::write(
        nexus_dir.join("config.toml"),
        "daemon_url = \"http://127.0.0.1:1\"\n",
    )
    .expect("seed config.toml");

    // --- 1. A local creator selection, committed locally (no listener) ---
    let select_creator = hermetic_cli(home.path(), cwd.path(), &["creator", "use", "local"]);
    let select_creator_stdout = String::from_utf8_lossy(&select_creator.stdout);
    let select_creator_stderr = String::from_utf8_lossy(&select_creator.stderr);
    assert!(
        select_creator.status.success(),
        "creator use must commit the selection locally: {select_creator_stdout}\n\
         {select_creator_stderr}"
    );
    let select_creator_output = combined_output(&select_creator);
    assert!(
        !select_creator_output.contains("daemon"),
        "creator use must not consult a daemon (stdout+stderr): {select_creator_output}"
    );

    // --- 2. Init: local materialization + committed selection, no listener ---
    let creative_arg = creative_root.to_str().expect("utf-8 creative root");
    let init = hermetic_cli(
        home.path(),
        cwd.path(),
        &[
            "creator",
            "workspace",
            "init",
            "workspace",
            "--creative-root",
            creative_arg,
        ],
    );
    let init_stdout = String::from_utf8_lossy(&init.stdout);
    let init_stderr = String::from_utf8_lossy(&init.stderr);
    assert!(
        init.status.success(),
        "workspace init must succeed without a daemon:\n{init_stdout}\n{init_stderr}"
    );
    let init_output = combined_output(&init);
    assert!(
        !init_output.contains("daemon"),
        "workspace init must not instruct or consult a daemon \
         (stdout+stderr): {init_output}"
    );
    assert!(
        init_stdout.contains("creator works cron"),
        "workspace init next steps must point at the retained local scheduling \
         leaf: {init_stdout}"
    );
    assert!(
        init_stdout.contains("nexus42 preset list"),
        "workspace init next steps must name the canonical retained preset group: {init_stdout}"
    );
    assert!(
        !init_stdout.contains("system preset list"),
        "workspace init next steps must not advertise the removed `system preset` \
         forwarding alias: {init_stdout}"
    );
    assert!(
        home.path()
            .join(".nexus42/creators/local/workspaces/default/meta.json")
            .is_file(),
        "init must materialize the operational workspace registration"
    );
    assert!(
        creative_root.join(".nexus42/workspace.json").is_file(),
        "init must materialize the creative tree"
    );
    let config = std::fs::read_to_string(nexus_dir.join("config.toml")).expect("read config.toml");
    assert!(
        config.contains("active_creator_id = \"local\""),
        "init must commit the active creator locally: {config}"
    );

    // --- 3. The committed selection stays usable ---
    let list = hermetic_cli(home.path(), cwd.path(), &["creator", "workspace", "list"]);
    let list_stdout = String::from_utf8_lossy(&list.stdout);
    let list_stderr = String::from_utf8_lossy(&list.stderr);
    assert!(
        list.status.success(),
        "workspace list must succeed: {list_stdout}\n{list_stderr}"
    );
    assert!(
        list_stdout.contains("default (active)"),
        "workspace list must show the initialized workspace as active: {list_stdout}"
    );
    let list_output = combined_output(&list);
    assert!(
        !list_output.contains("daemon"),
        "workspace list must not consult a daemon (stdout+stderr): {list_output}"
    );

    let reselect = hermetic_cli(
        home.path(),
        cwd.path(),
        &["creator", "workspace", "use", "default"],
    );
    assert!(
        reselect.status.success(),
        "re-selecting the initialized workspace must succeed: {}",
        combined_output(&reselect)
    );
    let reselect_output = combined_output(&reselect);
    assert!(
        !reselect_output.contains("daemon"),
        "workspace use must not consult a daemon (stdout+stderr): {reselect_output}"
    );

    let missing = hermetic_cli(
        home.path(),
        cwd.path(),
        &["creator", "workspace", "use", "missing-slug"],
    );
    assert!(
        !missing.status.success(),
        "selecting a workspace that was never materialized must fail"
    );

    // --- 4. demo-seed is real work and idempotent without --force ---
    let seed = hermetic_cli(home.path(), cwd.path(), &["creator", "demo-seed"]);
    let seed_stdout = String::from_utf8_lossy(&seed.stdout);
    let seed_stderr = String::from_utf8_lossy(&seed.stderr);
    assert!(
        seed.status.success(),
        "demo-seed must succeed: {seed_stdout}\n{seed_stderr}"
    );
    let seed_output = combined_output(&seed);
    assert!(
        !seed_output.contains("daemon"),
        "demo-seed must not consult a daemon (stdout+stderr): {seed_output}"
    );
    let world_id = seed_stdout
        .lines()
        .find_map(|line| line.strip_prefix("✓ Demo world: "))
        .expect("demo-seed must report the seeded world")
        .trim()
        .to_string();
    assert!(
        world_id.starts_with("wld_"),
        "seeded world id must be a real `wld_` id, got {world_id:?}"
    );

    let seeded_again = hermetic_cli(home.path(), cwd.path(), &["creator", "demo-seed"]);
    let seeded_again_stdout = String::from_utf8_lossy(&seeded_again.stdout);
    let seeded_again_stderr = String::from_utf8_lossy(&seeded_again.stderr);
    assert!(
        seeded_again.status.success(),
        "a repeated demo-seed must succeed: {seeded_again_stdout}\n{seeded_again_stderr}"
    );
    assert!(
        seeded_again_stdout.contains(&format!("Demo world already exists: {world_id}")),
        "a repeated demo-seed must find the same world instead of seeding another: \
         {seeded_again_stdout}"
    );
    assert!(
        !seeded_again_stdout.contains("✓ Demo world:"),
        "a repeated demo-seed must not create a second world: {seeded_again_stdout}"
    );
    let seeded_again_output = combined_output(&seeded_again);
    assert!(
        !seeded_again_output.contains("daemon"),
        "a repeated demo-seed must not consult a daemon (stdout+stderr): \
         {seeded_again_output}"
    );

    // --- 5. The removed workspace leaves are unknown ---
    for leaf in ["clone", "link", "unlink", "status"] {
        let output = hermetic_cli(home.path(), cwd.path(), &["creator", "workspace", leaf]);
        let combined = combined_output(&output);
        assert!(
            !output.status.success(),
            "removed leaf `creator workspace {leaf}` must not exit 0: {combined}"
        );
        assert!(
            combined.contains("unrecognized subcommand"),
            "removed leaf `creator workspace {leaf}` must be an unknown subcommand: {combined}"
        );
    }
}

/// NEW (v1.193 P0-T2 fix 1): `creator status` reads local identity and
/// credential state and never mediates through the configured daemon URL.
///
/// Discriminating regression for the removed daemon transport: the fixture
/// serves `daemon_url` with a counting loopback listener. The pre-fix leaf
/// constructed `DaemonClient` and probed `/v1/daemon/runtime/health` before
/// displaying anything, so the listener observed a connection and this case
/// failed; the retargeted leaf must print the retained three-layer identity
/// plus credential lines with **zero** connections.
#[test]
fn creator_status_is_local_and_never_probes_the_daemon() {
    use std::io::ErrorKind;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    let home = tempfile::tempdir().expect("temp home");
    let cwd = tempfile::tempdir().expect("temp cwd");

    // Counting loopback listener standing in for the configured daemon URL.
    // Accept-and-close: a probe is counted, never answered.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind probe listener");
    listener
        .set_nonblocking(true)
        .expect("nonblocking listener");
    let daemon_url = format!("http://{}", listener.local_addr().expect("probe addr"));
    let probes = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let accept_loop = {
        let probes = Arc::clone(&probes);
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        probes.fetch_add(1, Ordering::SeqCst);
                        drop(stream);
                    }
                    Err(err) if err.kind() == ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        })
    };

    let nexus_dir = home.path().join(".nexus42");
    std::fs::create_dir_all(&nexus_dir).expect("create .nexus42");
    std::fs::write(
        nexus_dir.join("config.toml"),
        format!("daemon_url = \"{daemon_url}\"\n"),
    )
    .expect("seed config.toml");

    let select_creator = hermetic_cli(home.path(), cwd.path(), &["creator", "use", "local"]);
    assert!(
        select_creator.status.success(),
        "creator use must commit the selection locally: {}",
        combined_output(&select_creator)
    );

    // Attribute every observed connection to `creator status` alone.
    probes.store(0, Ordering::SeqCst);
    let status = hermetic_cli(home.path(), cwd.path(), &["creator", "status"]);
    // Drain window: a connection the child opened is queued and accepted here.
    std::thread::sleep(Duration::from_millis(100));
    stop.store(true, Ordering::SeqCst);
    accept_loop.join().expect("join probe listener");

    let status_output = combined_output(&status);
    let status_stdout = String::from_utf8_lossy(&status.stdout);
    assert!(
        status.status.success(),
        "creator status must succeed without a daemon: {status_output}"
    );
    for label in ["Handle:", "Display Name:", "Auth:"] {
        assert!(
            status_stdout.contains(label),
            "creator status must keep the `{label}` line: {status_stdout}"
        );
    }
    let creator_id_line = status_stdout
        .lines()
        .find(|line| line.starts_with("Creator ID:"))
        .unwrap_or_else(|| {
            panic!("creator status must print the creator id line: {status_stdout}")
        });
    assert!(
        creator_id_line.trim_end().ends_with("local"),
        "creator status must report the locally selected creator: {creator_id_line}"
    );
    assert!(
        !status_output.contains("daemon"),
        "creator status must not mention a daemon (stdout+stderr): {status_output}"
    );
    assert_eq!(
        probes.load(Ordering::SeqCst),
        0,
        "creator status must not open any connection to the configured daemon URL ({daemon_url})"
    );
}

/// NEW (v1.193 P0-T2 fix 2): `creator status` renders the identity the **core
/// owner** holds, with no daemon.
///
/// The core cache (`creator_identity_cache.json`) is written by
/// `CoreHomeService::patch_creator` and read by `active_creator`; the pre-fix
/// leaf read only the CLI-private `creator-identities.json`, so a creator
/// whose display name lives in the core cache rendered `Display Name:  -`.
/// This case seeds the core cache through that production writer and requires
/// the retained four-line status to project it, while the fixture points
/// `daemon_url` at a port nothing listens on.
///
/// What it does not establish: precedence when both caches hold a value for
/// the same creator (only the cloud registration bridge writes the CLI-local
/// cache, so that would need a second mock-platform fixture), and no coverage
/// of any other retained leaf.
#[test]
fn creator_status_renders_core_owned_identity() {
    let home = tempfile::tempdir().expect("temp home");
    let cwd = tempfile::tempdir().expect("temp cwd");
    let creative_root = home.path().join("creative");

    let nexus_dir = home.path().join(".nexus42");
    std::fs::create_dir_all(&nexus_dir).expect("create .nexus42");
    std::fs::write(
        nexus_dir.join("config.toml"),
        "daemon_url = \"http://127.0.0.1:1\"\n",
    )
    .expect("seed config.toml");

    // A selected creator with a materialized workspace: the core writer
    // upserts that workspace's `creators` row, so the state DB must exist.
    let select = hermetic_cli(home.path(), cwd.path(), &["creator", "use", "local"]);
    assert!(
        select.status.success(),
        "creator use must commit the selection locally: {}",
        combined_output(&select)
    );
    let init = hermetic_cli(
        home.path(),
        cwd.path(),
        &[
            "creator",
            "workspace",
            "init",
            "workspace",
            "--creative-root",
            creative_root.to_str().expect("utf-8 creative root"),
        ],
    );
    assert!(
        init.status.success(),
        "workspace init must succeed without a daemon: {}",
        combined_output(&init)
    );

    // The production writer the daemon PATCH route calls: the core owns the
    // identity cache, not the CLI.
    patch_core_identity(home.path(), "local", CORE_OWNED_DISPLAY_NAME);

    let status = hermetic_cli(home.path(), cwd.path(), &["creator", "status"]);
    let status_output = combined_output(&status);
    let status_stdout = String::from_utf8_lossy(&status.stdout);
    assert!(
        status.status.success(),
        "creator status must succeed without a daemon: {status_output}"
    );
    let display_line = status_stdout
        .lines()
        .find(|line| line.starts_with("Display Name:"))
        .unwrap_or_else(|| {
            panic!("creator status must keep the display-name line: {status_stdout}")
        });
    assert_eq!(
        display_line.trim_end(),
        format!("Display Name:  {CORE_OWNED_DISPLAY_NAME}"),
        "creator status must project the display name the core owner holds: {status_stdout}"
    );
    let handle_line = status_stdout
        .lines()
        .find(|line| line.starts_with("Handle:"))
        .unwrap_or_else(|| panic!("creator status must keep the handle line: {status_stdout}"));
    assert_eq!(
        handle_line.trim_end(),
        "Handle:        -",
        "a display-only core entry must not invent a handle: {status_stdout}"
    );
    assert!(
        status_stdout.contains("Creator ID:    local"),
        "creator status must still report the selected creator: {status_stdout}"
    );
    assert!(
        !status_output.contains("daemon"),
        "creator status must not mention a daemon (stdout+stderr): {status_output}"
    );
}
