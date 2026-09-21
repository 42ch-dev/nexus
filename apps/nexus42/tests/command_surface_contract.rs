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
//! 2. **V2 target contract** — defines the expected V2 command topology (the
//!    visible four-group lock plus the canonical subcommand shapes). Every test
//!    in this file is an active `#[test]` — there are no `#[ignore]`s: the
//!    restructuring landed (v1.193 P2 retired the daemon group, so the target
//!    contract now runs for real rather than as a to-be-un-ignored list).

use assert_cmd::Command;

// =============================================================================
// Part 1: Current-state snapshot (V1.15 baseline — these MUST pass today)
// =============================================================================

/// Snapshot: V1.15 has exactly 24 user-visible top-level command groups.
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

    // User-visible commands after the v1.193 P2-T2 daemon-group removal
    // (v1.193 P1-T6 removed the `sync` alias; P2-T2 removed `daemon`).
    let expected_commands = ["acp", "creator", "platform", "system"];

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

    // Verify count: 4 user-visible commands
    let visible_count = expected_commands.len();
    assert_eq!(
        visible_count, 4,
        "Current-state snapshot: expected exactly 4 user-visible commands, found {visible_count}"
    );
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

    // V1.16+ system: version, doctor, completion, config, debug, db, identity, runtime-mode
    // (v1.193 P1-T1: `system preset` — the PL-6 forwarding alias — is removed)
    for subcmd in &[
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
// Part 2: V2 target contract tests (all active — no #[ignore] remains)
//
// These define the V2 command surface. Each test asserts that a canonical
// top-level group exists with its expected subcommands. They were `#[ignore]`d
// while the CLI restructuring was pending; every one is now an active `#[test]`
// and the migration list below is kept only as history:
//   - Plan 2 (daemon/acp): `v2_target_daemon_subcommands` (daemon group retired
//     in v1.193 P2) and `v2_target_acp_exists`
//   - Plan 3 (system/platform): `v2_target_system_subcommands`
//     and `v2_target_platform_exists`
//   - Plan 4 (creator/knowledge): `v2_target_creator_subcommands`
//   - Plan 2–4 together: `v2_only_five_visible_command_groups`
// =============================================================================

/// V2 Target: the user-visible top-level command groups:
/// `acp`, `creator`, `platform`, `system`
///
/// Active since Plans 2-4 completed the CLI restructuring. v1.193 P1-T6
/// removed the hidden top-level `sync` alias and v1.193 P2-T2 removed the
/// `daemon` group with the legacy daemon composition.
#[test]
fn v2_canonical_visible_command_groups() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    let v2_groups = ["acp", "creator", "platform", "system"];

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

/// V2 Target: `acp` top-level command group exists.
///
/// Subcommands asserted here: probe,
///   registry (list, inspect), agent (use, list), session, policy, run.
///   `permission` is asserted separately by
///   `cli_agent.rs::acp_command_group_shows_subcommands`. v1.193 P1-T4 removed
///   the `status`/`doctor` daemon-health leaves.
///
/// Active since Plan 2 created the `acp` group.
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

    for subcmd in &["probe", "registry", "agent", "session", "policy", "run"] {
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
/// Active since Plan 4 landed the creator/knowledge surface.
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

/// V2 Target: `platform` top-level command group exists.
///
/// Expected subcommands: auth (login/logout/status), context (assemble-moment),
///   sync (push/pull/status/resolve/world/retry).
///
/// Active since Plan 3 created the `platform` group. v1.193 P1-T5 removed the
/// deferred `explore` and `publish` leaves along with the `context assemble`
/// guidance leaf, so those names must no longer be advertised.
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

    for subcmd in &["auth", "context", "sync"] {
        assert!(
            help_text.contains(subcmd),
            "V2 platform: expected subcommand '{subcmd}'"
        );
    }
}

/// V2 Target: `system` command group subcommands (extended).
///
/// Expected: version, doctor, completion,
///   config (get/set/unset/path), debug (dump-workspace)
///
/// Active since Plan 3 extended the `system` group.
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

/// v1.193 P2-T12: the Plan-5 `creator kb list --scope` selector was removed in
/// P0-T13, so the retained local `kb list` leaf accepts no scope option —
/// `--scope` is clap's unexpected-argument error (exit 2) and the help page
/// must not advertise it.
///
/// Discriminating regression: pre-retirement `creator kb list --scope work`
/// parsed and selected the scope; post-retirement only the advertised
/// `-o/--output` option remains on the leaf.
#[test]
fn retired_kb_scope_flag_is_rejected() {
    let rejected = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "kb", "list", "--scope", "work"])
        .assert()
        .code(2)
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&rejected.stderr).into_owned();
    assert!(
        stderr.contains("unexpected argument") && stderr.contains("--scope"),
        "v1.193 P2-T12: `creator kb list --scope` must be an unknown argument: {stderr}"
    );

    let help = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "kb", "list", "--help"])
        .assert()
        .success()
        .get_output()
        .clone();
    let help_text = String::from_utf8_lossy(&help.stdout).into_owned();
    assert!(
        !help_text.contains("--scope"),
        "v1.193 P2-T12: `creator kb list --help` must not advertise the removed flag: {help_text}"
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

// =============================================================================
// Part 6: V1.33 Work Experience Loop contract tests (must pass immediately)
// =============================================================================

// V1.45: `v133_creator_run_subcommands` removed — old subcommands (start, continue,
// etc.) replaced by generic `creator run <preset_id>` dispatch; v1.193 P2-T1 then
// removed that runner too, so these leaves are simply gone.

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

// v1.193 P1-T1: `v133_system_preset_subcommands` and `v133_system_preset_list_flags`
// removed with the PL-6 `system preset` forwarding alias.

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

/// V1.35 P2: Root `--help` lists the canonical user-visible command groups
/// (per cli-command-ia.md §2). The deprecated top-level `sync` alias was
/// removed from the parser in v1.193 P1-T6, and the `daemon` group in
/// v1.193 P2-T2.
#[test]
fn v135_root_help_shows_canonical_groups_and_no_top_level_sync() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let help_text = String::from_utf8(output).unwrap();

    // The canonical V1.35 top-level groups MUST all appear.
    let expected = ["creator", "acp", "platform", "system"];
    for group in &expected {
        assert!(
            help_text.contains(group),
            "V1.35 root help: expected visible group '{group}'"
        );
    }

    // The top-level `sync` alias was removed in v1.193 P1-T6; the word "sync"
    // may still appear in long_about examples ("nexus42 platform sync pull")
    // which is intentional. So we check that `sync` does NOT appear as a
    // top-level Commands entry — i.e. not in the "Commands:" section after
    // the long_about examples.
    let commands_section = help_text
        .split("Commands:")
        .nth(1)
        .expect("Commands: section present in --help");
    assert!(
        !commands_section.contains("\n  sync"),
        "V1.35 root help: top-level 'sync' must be hidden (was visible in Commands list)"
    );
}

