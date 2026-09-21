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
use nexus_contracts::generated::daemon_api::actor_knowledge::add_knowledge_entry_request::AddKnowledgeEntryRequest;
use nexus_contracts::generated::daemon_api::characters::create_character_request::CreateCharacterRequest;
use nexus_contracts::generated::daemon_api::characters::tom::list_character_tom_query::ListCharacterTomQuery;
use nexus_contracts::generated::daemon_api::characters::tom::list_character_tom_response::ListCharacterTomResponse;
use nexus_contracts::generated::daemon_api::characters::tom::record_character_tom_request::RecordCharacterTomRequest;
use nexus_contracts::BlockType;
use nexus_core::{
    classify_pair, ActorFenceKind, ActorKnowledgePage, ActorKnowledgeViewQuery, ActorViewpoint,
    AdmittedActor, CoreAccess, CoreActorAdmission, CoreError, CoreOpenOptions, CoreService,
};
use nexus_knowledge::world_kb::knowledge_entry::{
    KnowledgeAudience, KnowledgeEntryRecord, KnowledgeOwnerRef, DISCLOSURE_OWNER_PRIVATE,
};
use nexus_knowledge::world_kb::store::{KbStore, KnowledgeReadPolicy, KnowledgeReadScope};
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_local_db::writer_protocol::{init_engine_pool, GuardedPoolOptions};
use nexus_local_db::{ensure_creator_row, CharacterPatch, CreateCharacterParams, FieldPatch};
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
                    scope
                        .authorized_holders()
                        .iter()
                        .any(|known| known == holder)
                }),
            }
        }
        // Unknown vocabulary cannot exist as a native row (the storage CHECK in
        // `seed_governance_fixture` proves it) and stays quarantined (§6).
        Some(_) => false,
    }
}

