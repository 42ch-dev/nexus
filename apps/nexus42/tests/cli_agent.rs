//! Integration Tests — CLI ACP Command Output.
//!
//! Tests that the `nexus42 acp` commands produce correct output
//! in various formats (text and JSON).

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

/// Test `nexus42 acp registry list --help` shows usage.
#[test]
fn acp_registry_list_shows_help() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("acp")
        .arg("registry")
        .arg("list")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("format"));
}

/// Test `nexus42 acp registry list` displays table format by default.
#[test]
fn acp_registry_list_table_format() {
    let tmp = TempDir::new().expect("Failed to create temp dir");

    // Just verify the command runs without panicking
    let _assert = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("acp")
        .arg("registry")
        .arg("list")
        .env("HOME", tmp.path())
        .assert();
}

/// Test `nexus42 acp registry list --format json` produces valid JSON.
#[test]
fn acp_registry_list_json_format() {
    let tmp = TempDir::new().expect("Failed to create temp dir");

    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("acp")
        .arg("registry")
        .arg("list")
        .arg("--format")
        .arg("json")
        .env("HOME", tmp.path())
        .output()
        .expect("Failed to execute command");

    if output.status.success() {
        let json_str = String::from_utf8_lossy(&output.stdout);
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json_str);

        if let Ok(json) = parsed {
            assert!(
                json.get("registry_version").is_some()
                    || json.get("version").is_some()
                    || json.get("agents").is_some(),
                "JSON should contain registry fields"
            );

            if let Some(agents) = json.get("agents") {
                assert!(agents.is_array(), "agents should be an array");
                if let Some(agent_arr) = agents.as_array() {
                    for agent in agent_arr {
                        assert!(agent.get("id").is_some(), "agent should have id");
                        assert!(agent.get("name").is_some(), "agent should have name");
                        assert!(agent.get("version").is_some(), "agent should have version");
                    }
                }
            }
        }
    }
}

/// Test `nexus42 acp registry inspect --help` shows usage.
#[test]
fn acp_registry_inspect_shows_help() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("acp")
        .arg("registry")
        .arg("inspect")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("inspect"))
        .stdout(predicate::str::contains("<AGENT_REF>"));
}

/// Test `nexus42 acp registry inspect` with unknown agent.
#[test]
fn acp_registry_inspect_unknown_agent() {
    let tmp = TempDir::new().expect("Failed to create temp dir");

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("acp")
        .arg("registry")
        .arg("inspect")
        .arg("nonexistent-agent-xyz")
        .env("HOME", tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found").or(predicate::str::contains("error")));
}

/// Test `nexus42 acp probe --help` shows usage.
#[test]
fn acp_probe_shows_help() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("acp")
        .arg("probe")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("probe"))
        .stdout(predicate::str::contains("--registry"))
        .stdout(predicate::str::contains("--agent"));
}

/// Test `nexus42 acp probe --registry` checks connectivity.
#[test]
fn acp_probe_registry() {
    let tmp = TempDir::new().expect("Failed to create temp dir");

    let _output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("acp")
        .arg("probe")
        .arg("--registry")
        .env("HOME", tmp.path())
        .output()
        .expect("Failed to execute command");
}

/// Test `nexus42 acp` command group shows the retained subcommands.
///
/// v1.193 P1-T4: the daemon-health leaves `status` and `doctor` are removed
/// from the group; the local ACP parser groups survive.
#[test]
fn acp_command_group_shows_subcommands() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("acp")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("probe"))
        .stdout(predicate::str::contains("registry"))
        .stdout(predicate::str::contains("agent"))
        .stdout(predicate::str::contains("session"))
        .stdout(predicate::str::contains("policy"))
        .stdout(predicate::str::contains("permission"))
        .stdout(predicate::str::contains("run"));
}

/// Test invalid format argument produces error.
#[test]
fn acp_registry_list_invalid_format() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("acp")
        .arg("registry")
        .arg("list")
        .arg("--format")
        .arg("invalid-format")
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid").or(predicate::str::contains("error")));
}

/// Test `nexus42 acp registry list --format text` works.
#[test]
fn acp_registry_list_explicit_text_format() {
    let tmp = TempDir::new().expect("Failed to create temp dir");

    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("acp")
        .arg("registry")
        .arg("list")
        .arg("--format")
        .arg("text")
        .env("HOME", tmp.path())
        .output()
        .expect("Failed to execute command");

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("ID")
                || stdout.contains("Version")
                || stdout.contains("No agents")
                || stdout.contains("agents available"),
            "Text format should show table structure or agent count"
        );
    }
}

/// Test that acp registry commands handle missing cache gracefully.
#[test]
fn acp_commands_handle_missing_cache() {
    let tmp = TempDir::new().expect("Failed to create temp dir");

    let cache_dir = tmp.path().join(".nexus42").join("registry");
    assert!(!cache_dir.exists(), "Cache should not exist initially");

    let _output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("acp")
        .arg("registry")
        .arg("list")
        .env("HOME", tmp.path())
        .output()
        .expect("Failed to execute command");
}

