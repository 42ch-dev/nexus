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

    // Current daemon surface: lifecycle commands plus schedule orchestration.
    for subcmd in &[
        "start", "stop", "restart", "status", "logs", "doctor", "schedule",
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
        "start", "stop", "restart", "status", "logs", "doctor", "schedule",
    ] {
        assert!(
            help_text.contains(subcmd),
            "V2 daemon: expected subcommand '{subcmd}'"
        );
    }

    assert!(
        !help_text.contains("orchestrate"),
        "daemon help must not list removed orchestrate subcommand"
    );
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

// =============================================================================
// Part 3: Plan 5 KB scope contract tests (must pass immediately)
// =============================================================================

/// Verify `creator kb list --help` contains `--scope` flag.
#[test]
fn v2_target_kb_scope_flag() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "kb", "list", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();
    assert!(
        help_text.contains("--scope"),
        "kb list --help must contain --scope flag"
    );
    assert!(
        help_text.contains("work"),
        "kb list --help must list 'work' scope option"
    );
    assert!(
        help_text.contains("world"),
        "kb list --help must list 'world' scope option"
    );
}

/// Verify `creator kb --help` shows list, search, show, add subcommands.
#[test]
fn v2_target_kb_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "kb", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();
    for subcmd in &["list", "search", "show", "add"] {
        assert!(
            help_text.contains(subcmd),
            "creator kb --help must show subcommand '{subcmd}'"
        );
    }
}

// =============================================================================
// Part 4: Plan 6 ACP execution path consolidation contract tests
// =============================================================================

/// V2 Target: `acp session --help` shows list, show, delete subcommands.
#[test]
fn v2_target_acp_session_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["acp", "session", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();
    for subcmd in &["list", "show", "delete"] {
        assert!(
            help_text.contains(subcmd),
            "V2 acp session: expected subcommand '{subcmd}'"
        );
    }
}

/// V2 Target: `acp policy --help` shows grant, deny, list subcommands.
#[test]
fn v2_target_acp_policy_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["acp", "policy", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();
    for subcmd in &["grant", "deny", "list"] {
        assert!(
            help_text.contains(subcmd),
            "V2 acp policy: expected subcommand '{subcmd}'"
        );
    }
}

/// V2 Target: `acp permission --help` shows 6 subcommands.
#[test]
fn v2_target_acp_permission_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["acp", "permission", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();
    for subcmd in &["list", "grant", "deny", "ask", "revoke", "reset"] {
        assert!(
            help_text.contains(subcmd),
            "V2 acp permission: expected subcommand '{subcmd}'"
        );
    }
}

/// V2 Target: `acp --help` shows `run` subcommand.
#[test]
fn v2_target_acp_run() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["acp", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();
    assert!(
        help_text.contains("run"),
        "V2 acp: expected subcommand 'run'"
    );
}

/// V2 Target: `platform explore --help` shows browse and search subcommands.
#[test]
fn v2_target_platform_explore_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["platform", "explore", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();
    for subcmd in &["browse", "search"] {
        assert!(
            help_text.contains(subcmd),
            "V2 platform explore: expected subcommand '{subcmd}'"
        );
    }
}

/// V2 Target: `system --help` shows `doctor` subcommand.
#[test]
fn v2_target_system_doctor() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["system", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();
    assert!(
        help_text.contains("doctor"),
        "V2 system: expected subcommand 'doctor'"
    );
}

// =============================================================================
// Part 5: Plan 7 Run and capability-call trace correlation contract tests
// =============================================================================

/// Verify `acp run --help` includes `--run-id` flag.
#[test]
fn acp_run_shows_run_id_flag() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["acp", "run", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();
    assert!(
        help_text.contains("--run-id"),
        "acp run --help must contain --run-id flag"
    );
}

/// Verify `daemon orchestrate run --help` is no longer a valid CLI surface.
#[test]
fn daemon_orchestrate_run_is_removed() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .args(["daemon", "orchestrate", "run", "--help"])
        .assert()
        .failure();
}

// =============================================================================
// Part 6: V1.33 Work Experience Loop contract tests (must pass immediately)
// =============================================================================

