//! P1-T2 `world_kb` contract tests (core service semantics).

use nexus_contracts::{
    CoreProviderJournalWrite, CoreProviderJournalWriteStatus, CoreProviderOperationStatus,
    WorldKbPatchEntityRequest,
};
use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService};
use nexus_local_db::open_pool_read_only;
use nexus_local_db::writer_protocol::{init_engine_pool, GuardedPoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use tempfile::TempDir;

const CREATOR: &str = "test_creator";
const SLUG: &str = "default";
const OWNED_WORLD: &str = "wld_owned";
const FOREIGN_WORLD: &str = "wld_foreign";

struct Fixture {
    tmp: TempDir,
    core: CoreService,
    principal: nexus_core::Principal,
}

async fn seed_world(pool: &SqlitePool, world_id: &str, owner: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) VALUES (?, 'Test', 'active', datetime('now'), '{}')",
    )
    .bind(owner)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO narrative_worlds (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json) VALUES (?, 'wrk', ?, 't', 's', 'active', 'private', 'manual', '{}')",
    )
    .bind(world_id)
    .bind(owner)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_kb(
    pool: &SqlitePool,
    id: &str,
    world_id: &str,
    name: &str,
    status: &str,
    revision: Option<i64>,
    modules: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO kb_key_blocks (key_block_id, world_id, block_type, canonical_name, status, revision, modules_json, created_at, updated_at) VALUES (?, ?, 'character', ?, ?, ?, ?, datetime('now'), datetime('now'))",
    )
    .bind(id)
    .bind(world_id)
    .bind(name)
    .bind(status)
    .bind(revision)
    .bind(modules)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_pending(
    pool: &SqlitePool,
    job_id: &str,
    world_id: &str,
    name: &str,
    created_at: &str,
) {
    // kb_extract_jobs is engine-writer-only (writer protocol §4).
    sqlx::query(
        "INSERT INTO kb_extract_jobs (job_id, creator_id, workspace_id, work_entry_id, world_id, status, promotion_status, proposed_payload, block_type_guess, canonical_name_guess, version, created_at) VALUES (?, ?, 'ws', ?, ?, 'done', 'pending', '{}', 'character', ?, 0, ?)",
    )
    .bind(job_id)
    .bind(CREATOR)
    .bind(format!("work_{job_id}"))
    .bind(world_id)
    .bind(name)
    .bind(created_at)
    .execute(pool)
    .await
    .unwrap();
}

async fn read_only_pool(user_home: &Path) -> SqlitePool {
    let db_path = nexus_home_layout::workspace_state_db_path(user_home, CREATOR, SLUG);
    open_pool_read_only(&db_path).await.expect("read-only pool")
}