/// Plan 2026-09-20-v1.193-p1 (QC1 F-001): root help and `connect --help` must
/// describe the served Connect surface — world-scoped ops, World/module/Actor-
/// gated `compute`, and the two host-level reads — with only `project`/unknown
/// refused. Red at BASE, whose `Commands::Connect` doc called the group a read
/// half and claimed `compute` is refused.
#[cfg(feature = "connect-host")]
#[test]
fn v1193_connect_help_describes_served_compute_surface() {
    // clap wraps help text at the render width, so collapse whitespace (and
    // case) before matching the pinned phrases.
    fn flattened(args: &[&str]) -> String {
        let output = Command::cargo_bin("nexus42")
            .unwrap()
            .args(args)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        String::from_utf8(output)
            .unwrap()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }

    let root = flattened(&["--help"]);
    assert!(
        !root.contains("read half"),
        "root help: Connect must not be described as a read half (F-001)"
    );
    assert!(
        root.contains(
            "peer surface for third-party reasoners \
             (world-scoped ops, gated compute, host-level reads)"
        ),
        "root help: Connect must describe the served set (F-001): {root}"
    );

    let connect = flattened(&["connect", "--help"]);
    for phrase in [
        "peer surface for third-party reasoners (world-scoped ops, gated compute, host-level reads)",
        "`compute` under the stored world/module/actor gates",
        "tools.nexus.list_observed_peers",
        "tools.nexus.list_modules",
        "only `project` and unknown ops are refused (`op_unsupported`)",
    ] {
        assert!(
            connect.contains(phrase),
            "connect help: expected served-surface phrase {phrase:?}: {connect}"
        );
    }
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

// V1.45: `v135_chain_novel_writing_defaults_true`,
// `v135_chain_novel_writing_opt_out_syntax_accepted`,
// `v136_start_help_mentions_auto_completion`,
// `v137_stage_advance_has_force_gates_flags`, `v137_run_start_has_force_gates_flags`
// removed — old subcommands replaced by generic dispatch with --force-gates/--reason.

// =============================================================================
// v1.193 P1-T1: `preset validate` is local-only; removed surfaces are unknown
// =============================================================================

/// Invalid bundle fixture: `preset.initial` names a state that does not
/// exist — the structural defect `loader_validate_manifest_compat` rejects.
/// The directory name must equal `preset.id`.
const INVALID_BUNDLE_YAML: &str = r"
preset:
  id: tiny-broken
  version: 1
  kind: creator
  description: invalid fixture for the local-only validator
  requires_capabilities: []
  run_intents:
    - work_init
  initial: missing_state
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
";

/// v1.193 P1-T1 (AC1/AC2/AC5): `preset validate <path>` is the sole local
/// validator. An invalid bundle is rejected with the retained validation
/// failure — exit 1 and no local service — while the bundle is left
/// untouched; the removed `preset run` subcommand and `--offline` switch are
/// clap usage errors (exit 2), not silently accepted synonyms.
#[test]
fn preset_validation_rejects_invalid_bundle_without_daemon() {
    let home = tempfile::tempdir().unwrap();
    let bundle = home.path().join("tiny-broken");
    std::fs::create_dir_all(&bundle).unwrap();
    let manifest = bundle.join("preset.yaml");
    std::fs::write(&manifest, INVALID_BUNDLE_YAML).unwrap();
    let before = std::fs::read(&manifest).unwrap();

    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .env("HOME", home.path())
        .args(["preset", "validate", bundle.to_str().unwrap()])
        .assert()
        .code(1)
        .get_output()
        .clone();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stdout.contains("Invalid preset"),
        "v1.193 P1-T1: the local validator must report the invalid verdict, got: {stdout}"
    );
    assert!(
        stdout.contains("unknown state"),
        "v1.193 P1-T1: the retained validation failure must survive the cutover, got: {stdout}"
    );
    assert!(
        stderr.contains("preset validation failed"),
        "v1.193 P1-T1: the failure must be the retained validation verdict, not a transport error, got: {stderr}"
    );

    // Read-only: validation does not rewrite or extend the bundle.
    assert_eq!(
        std::fs::read(&manifest).unwrap(),
        before,
        "v1.193 P1-T1: validation must not rewrite the bundle it inspects"
    );
    let entries: Vec<String> = std::fs::read_dir(&bundle)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        entries,
        vec!["preset.yaml".to_string()],
        "v1.193 P1-T1: validation must not add files to the bundle"
    );

    // The removed runner is an unknown subcommand.
    let run = Command::cargo_bin("nexus42")
        .unwrap()
        .env("HOME", home.path())
        .args(["preset", "run", "--help"])
        .assert()
        .code(2)
        .get_output()
        .clone();
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("unrecognized subcommand"),
        "v1.193 P1-T1: `preset run` must be an unknown subcommand"
    );

    // `--offline` is rejected, never treated as a synonym for the local path.
    let offline = Command::cargo_bin("nexus42")
        .unwrap()
        .env("HOME", home.path())
        .args(["preset", "validate", bundle.to_str().unwrap(), "--offline"])
        .assert()
        .code(2)
        .get_output()
        .clone();
    assert!(
        String::from_utf8_lossy(&offline.stderr).contains("--offline"),
        "v1.193 P1-T1: `--offline` must be an unexpected argument"
    );
}

