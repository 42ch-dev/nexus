//! P0-T1 `worlds` + canonical KB promote/relate contract tests (core service
//! semantics): foreign worlds cannot be mutated, stale CAS leaves rows and the
//! change sequence untouched, successful canonical mutations are visible to a
//! second guarded reader, and the World lifecycle keeps its create/delete
//! contracts.

use nexus_contracts::{
    CreateWorldRequest, WorldKbPatchRelationshipRequest, WorldKbPromoteCandidateRequest,
};
use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService};
use nexus_local_db::open_pool_read_only;
use nexus_local_db::writer_protocol::{init_engine_pool, GuardedPoolOptions};
use sqlx::{Sqlite, SqlitePool};
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
    extra_body: Option<&str>,
    source_work_id: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO kb_key_blocks (key_block_id, world_id, block_type, canonical_name, status, revision, body_json, source_work_id, created_at, updated_at) VALUES (?, ?, 'character', ?, ?, ?, ?, ?, datetime('now'), datetime('now'))",
    )
    .bind(id)
    .bind(world_id)
    .bind(name)
    .bind(status)
    .bind(revision)
    .bind(extra_body)
    .bind(source_work_id)
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
        "INSERT INTO kb_extract_jobs (job_id, creator_id, workspace_id, work_entry_id, world_id, status, promotion_status, proposed_payload, block_type_guess, canonical_name_guess, work_id, version, created_at) VALUES (?, ?, 'ws', ?, ?, 'done', 'pending', ?, 'character', ?, 'work_1', 0, ?)",
    )
    .bind(job_id)
    .bind(CREATOR)
    .bind(format!("work_{job_id}"))
    .bind(world_id)
    .bind(format!(
        r#"{{"summary": "Candidate {name}", "attributes": {{"aliases": [], "novel_category": "character"}}}}"#
    ))
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
        seed_world(&pool, OWNED_WORLD, CREATOR).await;
        seed_world(&pool, FOREIGN_WORLD, "other_creator").await;
        seed_kb(&pool, "kb_tgt", OWNED_WORLD, "Target", "confirmed", Some(1), None, None).await;
        seed_kb(
            &pool,
            "kb_comp",
            OWNED_WORLD,
            "Comp",
            "confirmed",
            Some(3),
            Some(r#"{"computable": true, "state": {"tick": 1}}"#),
            None,
        )
        .await;
        seed_pending(&pool, "xj_job1", OWNED_WORLD, "Cand1", "2020-01-01T00:00:01Z").await;
        seed_pending(&pool, "xj_job2", OWNED_WORLD, "Cand2", "2020-01-01T00:00:02Z").await;
        seed_pending(&pool, "xj_job3", OWNED_WORLD, "Cand3", "2020-01-01T00:00:03Z").await;
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

fn promote_req(job_id: &str, action: &str, expected_version: u64, merge_target: Option<&str>) -> WorldKbPromoteCandidateRequest {
    serde_json::from_value(serde_json::json!({
        "job_id": job_id,
        "candidate_id": job_id,
        "action": action,
        "expected_version": expected_version,
        "merge_target_id": merge_target,
    }))
    .unwrap()
}

fn relate_add(source: &str, target: &str) -> WorldKbPatchRelationshipRequest {
    serde_json::from_value(serde_json::json!({
        "action": "add",
        "relationship": {
            "source_entity_id": source,
            "target_entity_id": target,
            "relation_type": "allied_with",
            "symmetric": true,
            "source_anchor_ids": [],
        },
    }))
    .unwrap()
}

fn relate_update(relationship_id: &str, expected_version: u64) -> WorldKbPatchRelationshipRequest {
    serde_json::from_value(serde_json::json!({
        "action": "update",
        "relationship_id": relationship_id,
        "expected_version": expected_version,
        "relationship": {
            "source_entity_id": "kb_tgt",
            "target_entity_id": "kb_comp",
            "relation_type": "opposes",
            "symmetric": false,
            "source_anchor_ids": [],
        },
    }))
    .unwrap()
}

async fn change_sequence_head(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COALESCE(MAX(sequence), 0) FROM core_changes")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// The P0-T1 acceptance selector: foreign World cannot be mutated; stale
/// promote/relationship update leaves rows and change sequence untouched;
/// successful canonical mutations are visible to a second guarded reader.
#[tokio::test]
async fn promote_relate_preserves_owner_and_cas() {
    let fx = setup().await;
    let reader_pool = read_only_pool(fx.tmp.path()).await;

    // ── Foreign World cannot be mutated ────────────────────────────────────
    let before = change_sequence_head(&reader_pool).await;
    let err = fx
        .core
        .promote_world_kb_candidate(
            &fx.principal,
            FOREIGN_WORLD.to_string(),
            promote_req("xj_job1", "adopt", 0, None),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::Forbidden { .. }));
    let err = fx
        .core
        .patch_world_kb_relationship(
            &fx.principal,
            FOREIGN_WORLD.to_string(),
            relate_add("kb_tgt", "kb_comp"),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::Forbidden { .. }));
    assert_eq!(change_sequence_head(&reader_pool).await, before);

    // ── Stale promote leaves the candidate row and outbox untouched ────────
    let err = fx
        .core
        .promote_world_kb_candidate(
            &fx.principal,
            OWNED_WORLD.to_string(),
            promote_req("xj_job1", "adopt", 5, None),
        )
        .await
        .unwrap_err();
    let CoreError::WorldKbConflict(details) = err else {
        panic!("expected conflict, got {err:?}")
    };
    assert_eq!(details.current_version, 0);
    assert_eq!(details.conflicting_path, "version");
    let job_version: i64 =
        sqlx::query_scalar("SELECT version FROM kb_extract_jobs WHERE job_id = 'xj_job1'")
            .fetch_one(&reader_pool)
            .await
            .unwrap();
    assert_eq!(job_version, 0);
    assert_eq!(change_sequence_head(&reader_pool).await, before);

    // ── Successful canonical mutations are visible to a second reader ──────
    let adopted = fx
        .core
        .promote_world_kb_candidate(
            &fx.principal,
            OWNED_WORLD.to_string(),
            promote_req("xj_job1", "adopt", 0, None),
        )
        .await
        .unwrap();
    assert_eq!(adopted.version, 1);
    assert_eq!(adopted.job.status, "confirmed");
    let adopted_id = adopted.entity.as_ref().expect("adopted entity").key_block_id.clone();

    let related = fx
        .core
        .patch_world_kb_relationship(
            &fx.principal,
            OWNED_WORLD.to_string(),
            relate_add(&adopted_id, "kb_tgt"),
        )
        .await
        .unwrap();
    assert_eq!(related.version, 1);
    let relationship_id = related
        .relationship
        .as_ref()
        .expect("related row")
        .relationship_id
        .clone();

    // Second guarded reader (fresh read-only core) sees both canonical rows.
    let second = CoreService::open(CoreOpenOptions {
        user_home: fx.tmp.path().to_path_buf(),
        access: CoreAccess::ReadOnly,
    })
    .await
    .unwrap();
    let second_principal = second.active_principal().await.unwrap();
    let graph = second
        .world_kb_graph(&second_principal, OWNED_WORLD.to_string(), false)
        .await
        .unwrap();
    assert!(
        graph
            .entities
            .iter()
            .any(|e| e.key_block_id == adopted_id && e.status == "confirmed"),
        "adopted entity visible to the second reader"
    );
    assert!(
        graph
            .relationships
            .iter()
            .any(|r| r.relationship_id == relationship_id),
        "related row visible to the second reader"
    );

    // The adopt produced a durable core_changes row (same-transaction outbox).
    let outbox: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM core_changes WHERE resource_id = ?",
    )
    .bind(&adopted_id)
    .fetch_one(&reader_pool)
    .await
    .unwrap();
    assert!(outbox >= 1, "adopt emitted a core_changes row");

    // ── Stale relationship update leaves rows and sequence untouched ───────
    let after_mutations = change_sequence_head(&reader_pool).await;
    let err = fx
        .core
        .patch_world_kb_relationship(
            &fx.principal,
            OWNED_WORLD.to_string(),
            relate_update(&relationship_id, 999),
        )
        .await
        .unwrap_err();
    let CoreError::WorldKbConflict(details) = err else {
        panic!("expected conflict, got {err:?}")
    };
    assert_eq!(details.current_version, 1);
    let revision: i64 =
        sqlx::query_scalar("SELECT revision FROM kb_relationships WHERE relationship_id = ?")
            .bind(&relationship_id)
            .fetch_one(&reader_pool)
            .await
            .unwrap();
    assert_eq!(revision, 1);
    assert_eq!(change_sequence_head(&reader_pool).await, after_mutations);

    second.close().await.unwrap();
    let _ = reader_pool.close().await;
}