/// Seed the contract fixture rows: owned + foreign worlds, KB rows in every
/// status under test (deleted/confirmed/merged + NULL modules), and two
/// pending candidate jobs.
async fn seed_fixture_rows(pool: &SqlitePool) {
    seed_world(pool, OWNED_WORLD, CREATOR).await;
    seed_world(pool, FOREIGN_WORLD, "other_creator").await;
    seed_kb(
        pool,
        "kb_dead",
        OWNED_WORLD,
        "Dead",
        "deleted",
        Some(1),
        None,
    )
    .await;
    seed_kb(
        pool,
        "kb_mod",
        OWNED_WORLD,
        "Mod",
        "confirmed",
        Some(0),
        Some(r#"{"mental":{"goals":["old"]}}"#),
    )
    .await;
    seed_kb(
        pool,
        "kb_cas",
        OWNED_WORLD,
        "Cas",
        "confirmed",
        Some(2),
        None,
    )
    .await;
    seed_kb(
        pool,
        "kb_merged",
        OWNED_WORLD,
        "Merged",
        "merged",
        Some(1),
        None,
    )
    .await;
    seed_kb(
        pool,
        "kb_nullmod",
        OWNED_WORLD,
        "NullMod",
        "confirmed",
        Some(1),
        None,
    )
    .await;
    sqlx::query("UPDATE kb_key_blocks SET modules_json = NULL WHERE key_block_id = 'kb_nullmod'")
        .execute(pool)
        .await
        .unwrap();
    seed_pending(
        pool,
        "xj_job1",
        OWNED_WORLD,
        "Cand1",
        "2020-01-01T00:00:01Z",
    )
    .await;
    seed_pending(
        pool,
        "xj_job2",
        OWNED_WORLD,
        "Cand2",
        "2020-01-01T00:00:02Z",
    )
    .await;
}

async fn setup() -> Fixture {
    let tmp = TempDir::new().unwrap();
    let user_home = tmp.path().to_path_buf();
    let nexus_home = user_home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).unwrap();
    let op = nexus_home_layout::operational_workspace_dir(&user_home, CREATOR, SLUG);
    std::fs::create_dir_all(&op).unwrap();
    std::fs::write(
        nexus_home.join("config.toml"),
        format!(
            "active_creator_id = \"{CREATOR}\"
[active_workspace_slug_by_creator]
\"{CREATOR}\" = \"{SLUG}\""
        ),
    )
    .unwrap();
    let db_path = nexus_home_layout::workspace_state_db_path(&user_home, CREATOR, SLUG);
    {
        let guarded = init_engine_pool(&db_path, CREATOR, GuardedPoolOptions::default())
            .await
            .unwrap();
        let pool = guarded.clone_pool();
        seed_fixture_rows(&pool).await;
        pool.close().await;
    }

    let core = CoreService::open(CoreOpenOptions {
        user_home,
        access: CoreAccess::EngineOwner,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    Fixture {
        tmp,
        core,
        principal,
    }
}

#[tokio::test]
async fn world_kb_contract() {
    let fx = setup().await;

    let err = fx
        .core
        .world_kb_graph(&fx.principal, FOREIGN_WORLD.to_string(), false)
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::Forbidden { .. }));

    let err = fx
        .core
        .world_kb_graph(&fx.principal, "wld_missing".to_string(), false)
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::NotFound { .. }));

    assert_patch_contract(&fx).await;
    assert_candidates_contract(&fx).await;
}

/// Patch-path contract: create-on-absent, input validation, terminal-status
/// rejection, module merge, and OCC conflict/retry behavior.
async fn assert_patch_contract(fx: &Fixture) {
    let req: WorldKbPatchEntityRequest = serde_json::from_value(serde_json::json!({
        "entity_id": "kb_abc123",
        "expected_version": 0,
        "patch": {"title": "New Hero", "block_type": "character"}
    }))
    .unwrap();
    let created = fx
        .core
        .patch_world_kb_entity(&fx.principal, OWNED_WORLD.to_string(), req)
        .await
        .unwrap();
    assert_eq!(created.version, 1);

    let bad: WorldKbPatchEntityRequest = serde_json::from_value(serde_json::json!({
        "entity_id": "kb_def456",
        "expected_version": 0,
        "patch": {"title": "   ", "block_type": "character"}
    }))
    .unwrap();
    let err = fx
        .core
        .patch_world_kb_entity(&fx.principal, OWNED_WORLD.to_string(), bad)
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::WorldKbValidation(_)));

    let bad_id: WorldKbPatchEntityRequest = serde_json::from_value(serde_json::json!({
        "entity_id": "not_kb",
        "expected_version": 0,
        "patch": {"title": "X", "block_type": "character"}
    }))
    .unwrap();
    let err = fx
        .core
        .patch_world_kb_entity(&fx.principal, OWNED_WORLD.to_string(), bad_id)
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::WorldKbValidation(_)));

    let req: WorldKbPatchEntityRequest = serde_json::from_value(serde_json::json!({
        "entity_id": "kb_dead",
        "expected_version": 1,
        "patch": {"title": "Nope"}
    }))
    .unwrap();
    let err = fx
        .core
        .patch_world_kb_entity(&fx.principal, OWNED_WORLD.to_string(), req)
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::WorldKbValidation(_)));

    let req: WorldKbPatchEntityRequest = serde_json::from_value(serde_json::json!({
        "entity_id": "kb_mod",
        "expected_version": 0,
        "patch": {"modules": {"mental": {"goals": ["new"]}}}
    }))
    .unwrap();
    let resp = fx
        .core
        .patch_world_kb_entity(&fx.principal, OWNED_WORLD.to_string(), req)
        .await
        .unwrap();
    let mental = resp.entity.modules.get("mental").expect("mental");
    assert_eq!(mental.get("goals"), Some(&serde_json::json!(["new"])));

    let stale: WorldKbPatchEntityRequest = serde_json::from_value(serde_json::json!({
        "entity_id": "kb_cas",
        "expected_version": 1,
        "patch": {"title": "Stale"}
    }))
    .unwrap();
    let err = fx
        .core
        .patch_world_kb_entity(&fx.principal, OWNED_WORLD.to_string(), stale)
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::WorldKbConflict(_)));

    let ok: WorldKbPatchEntityRequest = serde_json::from_value(serde_json::json!({
        "entity_id": "kb_cas",
        "expected_version": 2,
        "patch": {"title": "Fresh"}
    }))
    .unwrap();
    let resp = fx
        .core
        .patch_world_kb_entity(&fx.principal, OWNED_WORLD.to_string(), ok)
        .await
        .unwrap();
    assert_eq!(resp.version, 3);
}