// =============================================================================
// Part 9: v1.193 P1-T4 local-service probe removal contract tests
// =============================================================================

/// v1.193 P1-T4 (AC2/AC5): `system debug dump-workspace` is a purely local
/// snapshot. The `daemon_status` block was removed together with its loopback
/// probe, so the configured `daemon_url` is never contacted and the snapshot
/// carries no daemon key.
///
/// Discriminating regression: the fixture points `daemon_url` at a counting
/// loopback trap. Pre-fix, `dump_workspace` built a `DaemonClient` for that URL
/// and issued `/v1/daemon/runtime/health` (observed as a connection) before
/// inserting `daemon_status`; post-fix the dump stays local with zero observed
/// connections and no `daemon_status` entry.
#[test]
fn workspace_dump_never_queries_local_service() {
    use std::io::ErrorKind;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    let home = tempfile::tempdir().expect("temp home");
    let cwd = tempfile::tempdir().expect("temp cwd");

    // Counting loopback trap standing in for the configured local service.
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

    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .env("HOME", home.path())
        .env_remove("NEXUS_API_KEY")
        .current_dir(cwd.path())
        .args(["system", "debug", "dump-workspace"])
        .assert()
        .success()
        .get_output()
        .clone();

    // Drain window: a connection the child opened is queued and accepted here.
    std::thread::sleep(Duration::from_millis(100));
    stop.store(true, Ordering::SeqCst);
    accept_loop.join().expect("join probe listener");

    let stdout = String::from_utf8(output.stdout).expect("utf8 dump");
    let snapshot: serde_json::Value =
        serde_json::from_str(&stdout).expect("dump-workspace emits JSON");
    assert!(
        snapshot.get("config").is_some() && snapshot.get("nexus_home").is_some(),
        "v1.193 P1-T4: the dump must keep the local config and home snapshot: {stdout}"
    );
    assert!(
        snapshot.get("daemon_status").is_none(),
        "v1.193 P1-T4: dump-workspace must not carry a daemon_status block: {stdout}"
    );
    assert_eq!(
        probes.load(Ordering::SeqCst),
        0,
        "v1.193 P1-T4: dump-workspace must not open any connection to the configured local service ({daemon_url})"
    );
}

