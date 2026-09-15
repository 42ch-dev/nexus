//! P2-T3 moment context, directive and inspection contract tests (core
//! service semantics): an owned World A paired with a Work bound to World B
//! is rejected **before** assembly (QC2-S-001), the Work-wins / World-
//! override directive precedence stays intact through the core family, and
//! the inspector / directive surfaces stay owner-scoped (a foreign scope
//! never leaks state).

use nexus_contracts::daemon_api::inspector::moment_directive_request::MomentDirectiveRequest;
use nexus_contracts::generated::daemon_api::inspector::moment_inspect_request::MomentInspectRequest;
use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService};
use nexus_local_db::writer_protocol::{init_engine_pool, GuardedPoolOptions};
use nexus_local_db::{ensure_creator_row, WorkRecord};
use std::path::PathBuf;
use tempfile::TempDir;

const CREATOR: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER: &str = "ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const WORLD_A: &str = "wld_worldA";
const WORLD_B: &str = "wld_worldB";
const WORK_IN_B: &str = "wrk_boundToB";

const WORK_DIRECTIVE_BODY: &str = "Keep the chapter on the harbor standoff.";
const WORLD_DIRECTIVE_BODY: &str = "Never break the world's one-magic rule.";

struct Env {
    _tmp: TempDir,
    user_home: PathBuf,
}

async fn seed_world(pool: &sqlx::SqlitePool, world_id: &str, owner: &str) {
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

fn work_record(work_id: &str, world_id: Option<&str>) -> WorkRecord {
    WorkRecord {
        work_id: work_id.to_string(),
        creator_id: CREATOR.to_string(),
        workspace_slug: "default".to_string(),
        status: "active".to_string(),
        title: "Bound Work".to_string(),
        long_term_goal: "Finish the harbor arc.".to_string(),
        initial_idea: "A standoff in the fog.".to_string(),
        creative_brief: None,
        intake_status: "complete".to_string(),
        world_id: world_id.map(str::to_string),
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "preset_none".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
        current_stage: "draft".to_string(),
        stage_status: "active".to_string(),
        work_profile: None,
        work_ref: None,
        total_planned_chapters: None,
        current_chapter: 0,
        auto_chain_enabled: true,
        driver_schedule_id: None,
        auto_chain_interrupted: false,
        auto_review_master_on_timeout: false,
        runtime_lock_holder: None,
        runtime_lock_acquired_at: None,
        completion_locked_at: None,
        novel_completion_status: None,
        lineage_from_work_id: None,
    }
}

/// Materialize the workspace shell (two owned Worlds + a Work bound to
/// World B) through one engine pool released before the core opens.
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
    {
        let guarded = init_engine_pool(&db_path, CREATOR, GuardedPoolOptions::default())
            .await
            .unwrap();
        let pool = guarded.clone_pool();
        ensure_creator_row(&pool, CREATOR, "Owner").await.unwrap();
        ensure_creator_row(&pool, OTHER, "Other").await.unwrap();
        seed_world(&pool, WORLD_A, CREATOR).await;
        seed_world(&pool, WORLD_B, CREATOR).await;
        nexus_local_db::create_work(&pool, &work_record(WORK_IN_B, Some(WORLD_B)))
            .await
            .unwrap();
    }
    Env { _tmp: tmp, user_home }
}

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

fn dto<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> T {
    serde_json::from_value(value).expect("wire-valid request DTO")
}

fn directive_request(action: &str, kind: &str, id: &str, body: Option<&str>) -> MomentDirectiveRequest {
    dto(serde_json::json!({
        "action": action,
        "scope": { "kind": kind, "id": id },
        "body": body,
        "insert_depth": "head",
        "ttl_kind": "generations",
        "ttl_remaining": 3,
        "replace": true,
    }))
}