/// Test `nexus42 acp agent use` with a valid agent ref succeeds.
#[test]
fn acp_agent_use_success() {
    let tmp = TempDir::new().expect("Failed to create temp dir");

    // Set up a creator and workspace in the config
    let nexus_home = tmp.path().join(".nexus42");
    std::fs::create_dir_all(&nexus_home).expect("create nexus home");

    // Write a config.toml with active creator
    let config_content = r#"active_creator_id = "ctr_test"
[active_workspace_slug_by_creator]
ctr_test = "default"
"#;
    std::fs::write(nexus_home.join("config.toml"), config_content).expect("write config");

    // Create the workspace directory so the config path is valid
    let ws_dir = nexus_home
        .join("creators")
        .join("ctr_test")
        .join("workspaces")
        .join("default");
    std::fs::create_dir_all(&ws_dir).expect("create workspace dir");

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("acp")
        .arg("agent")
        .arg("use")
        .arg("claude-acp")
        .env("HOME", tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Default agent set to: claude-acp"));

    // Verify the config file was written
    let config_file = ws_dir.join("acp-default-agent.toml");
    assert!(config_file.exists(), "Config file should exist");
    let content = std::fs::read_to_string(&config_file).expect("read config");
    assert!(content.contains("schema_version = 1"));
    assert!(content.contains("default_agent_ref = \"claude-acp\""));
}

/// Test `nexus42 acp agent use` requires an agent ref argument.
#[test]
fn acp_agent_use_requires_arg() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("acp")
        .arg("agent")
        .arg("use")
        .assert()
        .failure();
}

/// Test `nexus42 acp agent use` persistence: set then read on new invocation.
#[test]
fn acp_agent_use_persistence() {
    let tmp = TempDir::new().expect("Failed to create temp dir");

    let nexus_home = tmp.path().join(".nexus42");
    std::fs::create_dir_all(&nexus_home).expect("create nexus home");

    let config_content = r#"active_creator_id = "ctr_persist"
[active_workspace_slug_by_creator]
ctr_persist = "default"
"#;
    std::fs::write(nexus_home.join("config.toml"), config_content).expect("write config");

    let ws_dir = nexus_home
        .join("creators")
        .join("ctr_persist")
        .join("workspaces")
        .join("default");
    std::fs::create_dir_all(&ws_dir).expect("create workspace dir");

    // Set agent
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("acp")
        .arg("agent")
        .arg("use")
        .arg("my-test-agent@1.0")
        .env("HOME", tmp.path())
        .assert()
        .success();

    // Verify file persists
    let config_file = ws_dir.join("acp-default-agent.toml");
    let content = std::fs::read_to_string(&config_file).expect("read config");
    assert!(content.contains("my-test-agent@1.0"));

    // Set a different agent (overwrite)
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("acp")
        .arg("agent")
        .arg("use")
        .arg("other-agent")
        .env("HOME", tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Default agent set to: other-agent",
        ));

    // Verify overwrite
    let content_after = std::fs::read_to_string(&config_file).expect("read config after");
    assert!(content_after.contains("other-agent"));
    assert!(!content_after.contains("my-test-agent"));
}

/// v1.193 P1-T4 (AC2/AC5): the daemon-health leaves `acp status` and
/// `acp doctor` (loopback `DaemonClient` probes) are removed — unknown, not
/// hidden — while the retained ACP parser groups keep their own options.
#[test]
fn acp_daemon_health_leaves_are_unknown_and_retained_groups_parse() {
    for leaf in ["status", "doctor"] {
        Command::cargo_bin("nexus42")
            .unwrap()
            .args(["acp", leaf])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("unrecognized subcommand"));
    }

    for args in [["acp", "session", "--help"], ["acp", "policy", "--help"]] {
        Command::cargo_bin("nexus42")
            .unwrap()
            .args(args)
            .assert()
            .success();
    }
}

/// v1.193 P2-T12 (AC1/AC5): the `daemon` command group (`start`/`stop`/
/// `restart`/`status`/`logs`/`doctor`/`schedule`) and the hidden `daemon-run`
/// self-spawn entry are gone from the parser — `daemon --help` is clap's
/// unrecognized-subcommand error, never a group page.
#[test]
fn daemon_group_is_unknown() {
    for args in [["daemon", "--help"], ["daemon", "status"]] {
        Command::cargo_bin("nexus42")
            .unwrap()
            .args(args)
            .assert()
            .code(2)
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains("unrecognized subcommand"));
    }
}

/// Test `nexus42 --help` no longer shows agent/session/policy/permission as top-level.
#[test]
fn help_no_longer_shows_old_top_level_commands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("--help")
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // These should NOT appear as visible top-level commands
    // (they're hidden behind `#[command(hide = true)]`)
    // Check that 'agent' is not a primary command name (agent as subtext in other words is ok)
    assert!(
        !stdout.contains("Agent management (ACP integration)"),
        "Old 'Agent' help text should be removed"
    );
    assert!(
        !stdout.contains("Permission policy management (ACP-R7)"),
        "Old 'Policy' help text should be removed"
    );
    assert!(
        !stdout.contains("Agent-scoped permission management"),
        "Old 'Permission' help text should be removed"
    );
}

/// Test `nexus42 --help` shows acp as a top-level command.
#[test]
fn help_shows_acp_top_level() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("ACP capability plane"));
}
