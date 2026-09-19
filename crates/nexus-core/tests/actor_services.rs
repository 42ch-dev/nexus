//! P2-T1 actor services contract tests (core service semantics): two
//! independently opened cores fence one Character through the shared
//! activity/exclusive transition leases (busy refusal, epoch commit, stale
//! token), stored ownership admission keeps its deny matrix, the bounded
//! admission view keeps its hard cap, and the binding family keeps its
//! stable conflicts (`duplicate_active_actor_world_binding`,
//! `last_active_actor_world_binding`) plus foreign-owner 404s.

use nexus_contracts::generated::core::{
    CoreCharacterTransitionRequest, CoreCharacterTransitionRequestTargetStatus,
};
use nexus_contracts::BlockType;
use nexus_core::{
    classify_pair, ActorFenceKind, ActorKnowledgeViewQuery, ActorViewpoint, AdmittedActor,
    CoreAccess, CoreActorAdmission, CoreError, CoreOpenOptions, CoreService,
};
use nexus_contracts::generated::daemon_api::actor_knowledge::add_knowledge_entry_request::AddKnowledgeEntryRequest;
use nexus_knowledge::world_kb::knowledge_entry::{
    KnowledgeAudience, KnowledgeEntryRecord, KnowledgeOwnerRef, DISCLOSURE_OWNER_PRIVATE,
};
use nexus_knowledge::world_kb::store::{KbStore, KnowledgeReadPolicy, KnowledgeReadScope};
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_local_db::writer_protocol::{init_engine_pool, GuardedPoolOptions};
use nexus_local_db::{ensure_creator_row, CreateCharacterParams};
use sqlx::SqlitePool;
use std::path::PathBuf;
use tempfile::TempDir;

const CREATOR: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER: &str = "ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const WORLD: &str = "wld_worldA";
const WORLD_B: &str = "wld_worldB";
const FOREIGN_WORLD: &str = "wld_otherWorld";

struct Env {
    _tmp: TempDir,
    user_home: PathBuf,
    db_path: PathBuf,
    character_id: String,
    binding_id: String,
    foreign_character_id: String,
    foreign_binding_id: String,
}

fn assert_conflict(err: CoreError, code: &str) {
    match err {
        CoreError::ActorConflict { code: got, .. } => assert_eq!(got, code, "conflict code"),
        other => panic!("expected ActorConflict {code}, got {other:?}"),
    }
}

fn assert_not_found(err: &CoreError) {
    assert!(
        matches!(err, CoreError::NotFound { .. }),
        "expected NotFound, got {err:?}"
    );
}

fn assert_invalid_input(err: &CoreError) {
    assert!(
        matches!(err, CoreError::ActorInput(_)),
        "expected ActorInput, got {err:?}"
    );
}

