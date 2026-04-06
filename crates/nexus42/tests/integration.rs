//! Integration Tests — CLI binary behavior

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

/// Test that CLI shows help
#[test]
fn cli_shows_help() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("nexus42"))
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("auth"))
        .stdout(predicate::str::contains("creator"))
        .stdout(predicate::str::contains("manuscript"));
}

/// Test that CLI shows version
#[test]
fn cli_shows_version() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("0.1.0"));
}

/// Test init workspace command
#[test]
fn init_workspace_creates_structure() {
    let tmp = TempDir::new().unwrap();
    let tmp_path = tmp.path();

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("init")
        .arg("workspace")
        .arg("test-workspace")
        .current_dir(tmp_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Workspace initialized"));

    // Verify directory structure was created
    assert!(tmp_path.join("Stories").exists());
    assert!(tmp_path.join("References").exists());
    assert!(tmp_path.join(".nexus42").exists());
    assert!(tmp_path.join(".nexus42/workspace.json").exists());
    assert!(tmp_path.join(".nexus42/.gitignore").exists());
}

/// Test init workspace does not re-initialize
#[test]
fn init_workspace_idempotent() {
    let tmp = TempDir::new().unwrap();

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("init")
        .arg("workspace")
        .current_dir(tmp.path())
        .assert()
        .success();

    // Second init should warn already initialized
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("init")
        .arg("workspace")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("already initialized"));
}

/// Test auth status (no auth configured)
#[test]
fn auth_status_not_logged_in() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("auth")
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("Not logged in"));
}

/// Test auth login with token
#[test]
fn auth_token_login() {
    let tmp = TempDir::new().unwrap();

    // Set HOME to temp dir to isolate auth state
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("auth")
        .arg("token")
        .arg("test-access-token")
        .arg("--user-id")
        .arg("usr_test_123")
        .env("HOME", tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Authenticated successfully"))
        .stdout(predicate::str::contains("usr_test_123"));
}

/// Test auth logout
#[test]
fn auth_logout() {
    let tmp = TempDir::new().unwrap();

    // Login first
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("auth")
        .arg("token")
        .arg("test-token")
        .arg("--user-id")
        .arg("usr_test_456")
        .env("HOME", tmp.path())
        .assert()
        .success();

    // Logout
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("auth")
        .arg("logout")
        .env("HOME", tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Logged out"));
}

/// Test creator command group
#[test]
fn creator_help() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("register"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("pair"))
        .stdout(predicate::str::contains("credentials"));
}

/// Test creator list (no data)
#[test]
fn creator_list_empty() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("list")
        .env("HOME", tmp.path())
        .assert()
        .success();
}

/// Test manuscript command group
#[test]
fn manuscript_help() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("manuscript")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("phase"))
        .stdout(predicate::str::contains("promote"))
        .stdout(predicate::str::contains("verify"));
}

/// Test manuscript verify
#[test]
fn manuscript_verify() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("manuscript")
        .arg("verify")
        .assert()
        .success()
        .stdout(predicate::str::contains("Verification passed"));
}

/// Test research command group
#[test]
fn research_help() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("research")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("scan"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("extract"));
}

/// Test research scan (no directory)
#[test]
fn research_scan_missing_dir() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("research")
        .arg("scan")
        .arg("--path")
        .arg(tmp.path().join("nonexistent").to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("not found"));
}

/// Test daemon status (daemon not running)
#[test]
fn daemon_status_not_running() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("daemon")
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("Not running"));
}

/// Test sync requires daemon
#[test]
fn sync_requires_daemon() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("sync")
        .arg("status")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not running"));
}

/// Test context assemble command validates --world-id requirement
#[test]
fn context_assemble_requires_world_id() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("context")
        .arg("assemble")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--world-id"));
}

/// Test context assemble command with --world-id attempts daemon connection
#[test]
fn context_assemble_with_world_id_connects_daemon() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("context")
        .arg("assemble")
        .arg("--world-id")
        .arg("wld_test_123")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error"));
}