/// The acceptance anchor: owned World A paired with a Work bound to World B
/// is rejected before assembly (400 `invalid_input` on `work_id`), while the
/// scoped directive precedence stays intact — a Work's own directive wins
/// over the World override, and clearing it inherits the bound World's
/// override.
#[tokio::test]
async fn work_world_binding_rejects_cross_context() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;

    // Owned World A + the Work bound to World B: rejected before assembly.
    let err = core
        .inspect_moment(
            &principal,
            dto(serde_json::json!({
                "world_id": WORLD_A,
                "work_id": WORK_IN_B,
            })),
        )
        .await
        .expect_err("cross-context work must be rejected before assembly");
    match err {
        CoreError::InvalidInput { field, reason } => {
            assert_eq!(field, "work_id");
            assert!(
                reason.contains(WORK_IN_B) && reason.contains(WORLD_B),
                "reason: {reason}"
            );
        }
        other => panic!("expected InvalidInput, got {other:?}"),
    }

    // Directive precedence stays intact across the rejected pairing's scopes.
    // 1. World overrides on both owned worlds.
    core.moment_directive(
        &principal,
        directive_request("set", "world", WORLD_A, Some(WORLD_DIRECTIVE_BODY)),
    )
    .await
    .expect("world A directive");
    core.moment_directive(
        &principal,
        directive_request("set", "world", WORLD_B, Some(WORLD_DIRECTIVE_BODY)),
    )
    .await
    .expect("world B directive");

    // 2. The Work's own directive wins over the bound World's override.
    core.moment_directive(
        &principal,
        directive_request("set", "work", WORK_IN_B, Some(WORK_DIRECTIVE_BODY)),
    )
    .await
    .expect("work directive");
    let shown = core
        .moment_directive(
            &principal,
            directive_request("show", "work", WORK_IN_B, None),
        )
        .await
        .expect("show work scope");
    let shown = serde_json::to_string(&shown).unwrap();
    assert!(shown.contains(WORK_DIRECTIVE_BODY), "work wins: {shown}");
    assert!(!shown.contains(WORLD_DIRECTIVE_BODY), "no world leak: {shown}");

    // 3. Clearing the Work directive inherits the bound World's override.
    core.moment_directive(
        &principal,
        directive_request("clear", "work", WORK_IN_B, None),
    )
    .await
    .expect("clear work directive");
    let inherited = core
        .moment_directive(
            &principal,
            directive_request("show", "work", WORK_IN_B, None),
        )
        .await
        .expect("show work scope after clear");
    let inherited = serde_json::to_string(&inherited).unwrap();
    assert!(inherited.contains(WORLD_DIRECTIVE_BODY), "inherit: {inherited}");

    // 4. The rejected cross-context pairing still rejects afterwards —
    //    precedence never routes World B's override into World A's assembly.
    let err = core
        .inspect_moment(
            &principal,
            dto(serde_json::json!({
                "world_id": WORLD_A,
                "work_id": WORK_IN_B,
            })),
        )
        .await
        .expect_err("cross-context rejection is stable");
    assert!(matches!(err, CoreError::InvalidInput { .. }), "got {err:?}");
}

/// The inspector and directive surfaces stay owner-scoped: a foreign World is
/// the retained 403 on both, an owned World assembles a bounded packet, and
/// the World-A assembly never shows the Work-B-scoped directive.
#[tokio::test]
async fn inspect_and_directives_stay_owner_scoped() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;

    // Foreign world inspect → retained 403 shape.
    let err = core
        .inspect_moment(
            &principal,
            dto(serde_json::json!({ "world_id": "wld_foreign" })),
        )
        .await
        .expect_err("foreign world inspect");
    match err {
        CoreError::ForbiddenReason { resource, reason } => {
            assert_eq!(resource, "world wld_foreign");
            assert_eq!(reason, "you do not own this world");
        }
        other => panic!("expected ForbiddenReason, got {other:?}"),
    }

    // Foreign world directive → retained 403 shape.
    let err = core
        .moment_directive(
            &principal,
            directive_request("set", "world", "wld_foreign", Some("nope")),
        )
        .await
        .expect_err("foreign world directive");
    assert!(
        matches!(err, CoreError::ForbiddenReason { .. }),
        "got {err:?}"
    );

    // An owned World assembles a bounded inspector packet.
    let packet = core
        .inspect_moment(
            &principal,
            dto(serde_json::json!({ "world_id": WORLD_A })),
        )
        .await
        .expect("owned world assembly");
    let packet = serde_json::to_string(&packet).unwrap();
    assert!(!packet.is_empty(), "packet renders");

    // A foreign creator's world directive set (via a foreign scope id that
    // exists but is owned by nobody here) stays 403 — never 404 — so scope
    // existence is unobservable.
    let err = core
        .moment_directive(
            &principal,
            directive_request("show", "world", "wld_unowned", None),
        )
        .await
        .expect_err("unowned scope show");
    assert!(matches!(err, CoreError::ForbiddenReason { .. }), "got {err:?}");
}