async fn seed_world(pool: &SqlitePool, world_id: &str, owner: &str) {
    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, created_at) \
         VALUES (?, 'wrk', ?, ?, ?, 'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .bind(world_id)
    .bind(owner)
    .bind(world_id)
    .bind(world_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_character(
    pool: &SqlitePool,
    owner: &str,
    world_id: &str,
    display_name: &str,
) -> (String, String) {
    let created = nexus_local_db::create_character_with_initial_binding(
        pool,
        CreateCharacterParams {
            owner_creator_id: owner,
            display_name,
            image_uri: None,
            persona_json: "{}",
            world_id,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    (created.character.character_id, created.binding.binding_id)
}

/// Materialize the workspace shell and seed stored rows through one engine
/// pool (released before the cores open, mirroring the daemon boot).
async fn seed_env() -> Env {
    let tmp = TempDir::new().unwrap();
    let user_home = tmp.path().to_path_buf();
    let nexus_home = user_home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        &user_home, CREATOR, "default",
    ))
    .unwrap();
    std::fs::write(
        nexus_home.join("config.toml"),
        format!(
            "active_creator_id = \"{CREATOR}\"\n\
             [active_workspace_slug_by_creator]\n\
             \"{CREATOR}\" = \"default\""
        ),
    )
    .unwrap();
    let db_path = nexus_home_layout::workspace_state_db_path(&user_home, CREATOR, "default");

    let (character_id, binding_id, foreign_character_id, foreign_binding_id) = {
        let guarded = init_engine_pool(&db_path, CREATOR, GuardedPoolOptions::default())
            .await
            .unwrap();
        let pool = guarded.clone_pool();
        ensure_creator_row(&pool, CREATOR, "Owner").await.unwrap();
        ensure_creator_row(&pool, OTHER, "Other").await.unwrap();
        seed_world(&pool, WORLD, CREATOR).await;
        seed_world(&pool, WORLD_B, CREATOR).await;
        seed_world(&pool, FOREIGN_WORLD, OTHER).await;
        let (character_id, binding_id) = seed_character(&pool, CREATOR, WORLD, "Ada").await;
        let (foreign_character_id, foreign_binding_id) =
            seed_character(&pool, OTHER, FOREIGN_WORLD, "Foreign").await;
        (
            character_id,
            binding_id,
            foreign_character_id,
            foreign_binding_id,
        )
    };

    Env {
        _tmp: tmp,
        user_home,
        db_path,
        character_id,
        binding_id,
        foreign_character_id,
        foreign_binding_id,
    }
}

/// Open one engine-owner core against the seeded workspace. Two calls yield
/// two independently fenced services (the second joins the live engine pool).
async fn open_core(env: &Env) -> (CoreService, nexus_core::Principal) {
    let core = CoreService::open(CoreOpenOptions {
        user_home: env.user_home.clone(),
        access: CoreAccess::EngineOwner,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    (core, principal)
}

/// A plain pool for admission composition and DB-state assertions.
async fn plain_pool(env: &Env) -> SqlitePool {
    nexus_local_db::open_pool(&env.db_path).await.unwrap()
}

async fn character_row(env: &Env, character_id: &str) -> (String, i64, i64) {
    let pool = plain_pool(env).await;
    let (status, revision, epoch): (String, i64, i64) = sqlx::query_as(
        "SELECT status, revision, lifecycle_epoch FROM characters WHERE character_id = ?",
    )
    .bind(character_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    pool.close().await;
    (status, revision, epoch)
}

fn transition_request(
    character_id: &str,
    expected_revision: i64,
    target: CoreCharacterTransitionRequestTargetStatus,
) -> CoreCharacterTransitionRequest {
    CoreCharacterTransitionRequest::builder()
        .character_id(character_id.to_string())
        .expected_revision(expected_revision)
        .target_status(target)
        .try_into()
        .expect("transition request is wire-valid")
}

/// The acceptance race: two independently opened cores compete on one
/// Character. The active effect retains its shared lease (the second core's
/// transition refuses busy), after release the transition commits exactly one
/// epoch, and the previously admitted token is stale afterwards.
#[tokio::test]
async fn transition_races_admitted_activity() {
    let env = seed_env().await;
    let (core_a, principal_a) = open_core(&env).await;
    let (core_b, principal_b) = open_core(&env).await;
    let actor = AdmittedActor::Character {
        character_id: env.character_id.clone(),
    };

    // Active effect holds the shared lease on the first core.
    let activity = core_a
        .acquire_actor_activity(&principal_a, &actor)
        .await
        .expect("activity admission before transition");
    assert_eq!(activity.character_id(), env.character_id);
    assert_eq!(activity.epoch(), 0);

    // The second core's transition refuses busy — never waits, never cancels.
    let err = core_b
        .acquire_character_transition(&principal_b, env.character_id.clone())
        .await
        .expect_err("transition must refuse while activity is outstanding");
    assert_conflict(err, "character_busy");

    // Release; the transition then commits exactly one epoch.
    drop(activity);
    let (_, pre_revision, pre_epoch) = character_row(&env, &env.character_id).await;
    let response = core_b
        .transition_character(
            &principal_b,
            transition_request(
                &env.character_id,
                pre_revision,
                CoreCharacterTransitionRequestTargetStatus::Archived,
            ),
        )
        .await
        .expect("transition commits after the lease drains");
    let (status, revision, epoch) = character_row(&env, &env.character_id).await;
    assert_eq!(status, "archived");
    assert_eq!(revision, pre_revision + 1);
    assert_eq!(epoch, pre_epoch + 1);
    assert_eq!(response.character.status.to_string(), "archived");

    // The old admitted token is stale: activity on the pre-transition token
    // now observes the archived row under the fence.
    let err = core_a
        .acquire_actor_activity(&principal_a, &actor)
        .await
        .expect_err("activity must be refused on the stale token");
    assert_conflict(err, "character_inactive");
}

#[allow(clippy::significant_drop_tightening)] // the lease is deliberately held across the next call
/// The reverse race of [`transition_races_admitted_activity`]: while another
/// core HOLDS the exclusive transition lease, an activity admission must
/// report `character_busy` promptly instead of parking behind the holder.
///
/// This is the half the forward test cannot see. The forward test proves a
/// transition refuses an outstanding effect; here the effect is the one that
/// must not wait. A blocking activity acquisition (e.g. `lock_shared()` on a
/// blocking pool) would deadlock here until the lease drops, stalling
/// admission and cancellation — the lease contract forbids that ("shared
/// effect leases/exclusive transition leases are nonblocking — busy is
/// observable").
///
/// The bounded `timeout` IS the assertion: a waiting implementation trips it
/// with a clear message rather than hanging the suite.
#[tokio::test]
async fn activity_admission_is_busy_while_a_transition_lease_is_held() {
    let env = seed_env().await;
    let (core_a, principal_a) = open_core(&env).await;
    let (core_b, principal_b) = open_core(&env).await;
    let actor = AdmittedActor::Character {
        character_id: env.character_id.clone(),
    };

    // core_b holds the exclusive transition lease — acquired, not yet
    // committed, and deliberately still held across the next call.
    let lease = core_b
        .acquire_character_transition(&principal_b, env.character_id.clone())
        .await
        .expect("transition lease acquisition");

    let refused = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        core_a.acquire_actor_activity(&principal_a, &actor),
    )
    .await
    .expect("activity admission must not wait for the held transition lease");
    let err = refused.expect_err("activity must be refused while a transition is held");
    assert_conflict(err, "character_busy");

    // Releasing the lease admits the effect on the same core, still at the
    // pre-transition epoch (nothing was committed).
    drop(lease);
    let activity = core_a
        .acquire_actor_activity(&principal_a, &actor)
        .await
        .expect("activity admits once the transition lease drains");
    assert_eq!(activity.character_id(), env.character_id);
    assert_eq!(activity.epoch(), 0);
}

/// Ported from the daemon `characters_api.rs` stable-conflict test at the
/// planning baseline: duplicate active bindings conflict, and removing the
/// last active binding is a zero-mutation stable conflict.
#[tokio::test]
async fn duplicate_active_binding_and_last_binding_are_stable_conflicts() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;

    let dup = core
        .add_binding(
            &principal,
            env.character_id.clone(),
            WORLD.to_string(),
            None,
        )
        .await
        .expect_err("duplicate active binding for one world must conflict");
    assert_conflict(dup, "duplicate_active_actor_world_binding");

    let second = core
        .add_binding(
            &principal,
            env.character_id.clone(),
            WORLD_B.to_string(),
            None,
        )
        .await
        .expect("second binding on another world");
    let second_id = second.binding.binding_id.as_str().to_string();

    core.remove_binding(&principal, env.character_id.clone(), second_id.clone())
        .await
        .expect("non-last binding removes");

    let last = core
        .remove_binding(&principal, env.character_id.clone(), env.binding_id.clone())
        .await
        .expect_err("removing the last active binding must conflict");
    assert_conflict(last, "last_active_actor_world_binding");

    let pool = plain_pool(&env).await;
    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM actor_world_bindings WHERE character_id = ?")
            .bind(&env.character_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    pool.close().await;
    assert_eq!(remaining, 1, "the refused removal must not mutate");
}

/// Ported from the daemon `characters_api.rs` foreign-owner test: every
/// Character/binding route is 404 for a foreign Character and mutates nothing.
#[tokio::test]
async fn foreign_owner_routes_are_not_found_and_do_not_mutate() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let foreign = &env.foreign_character_id;

    let err = core
        .add_binding(&principal, foreign.clone(), WORLD.to_string(), None)
        .await
        .expect_err("foreign add must be 404");
    assert_not_found(&err);

    let err = core
        .list_bindings(&principal, foreign.clone(), 50, 0)
        .await
        .expect_err("foreign list must be 404");
    assert_not_found(&err);

    let err = core
        .binding(&principal, foreign.clone(), env.foreign_binding_id.clone())
        .await
        .expect_err("foreign read must be 404");
    assert_not_found(&err);

    let err = core
        .character(&principal, foreign.clone())
        .await
        .expect_err("foreign detail must be 404");
    assert_not_found(&err);

    let err = core
        .remove_binding(&principal, foreign.clone(), env.foreign_binding_id.clone())
        .await
        .expect_err("foreign removal must be 404");
    assert_not_found(&err);

    let pool = plain_pool(&env).await;
    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM actor_world_bindings WHERE character_id = ?")
            .bind(foreign)
            .fetch_one(&pool)
            .await
            .unwrap();
    pool.close().await;
    assert_eq!(remaining, 1, "foreign routes must not mutate");
}

/// Ported deny matrix from the daemon admission tests: ownership mismatches
/// fail closed with the retained stable codes, and the happy path returns the
/// trusted context plus the stored Character epoch.
#[tokio::test]
#[allow(clippy::too_many_lines)] // one assertion per deny class
async fn admission_deny_matrix_world_character_binding_mismatches() {
    let env = seed_env().await;
    let pool = plain_pool(&env).await;
    let admission = CoreActorAdmission::new(pool.clone());

    let viewpoint = |world: &str, binding: Option<&str>| ActorViewpoint {
        world_id: world.to_string(),
        binding_id: binding.map(str::to_string),
        branch_id: None,
        event_id: None,
    };
    let chr = &env.character_id;
    let bid = env.binding_id.as_str();

    // Happy path: trusted owner, binding and stored Character epoch.
    let ctx = admission
        .admit(
            CREATOR,
            AdmittedActor::Character {
                character_id: chr.clone(),
            },
            viewpoint(WORLD, Some(bid)),
        )
        .await
        .expect("owner admission");
    assert_eq!(ctx.owner_creator_id, CREATOR);
    assert_eq!(ctx.world_id, WORLD);
    assert_eq!(ctx.binding_id.as_deref(), Some(bid));
    assert_eq!(ctx.character_epoch, Some(0));

    // Foreign creator token and Creator-with-binding shape.
    let err = admission
        .admit(
            CREATOR,
            AdmittedActor::Creator {
                creator_id: OTHER.to_string(),
            },
            viewpoint(WORLD, None),
        )
        .await
        .expect_err("foreign creator token");
    assert_not_found(&err);

    let err = admission
        .admit(
            CREATOR,
            AdmittedActor::Creator {
                creator_id: CREATOR.to_string(),
            },
            viewpoint(WORLD, Some(bid)),
        )
        .await
        .expect_err("creator with binding");
    assert_invalid_input(&err);

    // Pair classification stays a stable 400 on partial pairs.
    assert!(matches!(
        classify_pair(true, false),
        Err(CoreError::ActorInput(_))
    ));

    // Missing/foreign World → 404; owned archived World → 409 world_inactive.
    let err = admission
        .admit(
            CREATOR,
            AdmittedActor::Character {
                character_id: chr.clone(),
            },
            viewpoint("wld_missing", Some(bid)),
        )
        .await
        .expect_err("missing world");
    assert_not_found(&err);

    sqlx::query("UPDATE narrative_worlds SET status = 'archived' WHERE world_id = ?")
        .bind(WORLD)
        .execute(&pool)
        .await
        .unwrap();
    let err = admission
        .admit(
            CREATOR,
            AdmittedActor::Character {
                character_id: chr.clone(),
            },
            viewpoint(WORLD, Some(bid)),
        )
        .await
        .expect_err("inactive world");
    assert_conflict(err, "world_inactive");
    sqlx::query("UPDATE narrative_worlds SET status = 'active' WHERE world_id = ?")
        .bind(WORLD)
        .execute(&pool)
        .await
        .unwrap();

    // Owned archived Character → 409 character_inactive.
    sqlx::query("UPDATE characters SET status = 'archived' WHERE character_id = ?")
        .bind(chr)
        .execute(&pool)
        .await
        .unwrap();
    let err = admission
        .admit(
            CREATOR,
            AdmittedActor::Character {
                character_id: chr.clone(),
            },
            viewpoint(WORLD, Some(bid)),
        )
        .await
        .expect_err("archived character");
    assert_conflict(err, "character_inactive");
    sqlx::query("UPDATE characters SET status = 'active' WHERE character_id = ?")
        .bind(chr)
        .execute(&pool)
        .await
        .unwrap();

    // Inactive binding → 404 (existence hidden).
    sqlx::query("UPDATE actor_world_bindings SET status = 'inactive' WHERE binding_id = ?")
        .bind(bid)
        .execute(&pool)
        .await
        .unwrap();
    let err = admission
        .admit(
            CREATOR,
            AdmittedActor::Character {
                character_id: chr.clone(),
            },
            viewpoint(WORLD, Some(bid)),
        )
        .await
        .expect_err("inactive binding");
    assert_not_found(&err);
    sqlx::query("UPDATE actor_world_bindings SET status = 'active' WHERE binding_id = ?")
        .bind(bid)
        .execute(&pool)
        .await
        .unwrap();

    // Cross-Character binding and binding targeting a foreign world → 404.
    let err = admission
        .admit(
            CREATOR,
            AdmittedActor::Character {
                character_id: chr.clone(),
            },
            viewpoint(WORLD, Some(&env.foreign_binding_id)),
        )
        .await
        .expect_err("cross-character binding");
    assert_not_found(&err);

    let err = admission
        .admit(
            CREATOR,
            AdmittedActor::Character {
                character_id: chr.clone(),
            },
            viewpoint(FOREIGN_WORLD, Some(bid)),
        )
        .await
        .expect_err("binding targeting foreign world");
    assert_not_found(&err);

    pool.close().await;
}

async fn insert_world_entries(pool: &SqlitePool, n: usize) {
    let store = SqliteKbStore::new(pool.clone());
    for i in 0..n {
        let mut row = KnowledgeEntryRecord::new(WORLD, BlockType::Item, &format!("WorldRow{i:03}"));
        let minute = i / 60;
        let second = i % 60;
        row.created_at = format!("2026-01-01T00:{minute:02}:{second:02}Z");
        store.insert_knowledge_entry(row).await.unwrap();
    }
}

/// The bounded admission view merges keyset pages under the hard cap.
#[tokio::test]
async fn admitted_view_follows_pages_under_cap() {
    let env = seed_env().await;
    let pool = plain_pool(&env).await;
    insert_world_entries(&pool, 150).await;
    let admission = CoreActorAdmission::new(pool.clone());
    let ctx = admission
        .admit(
            CREATOR,
            AdmittedActor::Character {
                character_id: env.character_id.clone(),
            },
            ActorViewpoint {
                world_id: WORLD.to_string(),
                binding_id: Some(env.binding_id.clone()),
                branch_id: None,
                event_id: None,
            },
        )
        .await
        .expect("admit 150");
    assert_eq!(ctx.view.items.len(), 150);
    assert!(!ctx.view.has_more);
    pool.close().await;
}

/// Beyond the hard cap the bounded view fails closed with the retained
/// `view_incomplete` code — never a partial page.
#[tokio::test]
async fn admitted_view_rejects_when_hard_cap_exceeded() {
    let env = seed_env().await;
    let pool = plain_pool(&env).await;
    insert_world_entries(&pool, 201).await;
    let admission = CoreActorAdmission::new(pool.clone());
    let err = admission
        .admit(
            CREATOR,
            AdmittedActor::Character {
                character_id: env.character_id.clone(),
            },
            ActorViewpoint {
                world_id: WORLD.to_string(),
                binding_id: Some(env.binding_id.clone()),
                branch_id: None,
                event_id: None,
            },
        )
        .await
        .expect_err("cap");
    assert_conflict(err, "view_incomplete");
    pool.close().await;
}

// ── v1.191 P1 T4 — holder lifecycle provisioning ────────────────────────
//
// Durable contract: `.mstar/specs/holder-governance.md` §2.1 (one stable holder
// per stored Creator/Character, committed with the subject) and §2.2 (rename/
// archive/restore keep the holder, retained archived reads resolve it, and a
// normal read never provisions a missing/corrupt registry row).

/// The holder registry row count of one subject, read from the workspace DB.
async fn holder_count(env: &Env, column: &str, subject_id: &str) -> i64 {
    let pool = plain_pool(env).await;
    let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM knowledge_holders WHERE {column} = ?"
    )))
    .bind(subject_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    pool.close().await;
    count
}

/// Remove one subject's holder registry row behind the API: the only reachable
/// way to break the subject+holder invariant (corrupt registry state).
async fn drop_holder_row(env: &Env, column: &str, subject_id: &str) {
    let pool = plain_pool(env).await;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DELETE FROM knowledge_holders WHERE {column} = ?"
    )))
    .bind(subject_id)
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;
}

