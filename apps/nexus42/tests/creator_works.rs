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
// `creator workspace` init/list/use + `creator demo-seed` — direct local path
// (v1.193 P0-T2)
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

/// NEW (v1.193 P0-T2): workspace initialization, selection and the demo seed
/// are local core/filesystem work — no daemon is consulted.
///
/// Defends the removed daemon-first branch: `creator workspace init` must
/// materialize the ADR-014 layout and commit the selection locally, the
/// retained `creator demo-seed` must stay real and idempotent without
/// `force`), the committed selection must stay usable, and the removed
/// workspace leaves must be unknown. The fixture points `daemon_url` at a port
/// nothing listens on, so a reintroduced health probe would print its
/// "falling back" warning on stderr and fail the `daemon` assertions.
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
    assert!(
        !select_creator_stderr.contains("daemon"),
        "creator use must not consult a daemon: {select_creator_stderr}"
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
    assert!(
        !init_stderr.contains("daemon"),
        "workspace init must not consult a daemon: {init_stderr}"
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
    assert!(
        !list_stderr.contains("daemon"),
        "workspace list must not consult a daemon: {list_stderr}"
    );

    let reselect = hermetic_cli(
        home.path(),
        cwd.path(),
        &["creator", "workspace", "use", "default"],
    );
    let reselect_stderr = String::from_utf8_lossy(&reselect.stderr);
    assert!(
        reselect.status.success(),
        "re-selecting the initialized workspace must succeed: {reselect_stderr}"
    );
    assert!(
        !reselect_stderr.contains("daemon"),
        "workspace use must not consult a daemon: {reselect_stderr}"
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
    assert!(
        !seed_stderr.contains("daemon"),
        "demo-seed must not consult a daemon: {seed_stderr}"
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

    // --- 5. The removed workspace leaves are unknown ---
    for leaf in ["clone", "link", "unlink", "status"] {
        let output = hermetic_cli(home.path(), cwd.path(), &["creator", "workspace", leaf]);
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
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
