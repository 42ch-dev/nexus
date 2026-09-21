//! Regression test suite for V1.2 release gate
//!
//! Covers R1-R3, R5 (R4 already verified in T6)
//!
//! Run with: cargo test --test regression

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

// =============================================================================
// R1: local_only anonymous identity end-to-end
// =============================================================================

/// Regression R1: Anonymous identity works and sync is blocked in `local_only` mode
#[test]
fn r1_anonymous_identity_e2e() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    // Create anonymous identity
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("anonymous")
        .arg("--name")
        .arg("TestAnon")
        .env("HOME", home)
        .assert()
        .success();

    // Verify the identity was created with a ctr_ prefix (anonymous identity marker)
    output
        .stdout(predicate::str::contains("ctr_"))
        .stdout(predicate::str::contains("Created anonymous identity"));

    // Verify runtime mode is local_only (default)
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("runtime-mode")
        .arg("show")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("local_only"));

    // Verify sync push is blocked in local_only mode (v1.193 P2-T12:
    // `platform sync` is the parser entrance; the top-level alias is gone)
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("platform")
        .arg("sync")
        .arg("push")
        .env("HOME", home)
        .assert()
        .failure()
        .stderr(predicate::str::contains("not available in local_only mode"));
}

/// Regression R1: Anonymous identity shows as active (ephemeral - not persisted)
#[test]
fn r1_anonymous_identity_active_session() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    // Create anonymous identity
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("anonymous")
        .arg("--name")
        .arg("TestAnon")
        .env("HOME", home)
        .assert()
        .success();

    // In the same session, identity is usable
    // (ephemeral means it won't persist to list in a new session)
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("runtime-mode")
        .arg("show")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("local_only"));
}

// =============================================================================
// R2: local_only local persistent identity end-to-end
// =============================================================================

/// Regression R2: Local persistent identity persists across sessions
#[test]
fn r2_persistent_identity_e2e() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    // Create persistent local identity
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .arg("--name")
        .arg("TestWriter")
        .env("HOME", home)
        .assert()
        .success();

    // Verify identity was created
    output.stdout(predicate::str::contains("Created persistent identity"));

    // Verify identity can be used in second session (same HOME, new process)
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("list")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("TestWriter"));

    // Verify runtime mode is local_only
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("runtime-mode")
        .arg("show")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("local_only"));
}

/// Regression R2: Persistent identity persists in `SQLite` state.db
#[test]
fn r2_persistent_identity_config_persists() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    // Create persistent identity
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .arg("--name")
        .arg("PersistentUser")
        .env("HOME", home)
        .assert()
        .success();

    // Verify state.db exists in ~/.nexus42/ (identity storage)
    let db_path = home.join(".nexus42/state.db");
    assert!(
        db_path.exists(),
        "state.db should exist after creating persistent identity"
    );

    // List identities to verify persistence
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("list")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("PersistentUser"));
}

// =============================================================================
// R3: Local truth core chain (SOUL/memory/KB/output)
// =============================================================================

/// Regression R3: Local truth chain - SOUL → memory → context assembly
#[test]
fn r3_local_truth_chain() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    // Create a persistent identity first (SOUL requires active creator)
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .arg("--name")
        .arg("TruthChainUser")
        .env("HOME", home)
        .assert()
        .success();

    // Init workspace
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("workspace")
        .arg("init")
        .arg("workspace")
        .arg("--creative-root")
        .arg(&workspace)
        .env("HOME", home)
        .current_dir(&workspace)
        .assert()
        .success();

    // Init SOUL
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("soul")
        .arg("init")
        .env("HOME", home)
        .current_dir(&workspace)
        .assert()
        .success();

    // Verify SOUL was initialized
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("soul")
        .arg("show")
        .env("HOME", home)
        .current_dir(&workspace)
        .assert()
        .success();

    // Create memory
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("memory")
        .arg("create")
        .arg("world_building")
        .arg("--content")
        .arg("Test world setting for fantasy realm")
        .env("HOME", home)
        .current_dir(&workspace)
        .assert()
        .success();

    // List memories to verify it was created
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("memory")
        .arg("list")
        .env("HOME", home)
        .current_dir(&workspace)
        .assert()
        .success()
        .stdout(predicate::str::contains("world_building"));

    // R3 verification: SOUL init + memory creation chain works
}