#[allow(clippy::too_many_lines)] // one lifecycle round-trip asserted end to end
#[tokio::test]
async fn v1191_holder_lifecycle_actor_holders_are_stable_across_the_lifecycle() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let pool = plain_pool(&env).await;

    // Both Actor kinds resolve the derived holder on a normal read.
    let creator_holder = nexus_core::require_actor_holder(
        &pool,
        CREATOR,
        &AdmittedActor::Creator {
            creator_id: CREATOR.to_string(),
        },
    )
    .await
    .expect("Creator holder");
    assert_eq!(
        creator_holder,
        nexus_local_db::creator_holder_entry_id(CREATOR)
    );
    let character_actor = AdmittedActor::Character {
        character_id: env.character_id.clone(),
    };
    let character_holder = nexus_core::require_actor_holder(&pool, CREATOR, &character_actor)
        .await
        .expect("Character holder");
    assert_eq!(
        character_holder,
        nexus_local_db::character_holder_entry_id(&env.character_id)
    );
    assert_eq!(holder_count(&env, "creator_id", CREATOR).await, 1);
    assert_eq!(
        holder_count(&env, "character_id", &env.character_id).await,
        1
    );

    // Rename: the projected label moves, the holder does not.
    let renamed = core
        .patch_character(
            &principal,
            env.character_id.clone(),
            0,
            nexus_local_db::CharacterPatch {
                display_name: Some("Ada Renamed"),
                image_uri: nexus_local_db::FieldPatch::Keep,
                persona_json: nexus_local_db::FieldPatch::Keep,
            },
        )
        .await
        .expect("rename");
    assert_eq!(
        String::from(renamed.character.display_name.clone()),
        "Ada Renamed",
        "the rename lands on the projected label"
    );
    let (_, revision, _) = character_row(&env, &env.character_id).await;

    // Archive keeps the holder and the retained management read answers.
    core.transition_character(
        &principal,
        transition_request(
            &env.character_id,
            revision,
            CoreCharacterTransitionRequestTargetStatus::Archived,
        ),
    )
    .await
    .expect("archive");
    assert_eq!(
        nexus_core::require_actor_holder(&pool, CREATOR, &character_actor)
            .await
            .expect("archived retained holder read"),
        character_holder
    );
    assert_eq!(
        character_row(&env, &env.character_id).await.0,
        "archived".to_string()
    );

    // Restore reuses the same holder.
    let (_, archived_revision, _) = character_row(&env, &env.character_id).await;
    core.transition_character(
        &principal,
        transition_request(
            &env.character_id,
            archived_revision,
            CoreCharacterTransitionRequestTargetStatus::Active,
        ),
    )
    .await
    .expect("restore");
    assert_eq!(
        nexus_core::require_actor_holder(&pool, CREATOR, &character_actor)
            .await
            .expect("holder after restore"),
        character_holder
    );
    assert_eq!(
        holder_count(&env, "character_id", &env.character_id).await,
        1,
        "one Character materialization keeps exactly one holder"
    );
    // The actor admission path resolves the same holder and still composes.
    let admission = CoreActorAdmission::new(pool.clone());
    admission
        .admit(
            CREATOR,
            character_actor,
            ActorViewpoint {
                world_id: WORLD.to_string(),
                binding_id: Some(env.binding_id.clone()),
                branch_id: None,
                event_id: None,
            },
        )
        .await
        .expect("admission after the lifecycle round-trip");
    pool.close().await;
}

#[tokio::test]
async fn v1191_holder_lifecycle_missing_registry_read_fails_closed() {
    let env = seed_env().await;
    let pool = plain_pool(&env).await;
    let character_actor = AdmittedActor::Character {
        character_id: env.character_id.clone(),
    };

    drop_holder_row(&env, "character_id", &env.character_id).await;
    assert_eq!(
        holder_count(&env, "character_id", &env.character_id).await,
        0
    );

    // The normal read refuses with the stable code instead of provisioning.
    let err = nexus_core::require_actor_holder(&pool, CREATOR, &character_actor)
        .await
        .expect_err("missing registry must fail the read");
    assert_conflict(err, "holder_state_invalid");
    assert_eq!(
        holder_count(&env, "character_id", &env.character_id).await,
        0,
        "a read never mints the missing row"
    );

    // Both admission entry points fail closed on the same state.
    let err = CoreActorAdmission::new(pool.clone())
        .admit(
            CREATOR,
            character_actor,
            ActorViewpoint {
                world_id: WORLD.to_string(),
                binding_id: Some(env.binding_id.clone()),
                branch_id: None,
                event_id: None,
            },
        )
        .await
        .expect_err("composed admission must fail closed");
    assert_conflict(err, "holder_state_invalid");

    // A missing Creator holder is refused as well.
    drop_holder_row(&env, "creator_id", CREATOR).await;
    let err = nexus_core::require_actor_holder(
        &pool,
        CREATOR,
        &AdmittedActor::Creator {
            creator_id: CREATOR.to_string(),
        },
    )
    .await
    .expect_err("Creator read must fail closed");
    assert_conflict(err, "holder_state_invalid");

    // A foreign Actor never widens existence through the holder read.
    drop_holder_row(&env, "character_id", &env.foreign_character_id).await;
    let err = nexus_core::require_actor_holder(
        &pool,
        CREATOR,
        &AdmittedActor::Character {
            character_id: env.foreign_character_id.clone(),
        },
    )
    .await
    .expect_err("foreign Character is not found");
    assert_not_found(&err);
    pool.close().await;
}

// ── v1.191 P1 T5 — trusted knowledge admission and typed knowledge fences ──
//
// Durable contract: `.mstar/specs/holder-governance.md` §4.1 (the two
// server-chosen policies, minted only after principal / stored-owner / admitted
// Actor checks) and §4.3 (World-then-Character shared effect leases, exclusive
// governance lease, knowledge revisions read under the leases).
//
// Applying the resolved selection as pre-limit SQL predicates is P1 T6; these
// cases prove the selection the store will consume, the fence mechanics, and
// the fingerprint that binds a context to its session.

/// Durable §4.2 reference rule expressed over one resolved selection. T6
/// applies the same rule as SQL predicates before cursor / LIMIT / count /
/// ranking / snippets.
fn selection_admits(scope: &KnowledgeReadScope, row: &KnowledgeEntryRecord) -> bool {
    if !scope.containers().contains(&row.owner) {
        return false;
    }
    match row.disclosure.as_deref() {
        None => true,
        Some(DISCLOSURE_OWNER_PRIVATE) => {
            let holder = row.holder_entry_id.as_deref();
            match scope.policy() {
                KnowledgeReadPolicy::ActorView => holder == scope.holder_entry_id(),
                KnowledgeReadPolicy::CreatorManagement => holder.is_some_and(|holder| {
                    scope.authorized_holders().iter().any(|known| known == holder)
                }),
            }
        }
        // Unknown vocabulary cannot exist as a native row (the storage CHECK in
        // `seed_governance_fixture` proves it) and stays quarantined (§6).
        Some(_) => false,
    }
}

fn governed_entry(
    owner: KnowledgeOwnerRef,
    name: &str,
    holder: Option<&str>,
    disclosure: Option<&str>,
) -> KnowledgeEntryRecord {
    let mut row = match &owner {
        KnowledgeOwnerRef::World(world_id) => {
            KnowledgeEntryRecord::new(world_id, BlockType::Item, name)
        }
        KnowledgeOwnerRef::Character(character_id) => {
            KnowledgeEntryRecord::for_character(character_id, BlockType::Item, name)
        }
        KnowledgeOwnerRef::ActorWorldBinding(binding_id) => {
            KnowledgeEntryRecord::for_binding(binding_id, BlockType::Item, name)
        }
    };
    row.holder_entry_id = holder.map(str::to_string);
    row.disclosure = disclosure.map(str::to_string);
    row
}

async fn load_governed_row(
    pool: &SqlitePool,
    store: &SqliteKbStore,
    name: &str,
) -> KnowledgeEntryRecord {
    let id: String =
        sqlx::query_scalar("SELECT key_block_id FROM kb_key_blocks WHERE canonical_name = ?")
            .bind(name)
            .fetch_one(pool)
            .await
            .unwrap();
    store.get_knowledge_entry(&id).await.unwrap()
}

async fn set_knowledge_revision(pool: &SqlitePool, subject: &str, id: &str, value: i64) {
    let sql = match subject {
        "world" => "UPDATE narrative_worlds SET knowledge_revision = ? WHERE world_id = ?",
        "character" => "UPDATE characters SET knowledge_revision = ? WHERE character_id = ?",
        other => panic!("unknown subject {other}"),
    };
    sqlx::query(sql).bind(value).bind(id).execute(pool).await.unwrap();
}

struct GovernanceFixture {
    second_character_id: String,
    second_binding_id: String,
    creator_holder: String,
    first_holder: String,
    second_holder: String,
}