/// Candidates contract: keyset pagination, malformed cursor rejection, and
/// foreign-world denial.
async fn assert_candidates_contract(fx: &Fixture) {
    let page1 = fx
        .core
        .world_kb_candidates(&fx.principal, OWNED_WORLD.to_string(), Some(1), None)
        .await
        .unwrap();
    assert_eq!(page1.items.len(), 1);
    assert!(page1.pagination.has_more);
    let cursor = page1.pagination.next_cursor.clone().expect("next cursor");

    let page2 = fx
        .core
        .world_kb_candidates(
            &fx.principal,
            OWNED_WORLD.to_string(),
            Some(1),
            Some(cursor),
        )
        .await
        .unwrap();
    assert_eq!(page2.items.len(), 1);

    let err = fx
        .core
        .world_kb_candidates(
            &fx.principal,
            OWNED_WORLD.to_string(),
            None,
            Some("kbp:bad".to_string()),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::InvalidInput { .. }));

    let err = fx
        .core
        .world_kb_candidates(
            &fx.principal,
            FOREIGN_WORLD.to_string(),
            None,
            Some("kbp:2020-01-01T00:00:00Z|xj_bad".to_string()),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::Forbidden { .. }));
}
#[tokio::test]
async fn world_kb_contract_merged_terminal_and_modules() {
    let fx = setup().await;
    let merged: WorldKbPatchEntityRequest = serde_json::from_value(serde_json::json!({
        "entity_id": "kb_merged",
        "expected_version": 1,
        "patch": {"title": "Nope"}
    }))
    .unwrap();
    let err = fx
        .core
        .patch_world_kb_entity(&fx.principal, OWNED_WORLD.to_string(), merged)
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::WorldKbValidation(_)));

    let absent: WorldKbPatchEntityRequest = serde_json::from_value(serde_json::json!({
        "entity_id": "kb_nullmod",
        "expected_version": 2,
        "patch": {"modules": {"mental": {"goals": ["a"]}}}
    }))
    .unwrap();
    let resp = fx
        .core
        .patch_world_kb_entity(&fx.principal, OWNED_WORLD.to_string(), absent)
        .await
        .unwrap();
    assert!(resp.entity.modules.contains_key("mental"));
}

#[tokio::test]
async fn world_kb_contract_cas_reports_committed_revision() {
    let fx = setup().await;
    let stale: WorldKbPatchEntityRequest = serde_json::from_value(serde_json::json!({
        "entity_id": "kb_cas",
        "expected_version": 1,
        "patch": {"title": "Stale"}
    }))
    .unwrap();
    let err = fx
        .core
        .patch_world_kb_entity(&fx.principal, OWNED_WORLD.to_string(), stale)
        .await
        .unwrap_err();
    let CoreError::WorldKbConflict(details) = err else {
        panic!("expected conflict")
    };
    assert_eq!(details.current_version, 2);
}

#[tokio::test]
async fn world_kb_contract_patch_event_atomicity() {
    let fx = setup().await;
    let pool = read_only_pool(fx.tmp.path()).await;
    let before: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(sequence), 0) FROM core_changes")
        .fetch_one(&pool)
        .await
        .unwrap();
    let req: WorldKbPatchEntityRequest = serde_json::from_value(serde_json::json!({
        "entity_id": "kb_abc124",
        "expected_version": 0,
        "patch": {"title": "Evt", "block_type": "character"}
    }))
    .unwrap();
    let resp = fx
        .core
        .patch_world_kb_entity(&fx.principal, OWNED_WORLD.to_string(), req)
        .await
        .unwrap();
    let row: (i64, Option<String>) = sqlx::query_as(
        "SELECT sequence, resource_revision FROM core_changes WHERE sequence > ? AND resource_id = 'kb_abc124' ORDER BY sequence DESC LIMIT 1",
    )
    .bind(before)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.1.as_deref(), Some("1"));
    assert_eq!(resp.version, 1);
}

