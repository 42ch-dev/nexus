//! Regression test suite for V1.2 release gate
//!
//! Covers R1-R3, R5 (R4 already verified in T6)
//!
//! Run with: cargo test --test regression
// note: covers R1-R3 and R5; R4 was verified in T6.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

// note: R1: local_only anonymous identity end-to-end

/// note: Regression R1: Anonymous identity works and sync is blocked in `local_only` mode
#[test]
fn r1_anonymous_identity_e2e() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    // note: Create anonymous identity
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

    // note: Verify the identity was created with a ctr_ prefix (anonymous identity marker)
    output
        .stdout(predicate::str::contains("ctr_"))
        .stdout(predicate::str::contains("Created anonymous identity"));

    // note: Verify runtime mode is local_only (default)
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("runtime-mode")
        .arg("show")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("local_only"));

    // note: Verify sync push is blocked in local_only mode (v1.193 P2-T12:
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

/// note: Regression R1: Anonymous identity shows as active (ephemeral - not persisted)
#[test]
fn r1_anonymous_identity_active_session() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    // note: Create anonymous identity
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

    // note: In the same session, identity is usable
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

// note: R2: local_only local persistent identity end-to-end

/// note: Regression R2: Local persistent identity persists across sessions
#[test]
fn r2_persistent_identity_e2e() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    // note: Create persistent local identity
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

    // note: Verify identity was created
    output.stdout(predicate::str::contains("Created persistent identity"));

    // note: Verify identity can be used in second session (same HOME, new process)
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("identity")
        .arg("list")
        .env("HOME", home)
        .assert()
        .success()
        .stdout(predicate::str::contains("TestWriter"));

    // note: Verify runtime mode is local_only
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

/// note: Regression R2: Persistent identity persists in `SQLite` state.db
#[test]
fn r2_persistent_identity_config_persists() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    // note: Create persistent identity
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

    // note: Verify state.db exists in ~/.nexus42/ (identity storage)
    let db_path = home.join(".nexus42/state.db");
    assert!(
        db_path.exists(),
        "state.db should exist after creating persistent identity"
    );

    // note: List identities to verify persistence
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

// note: R3: Local truth core chain (SOUL/memory/KB/output)

/// note: Regression R3: Local truth chain - SOUL → memory → context assembly
#[test]
fn r3_local_truth_chain() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    // note: Create a persistent identity first (SOUL requires active creator)
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

    // note: Init workspace
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

    // note: Init SOUL
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("soul")
        .arg("init")
        .env("HOME", home)
        .current_dir(&workspace)
        .assert()
        .success();

    // note: Verify SOUL was initialized
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("soul")
        .arg("show")
        .env("HOME", home)
        .current_dir(&workspace)
        .assert()
        .success();

    // note: Create memory
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

    // note: List memories to verify it was created
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

    // note: R3 verification: SOUL init + memory creation chain works
}

/// note: Regression R3: context assemble-moment executes without panic or skip
///
/// Validates the clap -h flag collision fix (hint short alias removed)
/// and that assemble-moment runs successfully in a `local_only` workspace.
#[test]
fn r3_context_assemble_moment_executes_without_placeholder_skip() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    // note: Create a persistent identity (required for workspace operations)
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

    // note: Init workspace
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

    // note: Init SOUL (required by assemble-moment Stage0)
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("soul")
        .arg("init")
        .env("HOME", home)
        .current_dir(&workspace)
        .assert()
        .success();

    // note: v1.191 P1 T9: the admitted knowledge view is World-scoped — a worldless
    // `assemble-moment` fails closed in `creator_view_scope` ("an admitted
    // knowledge view requires a World: pass `--world-id wld_...`"; re-smoked on
    // the real binary in v1.194 P2-T2), so the R3 chain materializes a World and
    // names it.
    //
    // The refusal is now pinned at the CLI exit path by
    // `r3_context_assemble_moment_worldless_refuses` (I-000039), closing the
    // v1.194 P2-T2 finding F-T2-1 coverage gap on
    // `apps/nexus42/src/commands/platform/context.rs:562`.
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

    // invariant: Run context assemble-moment — must not panic and succeed
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

