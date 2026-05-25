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
        .stdout(predicate::str::contains("creator"))
        .stdout(predicate::str::contains("daemon"));
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

/// Test init workspace command (now under `creator init`)
#[test]
fn init_workspace_creates_structure() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    let project = home.join("project");
    std::fs::create_dir_all(&project).unwrap();

    // Create a persistent identity first (creator commands require active creator)
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .arg("--name")
        .arg("TestCreator")
        .env("HOME", home)
        .assert()
        .success();

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("workspace")
        .arg("init")
        .arg("workspace")
        .arg("test-workspace")
        .arg("--creative-root")
        .arg(&project)
        .env("HOME", home)
        .current_dir(&project)
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Workspace initialized")
                .or(predicate::str::contains("already initialized")),
        );
}

/// Test init workspace does not re-initialize
#[test]
fn init_workspace_idempotent() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    let project = home.join("proj");
    std::fs::create_dir_all(&project).unwrap();

    // Create a persistent identity first (creator commands require active creator)
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .arg("--name")
        .arg("IdempotentTest")
        .env("HOME", home)
        .assert()
        .success();

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("workspace")
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
        .arg("creator")
        .arg("workspace")
        .arg("init")
        .arg("workspace")
        .arg("--creative-root")
        .arg(&project)
        .env("HOME", home)
        .current_dir(&project)
        .assert()
        .success()
        .stdout(
            predicate::str::contains("already initialized")
                .or(predicate::str::contains("already registered")),
        );
}

/// Test auth status (no daemon running — uses local `AuthStore`)
#[test]
fn auth_status_not_logged_in() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("platform")
        .arg("auth")
        .arg("status")
        .env("HOME", TempDir::new().unwrap().path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Not logged in"));
}

/// Test auth login with token (writes to local `AuthStore`, no daemon)
#[test]
fn auth_token_login() {
    let tmp = TempDir::new().unwrap();

    // V1.10: login_with_token writes to local AuthStore, no daemon needed
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("platform")
        .arg("auth")
        .arg("token")
        .arg("test-access-token")
        .arg("--user-id")
        .arg("usr_test_123")
        .env("HOME", tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("token stored"));
}

/// Test auth logout (clears local `AuthStore`, no daemon)
#[test]
fn auth_logout() {
    let tmp = TempDir::new().unwrap();

    // V1.10: logout clears local AuthStore, no daemon needed.
    // When not logged in, prints "Not logged in." (success exit).
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("platform")
        .arg("auth")
        .arg("logout")
        .env("HOME", tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Not logged in"));
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

/// Test sync status works without daemon — now queries local outbox directly
#[test]
fn sync_status_without_daemon() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("sync")
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("Sync Status"));
}

/// Test sync push is blocked in `local_only` mode with `PlatformOperationProhibited` error
#[test]
fn sync_push_blocked_in_local_only() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("sync")
        .arg("push")
        .env("HOME", home)
        .assert()
        .failure()
        .stderr(predicate::str::contains("not available in local_only mode"));
}

/// Test context assemble command validates --world-id requirement
#[test]
fn context_assemble_requires_world_id() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("platform")
        .arg("context")
        .arg("assemble")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--world-id"));
}

/// Test context assemble command returns "not yet available" in V1.10
#[test]
fn context_assemble_with_world_id_connects_daemon() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("platform")
        .arg("context")
        .arg("assemble")
        .arg("--world-id")
        .arg("wld_test123")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not yet available"));
}

/// Test soul command group help (now under `creator soul`)
#[test]
fn soul_help() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("soul")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("show"))
        .stdout(predicate::str::contains("edit-personality"))
        .stdout(predicate::str::contains("validate"));
}

/// Test soul requires active creator
#[test]
fn soul_requires_active_creator() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("soul")
        .arg("show")
        .env("HOME", tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("No active creator"));
}

/// Test soul validate requires active creator
#[test]
fn soul_validate_requires_active_creator() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("soul")
        .arg("validate")
        .env("HOME", tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("No active creator"));
}

