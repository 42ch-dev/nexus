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

/// Display name the core owner holds for a creator the CLI metadata cache
/// knows by id only (v1.193 P0-T2 fix 3).
const CORE_ONLY_DISPLAY_NAME: &str = "Core Only Author";

/// Stale CLI-local display name for the creator the core owner also holds —
/// the value a pre-fix `creator list` rendered (v1.193 P0-T2 fix 3).
const CLI_STALE_DISPLAY_NAME: &str = "CLI Stale Name";

/// Handle only the CLI-local cache holds: the core owner's production writer
/// persists no handle, so this field must keep falling back to the CLI cache
/// (v1.193 P0-T2 fix 3).
const CLI_FALLBACK_HANDLE: &str = "cli-h";

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

/// Shared fixture for the core-owner identity regressions: a `daemon_url`
/// nothing listens on, the `local` creator selected, and its workspace
/// materialized.
///
/// The selection uses the path-safe `local` id so no identity store has to
/// exist first, and the workspace must be materialized before
/// [`patch_core_identity`] runs — the core writer upserts that workspace's
/// `creators` row on its way to the identity cache.
fn seed_selected_local_creator(home: &std::path::Path, cwd: &std::path::Path) {
    let nexus_dir = home.join(".nexus42");
    std::fs::create_dir_all(&nexus_dir).expect("create .nexus42");
    std::fs::write(
        nexus_dir.join("config.toml"),
        "daemon_url = \"http://127.0.0.1:1\"\n",
    )
    .expect("seed config.toml");

    let select = hermetic_cli(home, cwd, &["creator", "use", "local"]);
    assert!(
        select.status.success(),
        "creator use must commit the selection locally: {}",
        combined_output(&select)
    );

    let creative_root = home.join("creative");
    let init = hermetic_cli(
        home,
        cwd,
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
/// the same creator (that is [`creator_list_renders_core_owned_identity`],
/// v1.193 P0-T2 fix 3), and no coverage of any other retained leaf.
#[test]
fn creator_status_renders_core_owned_identity() {
    let home = tempfile::tempdir().expect("temp home");
    let cwd = tempfile::tempdir().expect("temp cwd");
    seed_selected_local_creator(home.path(), cwd.path());

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

/// NEW (v1.193 P0-T2 fix 3): `creator list` renders the identity the **core
/// owner** holds, per field over the CLI-local cache.
///
/// `list` projected `handle`/`display_name` from the CLI-private
/// `creator-identities.json` alone while the core owner holds the same
/// projection in `creator_identity_cache.json` — the divergence fix 2 closed
/// for `creator status`, on a second surface of the same command family. The
/// fixture seeds both sources: the core owner through its production writer
/// ([`patch_core_identity`]) with a display name the CLI cache also claims for
/// the same id, and the CLI cache with an id-only entry for a second creator.
/// The listing must therefore show the core value over the stale CLI one, the
/// core value where the CLI holds nothing, and the handle the core does not
/// hold — with no daemon reachable.
///
/// What it does not establish: row membership and ordering (deliberately
/// unchanged — the core's Profile-directory SSOT is not a listing source
/// here), the DTO beyond the two seeded rows, and any other surface.
#[test]
fn creator_list_renders_core_owned_identity() {
    let home = tempfile::tempdir().expect("temp home");
    let cwd = tempfile::tempdir().expect("temp cwd");
    seed_selected_local_creator(home.path(), cwd.path());

    // The core owner's values: one overriding stale CLI metadata, one for a
    // creator the CLI metadata cache knows by id only.
    patch_core_identity(home.path(), "local", CORE_OWNED_DISPLAY_NAME);
    patch_core_identity(home.path(), "ctr_coreonly", CORE_ONLY_DISPLAY_NAME);

    // The CLI metadata cache is the platform row source and the per-field
    // fallback. Seeded as JSON: its production writer resolves `HOME` from
    // this process's environment, which a shared test binary must not mutate.
    let cli_cache = serde_json::json!({
        "creators": {
            "local": {
                "creator_id": "local",
                "handle": CLI_FALLBACK_HANDLE,
                "display_name": CLI_STALE_DISPLAY_NAME,
            },
            "ctr_coreonly": {
                "creator_id": "ctr_coreonly",
                "handle": null,
                "display_name": null,
            },
        }
    });
    std::fs::write(
        home.path().join(".nexus42").join("creator-identities.json"),
        serde_json::to_string_pretty(&cli_cache).expect("serialize the CLI metadata cache"),
    )
    .expect("seed the CLI metadata cache");

    let list = hermetic_cli(home.path(), cwd.path(), &["creator", "list"]);
    let list_output = combined_output(&list);
    let list_stdout = String::from_utf8_lossy(&list.stdout);
    assert!(
        list.status.success(),
        "creator list must succeed without a daemon: {list_output}"
    );
    let overridden = list_stdout.lines().find(|line| line.starts_with("local"));
    let overridden = overridden
        .unwrap_or_else(|| panic!("creator list must render the seeded creator: {list_stdout}"));
    assert!(
        overridden.contains(CORE_OWNED_DISPLAY_NAME),
        "creator list must project the display name the core owner holds: {list_stdout}"
    );
    assert!(
        !overridden.contains(CLI_STALE_DISPLAY_NAME),
        "a stale CLI display name must not shadow the core owner's value: {list_stdout}"
    );
    assert!(
        overridden.contains(CLI_FALLBACK_HANDLE),
        "the handle the core owner does not hold must still come from the CLI cache: {list_stdout}"
    );

    let core_only = list_stdout
        .lines()
        .find(|line| line.starts_with("ctr_coreonly"))
        .unwrap_or_else(|| panic!("creator list must render the id-only creator: {list_stdout}"));
    assert!(
        core_only.contains(CORE_ONLY_DISPLAY_NAME),
        "creator list must render a core-held identity the CLI cache lacks: {list_stdout}"
    );

    // The pinned machine DTO carries the same projection with exact fields
    // (the human table above is asserted by content, not by column offsets).
    let json = hermetic_cli(home.path(), cwd.path(), &["creator", "list", "--json"]);
    let json_output = combined_output(&json);
    assert!(
        json.status.success(),
        "creator list --json must succeed without a daemon: {json_output}"
    );
    let dto: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&json.stdout))
        .expect("creator list --json must emit the pinned DTO array");
    assert_eq!(
        dto,
        serde_json::json!([
            {
                "creator_id": "ctr_coreonly",
                "handle": null,
                "display_name": CORE_ONLY_DISPLAY_NAME,
                "active": false,
                "origin": "platform",
            },
            {
                "creator_id": "local",
                "handle": CLI_FALLBACK_HANDLE,
                "display_name": CORE_OWNED_DISPLAY_NAME,
                "active": true,
                "origin": "platform",
            }
        ]),
        "the DTO must carry the core-owned identity for both rows, sorted by id: {dto}"
    );
    assert!(
        !list_output.contains("daemon") && !json_output.contains("daemon"),
        "creator list must not consult a daemon (stdout+stderr): {list_output}{json_output}"
    );
}

// =============================================================================
// `creator works pool` / `creator works inspire` — separate stores, direct core
// (v1.193 P0-T5)
// =============================================================================

/// Parse a `--json` child's stdout, failing the case with both streams when the
/// payload is not the expected JSON document.
fn json_stdout(output: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "expected a JSON payload on stdout ({err}): {}",
            combined_output(output)
        )
    })
}

