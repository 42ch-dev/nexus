//! P1-T2 `world_kb` contract tests (core service semantics).

use nexus_contracts::{
    CoreProviderJournalWrite, CoreProviderJournalWriteStatus, CoreProviderOperationStatus,
    WorldKbPatchEntityRequest,
};
use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService};
use nexus_knowledge::world_kb::knowledge_entry::{KnowledgeGovernance, DISCLOSURE_OWNER_PRIVATE};
use nexus_knowledge::world_kb::KbStore;
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_local_db::open_pool_read_only;
use nexus_local_db::writer_protocol::{init_engine_pool, GuardedPoolOptions};
use nexus_local_db::{ActorContractConflict, LocalDbError};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
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
    // v1.191 P1: a stored Creator always carries its holder registry row
    // (§2.2), and the management read selection resolves it — the fixture must
    // therefore seed a complete subject, not a bare `creators` row.
    nexus_local_db::ensure_creator_row(pool, owner, "Test")
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
    assert!(matches!(err, CoreError::WorldOwnerDenied { .. }));

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
    assert!(matches!(err, CoreError::WorldOwnerDenied { .. }));
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

#[allow(clippy::too_many_lines)] // one end-to-end KB contract scenario
/// Provider journal contract (LIFE-3): owned write/read/settle seam on the
/// service — monotonic sequences, settlement that never downgrades a terminal
/// row, and read-only access that may read but never write the journal.
#[tokio::test]
async fn provider_journal_contract_write_read_settle() {
    let fx = setup().await;

    fx.core
        .journal_provider_operation(
            &fx.principal,
            journal_write(
                "op_a",
                "sess_a",
                "mock-acp",
                CoreProviderJournalWriteStatus::Running,
            ),
        )
        .await
        .unwrap();
    fx.core
        .journal_provider_operation(
            &fx.principal,
            journal_write(
                "op_b",
                "sess_b",
                "mock-acp",
                CoreProviderJournalWriteStatus::Running,
            ),
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
            journal_write(
                "op_b",
                "sess_b",
                "mock-acp",
                CoreProviderJournalWriteStatus::Cancelled,
            ),
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
    assert!(matches!(
        ro_a.status,
        CoreProviderOperationStatus::Interrupted
    ));
    let err = ro
        .journal_provider_operation(
            &ro_principal,
            journal_write(
                "op_ro",
                "sess_a",
                "mock-acp",
                CoreProviderJournalWriteStatus::Running,
            ),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::Forbidden { .. }));
    let err = ro.settle_provider_orphans().await.unwrap_err();
    assert!(matches!(err, CoreError::Forbidden { .. }));
}