fn governed_entry(
    owner: &KnowledgeOwnerRef,
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
    sqlx::query(sql)
        .bind(value)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
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
        governed_entry(&KnowledgeOwnerRef::world(WORLD), "WorldShared", None, None),
        governed_entry(
            &KnowledgeOwnerRef::world(WORLD),
            "WorldAuthorPrivate",
            Some(&creator_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        governed_entry(
            &KnowledgeOwnerRef::world(WORLD),
            "WorldFirstPrivate",
            Some(&first_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        governed_entry(
            &KnowledgeOwnerRef::world(WORLD),
            "WorldSecondPrivate",
            Some(&second_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        governed_entry(
            &KnowledgeOwnerRef::character(&second_character_id),
            "SecondCharacterPrivate",
            Some(&second_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        governed_entry(
            &KnowledgeOwnerRef::world(FOREIGN_WORLD),
            "ForeignPrivate",
            Some(&first_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        // v1.191 P1 T6 additions: the first Character's own container carries a
        // shared row, its own private row, and a storable private row held by
        // somebody else (the shape a hidden read-by-id must not reveal); its
        // binding carries a binding-local private row.
        governed_entry(
            &KnowledgeOwnerRef::character(&env.character_id),
            "FirstCharacterShared",
            None,
            None,
        ),
        governed_entry(
            &KnowledgeOwnerRef::character(&env.character_id),
            "FirstCharacterPrivate",
            Some(&first_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        governed_entry(
            &KnowledgeOwnerRef::character(&env.character_id),
            "FirstCharacterForeignPrivate",
            Some(&second_holder),
            Some(DISCLOSURE_OWNER_PRIVATE),
        ),
        governed_entry(
            &KnowledgeOwnerRef::actor_world_binding(&env.binding_id),
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

#[allow(clippy::too_many_lines, clippy::significant_drop_tightening)] // one fixture asserted across both policies; the admitted contexts are held across the assertions
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
    let mut characters = [
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
#[allow(clippy::too_many_lines)] // one visibility journey asserted across both policies
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
        vec![
            "SecondCharacterPrivate",
            "WorldSecondPrivate",
            "WorldShared"
        ],
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

#[allow(clippy::too_many_lines)] // one audience-resolution journey
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
            create_request(
                "world",
                Some(WORLD),
                None,
                None,
                "AuthoredOnly",
                Some(author_only()),
            ),
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
    assert_eq!(
        stored_governance(&pool, &omitted.entry_id).await,
        (None, None, 0)
    );
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
    assert_eq!(
        stored_governance(&pool, &explicit.entry_id).await,
        (None, None, 0)
    );

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

    assert!(
        !unbound.is_empty(),
        "the unbound Character exists for the matrix"
    );
    assert_eq!(
        stored_governance(&pool, &omitted.entry_id).await.0,
        None,
        "no create above leaked the foreign holder {foreign_holder}"
    );
    assert_eq!(knowledge_row_count(&pool).await, 6);
    pool.close().await;
}

#[allow(clippy::too_many_lines)] // one audience-refusal journey
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
    assert_eq!(
        knowledge_row_count(&pool).await,
        before,
        "refusal wrote no row"
    );

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
    assert_eq!(
        knowledge_row_count(&pool).await,
        before,
        "no refusal wrote a row"
    );
    characters.close().await;
    pool.close().await;
}

#[allow(clippy::too_many_lines)] // one governance-move journey under the KE revision
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
    assert_eq!(
        knowledge_row_count(&pool).await,
        before,
        "refusal wrote no row"
    );

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

// ── P2-T6 — Actor admission and operation observation ─────────────────────
//
// Migrated from the retired daemon fixtures `characters_api.rs`,
// `actor_knowledge_api.rs` and `character_tom_api.rs`. HTTP envelopes, router
// and boot wiring, API-key middleware tiers and daemon-identity plumbing stay
// retired with the host; only retained core domain behavior is asserted here.
// The matching assertion-level receipt lives in the task report.

fn create_character_request(
    name: &str,
    world_id: &str,
    sheet: Option<&str>,
) -> CreateCharacterRequest {
    let mut value = serde_json::json!({ "display_name": name, "world_id": world_id });
    if let Some(sheet) = sheet {
        value["world_sheet_entry_id"] = serde_json::json!(sheet);
    }
    serde_json::from_value(value).expect("create request is wire-valid")
}

const fn character_patch<'a>(
    display_name: Option<&'a str>,
    image_uri: FieldPatch<&'a str>,
    persona_json: FieldPatch<&'a str>,
) -> CharacterPatch<'a> {
    CharacterPatch {
        display_name,
        image_uri,
        persona_json,
    }
}

fn assert_actor_conflict(err: &CoreError) {
    assert!(
        matches!(err, CoreError::ActorConflict { .. }),
        "expected an ActorConflict, got {err:?}"
    );
}

async fn character_epoch(env: &Env, character_id: &str) -> i64 {
    let pool = plain_pool(env).await;
    let epoch: i64 =
        sqlx::query_scalar("SELECT lifecycle_epoch FROM characters WHERE character_id = ?")
            .bind(character_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    pool.close().await;
    epoch
}

async fn character_count(env: &Env) -> i64 {
    let pool = plain_pool(env).await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM characters")
        .fetch_one(&pool)
        .await
        .unwrap();
    pool.close().await;
    count
}

async fn binding_count(env: &Env, character_id: &str) -> i64 {
    let pool = plain_pool(env).await;
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM actor_world_bindings WHERE character_id = ? AND status = 'active'",
    )
    .bind(character_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    pool.close().await;
    count
}

async fn set_world_status(env: &Env, world_id: &str, status: &str) {
    let pool = plain_pool(env).await;
    sqlx::query("UPDATE narrative_worlds SET status = ? WHERE world_id = ?")
        .bind(status)
        .bind(world_id)
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
}

async fn set_character_status(env: &Env, character_id: &str, status: &str) {
    let pool = plain_pool(env).await;
    sqlx::query("UPDATE characters SET status = ? WHERE character_id = ?")
        .bind(status)
        .bind(character_id)
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
}

/// One World-owned `character` `KeyBlock`: the only shape eligible as a
/// `WorldSheet` link (live, same World, shared).
async fn seed_sheet(env: &Env, name: &str, world_id: &str, block_type: BlockType) -> String {
    let pool = plain_pool(env).await;
    let store = SqliteKbStore::new(pool.clone());
    let row = KnowledgeEntryRecord::new(world_id, block_type, name);
    let entry_id = row.entry_id.clone();
    store.insert_knowledge_entry(row).await.unwrap();
    pool.close().await;
    entry_id
}

/// Migrated `characters_api::create_character_returns_201_with_character_and_binding`,
/// `::create_rejects_unowned_world_as_not_found`,
/// `::create_rejects_invalid_world_sheet_with_stable_409`,
/// `::create_character_rejects_paused_world_with_404_zero_mutation`,
/// `::duplicate_display_name_is_stable_409` and
/// `::show_and_list_are_active_creator_scoped`.
#[tokio::test]
async fn retained_character_create_rules_and_owner_scoping() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;

    let created = core
        .create_character(&principal, create_character_request("Ava", WORLD, None))
        .await
        .expect("create admits on an owned active World");
    assert!(created.character.character_id.starts_with("chr_"));
    assert_eq!(
        String::from(created.character.owner_creator_id.clone()),
        CREATOR
    );
    assert_eq!(String::from(created.character.display_name.clone()), "Ava");
    assert_eq!(created.character.status.to_string(), "active");
    assert!(created.binding.binding_id.starts_with("awb_"));
    assert_eq!(String::from(created.binding.world_id.clone()), WORLD);
    assert_eq!(created.binding.status.to_string(), "active");

    // A duplicate display name is a stable conflict with zero second row.
    let before = character_count(&env).await;
    let dup = core
        .create_character(&principal, create_character_request("Ava", WORLD_B, None))
        .await
        .unwrap_err();
    assert_conflict(dup, "duplicate_character_display_name");
    assert_eq!(character_count(&env).await, before);

    // A non-eligible WorldSheet link is refused at create time too.
    let wrong_sheet = seed_sheet(&env, "sheet_wrong_create", WORLD, BlockType::Item).await;
    let sheeted = core
        .create_character(
            &principal,
            create_character_request("Sheeted", WORLD, Some(&wrong_sheet)),
        )
        .await
        .unwrap_err();
    assert_conflict(sheeted, "invalid_world_sheet");
    assert_eq!(character_count(&env).await, before);

    // A foreign World is indistinguishable from a missing one, with no write.
    let foreign = core
        .create_character(
            &principal,
            create_character_request("Ghost", FOREIGN_WORLD, None),
        )
        .await
        .unwrap_err();
    assert_not_found(&foreign);

    // An owned but inactive World is not an active owner either.
    set_world_status(&env, WORLD, "paused").await;
    let paused = core
        .create_character(&principal, create_character_request("Paused", WORLD, None))
        .await
        .unwrap_err();
    assert_not_found(&paused);
    assert_eq!(character_count(&env).await, before);
    set_world_status(&env, WORLD, "active").await;

    // Show/list stay scoped to the selected creator: the other creator's
    // Character is neither listed nor readable, so nothing leaks.
    let listed = core.list_characters(&principal, 50, 0).await.expect("list");
    let ids: Vec<String> = listed
        .items
        .iter()
        .map(|item| String::from(item.character_id.clone()))
        .collect();
    assert!(ids.contains(&env.character_id));
    assert!(!ids.contains(&env.foreign_character_id));
    let shown = core
        .character(&principal, env.character_id.clone())
        .await
        .expect("owned detail");
    assert_eq!(
        String::from(shown.character.character_id.clone()),
        env.character_id
    );
    let hidden = core
        .character(&principal, env.foreign_character_id.clone())
        .await
        .unwrap_err();
    assert_not_found(&hidden);
}

/// Migrated `characters_api::list_paginates_with_opaque_cursor` and
/// `::list_paginates_large_fixture_with_sql_bounds`: the retained offset page
/// walks every owned Character exactly once and terminates.
#[tokio::test]
async fn retained_character_list_offset_pagination_bounds() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let mut expected = vec![env.character_id.clone()];
    for i in 0..25 {
        let created = core
            .create_character(
                &principal,
                create_character_request(&format!("Page{i:02}"), WORLD, None),
            )
            .await
            .expect("create");
        expected.push(String::from(created.character.character_id.clone()));
    }
    expected.sort();

    let mut seen = Vec::new();
    let mut offset = 0u32;
    loop {
        let page = core
            .list_characters(&principal, 10, offset)
            .await
            .expect("page");
        assert!(page.items.len() <= 10, "limit is a hard bound");
        for item in &page.items {
            seen.push(String::from(item.character_id.clone()));
        }
        if !page.pagination.has_more {
            assert!(
                page.pagination.next_cursor.is_none(),
                "a terminal page carries no cursor"
            );
            break;
        }
        assert!(page.pagination.next_cursor.is_some());
        offset += 10;
        assert!(offset < 100, "the walk must terminate");
    }
    seen.sort();
    assert_eq!(seen.len(), expected.len(), "no duplicate page entry");
    assert_eq!(seen, expected, "no skipped owned Character");
}

/// Migrated `characters_api::patch_character_cas_updates_revision_and_selected_fields`,
/// `::patch_stale_revision_is_character_revision_conflict`,
/// `::patch_omit_vs_clear_members`, `::patch_no_op_leaves_revision_unchanged`
/// and `::patch_rename_collision_is_duplicate_character_display_name`.
#[allow(clippy::too_many_lines)] // one patch/CAS journey asserted end to end
#[tokio::test]
async fn retained_character_patch_cas_omit_clear_and_rename_collision() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let (_, revision, _) = character_row(&env, &env.character_id).await;

    let patched = core
        .patch_character(
            &principal,
            env.character_id.clone(),
            revision,
            character_patch(
                Some("Ada Renamed"),
                FieldPatch::Set("https://example.test/ada.png"),
                FieldPatch::Set("{\"role\":\"scout\"}"),
            ),
        )
        .await
        .expect("patch admits");
    assert_eq!(
        String::from(patched.character.display_name.clone()),
        "Ada Renamed"
    );
    assert_eq!(patched.character.revision, revision + 1);
    assert_eq!(
        patched
            .character
            .image_uri
            .clone()
            .map(String::from)
            .as_deref(),
        Some("https://example.test/ada.png")
    );
    assert_eq!(patched.character.persona["role"], "scout");

    // A stale revision is a conflict carrying the retained code.
    let stale = core
        .patch_character(
            &principal,
            env.character_id.clone(),
            revision,
            character_patch(Some("Stale"), FieldPatch::Keep, FieldPatch::Keep),
        )
        .await
        .unwrap_err();
    assert_conflict(stale, "character_revision_conflict");

    // Omission keeps the stored member; an explicit clear removes it.
    let omitted = core
        .patch_character(
            &principal,
            env.character_id.clone(),
            revision + 1,
            character_patch(
                Some("Ada Renamed The Second"),
                FieldPatch::Keep,
                FieldPatch::Keep,
            ),
        )
        .await
        .expect("omitted members are preserved");
    assert_eq!(
        omitted
            .character
            .image_uri
            .clone()
            .map(String::from)
            .as_deref(),
        Some("https://example.test/ada.png")
    );
    assert_eq!(omitted.character.persona["role"], "scout");
    let cleared = core
        .patch_character(
            &principal,
            env.character_id.clone(),
            revision + 2,
            character_patch(None, FieldPatch::Clear, FieldPatch::Clear),
        )
        .await
        .expect("clear admits");
    assert!(cleared.character.image_uri.is_none());
    assert!(cleared.character.persona.is_empty());
    assert_eq!(cleared.character.revision, revision + 3);

    // A no-op (same values, same revision) leaves the revision untouched.
    let no_op = core
        .patch_character(
            &principal,
            env.character_id.clone(),
            revision + 3,
            character_patch(
                Some("Ada Renamed The Second"),
                FieldPatch::Clear,
                FieldPatch::Clear,
            ),
        )
        .await
        .expect("no-op admits");
    assert_eq!(no_op.character.revision, revision + 3);

    // A rename onto another owned Character's name is refused with zero
    // mutation of the target row.
    let second = core
        .create_character(&principal, create_character_request("Bea", WORLD_B, None))
        .await
        .expect("second create");
    let second_id = String::from(second.character.character_id.clone());
    let collision = core
        .patch_character(
            &principal,
            second_id.clone(),
            second.character.revision,
            character_patch(
                Some("Ada Renamed The Second"),
                FieldPatch::Keep,
                FieldPatch::Keep,
            ),
        )
        .await
        .unwrap_err();
    assert_conflict(collision, "duplicate_character_display_name");
    let (status, name, revision_after) = {
        let pool = plain_pool(&env).await;
        let row: (String, String, i64) = sqlx::query_as(
            "SELECT status, display_name, revision FROM characters WHERE character_id = ?",
        )
        .bind(&second_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        pool.close().await;
        row
    };
    assert_eq!(name, "Bea");
    assert_eq!(status, "active");
    assert_eq!(revision_after, second.character.revision);
}

/// Migrated `characters_api::archive_restore_round_trip_and_list_includes_archived`,
/// `::archive_same_state_cas_no_op_keeps_revision`,
/// `::restore_requires_active_owned_world_binding` and
/// `::restore_name_collision_is_duplicate_character_display_name`.
#[allow(clippy::too_many_lines)] // one archive/restore lifecycle asserted end to end
#[tokio::test]
async fn retained_character_archive_restore_lifecycle_and_guards() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let (_, revision, epoch) = character_row(&env, &env.character_id).await;

    let archived = core
        .transition_character(
            &principal,
            transition_request(
                &env.character_id,
                revision,
                CoreCharacterTransitionRequestTargetStatus::Archived,
            ),
        )
        .await
        .expect("archive commits");
    assert_eq!(archived.character.status.to_string(), "archived");
    assert_eq!(archived.character.revision, revision + 1);
    let (status, archived_revision, archived_epoch) = character_row(&env, &env.character_id).await;
    assert_eq!(status, "archived");
    assert_eq!(
        archived_epoch,
        epoch + 1,
        "a material transition moves the epoch"
    );

    // The listing still carries archived rows (retained management read).
    let listed = core.list_characters(&principal, 50, 0).await.expect("list");
    assert!(listed
        .items
        .iter()
        .any(
            |item| String::from(item.character_id.clone()) == env.character_id
                && item.status.to_string() == "archived"
        ));

    // Writing an archived Character is refused, and the same-state archive is
    // a no-op that moves neither the revision nor the lifecycle epoch.
    let denied = core
        .patch_character(
            &principal,
            env.character_id.clone(),
            archived_revision,
            character_patch(Some("Nope"), FieldPatch::Keep, FieldPatch::Keep),
        )
        .await
        .unwrap_err();
    assert_conflict(denied, "character_inactive");
    let again = core
        .transition_character(
            &principal,
            transition_request(
                &env.character_id,
                archived_revision,
                CoreCharacterTransitionRequestTargetStatus::Archived,
            ),
        )
        .await
        .expect("same-state archive admits");
    assert_eq!(again.character.revision, archived_revision);
    assert_eq!(
        character_epoch(&env, &env.character_id).await,
        archived_epoch
    );

    let restored = core
        .transition_character(
            &principal,
            transition_request(
                &env.character_id,
                archived_revision,
                CoreCharacterTransitionRequestTargetStatus::Active,
            ),
        )
        .await
        .expect("restore commits");
    assert_eq!(restored.character.status.to_string(), "active");
    assert_eq!(
        String::from(restored.character.character_id.clone()),
        env.character_id
    );

    // Restore needs a live owned World binding.
    let (_, revision, _) = character_row(&env, &env.character_id).await;
    core.transition_character(
        &principal,
        transition_request(
            &env.character_id,
            revision,
            CoreCharacterTransitionRequestTargetStatus::Archived,
        ),
    )
    .await
    .expect("archive again");
    set_world_status(&env, WORLD, "paused").await;
    let (_, archived_revision, _) = character_row(&env, &env.character_id).await;
    let no_binding = core
        .transition_character(
            &principal,
            transition_request(
                &env.character_id,
                archived_revision,
                CoreCharacterTransitionRequestTargetStatus::Active,
            ),
        )
        .await
        .unwrap_err();
    assert_conflict(no_binding, "character_restore_requires_active_binding");
    set_world_status(&env, WORLD, "active").await;

    // Restore colliding with a live display name is refused.
    let collision = core
        .create_character(&principal, create_character_request("Ada", WORLD_B, None))
        .await
        .expect("colliding name while archived");
    let _ = collision;
    let (_, archived_revision, _) = character_row(&env, &env.character_id).await;
    let collided = core
        .transition_character(
            &principal,
            transition_request(
                &env.character_id,
                archived_revision,
                CoreCharacterTransitionRequestTargetStatus::Active,
            ),
        )
        .await
        .unwrap_err();
    assert_conflict(collided, "duplicate_character_display_name");
}

/// Migrated `characters_api::binding_detail_link_relink_clear_and_cas_errors`,
/// `::patch_binding_rejects_invalid_world_sheet`,
/// `::patch_binding_rejects_overlength_world_sheet_bytes_at_api`,
/// `::binding_detail_retained_read_survives_archive` and
/// `::add_binding_rejects_paused_world_with_404_zero_mutation`.
#[allow(clippy::too_many_lines)] // one binding/WorldSheet CAS journey asserted end to end
#[tokio::test]
async fn retained_binding_world_sheet_cas_and_retained_read() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let sheet_a = seed_sheet(&env, "sheet_a", WORLD, BlockType::Character).await;
    let sheet_b = seed_sheet(&env, "sheet_b", WORLD, BlockType::Character).await;
    let wrong_type = seed_sheet(&env, "sheet_wrong", WORLD, BlockType::Item).await;

    let shown = core
        .binding(&principal, env.character_id.clone(), env.binding_id.clone())
        .await
        .expect("binding detail");
    assert_eq!(shown.binding.revision, 0);

    let linked = core
        .patch_binding(
            &principal,
            env.character_id.clone(),
            env.binding_id.clone(),
            0,
            FieldPatch::Set(sheet_a.as_str()),
        )
        .await
        .expect("link admits");
    assert_eq!(linked.binding.revision, 1);
    assert_eq!(
        linked
            .binding
            .world_sheet_entry_id
            .clone()
            .map(String::from)
            .as_deref(),
        Some(sheet_a.as_str())
    );

    // Relink, clear, then a same-state no-op that keeps the revision.
    let relinked = core
        .patch_binding(
            &principal,
            env.character_id.clone(),
            env.binding_id.clone(),
            1,
            FieldPatch::Set(sheet_b.as_str()),
        )
        .await
        .expect("relink admits");
    assert_eq!(relinked.binding.revision, 2);
    let cleared = core
        .patch_binding(
            &principal,
            env.character_id.clone(),
            env.binding_id.clone(),
            2,
            FieldPatch::Clear,
        )
        .await
        .expect("clear admits");
    assert_eq!(cleared.binding.revision, 3);
    assert!(cleared.binding.world_sheet_entry_id.is_none());
    let untouched = core
        .patch_binding(
            &principal,
            env.character_id.clone(),
            env.binding_id.clone(),
            3,
            FieldPatch::Clear,
        )
        .await
        .expect("no-op admits");
    assert_eq!(untouched.binding.revision, 3);

    // A stale revision conflicts.
    let stale = core
        .patch_binding(
            &principal,
            env.character_id.clone(),
            env.binding_id.clone(),
            2,
            FieldPatch::Clear,
        )
        .await
        .unwrap_err();
    assert_conflict(stale, "binding_revision_conflict");

    // Wrong-type sheet: refused as `invalid_world_sheet` with an explanation,
    // never the bare code as its own message.
    let wrong = core
        .patch_binding(
            &principal,
            env.character_id.clone(),
            env.binding_id.clone(),
            3,
            FieldPatch::Set(wrong_type.as_str()),
        )
        .await
        .unwrap_err();
    match &wrong {
        CoreError::ActorConflict { code, message } => {
            assert_eq!(code, "invalid_world_sheet");
            assert_ne!(message, "invalid_world_sheet");
        }
        other => panic!("expected invalid_world_sheet, got {other:?}"),
    }

    // 128 Unicode scalars but 131 bytes: the storage byte bound, not the wire
    // scalar bound, decides.
    let overlength = format!("kb_{}{}", "a".repeat(124), "\u{1f3ad}");
    assert_eq!(overlength.chars().count(), 128);
    assert!(overlength.len() > 128);
    let bytes = core
        .patch_binding(
            &principal,
            env.character_id.clone(),
            env.binding_id.clone(),
            3,
            FieldPatch::Set(overlength.as_str()),
        )
        .await
        .unwrap_err();
    assert_conflict(bytes, "invalid_world_sheet");

    // Link a live sheet again, archive the Character, and prove the retained
    // read still answers while every write is refused.
    core.patch_binding(
        &principal,
        env.character_id.clone(),
        env.binding_id.clone(),
        3,
        FieldPatch::Set(sheet_a.as_str()),
    )
    .await
    .expect("relink");
    let (_, revision, _) = character_row(&env, &env.character_id).await;
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
    let retained = core
        .binding(&principal, env.character_id.clone(), env.binding_id.clone())
        .await
        .expect("retained read after archive");
    assert_eq!(
        retained
            .binding
            .world_sheet_entry_id
            .clone()
            .map(String::from)
            .as_deref(),
        Some(sheet_a.as_str())
    );
    let write_denied = core
        .patch_binding(
            &principal,
            env.character_id.clone(),
            env.binding_id.clone(),
            4,
            FieldPatch::Clear,
        )
        .await
        .unwrap_err();
    assert_conflict(write_denied, "character_inactive");

    set_character_status(&env, &env.character_id, "active").await;
    set_world_status(&env, WORLD_B, "paused").await;
    let before = binding_count(&env, &env.character_id).await;
    let paused = core
        .add_binding(
            &principal,
            env.character_id.clone(),
            WORLD_B.to_string(),
            None,
        )
        .await
        .unwrap_err();
    assert_not_found(&paused);
    assert_eq!(
        binding_count(&env, &env.character_id).await,
        before,
        "a refused add writes no binding"
    );
}

// ── Actor KnowledgeView / detail family ───────────────────────────────────

fn view_query(world_id: &str, binding_id: Option<&str>, limit: u32) -> ActorKnowledgeViewQuery {
    ActorKnowledgeViewQuery {
        world_id: world_id.to_string(),
        binding_id: binding_id.map(str::to_string),
        limit,
        cursor: None,
    }
}

fn character_actor(character_id: &str) -> AdmittedActor {
    AdmittedActor::Character {
        character_id: character_id.to_string(),
    }
}

async fn insert_legacy_world_row(pool: &SqlitePool, entry_id: &str, world_id: &str, name: &str) {
    sqlx::query(
        "INSERT INTO kb_key_blocks \
         (key_block_id, owner_kind, world_id, block_type, canonical_name, status) \
         VALUES (?, 'world', ?, 'item', ?, 'confirmed')",
    )
    .bind(entry_id)
    .bind(world_id)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_world_row_at(
    pool: &SqlitePool,
    entry_id: &str,
    world_id: &str,
    name: &str,
    created_at: &str,
) {
    sqlx::query(
        "INSERT INTO kb_key_blocks \
         (key_block_id, owner_kind, world_id, block_type, canonical_name, status, created_at) \
         VALUES (?, 'world', ?, 'item', ?, 'confirmed', ?)",
    )
    .bind(entry_id)
    .bind(world_id)
    .bind(name)
    .bind(created_at)
    .execute(pool)
    .await
    .unwrap();
}

fn page_ids(page: &ActorKnowledgePage) -> Vec<String> {
    let mut ids: Vec<String> = page.items.iter().map(|row| row.entry_id.clone()).collect();
    ids.sort();
    ids
}

/// Migrated `actor_knowledge_api::invalid_and_unbound_owners_are_404`,
/// `::character_view_without_binding_id_is_422_not_creator_page` and
/// `::character_view_and_binding_add_require_owned_target_world`.
#[tokio::test]
async fn retained_knowledge_view_admission_and_binding_requirement() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;

    // A missing/foreign World and a foreign Character are indistinguishable
    // from missing rows on the view.
    let missing_world = core
        .actor_knowledge_view(
            &principal,
            &character_actor(&env.character_id),
            view_query("wld_missing", Some(&env.binding_id), 50),
        )
        .await
        .unwrap_err();
    assert_not_found(&missing_world);
    let missing_character = core
        .actor_knowledge_view(
            &principal,
            &character_actor("chr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            view_query(WORLD, Some(&env.binding_id), 50),
        )
        .await
        .unwrap_err();
    assert_not_found(&missing_character);

    // A Character actor_ref without its binding never degrades into the
    // Creator management review.
    let unbounded = core
        .actor_knowledge_view(
            &principal,
            &character_actor(&env.character_id),
            view_query(WORLD, None, 50),
        )
        .await
        .unwrap_err();
    assert_invalid_input(&unbounded);

    // Authoring for a foreign owner is refused and writes nothing.
    let before = {
        let pool = plain_pool(&env).await;
        let rows = knowledge_row_count(&pool).await;
        pool.close().await;
        rows
    };
    let add_foreign = core
        .add_actor_knowledge_entry(
            &principal,
            create_request(
                "character",
                None,
                Some("chr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
                None,
                "Nope",
                None,
            ),
            false,
        )
        .await
        .unwrap_err();
    assert_not_found(&add_foreign);

    // A binding that targets a foreign World is outside the owned scope.
    let foreign_binding = "awb_cccccccccccccccccccccccccccccccc";
    {
        let pool = plain_pool(&env).await;
        sqlx::query(
            "INSERT INTO actor_world_bindings \
             (binding_id, character_id, world_id, status, created_at, updated_at) \
             VALUES (?, ?, ?, 'active', datetime('now'), datetime('now'))",
        )
        .bind(foreign_binding)
        .bind(&env.character_id)
        .bind(FOREIGN_WORLD)
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;
    }
    let foreign_view = core
        .actor_knowledge_view(
            &principal,
            &character_actor(&env.character_id),
            view_query(FOREIGN_WORLD, Some(foreign_binding), 50),
        )
        .await
        .unwrap_err();
    assert_not_found(&foreign_view);
    let foreign_add = core
        .add_actor_knowledge_entry(
            &principal,
            create_request(
                "actor_world_binding",
                Some(FOREIGN_WORLD),
                Some(&env.character_id),
                Some(foreign_binding),
                "ShouldNotInsert",
                None,
            ),
            false,
        )
        .await
        .unwrap_err();
    assert_not_found(&foreign_add);
    let after = {
        let pool = plain_pool(&env).await;
        let rows = knowledge_row_count(&pool).await;
        pool.close().await;
        rows
    };
    assert_eq!(after, before, "refused admissions author no row");
}

/// Migrated `actor_knowledge_api::non_last_binding_with_owned_knowledge_is_stable_409`
/// and `::last_binding_409_wins_even_with_owned_knowledge`.
#[tokio::test]
async fn retained_binding_removal_blocked_by_owned_knowledge() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let second = core
        .add_binding(
            &principal,
            env.character_id.clone(),
            WORLD_B.to_string(),
            None,
        )
        .await
        .expect("second binding");
    let second_id = String::from(second.binding.binding_id.clone());
    core.add_actor_knowledge_entry(
        &principal,
        create_request(
            "actor_world_binding",
            Some(WORLD),
            Some(&env.character_id),
            Some(&env.binding_id),
            "LocalA",
            None,
        ),
        false,
    )
    .await
    .expect("binding-local entry");

    let before_bindings = binding_count(&env, &env.character_id).await;
    let before_rows = {
        let pool = plain_pool(&env).await;
        let rows: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM kb_key_blocks WHERE actor_world_binding_id = ?",
        )
        .bind(&env.binding_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        pool.close().await;
        rows
    };
    let refused = core
        .remove_binding(&principal, env.character_id.clone(), env.binding_id.clone())
        .await
        .unwrap_err();
    assert_conflict(refused, "binding_has_owned_knowledge");
    assert_eq!(
        binding_count(&env, &env.character_id).await,
        before_bindings,
        "a refused removal deletes no binding"
    );
    let after_rows = {
        let pool = plain_pool(&env).await;
        let rows: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM kb_key_blocks WHERE actor_world_binding_id = ?",
        )
        .bind(&env.binding_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        pool.close().await;
        rows
    };
    assert_eq!(after_rows, before_rows, "the owned entry survives");

    // The last-binding conflict wins over the owned-knowledge conflict.
    core.remove_binding(&principal, env.character_id.clone(), second_id)
        .await
        .expect("the non-last binding removes");
    let last = core
        .remove_binding(&principal, env.character_id.clone(), env.binding_id.clone())
        .await
        .unwrap_err();
    assert_conflict(last, "last_active_actor_world_binding");
    assert_eq!(binding_count(&env, &env.character_id).await, 1);
}

/// Migrated `actor_knowledge_api::view_projects_legacy_sqlite_datetime_without_rewriting_bytes`
/// (the stored-bytes half; the daemon's RFC3339 wire canonicalization was a
/// handler projection that retires with the host) and
/// `::view_paginates_same_millisecond_reverse_ids_without_skip_or_duplicate`.
#[allow(clippy::too_many_lines)] // one knowledge view journey asserted end to end
#[tokio::test]
async fn retained_knowledge_view_stored_bytes_and_keyset_tie_break() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;

    // A pre-cutover World row carries SQLite `datetime('now')` bytes; the wire
    // projection canonicalizes them and never rewrites the stored value.
    let pool = plain_pool(&env).await;
    insert_legacy_world_row(
        &pool,
        "kb_legacy0000000000000000000000001",
        WORLD,
        "LegacyRow",
    )
    .await;
    let stored: String = sqlx::query_scalar(
        "SELECT created_at FROM kb_key_blocks WHERE key_block_id = 'kb_legacy0000000000000000000000001'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        stored.contains(' ') && !stored.contains('T'),
        "legacy bytes stay SQLite datetime: {stored}"
    );

    // Two rows sharing one millisecond: the keyset must tie-break
    // deterministically and never skip or duplicate around the tie.
    insert_world_row_at(
        &pool,
        "kb_m",
        WORLD,
        "TieLateId",
        "2026-01-01T10:00:00.123200Z",
    )
    .await;
    insert_world_row_at(
        &pool,
        "kb_a",
        WORLD,
        "TieEarlyId",
        "2026-01-01T10:00:00.123200Z",
    )
    .await;
    pool.close().await;

    let full = core
        .actor_knowledge_view(
            &principal,
            &character_actor(&env.character_id),
            view_query(WORLD, Some(&env.binding_id), 50),
        )
        .await
        .expect("full page");
    let legacy = full
        .items
        .iter()
        .find(|row| row.entry_id == "kb_legacy0000000000000000000000001")
        .expect("legacy row is visible");
    assert_eq!(
        legacy.created_at, stored,
        "the read never rewrites the stored timestamp bytes"
    );
    let stored_after: String = {
        let pool = plain_pool(&env).await;
        let value = sqlx::query_scalar(
            "SELECT created_at FROM kb_key_blocks WHERE key_block_id = 'kb_legacy0000000000000000000000001'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        pool.close().await;
        value
    };
    assert_eq!(stored_after, stored, "the view never rewrites the row");

    let mut expected = page_ids(&full);
    expected.retain(|id| id == "kb_a" || id == "kb_m");
    assert_eq!(expected.len(), 2, "both tied rows are on the full page");

    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let page = core
            .actor_knowledge_view(
                &principal,
                &character_actor(&env.character_id),
                ActorKnowledgeViewQuery {
                    world_id: WORLD.to_string(),
                    binding_id: Some(env.binding_id.clone()),
                    limit: 1,
                    cursor,
                },
            )
            .await
            .expect("single-row page");
        for row in &page.items {
            if row.entry_id == "kb_a" || row.entry_id == "kb_m" {
                seen.push(row.entry_id.clone());
            }
        }
        if !page.has_more {
            break;
        }
        cursor = page.next_cursor.clone();
        assert!(cursor.is_some(), "a continuing page carries a cursor");
        assert!(seen.len() <= 4, "the walk must terminate");
    }
    seen.sort();
    seen.dedup();
    assert_eq!(
        seen, expected,
        "paging around the tie skips and duplicates nothing"
    );

    // A malformed keyset token is refused before any query runs.
    let bad_cursor = core
        .actor_knowledge_view(
            &principal,
            &character_actor(&env.character_id),
            ActorKnowledgeViewQuery {
                world_id: WORLD.to_string(),
                binding_id: Some(env.binding_id.clone()),
                limit: 50,
                cursor: Some("v1:12".to_string()),
            },
        )
        .await
        .unwrap_err();
    assert_invalid_input(&bad_cursor);
}

/// Migrated `actor_knowledge_api::inactive_world_and_character_fail_closed_on_view_and_add`,
/// `::knowledge_archived_character_detail_read_write_split` and
/// `::knowledge_detail_wrong_scope_is_404_without_revision_leak`.
#[allow(clippy::too_many_lines)] // one inactive-knowledge scope journey asserted end to end
#[tokio::test]
async fn retained_knowledge_inactive_read_write_split_and_detail_scope() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let created = core
        .add_actor_knowledge_entry(
            &principal,
            create_request(
                "character",
                None,
                Some(&env.character_id),
                None,
                "Keystone",
                None,
            ),
            false,
        )
        .await
        .expect("create admits");
    let entry_id = created.entry_id.clone();

    // Detail reads hide a foreign scope and never leak a revision.
    let foreign_character = core
        .actor_knowledge_entry(
            &principal,
            "chr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
            entry_id.clone(),
        )
        .await
        .unwrap_err();
    assert_not_found(&foreign_character);
    assert!(
        !foreign_character.to_string().contains("revision"),
        "a hidden detail leaks no revision: {foreign_character}"
    );
    let foreign_entry = core
        .actor_knowledge_entry(
            &principal,
            env.character_id.clone(),
            "kb_missingentry".to_string(),
        )
        .await
        .unwrap_err();
    assert_not_found(&foreign_entry);

    // An archived Character keeps its retained reads and refuses writes.
    set_character_status(&env, &env.character_id, "archived").await;
    let retained_view = core
        .actor_knowledge_view(
            &principal,
            &character_actor(&env.character_id),
            view_query(WORLD, Some(&env.binding_id), 50),
        )
        .await
        .expect("an archived Character view is a retained read");
    assert!(retained_view
        .items
        .iter()
        .any(|row| row.entry_id == entry_id));
    let retained_detail = core
        .actor_knowledge_entry(&principal, env.character_id.clone(), entry_id.clone())
        .await
        .expect("an archived Character detail is a retained read");
    assert_eq!(retained_detail.entry_id, entry_id);

    let inactive_add = core
        .add_actor_knowledge_entry(
            &principal,
            create_request(
                "character",
                None,
                Some(&env.character_id),
                None,
                "Nope",
                None,
            ),
            false,
        )
        .await
        .unwrap_err();
    assert_conflict(inactive_add, "character_inactive");
    let inactive_revision =
        i64::try_from(retained_detail.revision.unwrap_or(0)).expect("stored revision fits i64");
    let inactive_patch = core
        .patch_actor_knowledge_entry(
            &principal,
            env.character_id.clone(),
            entry_id.clone(),
            inactive_revision,
            None,
            FieldPatch::Set("nope"),
            None,
        )
        .await
        .unwrap_err();
    assert_conflict(inactive_patch, "character_inactive");
    set_character_status(&env, &env.character_id, "active").await;

    // An archived owned World keeps its retained review and refuses the
    // World-owned write.
    set_world_status(&env, WORLD, "archived").await;
    core.actor_knowledge_view(
        &principal,
        &AdmittedActor::Creator {
            creator_id: CREATOR.to_string(),
        },
        view_query(WORLD, None, 50),
    )
    .await
    .expect("an archived World Creator review is a retained read");
    let world_inactive = core
        .add_actor_knowledge_entry(
            &principal,
            create_request("world", Some(WORLD), None, None, "Nope", None),
            false,
        )
        .await
        .unwrap_err();
    assert_conflict(world_inactive, "world_inactive");
    set_world_status(&env, WORLD, "active").await;
}

/// Migrated `actor_knowledge_api::knowledge_detail_summary_lifecycle_and_preserves_body_keys`,
/// `::knowledge_patch_omitted_summary_preserves_value`,
/// `::knowledge_stale_revision_and_in_use_delete_errors`,
/// `::knowledge_empty_patch_is_invalid_input` and
/// `::knowledge_delete_malformed_expected_revision_is_invalid_input`.
#[allow(clippy::too_many_lines)] // one knowledge summary/CAS journey asserted end to end
#[tokio::test]
async fn retained_knowledge_detail_summary_lifecycle_and_cas_guards() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let mut create = serde_json::json!({
        "owner_kind": "character",
        "character_id": &env.character_id,
        "block_type": "item",
        "canonical_name": "Fact",
        "summary": "initial summary",
    });
    let created = core
        .add_actor_knowledge_entry(
            &principal,
            serde_json::from_value(create.clone()).unwrap(),
            true,
        )
        .await
        .expect("create admits");
    create["canonical_name"] = serde_json::json!("unused");
    let entry_id = created.entry_id.clone();

    // An unknown sibling body member survives every summary edit.
    {
        let pool = plain_pool(&env).await;
        sqlx::query("UPDATE kb_key_blocks SET body_json = ? WHERE key_block_id = ?")
            .bind(r#"{"summary":"initial summary","custom_flag":true}"#)
            .bind(&entry_id)
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }

    let detail = core
        .actor_knowledge_entry(&principal, env.character_id.clone(), entry_id.clone())
        .await
        .expect("detail");
    assert_eq!(
        detail
            .body
            .as_ref()
            .and_then(|body| body.summary.as_deref()),
        Some("initial summary")
    );
    let revision = i64::try_from(detail.revision.unwrap_or(0)).expect("stored revision fits i64");

    let patched = core
        .patch_actor_knowledge_entry(
            &principal,
            env.character_id.clone(),
            entry_id.clone(),
            revision,
            None,
            FieldPatch::Set("edited summary"),
            None,
        )
        .await
        .expect("summary patch admits");
    assert_eq!(
        patched
            .body
            .as_ref()
            .and_then(|body| body.summary.as_deref()),
        Some("edited summary")
    );
    assert_eq!(patched.revision, Some(u64::try_from(revision + 1).unwrap()));

    // A rename-only patch preserves the summary.
    let renamed = core
        .patch_actor_knowledge_entry(
            &principal,
            env.character_id.clone(),
            entry_id.clone(),
            revision + 1,
            Some("FactRenamed"),
            FieldPatch::Keep,
            None,
        )
        .await
        .expect("rename admits");
    assert_eq!(renamed.canonical_name, "FactRenamed");
    assert_eq!(
        renamed
            .body
            .as_ref()
            .and_then(|body| body.summary.as_deref()),
        Some("edited summary")
    );
    assert_eq!(renamed.revision, Some(u64::try_from(revision + 2).unwrap()));

    // A null summary clears it; an empty patch is refused; a stale revision
    // conflicts.
    let cleared = core
        .patch_actor_knowledge_entry(
            &principal,
            env.character_id.clone(),
            entry_id.clone(),
            revision + 2,
            None,
            FieldPatch::Clear,
            None,
        )
        .await
        .expect("clear admits");
    assert!(cleared
        .body
        .as_ref()
        .and_then(|body| body.summary.as_ref())
        .is_none());
    let stale = core
        .patch_actor_knowledge_entry(
            &principal,
            env.character_id.clone(),
            entry_id.clone(),
            revision,
            None,
            FieldPatch::Set("late"),
            None,
        )
        .await
        .unwrap_err();
    assert_conflict(stale, "knowledge_revision_conflict");

    let body_json: String = {
        let pool = plain_pool(&env).await;
        let value =
            sqlx::query_scalar("SELECT body_json FROM kb_key_blocks WHERE key_block_id = ?")
                .bind(&entry_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        pool.close().await;
        value
    };
    assert!(
        body_json.contains("custom_flag"),
        "an unknown body member is never dropped: {body_json}"
    );

    // An anchored entry cannot be deleted; a stale revision conflicts.
    {
        let pool = plain_pool(&env).await;
        sqlx::query(
            "INSERT INTO kb_source_anchors (key_block_id, anchor_ordinal, source_anchor_json) VALUES (?, 0, '{}')",
        )
        .bind(&entry_id)
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;
    }
    let in_use = core
        .delete_actor_knowledge_entry(
            &principal,
            env.character_id.clone(),
            entry_id.clone(),
            revision + 3,
        )
        .await
        .unwrap_err();
    assert_conflict(in_use, "knowledge_entry_in_use");

    // An unrepresentable revision is refused by the CAS domain, never
    // truncated into a different row revision.
    let overflow = core
        .delete_actor_knowledge_entry(
            &principal,
            env.character_id.clone(),
            entry_id.clone(),
            i64::MAX,
        )
        .await
        .unwrap_err();
    assert_invalid_input(&overflow);

    {
        let pool = plain_pool(&env).await;
        sqlx::query("DELETE FROM kb_source_anchors WHERE key_block_id = ?")
            .bind(&entry_id)
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }

    let deleted = core
        .delete_actor_knowledge_entry(
            &principal,
            env.character_id.clone(),
            entry_id.clone(),
            revision + 3,
        )
        .await;
    assert!(
        deleted.is_ok(),
        "delete admits without anchors: {deleted:?}"
    );
    let missing = core
        .actor_knowledge_entry(&principal, env.character_id.clone(), entry_id.clone())
        .await
        .unwrap_err();
    assert_not_found(&missing);
}

/// Migrated `actor_knowledge_api::world_create_with_summary_is_invalid_input`
/// and `::character_create_oversize_multibyte_summary_is_invalid_input`.
#[tokio::test]
async fn retained_knowledge_authoring_shape_guards() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;

    let world_summary = core
        .add_actor_knowledge_entry(
            &principal,
            serde_json::from_value(serde_json::json!({
                "owner_kind": "world",
                "world_id": WORLD,
                "block_type": "item",
                "canonical_name": "Nope",
                "summary": "forbidden",
            }))
            .unwrap(),
            true,
        )
        .await
        .unwrap_err();
    assert_invalid_input(&world_summary);

    // 65536 bytes of summary is the bound; one byte more is refused.
    let at_limit = format!("{}a", "字".repeat(21845));
    assert_eq!(at_limit.len(), 65536);
    let over = format!("{at_limit}b");
    for (summary, label) in [(at_limit, "at limit"), (over, "over limit")] {
        let result = core
            .add_actor_knowledge_entry(
                &principal,
                serde_json::from_value(serde_json::json!({
                    "owner_kind": "character",
                    "character_id": &env.character_id,
                    "block_type": "item",
                    "canonical_name": format!("Summary{label}"),
                    "summary": summary,
                }))
                .unwrap(),
                true,
            )
            .await;
        if label == "at limit" {
            assert!(result.is_ok(), "the byte bound admits exactly: {result:?}");
        } else {
            assert_invalid_input(&result.unwrap_err());
        }
    }
}

// ── Character ToM (theory of mind) record/list family ─────────────────────

fn tom_request(
    world_id: &str,
    binding_id: &str,
    carrier_entry_id: &str,
    expected_revision: u64,
    holder: &str,
    proposition: &str,
    order: u64,
) -> RecordCharacterTomRequest {
    serde_json::from_value(serde_json::json!({
        "world_id": world_id,
        "binding_id": binding_id,
        "carrier_entry_id": carrier_entry_id,
        "expected_revision": expected_revision,
        "holder": holder,
        "proposition": proposition,
        "order": order,
        "truth": "True",
        "access": "Private",
        "representation": "Explicit",
        "content_type": "Location",
        "source": "Perception",
        "context": "Neutral"
    }))
    .expect("ToM record request is wire-valid")
}

fn tom_query(
    world_id: &str,
    binding_id: &str,
    limit: Option<i64>,
    cursor: Option<&str>,
) -> ListCharacterTomQuery {
    let mut value = serde_json::json!({ "world_id": world_id, "binding_id": binding_id });
    if let Some(limit) = limit {
        value["limit"] = serde_json::json!(limit);
    }
    if let Some(cursor) = cursor {
        value["cursor"] = serde_json::json!(cursor);
    }
    serde_json::from_value(value).expect("ToM list query is wire-valid")
}

/// One Character-owned `ToM` carrier with the given `modules` payload.
async fn seed_carrier(
    env: &Env,
    character_id: &str,
    name: &str,
    modules: serde_json::Value,
) -> String {
    let pool = plain_pool(env).await;
    let store = SqliteKbStore::new(pool.clone());
    let mut row = KnowledgeEntryRecord::for_character(character_id, BlockType::Character, name);
    row.modules = Some(modules);
    let entry_id = row.entry_id.clone();
    store.insert_knowledge_entry(row).await.unwrap();
    pool.close().await;
    entry_id
}

/// One binding-owned `ToM` carrier (the unselected-binding shape).
async fn seed_binding_carrier(
    env: &Env,
    binding_id: &str,
    name: &str,
    modules: serde_json::Value,
) -> String {
    let pool = plain_pool(env).await;
    let store = SqliteKbStore::new(pool.clone());
    let mut row = KnowledgeEntryRecord::for_binding(binding_id, BlockType::Character, name);
    row.modules = Some(modules);
    let entry_id = row.entry_id.clone();
    store.insert_knowledge_entry(row).await.unwrap();
    pool.close().await;
    entry_id
}

async fn mind_state_count(env: &Env) -> i64 {
    let pool = plain_pool(env).await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mind_states")
        .fetch_one(&pool)
        .await
        .unwrap();
    pool.close().await;
    count
}

async fn carrier_modules_json(env: &Env, carrier_id: &str) -> String {
    let pool = plain_pool(env).await;
    let text: String =
        sqlx::query_scalar("SELECT modules_json FROM kb_key_blocks WHERE key_block_id = ?")
            .bind(carrier_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    pool.close().await;
    text
}

async fn carrier_revision(env: &Env, carrier_id: &str) -> i64 {
    let pool = plain_pool(env).await;
    let revision: Option<i64> =
        sqlx::query_scalar("SELECT revision FROM kb_key_blocks WHERE key_block_id = ?")
            .bind(carrier_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    pool.close().await;
    revision.unwrap_or(0)
}

async fn set_carrier_modules_text(env: &Env, carrier_id: &str, raw: &str) {
    let pool = plain_pool(env).await;
    sqlx::query("UPDATE kb_key_blocks SET modules_json = ? WHERE key_block_id = ?")
        .bind(raw)
        .bind(carrier_id)
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
}

async fn set_carrier_status(env: &Env, carrier_id: &str, status: &str) {
    let pool = plain_pool(env).await;
    sqlx::query("UPDATE kb_key_blocks SET status = ? WHERE key_block_id = ?")
        .bind(status)
        .bind(carrier_id)
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
}

async fn insert_derivative(env: &Env, mind_state_id: &str, carrier_id: &str, occurred_at: &str) {
    let pool = plain_pool(env).await;
    sqlx::query(
        "INSERT INTO mind_states \
         (mind_state_id, schema_version, holder_entry_id, canonical_name, occurred_at, \
          sort_key, snapshot_json, deltas_json, source_anchor_json, created_at, updated_at, \
          extensions_json) \
         VALUES (?, 1, ?, 'derivative', ?, '0001', '{}', '[]', NULL, ?, ?, '{\"nexus\":{}}')",
    )
    .bind(mind_state_id)
    .bind(carrier_id)
    .bind(occurred_at)
    .bind(occurred_at)
    .bind(occurred_at)
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;
}

fn orders(page: &ListCharacterTomResponse) -> Vec<i64> {
    page.items.iter().map(|row| row.order).collect()
}

fn ordinals(page: &ListCharacterTomResponse) -> Vec<u64> {
    page.items.iter().map(|row| row.row_ordinal).collect()
}

fn carrier_ids(page: &ListCharacterTomResponse) -> Vec<String> {
    page.items
        .iter()
        .map(|row| row.carrier_entry_id.clone())
        .collect()
}

/// Migrated `character_tom_api::record_l1_list_l1_before_l2_and_l2_subject_rules`
/// and `::record_and_list_succeed_without_agent_host`.
#[allow(clippy::too_many_lines)] // one ToM L1/L2 ordering journey asserted end to end
#[tokio::test]
async fn retained_tom_l1_l2_order_and_subject_rules() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let carrier = seed_carrier(
        &env,
        &env.character_id,
        "TomCarrier",
        serde_json::json!({"belief": []}),
    )
    .await;
    let subject = core
        .create_character(&principal, create_character_request("Ben", WORLD, None))
        .await
        .expect("subject Character");
    let subject_id = String::from(subject.character.character_id.clone());

    let l1 = core
        .record_character_tom(
            &principal,
            env.character_id.clone(),
            tom_request(
                WORLD,
                &env.binding_id,
                &carrier,
                0,
                &env.character_id,
                "I know the dock",
                1,
            ),
        )
        .await
        .expect("L1 records without any agent host");
    assert_eq!(l1.revision.get(), 1);

    let l2 = core
        .record_character_tom(
            &principal,
            env.character_id.clone(),
            tom_request(
                WORLD,
                &env.binding_id,
                &carrier,
                1,
                &subject_id,
                "Ben is cautious",
                2,
            ),
        )
        .await
        .expect("L2 about another active owned Character records");
    assert_eq!(l2.revision.get(), 2);

    let page = core
        .list_character_tom(
            &principal,
            env.character_id.clone(),
            tom_query(WORLD, &env.binding_id, None, None),
        )
        .await
        .expect("list");
    assert_eq!(orders(&page), vec![1, 2]);

    // An L1 belief must be the viewer's own.
    let l1_foreign = core
        .record_character_tom(
            &principal,
            env.character_id.clone(),
            tom_request(WORLD, &env.binding_id, &carrier, 2, &subject_id, "bad", 1),
        )
        .await
        .unwrap_err();
    assert_invalid_input(&l1_foreign);

    // An L2 belief must name a different Character than the viewer.
    let l2_self = core
        .record_character_tom(
            &principal,
            env.character_id.clone(),
            tom_request(
                WORLD,
                &env.binding_id,
                &carrier,
                2,
                &env.character_id,
                "bad",
                2,
            ),
        )
        .await
        .unwrap_err();
    assert_invalid_input(&l2_self);

    // An L2 subject outside the owned active scope is a not-found.
    let l2_unbound = core
        .record_character_tom(
            &principal,
            env.character_id.clone(),
            tom_request(
                WORLD,
                &env.binding_id,
                &carrier,
                2,
                "chr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "foreign",
                2,
            ),
        )
        .await
        .unwrap_err();
    assert_not_found(&l2_unbound);
    assert_eq!(
        carrier_revision(&env, &carrier).await,
        2,
        "refusals never bump the CAS"
    );
}

/// Migrated `character_tom_api::foreign_carrier_alias_and_stale_revision_fail_closed`
/// and `::inactive_viewer_world_and_subject_fail_closed`.
#[allow(clippy::too_many_lines)] // one ToM carrier liveness journey asserted end to end
#[tokio::test]
async fn retained_tom_carrier_and_liveness_fail_closed() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let carrier = seed_carrier(
        &env,
        &env.character_id,
        "TomCarrier",
        serde_json::json!({"belief": []}),
    )
    .await;
    let world_carrier = {
        let pool = plain_pool(&env).await;
        let store = SqliteKbStore::new(pool.clone());
        let mut row = KnowledgeEntryRecord::new(WORLD, BlockType::Character, "WorldOwned");
        row.modules = Some(serde_json::json!({ "belief": [] }));
        let entry_id = row.entry_id.clone();
        store.insert_knowledge_entry(row).await.unwrap();
        pool.close().await;
        entry_id
    };

    let before = mind_state_count(&env).await;
    // A World-owned entry can never be a Character ToM carrier.
    let world_owned = core
        .record_character_tom(
            &principal,
            env.character_id.clone(),
            tom_request(
                WORLD,
                &env.binding_id,
                &world_carrier,
                0,
                &env.character_id,
                "x",
                1,
            ),
        )
        .await
        .unwrap_err();
    assert_invalid_input(&world_owned);
    assert_eq!(
        mind_state_count(&env).await,
        before,
        "no derivative is written"
    );

    // A carrier owned by another Character is a not-found.
    let foreign_carrier = seed_carrier(
        &env,
        &env.foreign_character_id,
        "ForeignCarrier",
        serde_json::json!({"belief": []}),
    )
    .await;
    let foreign = core
        .record_character_tom(
            &principal,
            env.character_id.clone(),
            tom_request(
                WORLD,
                &env.binding_id,
                &foreign_carrier,
                0,
                &env.character_id,
                "x",
                1,
            ),
        )
        .await
        .unwrap_err();
    assert_not_found(&foreign);

    // A stale revision conflicts and leaves the CAS where it was.
    core.record_character_tom(
        &principal,
        env.character_id.clone(),
        tom_request(
            WORLD,
            &env.binding_id,
            &carrier,
            0,
            &env.character_id,
            "ok",
            1,
        ),
    )
    .await
    .expect("first record");
    let stale = core
        .record_character_tom(
            &principal,
            env.character_id.clone(),
            tom_request(
                WORLD,
                &env.binding_id,
                &carrier,
                0,
                &env.character_id,
                "stale",
                1,
            ),
        )
        .await
        .unwrap_err();
    assert_actor_conflict(&stale);
    assert_eq!(mind_state_count(&env).await, before + 1);

    // An archived viewer refuses the write but keeps the retained list read.
    set_character_status(&env, &env.character_id, "archived").await;
    let archived_viewer = core
        .record_character_tom(
            &principal,
            env.character_id.clone(),
            tom_request(
                WORLD,
                &env.binding_id,
                &carrier,
                1,
                &env.character_id,
                "x",
                1,
            ),
        )
        .await
        .unwrap_err();
    assert_conflict(archived_viewer, "character_inactive");
    let retained = core
        .list_character_tom(
            &principal,
            env.character_id.clone(),
            tom_query(WORLD, &env.binding_id, None, None),
        )
        .await
        .expect("an archived viewer still reads its history");
    assert_eq!(orders(&retained), vec![1]);
    set_character_status(&env, &env.character_id, "active").await;

    // A paused World refuses the write.
    set_world_status(&env, WORLD, "paused").await;
    let paused = core
        .record_character_tom(
            &principal,
            env.character_id.clone(),
            tom_request(
                WORLD,
                &env.binding_id,
                &carrier,
                1,
                &env.character_id,
                "x",
                1,
            ),
        )
        .await
        .unwrap_err();
    assert_conflict(paused, "world_inactive");
    set_world_status(&env, WORLD, "active").await;
    assert_eq!(mind_state_count(&env).await, before + 1);
}

/// Migrated `character_tom_api::malformed_modules_reject_without_rewrite_and_unknown_keys_survive`,
/// `::absent_belief_is_zero_rows_and_legacy_modules_round_trip`,
/// `::invalid_json_modules_fails_closed_and_never_overwritten` and
/// `::order_outside_closed_space_and_invalid_labels_reject`.
#[allow(clippy::too_many_lines)] // one ToM module-shape journey asserted end to end
#[tokio::test]
async fn retained_tom_module_shape_and_absent_belief() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;

    // Order outside the closed space is refused (labels are enum-typed on the
    // wire and covered by the nexus-knowledge validation unit cases).
    let order3 = seed_carrier(
        &env,
        &env.character_id,
        "OrderCarrier",
        serde_json::json!({"belief": []}),
    )
    .await;
    let before = mind_state_count(&env).await;
    let refused_order = core
        .record_character_tom(
            &principal,
            env.character_id.clone(),
            tom_request(
                WORLD,
                &env.binding_id,
                &order3,
                0,
                &env.character_id,
                "x",
                3,
            ),
        )
        .await
        .unwrap_err();
    assert_invalid_input(&refused_order);
    assert_eq!(mind_state_count(&env).await, before);
    assert_eq!(carrier_revision(&env, &order3).await, 0);

    // A non-object `modules` and a non-array `belief` reject without rewrite.
    let array_modules = seed_carrier(
        &env,
        &env.character_id,
        "ArrModules",
        serde_json::json!([1, 2, 3]),
    )
    .await;
    let refused = core
        .record_character_tom(
            &principal,
            env.character_id.clone(),
            tom_request(
                WORLD,
                &env.binding_id,
                &array_modules,
                0,
                &env.character_id,
                "x",
                1,
            ),
        )
        .await
        .unwrap_err();
    assert_actor_conflict(&refused);
    assert_eq!(
        carrier_modules_json(&env, &array_modules).await,
        "[1,2,3]",
        "a malformed carrier is never rewritten"
    );

    let object_belief = seed_carrier(
        &env,
        &env.character_id,
        "ObjBelief",
        serde_json::json!({"belief": {"legacy": true}}),
    )
    .await;
    let refused = core
        .record_character_tom(
            &principal,
            env.character_id.clone(),
            tom_request(
                WORLD,
                &env.binding_id,
                &object_belief,
                0,
                &env.character_id,
                "x",
                1,
            ),
        )
        .await
        .unwrap_err();
    assert_actor_conflict(&refused);
    assert_eq!(
        carrier_modules_json(&env, &object_belief).await,
        "{\"belief\":{\"legacy\":true}}",
        "a non-array belief member is never replaced with an empty array"
    );

    // Invalid persisted JSON is a distinguishable refusal that never
    // overwrites the bytes, on both record and list.
    let invalid = seed_carrier(
        &env,
        &env.character_id,
        "InvalidJson",
        serde_json::json!({"belief": []}),
    )
    .await;
    set_carrier_modules_text(&env, &invalid, "{\"belief\": [").await;
    let refused = core
        .record_character_tom(
            &principal,
            env.character_id.clone(),
            tom_request(
                WORLD,
                &env.binding_id,
                &invalid,
                0,
                &env.character_id,
                "x",
                1,
            ),
        )
        .await
        .unwrap_err();
    assert_conflict(refused, "carrier_modules_invalid_json");
    let listed = core
        .list_character_tom(
            &principal,
            env.character_id.clone(),
            tom_query(WORLD, &env.binding_id, None, None),
        )
        .await
        .unwrap_err();
    assert_conflict(listed, "carrier_modules_invalid_json");
    {
        let pool = plain_pool(&env).await;
        let text: String =
            sqlx::query_scalar("SELECT modules_json FROM kb_key_blocks WHERE key_block_id = ?")
                .bind(&invalid)
                .fetch_one(&pool)
                .await
                .unwrap();
        pool.close().await;
        assert_eq!(text, "{\"belief\": [", "invalid bytes are preserved");
    }

    // An absent `belief` member is zero rows, and unknown sibling module keys
    // round-trip verbatim through the CAS.
    let absent = seed_carrier(&env, &env.character_id, "NoBelief", serde_json::json!({})).await;
    core.record_character_tom(
        &principal,
        env.character_id.clone(),
        tom_request(
            WORLD,
            &env.binding_id,
            &absent,
            0,
            &env.character_id,
            "empty ok",
            1,
        ),
    )
    .await
    .expect("an absent belief member admits");

    let mixed = seed_carrier(
        &env,
        &env.character_id,
        "MixedModules",
        serde_json::json!({
            "belief": [],
            "mental": {"identity": {"role": "harbor_master"}},
            "x_custom": {"n": 1}
        }),
    )
    .await;
    core.record_character_tom(
        &principal,
        env.character_id.clone(),
        tom_request(
            WORLD,
            &env.binding_id,
            &mixed,
            0,
            &env.character_id,
            "mixed",
            1,
        ),
    )
    .await
    .expect("unknown sibling keys do not block a record");
    let stored: serde_json::Value = {
        let pool = plain_pool(&env).await;
        let text: String =
            sqlx::query_scalar("SELECT modules_json FROM kb_key_blocks WHERE key_block_id = ?")
                .bind(&mixed)
                .fetch_one(&pool)
                .await
                .unwrap();
        pool.close().await;
        serde_json::from_str(&text).unwrap()
    };
    assert_eq!(
        stored["mental"],
        serde_json::json!({"identity": {"role": "harbor_master"}})
    );
    assert_eq!(stored["x_custom"], serde_json::json!({"n": 1}));
    assert_eq!(stored["belief"].as_array().unwrap().len(), 1);
}

/// Migrated `character_tom_api::physical_row_ordinal_survives_malformed_elements_and_cursor_pages`,
/// `::corpus_and_row_caps_fail_closed_before_materialization`,
/// `::oversize_belief_array_rejects_via_db_probe_without_panic` and
/// `::record_rejects_201st_belief_row_without_mutation`.
#[allow(clippy::too_many_lines)] // one ToM ordinal/corpus cap journey asserted end to end
#[tokio::test]
async fn retained_tom_physical_ordinal_and_corpus_caps() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;

    let valid = |text: &str| {
        serde_json::json!({
            "holder": &env.character_id,
            "proposition": text,
            "order": 1,
            "truth": "True",
            "access": "Private",
            "representation": "Explicit",
            "content_type": "Location",
            "source": "Perception",
            "context": "Neutral"
        })
    };
    let carrier = seed_carrier(
        &env,
        &env.character_id,
        "OrdinalCarrier",
        serde_json::json!({"belief": [valid("first"), 42, valid("third")]}),
    )
    .await;

    let page1 = core
        .list_character_tom(
            &principal,
            env.character_id.clone(),
            tom_query(WORLD, &env.binding_id, Some(1), None),
        )
        .await
        .expect("first page");
    assert_eq!(ordinals(&page1), vec![0], "the physical ordinal is kept");
    assert!(page1.pagination.has_more);
    let cursor = page1.pagination.next_cursor.clone().expect("cursor");
    let page2 = core
        .list_character_tom(
            &principal,
            env.character_id.clone(),
            tom_query(WORLD, &env.binding_id, Some(1), Some(&cursor)),
        )
        .await
        .expect("second page");
    assert_eq!(
        ordinals(&page2),
        vec![2],
        "a malformed element does not renumber the next row"
    );
    assert_eq!(carrier_ids(&page2), vec![carrier.clone()]);
    assert!(!page2.pagination.has_more);

    // Corpus cap: more carriers than the fixed per-scope bound.
    let store = {
        let pool = plain_pool(&env).await;
        (SqliteKbStore::new(pool.clone()), pool)
    };
    for i in 0..=201 {
        let mut row = KnowledgeEntryRecord::for_character(
            &env.character_id,
            BlockType::Character,
            &format!("Bulk{i}"),
        );
        row.modules = Some(serde_json::json!({"belief": []}));
        store.0.insert_knowledge_entry(row).await.unwrap();
    }
    store.1.close().await;
    let capped = core
        .list_character_tom(
            &principal,
            env.character_id.clone(),
            tom_query(WORLD, &env.binding_id, None, None),
        )
        .await
        .unwrap_err();
    assert_conflict(capped, "view_incomplete");

    // Per-carrier row cap: a carrier already at the cap refuses the next
    // belief with zero mutation and stays listable.
    let env2 = seed_env().await;
    let (core2, principal2) = open_core(&env2).await;
    let rows: Vec<serde_json::Value> = (0..200)
        .map(|i| {
            serde_json::json!({
                "holder": &env2.character_id,
                "proposition": format!("seeded belief {i}"),
                "order": 1,
                "truth": "True",
                "access": "Private",
                "representation": "Explicit",
                "content_type": "Location",
                "source": "Perception",
                "context": "Neutral"
            })
        })
        .collect();
    let full = seed_carrier(
        &env2,
        &env2.character_id,
        "FullCarrier",
        serde_json::json!({ "belief": rows }),
    )
    .await;
    let before = mind_state_count(&env2).await;
    let refused = core2
        .record_character_tom(
            &principal2,
            env2.character_id.clone(),
            tom_request(
                WORLD,
                &env2.binding_id,
                &full,
                0,
                &env2.character_id,
                "201st",
                1,
            ),
        )
        .await
        .unwrap_err();
    assert_conflict(refused, "view_incomplete");
    assert_eq!(
        mind_state_count(&env2).await,
        before,
        "a refused record inserts no MindState"
    );
    let still_200: i64 = {
        let pool = plain_pool(&env2).await;
        let len: i64 = sqlx::query_scalar(
            "SELECT json_array_length(modules_json, '$.belief') FROM kb_key_blocks WHERE key_block_id = ?",
        )
        .bind(&full)
        .fetch_one(&pool)
        .await
        .unwrap();
        pool.close().await;
        len
    };
    assert_eq!(still_200, 200, "a refused record appends nothing");
    let listed = core2
        .list_character_tom(
            &principal2,
            env2.character_id.clone(),
            tom_query(WORLD, &env2.binding_id, Some(100), None),
        )
        .await
        .expect("the capped corpus stays listable");
    assert_eq!(listed.items.len(), 100);
    assert!(listed.pagination.has_more);
}

/// Migrated `character_tom_api::derivative_history_uses_one_grouped_row_per_carrier`,
/// `::stale_deleted_carrier_histories_do_not_surface_or_error` and
/// `::timestamp_lookup_is_scoped_to_selected_binding_admitted_ids`.
#[tokio::test]
async fn retained_tom_derivative_history_and_binding_scoped_timestamps() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let alive = seed_carrier(
        &env,
        &env.character_id,
        "AliveCarrier",
        serde_json::json!({"belief": []}),
    )
    .await;
    core.record_character_tom(
        &principal,
        env.character_id.clone(),
        tom_request(
            WORLD,
            &env.binding_id,
            &alive,
            0,
            &env.character_id,
            "one",
            1,
        ),
    )
    .await
    .expect("record");

    // Several derivative rows: only the latest `occurred_at` surfaces.
    insert_derivative(&env, "ms_old", &alive, "2099-01-01T00:00:00Z").await;
    insert_derivative(&env, "ms_latest", &alive, "2099-01-03T00:00:00Z").await;

    // A deleted carrier's large history never surfaces.
    let deleted = seed_carrier(
        &env,
        &env.character_id,
        "StaleCarrier",
        serde_json::json!({"belief": []}),
    )
    .await;
    for i in 0..50 {
        insert_derivative(
            &env,
            &format!("ms_stale_{i}"),
            &deleted,
            &format!("2098-01-{:02}T00:00:00Z", (i % 28) + 1),
        )
        .await;
    }
    set_carrier_status(&env, &deleted, "deleted").await;

    // A second, unselected binding of the same Character carries its own
    // history and must stay outside the selected snapshot.
    let second = core
        .add_binding(
            &principal,
            env.character_id.clone(),
            WORLD_B.to_string(),
            None,
        )
        .await
        .expect("second binding");
    let second_binding = String::from(second.binding.binding_id.clone());
    let unselected = seed_binding_carrier(
        &env,
        &second_binding,
        "UnselectedCarrier",
        serde_json::json!({"belief": []}),
    )
    .await;
    for i in 0..40 {
        insert_derivative(
            &env,
            &format!("ms_unsel_{i}"),
            &unselected,
            &format!("2097-01-{:02}T00:00:00Z", (i % 28) + 1),
        )
        .await;
    }

    let page = core
        .list_character_tom(
            &principal,
            env.character_id.clone(),
            tom_query(WORLD, &env.binding_id, None, None),
        )
        .await
        .expect("list");
    assert_eq!(carrier_ids(&page), vec![alive.clone()]);
    assert_eq!(page.items.len(), 1, "one grouped row per alive carrier");
    let recorded_at = page.items[0]
        .carrier_recorded_at
        .expect("the latest derivative timestamp surfaces");
    assert!(
        recorded_at.to_rfc3339().starts_with("2099-01-03T00:00:00"),
        "only the latest derivative is projected: {recorded_at}"
    );
}

/// Migrated `character_tom_api::extreme_expected_revision_rejects_without_panic_or_mutation`,
/// `::storage_failure_is_internal_not_not_found`,
/// `::summary_patch_does_not_clobber_tom_modules_on_carrier` and
/// `::tom_cas_bumps_revision_blocking_stale_summary_patch`.
#[allow(clippy::too_many_lines)] // one ToM revision-domain journey asserted end to end
#[tokio::test]
async fn retained_tom_revision_domain_storage_error_and_summary_interaction() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let carrier = seed_carrier(
        &env,
        &env.character_id,
        "TomCarrier",
        serde_json::json!({"belief": []}),
    )
    .await;
    {
        let pool = plain_pool(&env).await;
        sqlx::query("UPDATE kb_key_blocks SET body_json = ? WHERE key_block_id = ?")
            .bind(r#"{"summary":"carrier summary"}"#)
            .bind(&carrier)
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }

    // The u64 revision domain is refused, never truncated; i64::MAX is
    // refused by the CAS increment guard; i64::MAX - 1 cas-misses normally.
    for extreme in [u64::MAX, i64::MAX as u64] {
        let refused = core
            .record_character_tom(
                &principal,
                env.character_id.clone(),
                tom_request(
                    WORLD,
                    &env.binding_id,
                    &carrier,
                    extreme,
                    &env.character_id,
                    "x",
                    1,
                ),
            )
            .await
            .unwrap_err();
        assert_invalid_input(&refused);
    }
    let near_max = core
        .record_character_tom(
            &principal,
            env.character_id.clone(),
            tom_request(
                WORLD,
                &env.binding_id,
                &carrier,
                (i64::MAX - 1) as u64,
                &env.character_id,
                "x",
                1,
            ),
        )
        .await
        .unwrap_err();
    assert_actor_conflict(&near_max);
    assert_eq!(mind_state_count(&env).await, 0);
    assert_eq!(carrier_revision(&env, &carrier).await, 0);

    // A storage failure is an internal error, never a not-found.
    {
        let pool = plain_pool(&env).await;
        sqlx::query("DROP TABLE kb_key_blocks")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }
    let storage = core
        .record_character_tom(
            &principal,
            env.character_id.clone(),
            tom_request(
                WORLD,
                &env.binding_id,
                &carrier,
                0,
                &env.character_id,
                "x",
                1,
            ),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(storage, CoreError::Internal { .. }),
        "a storage failure must not masquerade as not-found, got {storage:?}"
    );

    // A summary edit never clobbers the ToM modules, and the ToM CAS bump
    // blocks a stale summary patch.
    let env2 = seed_env().await;
    let (core2, principal2) = open_core(&env2).await;
    let carrier2 = seed_carrier(
        &env2,
        &env2.character_id,
        "TomCarrier2",
        serde_json::json!({"belief": []}),
    )
    .await;
    core2
        .record_character_tom(
            &principal2,
            env2.character_id.clone(),
            tom_request(
                WORLD,
                &env2.binding_id,
                &carrier2,
                0,
                &env2.character_id,
                "one",
                1,
            ),
        )
        .await
        .expect("record");
    let patched = core2
        .patch_actor_knowledge_entry(
            &principal2,
            env2.character_id.clone(),
            carrier2.clone(),
            1,
            None,
            FieldPatch::Set("updated carrier summary"),
            None,
        )
        .await
        .expect("summary patch admits");
    assert_eq!(
        patched
            .body
            .as_ref()
            .and_then(|body| body.summary.as_deref()),
        Some("updated carrier summary")
    );
    assert_eq!(patched.revision, Some(2));
    let listed = core2
        .list_character_tom(
            &principal2,
            env2.character_id.clone(),
            tom_query(WORLD, &env2.binding_id, None, None),
        )
        .await
        .expect("list after summary edit");
    assert_eq!(
        listed.items.len(),
        1,
        "the belief row survives a summary edit"
    );

    let stale = core2
        .patch_actor_knowledge_entry(
            &principal2,
            env2.character_id.clone(),
            carrier2.clone(),
            0,
            None,
            FieldPatch::Set("late"),
            None,
        )
        .await
        .unwrap_err();
    assert_conflict(stale, "knowledge_revision_conflict");
}