// =============================================================================
// E8: Integration tests for CLI commands (clone, config, debug, doctor)
// =============================================================================

/// Test clone command shows help (now under `creator clone`)
#[test]
fn clone_help() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("workspace")
        .arg("clone")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("WORLD_REF"))
        .stdout(predicate::str::contains("--source"))
        .stdout(predicate::str::contains("--dry-run"))
        .stdout(predicate::str::contains("--yes"));
}

/// Test clone requires `world_ref` argument
#[test]
fn clone_requires_world_ref() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("workspace")
        .arg("clone")
        .env("HOME", tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("WORLD_REF"));
}

/// Test clone is hard-deprecated (V1.27 H1) — always returns error.
#[test]
fn clone_dry_run_no_daemon() {
    let tmp = TempDir::new().unwrap();
    // Create a persistent identity first (creator commands require active creator)
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .arg("--name")
        .arg("CloneTestUser")
        .env("HOME", tmp.path())
        .assert()
        .success();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("workspace")
        .arg("clone")
        .arg("wld_test123")
        .arg("--source")
        .arg("local")
        .arg("--dry-run")
        .env("HOME", tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not available locally"));
}

/// Test clone with --source platform is hard-deprecated (V1.27 H1) — always returns error.
#[test]
fn clone_dry_run_source_platform_blocked_in_local_only() {
    let tmp = TempDir::new().unwrap();
    // Create a persistent identity first (creator commands require active creator)
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .arg("--name")
        .arg("ClonePlatformTest")
        .env("HOME", tmp.path())
        .assert()
        .success();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("workspace")
        .arg("clone")
        .arg("wld_test123")
        .arg("--source")
        .arg("platform")
        .arg("--dry-run")
        .env("HOME", tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not available locally"));
}

/// Test clone with --source local is hard-deprecated (V1.27 H1) — always returns error.
#[test]
fn clone_dry_run_source_local() {
    let tmp = TempDir::new().unwrap();
    // Create a persistent identity first (creator commands require active creator)
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .arg("--name")
        .arg("CloneLocalTest")
        .env("HOME", tmp.path())
        .assert()
        .success();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("workspace")
        .arg("clone")
        .arg("wld_test123")
        .arg("--source")
        .arg("local")
        .arg("--dry-run")
        .env("HOME", tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not available locally"));
}

/// Test clone rejects invalid `world_ref` format
#[test]
fn clone_rejects_invalid_world_ref() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("workspace")
        .arg("clone")
        .arg("wld_") // Too short - invalid
        .arg("--dry-run")
        .env("HOME", tmp.path())
        .assert()
        .failure();
}

/// Test config command shows help (now under `system config`)
#[test]
fn config_help() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("config")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("get"))
        .stdout(predicate::str::contains("set"))
        .stdout(predicate::str::contains("unset"))
        .stdout(predicate::str::contains("path"));
}

/// Test config get existing key (`runtime_mode` has default)
#[test]
fn config_get_runtime_mode() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("config")
        .arg("get")
        .arg("runtime_mode")
        .env("HOME", tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("runtime_mode"));
}

/// Test config get non-existent key shows unset
#[test]
fn config_get_nonexistent_key() {
    let tmp = TempDir::new().unwrap();
    // workspace_path is optional and defaults to empty
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("config")
        .arg("get")
        .arg("workspace_path")
        .env("HOME", tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("(unset)"));
}

/// Test config set updates value
#[test]
fn config_set_platform_url() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("config")
        .arg("set")
        .arg("platform_url")
        .arg("https://test.nexus42.io")
        .env("HOME", tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Set platform_url"));
}

/// Test config set invalid key fails
#[test]
fn config_set_invalid_key_fails() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("config")
        .arg("set")
        .arg("invalid_key")
        .arg("some_value")
        .env("HOME", tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid config key"));
}