/// NEW (v1.193 P0-T5): the pool-level inspiration store (`inspiration_items`,
/// `works pool inspiration …`) and the per-Work `inspiration_log`
/// (`works inspire`) are separate producers, and `works status|use` read the
/// Work from the core without touching the configured daemon URL.
///
/// The regression this defends is confusing the two inspiration stores: a pool
/// item append landing in the Work's `inspiration_log`, or — the direction a
/// reader is most likely to get wrong — the Work leaf `works inspire` writing a
/// pool item. The fixture therefore drives the real binary through the pool
/// store (add → list → promote), through the Work the promotion created, and
/// through `works inspire`, asserting after each step what the *other* store
/// holds. It also reads both `--json` DTOs and the retained `works use`
/// selection.
///
/// `daemon_url` is a counting loopback listener, so the whole sequence is also
/// evidence that the Work read and the pool producers never consult the daemon
/// (the retired leaves read the Work over HTTP). The `--json` status path is
/// used for that claim: since v1.193 P0-T7 the findings/stale enrichment reads
/// the typed core too, so a non-novel `--json` status makes neither read and
/// the human path's stale banner comes from the same in-process read.
///
/// What it does not establish: pool pagination/filter edges beyond the
/// single promoted row, and any concurrency behavior. The omitted `<work_id>`
/// step is a smoke check that the resolution addresses the pool `active` row
/// the promotion wrote; [`omitted_work_id_uses_pool_active_entry`] is the
/// regression that discriminates that selector (two Works, a newer non-active
/// pool row, an explicit id, and the no-active refusal).
#[allow(clippy::too_many_lines)] // one cross-write negative proof over a single local home
#[test]
fn work_pool_and_work_inspiration_do_not_cross_write() {
    use std::io::ErrorKind;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    let home = tempfile::tempdir().expect("temp home");
    let cwd = tempfile::tempdir().expect("temp cwd");
    seed_selected_local_creator(home.path(), cwd.path());

    // Counting loopback listener standing in for the configured daemon URL:
    // accept-and-close, so a probe is counted and never answered. The accept
    // loop is deliberately local to this case — the frozen v1.193 P0-T2 case
    // keeps its own copy rather than being reworked.
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

    // Re-point the seeded selection at the probe. `seed_selected_local_creator`
    // writes the dead-port literal below; re-reading the file keeps the
    // `active_creator_id` / workspace keys `workspace init` committed.
    let config_path = home.path().join(".nexus42").join("config.toml");
    let seeded_config = std::fs::read_to_string(&config_path).expect("read the seeded config.toml");
    assert!(
        seeded_config.contains("http://127.0.0.1:1"),
        "the fixture must seed the dead-daemon URL this case re-points: {seeded_config}"
    );
    std::fs::write(
        &config_path,
        seeded_config.replace("http://127.0.0.1:1", &daemon_url),
    )
    .expect("re-point daemon_url at the probe listener");

    // Attribute every observed connection to the sequence below.
    probes.store(0, Ordering::SeqCst);

    // --- 1. A pool inspiration item is a pool-store write only ------------
    let add = hermetic_cli(
        home.path(),
        cwd.path(),
        &[
            "creator",
            "works",
            "pool",
            "inspiration",
            "add",
            POOL_ITEM_TITLE,
            "--json",
        ],
    );
    assert!(
        add.status.success(),
        "pool inspiration add must succeed on the direct core: {}",
        combined_output(&add)
    );
    let added = json_stdout(&add);
    let item_id = added
        .get("item_id")
        .and_then(|v| v.as_str())
        .expect("the add DTO must carry the item id")
        .to_string();
    assert!(
        item_id.starts_with("npi_"),
        "the pool store must mint an `npi_` item id, got {item_id:?}"
    );
    assert!(
        added
            .get("rel_path")
            .and_then(|v| v.as_str())
            .is_some_and(|p| !p.is_empty()),
        "the add DTO must carry the item scaffold path: {added}"
    );

    let works_after_add = hermetic_cli(
        home.path(),
        cwd.path(),
        &["creator", "works", "list", "--json"],
    );
    assert!(
        works_after_add.status.success(),
        "works list must succeed on the direct core: {}",
        combined_output(&works_after_add)
    );
    let works_page = json_stdout(&works_after_add);
    assert_eq!(
        works_page
            .get("items")
            .and_then(|v| v.as_array())
            .map(Vec::len),
        Some(0),
        "a pool inspiration item must not create a Work: {works_page}"
    );

    let listed_idea = hermetic_cli(
        home.path(),
        cwd.path(),
        &["creator", "works", "pool", "inspiration", "list", "--json"],
    );
    assert!(
        listed_idea.status.success(),
        "pool inspiration list must succeed on the direct core: {}",
        combined_output(&listed_idea)
    );
    let idea_page = json_stdout(&listed_idea);
    let idea_items = idea_page
        .get("items")
        .and_then(|v| v.as_array())
        .expect("the pool list DTO must carry an items array");
    assert_eq!(
        idea_items.len(),
        1,
        "the pool list must show exactly the added item: {idea_page}"
    );
    assert_eq!(
        idea_items[0].get("item_id").and_then(|v| v.as_str()),
        Some(item_id.as_str())
    );
    assert_eq!(
        idea_items[0].get("status").and_then(|v| v.as_str()),
        Some("idea"),
        "a fresh pool item is an `idea`: {idea_page}"
    );
    assert_eq!(
        idea_items[0].get("title").and_then(|v| v.as_str()),
        Some(POOL_ITEM_TITLE)
    );
    assert!(
        !idea_page.to_string().contains("creator_id"),
        "the pool wire shape must not leak the stored creator id: {idea_page}"
    );

    // --- 2. Promotion creates the Work, and writes no Work note ----------
    let promote = hermetic_cli(
        home.path(),
        cwd.path(),
        &[
            "creator",
            "works",
            "pool",
            "inspiration",
            "promote",
            &item_id,
        ],
    );
    assert!(
        promote.status.success(),
        "pool inspiration promote must succeed on the direct core: {}",
        combined_output(&promote)
    );

    let promoted_page = json_stdout(&hermetic_cli(
        home.path(),
        cwd.path(),
        &["creator", "works", "pool", "inspiration", "list", "--json"],
    ));
    let promoted_items = promoted_page
        .get("items")
        .and_then(|v| v.as_array())
        .expect("the pool list DTO must carry an items array");
    assert_eq!(
        promoted_items.len(),
        1,
        "promotion must not add a second pool item: {promoted_page}"
    );
    assert_eq!(
        promoted_items[0].get("status").and_then(|v| v.as_str()),
        Some("promoted"),
        "the promoted item records its own status: {promoted_page}"
    );
    let work_id = promoted_items[0]
        .get("promoted_work_id")
        .and_then(|v| v.as_str())
        .expect("the promoted item must name the Work it created")
        .to_string();
    assert!(
        work_id.starts_with("wrk_"),
        "promotion must create a real `wrk_` Work, got {work_id:?}"
    );

    let status_after_promote = hermetic_cli(
        home.path(),
        cwd.path(),
        &["creator", "works", "status", &work_id, "--json"],
    );
    assert!(
        status_after_promote.status.success(),
        "works status must read the promoted Work from the core: {}",
        combined_output(&status_after_promote)
    );
    let promoted_work = json_stdout(&status_after_promote);
    assert_eq!(
        promoted_work.get("title").and_then(|v| v.as_str()),
        Some(POOL_ITEM_TITLE),
        "the promoted Work carries the pool item's title: {promoted_work}"
    );
    assert_eq!(
        promoted_work
            .get("inspiration_log")
            .and_then(|v| v.as_array())
            .map(Vec::len),
        Some(0),
        "a pool promotion must not append to the Work's inspiration_log: {promoted_work}"
    );

    // --- 3. `works inspire` writes the Work store only -------------------
    let inspire = hermetic_cli(
        home.path(),
        cwd.path(),
        &[
            "creator", "works", "inspire", &work_id, "--note", WORK_NOTE, "--json",
        ],
    );
    assert!(
        inspire.status.success(),
        "works inspire must succeed on the direct core: {}",
        combined_output(&inspire)
    );
    let appended = json_stdout(&inspire);
    assert_eq!(
        appended.get("work_id").and_then(|v| v.as_str()),
        Some(work_id.as_str()),
        "the append DTO must name the Work it appended to: {appended}"
    );
    assert_eq!(
        appended
            .get("inspiration_count")
            .and_then(serde_json::Value::as_i64),
        Some(1),
        "the append DTO must report the Work's own note count: {appended}"
    );

    let status_after_inspire = hermetic_cli(
        home.path(),
        cwd.path(),
        &["creator", "works", "status", &work_id, "--json"],
    );
    assert!(
        status_after_inspire.status.success(),
        "works status must still read the Work: {}",
        combined_output(&status_after_inspire)
    );
    let inspired_work = json_stdout(&status_after_inspire);
    let log = inspired_work
        .get("inspiration_log")
        .and_then(|v| v.as_array())
        .expect("the Work DTO must carry its inspiration_log");
    assert_eq!(
        log.len(),
        1,
        "the Work leaf must append exactly one note: {inspired_work}"
    );
    assert_eq!(
        log[0].get("note").and_then(|v| v.as_str()),
        Some(WORK_NOTE),
        "the Work store must hold the note text: {inspired_work}"
    );

    let pool_after_inspire = json_stdout(&hermetic_cli(
        home.path(),
        cwd.path(),
        &["creator", "works", "pool", "inspiration", "list", "--json"],
    ));
    assert_eq!(
        pool_after_inspire
            .get("items")
            .and_then(|v| v.as_array())
            .map(Vec::len),
        Some(1),
        "`works inspire` must not create a pool inspiration item: {pool_after_inspire}"
    );

    let pool_entries = json_stdout(&hermetic_cli(
        home.path(),
        cwd.path(),
        &["creator", "works", "pool", "list", "--json"],
    ));
    let entries = pool_entries
        .get("entries")
        .and_then(|v| v.as_array())
        .expect("the pool list DTO must carry an entries array");
    assert!(
        entries
            .iter()
            .any(|e| e.get("work_id").and_then(|v| v.as_str()) == Some(work_id.as_str())),
        "the selection pool must hold the promoted Work's entry: {pool_entries}"
    );
    assert!(
        !pool_entries.to_string().contains("creator_id"),
        "the pool wire shape must not leak the stored creator id: {pool_entries}"
    );

    // --- 4. `works use` writes the selection, still from the core --------
    let use_output = hermetic_cli(
        home.path(),
        cwd.path(),
        &["creator", "works", "use", &work_id],
    );
    assert!(
        use_output.status.success(),
        "works use must set the active Work on the direct core: {}",
        combined_output(&use_output)
    );
    assert!(
        String::from_utf8_lossy(&use_output.stdout)
            .contains(&format!("Active Work set to {work_id}")),
        "works use must report the retained selection line: {}",
        combined_output(&use_output)
    );

    let selected = json_stdout(&hermetic_cli(
        home.path(),
        cwd.path(),
        &["creator", "works", "pool", "list", "--json"],
    ));
    assert!(
        selected
            .get("entries")
            .and_then(|v| v.as_array())
            .is_some_and(|entries| entries.iter().any(|e| {
                e.get("work_id").and_then(|v| v.as_str()) == Some(work_id.as_str())
                    && e.get("status").and_then(|v| v.as_str()) == Some("active")
            })),
        "works use must leave the selected Work as the pool `active` row: {selected}"
    );

    // The omitted `<work_id>` resolves that same pool `active` row. The
    // selection reads `novel_pool_entries` — which holds this Work as `active`
    // — and not the Work's own `works.status` column, which a promotion leaves
    // at `draft` (R-V1193-P0T5-OMITTED-ID-POOL-ACTIVE; the selector
    // discriminating regression is
    // [`omitted_work_id_uses_pool_active_entry`]). The read must stay on the
    // direct core: the retired leaf resolved the active Work with a daemon call.
    let omitted_id_status = hermetic_cli(
        home.path(),
        cwd.path(),
        &["creator", "works", "status", "--json"],
    );
    let omitted_id_output = combined_output(&omitted_id_status);
    assert!(
        omitted_id_status.status.success(),
        "the omitted <work_id> must resolve the pool `active` Work: {omitted_id_output}"
    );
    let omitted_resolved = json_stdout(&omitted_id_status);
    assert_eq!(
        omitted_resolved.get("work_id").and_then(|v| v.as_str()),
        Some(work_id.as_str()),
        "the omitted <work_id> must address the Work `works use` selected: {omitted_resolved}"
    );
    assert_eq!(
        omitted_resolved.get("status").and_then(|v| v.as_str()),
        Some("draft"),
        "the pool-active Work keeps its own draft status: {omitted_resolved}"
    );

    // --- 5. No step of the sequence consulted the configured daemon ------
    // Drain window: a connection the child opened is queued and accepted here.
    std::thread::sleep(Duration::from_millis(100));
    let observed = probes.load(Ordering::SeqCst);
    stop.store(true, Ordering::SeqCst);
    accept_loop.join().expect("join probe listener");
    assert_eq!(
        observed, 0,
        "works list/status/use, pool list/promote/inspiration and inspire must not open \
         any connection to the configured daemon URL ({daemon_url})"
    );
}

