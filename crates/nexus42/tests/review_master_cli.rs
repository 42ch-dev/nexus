//! Hermetic CLI surface tests for `creator run review-master` (V1.44 P1).
//!
//! Plan: `2026-06-13-v1.44-review-master-cli-surface`
//!
//! These tests verify the CLI subcommand surface, help text, and flag
//! documentation without requiring a running daemon. Integration tests
//! for the daemon handler are in `nexus-daemon-runtime/tests/`.
//!
//! Run with: cargo test -p nexus42 --test review_master_cli

use assert_cmd::Command;

// =============================================================================
// T1: CLI subcommand + help text
// =============================================================================

/// AC1: `creator run review-master --help` documents flags from spec §3.4.
#[test]
fn review_master_help_shows_expected_flags() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "run", "review-master", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    // Required argument
    assert!(
        help_text.contains("<WORK_ID>"),
        "review-master --help must show WORK_ID argument"
    );

    // Flags from spec §3.4
    assert!(
        help_text.contains("--finding-id"),
        "review-master --help must list --finding-id flag"
    );
    assert!(
        help_text.contains("--auto-schedule"),
        "review-master --help must list --auto-schedule flag"
    );
    assert!(
        help_text.contains("--json"),
        "review-master --help must list --json flag"
    );

    // Help text must mention the preset name
    assert!(
        help_text.contains("novel-review-master"),
        "review-master --help must mention novel-review-master preset"
    );

    // Help text must distinguish from stage advance
    assert!(
        help_text.contains("reflection-loop") || help_text.contains("FL-E"),
        "review-master --help must distinguish from reflection-loop FL-E review stage"
    );

    // Help text must reference quickstart
    assert!(
        help_text.contains("novel-writing-quickstart.md"),
        "review-master --help must reference quickstart"
    );
}

/// AC1: `creator run --help` lists `review-master` as a subcommand.
#[test]
fn creator_run_help_lists_review_master() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "run", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    assert!(
        help_text.contains("review-master"),
        "creator run --help must list review-master subcommand"
    );
}

// =============================================================================
// T3: Status presentation (open master findings summary)
// =============================================================================

/// Verify that the review-master command does NOT create an auto_chain schedule
/// (it must not fork or cancel the FL-E auto-chain driver).
///
/// This is a surface-level test: the command's help text and implementation
/// must not reference auto-chain creation. The actual behavior is tested
/// in daemon integration tests.
#[test]
fn review_master_help_does_not_mention_auto_chain() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "run", "review-master", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    // The command must not mention auto-chain or FL-E driver
    assert!(
        !help_text.contains("auto-chain"),
        "review-master --help must NOT mention auto-chain (it does not fork FL-E driver)"
    );
    assert!(
        !help_text.contains("auto_chain"),
        "review-master --help must NOT mention auto_chain (it does not fork FL-E driver)"
    );
}

// =============================================================================
// T4: Quickstart §5 convergence
// =============================================================================

/// Verify that the command help references the quickstart for discoverability.
#[test]
fn review_master_help_references_quickstart_section_5() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "run", "review-master", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    assert!(
        help_text.contains("novel-writing-quickstart.md"),
        "review-master --help must reference novel-writing-quickstart.md"
    );
}

// =============================================================================
// T5: Spec convergence — command does not reference reflection-loop as primary
// =============================================================================

/// Verify that the command help distinguishes itself from `stage advance --stage review`.
/// The `review-master` command runs `novel-review-master`, not `reflection-loop`.
#[test]
fn review_master_help_distinguishes_from_stage_advance() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "run", "review-master", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    // Must mention the correct preset
    assert!(
        help_text.contains("novel-review-master"),
        "review-master --help must mention novel-review-master preset"
    );

    // Must NOT suggest that it's the same as stage advance
    // (stage advance --stage review runs reflection-loop, not novel-review-master)
    assert!(
        !help_text.contains("same as stage advance"),
        "review-master --help must NOT claim equivalence with stage advance"
    );
}