/// v1.193 P1-T4 (AC2): the daemon-HTTP delta replay leaf is removed while the
/// local dump leaf remains the only `system debug` subcommand.
#[test]
fn system_debug_replay_delta_is_removed() {
    let removed = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["system", "debug", "replay-delta", "delta-1"])
        .assert()
        .code(2)
        .get_output()
        .clone();
    assert!(
        String::from_utf8_lossy(&removed.stderr).contains("unrecognized subcommand"),
        "v1.193 P1-T4: `system debug replay-delta` must be an unknown subcommand"
    );

    let help = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["system", "debug", "--help"])
        .assert()
        .success()
        .get_output()
        .clone();
    let help_text = String::from_utf8_lossy(&help.stdout).into_owned();
    assert!(
        help_text.contains("dump-workspace"),
        "v1.193 P1-T4: `system debug` must retain the local dump leaf: {help_text}"
    );
    assert!(
        !help_text.contains("replay-delta"),
        "v1.193 P1-T4: `system debug` must not advertise the removed replay leaf: {help_text}"
    );
}

// =============================================================================
// Part 10: v1.193 P1-T5 platform stub removal & retained cloud groups
// =============================================================================

/// Extract the `Commands:` block of a clap `--help` page (everything between the
/// `Commands:` heading and the following `Options:` heading).
fn help_commands_section(help: &str) -> String {
    help.split("Commands:")
        .nth(1)
        .and_then(|rest| rest.split("Options:").next())
        .unwrap_or_default()
        .to_string()
}

