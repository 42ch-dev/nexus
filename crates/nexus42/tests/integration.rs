//! Integration Tests — CLI binary behavior

use assert_cmd::Command;
use nexus_local_db::{init as db_init, RuntimeRole};
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
    let home = tmp.path();
    let project = home.join("project");
    std::fs::create_dir_all(&project).unwrap();

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("init")
        .arg("workspace")
        .arg("test-workspace")
        .arg("--creative-root")
        .arg(&project)
        .env("HOME", home)
        .current_dir(&project)
        .assert()
        .success()
        .stdout(predicate::str::contains("Workspace initialized"));

    // Creative tree under chosen root (ADR-014 operational state lives under $HOME/.nexus42/...)
    assert!(project.join("Stories").exists());
    assert!(project.join("References").exists());
    assert!(project.join(".nexus42").exists());
    assert!(project.join(".nexus42/workspace.json").exists());
    assert!(project.join(".nexus42/.gitignore").exists());
    let meta = home.join(".nexus42/creators/local/workspaces/default/meta.json");
    assert!(meta.is_file());
    let db = home.join(".nexus42/creators/local/workspaces/default/state.db");
    assert!(db.is_file());
}

/// Test init workspace does not re-initialize
#[test]
fn init_workspace_idempotent() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    let project = home.join("proj");
    std::fs::create_dir_all(&project).unwrap();

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("init")
        .arg("workspace")
        .arg("--creative-root")
        .arg(&project)
        .env("HOME", home)
        .current_dir(&project)
        .assert()
        .success();

    // Second init should no-op (same creator/slug registration)
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("init")
        .arg("workspace")
        .arg("--creative-root")
        .arg(&project)
        .env("HOME", home)
        .current_dir(&project)
        .assert()
        .success()
        .stdout(predicate::str::contains("already registered"));
}

/// Test auth status (no daemon running)
#[test]
fn auth_status_not_logged_in() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("auth")
        .arg("status")
        .env("HOME", TempDir::new().unwrap().path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Daemon not running"));
}

/// Test auth login with token (daemon not running)
#[test]
fn auth_token_login() {
    let tmp = TempDir::new().unwrap();

    // Set HOME to temp dir to isolate auth state — daemon won't be running
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("auth")
        .arg("token")
        .arg("test-access-token")
        .arg("--user-id")
        .arg("usr_test_123")
        .env("HOME", tmp.path())
        .assert()
        .failure() // Daemon not running
        .stderr(predicate::str::contains("Daemon not running"));
}

/// Test auth logout (daemon not running)
#[test]
fn auth_logout() {
    let tmp = TempDir::new().unwrap();

    // Logout requires daemon running
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("auth")
        .arg("logout")
        .env("HOME", tmp.path())
        .assert()
        .failure() // Daemon not running
        .stderr(predicate::str::contains("Daemon not running"));
}

/// Legacy flat `state.db` is moved into ADR-014 layout by `migrate local-fs`.
#[test]
fn migrate_local_fs_moves_legacy_state_db() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    let nexus = home.join(".nexus42");
    std::fs::create_dir_all(&nexus).unwrap();
    let legacy = nexus.join("state.db");
    let conn = rusqlite::Connection::open(&legacy).unwrap();
    db_init(&conn, RuntimeRole::Cli).unwrap();
    drop(conn);

    let creative = home.join("creative");
    std::fs::create_dir_all(&creative).unwrap();

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("migrate")
        .arg("local-fs")
        .arg("--creator-id")
        .arg("ctr_mig")
        .arg("--workspace-slug")
        .arg("default")
        .arg("--local-root")
        .arg(&creative)
        .arg("--yes")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Migrated legacy"));

    assert!(!legacy.exists(), "legacy file should be renamed away");
    let migrated = home.join(".nexus42/creators/ctr_mig/workspaces/default/state.db");
    assert!(migrated.is_file());
    let backup = nexus.join("state.db.pre-adr014-migrated");
    assert!(backup.is_file());
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
    let tmp = TempDir::new().unwrap();

    // Init workspace (creates .nexus42, Stories, References in current dir)
    // Use env HOME to isolate from any existing workspace in parent dirs
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("init")
        .arg("workspace")
        .env("HOME", tmp.path())
        .current_dir(tmp.path())
        .assert()
        .success();

    // Create a manuscript (must run in workspace dir)
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("manuscript")
        .arg("create")
        .arg("Test Manuscript")
        .env("HOME", tmp.path())
        .current_dir(tmp.path())
        .assert()
        .success();

    // Verify the manuscript
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("manuscript")
        .arg("verify")
        .arg("Test Manuscript")
        .env("HOME", tmp.path())
        .current_dir(tmp.path())
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

/// Test sync status shows daemon not running message
#[test]
fn sync_status_without_daemon() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("sync")
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("not running"));
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
        .arg("wld_test123")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error"));
}
