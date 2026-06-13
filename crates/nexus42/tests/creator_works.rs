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
