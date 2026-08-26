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
        .arg("--port")
        .arg("19999")
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

// =============================================================================
// V1.176 P0 T1 (AR-88): shared bootstrap helper — persistent-identity parity
// =============================================================================

/// `system identity create --persistent --name <n>` reaches the same
/// observable end state as `register --local` (compass PL-3): the identity is
/// active and the workspace `creators` row is materialized, so
/// `creator world create` succeeds immediately (no FK miss).
#[test]
fn persistent_identity_create_converges_workspace_row() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .arg("--name")
        .arg("ParityTester")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Created persistent identity"))
        .stdout(predicate::str::contains("Set as active identity."));

    // The workspace `creators` row must exist — `creator world create` passes
    // its FK precheck immediately after the persistent-identity bootstrap.
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("world")
        .arg("create")
        .arg("--title")
        .arg("Parity World")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("World created"));

    // The identity-cache store is NOT written by the local bootstrap (AR-88 #3).
    assert!(
        !home.join(".nexus42/creator-identities.json").exists(),
        "creator-identities.json must not be written by the local bootstrap"
    );
}

/// `creator register --local --name <n>` converges the same three-store end
/// state (identity active + workspace row) — `creator world create` succeeds
/// immediately after.
#[test]
fn register_local_converges_workspace_row() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("register")
        .arg("--local")
        .arg("--name")
        .arg("LocalParity")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Created persistent identity"))
        .stdout(predicate::str::contains("Local-only (no platform)"));

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("world")
        .arg("create")
        .arg("--title")
        .arg("Local World")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("World created"));
    // The identity-cache store is NOT written by `register --local` (AR-88
    // #3 / qc1 S#2): local display SSOT is `local_identities`, never the
    // platform cache file.
    assert!(
        !home.join(".nexus42/creator-identities.json").exists(),
        "creator-identities.json must not be written by the local bootstrap"
    );
}

// =============================================================================
// V1.176 P0 T2 (AR-89): idempotent re-register + partial-bootstrap recovery
// =============================================================================

/// Open the global identity store at `$HOME/.nexus42/state.db` (same path
/// the CLI resolves) and return the single persistent identity's creator id.
fn single_persistent_id(home: &std::path::Path) -> String {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let pool = nexus42::db::Schema::init(&home.join(".nexus42/state.db"))
            .await
            .expect("open global db");
        let rows = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(rows.len(), 1, "expected exactly one identity");
        rows[0].creator_id.clone()
    })
}

/// Delete the workspace `creators` row for `creator_id` — simulates the
/// DF-83 partial (identity present, workspace row missing).
fn delete_workspace_row(home: &std::path::Path, creator_id: &str) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let db_path = nexus42::paths::state_db_path(home, creator_id, "default");
        let pool = nexus42::db::Schema::init(&db_path)
            .await
            .expect("open workspace db");
        sqlx::query("DELETE FROM creators WHERE creator_id = ?")
            .bind(creator_id)
            .execute(&pool)
            .await
            .expect("delete workspace row");
    });
}

/// No-op success on `system identity create --persistent`: re-running the
/// same name exits 0, must NOT print "Created", and mints no second identity.
#[test]
fn persistent_identity_create_noop_does_not_claim_new_mint() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .arg("--name")
        .arg("NoopTester")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Created persistent identity"));
    let id = single_persistent_id(home);

    // Re-run the same name → no-op success: exit 0, no "Created", no new mint.
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .arg("--name")
        .arg("NoopTester")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("already converged"))
        .stdout(predicate::str::contains("Created").not());

    assert_eq!(single_persistent_id(home), id, "no second identity minted");
}

/// No-op success on `creator register --local`: re-running the same name
/// exits 0, must NOT print "Created", and mints no second identity.
#[test]
fn register_local_noop_does_not_claim_new_mint() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("register")
        .arg("--local")
        .arg("--name")
        .arg("LocalNoop")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Created persistent identity"));
    let id = single_persistent_id(home);

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("register")
        .arg("--local")
        .arg("--name")
        .arg("LocalNoop")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("already converged"))
        .stdout(predicate::str::contains("Created").not());

    assert_eq!(single_persistent_id(home), id, "no second identity minted");
}

/// Repair on `system identity create --persistent`: after the workspace row
/// is deleted (simulated partial), re-running the same name materializes the
/// row again (exit 0, repair named, no "Created").
#[test]
fn persistent_identity_create_repairs_missing_workspace_row() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .arg("--name")
        .arg("RepairTester")
        .env("HOME", home)
        .assert()
        .success();
    let id = single_persistent_id(home);
    delete_workspace_row(home, &id);

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .arg("--name")
        .arg("RepairTester")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("materialized"))
        .stdout(predicate::str::contains("Created").not());

    assert_eq!(single_persistent_id(home), id, "no second identity minted");
}