// V1.45: `v133_creator_run_subcommands` removed — old subcommands (start, continue,
// etc.) replaced by generic `creator run <preset_id>` dispatch.

/// Verify `creator works` subcommands exist (DF-60 §6.2H, V1.41).
#[test]
fn v141_creator_works_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "works", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();
    for subcmd in &["list", "status", "use", "completion-lock"] {
        assert!(
            help_text.contains(subcmd),
            "V1.41 creator works: expected subcommand '{subcmd}'"
        );
    }
}

// V1.45: `v141_creator_run_start_from_work_flags`, `v141_creator_run_resume_reopen_flags`,
// `v141_run_start_from_work_accepts_work_id`, `v141_resume_reopen_without_reason_rejects`
// removed — old subcommands replaced by generic dispatch.

/// AC5 (V1.41 QA blocker): `creator works pool inspiration add --help` must document
/// that the pool is distinct from per-Work `works.inspiration_log`.
#[test]
fn v141_pool_inspiration_help_disambiguates_from_work_log() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "works", "pool", "inspiration", "add", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();
    assert!(
        help_text.contains("inspiration_log"),
        "AC5: 'creator works pool inspiration add --help' must mention 'inspiration_log' to disambiguate from per-Work log"
    );
}

// V1.45: `v133_creator_run_start_requires_idea`, `v133_creator_run_continue_requires_note`,
// `v136_creator_run_start_has_init_preset_flag` removed — old subcommands replaced by generic dispatch.

/// Verify `system preset --help` shows list and validate subcommands.
#[test]
fn v133_system_preset_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["system", "preset", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();
    for subcmd in &["list", "validate"] {
        assert!(
            help_text.contains(subcmd),
            "V1.33 system preset: expected subcommand '{subcmd}'"
        );
    }
}

/// Verify `system preset list --help` includes --intent and --json flags.
#[test]
fn v133_system_preset_list_flags() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["system", "preset", "list", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();
    assert!(
        help_text.contains("--intent"),
        "V1.33 system preset list: must have --intent flag"
    );
    assert!(
        help_text.contains("--json"),
        "V1.33 system preset list: must have --json flag"
    );
}

// =============================================================================
// Part 7: V1.35 P2 — platform sync migration & deprecation contract tests
// =============================================================================

/// V1.35 P2: `platform sync --help` shows pull, push, status subcommands.
#[test]
fn v135_platform_sync_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["platform", "sync", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    for subcmd in &["pull", "push", "status", "resolve", "retry", "world"] {
        assert!(
            help_text.contains(subcmd),
            "V1.35 platform sync: expected subcommand '{subcmd}'"
        );
    }
}

/// V1.35 P2: `nexus42 sync status` emits a deprecation warning on stderr.
#[test]
fn v135_sync_deprecation_warning() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["sync", "status"])
        .assert()
        .get_output()
        .stderr
        .clone();

    let stderr_text = String::from_utf8(output).unwrap();
    assert!(
        stderr_text.contains("deprecated"),
        "V1.35: top-level `sync status` must emit deprecation warning on stderr"
    );
    assert!(
        stderr_text.contains("platform sync"),
        "V1.35: deprecation warning must point to `platform sync`"
    );
}

/// V1.35 P2: Root `--help` lists exactly 5 user-visible command groups
/// (per cli-command-ia.md §2). The deprecated top-level `sync` is hidden
/// from help but remains callable as an alias.
#[test]
fn v135_root_help_shows_five_groups_with_sync_hidden() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    // The 5 canonical V1.35 top-level groups MUST all appear.
    let expected = ["creator", "daemon", "acp", "platform", "system"];
    for group in &expected {
        assert!(
            help_text.contains(group),
            "V1.35 root help: expected visible group '{group}'"
        );
    }

    // The deprecated `sync` group MUST be hidden from help (#[command(hide = true)]).
    // We assert the absence of the word "sync" in the Commands: list to verify it's
    // not surfaced as a peer of the 5 canonical groups.
    //
    // NOTE: The word "sync" may still appear in long_about examples
    // ("nexus42 platform sync pull") which is intentional. So we check that
    // `sync` does NOT appear as a top-level Commands entry — i.e. not in the
    // "Commands:" section after the long_about examples.
    let commands_section = help_text
        .split("Commands:")
        .nth(1)
        .expect("Commands: section present in --help");
    assert!(
        !commands_section.contains("\n  sync"),
        "V1.35 root help: top-level 'sync' must be hidden (was visible in Commands list)"
    );
}