/// Title of the pool-level inspiration item this case adds.
const POOL_ITEM_TITLE: &str = "Pool-only idea";

/// Note appended to the Work's own `inspiration_log`.
const WORK_NOTE: &str = "Work-side note";

// =============================================================================
// Omitted `<work_id>` → pool `active` Work
// (v1.194 P2-T1 — R-V1193-P0T5-OMITTED-ID-POOL-ACTIVE)
// =============================================================================

/// Title of the first Work promoted by [`omitted_work_id_uses_pool_active_entry`].
const POOL_ACTIVE_ALPHA_TITLE: &str = "Alpha pool idea";

/// Title of the second promoted Work — the one the pool holds `active`.
const POOL_ACTIVE_BETA_TITLE: &str = "Beta pool idea";

/// Note the omitted-`<work_id>` `works inspire` appends.
const OMITTED_ID_NOTE: &str = "note via the pool-active Work";

/// Note the explicit-`<work_id>` `works inspire` appends.
const EXPLICIT_ID_NOTE: &str = "note via an explicit Work id";

/// Add a pool inspiration item titled `title`, promote it, and return the Work
/// the promotion created.
///
/// Both steps go through the real leaves: `pool inspiration add` (pool-store
/// write) and `pool inspiration promote` (the core's atomic Work create + pool
/// promote + item update).
fn promote_pool_idea(home: &std::path::Path, cwd: &std::path::Path, title: &str) -> String {
    let add = hermetic_cli(
        home,
        cwd,
        &["creator", "works", "pool", "inspiration", "add", title, "--json"],
    );
    assert!(
        add.status.success(),
        "pool inspiration add must succeed on the direct core: {}",
        combined_output(&add)
    );
    let item_id = json_stdout(&add)
        .get("item_id")
        .and_then(|v| v.as_str())
        .expect("the add DTO must carry the item id")
        .to_string();

    let promote = hermetic_cli(
        home,
        cwd,
        &[
            "creator",
            "works",
            "pool",
            "inspiration",
            "promote",
            &item_id,
        ],
    );
    assert!(
        promote.status.success(),
        "pool inspiration promote must succeed on the direct core: {}",
        combined_output(&promote)
    );

    let page = json_stdout(&hermetic_cli(
        home,
        cwd,
        &["creator", "works", "pool", "inspiration", "list", "--json"],
    ));
    let work_id = page
        .get("items")
        .and_then(|v| v.as_array())
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("item_id").and_then(|v| v.as_str()) == Some(item_id.as_str()))
        })
        .and_then(|item| item.get("promoted_work_id"))
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("promoted item {item_id} must name its Work: {page}"))
        .to_string();
    assert!(
        work_id.starts_with("wrk_"),
        "promotion must create a `wrk_` Work, got {work_id:?}"
    );
    work_id
}