/// Reject/merge outcomes keep their CAS + fold contracts, and a replayed
/// adopt after durable success returns the confirmed entry idempotently.
#[tokio::test]
async fn promote_reject_merge_and_idempotent_readopt() {
    let fx = setup().await;
    let pool = read_only_pool(fx.tmp.path()).await;

    // Reject: stale version conflicts, then the guarded flip succeeds.
    let err = fx
        .core
        .promote_world_kb_candidate(
            &fx.principal,
            OWNED_WORLD.to_string(),
            promote_req("xj_job1", "reject", 9, None),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::WorldKbConflict(_)));
    let rejected = fx
        .core
        .promote_world_kb_candidate(
            &fx.principal,
            OWNED_WORLD.to_string(),
            promote_req("xj_job1", "reject", 0, None),
        )
        .await
        .unwrap();
    assert_eq!(rejected.job.status, "rejected");
    assert_eq!(rejected.version, 1);
    assert!(rejected.entity.is_none());

    // Merge: folds the candidate summary into the confirmed target and
    // dismisses the candidate atomically.
    let merged = fx
        .core
        .promote_world_kb_candidate(
            &fx.principal,
            OWNED_WORLD.to_string(),
            promote_req("xj_job2", "merge", 0, Some("kb_tgt")),
        )
        .await
        .unwrap();
    let entity = merged.entity.as_ref().expect("merged target");
    assert_eq!(entity.key_block_id, "kb_tgt");
    // Idempotent re-adopt: a replayed adopt on a confirmed job returns the
    // attributed entry instead of erroring.
    let first = fx
        .core
        .promote_world_kb_candidate(
            &fx.principal,
            OWNED_WORLD.to_string(),
            promote_req("xj_job3", "adopt", 0, None),
        )
        .await
        .unwrap();
    let replay = fx
        .core
        .promote_world_kb_candidate(
            &fx.principal,
            OWNED_WORLD.to_string(),
            promote_req("xj_job3", "adopt", 0, None),
        )
        .await
        .unwrap();
    assert_eq!(
        first.entity.as_ref().expect("first").key_block_id,
        replay.entity.as_ref().expect("replay").key_block_id
    );

    let _ = pool.close().await;
}