#[tokio::test]
async fn world_kb_contract_stale_principal_after_config_change() {
    let fx = setup().await;
    let nexus_home = fx.tmp.path().join(".nexus42");
    std::fs::write(
        nexus_home.join("config.toml"),
        "active_creator_id = \"other_creator\"\n[active_workspace_slug_by_creator]\n\"other_creator\" = \"default\"\n",
    )
    .unwrap();
    let err = fx
        .core
        .world_kb_graph(&fx.principal, OWNED_WORLD.to_string(), false)
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::AuthRequired));
}

#[tokio::test]
async fn world_kb_contract_legacy_json_open() {
    let tmp = TempDir::new().unwrap();
    let user_home = tmp.path().to_path_buf();
    let nexus_home = user_home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).unwrap();
    let op = nexus_home_layout::operational_workspace_dir(&user_home, CREATOR, SLUG);
    std::fs::create_dir_all(&op).unwrap();
    std::fs::write(
        nexus_home.join("config.json"),
        format!(
            r#"{{"active_creator_id":"{CREATOR}","active_workspace_slug_by_creator":{{"{CREATOR}":"{SLUG}"}}}}"#
        ),
    )
    .unwrap();
    let core = CoreService::open(CoreOpenOptions {
        user_home,
        access: CoreAccess::EngineOwner,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    let _ = core
        .world_kb_graph(&principal, OWNED_WORLD.to_string(), false)
        .await
        .unwrap_err();
}

#[tokio::test]
async fn world_kb_contract_stored_revision_exactness() {
    let fx = setup().await;
    let pool = read_only_pool(fx.tmp.path()).await;

    let create_req: WorldKbPatchEntityRequest = serde_json::from_value(serde_json::json!({
        "entity_id": "kb_aabbccdd",
        "expected_version": 0,
        "patch": {"title": "Fresh", "block_type": "character"}
    }))
    .unwrap();
    let created = fx
        .core
        .patch_world_kb_entity(&fx.principal, OWNED_WORLD.to_string(), create_req)
        .await
        .unwrap();
    assert_eq!(created.version, 1);
    let stored_after_create: i64 =
        sqlx::query_scalar("SELECT revision FROM kb_key_blocks WHERE key_block_id = 'kb_aabbccdd'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        stored_after_create, 1,
        "create-on-absent must persist revision exactly 1"
    );

    let patch_req: WorldKbPatchEntityRequest = serde_json::from_value(serde_json::json!({
        "entity_id": "kb_aabbccdd",
        "expected_version": 1,
        "patch": {"title": "Fresh v2"}
    }))
    .unwrap();
    let patched = fx
        .core
        .patch_world_kb_entity(&fx.principal, OWNED_WORLD.to_string(), patch_req)
        .await
        .unwrap();
    assert_eq!(patched.version, 2);
    let stored_after_patch: i64 =
        sqlx::query_scalar("SELECT revision FROM kb_key_blocks WHERE key_block_id = 'kb_aabbccdd'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        stored_after_patch, 2,
        "second accepted patch must persist revision exactly 2"
    );
}

#[tokio::test]
async fn direct_writer_close_allows_reopen() {
    let tmp = TempDir::new().unwrap();
    let user_home = tmp.path().to_path_buf();
    let nexus_home = user_home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).unwrap();
    let op = nexus_home_layout::operational_workspace_dir(&user_home, CREATOR, SLUG);
    std::fs::create_dir_all(&op).unwrap();
    std::fs::write(
        nexus_home.join("config.toml"),
        format!(
            "active_creator_id = \"{CREATOR}\"
[active_workspace_slug_by_creator]
\"{CREATOR}\" = \"{SLUG}\""
        ),
    )
    .unwrap();
    let db_path = nexus_home_layout::workspace_state_db_path(&user_home, CREATOR, SLUG);
    {
        let guarded = init_engine_pool(&db_path, CREATOR, GuardedPoolOptions::default())
            .await
            .unwrap();
        let pool = guarded.clone_pool();
        seed_world(&pool, OWNED_WORLD, CREATOR).await;
        pool.close().await;
    }

    let core = CoreService::open(CoreOpenOptions {
        user_home: user_home.clone(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    core.close().await.expect("first close must succeed");
    drop(core);

    let core2 = CoreService::open(CoreOpenOptions {
        user_home,
        access: CoreAccess::DirectWriter,
    })
    .await
    .expect("second DirectWriter open after close must succeed");
    let _ = core2
        .world_kb_graph(&principal, OWNED_WORLD.to_string(), false)
        .await
        .expect("graph after reopen");
    core2.close().await.expect("second close must succeed");
}

fn journal_write(
    operation_id: &str,
    session_id: &str,
    provider_id: &str,
    status: CoreProviderJournalWriteStatus,
) -> CoreProviderJournalWrite {
    CoreProviderJournalWrite {
        operation_id: operation_id.parse().unwrap(),
        session_id: session_id.parse().unwrap(),
        provider_id: provider_id.parse().unwrap(),
        status,
    }
}

/// Provider journal contract (LIFE-3): owned write/read/settle seam on the
/// service — monotonic sequences, settlement that never downgrades a terminal
/// row, and read-only access that may read but never write the journal.
#[tokio::test]
async fn provider_journal_contract_write_read_settle() {
    let fx = setup().await;

    fx.core
        .journal_provider_operation(
            &fx.principal,
            journal_write("op_a", "sess_a", "mock-acp", CoreProviderJournalWriteStatus::Running),
        )
        .await
        .unwrap();
    fx.core
        .journal_provider_operation(
            &fx.principal,
            journal_write("op_b", "sess_b", "mock-acp", CoreProviderJournalWriteStatus::Running),
        )
        .await
        .unwrap();

    let a = fx
        .core
        .provider_operation(&fx.principal, "op_a".to_string())
        .await
        .unwrap()
        .expect("journaled op_a is readable");
    assert_eq!(a.operation_id.as_str(), "op_a");
    assert_eq!(a.session_id.as_str(), "sess_a");
    assert_eq!(a.provider_id.as_str(), "mock-acp");
    assert!(matches!(a.status, CoreProviderOperationStatus::Running));

    let b = fx
        .core
        .provider_operation(&fx.principal, "op_b".to_string())
        .await
        .unwrap()
        .expect("journaled op_b is readable");
    assert!(b.sequence > a.sequence, "journal sequences are monotonic");

    assert!(
        fx.core
            .provider_operation(&fx.principal, "op_missing".to_string())
            .await
            .unwrap()
            .is_none(),
        "unknown ids stay None, never a fabricated record"
    );

    // op_b reaches a terminal state; settlement must not downgrade it.
    fx.core
        .journal_provider_operation(
            &fx.principal,
            journal_write("op_b", "sess_b", "mock-acp", CoreProviderJournalWriteStatus::Cancelled),
        )
        .await
        .unwrap();
    let settled = fx.core.settle_provider_orphans().await.unwrap();
    assert_eq!(settled, 1, "only the non-terminal op_a is settled");
    let a = fx
        .core
        .provider_operation(&fx.principal, "op_a".to_string())
        .await
        .unwrap()
        .expect("op_a survives settlement");
    assert!(matches!(a.status, CoreProviderOperationStatus::Interrupted));
    let b = fx
        .core
        .provider_operation(&fx.principal, "op_b".to_string())
        .await
        .unwrap()
        .expect("op_b survives settlement");
    assert!(matches!(b.status, CoreProviderOperationStatus::Cancelled));
    assert_eq!(
        fx.core.settle_provider_orphans().await.unwrap(),
        0,
        "settlement is idempotent"
    );

    // Read-only access may read the journal but never write or settle it.
    let ro = CoreService::open(CoreOpenOptions {
        user_home: fx.tmp.path().to_path_buf(),
        access: CoreAccess::ReadOnly,
    })
    .await
    .unwrap();
    let ro_principal = ro.active_principal().await.unwrap();
    let ro_a = ro
        .provider_operation(&ro_principal, "op_a".to_string())
        .await
        .unwrap()
        .expect("read-only access reads the durable journal");
    assert!(matches!(ro_a.status, CoreProviderOperationStatus::Interrupted));
    let err = ro
        .journal_provider_operation(
            &ro_principal,
            journal_write("op_ro", "sess_a", "mock-acp", CoreProviderJournalWriteStatus::Running),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::Forbidden { .. }));
    let err = ro.settle_provider_orphans().await.unwrap_err();
    assert!(matches!(err, CoreError::Forbidden { .. }));
}