/// Repair on `creator register --local`: after the workspace row is deleted,
/// re-running the same name materializes the row again (exit 0, repair named,
/// no "Created").
#[test]
fn register_local_repairs_missing_workspace_row() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("register")
        .arg("--local")
        .arg("--name")
        .arg("LocalRepair")
        .env("HOME", home)
        .assert()
        .success();
    let id = single_persistent_id(home);
    delete_workspace_row(home, &id);

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("register")
        .arg("--local")
        .arg("--name")
        .arg("LocalRepair")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("materialized"))
        .stdout(predicate::str::contains("Created").not());

    assert_eq!(single_persistent_id(home), id, "no second identity minted");
}

/// Nameless `system identity create --persistent` converges the already-active
/// persistent identity (AR-89 #2): no second mint, no "Created" claim.
#[test]
fn persistent_identity_create_nameless_converges_active() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .arg("--name")
        .arg("NamelessBase")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Created persistent identity"));
    let id = single_persistent_id(home);

    // Nameless re-run → converge the active persistent identity (no mint).
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("already converged"))
        .stdout(predicate::str::contains("Created").not());

    assert_eq!(single_persistent_id(home), id, "no second identity minted");
}

/// `--anonymous` must NOT materialize a workspace row (AR-88 #5): the
/// ephemeral identity is active, but `creator world create` still fails its
/// FK precheck.
#[test]
fn anonymous_identity_does_not_materialize_workspace_row() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("anonymous")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Created anonymous identity"));

    // No workspace `creators` row exists for the anonymous identity — the
    // world-create FK precheck must fail (referenced creator not found).
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("world")
        .arg("create")
        .arg("--title")
        .arg("Anon World")
        .env("HOME", home)
        .assert()
        .failure()
        .stderr(predicate::str::contains("referenced creator"));
}

// =============================================================================
// V1.176 P0 T3 (AR-90): creator list local-identity visibility + --json
// =============================================================================

/// Seed a platform creator in the identity cache (`creator-identities.json`)
/// — exactly today's platform source for `creator list`.
fn seed_platform_cache(home: &std::path::Path, creator_id: &str, handle: &str, display_name: &str) {
    let dir = home.join(".nexus42");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("creator-identities.json");
    let cache = serde_json::json!({
        "creators": {
            creator_id: {
                "creator_id": creator_id,
                "handle": handle,
                "display_name": display_name,
            }
        }
    });
    std::fs::write(&path, serde_json::to_string_pretty(&cache).unwrap()).unwrap();
}

/// ORIGIN cell of one rendered human row, sliced at the header's ORIGIN
/// column offset. The CREATOR ID column is padded to the widest id in the
/// listing, so header and data rows share the same column layout — this
/// pins the ORIGIN cell itself, not a substring (a plain `contains("local")`
/// would already match `ctr_local*` ids).
fn origin_cell<'a>(line: &'a str, header_line: &str) -> &'a str {
    let col = header_line.find("ORIGIN").expect("ORIGIN header token");
    line.get(col..col + 8).unwrap_or("").trim()
}

