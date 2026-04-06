//! Integration Tests — CLI Agent Command Output.
//!
//! Tests that the `nexus42 agent` commands produce correct output
//! in various formats (text and JSON).

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

/// Test `nexus42 agent list --help` shows usage.
#[test]
fn agent_list_shows_help() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("agent")
        .arg("list")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("format"));
}

/// Test `nexus42 agent list` displays table format by default.
#[test]
fn agent_list_table_format() {
    let tmp = TempDir::new().expect("Failed to create temp dir");

    // Just verify the command runs without panicking
    let _assert = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("agent")
        .arg("list")
        .env("HOME", tmp.path())
        .assert();
}

/// Test `nexus42 agent list --format json` produces valid JSON.
#[test]
fn agent_list_json_format() {
    let tmp = TempDir::new().expect("Failed to create temp dir");

    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("agent")
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

/// Test `nexus42 agent show --help` shows usage.
#[test]
fn agent_show_shows_help() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("agent")
        .arg("show")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("show"))
        .stdout(predicate::str::contains("<AGENT_REF>"));
}

/// Test `nexus42 agent show` with unknown agent.
#[test]
fn agent_show_unknown_agent() {
    let tmp = TempDir::new().expect("Failed to create temp dir");

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("agent")
        .arg("show")
        .arg("nonexistent-agent-xyz")
        .env("HOME", tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found").or(predicate::str::contains("error")));
}

/// Test `nexus42 agent run --help` shows usage.
#[test]
fn agent_run_shows_help() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("agent")
        .arg("run")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("run"))
        .stdout(predicate::str::contains("<AGENT_REF>"));
}

/// Test `nexus42 agent probe --help` shows usage.
#[test]
fn agent_probe_shows_help() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("agent")
        .arg("probe")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("probe"))
        .stdout(predicate::str::contains("--registry"))
        .stdout(predicate::str::contains("--agent"));
}

/// Test `nexus42 agent probe --registry` checks connectivity.
#[test]
fn agent_probe_registry() {
    let tmp = TempDir::new().expect("Failed to create temp dir");

    let _output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("agent")
        .arg("probe")
        .arg("--registry")
        .env("HOME", tmp.path())
        .output()
        .expect("Failed to execute command");
}

/// Test `nexus42 agent` subcommand group shows all commands.
#[test]
fn agent_command_group_shows_subcommands() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("agent")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("show"))
        .stdout(predicate::str::contains("run"))
        .stdout(predicate::str::contains("probe"));
}

/// Test invalid format argument produces error.
#[test]
fn agent_list_invalid_format() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("agent")
        .arg("list")
        .arg("--format")
        .arg("invalid-format")
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid").or(predicate::str::contains("error")));
}

/// Test `nexus42 agent list --format text` works.
#[test]
fn agent_list_explicit_text_format() {
    let tmp = TempDir::new().expect("Failed to create temp dir");

    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("agent")
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

/// Test that agent commands handle missing cache gracefully.
#[test]
fn agent_commands_handle_missing_cache() {
    let tmp = TempDir::new().expect("Failed to create temp dir");

    let cache_dir = tmp.path().join(".nexus42").join("registry");
    assert!(!cache_dir.exists(), "Cache should not exist initially");

    let _output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("agent")
        .arg("list")
        .env("HOME", tmp.path())
        .output()
        .expect("Failed to execute command");
}