/// Settlement admission contract: a service that stayed open across an
/// on-disk Creator/workspace selection change must refuse to settle and must
/// leave the stale workspace's journal untouched.
#[tokio::test]
async fn provider_journal_contract_settle_rejects_stale_selection() {
    let fx = setup().await;

    fx.core
        .journal_provider_operation(
            &fx.principal,
            journal_write(
                "op_stale",
                "sess_a",
                "mock-acp",
                CoreProviderJournalWriteStatus::Running,
            ),
        )
        .await
        .unwrap();

    // Active selection moves to another creator on disk; the service is not
    // reopened, so its bound context is stale for every domain write.
    let nexus_home = fx.tmp.path().join(".nexus42");
    std::fs::write(
        nexus_home.join("config.toml"),
        "active_creator_id = \"other_creator\"\n[active_workspace_slug_by_creator]\n\"other_creator\" = \"default\"\n",
    )
    .unwrap();

    let err = fx.core.settle_provider_orphans().await.unwrap_err();
    assert!(
        matches!(err, CoreError::AuthRequired),
        "settlement after a selection change must be rejected"
    );

    // No side effects: the stale workspace's orphan is still running.
    let pool = read_only_pool(fx.tmp.path()).await;
    let status: String = sqlx::query_scalar(
        "SELECT status FROM js_provider_operation_journal WHERE operation_id = 'op_stale'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "running", "stale workspace journal is untouched");
}

// ── v1.191 P1 T7: WorldSheet governance (durable §3) ────────────────────
//
// A WorldSheet link is a reverse reference: an entry linked as a sheet must
// stay a live, World-owned, **shared** `character` KE of the same World. Both
// directions are asserted here — the forward rule when a link is authored, and
// the reverse rule when an already-linked entry's governance or status moves.
//
// The authoring half is driven through the local-db primitives the canvas
// entity patch calls inside its own `BEGIN IMMEDIATE` transaction
// (`author_world_knowledge_governance_tx` is the governance + invalidation
// side-car of that transaction). The canvas *update* lane itself cannot be
// exercised end to end yet: `nexus-spoke-adapter`'s `run_cas_update_in_tx`
// still reads the retired `creator_only` column (finding F1, owner T8), which
// reddens three pre-existing cases in this file. The end-to-end direction that
// needs no update lane (canvas create with an audience) is asserted below.

const SHEET_WORLD: &str = "wld_sheetworld";
const SHEET_OTHER_WORLD: &str = "wld_sheetother";
const SHEET_ID: &str = "kb_5hee7000000000000000000000000000";

struct SheetFixture {
    _tmp: TempDir,
    db_path: PathBuf,
    core: CoreService,
    principal: nexus_core::Principal,
    character_id: String,
    binding_id: String,
    unbound_character_id: String,
}

async fn sheet_pool(fx: &SheetFixture) -> SqlitePool {
    nexus_local_db::open_pool(&fx.db_path).await.unwrap()
}

async fn world_revision(pool: &SqlitePool, world_id: &str) -> i64 {
    sqlx::query_scalar("SELECT knowledge_revision FROM narrative_worlds WHERE world_id = ?")
        .bind(world_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn sheet_pair(pool: &SqlitePool, entry_id: &str) -> (Option<String>, Option<String>) {
    sqlx::query_as("SELECT holder_entry_id, disclosure FROM kb_key_blocks WHERE key_block_id = ?")
        .bind(entry_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn sheet_revision(pool: &SqlitePool, entry_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COALESCE(revision, 0) FROM kb_key_blocks WHERE key_block_id = ?")
        .bind(entry_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn sheet_status(pool: &SqlitePool, entry_id: &str) -> String {
    sqlx::query_scalar("SELECT status FROM kb_key_blocks WHERE key_block_id = ?")
        .bind(entry_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn linked_sheet(pool: &SqlitePool, binding_id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT world_sheet_entry_id FROM actor_world_bindings WHERE binding_id = ?")
        .bind(binding_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn private_pair(holder: &str) -> KnowledgeGovernance {
    KnowledgeGovernance {
        holder_entry_id: Some(holder.to_string()),
        disclosure: Some(DISCLOSURE_OWNER_PRIVATE.to_string()),
    }
}

/// A Creator with its holder registry row, two Worlds, a bound Character and a
/// second owned Character with no binding to `SHEET_WORLD`.
async fn setup_sheet_fixture() -> SheetFixture {
    let tmp = TempDir::new().unwrap();
    let user_home = tmp.path().to_path_buf();
    let nexus_home = user_home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        &user_home, CREATOR, SLUG,
    ))
    .unwrap();
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
    let (character_id, binding_id, unbound_character_id) = {
        let guarded = init_engine_pool(&db_path, CREATOR, GuardedPoolOptions::default())
            .await
            .unwrap();
        let pool = guarded.clone_pool();
        nexus_local_db::ensure_creator_row(&pool, CREATOR, "Sheet Owner")
            .await
            .unwrap();
        seed_world(&pool, SHEET_WORLD, CREATOR).await;
        seed_world(&pool, SHEET_OTHER_WORLD, CREATOR).await;
        let bound = nexus_local_db::create_character_with_initial_binding(
            &pool,
            nexus_local_db::CreateCharacterParams {
                owner_creator_id: CREATOR,
                display_name: "Sheet Holder",
                image_uri: None,
                persona_json: "{}",
                world_id: SHEET_WORLD,
                world_sheet_entry_id: None,
            },
        )
        .await
        .unwrap();
        let unbound = nexus_local_db::create_character_with_initial_binding(
            &pool,
            nexus_local_db::CreateCharacterParams {
                owner_creator_id: CREATOR,
                display_name: "Unbound Audience",
                image_uri: None,
                persona_json: "{}",
                world_id: SHEET_OTHER_WORLD,
                world_sheet_entry_id: None,
            },
        )
        .await
        .unwrap();
        // The linkable sheet: a live, World-owned, shared `character` entry.
        seed_kb(
            &pool,
            SHEET_ID,
            SHEET_WORLD,
            "Sheet",
            "confirmed",
            Some(0),
            None,
        )
        .await;
        pool.close().await;
        (
            bound.character.character_id,
            bound.binding.binding_id,
            unbound.character.character_id,
        )
    };
    let core = CoreService::open(CoreOpenOptions {
        user_home,
        access: CoreAccess::EngineOwner,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    SheetFixture {
        _tmp: tmp,
        db_path,
        core,
        principal,
        character_id,
        binding_id,
        unbound_character_id,
    }
}

#[allow(clippy::too_many_lines)] // one both-directions journey over the sheet guard
#[tokio::test]
async fn v1191_world_sheet_governance_rejects_both_directions() {
    let fx = setup_sheet_fixture().await;
    let pool = sheet_pool(&fx).await;
    let holder = nexus_local_db::character_holder_entry_id(&fx.character_id);

    // Forward: the shared sheet links.
    let binding = nexus_local_db::update_actor_world_binding(
        &pool,
        CREATOR,
        &fx.character_id,
        &fx.binding_id,
        0,
        nexus_local_db::FieldPatch::Set(SHEET_ID),
    )
    .await
    .unwrap();
    assert_eq!(binding.world_sheet_entry_id.as_deref(), Some(SHEET_ID));

    // Reverse (governance): privatizing the linked sheet refuses
    // `invalid_world_sheet` with zero mutation — the pair stays null, the
    // World revision stays put and the link is never silently dropped.
    let mut tx = nexus_local_db::begin_immediate(&pool).await.unwrap();
    let err = nexus_local_db::kb_store::author_world_knowledge_governance_tx(
        &mut tx,
        SHEET_WORLD,
        SHEET_ID,
        Some(&private_pair(&holder)),
        nexus_local_db::kb_store::AuthoringRevision::Bump,
    )
    .await
    .unwrap_err();
    drop(tx);
    assert!(
        matches!(
            err,
            LocalDbError::ActorContractConflict {
                code: ActorContractConflict::InvalidWorldSheet
            }
        ),
        "the reverse rule keeps the link-time refusal, got {err:?}"
    );
    assert_eq!(sheet_pair(&pool, SHEET_ID).await, (None, None));
    assert_eq!(world_revision(&pool, SHEET_WORLD).await, 0);
    assert_eq!(
        linked_sheet(&pool, &fx.binding_id).await.as_deref(),
        Some(SHEET_ID)
    );

    // Reverse (status): the soft delete is a status change with the same
    // refusal on the ordinary store lane.
    let store = SqliteKbStore::new(pool.clone());
    let mut row = store.get_knowledge_entry(SHEET_ID).await.unwrap();
    row.status = "deprecated".to_string();
    let err = store.update_knowledge_entry(row).await.unwrap_err();
    assert!(
        matches!(
            err,
            nexus_knowledge::world_kb::store::KbStoreError::LinkedWorldSheet(ref id)
                if id == SHEET_ID
        ),
        "linked sheet status change must refuse, got {err:?}"
    );
    assert_eq!(sheet_status(&pool, SHEET_ID).await, "confirmed");
    let err = store.delete_knowledge_entry(SHEET_ID).await.unwrap_err();
    assert!(matches!(
        err,
        nexus_knowledge::world_kb::store::KbStoreError::LinkedWorldSheet(_)
    ));
    assert_eq!(sheet_status(&pool, SHEET_ID).await, "confirmed");

    // The unlinked sheet can be privatized, and only then does the forward
    // rule refuse a new link: the same predicate in both directions.
    let cleared = nexus_local_db::update_actor_world_binding(
        &pool,
        CREATOR,
        &fx.character_id,
        &fx.binding_id,
        binding.revision,
        nexus_local_db::FieldPatch::Clear,
    )
    .await
    .unwrap();
    assert_eq!(cleared.world_sheet_entry_id, None);
    let mut tx = nexus_local_db::begin_immediate(&pool).await.unwrap();
    nexus_local_db::kb_store::author_world_knowledge_governance_tx(
        &mut tx,
        SHEET_WORLD,
        SHEET_ID,
        Some(&private_pair(&holder)),
        nexus_local_db::kb_store::AuthoringRevision::Bump,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        sheet_pair(&pool, SHEET_ID).await,
        (
            Some(holder.clone()),
            Some(DISCLOSURE_OWNER_PRIVATE.to_string())
        )
    );
    assert_eq!(world_revision(&pool, SHEET_WORLD).await, 1);

    let refused = nexus_local_db::update_actor_world_binding(
        &pool,
        CREATOR,
        &fx.character_id,
        &fx.binding_id,
        cleared.revision,
        nexus_local_db::FieldPatch::Set(SHEET_ID),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(
            refused,
            LocalDbError::ActorContractConflict {
                code: ActorContractConflict::InvalidWorldSheet
            }
        ),
        "a disclosed sheet must not be linkable, got {refused:?}"
    );
    assert_eq!(linked_sheet(&pool, &fx.binding_id).await, None);
    pool.close().await;
}

/// Regression (L2 C2): a patch that merely re-states stored values is a no-op
/// for **both** revisions. The no-op decision is made on the materialised
/// post-patch content (not on request-field presence) plus the resolved
/// governance pair, and it short-circuits before any CAS.
#[tokio::test]
async fn v1191_world_sheet_governance_restated_content_is_a_no_op() {
    let fx = setup_sheet_fixture().await;
    let pool = sheet_pool(&fx).await;

    // The stored row is `Sheet` (body `{}`, revision 0, shared). Re-stating the
    // title and an explicitly `shared` audience changes nothing.
    let restated: WorldKbPatchEntityRequest = serde_json::from_value(serde_json::json!({
        "entity_id": SHEET_ID,
        "expected_version": 0,
        "patch": {"title": "Sheet", "audience": {"kind": "shared"}},
    }))
    .unwrap();
    let response = fx
        .core
        .patch_world_kb_entity(&fx.principal, SHEET_WORLD.to_string(), restated)
        .await
        .expect("a value-identical patch is a no-op, not a write");
    assert_eq!(response.version, 0, "the KE revision does not move");
    assert_eq!(sheet_revision(&pool, SHEET_ID).await, 0);
    assert_eq!(
        world_revision(&pool, SHEET_WORLD).await,
        0,
        "a no-op must not move the World knowledge revision"
    );
    assert_eq!(sheet_pair(&pool, SHEET_ID).await, (None, None));
    pool.close().await;
}

#[tokio::test]
async fn v1191_world_sheet_governance_canvas_create_authors_the_audience() {
    let fx = setup_sheet_fixture().await;
    let pool = sheet_pool(&fx).await;
    let creator_holder = nexus_local_db::creator_holder_entry_id(CREATOR);

    let governed: WorldKbPatchEntityRequest = serde_json::from_value(serde_json::json!({
        "entity_id": "kb_4d111111111111111111111111111111",
        "expected_version": 0,
        "patch": {
            "title": "Governed Sheet",
            "block_type": "character",
            "audience": {"kind": "author-only"},
        },
    }))
    .unwrap();
    let created = fx
        .core
        .patch_world_kb_entity(&fx.principal, SHEET_WORLD.to_string(), governed)
        .await
        .expect("an author-only canvas create admits");
    assert_eq!(created.version, 1);
    assert_eq!(
        sheet_pair(&pool, "kb_4d111111111111111111111111111111").await,
        (
            Some(creator_holder),
            Some(DISCLOSURE_OWNER_PRIVATE.to_string())
        ),
        "the canvas create authored the resolved governance pair"
    );

    // A Character with no binding to this World is not a permitted audience.
    let unbound: WorldKbPatchEntityRequest = serde_json::from_value(serde_json::json!({
        "entity_id": "kb_4d222222222222222222222222222222",
        "expected_version": 0,
        "patch": {
            "title": "Unbound Sheet",
            "block_type": "character",
            "audience": {
                "kind": "character-private",
                "character_id": fx.unbound_character_id,
            },
        },
    }))
    .unwrap();
    let err = fx
        .core
        .patch_world_kb_entity(&fx.principal, SHEET_WORLD.to_string(), unbound)
        .await
        .unwrap_err();
    // The permission refusal comes from the in-transaction resolution, so it
    // keeps the actor-input family (a malformed id would be
    // `InvalidInput { field: "patch.audience" }` from the wire mapping).
    assert!(matches!(err, CoreError::ActorInput(_)), "got {err:?}");
    let exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM kb_key_blocks WHERE key_block_id = 'kb_4d222222222222222222222222222222'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(exists, 0, "a refused audience writes no row");
    pool.close().await;
}