/// Mixed local+platform listing (AR-90): a persistent local identity appears
/// marked `local` (HANDLE renders `-`, never `[local]`); a seeded platform row
/// keeps its id/handle/display byte-stable marked `platform`. `--json` emits
/// the pinned DTO array verbatim (`origin` key; nullable `handle`/`display_name`).
#[test]
fn creator_list_mixed_local_and_platform() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    // Platform row via the identity cache (today's platform source).
    seed_platform_cache(home, "ctr_platabc", "alice", "Alice Platform");

    // Local persistent row via the shared bootstrap helper.
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("register")
        .arg("--local")
        .arg("--name")
        .arg("Local Alice")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Created persistent identity"));
    let local_id = single_persistent_id(home);

    // Human default: additive ORIGIN column — local row marked, platform row
    // byte-stable (id/handle/display unchanged).
    let human = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("list")
        .env("HOME", home)
        .assert()
        .success();
    let stdout = String::from_utf8(human.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.contains("ORIGIN"),
        "table must gain the ORIGIN header: {stdout}"
    );
    // Column-aware ORIGIN pin: slice each row's ORIGIN cell at the header
    // column offset — a plain substring would already match `ctr_local*` ids
    // (which embed "local") and "Alice Platform" (which embeds "platform").
    let header_line = stdout
        .lines()
        .find(|l| l.contains("ORIGIN"))
        .expect("ORIGIN header line");
    let local_line = stdout
        .lines()
        .find(|l| l.contains(&local_id))
        .expect("local row rendered");
    assert!(
        local_line.contains("Local Alice"),
        "local row authoritative display name: {local_line}"
    );
    assert!(
        !local_line.contains("[local]"),
        "HANDLE must not be overloaded with [local]: {local_line}"
    );
    assert_eq!(
        origin_cell(local_line, header_line),
        "local",
        "ORIGIN cell on the local row: {local_line}"
    );
    let platform_line = stdout
        .lines()
        .find(|l| l.contains("ctr_platabc"))
        .expect("platform row rendered");
    assert!(
        platform_line.contains("alice") && platform_line.contains("Alice Platform"),
        "platform row byte-stable: {platform_line}"
    );
    assert_eq!(
        origin_cell(platform_line, header_line),
        "platform",
        "ORIGIN cell on the platform row: {platform_line}"
    );

    // `--json`: pinned DTO array verbatim.
    let json_out = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("list")
        .arg("--json")
        .env("HOME", home)
        .assert()
        .success();
    let json_text = String::from_utf8(json_out.get_output().stdout.clone()).unwrap();
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&json_text).expect("--json must emit a JSON array");
    assert_eq!(rows.len(), 2, "local + platform rows");
    for row in &rows {
        let obj = row.as_object().expect("row is an object");
        assert_eq!(obj.len(), 5, "exactly the pinned keys: {row}");
        for key in ["creator_id", "handle", "display_name", "active", "origin"] {
            assert!(obj.contains_key(key), "missing pinned key {key}: {row}");
        }
    }
    let local = rows
        .iter()
        .find(|r| r["creator_id"] == local_id)
        .expect("local row in json");
    assert_eq!(local["origin"], "local");
    assert!(
        local["handle"].is_null(),
        "local handle must be null: {local}"
    );
    assert_eq!(local["display_name"], "Local Alice");
    assert_eq!(local["active"], true);
    let platform = rows
        .iter()
        .find(|r| r["creator_id"] == "ctr_platabc")
        .expect("platform row in json");
    assert_eq!(platform["origin"], "platform");
    assert_eq!(platform["handle"], "alice");
    assert_eq!(platform["display_name"], "Alice Platform");
    assert_eq!(platform["active"], false);
}

/// `--anonymous` identities are ephemeral, not registered creators (AR-90
/// #2) — even while an anonymous identity is the active creator, it must not
/// appear in `creator list` on either surface (CLI-level leg).
#[test]
fn creator_list_excludes_anonymous_identity() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    // A persistent local row + a seeded platform row keep the listing
    // non-empty while the anonymous identity is active.
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("register")
        .arg("--local")
        .arg("--name")
        .arg("Local Alice")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Created persistent identity"));
    seed_platform_cache(home, "ctr_platabc", "alice", "Alice Platform");

    // The anonymous identity becomes the active creator (ephemeral — no
    // `local_identities` row, no workspace row).
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("anonymous")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Created anonymous identity"));

    let human = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("list")
        .env("HOME", home)
        .assert()
        .success();
    let stdout = String::from_utf8(human.get_output().stdout.clone()).unwrap();
    assert!(
        !stdout.contains("ctr_anon"),
        "anonymous identity must not appear in the human table: {stdout}"
    );

    let json_out = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("list")
        .arg("--json")
        .env("HOME", home)
        .assert()
        .success();
    let json_text = String::from_utf8(json_out.get_output().stdout.clone()).unwrap();
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&json_text).expect("--json must emit a JSON array");
    assert_eq!(
        rows.len(),
        2,
        "local + platform rows; anonymous excluded: {json_text}"
    );
    assert!(
        rows.iter().all(|r| !r["creator_id"]
            .as_str()
            .unwrap_or_default()
            .starts_with("ctr_anon")),
        "anonymous identity must not appear in --json: {json_text}"
    );
}

/// `creator list --json` on an empty identity surface emits `[]` (DTO
/// verbatim); the human default keeps the unchanged empty-state copy.
#[test]
fn creator_list_empty_states_json_and_human() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("list")
        .arg("--json")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("[]"));

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("list")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("No registered Creators found."));
}

/// `creator list --help` documents the new `--json` flag.
#[test]
fn creator_list_help_documents_json() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("list")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--json"));
}
// =============================================================================
// V1.176 P0 QC fix wave (qc2 S#2 / qc3 S-003): hermetic CLI pins for the
// shipped-but-unpinned legs + local-source failure mode
// =============================================================================

