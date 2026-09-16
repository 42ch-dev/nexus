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
    classify_pair, ActorViewpoint, AdmittedActor, CoreAccess, CoreActorAdmission, CoreError,
    CoreOpenOptions, CoreService,
};
use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;
use nexus_knowledge::world_kb::store::KbStore;
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
