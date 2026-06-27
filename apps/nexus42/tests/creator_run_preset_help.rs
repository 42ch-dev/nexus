//! V1.46 P2 (Grill #20, #21) — End-to-end smoke tests for the dynamic
//! `creator run <preset_id> --help` `cli_args` injection.
//!
//! These tests invoke the real `nexus42` binary (via `assert_cmd`) to verify
//! the intercept wired into `main()` actually fires and surfaces manifest-
//! declared `cli_args` for the first-slice presets. They complement the pure
//! unit tests on `extract_run_help_target` / `format_preset_run_help`
//! (in `run::tests`) by proving the full path: argv → intercept → manifest
//! load → enriched help → exit(0).
//!
//! Plan: `2026-06-14-v1.46-novel-runtime-ux-edges` (T3).

use assert_cmd::Command;

/// Helper: run `nexus42 creator run <preset> --help` and return stdout as a
/// String, asserting success. Uses the `.clone()` owned-bytes pattern (see
/// `command_surface_contract.rs`) to avoid temporary-lifetime issues.
fn run_preset_help_stdout(preset: &str, help_flag: &str) -> String {
    let stdout = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "run", preset, help_flag])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8_lossy(&stdout).into_owned()
}

/// The enriched help block always emits this exact header line.
const PRESET_ARGS_HEADER: &str = "Preset-specific args (captured verbatim after positional args):";

#[test]
fn novel_review_master_help_lists_manifest_cli_args() {
    let help = run_preset_help_stdout("novel-review-master", "--help");
    // Intercept fired (not clap's generic help).
    assert!(
        help.contains(PRESET_ARGS_HEADER),
        "enriched help header should appear; got:\n{help}"
    );
    // Manifest-declared flags surface.
    assert!(
        help.contains("--finding-id"),
        "--finding-id must appear in help:\n{help}"
    );
    assert!(
        help.contains("--auto-schedule"),
        "--auto-schedule must appear in help:\n{help}"
    );
    // Preset id and description present.
    assert!(
        help.contains("novel-review-master"),
        "help must reference the preset id:\n{help}"
    );
}

#[test]
fn novel_manuscript_audit_review_help_lists_manifest_cli_args() {
    let help = run_preset_help_stdout("novel-manuscript-audit-review", "--help");
    assert!(
        help.contains(PRESET_ARGS_HEADER),
        "enriched help header should appear; got:\n{help}"
    );
    assert!(
        help.contains("--chapter"),
        "--chapter must appear in help:\n{help}"
    );
    assert!(
        help.contains("--volume"),
        "--volume must appear in help:\n{help}"
    );
    // Required marker for chapter.
    assert!(
        help.contains("required"),
        "required annotation must appear:\n{help}"
    );
}

#[test]
fn novel_manuscript_audit_extract_help_lists_manifest_cli_args() {
    let help = run_preset_help_stdout("novel-manuscript-audit-extract", "--help");
    assert!(
        help.contains(PRESET_ARGS_HEADER),
        "enriched help header should appear; got:\n{help}"
    );
    assert!(
        help.contains("--chapter"),
        "--chapter must appear in help:\n{help}"
    );
}

#[test]
fn preset_without_cli_args_falls_through_to_clap_generic_help() {
    // `novel-writing` has no `cli_args` in its manifest → the intercept must
    // NOT fire; clap's generic RunCommand help renders instead. Discriminator:
    // the enriched block's exact header line is absent.
    let stdout = run_preset_help_stdout("novel-writing", "--help");
    assert!(
        !stdout.contains(PRESET_ARGS_HEADER),
        "no-cli_args preset must fall through to clap generic help (no enriched header); got:\n{stdout}"
    );
    // clap's generic help should still mention the PRESET_ID positional.
    assert!(
        stdout.contains("PRESET_ID"),
        "clap generic help should mention PRESET_ID:\n{stdout}"
    );
}

#[test]
fn short_h_flag_also_triggers_enriched_help() {
    // `-h` (short form) must trigger the same intercept as `--help`.
    let stdout = run_preset_help_stdout("novel-review-master", "-h");
    assert!(
        stdout.contains("--finding-id"),
        "-h must trigger the enriched help with --finding-id:\n{stdout}"
    );
}