/// The pool listing's entry for `work_id`, failing the case when it holds none.
fn pool_entry_for(pool_page: &serde_json::Value, work_id: &str) -> serde_json::Value {
    pool_page
        .get("entries")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("the pool list DTO must carry an entries array: {pool_page}"))
        .iter()
        .find(|entry| entry.get("work_id").and_then(|v| v.as_str()) == Some(work_id))
        .unwrap_or_else(|| panic!("the pool must hold an entry for {work_id}: {pool_page}"))
        .clone()
}

/// Read one Work through `works status --json` (an explicit `<work_id>`).
fn work_status_json(
    home: &std::path::Path,
    cwd: &std::path::Path,
    work_id: &str,
) -> serde_json::Value {
    let output = hermetic_cli(home, cwd, &["creator", "works", "status", work_id, "--json"]);
    assert!(
        output.status.success(),
        "works status {work_id} must succeed on the direct core: {}",
        combined_output(&output)
    );
    json_stdout(&output)
}

/// The note texts the Work's own `inspiration_log` holds, in stored order.
fn inspiration_note_texts(work: &serde_json::Value) -> Vec<String> {
    work.get("inspiration_log")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("the Work DTO must carry its inspiration_log: {work}"))
        .iter()
        .filter_map(|entry| entry.get("note").and_then(|v| v.as_str()))
        .map(str::to_string)
        .collect()
}

