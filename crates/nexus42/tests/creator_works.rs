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
//! Run with: cargo test -p nexus42 --test creator_works

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

/// `creator works use` requires a work_id positional argument.
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

/// `creator works completion-lock release` requires a work_id argument.
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