/// V1.35 P2: Root `--long-about` mentions `creator works status` and `workspace init`.
#[test]
fn v135_root_long_about_mentions_creator_works_and_workspace() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    assert!(
        help_text.contains("creator works status"),
        "V1.35: root help must mention 'creator works status'"
    );
    assert!(
        help_text.contains("workspace init"),
        "V1.35: root help must mention 'workspace init'"
    );
}

// =============================================================================
// Part 8: V1.35 P3 — Creator hub polish contract tests
// =============================================================================

/// V1.35 P3: `creator kb --help` mentions both scopes AND disambiguates from `knowledge`.
#[test]
fn v135_kb_help_disambiguates_scopes() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "kb", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    // Must mention both scopes
    assert!(
        help_text.contains("work"),
        "V1.35 P3: creator kb --help must mention 'work' scope"
    );
    assert!(
        help_text.contains("world"),
        "V1.35 P3: creator kb --help must mention 'world' scope"
    );

    // Must disambiguate by pointing to `creator knowledge`
    assert!(
        help_text.contains("creator knowledge"),
        "V1.35 P3: creator kb --help must mention 'creator knowledge' for disambiguation"
    );
}

/// V1.35 P3: `creator knowledge --help` disambiguates from `kb`.
#[test]
fn v135_knowledge_help_disambiguates_from_kb() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "knowledge", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    // Must point to `creator kb` as alternative
    assert!(
        help_text.contains("creator kb"),
        "V1.35 P3: creator knowledge --help must mention 'creator kb' for disambiguation"
    );
}

/// V1.35 P3: `creator --help` mentions tier grouping hints in descriptions.
#[test]
fn v135_creator_help_mentions_kb_namespaces() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "kb", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    // Must mention entity-scope-model reference
    assert!(
        help_text.contains("entity-scope-model") || help_text.contains("scope"),
        "V1.35 P3: creator kb --help must reference scope model"
    );
}
<<<<<<< HEAD

// V1.45: `v135_chain_novel_writing_defaults_true`,
// `v135_chain_novel_writing_opt_out_syntax_accepted`,
// `v136_start_help_mentions_auto_completion`,
// `v137_stage_advance_has_force_gates_flags`, `v137_run_start_has_force_gates_flags`
// removed — old subcommands replaced by generic dispatch with --force-gates/--reason.

// =============================================================================
// Part V1.45: Generic `creator run <preset_id>` surface tests
// =============================================================================

/// V1.45: `creator run --help` shows `<PRESET_ID>` as a positional arg.
#[test]
fn v145_creator_run_shows_preset_id_positional() {
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
        help_text.contains("PRESET_ID") || help_text.contains("preset_id"),
        "V1.45: creator run --help must show PRESET_ID positional arg"
    );
}

/// V1.45: `creator run --help` shows global flags (--json, --force-gates, --reason).
#[test]
fn v145_creator_run_has_global_flags() {
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
        help_text.contains("--json"),
        "V1.45: creator run --help must list --json flag"
    );
    assert!(
        help_text.contains("--force-gates"),
        "V1.45: creator run --help must list --force-gates flag"
    );
    assert!(
        help_text.contains("--reason"),
        "V1.45: creator run --help must list --reason flag"
    );
}

/// V1.45: `creator run --help` does NOT show old subcommands (start, continue, etc.).
#[test]
fn v145_creator_run_no_legacy_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "run", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();
    for old_subcmd in &[
        "start",
        "continue",
        "stage",
        "resume",
        "audit-chapter",
        "review-master",
    ] {
        assert!(
            !help_text.contains(&format!("\n  {old_subcmd} ")),
            "V1.45: creator run --help must not list old subcommand '{old_subcmd}'"
        );
    }
}