/// Every Work id the active creator's `works list` page holds, in page order.
fn listed_work_ids(home: &std::path::Path, cwd: &std::path::Path) -> Vec<String> {
    let page = json_stdout(&hermetic_cli(
        home,
        cwd,
        &["creator", "works", "list", "--json"],
    ));
    page.get("items")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("the works list DTO must carry an items array: {page}"))
        .iter()
        .filter_map(|item| item.get("work_id").and_then(|v| v.as_str()))
        .map(str::to_string)
        .collect()
}

/// NEW (v1.194 P2-T1, R-V1193-P0T5-OMITTED-ID-POOL-ACTIVE): an omitted
/// `<work_id>` resolves the pool `active` Work.
///
/// The selection pool (`novel_pool_entries.status = 'active'`, written by
/// `works use` and every promotion) and the Work's own `works.status` column are
/// independent domains: a promoted Work stays `draft`, so the retired
/// resolution (`works.status = 'active'`) found nothing and `works status` /
/// `works inspire` refused with "No active Work found" for a pool-active Work.
///
/// The fixture promotes two Works so a wrong selector fails on it. The second
/// promotion leaves `Alpha` queued and `Beta` active, and `Alpha`'s pool row is
/// then archived — under the pool listing's `updated_at DESC` order that makes
/// the non-active entry the first row (asserted below, so the case fails loudly
/// if that premise ever stops holding). A `works.status` selector therefore
/// resolves nothing, a "first row of the pool listing" selector resolves the
/// archived `Alpha`, and only the `status = 'active'` pool query resolves
/// `Beta`.
///
/// What it does not establish: the other omitted-`<work_id>` arms (`reopen`,
/// `reconcile-chapters`, the findings/rules leaves), pool pagination beyond the
/// two entries, and any concurrency behavior.
#[allow(clippy::too_many_lines)] // one selection-domain proof over a single local home
#[test]
fn omitted_work_id_uses_pool_active_entry() {
    let home = tempfile::tempdir().expect("temp home");
    let cwd = tempfile::tempdir().expect("temp cwd");
    seed_selected_local_creator(home.path(), cwd.path());

    // --- 1. Two promoted Works; the pool holds the second one `active` ----
    let alpha = promote_pool_idea(home.path(), cwd.path(), POOL_ACTIVE_ALPHA_TITLE);
    let beta = promote_pool_idea(home.path(), cwd.path(), POOL_ACTIVE_BETA_TITLE);
    assert_ne!(alpha, beta, "each promotion must create its own Work");

    // Promotion writes the pool row, never `works.status`: both Works stay
    // `draft`, so no Work in this fixture satisfies the retired selector.
    for work_id in [&alpha, &beta] {
        let work = work_status_json(home.path(), cwd.path(), work_id);
        assert_eq!(
            work.get("status").and_then(|v| v.as_str()),
            Some("draft"),
            "a pool-promoted Work must stay draft: {work}"
        );
    }

    let pool = json_stdout(&hermetic_cli(
        home.path(),
        cwd.path(),
        &["creator", "works", "pool", "list", "--json"],
    ));
    let alpha_entry = pool_entry_for(&pool, &alpha);
    let beta_entry = pool_entry_for(&pool, &beta);
    assert_eq!(
        alpha_entry.get("status").and_then(|v| v.as_str()),
        Some("queued"),
        "the first promoted Work is demoted to queued: {pool}"
    );
    assert_eq!(
        beta_entry.get("status").and_then(|v| v.as_str()),
        Some("active"),
        "the pool `active` row is the second promoted Work: {pool}"
    );
    let alpha_entry_id = alpha_entry
        .get("entry_id")
        .and_then(|v| v.as_str())
        .expect("the pool entry carries its id")
        .to_string();
    let beta_entry_id = beta_entry
        .get("entry_id")
        .and_then(|v| v.as_str())
        .expect("the pool entry carries its id")
        .to_string();

    // Archive the *queued* Work's pool row: the archived, non-active entry
    // becomes the pool listing's newest row, which is what discriminates the
    // `status = 'active'` filter from a first-row read.
    let archive_queued = hermetic_cli(
        home.path(),
        cwd.path(),
        &["creator", "works", "pool", "archive", &alpha_entry_id],
    );
    assert!(
        archive_queued.status.success(),
        "pool archive must succeed on the direct core: {}",
        combined_output(&archive_queued)
    );

    let pool_after_archive = json_stdout(&hermetic_cli(
        home.path(),
        cwd.path(),
        &["creator", "works", "pool", "list", "--json"],
    ));
    let entries_after_archive = pool_after_archive
        .get("entries")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| {
            panic!("the pool list DTO must carry an entries array: {pool_after_archive}")
        });
    assert_eq!(
        entries_after_archive.len(),
        2,
        "both pool entries stay listed: {pool_after_archive}"
    );
    assert_eq!(
        entries_after_archive[0].get("entry_id").and_then(|v| v.as_str()),
        Some(alpha_entry_id.as_str()),
        "fixture premise: the archived, non-active entry must be the pool listing's first row \
         (`ORDER BY updated_at DESC`): {pool_after_archive}"
    );
    assert_eq!(
        entries_after_archive[0].get("status").and_then(|v| v.as_str()),
        Some("archived"),
        "the first pool row is the archived entry: {pool_after_archive}"
    );

    // --- 2. Omitted `works status` resolves the pool-active Work ----------
    let omitted_status = hermetic_cli(
        home.path(),
        cwd.path(),
        &["creator", "works", "status", "--json"],
    );
    assert!(
        omitted_status.status.success(),
        "an omitted <work_id> must resolve the pool-active Work: {}",
        combined_output(&omitted_status)
    );
    let resolved = json_stdout(&omitted_status);
    assert_eq!(
        resolved.get("work_id").and_then(|v| v.as_str()),
        Some(beta.as_str()),
        "the omitted <work_id> must resolve the pool `active` entry's Work: {resolved}"
    );
    assert_eq!(
        resolved.get("title").and_then(|v| v.as_str()),
        Some(POOL_ACTIVE_BETA_TITLE),
        "the resolved Work is the pool-active one: {resolved}"
    );
    assert_eq!(
        resolved.get("status").and_then(|v| v.as_str()),
        Some("draft"),
        "the resolved pool Work keeps its own draft status: {resolved}"
    );

    // --- 3. Omitted `works inspire` writes the same Work -----------------
    let omitted_inspire = hermetic_cli(
        home.path(),
        cwd.path(),
        &[
            "creator",
            "works",
            "inspire",
            "--note",
            OMITTED_ID_NOTE,
            "--json",
        ],
    );
    assert!(
        omitted_inspire.status.success(),
        "an omitted <work_id> must resolve the pool-active Work: {}",
        combined_output(&omitted_inspire)
    );
    let appended = json_stdout(&omitted_inspire);
    assert_eq!(
        appended.get("work_id").and_then(|v| v.as_str()),
        Some(beta.as_str()),
        "the omitted <work_id> append must address the pool-active Work: {appended}"
    );
    assert_eq!(
        appended
            .get("inspiration_count")
            .and_then(serde_json::Value::as_i64),
        Some(1),
        "the append DTO must report the Work's own note count: {appended}"
    );
    assert_eq!(
        inspiration_note_texts(&work_status_json(home.path(), cwd.path(), &beta)),
        vec![OMITTED_ID_NOTE.to_string()],
        "the pool-active Work must hold the omitted-id note"
    );
    assert!(
        inspiration_note_texts(&work_status_json(home.path(), cwd.path(), &alpha)).is_empty(),
        "the omitted <work_id> must not append to the other Work"
    );

    // --- 4. An explicit `<work_id>` still wins over the pool default -----
    let explicit = work_status_json(home.path(), cwd.path(), &alpha);
    assert_eq!(
        explicit.get("work_id").and_then(|v| v.as_str()),
        Some(alpha.as_str()),
        "an explicit <work_id> must address that Work, not the pool default: {explicit}"
    );
    assert_eq!(
        explicit.get("title").and_then(|v| v.as_str()),
        Some(POOL_ACTIVE_ALPHA_TITLE),
        "the explicit <work_id> must win over the pool default: {explicit}"
    );

    let explicit_inspire = hermetic_cli(
        home.path(),
        cwd.path(),
        &[
            "creator",
            "works",
            "inspire",
            &alpha,
            "--note",
            EXPLICIT_ID_NOTE,
            "--json",
        ],
    );
    assert!(
        explicit_inspire.status.success(),
        "works inspire with an explicit <work_id> must succeed: {}",
        combined_output(&explicit_inspire)
    );
    assert_eq!(
        json_stdout(&explicit_inspire)
            .get("work_id")
            .and_then(|v| v.as_str()),
        Some(alpha.as_str()),
        "an explicit <work_id> must divert the append from the pool default"
    );
    assert_eq!(
        inspiration_note_texts(&work_status_json(home.path(), cwd.path(), &alpha)),
        vec![EXPLICIT_ID_NOTE.to_string()],
        "the explicit Work holds only its own note"
    );
    assert_eq!(
        inspiration_note_texts(&work_status_json(home.path(), cwd.path(), &beta)),
        vec![OMITTED_ID_NOTE.to_string()],
        "an explicit <work_id> must not append to the pool-active Work"
    );

    // --- 5. No pool `active` entry: refuse, and write no Work -------------
    let works_before = listed_work_ids(home.path(), cwd.path());
    assert_eq!(
        works_before.len(),
        2,
        "the fixture holds the two promoted Works: {works_before:?}"
    );

    let archive_active = hermetic_cli(
        home.path(),
        cwd.path(),
        &["creator", "works", "pool", "archive", &beta_entry_id],
    );
    assert!(
        archive_active.status.success(),
        "archiving the pool `active` entry must succeed: {}",
        combined_output(&archive_active)
    );

    for args in [
        vec!["creator", "works", "status", "--json"],
        vec![
            "creator",
            "works",
            "inspire",
            "--note",
            OMITTED_ID_NOTE,
            "--json",
        ],
    ] {
        let refused = hermetic_cli(home.path(), cwd.path(), &args);
        let output = combined_output(&refused);
        assert!(
            !refused.status.success(),
            "with no pool `active` entry the omitted <work_id> must refuse: {output}"
        );
        assert!(
            output.contains("No active Work found"),
            "the refusal stays the retained selection text: {output}"
        );
    }
    assert_eq!(
        listed_work_ids(home.path(), cwd.path()),
        works_before,
        "a refused omitted <work_id> must not write another Work"
    );
}