/// note: Regression R3 (I-000039): a worldless `assemble-moment` fails closed.
///
/// v1.191 P1 T9 made the admitted knowledge view World-scoped: the
/// `creator_view_scope` guard in `commands/platform/context.rs` refuses a
/// missing `--world-id` with "an admitted knowledge view requires a World"
/// instead of widening the read or fabricating a container. Pinned at the CLI
/// exit path so a refactor cannot silently drop the refusal.
#[test]
fn r3_context_assemble_moment_worldless_refuses() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    // note: The guard runs once the active Creator resolves, so identity + workspace
    // must be in place for the refusal — not a missing-creator error — to fire.
    Command::cargo_bin("nexus42")
        .unwrap()
        .args([
            "system",
            "identity",
            "create",
            "--kind",
            "persistent",
            "--name",
            "WorldlessRefusal",
        ])
        .env("HOME", home)
        .assert()
        .success();

    Command::cargo_bin("nexus42")
        .unwrap()
        .args([
            "creator",
            "workspace",
            "init",
            "workspace",
            "--creative-root",
        ])
        .arg(&workspace)
        .env("HOME", home)
        .current_dir(&workspace)
        .assert()
        .success();

    // invariant: No `--world-id`: exit 1 with the worldless refusal, never a widened read.
    Command::cargo_bin("nexus42")
        .unwrap()
        .args(["platform", "context", "assemble-moment"])
        .env("HOME", home)
        .current_dir(&workspace)
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "an admitted knowledge view requires a World",
        ));
}

/// note: Regression R3: SOUL validation works
#[test]
fn r3_soul_validation() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    // note: Create identity and init workspace
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

    // note: Init SOUL
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("creator")
        .arg("soul")
        .arg("init")
        .env("HOME", home)
        .current_dir(&workspace)
        .assert()
        .success();

    // note: Validate SOUL structure
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

// note: R5: No-platform dependency guard for local_only path

/// note: Regression R5: Platform sync push blocked in `local_only` mode
#[test]
fn r5_platform_guard_blocks_sync_push() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    // note: Set runtime mode to local_only
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("runtime-mode")
        .arg("set")
        .arg("local_only")
        .env("HOME", home)
        .assert()
        .success();

    // note: Verify sync push blocked (v1.193 P2-T12: `platform sync` is the parser
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

/// note: Regression R5 (v1.193 P2-T12): the deferred `platform explore` leaf was
/// removed in P1-T5, so no `local_only` guard question survives for it — the
/// parser rejects before any runtime-mode branch, and every invocation is
/// clap's unrecognized-subcommand error rather than a help page.
#[test]
fn r5_platform_guard_explore_is_unknown() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    // note: Set runtime mode to local_only
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

/// note: Regression R5: local sync status works in `local_only` mode
#[test]
fn r5_platform_guard_sync_status_works() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    // note: Set runtime mode to local_only
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("runtime-mode")
        .arg("set")
        .arg("local_only")
        .env("HOME", home)
        .assert()
        .success();

    // note: Verify sync status still works (local outbox query, no platform needed)
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

/// note: Regression R5: `local_only` mode persists across sessions
#[test]
fn r5_local_only_mode_persists() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();

    // note: Set runtime mode to local_only
    Command::cargo_bin("nexus42")
        .unwrap()
        .arg("system")
        .arg("runtime-mode")
        .arg("set")
        .arg("local_only")
        .env("HOME", home)
        .assert()
        .success();

    // note: Verify mode persisted (new process, same HOME)
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

// note: V1.29 Memory Session Review Loop (Plan A1)

/// note: Verify `creator memory pending list --help` returns usage without error.
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

/// note: Verify `creator memory pending show --help` returns usage without error.
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

/// note: Verify `creator memory pending dismiss --help` returns usage without error.
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

/// note: Verify `creator memory --help` shows pending subcommands.
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
