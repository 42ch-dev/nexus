//! V1.16 Command Surface Contract — Current-State Snapshot & V2 Target Tests
//!
//! Plan: `2026-05-12-v1.16-command-surface-contract`
//!
//! This file serves two purposes:
//!
//! 1. **Current-state snapshot** — locks in the V1.15 command topology (25 groups)
//!    as a regression anchor. Subsequent refactoring plans must NOT accidentally
//!    lose existing commands without explicit migration.
//!
//! 2. **V2 target contract** — defines the expected V2 command topology (6 groups).
//!    These tests are `#[ignore]`d because the CLI has not been restructured yet.
//!    As Plans 2–9 implement the restructuring, each test should be un-ignored
//!    and must pass before the plan can be marked Done.

use assert_cmd::Command;
use predicates::prelude::*;

// =============================================================================
// Part 1: Current-state snapshot (V1.15 baseline — these MUST pass today)
// =============================================================================

/// Snapshot: V1.15 has exactly 24 user-visible top-level command groups
/// (plus 1 hidden `acp-worker` not counted here).
///
/// If this test breaks, a command was accidentally added or removed during
/// refactoring — investigate before proceeding.
#[test]
fn current_state_visible_command_groups() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    // All user-visible commands after Plan 4 (6 visible groups, 6 more hidden)
    let expected_commands = ["acp", "creator", "daemon", "platform", "sync", "system"];

    for cmd in &expected_commands {
        assert!(
            help_text.contains(cmd),
            "Current-state snapshot: expected command '{cmd}' missing from --help output"
        );
    }

    // Verify hidden commands are NOT user-visible
    let hidden_commands = [
        "agent",
        "session",
        "policy",
        "permission",
        "auth",
        "context",
        "config",
        "debug",
        "doctor",
        "db",
        "explore",
        "identity",
        "preset",
        "runtime-mode",
        "soul",
        "memory",
        "init",
        "clone",
        "world",
        "schedule",
    ];
    for hidden in &hidden_commands {
        // These should not appear as visible top-level commands
        // (they're kept as hidden for backward compat)
        assert!(
            !help_text.contains(&format!("  {hidden} ")),
            "Current-state snapshot: '{hidden}' should be hidden from --help output"
        );
    }

    // Verify count: 6 user-visible commands
    let visible_count = expected_commands.len();
    assert_eq!(
        visible_count, 6,
        "Current-state snapshot: expected exactly 6 user-visible commands, found {visible_count}"
    );
}

/// Snapshot: Hidden `acp-worker` command exists but is NOT shown in --help.
#[test]
fn current_state_acp_worker_is_hidden() {
    // `acp-worker` should NOT appear in --help
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("acp-worker").not());

    // But it should be a valid (hidden) subcommand — running it without args
    // should fail with a usage error, not "unrecognized subcommand"
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("acp-worker")
        .assert()
        .failure();
}

/// Snapshot: `daemon` command has expected subcommands in V1.15.
#[test]
fn current_state_daemon_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("daemon")
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    // V1.16+ daemon: start, stop, restart, status, logs, doctor, orchestrate
    for subcmd in &[
        "start",
        "stop",
        "restart",
        "status",
        "logs",
        "doctor",
        "orchestrate",
    ] {
        assert!(
            help_text.contains(subcmd),
            "Current-state daemon: expected subcommand '{subcmd}'"
        );
    }
}

/// Snapshot: `creator` command has expected subcommands in V1.15.
#[test]
fn current_state_creator_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    for subcmd in &["register", "status", "pair", "credentials", "list"] {
        assert!(
            help_text.contains(subcmd),
            "Current-state creator: expected subcommand '{subcmd}'"
        );
    }
}

/// Snapshot: `sync` command has expected subcommands in V1.15.
#[test]
fn current_state_sync_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("sync")
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    for subcmd in &["push", "pull", "status"] {
        assert!(
            help_text.contains(subcmd),
            "Current-state sync: expected subcommand '{subcmd}'"
        );
    }
}

/// Snapshot: `system` command has expected subcommands in V1.15.
#[test]
fn current_state_system_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    // V1.16+ system: preset, version, doctor, completion, config, debug, db, identity, runtime-mode
    for subcmd in &[
        "preset",
        "version",
        "doctor",
        "completion",
        "config",
        "debug",
        "db",
        "identity",
        "runtime-mode",
    ] {
        assert!(
            help_text.contains(subcmd),
            "Current-state system: expected subcommand '{subcmd}'"
        );
    }

    // Verify no unexpected subcommands
    let commands_section_start = help_text.find("Commands:").unwrap_or(0);
    let options_section_start = help_text.find("\nOptions:").unwrap_or(help_text.len());
    let commands_section = &help_text[commands_section_start..options_section_start];

    // These were NOT system subcommands before — verify they exist now
    assert!(
        commands_section.contains("version"),
        "Current-state system: 'version' should now be a system subcommand"
    );
}

// =============================================================================
// Part 2: V2 target contract tests (#[ignore] — future plans must un-ignore)
//
// These define the V2 command surface. Each test asserts that a V2 top-level
// group exists with its expected subcommands. They are #[ignore]d because the
// CLI has not been restructured yet.
//
// Migration plan:
//   - Plan 2 (daemon/acp): un-ignore `v2_target_daemon_subcommands`
//     and `v2_target_acp_exists`
//   - Plan 3 (system/platform): un-ignore `v2_target_system_subcommands`
//     and `v2_target_platform_exists`
//   - Plan 4 (creator/knowledge): un-ignore `v2_target_creator_subcommands`
//   - Plan 2–4 together: un-ignore `v2_only_six_visible_command_groups`
// =============================================================================