/// World lifecycle: title validation, create visibility, and the hard-delete
/// contract (binding guard marker, foreign denial, works preserved).
#[tokio::test]
async fn world_lifecycle_create_delete_and_binding_guard() {
    let fx = setup().await;
    let pool = read_only_pool(fx.tmp.path()).await;
    // Raw seed writes need the guarded writer function registered; join the
    // live engine pool held by `fx.core` (same writer id, so no fencing).
    let engine = nexus_local_db::writer_protocol::join_live_engine_pool(
        &nexus_home_layout::workspace_state_db_path(fx.tmp.path(), CREATOR, SLUG),
        GuardedPoolOptions::default(),
    )
    .await
    .unwrap()
    .expect("live engine pool");
    let write_pool = engine.clone_pool();

    // Title validation: empty after trim is rejected with the retained reason.
    let bad: CreateWorldRequest = serde_json::from_value(serde_json::json!({"title": "   "}))
        .unwrap();
    let err = fx.core.create_world(&fx.principal, bad).await.unwrap_err();
    let CoreError::InvalidInput { field, reason } = err else {
        panic!("expected invalid input, got {err:?}")
    };
    assert_eq!(field, "title");
    assert!(reason.contains("1-200 characters"));

    // Foreign worlds are not deletable by the caller.
    let err = fx
        .core
        .delete_world(&fx.principal, FOREIGN_WORLD.to_string())
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::NotFound { .. }));

    // Create is visible to a second guarded reader.
    let req: CreateWorldRequest =
        serde_json::from_value(serde_json::json!({"title": "Starfall Isle"})).unwrap();
    let created = fx.core.create_world(&fx.principal, req).await.unwrap();
    assert!(created.world_id.starts_with("wld_"));

    let second = CoreService::open(CoreOpenOptions {
        user_home: fx.tmp.path().to_path_buf(),
        access: CoreAccess::ReadOnly,
    })
    .await
    .unwrap();
    let second_principal = second.active_principal().await.unwrap();
    let world = second
        .get_world(&second_principal, created.world_id.clone())
        .await
        .unwrap();
    assert_eq!(world.title, "Starfall Isle");
    assert_eq!(world.slug, "starfall-isle");

    // A Character binding blocks the hard delete with the retained marker.
    sqlx::query(
        "INSERT INTO characters (character_id, owner_creator_id, display_name, status, created_at, updated_at) VALUES (?, ?, 'Hero', 'active', datetime('now'), datetime('now'))",
    )
    .bind(format!("chr_{}", "a".repeat(32)))
    .bind(CREATOR)
    .execute(&write_pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO actor_world_bindings (binding_id, character_id, world_id, status, created_at, updated_at) VALUES (?, ?, ?, 'active', datetime('now'), datetime('now'))",
    )
    .bind(format!("awb_{}", "b".repeat(32)))
    .bind(format!("chr_{}", "a".repeat(32)))
    .bind(&created.world_id)
    .execute(&write_pool)
    .await
    .unwrap();

    let err = fx
        .core
        .delete_world(&fx.principal, created.world_id.clone())
        .await
        .unwrap_err();
    let CoreError::Forbidden { resource } = err else {
        panic!("expected binding conflict, got {err:?}")
    };
    assert_eq!(resource, nexus_core::DELETE_WORLD_BLOCKED_BY_BINDINGS);

    // Works owned by the caller survive with world_id cleared.
    sqlx::query(
        "INSERT INTO works (work_id, creator_id, workspace_slug, status, title, long_term_goal, initial_idea, intake_status, world_id, created_at, updated_at) VALUES ('work_keep', ?, ?, 'draft', 'Keep', 'g', 'i', 'pending', ?, datetime('now'), datetime('now'))",
    )
    .bind(CREATOR)
    .bind(SLUG)
    .bind(&created.world_id)
    .execute(&write_pool)
    .await
    .unwrap();

    sqlx::query("DELETE FROM actor_world_bindings WHERE binding_id = ?")
        .bind(format!("awb_{}", "b".repeat(32)))
        .execute(&write_pool)
        .await
        .unwrap();
    fx.core
        .delete_world(&fx.principal, created.world_id.clone())
        .await
        .unwrap();

    let works_world: Option<String> =
        sqlx::query_scalar("SELECT world_id FROM works WHERE work_id = 'work_keep'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(works_world, None);
    let err = second
        .get_world(&second_principal, created.world_id.clone())
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::NotFound { .. }));

    second.close().await.unwrap();
    let _ = pool.close().await;
}

/// Key-block state read: computable projection, non-computable null, and the
/// foreign/missing distinctions.
#[tokio::test]
async fn key_block_state_read_guards() {
    let fx = setup().await;

    let state = fx
        .core
        .world_kb_key_block_state(&fx.principal, OWNED_WORLD.to_string(), "kb_comp".to_string())
        .await
        .unwrap();
    assert!(state.is_computable);
    assert_eq!(
        state.state.as_ref().expect("state map").get("tick"),
        Some(&serde_json::json!(1))
    );
    assert_eq!(state.version, 3);

    let state = fx
        .core
        .world_kb_key_block_state(&fx.principal, OWNED_WORLD.to_string(), "kb_tgt".to_string())
        .await
        .unwrap();
    assert!(!state.is_computable);
    assert!(state.state.is_none());

    let err = fx
        .core
        .world_kb_key_block_state(&fx.principal, OWNED_WORLD.to_string(), "kb_missing".to_string())
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::NotFound { .. }));

    let err = fx
        .core
        .world_kb_key_block_state(&fx.principal, FOREIGN_WORLD.to_string(), "kb_comp".to_string())
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::Forbidden { .. }));
}