/// Test config unset reverts to default
#[test]
fn config_unset_resets_to_default() {
    let tmp = TempDir::new().unwrap();
    // First set a custom value
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("config")
        .arg("set")
        .arg("platform_url")
        .arg("https://custom.io")
        .env("HOME", tmp.path())
        .assert()
        .success();

    // Then unset it
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("config")
        .arg("unset")
        .arg("platform_url")
        .env("HOME", tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Unset"));
}

/// Test config path shows location
#[test]
fn config_path_shows_location() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("config")
        .arg("path")
        .env("HOME", tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("config.toml"));
}

/// Test debug command shows help (now under `system debug`)
#[test]
fn debug_help() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("debug")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("dump-workspace"))
        .stdout(predicate::str::contains("replay-delta"));
}

/// Test debug dump-workspace runs without error (daemon may not be running)
#[test]
fn debug_dump_workspace_no_panic() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("debug")
        .arg("dump-workspace")
        .env("HOME", tmp.path())
        .assert()
        .success(); // Should not panic, may show daemon not running
}

/// Test debug dump-workspace with json format (default)
#[test]
fn debug_dump_workspace_json_format() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("debug")
        .arg("dump-workspace")
        .arg("--format")
        .arg("json")
        .env("HOME", tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"config\""));
}

/// Test debug dump-workspace with toml format
#[test]
fn debug_dump_workspace_toml_format() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("debug")
        .arg("dump-workspace")
        .arg("--format")
        .arg("toml")
        .env("HOME", tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("config"));
}

/// Test debug replay-delta requires `delta_id`
#[test]
fn debug_replay_delta_requires_id() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("debug")
        .arg("replay-delta")
        .env("HOME", tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("DELTA_ID"));
}

/// Test debug replay-delta with nonexistent delta (daemon not running)
#[test]
fn debug_replay_delta_nonexistent() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("debug")
        .arg("replay-delta")
        .arg("delta-nonexistent-123")
        .env("HOME", tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Daemon not running"));
}

/// Test doctor command shows help (now under `system doctor`)
#[test]
fn doctor_help() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("doctor")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Diagnostic"));
}

/// Test doctor runs (daemon may not be running)
#[test]
fn doctor_check_no_panic() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("doctor")
        .env("HOME", tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("system doctor"));
}

/// Test doctor shows daemon connectivity check
#[test]
fn doctor_check_shows_daemon_status() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("doctor")
        .env("HOME", tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Daemon connectivity"));
}

/// Test doctor shows home directory check
#[test]
fn doctor_check_shows_config_status() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("doctor")
        .env("HOME", tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Home directory"));
}

/// Test doctor shows combined diagnostics output
#[test]
fn doctor_check_shows_database_status() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("doctor")
        .env("HOME", tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("diagnostics"));
}

/// Test doctor shows issue summary
#[test]
fn doctor_check_shows_workspace_status() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("doctor")
        .env("HOME", tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("issue"));
}

/// Test doctor shows ACP registry check
#[test]
fn doctor_check_shows_version_compatibility() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("doctor")
        .env("HOME", tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("ACP registry"));
}

/// Test doctor shows issue count summary
#[test]
fn doctor_check_shows_summary() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("doctor")
        .env("HOME", tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("issue(s) found"));
}

/// Test identity command shows help (now under `system identity`)
#[test]
fn identity_help() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("create"))
        .stdout(predicate::str::contains("use"))
        .stdout(predicate::str::contains("link"))
        .stdout(predicate::str::contains("unlink"));
}

/// Test identity unlink requires `creator_id`
#[test]
fn identity_unlink_requires_creator_id() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("unlink")
        .env("HOME", tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("CREATOR_ID"));
}

/// Test identity unlink with nonexistent `creator_id` (local database exists but identity not found)
#[test]
fn identity_unlink_nonexistent_creator() {
    let tmp = TempDir::new().unwrap();
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("unlink")
        .arg("ctr_nonexistent")
        .env("HOME", tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}