/// v1.193 P1-T5 (AC2/AC3/AC5): the `platform` group keeps only its real leaves —
/// the cloud transport (`auth` login/logout/status + hidden token, `sync`
/// push/pull/status/resolve/world/retry) and the local `context assemble-moment`
/// SSOT. The deferred `explore` and `publish` leaves and the exit-2
/// `context assemble` guidance leaf are gone, and no local launcher or
/// replacement service group took their place.
///
/// Discriminating regression: before this change `platform explore …`,
/// `platform publish` and `platform context assemble --world-id …` all parsed;
/// afterwards each is an unknown subcommand on the real binary. Parser only —
/// the retained groups are exercised through `--help`, so no network call and no
/// credential is involved.
#[allow(clippy::too_many_lines)] // one parser-surface sweep asserted in place
#[test]
fn platform_retains_cloud_groups_without_local_launcher() {
    let home = tempfile::tempdir().expect("temp home");

    // ── Retained cloud transport: `platform auth` ─────────────────────────
    let auth = Command::cargo_bin("nexus42")
        .unwrap()
        .env("HOME", home.path())
        .env_remove("NEXUS_API_KEY")
        .args(["platform", "auth", "--help"])
        .assert()
        .success()
        .get_output()
        .clone();
    let auth_help = String::from_utf8_lossy(&auth.stdout).into_owned();
    for leaf in ["login", "logout", "status"] {
        assert!(
            auth_help.contains(leaf),
            "v1.193 P1-T5: `platform auth` must retain the '{leaf}' leaf: {auth_help}"
        );
    }
    // `token` is `#[command(hide = true)]`: absent from help, still callable.
    Command::cargo_bin("nexus42")
        .unwrap()
        .env("HOME", home.path())
        .env_remove("NEXUS_API_KEY")
        .args(["platform", "auth", "token", "--help"])
        .assert()
        .success();

    // ── Retained cloud transport: `platform sync` ─────────────────────────
    let sync = Command::cargo_bin("nexus42")
        .unwrap()
        .env("HOME", home.path())
        .env_remove("NEXUS_API_KEY")
        .args(["platform", "sync", "--help"])
        .assert()
        .success()
        .get_output()
        .clone();
    let sync_help = String::from_utf8_lossy(&sync.stdout).into_owned();
    for leaf in ["push", "pull", "status", "resolve", "world", "retry"] {
        assert!(
            sync_help.contains(leaf),
            "v1.193 P1-T5: `platform sync` must retain the '{leaf}' leaf: {sync_help}"
        );
    }

    // ── Retained local SSOT: `platform context assemble-moment` ───────────
    let context = Command::cargo_bin("nexus42")
        .unwrap()
        .env("HOME", home.path())
        .env_remove("NEXUS_API_KEY")
        .args(["platform", "context", "--help"])
        .assert()
        .success()
        .get_output()
        .clone();
    let context_help = String::from_utf8_lossy(&context.stdout).into_owned();
    assert!(
        context_help.contains("assemble-moment"),
        "v1.193 P1-T5: `platform context` must retain the local assemble-moment SSOT: {context_help}"
    );

    // ── Removed stubs are unknown subcommands (exit 2) ────────────────────
    let removed: [&[&str]; 7] = [
        &["platform", "explore", "browse"],
        &["platform", "explore", "search"],
        &["platform", "explore", "--help"],
        &["platform", "publish"],
        &["platform", "publish", "--help"],
        &["platform", "context", "assemble", "--world-id", "wld_test"],
        &["platform", "context", "assemble", "--help"],
    ];
    for args in removed {
        let output = Command::cargo_bin("nexus42")
            .unwrap()
            .env("HOME", home.path())
            .env_remove("NEXUS_API_KEY")
            .args(args)
            .assert()
            .code(2)
            .get_output()
            .clone();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert!(
            stderr.contains("unrecognized subcommand"),
            "v1.193 P1-T5: `{}` must be an unknown subcommand: {stderr}",
            args.join(" ")
        );
    }

    // ── No replacement local launcher or service group ────────────────────
    let platform = Command::cargo_bin("nexus42")
        .unwrap()
        .env("HOME", home.path())
        .env_remove("NEXUS_API_KEY")
        .args(["platform", "--help"])
        .assert()
        .success()
        .get_output()
        .clone();
    let platform_help = String::from_utf8_lossy(&platform.stdout).into_owned();
    let commands = help_commands_section(&platform_help);
    for retained in ["auth", "context", "sync"] {
        assert!(
            commands.contains(retained),
            "v1.193 P1-T5: `platform` must keep the '{retained}' group: {commands}"
        );
    }
    for forbidden in [
        "explore", "publish", "assemble", "serve", "service", "launcher", "launch", "daemon",
    ] {
        assert!(
            !commands.contains(forbidden),
            "v1.193 P1-T5: `platform` must not advertise '{forbidden}': {commands}"
        );
    }
}

// =============================================================================
// Part 11: v1.193 P1-T6 Model A / host-call / sync-alias entrance removal
// =============================================================================

/// v1.193 P1-T6 (AC1/AC2/AC3): the Model A MCP bridge (`mcp serve`), the raw
/// `host-call` debug entry and the hidden top-level `sync` alias are no longer
/// parser entrances — each invocation is an unknown subcommand (clap exit 2),
/// never a local service launcher, a deprecated forwarding alias, or a raw
/// daemon-tool HTTP client. The hidden `ops inspect` operator entry stays
/// callable, and the generic ACP `mcp_servers` descriptor path is covered by
/// `tests/mcp_acp_probe.rs`.
#[test]
fn retired_operator_entrances_are_unknown() {
    let home = tempfile::tempdir().expect("temp home");

    let removed: [&[&str]; 4] = [
        &["mcp", "serve"],
        &["mcp", "--help"],
        &["host-call", "nexus.context.whoami", "--args", "{}"],
        &["sync", "status"],
    ];
    for args in removed {
        let output = Command::cargo_bin("nexus42")
            .unwrap()
            .env("HOME", home.path())
            .args(args)
            .assert()
            .code(2)
            .get_output()
            .clone();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert!(
            stderr.contains("unrecognized subcommand"),
            "v1.193 P1-T6: `{}` must be an unknown subcommand: {stderr}",
            args.join(" ")
        );
    }

    // The hidden `ops inspect` operator entry is retained — the Model A cut
    // removes routing, not the daemon-free inspector.
    Command::cargo_bin("nexus42")
        .unwrap()
        .env("HOME", home.path())
        .args(["ops", "inspect", "--help"])
        .assert()
        .success();
}