/// Seed a second owned Character with its binding in `WORLD` plus one row per
/// governance class, through one engine pool released before the cores open.
async fn seed_governance_fixture(env: &Env) -> GovernanceFixture {
    let guarded = init_engine_pool(&env.db_path, CREATOR, GuardedPoolOptions::default())
        .await
        .unwrap();
    let pool = guarded.clone_pool();
    let (second_character_id, second_binding_id) =
        seed_character(&pool, CREATOR, WORLD, "Bea").await;
    let store = SqliteKbStore::new(pool.clone());
    let creator_holder = nexus_local_db::creator_holder_entry_id(CREATOR);
    let first_holder = nexus_local_db::character_holder_entry_id(&env.character_id);
    let second_holder = nexus_local_db::character_holder_entry_id(&second_character_id);
    for row in [
        governed_entry(KnowledgeOwnerRef::world(WORLD), "WorldShared", None, None),
        governed_entry(
            KnowledgeOwnerRef::world(WORLD),
            "WorldAuthorPrivate",
            Some(&creator_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        governed_entry(
            KnowledgeOwnerRef::world(WORLD),
            "WorldFirstPrivate",
            Some(&first_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        governed_entry(
            KnowledgeOwnerRef::world(WORLD),
            "WorldSecondPrivate",
            Some(&second_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        governed_entry(
            KnowledgeOwnerRef::character(&second_character_id),
            "SecondCharacterPrivate",
            Some(&second_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        governed_entry(
            KnowledgeOwnerRef::world(FOREIGN_WORLD),
            "ForeignPrivate",
            Some(&first_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        // v1.191 P1 T6 additions: the first Character's own container carries a
        // shared row, its own private row, and a storable private row held by
        // somebody else (the shape a hidden read-by-id must not reveal); its
        // binding carries a binding-local private row.
        governed_entry(
            KnowledgeOwnerRef::character(&env.character_id),
            "FirstCharacterShared",
            None,
            None,
        ),
        governed_entry(
            KnowledgeOwnerRef::character(&env.character_id),
            "FirstCharacterPrivate",
            Some(&first_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        governed_entry(
            KnowledgeOwnerRef::character(&env.character_id),
            "FirstCharacterForeignPrivate",
            Some(&second_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        governed_entry(
            KnowledgeOwnerRef::actor_world_binding(&env.binding_id),
            "FirstBindingPrivate",
            Some(&first_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
    ] {
        store.insert_knowledge_entry(row).await.unwrap();
    }
    // Unknown disclosure vocabulary cannot exist as a native row at all: the
    // cutover keeps it out of `kb_key_blocks` (durable §6 quarantine).
    let unknown = sqlx::query(
        "INSERT INTO kb_key_blocks \
         (key_block_id, owner_kind, world_id, block_type, canonical_name, status, body_json, \
          created_at, holder_entry_id, disclosure) \
         VALUES ('kb_unknown_disclosure', 'world', ?, 'item', 'WorldUnknown', 'confirmed', '{}', \
                 datetime('now'), ?, 'owner-profile')",
    )
    .bind(WORLD)
    .bind(&first_holder)
    .execute(&pool)
    .await;
    assert!(
        unknown.is_err(),
        "unknown disclosure vocabulary must not be storable as a native row"
    );
    pool.close().await;
    GovernanceFixture {
        second_character_id,
        second_binding_id,
        creator_holder,
        first_holder,
        second_holder,
    }
}

fn character_viewpoint(world_id: &str, binding_id: &str) -> ActorViewpoint {
    ActorViewpoint {
        world_id: world_id.to_string(),
        binding_id: Some(binding_id.to_string()),
        branch_id: None,
        event_id: None,
    }
}

fn container_ids(scope: &KnowledgeReadScope) -> Vec<(String, String)> {
    let mut ids: Vec<(String, String)> = scope
        .containers()
        .iter()
        .map(|owner| (owner.kind().to_string(), owner.id().to_string()))
        .collect();
    ids.sort();
    ids
}

#[allow(clippy::too_many_lines)] // one fixture asserted across both policies
#[tokio::test]
async fn v1191_knowledge_admission_management_visible_actor_hidden() {
    let env = seed_env().await;
    let fixture = seed_governance_fixture(&env).await;
    let (core, principal) = open_core(&env).await;

    // CreatorManagement: the owned World plus every owned Character/binding
    // container and the known-governance holder set — never an absent viewpoint.
    let management = core
        .admit_creator_management_knowledge(&principal, WORLD.to_string())
        .await
        .expect("Creator management review admits");
    let management_scope = management.scope();
    assert_eq!(
        management_scope.policy(),
        KnowledgeReadPolicy::CreatorManagement
    );
    assert_eq!(
        management_scope.holder_entry_id(),
        None,
        "management selection is container-scoped, not holder-narrowed"
    );
    let mut expected_containers = vec![
        ("actor_world_binding".to_string(), env.binding_id.clone()),
        (
            "actor_world_binding".to_string(),
            fixture.second_binding_id.clone(),
        ),
        ("character".to_string(), env.character_id.clone()),
        ("character".to_string(), fixture.second_character_id.clone()),
        ("world".to_string(), WORLD.to_string()),
    ];
    expected_containers.sort();
    assert_eq!(container_ids(&management_scope), expected_containers);
    let mut expected_holders = vec![
        fixture.creator_holder.clone(),
        fixture.first_holder.clone(),
        fixture.second_holder.clone(),
    ];
    expected_holders.sort();
    let mut stored_holders = management_scope.authorized_holders().to_vec();
    stored_holders.sort();
    assert_eq!(
        stored_holders, expected_holders,
        "management known-governance holders are the owner Creator plus every owned Character"
    );

    // ActorView: the exact admitted holder plus its authorized containers only.
    let first_actor = AdmittedActor::Character {
        character_id: env.character_id.clone(),
    };
    let first = core
        .admit_actor_knowledge_view(
            &principal,
            &first_actor,
            character_viewpoint(WORLD, &env.binding_id),
        )
        .await
        .expect("Character ActorView admits");
    let first_scope = first.scope();
    assert_eq!(first_scope.policy(), KnowledgeReadPolicy::ActorView);
    assert_eq!(
        first_scope.holder_entry_id(),
        Some(fixture.first_holder.as_str())
    );
    let mut expected_first_containers = vec![
        ("actor_world_binding".to_string(), env.binding_id.clone()),
        ("character".to_string(), env.character_id.clone()),
        ("world".to_string(), WORLD.to_string()),
    ];
    expected_first_containers.sort();
    assert_eq!(container_ids(&first_scope), expected_first_containers);
    assert!(
        first_scope.authorized_holders().is_empty(),
        "an ActorView never inherits the management known-governance set"
    );

    // Row-level outcomes under the §4.2 rule: the same stored rows are
    // management-visible and Actor-hidden (and vice versa).
    let pool = plain_pool(&env).await;
    let store = SqliteKbStore::new(pool.clone());
    for (name, expected) in [
        ("WorldShared", (true, true)),
        ("WorldAuthorPrivate", (true, false)),
        ("WorldFirstPrivate", (true, true)),
        ("WorldSecondPrivate", (true, false)),
        ("SecondCharacterPrivate", (true, false)),
        ("ForeignPrivate", (false, false)),
    ] {
        let row = load_governed_row(&pool, &store, name).await;
        assert_eq!(
            (
                selection_admits(&management_scope, &row),
                selection_admits(&first_scope, &row)
            ),
            expected,
            "{name}: management-visible vs Actor-hidden"
        );
    }

    // A second ActorView sees exactly its own private rows, never the first
    // Character's: the exact holder is the only private row it admits.
    let second = core
        .admit_actor_knowledge_view(
            &principal,
            &AdmittedActor::Character {
                character_id: fixture.second_character_id.clone(),
            },
            character_viewpoint(WORLD, &fixture.second_binding_id),
        )
        .await
        .expect("second Character ActorView admits");
    let second_scope = second.scope();
    assert_eq!(
        second_scope.holder_entry_id(),
        Some(fixture.second_holder.as_str())
    );
    assert!(
        selection_admits(
            &second_scope,
            &load_governed_row(&pool, &store, "SecondCharacterPrivate").await
        ),
        "the owning Character sees its own private row"
    );
    assert!(
        !selection_admits(
            &second_scope,
            &load_governed_row(&pool, &store, "WorldFirstPrivate").await
        ),
        "a Character view never inherits another holder's private row"
    );

    // Both fingerprints were read under the leases and differ by policy.
    assert_eq!(
        management.revisions(),
        nexus_core::KnowledgeRevisions {
            world: 0,
            character: None
        }
    );
    assert_eq!(
        first.revisions(),
        nexus_core::KnowledgeRevisions {
            world: 0,
            character: Some(0)
        }
    );
    assert_ne!(management.identity(), first.identity());
    assert_eq!(management.policy(), KnowledgeReadPolicy::CreatorManagement);
    assert_eq!(first.policy(), KnowledgeReadPolicy::ActorView);
    pool.close().await;
}

#[tokio::test]
async fn v1191_knowledge_admission_spoofed_viewpoint_and_actor_are_refused() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let first_actor = AdmittedActor::Character {
        character_id: env.character_id.clone(),
    };

    // The holder is the stored derivation, never anything the caller supplies.
    let admitted = core
        .admit_actor_knowledge_view(
            &principal,
            &first_actor,
            character_viewpoint(WORLD, &env.binding_id),
        )
        .await
        .expect("owned Character admits");
    assert_eq!(
        admitted.scope().holder_entry_id(),
        Some(nexus_local_db::character_holder_entry_id(&env.character_id).as_str())
    );

    // A foreign holder string supplied through the actor never widens the
    // selection: the foreign Actor is simply not found.
    let err = core
        .admit_actor_knowledge_view(
            &principal,
            &AdmittedActor::Character {
                character_id: env.foreign_character_id.clone(),
            },
            character_viewpoint(FOREIGN_WORLD, &env.foreign_binding_id),
        )
        .await
        .expect_err("foreign Actor");
    assert_not_found(&err);

    // A viewpoint naming another Character's binding is refused.
    let err = core
        .admit_actor_knowledge_view(
            &principal,
            &first_actor,
            character_viewpoint(WORLD, &env.foreign_binding_id),
        )
        .await
        .expect_err("cross-Character binding");
    assert_not_found(&err);

    // A foreign World viewpoint is refused.
    let err = core
        .admit_actor_knowledge_view(
            &principal,
            &first_actor,
            character_viewpoint(FOREIGN_WORLD, &env.binding_id),
        )
        .await
        .expect_err("foreign World");
    assert_not_found(&err);

    // A Creator actor carrying a binding is a pair-shape violation.
    let err = core
        .admit_actor_knowledge_view(
            &principal,
            &AdmittedActor::Creator {
                creator_id: CREATOR.to_string(),
            },
            character_viewpoint(WORLD, &env.binding_id),
        )
        .await
        .expect_err("Creator with binding");
    assert_invalid_input(&err);

    // A missing registry row fails closed instead of provisioning a holder.
    drop_holder_row(&env, "character_id", &env.character_id).await;
    let err = core
        .admit_actor_knowledge_view(
            &principal,
            &first_actor,
            character_viewpoint(WORLD, &env.binding_id),
        )
        .await
        .expect_err("missing holder registry row");
    assert_conflict(err, "holder_state_invalid");
}

#[tokio::test]
async fn v1191_knowledge_admission_identity_binds_policy_and_stored_revisions() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let pool = plain_pool(&env).await;
    let actor = AdmittedActor::Character {
        character_id: env.character_id.clone(),
    };
    let viewpoint = character_viewpoint(WORLD, &env.binding_id);

    let baseline = core
        .admit_actor_knowledge_view(&principal, &actor, viewpoint.clone())
        .await
        .expect("admitted baseline");
    assert_eq!(baseline.identity().policy(), KnowledgeReadPolicy::ActorView);
    assert_eq!(baseline.identity().world_revision(), 0);
    assert_eq!(baseline.identity().character_revision(), Some(0));
    let baseline_identity = baseline.identity();
    drop(baseline);

    // A cosmetic rename moves the projected label and the content revision,
    // never the knowledge revisions: the reuse fingerprint stays usable.
    let (_, revision, _) = character_row(&env, &env.character_id).await;
    core.patch_character(
        &principal,
        env.character_id.clone(),
        revision,
        nexus_local_db::CharacterPatch {
            display_name: Some("Ada Cosmetically Renamed"),
            image_uri: nexus_local_db::FieldPatch::Keep,
            persona_json: nexus_local_db::FieldPatch::Keep,
        },
    )
    .await
    .expect("cosmetic rename");
    let after_rename = core
        .admit_actor_knowledge_view(&principal, &actor, viewpoint.clone())
        .await
        .expect("admitted after rename");
    assert_eq!(
        after_rename.identity(),
        baseline_identity,
        "a cosmetic rename leaves the reuse fingerprint untouched"
    );

    // A stored Character governance bump is observed by the next admission
    // (this is what retires a reused session).
    set_knowledge_revision(&pool, "character", &env.character_id, 1).await;
    let after_character = core
        .admit_actor_knowledge_view(&principal, &actor, viewpoint.clone())
        .await
        .expect("admitted after the Character governance bump");
    assert_eq!(after_character.identity().character_revision(), Some(1));
    assert_ne!(after_character.identity(), after_rename.identity());

    // The World revision participates too.
    set_knowledge_revision(&pool, "world", WORLD, 2).await;
    let after_world = core
        .admit_actor_knowledge_view(&principal, &actor, viewpoint.clone())
        .await
        .expect("admitted after the World governance bump");
    assert_eq!(after_world.identity().world_revision(), 2);
    assert_ne!(after_world.identity(), after_character.identity());

    // Same stored revisions, different policy kind: the policy is part of the
    // fingerprint, so management and ActorView can never share a session key.
    let management = core
        .admit_creator_management_knowledge(&principal, WORLD.to_string())
        .await
        .expect("management admits");
    assert_eq!(
        management.identity().policy(),
        KnowledgeReadPolicy::CreatorManagement
    );
    assert_eq!(management.identity().character_revision(), None);
    assert_eq!(management.identity().world_revision(), 2);
    assert_ne!(management.identity(), after_world.identity());
    pool.close().await;
}

#[allow(clippy::too_many_lines)] // one fence matrix asserted in order
#[tokio::test]
async fn v1191_knowledge_admission_fence_blocks_and_releases_partial_plan() {
    let env = seed_env().await;
    let fixture = seed_governance_fixture(&env).await;
    let (core, principal) = open_core(&env).await;
    let first_actor = AdmittedActor::Character {
        character_id: env.character_id.clone(),
    };

    // Canonical order inside a plan is World ids then Character ids. Holding
    // both the World and the lexically-last Character exclusively must refuse
    // on the World, proving the World is attempted first.
    let mut characters = vec![
        env.character_id.clone(),
        fixture.second_character_id.clone(),
    ];
    characters.sort();
    let first_character = characters[0].clone();
    let last_character = characters[1].clone();
    let world_lease = core
        .acquire_knowledge_governance(&principal, ActorFenceKind::World, WORLD.to_string())
        .await
        .expect("World governance lease");
    assert_eq!(world_lease.kind(), ActorFenceKind::World);
    assert_eq!(world_lease.subject_id(), WORLD);
    let last_lease = core
        .acquire_knowledge_governance(
            &principal,
            ActorFenceKind::Character,
            last_character.clone(),
        )
        .await
        .expect("Character governance lease");
    assert_eq!(last_lease.kind(), ActorFenceKind::Character);
    assert_eq!(last_lease.subject_id(), last_character);
    let refused = core
        .admit_creator_management_knowledge(&principal, WORLD.to_string())
        .await
        .expect_err("World-first ordering refuses on the World");
    assert_conflict(refused, "world_busy");
    drop(world_lease);

    // Partial acquisition release: with only the last Character contended the
    // plan takes World + first Character, fails on the last, and must release
    // everything it already took.
    let refused = core
        .admit_creator_management_knowledge(&principal, WORLD.to_string())
        .await
        .expect_err("the contended Character refuses the plan");
    assert_conflict(refused, "character_busy");
    drop(last_lease);
    let world_after = core
        .acquire_knowledge_governance(&principal, ActorFenceKind::World, WORLD.to_string())
        .await
        .expect("the failed plan released the World shared lease");
    let first_after = core
        .acquire_knowledge_governance(
            &principal,
            ActorFenceKind::Character,
            first_character.clone(),
        )
        .await
        .expect("the failed plan released the intermediate Character shared lease");
    drop(world_after);
    drop(first_after);

    // An in-flight activity effect (what a running stream holds) blocks the
    // disclosure edit on its Character.
    let activity = core
        .acquire_actor_activity(&principal, &first_actor)
        .await
        .expect("activity admission");
    let refused = core
        .acquire_knowledge_governance(
            &principal,
            ActorFenceKind::Character,
            env.character_id.clone(),
        )
        .await
        .expect_err("an in-flight activity blocks the Character governance edit");
    assert_conflict(refused, "character_busy");
    drop(activity);

    // An admitted ActorView context holds World then Character shared leases
    // through its effect, blocking both governance edits and the retained
    // lifecycle transition; draining it releases every lease.
    let context = core
        .admit_actor_knowledge_view(
            &principal,
            &first_actor,
            character_viewpoint(WORLD, &env.binding_id),
        )
        .await
        .expect("ActorView context admits");
    let refused = core
        .acquire_knowledge_governance(&principal, ActorFenceKind::World, WORLD.to_string())
        .await
        .expect_err("World governance blocks while the ActorView effect runs");
    assert_conflict(refused, "world_busy");
    let refused = core
        .acquire_knowledge_governance(
            &principal,
            ActorFenceKind::Character,
            env.character_id.clone(),
        )
        .await
        .expect_err("Character governance blocks while the ActorView effect runs");
    assert_conflict(refused, "character_busy");
    let refused = core
        .acquire_character_transition(&principal, env.character_id.clone())
        .await
        .expect_err("the lifecycle transition contends with the held activity lease");
    assert_conflict(refused, "character_busy");
    drop(context);
    let drained = core
        .acquire_knowledge_governance(&principal, ActorFenceKind::World, WORLD.to_string())
        .await
        .expect("draining the effect releases the World lease");
    drop(drained);
    let drained = core
        .acquire_knowledge_governance(
            &principal,
            ActorFenceKind::Character,
            env.character_id.clone(),
        )
        .await
        .expect("draining the effect releases the Character lease");
    drop(drained);

    // Reverse direction: a held governance edit refuses the next admission.
    let world_lease = core
        .acquire_knowledge_governance(&principal, ActorFenceKind::World, WORLD.to_string())
        .await
        .expect("World governance lease");
    let refused = core
        .admit_actor_knowledge_view(
            &principal,
            &first_actor,
            character_viewpoint(WORLD, &env.binding_id),
        )
        .await
        .expect_err("an ActorView admission blocks while the World is governed");
    assert_conflict(refused, "world_busy");
    drop(world_lease);
    let character_lease = core
        .acquire_knowledge_governance(
            &principal,
            ActorFenceKind::Character,
            env.character_id.clone(),
        )
        .await
        .expect("Character governance lease");
    let refused = core
        .admit_actor_knowledge_view(
            &principal,
            &first_actor,
            character_viewpoint(WORLD, &env.binding_id),
        )
        .await
        .expect_err("an ActorView admission blocks while the Character is governed");
    assert_conflict(refused, "character_busy");
    drop(character_lease);
    core.admit_actor_knowledge_view(
        &principal,
        &first_actor,
        character_viewpoint(WORLD, &env.binding_id),
    )
    .await
    .expect("the admission works again once the governance edit drains");
}

// ── v1.191 P1 T6 — pre-limit native read visibility ─────────────────────────
//
// Durable contract: `.mstar/specs/holder-governance.md` §4.2 (the visibility
// rule is an eligibility predicate applied before the keyset cursor, LIMIT,
// count and snippet observation; a hidden read-by-id is indistinguishable from
// a missing one) and §4.1 (CreatorManagement reviews owned known-private rows;
// an ActorView — including a Creator's — never inherits that review).
//
// T5 proved the selection each policy resolves; these cases prove the core
// read paths (view / list / detail, and the admitted-context page) consume it
// at the SQL layer, page by page.

/// `canonical_name` → `key_block_id` for the fixture rows.
async fn entry_id_by_name(pool: &SqlitePool, name: &str) -> String {
    sqlx::query_scalar("SELECT key_block_id FROM kb_key_blocks WHERE canonical_name = ?")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn sorted_names(rows: &[KnowledgeEntryRecord]) -> Vec<String> {
    let mut names: Vec<String> = rows.iter().map(|row| row.canonical_name.clone()).collect();
    names.sort();
    names
}

/// Walk one `ActorView` one row per page: every eligible row appears exactly
/// once, no hidden row is ever paged into, and the walk ends only when the page
/// says so. Filtering *after* the keyset `LIMIT` returns a short or empty page
/// here (`has_more` false while rows remain), which the walk cannot complete.
#[allow(clippy::significant_drop_tightening)] // the second Character's context is held across its page call
#[tokio::test]
async fn v1191_holder_visibility_actor_view_pages_skip_hidden_rows() {
    let env = seed_env().await;
    let fixture = seed_governance_fixture(&env).await;
    let (core, principal) = open_core(&env).await;
    let actor = AdmittedActor::Character {
        character_id: env.character_id.clone(),
    };

    let mut cursor: Option<String> = None;
    let mut names: Vec<String> = Vec::new();
    let mut pages = 0;
    loop {
        let page = core
            .actor_knowledge_view(
                &principal,
                &actor,
                ActorKnowledgeViewQuery {
                    world_id: WORLD.to_string(),
                    binding_id: Some(env.binding_id.clone()),
                    limit: 1,
                    cursor,
                },
            )
            .await
            .expect("one-row page");
        assert!(
            page.items.len() <= 1,
            "a one-row page carries at most one row"
        );
        names.extend(page.items.iter().map(|row| row.canonical_name.clone()));
        pages += 1;
        if !page.has_more {
            assert!(page.next_cursor.is_none(), "a final page exposes no cursor");
            break;
        }
        cursor = page.next_cursor;
        assert!(cursor.is_some(), "has_more implies a next cursor");
        assert!(pages < 32, "the walk must terminate: {names:?}");
    }

    let expected = vec![
        "FirstBindingPrivate",
        "FirstCharacterPrivate",
        "FirstCharacterShared",
        "WorldFirstPrivate",
        "WorldShared",
    ];
    assert_eq!(pages, expected.len(), "one page per eligible row");
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(
        sorted, expected,
        "the paginated ActorView holds exactly the eligible rows"
    );
    for hidden in [
        "WorldAuthorPrivate",
        "WorldSecondPrivate",
        "SecondCharacterPrivate",
        "FirstCharacterForeignPrivate",
        "ForeignPrivate",
    ] {
        assert!(
            !names.contains(&hidden.to_string()),
            "{hidden} must not be paged into a Character ActorView"
        );
    }
    // Two independently admitted Characters and the reason the matrix holds:
    // the same stored rows, a different resolved holder.
    let second = core
        .admit_actor_knowledge_view(
            &principal,
            &AdmittedActor::Character {
                character_id: fixture.second_character_id.clone(),
            },
            character_viewpoint(WORLD, &fixture.second_binding_id),
        )
        .await
        .expect("second Character admits");
    let second_page = core
        .admitted_knowledge_page(&principal, &second, 100, None)
        .await
        .expect("second Character page");
    let second_names = sorted_names(&second_page.items);
    assert!(second_names.contains(&"SecondCharacterPrivate".to_string()));
    assert!(second_names.contains(&"WorldSecondPrivate".to_string()));
    assert!(second_names.contains(&"WorldShared".to_string()));
    assert!(
        !second_names.contains(&"FirstCharacterPrivate".to_string())
            && !second_names.contains(&"FirstCharacterShared".to_string()),
        "another Character's container never joins this view: {second_names:?}"
    );
}

/// The observable shape of one detail refusal, with the caller-supplied id
/// normalized away: two refusals equal under this measure are indistinguishable
/// to the caller, which is what "a hidden id behaves like an absent one" means.
fn refusal_shape(err: &CoreError, requested: &str) -> String {
    match err {
        CoreError::NotFound { resource } => resource.replace(requested, "<requested-id>"),
        other => format!("{other:?}"),
    }
}

/// The detail read is one eligibility `WHERE` (container + registry holder +
/// disclosure): a row hidden by holder, a row in another container, and an
/// absent id are observably indistinguishable, and no refusal names the hidden
/// row or a holder.
#[tokio::test]
async fn v1191_holder_visibility_detail_hidden_equals_absent() {
    let env = seed_env().await;
    let fixture = seed_governance_fixture(&env).await;
    let (core, principal) = open_core(&env).await;
    let pool = plain_pool(&env).await;
    let character_id = env.character_id.clone();

    let own = core
        .actor_knowledge_entry(
            &principal,
            character_id.clone(),
            entry_id_by_name(&pool, "FirstCharacterPrivate").await,
        )
        .await
        .expect("the admitted Character reads its own private row");
    assert_eq!(own.canonical_name, "FirstCharacterPrivate");

    let shared = core
        .actor_knowledge_entry(
            &principal,
            character_id.clone(),
            entry_id_by_name(&pool, "FirstCharacterShared").await,
        )
        .await
        .expect("a shared row stays readable");
    assert_eq!(shared.canonical_name, "FirstCharacterShared");

    // The observable refusal shape with the caller-supplied id normalized away:
    // two refusals must be identical by this measure, so a hidden id cannot be
    // distinguished from an absent one by the caller.
    let absent_id = "kb_never_written";
    let absent = core
        .actor_knowledge_entry(&principal, character_id.clone(), absent_id.to_string())
        .await
        .expect_err("an absent id is absent");
    let absent_shape = refusal_shape(&absent, absent_id);
    assert_not_found(&absent);

    // Hidden by holder, then outside the container (World container, the other
    // Character's container, the other binding's provenance).
    for name in [
        "FirstCharacterForeignPrivate",
        "WorldFirstPrivate",
        "SecondCharacterPrivate",
    ] {
        let entry_id = entry_id_by_name(&pool, name).await;
        let denied = core
            .actor_knowledge_entry(&principal, character_id.clone(), entry_id.clone())
            .await
            .expect_err("a row outside the admitted selection is unobservable");
        assert_not_found(&denied);
        assert_eq!(
            refusal_shape(&denied, &entry_id),
            absent_shape,
            "{name} must be indistinguishable from an absent id"
        );
        let text = format!("{denied:?}");
        assert!(
            !text.contains(name),
            "the refusal must not name the hidden row: {text}"
        );
        assert!(
            !text.contains(&fixture.second_holder),
            "the refusal must not name a holder: {text}"
        );
    }
    pool.close().await;
}

/// Both policies read through the same page composition, and the *entry point*
/// picks the policy (durable §5.1): the Creator knowledge-view surface is the
/// **management review** — every owned known-private row, foreign Worlds
/// excluded — while a Character `ActorView` stays strictly holder-filtered and
/// is a proper subset of it.
///
/// The two directions are asserted independently: the management side may not
/// narrow (that would hide owned private facts from their author), and the
/// Character side may not widen (that would leak another identity's private
/// rows).
#[allow(clippy::significant_drop_tightening)] // each context is held across its page call
#[tokio::test]
async fn v1191_holder_visibility_management_and_actor_view_stay_separate() {
    let env = seed_env().await;
    let fixture = seed_governance_fixture(&env).await;
    let (core, principal) = open_core(&env).await;

    let management = core
        .admit_creator_management_knowledge(&principal, WORLD.to_string())
        .await
        .expect("Creator management review admits");
    let management_page = core
        .admitted_knowledge_page(&principal, &management, 100, None)
        .await
        .expect("management page");
    assert_eq!(
        sorted_names(&management_page.items),
        vec![
            "FirstBindingPrivate",
            "FirstCharacterForeignPrivate",
            "FirstCharacterPrivate",
            "FirstCharacterShared",
            "SecondCharacterPrivate",
            "WorldAuthorPrivate",
            "WorldFirstPrivate",
            "WorldSecondPrivate",
            "WorldShared",
        ],
        "management review reaches every owned known-private row"
    );

    let first = core
        .admit_actor_knowledge_view(
            &principal,
            &AdmittedActor::Character {
                character_id: env.character_id.clone(),
            },
            character_viewpoint(WORLD, &env.binding_id),
        )
        .await
        .expect("first Character admits");
    let actor_page = core
        .admitted_knowledge_page(&principal, &first, 100, None)
        .await
        .expect("ActorView page");
    let actor_names = sorted_names(&actor_page.items);
    assert_eq!(
        actor_names,
        vec![
            "FirstBindingPrivate",
            "FirstCharacterPrivate",
            "FirstCharacterShared",
            "WorldFirstPrivate",
            "WorldShared",
        ],
        "the same read path under ActorView holds the holder-filtered rows"
    );
    assert!(
        !actor_names.contains(&"WorldAuthorPrivate".to_string())
            && !actor_names.contains(&"FirstCharacterForeignPrivate".to_string()),
        "a management snapshot is never returned as an ActorView"
    );

    // Durable §5.1: the Creator knowledge-view surface **is** the management
    // review (the authorized Creator reviews owned private facts). It therefore
    // equals the management set above — never the old holder-filtered Creator
    // ActorView — while still excluding the foreign World's row.
    let creator_page = core
        .actor_knowledge_view(
            &principal,
            &AdmittedActor::Creator {
                creator_id: CREATOR.to_string(),
            },
            ActorKnowledgeViewQuery {
                world_id: WORLD.to_string(),
                binding_id: None,
                limit: 100,
                cursor: None,
            },
        )
        .await
        .expect("Creator knowledge view admits");
    assert_eq!(
        sorted_names(&creator_page.items),
        sorted_names(&management_page.items),
        "the Creator knowledge-view surface is the management review (durable §5.1)"
    );
    assert_eq!(
        sorted_names(&creator_page.items),
        vec![
            "FirstBindingPrivate",
            "FirstCharacterForeignPrivate",
            "FirstCharacterPrivate",
            "FirstCharacterShared",
            "SecondCharacterPrivate",
            "WorldAuthorPrivate",
            "WorldFirstPrivate",
            "WorldSecondPrivate",
            "WorldShared",
        ],
        "management review reaches every owned known-private row"
    );
    let creator_names = sorted_names(&creator_page.items);
    assert!(
        !creator_names.contains(&"ForeignPrivate".to_string()),
        "the foreign World's row never enters the management review"
    );

    // The other direction: a Character `ActorView` stays strictly
    // holder-filtered and never inherits that review. The second Character's
    // view is the smallest case — its own holder plus the shared World row —
    // and excludes both the binding-local private row and every row held by
    // another identity.
    let second = core
        .admit_actor_knowledge_view(
            &principal,
            &AdmittedActor::Character {
                character_id: fixture.second_character_id.clone(),
            },
            character_viewpoint(WORLD, &fixture.second_binding_id),
        )
        .await
        .expect("second Character admits");
    let second_page = core
        .admitted_knowledge_page(&principal, &second, 100, None)
        .await
        .expect("second Character ActorView page");
    let second_names = sorted_names(&second_page.items);
    assert_eq!(
        second_names,
        vec!["SecondCharacterPrivate", "WorldSecondPrivate", "WorldShared"],
        "a Character ActorView holds exactly its own holder's rows"
    );
    assert!(
        !second_names.contains(&"FirstBindingPrivate".to_string())
            && !second_names.contains(&"FirstCharacterForeignPrivate".to_string())
            && !second_names.contains(&"WorldAuthorPrivate".to_string()),
        "a Character ActorView never inherits the management review"
    );

    // The Character view is a **proper subset** of the management review: no
    // rows are gained, so the two policies cannot be confused for one another.
    for name in &second_names {
        assert!(
            creator_names.contains(name),
            "{name} must also be visible to the Creator management review"
        );
    }
    assert!(
        second_names.len() < creator_names.len(),
        "an ActorView must not equal the management review"
    );

    // The Character detail listing (no World filter) applies the same
    // selection: its own container, its own holder.
    let listed = core
        .list_character_knowledge(&principal, env.character_id.clone(), 100, None)
        .await
        .expect("Character-owned listing");
    assert_eq!(
        sorted_names(&listed.items),
        vec!["FirstCharacterPrivate", "FirstCharacterShared"],
        "the Character-owned listing hides the foreign-holder private row"
    );
}

// ── v1.191 P1 T7: admitted audience authoring (durable §3/§4.3) ──────────

/// Owner-scoped native governance pair of one stored KE.
async fn stored_governance(
    pool: &SqlitePool,
    entry_id: &str,
) -> (Option<String>, Option<String>, i64) {
    sqlx::query_as(
        "SELECT holder_entry_id, disclosure, COALESCE(revision, 0) \
         FROM kb_key_blocks WHERE key_block_id = ?",
    )
    .bind(entry_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn stored_character_revision(pool: &SqlitePool, character_id: &str) -> i64 {
    sqlx::query_scalar("SELECT knowledge_revision FROM characters WHERE character_id = ?")
        .bind(character_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn stored_world_revision(pool: &SqlitePool, world_id: &str) -> i64 {
    sqlx::query_scalar("SELECT knowledge_revision FROM narrative_worlds WHERE world_id = ?")
        .bind(world_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn knowledge_row_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM kb_key_blocks")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// One wire-valid actor-knowledge create request (owner + audience).
fn create_request(
    owner_kind: &str,
    world_id: Option<&str>,
    character_id: Option<&str>,
    binding_id: Option<&str>,
    name: &str,
    audience: Option<serde_json::Value>,
) -> AddKnowledgeEntryRequest {
    let mut value = serde_json::json!({
        "owner_kind": owner_kind,
        "block_type": "item",
        "canonical_name": name,
    });
    for (key, field) in [
        ("world_id", world_id),
        ("character_id", character_id),
        ("binding_id", binding_id),
    ] {
        if let Some(field) = field {
            value[key] = serde_json::json!(field);
        }
    }
    if let Some(audience) = audience {
        value["audience"] = audience;
    }
    serde_json::from_value(value).expect("create request is wire-valid")
}

fn author_only() -> serde_json::Value {
    serde_json::json!({ "kind": "author-only" })
}

fn shared_audience() -> serde_json::Value {
    serde_json::json!({ "kind": "shared" })
}

fn character_private(character_id: &str) -> serde_json::Value {
    serde_json::json!({ "kind": "character-private", "character_id": character_id })
}

/// A second owned Character, created in `WORLD_B` so it holds no binding to
/// `WORLD` (the World-row permission probe's negative case).
async fn seed_unbound_character(env: &Env) -> String {
    let pool = plain_pool(env).await;
    let (character_id, _) = seed_character(&pool, CREATOR, WORLD_B, "Unbound").await;
    pool.close().await;
    character_id
}

#[tokio::test]
async fn v1191_audience_cas_create_resolves_permitted_identities() {
    let env = seed_env().await;
    let unbound = seed_unbound_character(&env).await;
    let (core, principal) = open_core(&env).await;
    let pool = plain_pool(&env).await;
    let creator_holder = nexus_local_db::creator_holder_entry_id(CREATOR);
    let character_holder = nexus_local_db::character_holder_entry_id(&env.character_id);
    let foreign_holder = nexus_local_db::character_holder_entry_id(&env.foreign_character_id);

    // World row + author-only → the admitted controlling Creator's holder.
    let author_only_row = core
        .add_actor_knowledge_entry(
            &principal,
            create_request("world", Some(WORLD), None, None, "AuthoredOnly", Some(author_only())),
            false,
        )
        .await
        .expect("author-only create admits");
    assert_eq!(
        stored_governance(&pool, &author_only_row.entry_id).await,
        (
            Some(creator_holder.clone()),
            Some(DISCLOSURE_OWNER_PRIVATE.to_string()),
            0
        )
    );

    // World row + character-private naming a Character actively bound to that
    // World → that Character's holder.
    let world_private = core
        .add_actor_knowledge_entry(
            &principal,
            create_request(
                "world",
                Some(WORLD),
                None,
                None,
                "WorldCharacterPrivate",
                Some(character_private(&env.character_id)),
            ),
            false,
        )
        .await
        .expect("a bound Character is a permitted World-row audience");
    assert_eq!(
        stored_governance(&pool, &world_private.entry_id).await,
        (
            Some(character_holder.clone()),
            Some(DISCLOSURE_OWNER_PRIVATE.to_string()),
            0
        )
    );

    // Omitted audience (and explicit shared) both store no governance.
    let omitted = core
        .add_actor_knowledge_entry(
            &principal,
            create_request("world", Some(WORLD), None, None, "OmittedShared", None),
            false,
        )
        .await
        .expect("omitted create audience admits");
    assert_eq!(stored_governance(&pool, &omitted.entry_id).await, (None, None, 0));
    let explicit = core
        .add_actor_knowledge_entry(
            &principal,
            create_request(
                "world",
                Some(WORLD),
                None,
                None,
                "ExplicitShared",
                Some(shared_audience()),
            ),
            false,
        )
        .await
        .expect("explicit shared admits");
    assert_eq!(stored_governance(&pool, &explicit.entry_id).await, (None, None, 0));

    // Character-owned row + character-private naming its own owning Character.
    let character_row = core
        .add_actor_knowledge_entry(
            &principal,
            create_request(
                "character",
                None,
                Some(&env.character_id),
                None,
                "OwnCharacterPrivate",
                Some(character_private(&env.character_id)),
            ),
            false,
        )
        .await
        .expect("the owning Character is a permitted audience");
    assert_eq!(
        stored_governance(&pool, &character_row.entry_id).await,
        (
            Some(character_holder.clone()),
            Some(DISCLOSURE_OWNER_PRIVATE.to_string()),
            0
        )
    );

    // Binding-owned row + character-private naming its owning Character.
    let binding_row = core
        .add_actor_knowledge_entry(
            &principal,
            create_request(
                "actor_world_binding",
                Some(WORLD),
                Some(&env.character_id),
                Some(&env.binding_id),
                "OwnBindingPrivate",
                Some(character_private(&env.character_id)),
            ),
            false,
        )
        .await
        .expect("the owning Character is a permitted binding-row audience");
    assert_eq!(
        stored_governance(&pool, &binding_row.entry_id).await,
        (
            Some(character_holder.clone()),
            Some(DISCLOSURE_OWNER_PRIVATE.to_string()),
            0
        )
    );

    assert!(!unbound.is_empty(), "the unbound Character exists for the matrix");
    assert_eq!(
        stored_governance(&pool, &omitted.entry_id).await.0,
        None,
        "no create above leaked the foreign holder {foreign_holder}"
    );
    assert_eq!(knowledge_row_count(&pool).await, 6);
    pool.close().await;
}

#[tokio::test]
async fn v1191_audience_cas_create_refuses_unpermitted_identities() {
    let env = seed_env().await;
    let unbound = seed_unbound_character(&env).await;
    let (core, principal) = open_core(&env).await;
    let pool = plain_pool(&env).await;
    let before = knowledge_row_count(&pool).await;

    // Owned + active, but no binding to the target World.
    let err = core
        .add_actor_knowledge_entry(
            &principal,
            create_request(
                "world",
                Some(WORLD),
                None,
                None,
                "UnboundAudience",
                Some(character_private(&unbound)),
            ),
            false,
        )
        .await
        .unwrap_err();
    assert_invalid_input(&err);
    assert_eq!(knowledge_row_count(&pool).await, before, "refusal wrote no row");

    // Foreign Character (another Creator's) stays hidden.
    let err = core
        .add_actor_knowledge_entry(
            &principal,
            create_request(
                "world",
                Some(WORLD),
                None,
                None,
                "ForeignAudience",
                Some(character_private(&env.foreign_character_id)),
            ),
            false,
        )
        .await
        .unwrap_err();
    assert_not_found(&err);
    assert_eq!(knowledge_row_count(&pool).await, before);

    // A Character row may only target its own owning Character.
    let err = core
        .add_actor_knowledge_entry(
            &principal,
            create_request(
                "character",
                None,
                Some(&env.character_id),
                None,
                "ForeignOwnAudience",
                Some(character_private(&unbound)),
            ),
            false,
        )
        .await
        .unwrap_err();
    assert_not_found(&err);

    // A binding row likewise targets only its owning Character.
    let err = core
        .add_actor_knowledge_entry(
            &principal,
            create_request(
                "actor_world_binding",
                Some(WORLD),
                Some(&env.character_id),
                Some(&env.binding_id),
                "ForeignBindingAudience",
                Some(character_private(&unbound)),
            ),
            false,
        )
        .await
        .unwrap_err();
    assert_not_found(&err);
    assert_eq!(knowledge_row_count(&pool).await, before);

    // Owned but archiving the Character removes it from the permitted set.
    let characters = plain_pool(&env).await;
    let (_, revision, _) = character_row(&env, &unbound).await;
    nexus_local_db::transition_character(
        &characters,
        CREATOR,
        &unbound,
        revision,
        nexus_local_db::CharacterStatus::Archived,
    )
    .await
    .unwrap();
    let err = core
        .add_actor_knowledge_entry(
            &principal,
            create_request(
                "world",
                Some(WORLD),
                None,
                None,
                "ArchivedAudience",
                Some(character_private(&unbound)),
            ),
            false,
        )
        .await
        .unwrap_err();
    assert_conflict(err, "character_inactive");
    assert_eq!(knowledge_row_count(&pool).await, before, "no refusal wrote a row");
    characters.close().await;
    pool.close().await;
}

#[tokio::test]
async fn v1191_audience_cas_patch_moves_governance_under_the_ke_revision() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let pool = plain_pool(&env).await;
    let creator_holder = nexus_local_db::creator_holder_entry_id(CREATOR);

    let created = core
        .add_actor_knowledge_entry(
            &principal,
            create_request(
                "character",
                None,
                Some(&env.character_id),
                None,
                "PatchSubject",
                None,
            ),
            false,
        )
        .await
        .expect("shared create admits");
    let entry_id = created.entry_id.clone();
    assert_eq!(stored_governance(&pool, &entry_id).await, (None, None, 0));

    // Governed patch: the pair and the owning Character's knowledge revision
    // move under the same expected_revision CAS.
    let patched = core
        .patch_actor_knowledge_entry(
            &principal,
            env.character_id.clone(),
            entry_id.clone(),
            0,
            None,
            nexus_local_db::FieldPatch::Keep,
            Some(KnowledgeAudience::AuthorOnly),
        )
        .await
        .expect("author-only patch admits");
    assert_eq!(patched.revision, Some(1));
    assert_eq!(
        stored_governance(&pool, &entry_id).await,
        (
            Some(creator_holder.clone()),
            Some(DISCLOSURE_OWNER_PRIVATE.to_string()),
            1
        )
    );
    assert_eq!(stored_character_revision(&pool, &env.character_id).await, 1);
    let admitted = core
        .admit_actor_knowledge_view(
            &principal,
            &AdmittedActor::Character {
                character_id: env.character_id.clone(),
            },
            character_viewpoint(WORLD, &env.binding_id),
        )
        .await
        .expect("admitted after the governance patch");
    assert_eq!(
        admitted.identity().character_revision(),
        Some(1),
        "the next admission observes the bumped Character revision"
    );
    drop(admitted);

    // A content-only patch omits the member: the stored pair is preserved.
    let content_only = core
        .patch_actor_knowledge_entry(
            &principal,
            env.character_id.clone(),
            entry_id.clone(),
            1,
            None,
            nexus_local_db::FieldPatch::Set("edited summary"),
            None,
        )
        .await
        .expect("content-only patch admits");
    assert_eq!(content_only.revision, Some(2));
    assert_eq!(
        stored_governance(&pool, &entry_id).await,
        (
            Some(creator_holder.clone()),
            Some(DISCLOSURE_OWNER_PRIVATE.to_string()),
            2
        ),
        "an omitted patch audience preserves the stored governance"
    );
    assert_eq!(stored_character_revision(&pool, &env.character_id).await, 2);

    // Explicit shared clears both columns.
    core.patch_actor_knowledge_entry(
        &principal,
        env.character_id.clone(),
        entry_id.clone(),
        2,
        None,
        nexus_local_db::FieldPatch::Keep,
        Some(KnowledgeAudience::Shared),
    )
    .await
    .expect("shared patch admits");
    assert_eq!(stored_governance(&pool, &entry_id).await, (None, None, 3));
    assert_eq!(stored_character_revision(&pool, &env.character_id).await, 3);

    // No-op: identical content and identical audience move neither revision.
    let no_op = core
        .patch_actor_knowledge_entry(
            &principal,
            env.character_id.clone(),
            entry_id.clone(),
            3,
            None,
            nexus_local_db::FieldPatch::Set("edited summary"),
            Some(KnowledgeAudience::Shared),
        )
        .await
        .expect("no-op patch admits");
    assert_eq!(no_op.revision, Some(3));
    assert_eq!(stored_character_revision(&pool, &env.character_id).await, 3);

    // Stale CAS writes nothing.
    let err = core
        .patch_actor_knowledge_entry(
            &principal,
            env.character_id.clone(),
            entry_id.clone(),
            0,
            None,
            nexus_local_db::FieldPatch::Keep,
            Some(KnowledgeAudience::AuthorOnly),
        )
        .await
        .unwrap_err();
    assert_conflict(err, "knowledge_revision_conflict");
    assert_eq!(stored_governance(&pool, &entry_id).await, (None, None, 3));
    assert_eq!(stored_character_revision(&pool, &env.character_id).await, 3);

    // A foreign Character is never a permitted patch audience, and the
    // refusal leaves the row byte-identical.
    let err = core
        .patch_actor_knowledge_entry(
            &principal,
            env.character_id.clone(),
            entry_id.clone(),
            3,
            None,
            nexus_local_db::FieldPatch::Keep,
            Some(KnowledgeAudience::CharacterPrivate {
                character_id: env.foreign_character_id.clone(),
            }),
        )
        .await
        .unwrap_err();
    assert_not_found(&err);
    assert_eq!(stored_governance(&pool, &entry_id).await, (None, None, 3));
    assert_eq!(stored_character_revision(&pool, &env.character_id).await, 3);
    pool.close().await;
}

#[tokio::test]
async fn v1191_audience_cas_binding_patch_bumps_only_the_owning_character() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let pool = plain_pool(&env).await;
    let character_holder = nexus_local_db::character_holder_entry_id(&env.character_id);

    let created = core
        .add_actor_knowledge_entry(
            &principal,
            create_request(
                "actor_world_binding",
                Some(WORLD),
                Some(&env.character_id),
                Some(&env.binding_id),
                "BindingPatchSubject",
                None,
            ),
            false,
        )
        .await
        .expect("binding-owned create admits");

    core.patch_actor_knowledge_entry(
        &principal,
        env.character_id.clone(),
        created.entry_id.clone(),
        0,
        None,
        nexus_local_db::FieldPatch::Set("binding summary"),
        Some(KnowledgeAudience::CharacterPrivate {
            character_id: env.character_id.clone(),
        }),
    )
    .await
    .expect("binding-owned governed patch admits");

    assert_eq!(
        stored_governance(&pool, &created.entry_id).await,
        (
            Some(character_holder),
            Some(DISCLOSURE_OWNER_PRIVATE.to_string()),
            1
        )
    );
    assert_eq!(
        stored_character_revision(&pool, &env.character_id).await,
        1,
        "a binding-owned governance write bumps its owning Character"
    );
    assert_eq!(
        stored_world_revision(&pool, WORLD).await,
        0,
        "the binding has no revision of its own and moves no World revision"
    );
    pool.close().await;
}

/// Regression (L2 C1): at the core layer an explicitly re-stated content field
/// may not mask a governance change (both directions).
#[tokio::test]
async fn v1191_audience_cas_restated_content_still_moves_governance() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let pool = plain_pool(&env).await;
    let creator_holder = nexus_local_db::creator_holder_entry_id(CREATOR);
    let created = core
        .add_actor_knowledge_entry(
            &principal,
            create_request(
                "character",
                None,
                Some(&env.character_id),
                None,
                "RestatedSubject",
                None,
            ),
            false,
        )
        .await
        .expect("shared create admits");

    // shared → author-only while re-stating the (absent) summary is still a
    // material governance write.
    let authored = core
        .patch_actor_knowledge_entry(
            &principal,
            env.character_id.clone(),
            created.entry_id.clone(),
            0,
            None,
            nexus_local_db::FieldPatch::Set("same"),
            Some(KnowledgeAudience::AuthorOnly),
        )
        .await
        .expect("governed patch admits");
    assert_eq!(authored.revision, Some(1));
    assert_eq!(
        stored_governance(&pool, &created.entry_id).await,
        (
            Some(creator_holder),
            Some(DISCLOSURE_OWNER_PRIVATE.to_string()),
            1
        ),
        "the authored audience must land even when the content half matches"
    );
    assert_eq!(stored_character_revision(&pool, &env.character_id).await, 1);

    // …and the same explicit summary with `shared` clears the pair.
    let cleared = core
        .patch_actor_knowledge_entry(
            &principal,
            env.character_id.clone(),
            created.entry_id.clone(),
            1,
            None,
            nexus_local_db::FieldPatch::Set("same"),
            Some(KnowledgeAudience::Shared),
        )
        .await
        .expect("shared patch admits");
    assert_eq!(cleared.revision, Some(2));
    assert_eq!(
        stored_governance(&pool, &created.entry_id).await,
        (None, None, 2)
    );
    assert_eq!(stored_character_revision(&pool, &env.character_id).await, 2);

    // A true no-op (same content, same audience) still moves neither.
    let no_op = core
        .patch_actor_knowledge_entry(
            &principal,
            env.character_id.clone(),
            created.entry_id.clone(),
            2,
            None,
            nexus_local_db::FieldPatch::Set("same"),
            Some(KnowledgeAudience::Shared),
        )
        .await
        .expect("no-op patch admits");
    assert_eq!(no_op.revision, Some(2));
    assert_eq!(stored_character_revision(&pool, &env.character_id).await, 2);
    pool.close().await;
}

/// Regression (L2 I2): an audience-bearing create takes the **exclusive**
/// Character governance fence, not the shared activity fence — a live shared
/// activity lease therefore refuses it, while the same audience-less create
/// still admits under that fence.
#[tokio::test]
async fn v1191_audience_cas_governed_create_takes_the_exclusive_fence() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let pool = plain_pool(&env).await;
    let actor = AdmittedActor::Character {
        character_id: env.character_id.clone(),
    };
    let activity = core
        .acquire_actor_activity(&principal, &actor)
        .await
        .expect("shared activity lease");
    let before = knowledge_row_count(&pool).await;

    let err = core
        .add_actor_knowledge_entry(
            &principal,
            create_request(
                "character",
                None,
                Some(&env.character_id),
                None,
                "FencedGoverned",
                Some(character_private(&env.character_id)),
            ),
            false,
        )
        .await
        .unwrap_err();
    assert_conflict(err, "character_busy");
    assert_eq!(knowledge_row_count(&pool).await, before, "refusal wrote no row");

    // The retained shared lane still admits under the same activity lease.
    core.add_actor_knowledge_entry(
        &principal,
        create_request(
            "character",
            None,
            Some(&env.character_id),
            None,
            "FencedShared",
            None,
        ),
        false,
    )
    .await
    .expect("a shared create keeps the shared fence");
    drop(activity);
    pool.close().await;
}
