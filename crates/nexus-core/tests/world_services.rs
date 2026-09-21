//! P0-T1 `worlds` + canonical KB promote/relate contract tests (core service
//! semantics): foreign worlds cannot be mutated, stale CAS leaves rows and the
//! change sequence untouched, successful canonical mutations are visible to a
//! second guarded reader, and the World lifecycle keeps its create/delete
//! contracts.
#![allow(clippy::too_many_lines)] // one end-to-end scenario per test

use nexus_contracts::{
    CreateForkRequest, CreateWorldRequest, WorldKbPatchRelationshipRequest,
    WorldKbPromoteCandidateRequest, WorldRuleCreateRequest, WorldRuleUpdateRequest,
};
use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService, CoreTimelineEventsQuery};
use nexus_local_db::open_pool_read_only;
use nexus_local_db::spoke_rules::{insert_rule, SpokeRuleRow};
use nexus_local_db::writer_protocol::{init_engine_pool, GuardedPoolOptions};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const CREATOR: &str = "test_creator";
const SLUG: &str = "default";
const OWNED_WORLD: &str = "wld_owned";
const FOREIGN_WORLD: &str = "wld_foreign";

struct Fixture {
    tmp: TempDir,
    db_path: PathBuf,
    core: CoreService,
    principal: nexus_core::Principal,
}

async fn seed_world(pool: &SqlitePool, world_id: &str, owner: &str) {
    // v1.191 P1: a stored Creator always carries its holder registry row, and
    // the World-KB management admission resolves it — the fixture must seed a
    // complete subject, not a bare `creators` row (same repair as the
    // `world_kb_contract` / `world_pack_services` fixtures).
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

#[allow(clippy::too_many_arguments)] // the fixture seeds every column in one call
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
        seed_kb(
            &pool,
            "kb_tgt",
            OWNED_WORLD,
            "Target",
            "confirmed",
            Some(1),
            None,
            None,
        )
        .await;
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
        seed_pending(
            &pool,
            "xj_job1",
            OWNED_WORLD,
            "Cand1",
            "2020-01-01T00:00:01Z",
        )
        .await;
        seed_pending(
            &pool,
            "xj_job2",
            OWNED_WORLD,
            "Cand2",
            "2020-01-01T00:00:02Z",
        )
        .await;
        seed_pending(
            &pool,
            "xj_job3",
            OWNED_WORLD,
            "Cand3",
            "2020-01-01T00:00:03Z",
        )
        .await;
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
        db_path,
        core,
        principal,
    }
}

/// A second guarded writer over the live engine pool, for test-only seeds that
/// must land after the core opened (same writer id → no fencing). The returned
/// handle owns the registration for the test's lifetime.
async fn live_write_pool(
    fx: &Fixture,
) -> (nexus_local_db::writer_protocol::GuardedPool, SqlitePool) {
    let engine = nexus_local_db::writer_protocol::join_live_engine_pool(
        &fx.db_path,
        GuardedPoolOptions::default(),
    )
    .await
    .unwrap()
    .expect("live engine pool");
    let pool = engine.clone_pool();
    (engine, pool)
}

fn promote_req(
    job_id: &str,
    action: &str,
    expected_version: u64,
    merge_target: Option<&str>,
) -> WorldKbPromoteCandidateRequest {
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
    assert!(matches!(err, CoreError::WorldOwnerDenied { .. }));
    let err = fx
        .core
        .patch_world_kb_relationship(
            &fx.principal,
            FOREIGN_WORLD.to_string(),
            relate_add("kb_tgt", "kb_comp"),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::WorldOwnerDenied { .. }));
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
    let adopted_id = adopted
        .entity
        .as_ref()
        .expect("adopted entity")
        .key_block_id
        .clone();

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
    let outbox: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM core_changes WHERE resource_id = ?")
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
    let () = reader_pool.close().await;
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

    let () = pool.close().await;
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
    let bad: CreateWorldRequest =
        serde_json::from_value(serde_json::json!({"title": "   "})).unwrap();
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

    // The hard delete is reflected in the workspace-wide lifecycle read
    // (coverage migrated from the retired daemon gateway-only handler tests).
    let worlds = fx.core.list_worlds(&fx.principal).await.unwrap();
    assert!(
        worlds.iter().all(|w| w.world_id != created.world_id),
        "deleted world must not appear in list_worlds: {:?}",
        worlds.iter().map(|w| &w.world_id).collect::<Vec<_>>()
    );

    second.close().await.unwrap();
    let () = pool.close().await;
}

/// Key-block state read: computable projection, non-computable null, and the
/// foreign/missing distinctions.
#[tokio::test]
async fn key_block_state_read_guards() {
    let fx = setup().await;
    let (_guard, pool) = live_write_pool(&fx).await;

    let state = fx
        .core
        .world_kb_key_block_state(
            &fx.principal,
            OWNED_WORLD.to_string(),
            "kb_comp".to_string(),
        )
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
        .world_kb_key_block_state(
            &fx.principal,
            OWNED_WORLD.to_string(),
            "kb_missing".to_string(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::NotFound { .. }));

    let err = fx
        .core
        .world_kb_key_block_state(
            &fx.principal,
            FOREIGN_WORLD.to_string(),
            "kb_comp".to_string(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::WorldOwnerDenied { .. }));

    let err = fx
        .core
        .world_kb_key_block_state(
            &fx.principal,
            "wld_unknown".to_string(),
            "kb_comp".to_string(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::NotFound { .. }));

    // A key block of another owned World is unreadable through this World's
    // path: the guard passes, the entity lookup does not (migrated from the
    // retired `world_kb_patch.rs::get_key_block_state_cross_world_returns_404`).
    seed_world(&pool, "wld_other_owned", CREATOR).await;
    seed_kb(
        &pool,
        "kb_elsewhere",
        "wld_other_owned",
        "Elsewhere",
        "confirmed",
        Some(1),
        None,
        None,
    )
    .await;
    let err = fx
        .core
        .world_kb_key_block_state(
            &fx.principal,
            OWNED_WORLD.to_string(),
            "kb_elsewhere".to_string(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::NotFound { .. }));
}

// ═══════════════════════════════════════════════════════════════════════════
// MIGRATED (daemon World/KB HTTP fixture retirement, v1.193 P2-T3)
//
// The cases below carry the surviving domain assertions of the retired
// `crates/nexus-daemon-runtime/tests/` fixtures `fork_api.rs`,
// `world_findings_api.rs`, `world_rules_api.rs`, `world_kb_relationships.rs`
// and `world_kb_patch.rs`. Retired with the host — the `POST /v1/daemon/check`
// product composition (`crates/nexus-daemon-runtime/src/check/`, disposed for
// deletion by technical-contracts §4: "`check` … Delete obsolete Rust-host
// composition"), its mental-pair / rule-evaluator findings, the
// foreign-`rule_refs` and embedded-`rules` whole-check rejections, the
// `Axum`/`error_envelope` status codes and the framework extractor rejections.
// The retained Connect check seam still runs its documented no-op checker
// (`apps/nexus42/src/commands/connect/invoke.rs`), and finding persistence
// keeps its library owner in `nexus-spoke-adapter` (`finding_port.rs`).
//
// Each migrated case is labelled with the retired selector it came from.
// ═══════════════════════════════════════════════════════════════════════════

fn fork_request(
    parent_branch_id: &str,
    forked_from_event_id: &str,
    label: Option<&str>,
) -> CreateForkRequest {
    let mut body = json!({
        "parent_branch_id": parent_branch_id,
        "forked_from_event_id": forked_from_event_id,
    });
    if let Some(label) = label {
        body["label"] = json!(label);
    }
    serde_json::from_value(body).unwrap()
}

async fn seed_timeline_event(
    pool: &SqlitePool,
    event_id: &str,
    world_id: &str,
    branch_id: &str,
    sequence_no: i64,
) {
    sqlx::query(
        "INSERT INTO narrative_timeline_events (timeline_event_id, world_id, branch_id, event_type, status, sequence_no, title, summary, metadata_json, created_at) VALUES (?, ?, ?, 'story_advance', 'canon', ?, 'Parent', 'Parent event', '{}', datetime('now'))",
    )
    .bind(event_id)
    .bind(world_id)
    .bind(branch_id)
    .bind(sequence_no)
    .execute(pool)
    .await
    .unwrap();
}