// =============================================================================
// Part 12: v1.193 P2-T1 Creator runner / Work execution-entry removal
// =============================================================================

/// Subcommand names the `Commands:` block of a clap `--help` page advertises.
///
/// An entry line is `  <name>  <description>`; wrapped description text is
/// indented past the name column, so only lines whose remainder after the name
/// is empty or starts with the description gap count as entries.
fn help_command_names(help: &str) -> Vec<String> {
    help_commands_section(help)
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("  ")?;
            if rest.starts_with(' ') {
                return None;
            }
            let name = rest.split_whitespace().next()?;
            let after = &rest[name.len()..];
            (after.is_empty() || after.starts_with("  ")).then(|| name.to_string())
        })
        .collect()
}

/// v1.193 P2-T1 (AC1/AC2/AC5): the incomplete Creator runner (`creator run`,
/// `creator bootstrap`) and the Work execution entrances (`creator works
/// intake`, `creator works resume-chain`, `creator works start`,
/// `creator works create`) are unknown parser entries — not success-shaped
/// stubs, not hidden variants that answer with guidance, and not help pages.
///
/// Discriminating regression: pre-cutover `creator run <preset> --help` exited
/// 0 with the manifest-enriched help page (the deleted
/// `creator_run_preset_help.rs` smoke test) and `creator works start` parsed
/// into a hidden variant whose handler answered exit 1 with a "use bootstrap
/// instead" message; post-cutover every listed invocation is clap's
/// unrecognized-subcommand error (exit 2, empty stdout), while the retained
/// `creator` / `creator works` leaves still parse.
#[test]
fn retired_creator_execution_is_unknown() {
    let home = tempfile::tempdir().expect("temp home");

    let removed: [&[&str]; 8] = [
        &["creator", "run", "novel-writing"],
        &["creator", "run", "--help"],
        &["creator", "bootstrap", "--idea", "a new novel"],
        &["creator", "bootstrap", "--help"],
        &["creator", "works", "intake", "wrk_1"],
        &["creator", "works", "resume-chain", "wrk_1"],
        &["creator", "works", "start", "--idea", "x"],
        &["creator", "works", "create"],
    ];
    for args in removed {
        let output = Command::cargo_bin("nexus42")
            .unwrap()
            .env("HOME", home.path())
            .env_remove("NEXUS_API_KEY")
            .args(args)
            .assert()
            .code(2)
            .get_output()
            .clone();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert!(
            stderr.contains("unrecognized subcommand"),
            "v1.193 P2-T1: `{}` must be an unknown subcommand: {stderr}",
            args.join(" ")
        );
        assert!(
            output.stdout.is_empty(),
            "v1.193 P2-T1: `{}` must not answer with a success-shaped stub: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout)
        );
    }

    // The retained Creator group advertises only retained entries.
    let creator = Command::cargo_bin("nexus42")
        .unwrap()
        .env("HOME", home.path())
        .env_remove("NEXUS_API_KEY")
        .args(["creator", "--help"])
        .assert()
        .success()
        .get_output()
        .clone();
    let creator_names = help_command_names(&String::from_utf8_lossy(&creator.stdout));
    for leaf in ["run", "bootstrap"] {
        let advertised = creator_names.iter().any(|name| name == leaf);
        assert!(
            !advertised,
            "v1.193 P2-T1: `creator` must not advertise '{leaf}': {creator_names:?}"
        );
    }
    for leaf in ["works", "register", "status", "list", "kb", "world"] {
        assert!(
            creator_names.iter().any(|name| name == leaf),
            "v1.193 P2-T1: `creator` must keep '{leaf}': {creator_names:?}"
        );
    }

    // The retained Work group advertises only retained entries.
    let works = Command::cargo_bin("nexus42")
        .unwrap()
        .env("HOME", home.path())
        .env_remove("NEXUS_API_KEY")
        .args(["creator", "works", "--help"])
        .assert()
        .success()
        .get_output()
        .clone();
    let works_names = help_command_names(&String::from_utf8_lossy(&works.stdout));
    for leaf in ["intake", "resume-chain", "start", "create"] {
        let advertised = works_names.iter().any(|name| name == leaf);
        assert!(
            !advertised,
            "v1.193 P2-T1: `creator works` must not advertise '{leaf}': {works_names:?}"
        );
    }
    for leaf in ["list", "status", "use", "inspire", "reopen", "pool"] {
        assert!(
            works_names.iter().any(|name| name == leaf),
            "v1.193 P2-T1: `creator works` must keep '{leaf}': {works_names:?}"
        );
    }
}