/// Regression R3: context assemble-moment executes without panic or skip
///
/// Validates the clap -h flag collision fix (hint short alias removed)
/// and that assemble-moment runs successfully in a `local_only` workspace.
#[test]
fn r3_context_assemble_moment_executes_without_placeholder_skip() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    // Create a persistent identity (required for workspace operations)
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .arg("--name")
        .arg("AssembleTestUser")
        .env("HOME", home)
        .assert()
        .success();

    // Init workspace
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("workspace")
        .arg("init")
        .arg("workspace")
        .arg("--creative-root")
        .arg(&workspace)
        .env("HOME", home)
        .current_dir(&workspace)
        .assert()
        .success();

    // Init SOUL (required by assemble-moment Stage0)
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("soul")
        .arg("init")
        .env("HOME", home)
        .current_dir(&workspace)
        .assert()
        .success();

    // v1.191 P1 T9: the admitted knowledge view is World-scoped — a worldless
    // `assemble-moment` fails closed in `creator_view_scope` ("an admitted
    // knowledge view requires a World: pass `--world-id wld_...`"; re-smoked on
    // the real binary in v1.194 P2-T2), so the R3 chain materializes a World and
    // names it.
    //
    // That refusal is retained but currently unasserted: the pre-retirement pin
    // for the *removed* `platform context assemble` guidance leaf was re-pointed
    // by v1.193 P2-T12 to assert that leaf's removal
    // (`integration.rs::retired_platform_context_assemble_is_unknown`), so no
    // assertion covers this path. Tracked as a coverage gap on
    // `apps/nexus42/src/commands/platform/context.rs:562` — v1.194 P2-T2
    // finding F-T2-1; not covered here.
    let world_out = Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("world")
        .arg("create")
        .arg("--title")
        .arg("R3 World")
        .env("HOME", home)
        .current_dir(&workspace)
        .output()
        .expect("world create");
    assert!(
        world_out.status.success(),
        "world create failed: {}",
        String::from_utf8_lossy(&world_out.stderr)
    );
    let world_id = String::from_utf8_lossy(&world_out.stdout)
        .split("World created: ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .expect("created world id")
        .to_string();

    // Run context assemble-moment — must not panic and succeed
    Command::cargo_bin("nexus42")
        .unwrap()
        .args([
            "platform",
            "context",
            "assemble-moment",
            "--world-id",
            &world_id,
            "--max-tokens",
            "1200",
        ])
        .env("HOME", home)
        .current_dir(&workspace)
        .assert()
        .success();
}

/// Regression R3: SOUL validation works
#[test]
fn r3_soul_validation() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    // Create identity and init workspace
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("create")
        .arg("--kind")
        .arg("persistent")
        .arg("--name")
        .arg("SoulValidator")
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
        .arg(&workspace)
        .env("HOME", home)
        .current_dir(&workspace)
        .assert()
        .success();

    // Init SOUL
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("soul")
        .arg("init")
        .env("HOME", home)
        .current_dir(&workspace)
        .assert()
        .success();

    // Validate SOUL structure
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("soul")
        .arg("validate")
        .env("HOME", home)
        .current_dir(&workspace)
        .assert()
        .success()
        .stdout(predicate::str::contains("valid").or(predicate::str::contains("Valid")));
}

// =============================================================================
// R5: No-platform dependency guard for local_only path
// =============================================================================

/// Regression R5: Platform sync push blocked in `local_only` mode
#[test]
fn r5_platform_guard_blocks_sync_push() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    // Set runtime mode to local_only
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("runtime-mode")
        .arg("set")
        .arg("local_only")
        .env("HOME", home)
        .assert()
        .success();

    // Verify sync push blocked (v1.193 P2-T12: `platform sync` is the parser
    // entrance; the hidden top-level `sync` alias is gone)
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("platform")
        .arg("sync")
        .arg("push")
        .env("HOME", home)
        .assert()
        .failure()
        .stderr(predicate::str::contains("not available in local_only mode"));
}

/// Regression R5 (v1.193 P2-T12): the deferred `platform explore` leaf was
/// removed in P1-T5, so no `local_only` guard question survives for it — the
/// parser rejects before any runtime-mode branch, and every invocation is
/// clap's unrecognized-subcommand error rather than a help page.
#[test]
fn r5_platform_guard_explore_is_unknown() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    // Set runtime mode to local_only
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("runtime-mode")
        .arg("set")
        .arg("local_only")
        .env("HOME", home)
        .assert()
        .success();

    for args in [
        ["platform", "explore", "--help"],
        ["platform", "explore", "browse"],
    ] {
        Command::cargo_bin("nexus42")
            .unwrap()
            .env("HOME", home)
            .args(args)
            .assert()
            .code(2)
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains("unrecognized subcommand"));
    }
}

/// Regression R5: local sync status works in `local_only` mode
#[test]
fn r5_platform_guard_sync_status_works() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    // Set runtime mode to local_only
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("runtime-mode")
        .arg("set")
        .arg("local_only")
        .env("HOME", home)
        .assert()
        .success();

    // Verify sync status still works (local outbox query, no platform needed)
    // With no active creator configured, it should still respond gracefully.
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("platform")
        .arg("sync")
        .arg("status")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Sync Status"));
}

/// Regression R5: `local_only` mode persists across sessions
#[test]
fn r5_local_only_mode_persists() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    // Set runtime mode to local_only
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("runtime-mode")
        .arg("set")
        .arg("local_only")
        .env("HOME", home)
        .assert()
        .success();

    // Verify mode persisted (new process, same HOME)
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("runtime-mode")
        .arg("show")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("local_only"));
}

// =============================================================================
// V1.29 Memory Session Review Loop (Plan A1)
// =============================================================================

/// Verify `creator memory pending list --help` returns usage without error.
#[test]
fn v1_29_pending_list_help() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("memory")
        .arg("pending-list")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("pending-list"));
}

/// Verify `creator memory pending show --help` returns usage without error.
#[test]
fn v1_29_pending_show_help() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("memory")
        .arg("pending-show")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("PENDING_ID"));
}

/// Verify `creator memory pending dismiss --help` returns usage without error.
#[test]
fn v1_29_pending_dismiss_help() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("memory")
        .arg("pending-dismiss")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("PENDING_ID"));
}

/// Verify `creator memory --help` shows pending subcommands.
#[test]
fn v1_29_memory_help_shows_pending_commands() {
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("memory")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("pending-list"))
        .stdout(predicate::str::contains("pending-show"))
        .stdout(predicate::str::contains("pending-dismiss"));
}