/// Fork create (migrated from `fork_api.rs`): ownership is decided before the
/// fork point is read, fork points are branch-scoped, the retained request DTO
/// bounds the label, and the canon `fork_created` lineage marker is readable
/// through the timeline-events owner.
#[tokio::test]
async fn retained_fork_create_guards_and_lineage() {
    let fx = setup().await;
    let (_guard, pool) = live_write_pool(&fx).await;
    let fork_point = "evt_fork_point";
    seed_timeline_event(&pool, fork_point, OWNED_WORLD, "fbk_root", 1).await;

    // A World the caller does not own is refused before the fork point is
    // read — the request quotes REAL ids from the caller's own World, so a
    // reordered guard would reach the capability instead.
    let denied = fx
        .core
        .create_fork(
            &fx.principal,
            FOREIGN_WORLD.to_string(),
            fork_request("fbk_root", fork_point, None),
        )
        .await
        .unwrap_err();
    assert!(matches!(denied, CoreError::WorldOwnerDenied { .. }));
    let markers: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM narrative_timeline_events WHERE event_type = 'fork_created'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(markers, 0, "a denied fork writes no marker");

    // An unknown event, and a real event quoted against the wrong branch, are
    // both input rejection (never a silent fork).
    let err = fx
        .core
        .create_fork(
            &fx.principal,
            OWNED_WORLD.to_string(),
            fork_request("fbk_root", "evt_missing", None),
        )
        .await
        .unwrap_err();
    let CoreError::InvalidInput { field, .. } = err else {
        panic!("expected invalid input, got {err:?}")
    };
    assert_eq!(field, "fork_point");

    seed_timeline_event(&pool, "evt_other_branch", OWNED_WORLD, "fbk_other", 1).await;
    let err = fx
        .core
        .create_fork(
            &fx.principal,
            OWNED_WORLD.to_string(),
            fork_request("fbk_other", fork_point, None),
        )
        .await
        .unwrap_err();
    let CoreError::InvalidInput { field, .. } = err else {
        panic!("expected invalid input, got {err:?}")
    };
    assert_eq!(
        field, "fork_point",
        "the fork point must sit on the stated branch"
    );

    // Happy path: a fresh branch id, the echoed parent + fork point, and the
    // label carried into the lineage marker.
    let created = fx
        .core
        .create_fork(
            &fx.principal,
            OWNED_WORLD.to_string(),
            fork_request("fbk_root", fork_point, Some("alt-ending")),
        )
        .await
        .unwrap();
    assert!(created.branch_id.starts_with("fbk_"), "{created:?}");
    assert_eq!(created.parent_branch_id, "fbk_root");
    assert_eq!(created.forked_from_event_id, fork_point);

    // Exactly one canon `fork_created` marker on the new branch, and its
    // lineage block is stored verbatim.
    let query = CoreTimelineEventsQuery {
        branch_id: Some(created.branch_id.clone()),
        status: Some("canon".to_string()),
        event_type: Some("fork_created".to_string()),
        limit: None,
        cursor: None,
    };
    let events = fx
        .core
        .list_timeline_events(&fx.principal, OWNED_WORLD.to_string(), query)
        .await
        .unwrap();
    assert_eq!(
        events.items.len(),
        1,
        "exactly one canon fork marker: {events:?}"
    );
    let lineage: String = sqlx::query_scalar(
        "SELECT extensions_nexus_json FROM narrative_timeline_events WHERE world_id = ? AND branch_id = ? AND event_type = 'fork_created'",
    )
    .bind(OWNED_WORLD)
    .bind(&created.branch_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let lineage: Value = serde_json::from_str(&lineage).unwrap();
    assert_eq!(lineage["fork_lineage"]["parent_branch_id"], "fbk_root");
    assert_eq!(lineage["fork_lineage"]["forked_from_event_id"], fork_point);
    assert_eq!(lineage["fork_lineage"]["label"], "alt-ending");

    // The retained request DTO bounds the label (1-200 chars when present):
    // an empty or over-long label never reaches the domain.
    for label in ["", &"x".repeat(201)] {
        assert!(
            serde_json::from_value::<CreateForkRequest>(json!({
                "parent_branch_id": "fbk_root",
                "forked_from_event_id": fork_point,
                "label": label,
            }))
            .is_err(),
            "label length {} must not deserialize",
            label.len()
        );
    }
}

#[allow(clippy::too_many_arguments)]
async fn seed_finding(pool: &SqlitePool, finding_id: &str, world_id: &str, created_at: i64) {
    let mut tx = pool.begin().await.unwrap();
    nexus_local_db::world_findings::insert_world_finding_in_tx(
        &mut tx,
        finding_id,
        world_id,
        1,
        "info",
        "open",
        "A seeded finding",
        "Seeded body",
        Some("dramatic_irony_asymmetry"),
        None,
        Some(r#"{"event_id":"evt_transfer"}"#),
        None,
        r#"{"paragraph":1}"#,
        r#"{"nexus":{"creator_id":"test_creator"}}"#,
        created_at,
        created_at,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

/// World findings read surface (migrated from `world_findings_api.rs`): the
/// owned/unknown/foreign guards, the stored-to-wire projection (spoke
/// vocabulary verbatim, epoch → RFC 3339, the routing stamp rehydrated), and
/// the honest 500-row cap. The retired check-composition half of that fixture
/// (`box_basket_check_200_persists_and_read_route_lists`,
/// `empty_world_check_200_and_read_200_empty`) went with the host — see the
/// section header.
#[tokio::test]
async fn retained_world_findings_read_projection_and_cap() {
    let fx = setup().await;
    let (_guard, pool) = live_write_pool(&fx).await;
    let world = "wld_findings";
    seed_world(&pool, world, CREATOR).await;

    let empty = fx
        .core
        .list_world_findings(&fx.principal, world.to_string())
        .await
        .unwrap();
    assert!(empty.findings.is_empty());
    assert!(!empty.truncated);

    let err = fx
        .core
        .list_world_findings(&fx.principal, "wld_missing".to_string())
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::NotFound { .. }));
    let err = fx
        .core
        .list_world_findings(&fx.principal, FOREIGN_WORLD.to_string())
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::WorldOwnerDenied { .. }));

    seed_finding(&pool, "fnd_one", world, 1_700_000_000).await;
    let listed = fx
        .core
        .list_world_findings(&fx.principal, world.to_string())
        .await
        .unwrap();
    assert_eq!(listed.findings.len(), 1);
    assert!(!listed.truncated);
    let wire = serde_json::to_value(&listed.findings[0]).unwrap();
    assert_eq!(wire["finding_id"], "fnd_one");
    assert_eq!(wire["severity"], "info", "spoke severity verbatim");
    assert_eq!(wire["status"], "open");
    assert_eq!(wire["kind"], "dramatic_irony_asymmetry");
    assert_eq!(wire["description"], "Seeded body");
    assert_eq!(wire["extensions"]["nexus"]["creator_id"], "test_creator");
    assert!(
        chrono::DateTime::parse_from_rfc3339(wire["created_at"].as_str().unwrap()).is_ok(),
        "epoch → RFC 3339: {wire}"
    );
    assert!(
        chrono::DateTime::parse_from_rfc3339(wire["updated_at"].as_str().unwrap()).is_ok(),
        "epoch → RFC 3339: {wire}"
    );

    // 502 stored rows: exactly the newest 500, honestly flagged.
    for i in 0..501 {
        seed_finding(&pool, &format!("fnd_bulk_{i:03}"), world, 1_700_001_000 + i).await;
    }
    let capped = fx
        .core
        .list_world_findings(&fx.principal, world.to_string())
        .await
        .unwrap();
    assert!(capped.truncated, "502 stored rows exceed the cap");
    assert_eq!(capped.findings.len(), 500);
    let ids: Vec<String> = capped
        .findings
        .iter()
        .map(|finding| finding.finding_id.clone())
        .collect();
    assert!(
        ids.contains(&"fnd_bulk_500".to_string()),
        "the newest row survives the cap"
    );
    assert!(
        !ids.contains(&"fnd_one".to_string()),
        "the oldest row falls outside the newest-500 window"
    );
}

fn rule_create(body: Value) -> WorldRuleCreateRequest {
    serde_json::from_value(body).unwrap()
}

fn rule_update(body: Value) -> WorldRuleUpdateRequest {
    serde_json::from_value(body).unwrap()
}

#[allow(clippy::too_many_arguments)] // seed helper: the row's columns are the fixture surface
async fn seed_world_rule(
    pool: &SqlitePool,
    rule_id: &str,
    world_id: &str,
    canonical_name: &str,
    status: &str,
    severity_hint: Option<&str>,
    target_entry_types: &[&str],
    constraint: Value,
) {
    insert_rule(
        pool,
        &SpokeRuleRow {
            rule_id: rule_id.to_string(),
            world_id: world_id.to_string(),
            schema_version: 1,
            canonical_name: canonical_name.to_string(),
            kind: "rule".to_string(),
            statement: Some(format!("Statement for {canonical_name}")),
            description: None,
            target_entry_types_json: serde_json::to_string(target_entry_types).unwrap(),
            severity_hint: severity_hint.map(str::to_string),
            status: Some(status.to_string()),
            source_anchor_json: None,
            extensions_json: json!({ "nexus": { "constraint": constraint } }).to_string(),
            created_at: Some(1_700_000_000),
            updated_at: Some(1_700_000_100),
        },
    )
    .await
    .unwrap();
}