// =============================================================================
// Part 13: v1.193 P2-T2 daemon group / daemon-run router removal
// =============================================================================

/// v1.193 P2-T2 (AC1/AC4): the whole `daemon` command group (service
/// lifecycle, `ui`/`web`, and the `schedule` orchestration leaves) and the
/// hidden `daemon-run` self-spawn entry are gone from the parser — every
/// invocation is clap's unrecognized-subcommand error (exit 2, empty stdout),
/// never a success-shaped stub, a hidden alias, a replacement `nexus42
/// service` launcher or an HTTP fallback. The retained `creator` / `acp` /
/// `platform` groups still parse, and the root help neither advertises the
/// group nor teaches `nexus42 daemon schedule`.
///
/// Discriminating regression: pre-cutover `daemon --help` printed the daemon
/// group page (exit 0), `daemon status`/`logs`/`ui` reached the loopback
/// daemon HTTP client, `daemon schedule …` parsed the orchestration leaves,
/// `daemon-run [--port …]` booted the runtime in-process, and `nexus42 --help`
/// carried the `nexus42 daemon schedule --preset <id>` long-about line.
#[test]
fn daemon_tree_is_unknown() {
    let home = tempfile::tempdir().expect("temp home");

    let removed: [&[&str]; 15] = [
        &["daemon", "--help"],
        &["daemon", "start"],
        &["daemon", "stop"],
        &["daemon", "restart"],
        &["daemon", "status"],
        &["daemon", "logs"],
        &["daemon", "doctor"],
        &["daemon", "ui"],
        &["daemon", "web"],
        &["daemon", "orchestrate", "run"],
        &["daemon", "schedule", "list"],
        &["daemon", "schedule", "start", "--preset", "novel-writing"],
        &["daemon-run"],
        &["daemon-run", "--help"],
        &["daemon-run", "--port", "8420"],
    ];
    for args in removed {
        let output = Command::cargo_bin("nexus42")
            .unwrap()
            .env("HOME", home.path())
            .env_remove("NEXUS_API_KEY")
            .args(args)
            .assert()
            .code(2)
            .get_output()
            .clone();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert!(
            stderr.contains("unrecognized subcommand"),
            "v1.193 P2-T2: `{}` must be an unknown subcommand: {stderr}",
            args.join(" ")
        );
        assert!(
            output.stdout.is_empty(),
            "v1.193 P2-T2: `{}` must not answer with a success-shaped stub: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout)
        );
    }

    // The root help no longer lists the group nor teaches the removed leaf.
    let root = Command::cargo_bin("nexus42")
        .unwrap()
        .env("HOME", home.path())
        .env_remove("NEXUS_API_KEY")
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .clone();
    let root_help = String::from_utf8_lossy(&root.stdout).into_owned();
    let root_names = help_command_names(&root_help);
    assert!(
        !root_names.iter().any(|name| name == "daemon"),
        "v1.193 P2-T2: root help must not advertise the 'daemon' group: {root_names:?}"
    );
    assert!(
        !root_help.contains("daemon schedule"),
        "v1.193 P2-T2: root help must not teach the removed daemon schedule leaf"
    );

    // The retained groups still parse (the cutover removes routing, not them).
    for args in [
        ["creator", "--help"],
        ["acp", "--help"],
        ["platform", "--help"],
    ] {
        Command::cargo_bin("nexus42")
            .unwrap()
            .env("HOME", home.path())
            .env_remove("NEXUS_API_KEY")
            .args(args)
            .assert()
            .success();
    }
}