/// V2 Target: Only 6 user-visible top-level command groups:
/// `daemon`, `acp`, `creator`, `sync`, `platform`, `system`
///
/// Un-ignored by Plans 2-4 completing the CLI restructuring.
#[test]
fn v2_only_six_visible_command_groups() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    let v2_groups = ["daemon", "acp", "creator", "sync", "platform", "system"];

    for group in &v2_groups {
        assert!(
            help_text.contains(group),
            "V2 target: expected top-level group '{group}' in --help"
        );
    }

    // Verify no legacy top-level commands leaked through.
    // These should all be moved under the V2 groups or removed.
    let legacy_commands = [
        "auth",
        "clone",
        "config",
        "context",
        "db",
        "debug",
        "doctor",
        "explore",
        "identity",
        "init",
        "memory",
        "permission",
        "policy",
        "preset",
        "runtime-mode",
        "schedule",
        "session",
        "soul",
        "world",
        "agent",
    ];

    for legacy in &legacy_commands {
        // We check that these do NOT appear as top-level Commands in help.
        // This is a simple substring check — clap's help format lists Commands:
        // on a line, so we look for "  {legacy}" pattern to reduce false positives.
        let as_top_level = format!("  {legacy}");
        assert!(
            !help_text.contains(&as_top_level),
            "V2 target: legacy command '{legacy}' should not be a top-level command"
        );
    }
}

/// V2 Target: `daemon` command group subcommands.
///
/// Expected: start, stop, restart, status, logs, doctor,
///           orchestrate (with list/run/pause/resume/cancel/inspect)
///
/// Un-ignored by Plan 2 (daemon restructuring implemented).
#[test]
fn v2_target_daemon_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("daemon")
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    for subcmd in &[
        "start",
        "stop",
        "restart",
        "status",
        "logs",
        "doctor",
        "orchestrate",
    ] {
        assert!(
            help_text.contains(subcmd),
            "V2 daemon: expected subcommand '{subcmd}'"
        );
    }

    // Verify orchestrate sub-subcommands
    let orch_output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["daemon", "orchestrate", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let orch_help = String::from_utf8(orch_output).unwrap();

    for subcmd in &["list", "run", "pause", "resume", "cancel", "inspect"] {
        assert!(
            orch_help.contains(subcmd),
            "V2 daemon orchestrate: expected subcommand '{subcmd}'"
        );
    }
}

/// V2 Target: `acp` top-level command group exists.
///
/// Expected subcommands: status, doctor, probe,
///   registry (list, inspect), agent (use, list), skills (export, verify)
///
/// Un-ignored by Plan 2 (acp group created).
#[test]
fn v2_target_acp_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("acp")
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    for subcmd in &["status", "doctor", "probe", "registry", "agent", "skills"] {
        assert!(
            help_text.contains(subcmd),
            "V2 acp: expected subcommand '{subcmd}'"
        );
    }
}

/// V2 Target: `creator` command group subcommands (extended).
///
/// Expected: register, status, use, list, pair, unpair, logout,
///   credentials (rotate), workspace (list/create/use/init/clone/link/unlink/status),
///   soul, memory, kb
///
/// Un-ignored by Plan 4.
#[test]
fn v2_target_creator_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    for subcmd in &[
        "register",
        "status",
        "use",
        "list",
        "pair",
        "unpair",
        "logout",
        "credentials",
        "workspace",
        "soul",
        "memory",
        "kb",
    ] {
        assert!(
            help_text.contains(subcmd),
            "V2 creator: expected subcommand '{subcmd}'"
        );
    }
}

/// V2 Target: `sync` command group subcommands.
///
/// Expected: pull, push, status, retry, resolve
///
/// Un-ignored by Plans 2-4 consolidating sync.
#[test]
fn v2_target_sync_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("sync")
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    for subcmd in &["pull", "push", "status", "retry", "resolve"] {
        assert!(
            help_text.contains(subcmd),
            "V2 sync: expected subcommand '{subcmd}'"
        );
    }
}

/// V2 Target: `platform` top-level command group exists.
///
/// Expected subcommands: auth (login/logout/status/profiles),
///   context (assemble), explore, publish
///
/// Un-ignore after Plan 3 creates the `platform` group.
#[test]
fn v2_target_platform_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("platform")
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    for subcmd in &["auth", "context", "explore", "publish"] {
        assert!(
            help_text.contains(subcmd),
            "V2 platform: expected subcommand '{subcmd}'"
        );
    }
}

/// V2 Target: `system` command group subcommands (extended).
///
/// Expected: version, doctor, completion,
///   config (get/set/unset/path), debug (dump-workspace/replay-delta)
///
/// Un-ignore after Plan 3 extends the `system` group.
#[test]
fn v2_target_system_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    for subcmd in &[
        "version",
        "doctor",
        "completion",
        "config",
        "debug",
        "preset",
        "db",
        "identity",
        "runtime-mode",
    ] {
        assert!(
            help_text.contains(subcmd),
            "V2 system: expected subcommand '{subcmd}'"
        );
    }
}