/// World structured rules (migrated from `world_rules_api.rs`): the write
/// surface's carrier grammar and field-level rejections, the per-field PATCH
/// semantics with whole-carrier replacement, and the read guards/projection.
/// The retired fixture's check-composition cases
/// (`check_composes_mental_and_rule_findings_and_read_routes_list`,
/// `foreign_rule_ref_rejects_whole_check_no_partial_persist`,
/// `embedded_rules_reject_400_invalid_input`,
/// `zero_active_rules_draft_only_200_mental_pair_only`) and its tier-2
/// middleware / `Axum` extractor expectations went with the host.
#[tokio::test]
async fn retained_world_rules_write_surface_and_read_guards() {
    let fx = setup().await;
    let (_guard, pool) = live_write_pool(&fx).await;
    let world = "wld_rules";
    seed_world(&pool, world, CREATOR).await;

    // Read guards and the empty list.
    let empty = fx
        .core
        .list_world_rules(&fx.principal, world.to_string())
        .await
        .unwrap();
    assert!(empty.rules.is_empty());
    assert!(!empty.truncated);
    let err = fx
        .core
        .list_world_rules(&fx.principal, "wld_missing".to_string())
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::NotFound { .. }));
    let err = fx
        .core
        .list_world_rules(&fx.principal, FOREIGN_WORLD.to_string())
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::WorldOwnerDenied { .. }));
    let err = fx
        .core
        .create_world_rule(
            &fx.principal,
            FOREIGN_WORLD.to_string(),
            rule_create(json!({
                "canonical_name": "X",
                "statement": "Y",
                "constraint": { "family": "module_presence", "module_key": "m" },
            })),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::WorldOwnerDenied { .. }));

    // Create: defaults, the server-minted id shape, and the carrier surfaced
    // first-class.
    let created = fx
        .core
        .create_world_rule(
            &fx.principal,
            world.to_string(),
            rule_create(json!({
                "canonical_name": "Characters need summaries",
                "statement": "Every character carries a summary",
                "constraint": { "family": "required_field", "field": "body.summary" },
            })),
        )
        .await
        .unwrap();
    let wire = serde_json::to_value(&created).unwrap();
    assert_eq!(wire["canonical_name"], "Characters need summaries");
    assert_eq!(wire["kind"], "rule", "defaulted kind");
    assert_eq!(wire["status"], "active", "defaulted status");
    assert_eq!(wire["target_entry_types"], json!([]));
    assert!(
        wire.get("severity_hint").is_none(),
        "an absent severity hint is omitted, never null: {wire}"
    );
    assert_eq!(
        wire["constraint"],
        json!({ "family": "required_field", "field": "body.summary" }),
        "the AR-2 carrier is projected first-class"
    );
    assert!(
        wire.get("extensions").is_none(),
        "the extensions bag itself is never exposed: {wire}"
    );
    assert_eq!(created.created_at, created.updated_at, "create stamps both");
    let rule_id = created.rule_id.clone();
    let hex = rule_id.strip_prefix("rul_").expect("rul_ prefix");
    assert_eq!(hex.len(), 32, "uuid v4 simple: {rule_id}");
    assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));

    // The create defaults are real at rest, not just on the wire.
    let stored: (i64, String, Option<String>, String, String, String) = sqlx::query_as(
        "SELECT schema_version, kind, severity_hint, status, target_entry_types_json, extensions_json FROM spoke_rules WHERE rule_id = ?",
    )
    .bind(&rule_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored.0, 1, "schema_version");
    assert_eq!(stored.1, "rule", "defaulted kind");
    assert_eq!(stored.2, None, "no defaulted severity hint");
    assert_eq!(stored.3, "active", "defaulted status");
    assert_eq!(stored.4, "[]", "defaulted target axis");
    let stored_bag: Value = serde_json::from_str(&stored.5).unwrap();
    assert_eq!(
        stored_bag,
        json!({ "nexus": { "constraint": { "family": "required_field", "field": "body.summary" } } }),
        "the nexus namespace is written fresh at create"
    );

    // Carrier grammar + meta-field rejections carry the AR-2 field vocabulary.
    let rejections: Vec<(Value, &str)> = vec![
        (
            json!({ "canonical_name": "X", "statement": "Y", "constraint": { "family": "module_presence", "module_key": "m", "bogus": true } }),
            "constraint",
        ),
        (
            json!({ "canonical_name": "X", "statement": "Y", "constraint": { "family": "tone", "module_key": "m" } }),
            "constraint.family",
        ),
        (
            json!({ "canonical_name": "X", "statement": "Y", "constraint": { "family": "module_presence" } }),
            "constraint.module_key",
        ),
        (
            json!({ "canonical_name": "X", "statement": "Y", "constraint": { "family": "module_absence", "module_key": "" } }),
            "constraint.module_key",
        ),
        (
            json!({ "canonical_name": "X", "statement": "Y", "constraint": { "family": "required_field", "field": "body.title" } }),
            "constraint.field",
        ),
        (
            json!({ "canonical_name": "X", "statement": "Y", "constraint": { "family": "observer_cardinality", "min": 5, "max": 3 } }),
            "constraint.min",
        ),
        (
            json!({ "canonical_name": "X", "statement": "Y", "constraint": { "family": "observer_cardinality", "max": 3 }, "target_entry_types": ["body_summary"] }),
            "target_entry_types",
        ),
        (
            json!({ "canonical_name": "X", "statement": "Y", "constraint": { "family": "module_presence", "module_key": "m" }, "target_entry_types": [""] }),
            "target_entry_types",
        ),
        (
            json!({ "canonical_name": "   ", "statement": "Y", "constraint": { "family": "module_presence", "module_key": "m" } }),
            "canonical_name",
        ),
        (
            json!({ "canonical_name": "X", "statement": "", "constraint": { "family": "module_presence", "module_key": "m" } }),
            "statement",
        ),
        (
            json!({ "canonical_name": "X", "statement": "Y", "constraint": { "family": "module_presence", "module_key": "m" }, "status": "banana" }),
            "status",
        ),
        (
            json!({ "canonical_name": "X", "statement": "Y", "constraint": { "family": "module_presence", "module_key": "m" }, "severity_hint": "  " }),
            "severity_hint",
        ),
        (
            json!({ "canonical_name": "X", "statement": "Y", "constraint": { "family": "module_presence", "module_key": "m" }, "kind": "" }),
            "kind",
        ),
    ];
    for (body, expected_field) in rejections {
        let err = fx
            .core
            .create_world_rule(&fx.principal, world.to_string(), rule_create(body.clone()))
            .await
            .unwrap_err();
        let CoreError::InvalidInput { field, .. } = err else {
            panic!("expected invalid input for {body}, got {err:?}")
        };
        assert_eq!(field, expected_field, "for {body}");
    }

    // Every carrier family creates: the member-aware grammar accepts the same
    // four families it rejects elsewhere.
    for carrier in [
        json!({ "family": "module_presence", "module_key": "characters" }),
        json!({ "family": "module_absence", "module_key": "lore" }),
        json!({ "family": "required_field", "field": "body.summary" }),
        json!({ "family": "observer_cardinality", "min": 1, "max": 3 }),
    ] {
        let family = carrier["family"].as_str().unwrap().to_string();
        let item = fx
            .core
            .create_world_rule(
                &fx.principal,
                world.to_string(),
                rule_create(json!({
                    "canonical_name": format!("Family {family}"),
                    "statement": format!("A {family} rule"),
                    "constraint": carrier.clone(),
                })),
            )
            .await
            .unwrap_or_else(|error| panic!("{family} must create: {error:?}"));
        let wire = serde_json::to_value(&item).unwrap();
        assert_eq!(
            wire["constraint"], carrier,
            "the carrier is echoed verbatim for {family}"
        );
    }

    // PATCH replaces every mutable member and never touches description or
    // created_at.
    let updated = fx
        .core
        .update_world_rule(
            &fx.principal,
            world.to_string(),
            rule_id.clone(),
            rule_update(json!({
                "canonical_name": "After",
                "statement": "New statement",
                "severity_hint": "error",
                "status": "draft",
                "kind": "prohibition",
                "target_entry_types": ["new_type"],
                "constraint": { "family": "module_absence", "module_key": "lore" },
            })),
        )
        .await
        .unwrap();
    let wire = serde_json::to_value(&updated).unwrap();
    assert_eq!(wire["canonical_name"], "After");
    assert_eq!(wire["statement"], "New statement");
    assert_eq!(wire["severity_hint"], "error");
    assert_eq!(wire["status"], "draft");
    assert_eq!(wire["kind"], "prohibition");
    assert_eq!(wire["target_entry_types"], json!(["new_type"]));
    assert_eq!(
        wire["constraint"],
        json!({ "family": "module_absence", "module_key": "lore" })
    );
    assert_eq!(
        updated.created_at, created.created_at,
        "created_at never moves"
    );

    // Whole-carrier replacement: only `extensions.nexus.constraint` is
    // overwritten — sibling nexus keys and every other namespace survive.
    let bagged = json!({
        "nexus": {
            "constraint": { "family": "module_presence", "module_key": "old" },
            "other_nexus_key": "keep-me",
        },
        "other_namespace": { "k": [1, 2] },
    });
    sqlx::query("INSERT INTO spoke_rules (rule_id, world_id, schema_version, canonical_name, kind, statement, target_entry_types_json, status, extensions_json, created_at, updated_at) VALUES ('rul_bag', ?, 1, 'Bag', 'rule', 'S', '[]', 'active', ?, 1700000000, 1700000100)")
        .bind(world)
        .bind(bagged.to_string())
        .execute(&pool)
        .await
        .unwrap();
    fx.core
        .update_world_rule(
            &fx.principal,
            world.to_string(),
            "rul_bag".to_string(),
            rule_update(
                json!({ "constraint": { "family": "required_field", "field": "body.tags" } }),
            ),
        )
        .await
        .unwrap();
    let stored: String =
        sqlx::query_scalar("SELECT extensions_json FROM spoke_rules WHERE rule_id = 'rul_bag'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let stored: Value = serde_json::from_str(&stored).unwrap();
    assert_eq!(
        stored["nexus"]["constraint"],
        json!({ "family": "required_field", "field": "body.tags" }),
        "carrier replaced"
    );
    assert_eq!(stored["nexus"]["other_nexus_key"], "keep-me");
    assert_eq!(stored["other_namespace"], json!({ "k": [1, 2] }));

    // Addressing precedes payload: an empty PATCH, an unknown id and a
    // cross-world id each fail without writing.
    let err = fx
        .core
        .update_world_rule(
            &fx.principal,
            world.to_string(),
            rule_id.clone(),
            rule_update(json!({})),
        )
        .await
        .unwrap_err();
    let CoreError::InvalidInput { field, .. } = err else {
        panic!("expected invalid input, got {err:?}")
    };
    assert_eq!(field, "patch");

    let err = fx
        .core
        .update_world_rule(
            &fx.principal,
            world.to_string(),
            "rul_absent".to_string(),
            rule_update(json!({ "statement": "x" })),
        )
        .await
        .unwrap_err();
    let CoreError::NotFound { resource } = err else {
        panic!("expected not found, got {err:?}")
    };
    assert!(resource.contains("rul_absent"));

    seed_world_rule(
        &pool,
        "rul_other_world",
        "wld_other_owned",
        "SecretOtherWorldRule",
        "active",
        None,
        &[],
        json!({ "family": "module_presence", "module_key": "m" }),
    )
    .await;
    let err = fx
        .core
        .update_world_rule(
            &fx.principal,
            world.to_string(),
            "rul_other_world".to_string(),
            rule_update(json!({ "statement": "x" })),
        )
        .await
        .unwrap_err();
    let CoreError::NotFound { resource } = err else {
        panic!("expected not found, got {err:?}")
    };
    assert!(
        resource.contains("rul_other_world") && !resource.contains("SecretOtherWorldRule"),
        "a cross-world rule is indistinguishable from an unknown id: {resource}"
    );
    let untouched: String = sqlx::query_scalar(
        "SELECT canonical_name FROM spoke_rules WHERE rule_id = 'rul_other_world'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(untouched, "SecretOtherWorldRule");

    // The pair rule judges the effective pair, and an explicit empty target
    // axis clears while an absent one leaves the stored axis alone.
    seed_world_rule(
        &pool,
        "rul_obs",
        world,
        "Obs",
        "active",
        None,
        &[],
        json!({ "family": "observer_cardinality", "max": 3 }),
    )
    .await;
    let err = fx
        .core
        .update_world_rule(
            &fx.principal,
            world.to_string(),
            "rul_obs".to_string(),
            rule_update(json!({ "target_entry_types": ["x"] })),
        )
        .await
        .unwrap_err();
    let CoreError::InvalidInput { field, .. } = err else {
        panic!("expected invalid input, got {err:?}")
    };
    assert_eq!(field, "target_entry_types");

    let err = fx
        .core
        .update_world_rule(
            &fx.principal,
            world.to_string(),
            "rul_obs".to_string(),
            rule_update(json!({ "constraint": { "family": "observer_cardinality", "min": 1 }, "target_entry_types": ["y"] })),
        )
        .await
        .unwrap_err();
    let CoreError::InvalidInput { field, .. } = err else {
        panic!("expected invalid input, got {err:?}")
    };
    assert_eq!(field, "target_entry_types");

    let cleared = fx
        .core
        .update_world_rule(
            &fx.principal,
            world.to_string(),
            "rul_obs".to_string(),
            rule_update(json!({ "target_entry_types": [] })),
        )
        .await
        .unwrap();
    assert_eq!(cleared.target_entry_types, Vec::<String>::new());
    let kept = fx
        .core
        .update_world_rule(
            &fx.principal,
            world.to_string(),
            rule_id.clone(),
            rule_update(json!({ "statement": "left alone" })),
        )
        .await
        .unwrap();
    assert_eq!(
        kept.target_entry_types,
        vec!["new_type".to_string()],
        "an absent axis is not a clear"
    );

    // Every rejected PATCH returned before any write: the stored row of the
    // last untouched seed still carries its original timestamp.
    let frozen: i64 =
        sqlx::query_scalar("SELECT updated_at FROM spoke_rules WHERE rule_id = 'rul_other_world'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(frozen, 1_700_000_100);

    // Read projection: store order (canonical_name ASC, rule_id ASC) with the
    // carrier first-class.
    let rules = fx
        .core
        .list_world_rules(&fx.principal, world.to_string())
        .await
        .unwrap();
    assert!(!rules.truncated);
    let names: Vec<&str> = rules
        .rules
        .iter()
        .map(|rule| rule.canonical_name.as_str())
        .collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted, "canonical_name ASC store order");
    let bag_item = rules
        .rules
        .iter()
        .find(|rule| rule.rule_id == "rul_bag")
        .expect("seeded rule listed");
    assert_eq!(
        bag_item.constraint.get("family").and_then(Value::as_str),
        Some("required_field")
    );
    let bag_wire = serde_json::to_value(bag_item).unwrap();
    assert!(bag_wire.get("extensions").is_none());
}

/// The 500-rule read cap (migrated from
/// `world_rules_api.rs::read_route_caps_and_flags_truncation`): the response
/// carries the first 500 of `canonical_name ASC, rule_id ASC` and flags the
/// overflow, and an under-cap World returns every row unflagged.
#[tokio::test]
async fn retained_world_rules_read_cap_and_store_order() {
    let fx = setup().await;
    let (_guard, pool) = live_write_pool(&fx).await;
    let world = "wld_rules_bulk";
    seed_world(&pool, world, CREATOR).await;

    for i in 0..499 {
        seed_world_rule(
            &pool,
            &format!("rul_bulk_{i:03}"),
            world,
            &format!("Bulk rule {i:03}"),
            "active",
            None,
            &[],
            json!({ "family": "module_presence", "module_key": "x" }),
        )
        .await;
    }
    // Three rows share the 500th canonical name — the rule_id ASC tie-break
    // decides which one survives the cap.
    for rule_id in ["rul_aaa_499", "rul_bulk_499", "rul_zzz_499"] {
        seed_world_rule(
            &pool,
            rule_id,
            world,
            "Bulk rule 499",
            "active",
            None,
            &[],
            json!({ "family": "module_presence", "module_key": "x" }),
        )
        .await;
    }

    let capped = fx
        .core
        .list_world_rules(&fx.principal, world.to_string())
        .await
        .unwrap();
    assert!(capped.truncated, "502 stored rows exceed the cap");
    assert_eq!(capped.rules.len(), 500);
    assert_eq!(capped.rules[0].canonical_name, "Bulk rule 000");
    assert_eq!(capped.rules[0].rule_id, "rul_bulk_000");
    assert_eq!(capped.rules[499].canonical_name, "Bulk rule 499");
    assert_eq!(
        capped.rules[499].rule_id, "rul_aaa_499",
        "the rule_id ASC tie-break decides the truncation boundary"
    );

    let few = "wld_rules_few";
    seed_world(&pool, few, CREATOR).await;
    for i in 0..3 {
        seed_world_rule(
            &pool,
            &format!("rul_few_{i}"),
            few,
            &format!("Few rule {i}"),
            "draft",
            Some("info"),
            &["character"],
            json!({ "family": "module_presence", "module_key": "x" }),
        )
        .await;
    }
    let listed = fx
        .core
        .list_world_rules(&fx.principal, few.to_string())
        .await
        .unwrap();
    assert!(!listed.truncated);
    assert_eq!(listed.rules.len(), 3);
    assert_eq!(
        listed.rules[0].status.as_deref(),
        Some("draft"),
        "every status is visible to the author, draft included"
    );
}

/// Rule PATCH failure modes and axis semantics (migrated from
/// `world_rules_api.rs` `patch_malformed_extensions_json_500_internal_no_write`,
/// `patch_meta_field_value_checks`, `patch_empty_body_rejects_field_patch_no_write`,
/// `patch_invalid_payload_404_precedes_payload_validation`,
/// `patch_null_target_entry_types_leaves_unchanged`,
/// `patch_status_deprecated_deactivates`,
/// `patch_updates_updated_at_keeps_created_at` and
/// `write_routes_guards_403_foreign_world`).
#[tokio::test]
async fn retained_world_rules_patch_failure_modes_and_axis_semantics() {
    let fx = setup().await;
    let (_guard, pool) = live_write_pool(&fx).await;
    let world = "wld_rules_patch";
    seed_world(&pool, world, CREATOR).await;

    // A corrupted stored extensions bag fails the carrier-replacing patch
    // closed: no silent clobber, no write at all.
    sqlx::query("INSERT INTO spoke_rules (rule_id, world_id, schema_version, canonical_name, kind, statement, target_entry_types_json, status, extensions_json, created_at, updated_at) VALUES ('rul_corrupt', ?, 1, 'Corrupt', 'rule', 'S', '[]', 'active', '{not valid json', 1700000000, 1700000100)")
        .bind(world)
        .execute(&pool)
        .await
        .unwrap();
    let err = fx
        .core
        .update_world_rule(
            &fx.principal,
            world.to_string(),
            "rul_corrupt".to_string(),
            rule_update(
                json!({ "constraint": { "family": "module_presence", "module_key": "m" } }),
            ),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, CoreError::Internal { .. }),
        "a corrupt stored bag is an internal failure, got {err:?}"
    );
    let (bag, stamp): (String, i64) = sqlx::query_as(
        "SELECT extensions_json, updated_at FROM spoke_rules WHERE rule_id = 'rul_corrupt'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        bag, "{not valid json",
        "the corrupt row stays byte-identical"
    );
    assert_eq!(
        stamp, 1_700_000_100,
        "a failed patch never refreshes the row"
    );

    // Meta-field value checks mirror create and leave the row untouched.
    seed_world_rule(
        &pool,
        "rul_meta",
        world,
        "Meta",
        "active",
        Some("info"),
        &["x"],
        json!({ "family": "module_presence", "module_key": "m" }),
    )
    .await;
    let rejections: Vec<(Value, &str)> = vec![
        (json!({ "status": "banana" }), "status"),
        (json!({ "canonical_name": "   " }), "canonical_name"),
        (json!({ "statement": "" }), "statement"),
        (json!({ "severity_hint": " " }), "severity_hint"),
        (json!({ "kind": "" }), "kind"),
        (
            json!({ "constraint": { "family": "tone" } }),
            "constraint.family",
        ),
        (json!({ "target_entry_types": [""] }), "target_entry_types"),
    ];
    for (body, field) in rejections {
        let err = fx
            .core
            .update_world_rule(
                &fx.principal,
                world.to_string(),
                "rul_meta".to_string(),
                rule_update(body.clone()),
            )
            .await
            .unwrap_err();
        let CoreError::InvalidInput { field: actual, .. } = err else {
            panic!("expected invalid input for {body}, got {err:?}")
        };
        assert_eq!(actual, field, "for {body}");
    }
    let (name, status, stamp): (String, String, i64) = sqlx::query_as(
        "SELECT canonical_name, status, updated_at FROM spoke_rules WHERE rule_id = 'rul_meta'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (name.as_str(), status.as_str(), stamp),
        ("Meta", "active", 1_700_000_100)
    );

    // An empty PATCH is refused and never refreshes the row.
    let err = fx
        .core
        .update_world_rule(
            &fx.principal,
            world.to_string(),
            "rul_meta".to_string(),
            rule_update(json!({})),
        )
        .await
        .unwrap_err();
    let CoreError::InvalidInput { field, .. } = err else {
        panic!("expected invalid input, got {err:?}")
    };
    assert_eq!(field, "patch");
    let stamp: i64 =
        sqlx::query_scalar("SELECT updated_at FROM spoke_rules WHERE rule_id = 'rul_meta'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stamp, 1_700_000_100, "an empty PATCH writes nothing");

    // Addressing precedes payload: an unknown id and a foreign-World id stay
    // 404 even when the payload itself would be rejected.
    seed_world_rule(
        &pool,
        "rul_elsewhere",
        "wld_other_owned",
        "Elsewhere",
        "active",
        None,
        &[],
        json!({ "family": "module_presence", "module_key": "m" }),
    )
    .await;
    for rule_id in ["rul_unknown_invalid", "rul_elsewhere"] {
        let err = fx
            .core
            .update_world_rule(
                &fx.principal,
                world.to_string(),
                rule_id.to_string(),
                rule_update(json!({ "status": "banana" })),
            )
            .await
            .unwrap_err();
        let CoreError::NotFound { resource } = err else {
            panic!("{rule_id} must be not-found before the payload is judged, got {err:?}")
        };
        assert!(resource.contains(rule_id));
        assert!(
            !resource.contains("Elsewhere"),
            "no existence leak: {resource}"
        );
    }

    // The write guards cover PATCH as well as create.
    let err = fx
        .core
        .update_world_rule(
            &fx.principal,
            FOREIGN_WORLD.to_string(),
            "rul_any".to_string(),
            rule_update(json!({ "statement": "y" })),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::WorldOwnerDenied { .. }));

    // `status: deprecated` is the deactivation path; an omitted target axis on
    // a later edit leaves the stored axis alone, and a `null` axis is the same
    // absence on the wire.
    let deactivated = fx
        .core
        .update_world_rule(
            &fx.principal,
            world.to_string(),
            "rul_meta".to_string(),
            rule_update(json!({ "status": "deprecated" })),
        )
        .await
        .unwrap();
    assert_eq!(deactivated.status.as_deref(), Some("deprecated"));
    assert_eq!(
        deactivated.target_entry_types,
        vec!["x".to_string()],
        "an absent axis is not a clear"
    );

    let null_axis =
        rule_update(json!({ "target_entry_types": null, "statement": "null is absent" }));
    assert!(
        null_axis.target_entry_types.is_none(),
        "a JSON null axis is the absent member"
    );
    let after_null = fx
        .core
        .update_world_rule(
            &fx.principal,
            world.to_string(),
            "rul_meta".to_string(),
            null_axis,
        )
        .await
        .unwrap();
    assert_eq!(
        after_null.target_entry_types,
        vec!["x".to_string()],
        "null ≡ absent, so the stored axis survives"
    );

    // A matched patch refreshes `updated_at` and never touches `created_at`.
    let (created_at, updated_at): (i64, i64) =
        sqlx::query_as("SELECT created_at, updated_at FROM spoke_rules WHERE rule_id = 'rul_meta'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(created_at, 1_700_000_000, "created_at never moves");
    assert!(
        updated_at > 1_700_000_100,
        "a matched patch refreshes updated_at, got {updated_at}"
    );
}

fn relate(
    action: &str,
    relationship_id: Option<&str>,
    expected_version: Option<u64>,
    input: Option<Value>,
) -> WorldKbPatchRelationshipRequest {
    let mut body = json!({ "action": action });
    if let Some(id) = relationship_id {
        body["relationship_id"] = json!(id);
    }
    if let Some(version) = expected_version {
        body["expected_version"] = json!(version);
    }
    if let Some(input) = input {
        body["relationship"] = input;
    }
    serde_json::from_value(body).unwrap()
}

fn relate_input(source: &str, target: &str, relation_type: &str) -> Value {
    json!({
        "source_entity_id": source,
        "target_entity_id": target,
        "relation_type": relation_type,
        "symmetric": false,
        "source_anchor_ids": [],
    })
}

#[allow(clippy::too_many_arguments)] // seed helper: the row's columns are the fixture surface
async fn seed_relationship(
    pool: &SqlitePool,
    relationship_id: &str,
    world_id: &str,
    source: &str,
    target: &str,
    symmetric: bool,
    needs_review: bool,
    updated_at: &str,
) {
    sqlx::query(
        "INSERT INTO kb_relationships (relationship_id, world_id, source_entity_id, target_entity_id, relation_type, symmetric, confidence, source_anchor_ids, metadata, created_at, updated_at, revision, needs_review, source) VALUES (?, ?, ?, ?, 'allied_with', ?, NULL, '[]', '{}', ?, ?, 0, ?, 'manual')",
    )
    .bind(relationship_id)
    .bind(world_id)
    .bind(source)
    .bind(target)
    .bind(i64::from(symmetric))
    .bind(updated_at)
    .bind(updated_at)
    .bind(i64::from(needs_review))
    .execute(pool)
    .await
    .unwrap();
}

/// Relationship patch (migrated from `world_kb_relationships.rs`): the
/// validation refusals, the anchor round-trip, the symmetric reverse
/// projection, the CAS conflict and the cross-world scope refusal. The
/// `Axum` handler-level 422/403 envelopes and the `error_code()` vocabulary
/// went with the host.
#[tokio::test]
async fn retained_relationship_patch_validation_and_projection() {
    let fx = setup().await;
    let (_guard, pool) = live_write_pool(&fx).await;
    let world = "wld_rel";
    let other_world = "wld_rel_other";
    seed_world(&pool, world, CREATOR).await;
    seed_world(&pool, other_world, CREATOR).await;
    seed_kb(
        &pool,
        "kb_rel_a",
        world,
        "Aria",
        "confirmed",
        Some(0),
        None,
        None,
    )
    .await;
    seed_kb(
        &pool,
        "kb_rel_b",
        world,
        "Kael",
        "confirmed",
        Some(0),
        None,
        None,
    )
    .await;
    seed_kb(
        &pool,
        "kb_rel_source",
        world,
        "Sourced",
        "confirmed",
        Some(0),
        None,
        Some("work_source"),
    )
    .await;

    // Refusals: self-loop, custom without a label, an out-of-range confidence,
    // an unresolvable anchor and a same-name endpoint in another World.
    let refusals: Vec<Value> = vec![
        relate_input("kb_rel_a", "kb_rel_a", "allied_with"),
        relate_input("kb_rel_a", "kb_rel_b", "custom"),
        {
            let mut input = relate_input("kb_rel_a", "kb_rel_b", "allied_with");
            input["confidence"] = json!(1.5);
            input
        },
        {
            let mut input = relate_input("kb_rel_a", "kb_rel_b", "allied_with");
            input["source_anchor_ids"] = json!(["sa_kb_missing"]);
            input
        },
        relate_input("kb_rel_a", "kb_rel_nowhere", "allied_with"),
    ];
    for refusal in refusals {
        let err = fx
            .core
            .patch_world_kb_relationship(
                &fx.principal,
                world.to_string(),
                relate("add", None, Some(0), Some(refusal.clone())),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(err, CoreError::WorldKbValidation(_)),
            "expected validation refusal for {refusal}, got {err:?}"
        );
    }

    // A resolved anchor round-trips on the created row.
    let mut anchored = relate_input("kb_rel_source", "kb_rel_b", "allied_with");
    anchored["source_anchor_ids"] = json!(["sa_kb_rel_source"]);
    let created = fx
        .core
        .patch_world_kb_relationship(
            &fx.principal,
            world.to_string(),
            relate("add", None, Some(0), Some(anchored)),
        )
        .await
        .unwrap();
    assert_eq!(created.version, 1);
    let anchored_row = created.relationship.clone().expect("created row");
    assert_eq!(anchored_row.source_anchor_ids, vec!["sa_kb_rel_source"]);

    // Remove drops the row (null projection); a stale update conflicts with
    // the committed revision.
    let removed = fx
        .core
        .patch_world_kb_relationship(
            &fx.principal,
            world.to_string(),
            relate(
                "remove",
                Some(&anchored_row.relationship_id),
                Some(created.version),
                None,
            ),
        )
        .await
        .unwrap();
    assert!(removed.relationship.is_none());

    let added = fx
        .core
        .patch_world_kb_relationship(
            &fx.principal,
            world.to_string(),
            relate(
                "add",
                None,
                Some(0),
                Some(relate_input("kb_rel_a", "kb_rel_b", "rival_of")),
            ),
        )
        .await
        .unwrap();
    let relationship_id = added
        .relationship
        .as_ref()
        .expect("added row")
        .relationship_id
        .clone();
    let err = fx
        .core
        .patch_world_kb_relationship(
            &fx.principal,
            world.to_string(),
            relate(
                "update",
                Some(&relationship_id),
                Some(999),
                Some(relate_input("kb_rel_a", "kb_rel_b", "mentor_of")),
            ),
        )
        .await
        .unwrap_err();
    let CoreError::WorldKbConflict(details) = err else {
        panic!("expected conflict, got {err:?}")
    };
    assert_eq!(details.current_version, 1);
    assert_eq!(details.conflicting_path, "version");

    // A relationship owned by another World of the same creator is refused as
    // cross-world access.
    seed_kb(
        &pool,
        "kb_other_a",
        other_world,
        "Other A",
        "confirmed",
        Some(0),
        None,
        None,
    )
    .await;
    seed_kb(
        &pool,
        "kb_other_b",
        other_world,
        "Other B",
        "confirmed",
        Some(0),
        None,
        None,
    )
    .await;
    let foreign_rel = fx
        .core
        .patch_world_kb_relationship(
            &fx.principal,
            other_world.to_string(),
            relate(
                "add",
                None,
                Some(0),
                Some(relate_input("kb_other_a", "kb_other_b", "allied_with")),
            ),
        )
        .await
        .unwrap();
    let foreign_rel_id = foreign_rel
        .relationship
        .as_ref()
        .expect("row in the other world")
        .relationship_id
        .clone();
    for action in ["update", "remove"] {
        let input = (action == "update").then(|| relate_input("kb_rel_a", "kb_rel_b", "mentor_of"));
        let err = fx
            .core
            .patch_world_kb_relationship(
                &fx.principal,
                world.to_string(),
                relate(action, Some(&foreign_rel_id), Some(0), input),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(err, CoreError::Forbidden { .. }),
            "{action} of a foreign-World relationship must be refused, got {err:?}"
        );
    }

    // A symmetric relation emits both the stored and the mirrored projection;
    // the stored row keeps the swapped endpoints of its reverse.
    let symmetric = fx
        .core
        .patch_world_kb_relationship(
            &fx.principal,
            world.to_string(),
            relate(
                "update",
                Some(&relationship_id),
                Some(1),
                Some({
                    let mut input = relate_input("kb_rel_a", "kb_rel_b", "rival_of");
                    input["symmetric"] = json!(true);
                    input["confidence"] = json!(0.75);
                    input
                }),
            ),
        )
        .await
        .unwrap();
    assert_eq!(symmetric.version, 2);
    let graph = fx
        .core
        .world_kb_graph(&fx.principal, world.to_string(), false)
        .await
        .unwrap();
    let wire = serde_json::to_value(&graph).unwrap();
    let rows: Vec<&Value> = wire["relationships"]
        .as_array()
        .expect("relationships")
        .iter()
        .filter(|row| row["relationship_id"] == json!(relationship_id))
        .collect();
    assert_eq!(rows.len(), 2, "a symmetric pair projects twice: {wire}");
    let stored = rows
        .iter()
        .find(|row| row["projection_direction"] == "stored")
        .expect("stored projection");
    let reverse = rows
        .iter()
        .find(|row| row["projection_direction"] == "symmetric_reverse")
        .expect("reverse projection");
    assert_eq!(stored["relation_type"], "rival_of");
    assert_eq!(stored["confidence"], json!(0.75));
    assert_eq!(stored["source_entity_id"], reverse["target_entity_id"]);
    assert_eq!(stored["target_entity_id"], reverse["source_entity_id"]);
}

/// Relationship review gate, graph cap and stored-extension preservation
/// (migrated from `world_kb_relationships.rs`): extraction suggestions stay
/// hidden unless explicitly requested, promotion clears the gate, a routine
/// edit preserves it, the graph bounds its relationship page, and unknown
/// `extensions.nexus` keys survive an update.
#[tokio::test]
async fn retained_relationship_review_gate_and_extension_preservation() {
    let fx = setup().await;
    let (_guard, pool) = live_write_pool(&fx).await;
    let world = "wld_rel_gate";
    seed_world(&pool, world, CREATOR).await;
    seed_kb(
        &pool,
        "kb_gate_a",
        world,
        "Aria",
        "confirmed",
        Some(0),
        None,
        None,
    )
    .await;
    seed_kb(
        &pool,
        "kb_gate_b",
        world,
        "Kael",
        "confirmed",
        Some(0),
        None,
        None,
    )
    .await;
    seed_kb(
        &pool,
        "kb_gate_c",
        world,
        "Mira",
        "confirmed",
        Some(0),
        None,
        None,
    )
    .await;

    // One confirmed manual row plus one extraction suggestion.
    let confirmed = fx
        .core
        .patch_world_kb_relationship(
            &fx.principal,
            world.to_string(),
            relate(
                "add",
                None,
                Some(0),
                Some(relate_input("kb_gate_a", "kb_gate_c", "allied_with")),
            ),
        )
        .await
        .unwrap();
    assert!(
        confirmed.relationship.is_some(),
        "the manual add lands a confirmed row"
    );

    sqlx::query(
        "INSERT INTO kb_relationships (relationship_id, world_id, source_entity_id, target_entity_id, relation_type, symmetric, confidence, source_anchor_ids, metadata, created_at, updated_at, revision, needs_review, source) VALUES ('rel_suggested', ?, 'kb_gate_a', 'kb_gate_b', 'rival_of', 1, 0.8, '[]', '{}', '2026-06-30T00:00:01Z', '2026-06-30T00:00:01Z', 0, 1, 'extraction')",
    )
    .bind(world)
    .execute(&pool)
    .await
    .unwrap();

    let hidden = fx
        .core
        .world_kb_graph(&fx.principal, world.to_string(), false)
        .await
        .unwrap();
    assert_eq!(
        hidden.relationships.len(),
        1,
        "the default graph hides needs_review suggestions"
    );

    let shown = fx
        .core
        .world_kb_graph(&fx.principal, world.to_string(), true)
        .await
        .unwrap();
    let wire = serde_json::to_value(&shown).unwrap();
    assert_eq!(
        shown.relationships.len(),
        3,
        "the suggestion surfaces beside the symmetric pair: {wire}"
    );
    let suggestion = wire["relationships"]
        .as_array()
        .expect("relationships")
        .iter()
        .find(|row| row["relationship_id"] == "rel_suggested")
        .expect("suggestion listed");
    assert_eq!(suggestion["needs_review"], true);
    assert_eq!(suggestion["source"], "extraction");

    // A routine edit that omits `needs_review` must not silently confirm the
    // suggestion; an explicit `false` promotes it.
    let edited = fx
        .core
        .patch_world_kb_relationship(
            &fx.principal,
            world.to_string(),
            relate(
                "update",
                Some("rel_suggested"),
                Some(0),
                Some(relate_input("kb_gate_a", "kb_gate_b", "mentor_of")),
            ),
        )
        .await
        .unwrap();
    assert!(
        edited.relationship.as_ref().expect("row").needs_review,
        "an omitted needs_review preserves the gate"
    );

    let promoted = fx
        .core
        .patch_world_kb_relationship(
            &fx.principal,
            world.to_string(),
            relate(
                "update",
                Some("rel_suggested"),
                Some(edited.version),
                Some({
                    let mut input = relate_input("kb_gate_a", "kb_gate_b", "mentor_of");
                    input["needs_review"] = json!(false);
                    input
                }),
            ),
        )
        .await
        .unwrap();
    let promoted_row = promoted.relationship.expect("promoted row");
    assert!(!promoted_row.needs_review, "promotion clears the gate");
    assert_eq!(
        promoted_row.source.to_string(),
        "extraction",
        "promotion keeps the stored provenance"
    );
    let after = fx
        .core
        .world_kb_graph(&fx.principal, world.to_string(), false)
        .await
        .unwrap();
    assert!(
        after
            .relationships
            .iter()
            .any(|row| row.relationship_id == "rel_suggested"),
        "the promoted suggestion joins the default graph"
    );

    // Unknown stored extension keys survive a routine update. This arm runs in
    // its own World so the port's endpoint-resolution has exactly one row to
    // write through.
    let ext_world = "wld_rel_ext";
    seed_world(&pool, ext_world, CREATOR).await;
    seed_kb(
        &pool,
        "kb_ext_a",
        ext_world,
        "Ext A",
        "confirmed",
        Some(0),
        None,
        None,
    )
    .await;
    seed_kb(
        &pool,
        "kb_ext_b",
        ext_world,
        "Ext B",
        "confirmed",
        Some(0),
        None,
        None,
    )
    .await;
    let ext_row = fx
        .core
        .patch_world_kb_relationship(
            &fx.principal,
            ext_world.to_string(),
            relate(
                "add",
                None,
                Some(0),
                Some(relate_input("kb_ext_a", "kb_ext_b", "allied_with")),
            ),
        )
        .await
        .unwrap();
    let ext_id = ext_row
        .relationship
        .as_ref()
        .expect("seeded extension row")
        .relationship_id
        .clone();
    sqlx::query("UPDATE kb_relationships SET extensions_nexus_json = ? WHERE relationship_id = ?")
        .bind(r#"{"world_id":"wld_rel_ext","custom_tag":"imported","batch_id":"B42"}"#)
        .bind(&ext_id)
        .execute(&pool)
        .await
        .unwrap();
    let base_revision: i64 =
        sqlx::query_scalar("SELECT revision FROM kb_relationships WHERE relationship_id = ?")
            .bind(&ext_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let updated = fx
        .core
        .patch_world_kb_relationship(
            &fx.principal,
            ext_world.to_string(),
            relate(
                "update",
                Some(&ext_id),
                Some(u64::try_from(base_revision).unwrap()),
                Some(relate_input("kb_ext_a", "kb_ext_b", "opposes")),
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        updated.version,
        u64::try_from(base_revision + 1).unwrap(),
        "the update bumps the stored revision"
    );
    let stored: String = sqlx::query_scalar(
        "SELECT extensions_nexus_json FROM kb_relationships WHERE relationship_id = ?",
    )
    .bind(&ext_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let stored: Value = serde_json::from_str(&stored).unwrap();
    assert_eq!(stored["custom_tag"], "imported");
    assert_eq!(stored["batch_id"], "B42");
    let relation_type: String =
        sqlx::query_scalar("SELECT relation_type FROM kb_relationships WHERE relationship_id = ?")
            .bind(&ext_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(relation_type, "opposes", "the update really landed");

    // The relationship page is bounded: 1002 stored rows project at most the
    // cap, dropping the oldest.
    let capped_world = "wld_rel_cap";
    seed_world(&pool, capped_world, CREATOR).await;
    seed_kb(
        &pool,
        "kb_cap_a",
        capped_world,
        "Cap A",
        "confirmed",
        Some(0),
        None,
        None,
    )
    .await;
    seed_kb(
        &pool,
        "kb_cap_b",
        capped_world,
        "Cap B",
        "confirmed",
        Some(0),
        None,
        None,
    )
    .await;
    for i in 0..1002 {
        let stamp = format!(
            "2026-06-30T{:02}:{:02}:{:02}.000Z",
            i / 3600,
            (i % 3600) / 60,
            i % 60
        );
        seed_relationship(
            &pool,
            &format!("rel_cap_{i:04}"),
            capped_world,
            "kb_cap_a",
            "kb_cap_b",
            false,
            false,
            &stamp,
        )
        .await;
    }
    let capped = fx
        .core
        .world_kb_graph(&fx.principal, capped_world.to_string(), false)
        .await
        .unwrap();
    assert_eq!(
        capped.relationships.len(),
        1000,
        "the relationship page is capped at 1000"
    );
    let ids: Vec<&str> = capped
        .relationships
        .iter()
        .map(|row| row.relationship_id.as_str())
        .collect();
    assert!(!ids.contains(&"rel_cap_0000"), "the oldest rows drop first");
    assert!(ids.contains(&"rel_cap_1001"), "the newest row survives");
}

/// A merge whose target revision moved between the merge's read and its CAS
/// reports the TARGET as the conflict (migrated from `world_kb_patch.rs`
/// `promote_merge_target_cas_miss_marks_target_conflict`). The retired fixture
/// forced the miss with a held RESERVED write lock; here the concurrent writer
/// is simulated by a `BEFORE UPDATE` trigger that bumps the target and lets
/// the CAS row be skipped — the same zero-row CAS outcome without scheduler
/// dependence.
#[tokio::test]
async fn retained_promote_merge_target_cas_miss_marks_the_target() {
    let fx = setup().await;
    let (_guard, pool) = live_write_pool(&fx).await;
    let world = "wld_merge";
    seed_world(&pool, world, CREATOR).await;
    seed_kb(
        &pool,
        "kb_merge_target",
        world,
        "Aria",
        "confirmed",
        Some(0),
        Some(r#"{"summary":"Original"}"#),
        None,
    )
    .await;
    seed_pending_candidate(&pool, "xj_merge_c1", "we_merge_c1", world, "Racea").await;

    sqlx::query(
        "CREATE TRIGGER trg_move_target_revision BEFORE UPDATE ON kb_key_blocks \
         WHEN OLD.key_block_id = 'kb_merge_target' AND OLD.revision = 0 \
         BEGIN \
           UPDATE kb_key_blocks SET revision = COALESCE(revision, 0) + 1 \
             WHERE key_block_id = 'kb_merge_target'; \
           SELECT RAISE(IGNORE); \
         END",
    )
    .execute(&pool)
    .await
    .unwrap();

    let err = fx
        .core
        .promote_world_kb_candidate(
            &fx.principal,
            world.to_string(),
            promote_req("xj_merge_c1", "merge", 0, Some("kb_merge_target")),
        )
        .await
        .unwrap_err();
    sqlx::query("DROP TRIGGER trg_move_target_revision")
        .execute(&pool)
        .await
        .unwrap();

    let CoreError::WorldKbConflict(details) = err else {
        panic!("expected a target conflict, got {err:?}")
    };
    assert_eq!(
        details.conflicting_path, "merge_target",
        "the client must be able to tell a target conflict from a candidate one"
    );
    assert_eq!(details.entity_id, "kb_merge_target");
    assert_eq!(
        details.current_version, 1,
        "the conflict reports the moved target revision"
    );

    // Nothing was folded: the candidate is still pending and the target body
    // is untouched.
    let (status, body): (String, String) = sqlx::query_as(
        "SELECT promotion_status, proposed_payload FROM kb_extract_jobs WHERE job_id = 'xj_merge_c1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "pending");
    assert!(
        body.contains("A brave hero"),
        "the candidate payload is unchanged"
    );
    let target: String = sqlx::query_scalar(
        "SELECT body_json FROM kb_key_blocks WHERE key_block_id = 'kb_merge_target'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(target, r#"{"summary":"Original"}"#, "no partial fold");
}

async fn seed_attributed_key_block(
    pool: &SqlitePool,
    key_block_id: &str,
    world_id: &str,
    canonical_name: &str,
    created_from_command_id: &str,
) {
    sqlx::query(
        "INSERT INTO kb_key_blocks (key_block_id, world_id, block_type, canonical_name, status, revision, body_json, created_from_command_id, created_at, updated_at) VALUES (?, ?, 'character', ?, 'confirmed', 1, ?, ?, datetime('now'), datetime('now'))",
    )
    .bind(key_block_id)
    .bind(world_id)
    .bind(canonical_name)
    .bind(r#"{"summary":"A brave hero","attributes":{"novel_category":"character"}}"#)
    .bind(created_from_command_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Promote rollback and collision recovery (migrated from `world_kb_patch.rs`
/// `promote_adopt_rollbacks_entry_*`, `promote_adopt_*collision*` and
/// `promote_adopt_retry_recovers_*`): a failed job flip rolls the in-flight
/// entry back, a collision never auto-deletes a live entry, and a replayed
/// adopt on a confirmed job returns the attributed entry.
#[tokio::test]
async fn retained_promote_rollback_and_collision_recovery() {
    let fx = setup().await;
    let (_guard, pool) = live_write_pool(&fx).await;
    let world = "wld_promote";
    seed_world(&pool, world, CREATOR).await;
    let store = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());

    // (a) The job flip rejects the adopt after the entry insert — the entry
    //     must not survive, and the candidate must stay adoptable.
    seed_pending(
        &pool,
        "xj_rollback",
        world,
        "CompensateMe",
        "2020-01-01T00:00:01Z",
    )
    .await;
    sqlx::query(
        "CREATE TRIGGER trg_reject_flip AFTER INSERT ON kb_key_blocks WHEN NEW.canonical_name = 'CompensateMe' BEGIN UPDATE kb_extract_jobs SET promotion_status = 'rejected', version = version + 1 WHERE job_id = 'xj_rollback' AND promotion_status = 'pending'; END",
    )
    .execute(&pool)
    .await
    .unwrap();
    let err = fx
        .core
        .promote_world_kb_candidate(
            &fx.principal,
            world.to_string(),
            promote_req("xj_rollback", "adopt", 0, None),
        )
        .await
        .unwrap_err();
    sqlx::query("DROP TRIGGER trg_reject_flip")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        matches!(err, CoreError::WorldKbValidation(_)),
        "a raced job flip surfaces as a validation refusal, got {err:?}"
    );
    assert!(
        store
            .get_active_by_unique_key(world, "CompensateMe", nexus_contracts::BlockType::Character)
            .await
            .unwrap()
            .is_none(),
        "the atomic rollback leaves no confirmed entry behind"
    );
    let retried = fx
        .core
        .promote_world_kb_candidate(
            &fx.principal,
            world.to_string(),
            promote_req("xj_rollback", "adopt", 0, None),
        )
        .await
        .expect("the rolled-back candidate is still adoptable");
    assert_eq!(retried.job.status, "confirmed");

    // (b) A storage fault during the flip also rolls the entry back.
    seed_pending(
        &pool,
        "xj_cas_fault",
        world,
        "CasFailMe",
        "2020-01-01T00:00:02Z",
    )
    .await;
    sqlx::query(
        "CREATE TRIGGER trg_abort_flip BEFORE UPDATE ON kb_extract_jobs WHEN OLD.job_id = 'xj_cas_fault' AND NEW.promotion_status = 'confirmed' BEGIN SELECT RAISE(ABORT, 'simulated flip CAS failure'); END",
    )
    .execute(&pool)
    .await
    .unwrap();
    let err = fx
        .core
        .promote_world_kb_candidate(
            &fx.principal,
            world.to_string(),
            promote_req("xj_cas_fault", "adopt", 0, None),
        )
        .await
        .unwrap_err();
    sqlx::query("DROP TRIGGER trg_abort_flip")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        matches!(err, CoreError::Internal { .. }),
        "a storage fault during the flip surfaces as internal, got {err:?}"
    );
    assert!(
        store
            .get_active_by_unique_key(world, "CasFailMe", nexus_contracts::BlockType::Character)
            .await
            .unwrap()
            .is_none(),
        "a failed flip never leaves a half-adopted entry"
    );

    // (c) A live entry stamped with the same job while it is still pending is
    //     never deleted by a retry.
    seed_attributed_key_block(
        &pool,
        "kb_orphan_prior",
        world,
        "OrphanRetry",
        "xj_orphan_retry",
    )
    .await;
    seed_pending(
        &pool,
        "xj_orphan_retry",
        world,
        "OrphanRetry",
        "2020-01-01T00:00:03Z",
    )
    .await;
    let err = fx
        .core
        .promote_world_kb_candidate(
            &fx.principal,
            world.to_string(),
            promote_req("xj_orphan_retry", "adopt", 0, None),
        )
        .await
        .unwrap_err();
    let CoreError::WorldKbValidation(summary) = &err else {
        panic!("expected validation refusal, got {err:?}")
    };
    assert!(
        summary
            .validation_summary
            .errors
            .iter()
            .any(|message| message.contains("still pending")),
        "the pending collision is surfaced: {summary:?}"
    );
    let survivor = store
        .get_active_by_unique_key(world, "OrphanRetry", nexus_contracts::BlockType::Character)
        .await
        .unwrap()
        .expect("the attributed entry survives the retry");
    assert_eq!(survivor.entry_id, "kb_orphan_prior");

    // (d) An independent (unattributed) collision is reported, never removed.
    sqlx::query("UPDATE kb_extract_jobs SET promotion_status = 'pending', version = 0, created_at = '2020-01-01T00:00:04Z' WHERE job_id = 'xj_orphan_retry'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE kb_key_blocks SET created_from_command_id = NULL WHERE key_block_id = 'kb_orphan_prior'")
        .execute(&pool)
        .await
        .unwrap();
    let err = fx
        .core
        .promote_world_kb_candidate(
            &fx.principal,
            world.to_string(),
            promote_req("xj_orphan_retry", "adopt", 0, None),
        )
        .await
        .unwrap_err();
    let CoreError::WorldKbValidation(summary) = &err else {
        panic!("expected validation refusal, got {err:?}")
    };
    assert!(
        summary
            .validation_summary
            .errors
            .iter()
            .any(|message| message.contains("still pending")),
        "the unattributed collision is surfaced: {summary:?}"
    );
    assert!(
        !summary
            .validation_summary
            .errors
            .iter()
            .any(|message| message.contains("automatically removed")),
        "a retry never auto-deletes a live entry: {summary:?}"
    );
    assert!(
        store
            .get_active_by_unique_key(world, "OrphanRetry", nexus_contracts::BlockType::Character)
            .await
            .unwrap()
            .is_some(),
        "the independent entry is untouched"
    );

    // (e) A confirmed job whose entry is attributed recovers idempotently.
    seed_attributed_key_block(&pool, "kb_recovered", world, "RetryRecover", "xj_recovered").await;
    seed_pending(
        &pool,
        "xj_recovered",
        world,
        "RetryRecover",
        "2020-01-01T00:00:05Z",
    )
    .await;
    sqlx::query("UPDATE kb_extract_jobs SET promotion_status = 'confirmed', version = 1 WHERE job_id = 'xj_recovered'")
        .execute(&pool)
        .await
        .unwrap();
    let recovered = fx
        .core
        .promote_world_kb_candidate(
            &fx.principal,
            world.to_string(),
            promote_req("xj_recovered", "adopt", 0, None),
        )
        .await
        .expect("a confirmed attributed adopt replays as success");
    let entity = recovered.entity.expect("recovered entity");
    assert_eq!(entity.key_block_id, "kb_recovered");
    assert_eq!(recovered.job.status, "confirmed");
    let attributed = store
        .get_active_by_unique_key(world, "RetryRecover", nexus_contracts::BlockType::Character)
        .await
        .unwrap()
        .expect("single active entry");
    assert_eq!(attributed.entry_id, "kb_recovered");

    // (f) The adopt refinement set is closed: `patch.modules` is refused
    //     rather than silently dropped.
    let mut refinement: WorldKbPromoteCandidateRequest =
        promote_req("xj_recovered", "adopt", 1, None);
    refinement.patch = Some(
        serde_json::from_value(json!({ "modules": { "mental": { "goals": ["x"] } } })).unwrap(),
    );
    let err = fx
        .core
        .promote_world_kb_candidate(&fx.principal, world.to_string(), refinement)
        .await
        .unwrap_err();
    let CoreError::InvalidInput { field, .. } = err else {
        panic!("expected invalid input, got {err:?}")
    };
    assert_eq!(field, "patch.modules");
}

/// Promote CAS details and the candidate walk (migrated from
/// `world_kb_patch.rs` `promote_reject_cas_miss_conflict_carries_bumped_version`,
/// `get_candidates_multi_page_cursor_reaches_all_rows` and
/// `get_candidates_distinct_candidate_id_for_same_canonical_name`): a CAS miss
/// reports the re-read version, a cursor walk reaches every pending candidate
/// exactly once, and `candidate_id` is the unique row key.
#[tokio::test]
async fn retained_promote_cas_details_and_candidate_walk() {
    let fx = setup().await;
    let (_guard, pool) = live_write_pool(&fx).await;
    let world = "wld_candidates";
    seed_world(&pool, world, CREATOR).await;

    for idx in 0..4 {
        seed_pending_candidate(
            &pool,
            &format!("xj_walk_{idx}"),
            &format!("we_source_{idx}"),
            world,
            &format!("Cand {idx}"),
        )
        .await;
    }
    // A concurrent write bumps the version while the candidate stays pending:
    // the losing transition must report the re-read version, not the stale
    // expectation.
    seed_pending_candidate(&pool, "xj_cas_miss", "we_cas", world, "Racea").await;
    sqlx::query("UPDATE kb_extract_jobs SET version = version + 1 WHERE job_id = 'xj_cas_miss'")
        .execute(&pool)
        .await
        .unwrap();
    let err = fx
        .core
        .promote_world_kb_candidate(
            &fx.principal,
            world.to_string(),
            promote_req("xj_cas_miss", "reject", 0, None),
        )
        .await
        .unwrap_err();
    let CoreError::WorldKbConflict(details) = err else {
        panic!("expected conflict, got {err:?}")
    };
    assert_eq!(
        details.current_version, 1,
        "the conflict reports the re-read version"
    );
    assert_eq!(details.entity_id, "xj_cas_miss");

    // Two candidates that guess the same canonical name stay addressable: the
    // candidate id is the row key, not the guessed display name.
    seed_pending_candidate(&pool, "xj_twin_a", "we_twin_a", world, "Duplicate Name").await;
    seed_pending_candidate(&pool, "xj_twin_b", "we_twin_b", world, "Duplicate Name").await;

    let page1 = fx
        .core
        .world_kb_candidates(&fx.principal, world.to_string(), Some(2), None)
        .await
        .unwrap();
    assert_eq!(page1.items.len(), 2);
    assert!(page1.pagination.has_more);
    let cursor = page1.pagination.next_cursor.clone().expect("next cursor");
    let page2 = fx
        .core
        .world_kb_candidates(&fx.principal, world.to_string(), Some(2), Some(cursor))
        .await
        .unwrap();
    assert_eq!(
        page2.items.len(),
        2,
        "the second page reaches the remaining rows"
    );

    let full = fx
        .core
        .world_kb_candidates(&fx.principal, world.to_string(), None, None)
        .await
        .unwrap();
    assert_eq!(full.items.len(), 7, "every pending candidate is listed");
    let ids: Vec<String> = full.items.iter().map(|item| item.job_id.clone()).collect();
    let mut unique = ids.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), ids.len(), "no duplication across the walk");
    assert!(
        full.items
            .iter()
            .all(|item| item.candidate_id == item.job_id),
        "candidate_id is the unique row key"
    );
    let twins: Vec<&str> = full
        .items
        .iter()
        .filter(|item| item.canonical_name == "Duplicate Name")
        .map(|item| item.candidate_id.as_str())
        .collect();
    assert_eq!(twins.len(), 2, "same-name candidates both list");
    assert_ne!(twins[0], twins[1], "their candidate ids stay distinct");

    let walked: Vec<String> = page1
        .items
        .iter()
        .chain(page2.items.iter())
        .map(|item| item.job_id.clone())
        .collect();
    assert_eq!(
        walked.len(),
        walked
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        "the two pages never repeat a row"
    );
}

async fn seed_pending_candidate(
    pool: &SqlitePool,
    job_id: &str,
    work_entry_id: &str,
    world_id: &str,
    canonical_name_guess: &str,
) {
    sqlx::query(
        "INSERT INTO kb_extract_jobs (job_id, creator_id, workspace_id, work_entry_id, world_id, status, promotion_status, proposed_payload, block_type_guess, canonical_name_guess, version) VALUES (?, ?, 'ws', ?, ?, 'done', 'pending', ?, 'character', ?, 0)",
    )
    .bind(job_id)
    .bind(CREATOR)
    .bind(work_entry_id)
    .bind(world_id)
    .bind(r#"{"summary":"A brave hero","attributes":{"novel_category":"character"}}"#)
    .bind(canonical_name_guess)
    .execute(pool)
    .await
    .unwrap();
}