/// `creator list` must not materialize `~/.nexus42/state.db` when it is
/// absent (lazy-open pin, qc2 S#2 leg 1) — even while the platform cache
/// keeps the listing non-empty.
#[test]
fn creator_list_does_not_create_state_db_when_absent() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    seed_platform_cache(home, "ctr_platabc", "alice", "Alice Platform");

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("list")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("ctr_platabc"));

    assert!(
        !home.join(".nexus42/state.db").exists(),
        "creator list must not create state.db when it is absent"
    );
}

/// `Alice` vs `alice` are two distinct byte-exact names (AR-89 #1) — pinned
/// at the CLI level (qc2 S#2 leg 2): minting both creates two identities and
/// `creator list` shows both display names verbatim.
#[test]
fn creator_list_case_pair_is_byte_exact() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .arg("--name")
        .arg("Alice")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Created persistent identity"));
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .arg("--name")
        .arg("alice")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Created persistent identity"));

    let human = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("list")
        .env("HOME", home)
        .assert()
        .success();
    let stdout = String::from_utf8(human.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("Alice"), "byte-exact Alice row: {stdout}");
    assert!(stdout.contains("alice"), "byte-exact alice row: {stdout}");
}

/// Named 1-match on a *different* already-registered id is the allowed
/// session-selection path (AR-89 #2 / qc2 S#2 leg 3): re-running a name whose
/// single match is another identity activates that id — never a collision,
/// never a "Created" claim.
#[test]
fn persistent_identity_create_session_selection_activates_matched_id() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .arg("--name")
        .arg("TargetName")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Created persistent identity"));
    let target_id = single_persistent_id(home);

    // A second persistent identity becomes the active one.
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .arg("--name")
        .arg("OtherName")
        .env("HOME", home)
        .assert()
        .success();

    // Re-running the target name → single match → activate the matched id.
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .arg("--name")
        .arg("TargetName")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "already registered; set as active identity",
        ))
        .stdout(predicate::str::contains("Created").not());

    let listed = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("list")
        .env("HOME", home)
        .assert()
        .success();
    let stdout = String::from_utf8(listed.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.contains(&format!("{target_id} [local] TargetName (active)")),
        "matched identity must be active: {stdout}"
    );
}

/// Nameless `system identity create --persistent` on an empty HOME mints a
/// nameless persistent identity (AR-89 #2 / qc2 S#2 leg 4): no name, and the
/// workspace row `display_name` falls back to the `creator_id` (AR-88 #4).
#[test]
fn persistent_identity_create_nameless_first_mint() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Created persistent identity"));

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let pool = nexus42::db::Schema::init(&home.join(".nexus42/state.db"))
            .await
            .expect("open global db");
        let rows = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(rows.len(), 1, "nameless first mint creates one identity");
        assert!(
            rows[0].display_name.is_none(),
            "nameless mint stores no name"
        );

        // The workspace db is per-creator + per-workspace-slug, resolved under
        // the test's HOME (the child binary's HOME env does not change this
        // process), so use `paths::state_db_path` with the explicit home.
        let db_path = nexus42::paths::state_db_path(home, &rows[0].creator_id, "default");
        let workspace_pool = nexus42::db::Schema::init(&db_path)
            .await
            .expect("open workspace db");
        let display_name: String =
            sqlx::query_scalar("SELECT display_name FROM creators WHERE creator_id = ?")
                .bind(&rows[0].creator_id)
                .fetch_one(&workspace_pool)
                .await
                .expect("query creators display_name");
        assert_eq!(
            display_name, rows[0].creator_id,
            "nameless row display_name = creator_id"
        );
    });
}

/// `creator list` degrades with a warning when the local source
/// (`~/.nexus42/state.db`) exists but is unreadable (qc3 S-003): platform
/// rows still render, stderr carries an honest note, and exit stays 0.
#[test]
fn creator_list_degrades_when_local_source_unreadable() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    seed_platform_cache(home, "ctr_platabc", "alice", "Alice Platform");

    // Corrupt the local store: garbage bytes make the read-only open fail.
    let dir = home.join(".nexus42");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("state.db"), b"this is not a sqlite database file").unwrap();

    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("list")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("ctr_platabc"))
        .stderr(predicate::str::contains("warning"))
        .stderr(predicate::str::contains("local identities unavailable"));

    // `--json` keeps its machine contract on the same degraded path.
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("list")
        .arg("--json")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"origin\": \"platform\""))
        .stderr(predicate::str::contains("local identities unavailable"));
}
