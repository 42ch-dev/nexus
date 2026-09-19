//! P2-T3 moment context, directive and inspection contract tests (core
//! service semantics): an owned World A paired with a Work bound to World B
//! is rejected **before** assembly (QC2-S-001), the Work-wins / World-
//! override directive precedence stays intact through the core family, and
//! the inspector / directive surfaces stay owner-scoped (a foreign scope
//! never leaks state).

use nexus_contracts::daemon_api::inspector::moment_directive_request::MomentDirectiveRequest;
use nexus_contracts::BlockType;
use nexus_core::{
    CoreAccess, CoreError, CoreOpenOptions, CoreService, LocalDirectiveStore,
    ReadOnlyDirectiveStore,
};
use nexus_local_db::writer_protocol::{init_engine_pool, GuardedPoolOptions};
use nexus_local_db::{
    create_character_with_initial_binding, ensure_creator_row, CreateCharacterParams, WorkRecord,
};
use nexus_moment_context_assembly::directive::DirectiveStore;
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
    db_path: PathBuf,
    character_id: String,
    binding_id: String,
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
    let created;
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
        created = Some(
            create_character_with_initial_binding(
                &pool,
                CreateCharacterParams {
                    owner_creator_id: CREATOR,
                    display_name: "Ada",
                    image_uri: None,
                    persona_json: "{}",
                    world_id: WORLD_A,
                    world_sheet_entry_id: None,
                },
            )
            .await
            .unwrap(),
        );
    }
    let created = created.expect("character seeded");
    Env {
        _tmp: tmp,
        user_home,
        db_path,
        character_id: created.character.character_id,
        binding_id: created.binding.binding_id,
    }
}

/// A plain pool for DB-state assertions.
async fn plain_pool(env: &Env) -> sqlx::SqlitePool {
    nexus_local_db::open_pool(&env.db_path).await.unwrap()
}

/// `(ttl_remaining, last_focused_event_id, status)` of the active row for a
/// scope — the read-only invariant columns.
async fn directive_row_state(
    env: &Env,
    world_id: &str,
) -> Option<(String, i64, Option<String>, String)> {
    let pool = plain_pool(env).await;
    let row: Option<(String, i64, Option<String>, String)> = sqlx::query_as(
        "SELECT directive_id, ttl_remaining, last_focused_event_id, status FROM moment_directives \
         WHERE scope_kind = 'world' AND scope_id = ? AND status = 'active'",
    )
    .bind(world_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    pool.close().await;
    row
}

async fn chapter_anchor_count(env: &Env) -> i64 {
    let pool = plain_pool(env).await;
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM moment_directive_chapter_anchors")
        .fetch_one(&pool)
        .await
        .unwrap();
    pool.close().await;
    n
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

/// v1.191 P1 T11 fixture: one row per governance class in `WORLD_A`, written
/// through one engine pool released before the core opens. Names are what the
/// inspector packet renders; the shared row's entry id is what a revision-bound
/// re-read is asserted on.
struct ViewFixture {
    shared_row: String,
    creator_private_row: String,
    character_private_row: String,
    character_shared_row: String,
    character_shared_entry_id: String,
    binding_row: String,
}

async fn seed_view_fixture(env: &Env) -> ViewFixture {
    use nexus_knowledge::world_kb::knowledge_entry::{
        KnowledgeEntryRecord, DISCLOSURE_OWNER_PRIVATE,
    };
    use nexus_knowledge::world_kb::KbStore as _;
    use nexus_local_db::kb_store::SqliteKbStore;

    let guarded = init_engine_pool(&env.db_path, CREATOR, GuardedPoolOptions::default())
        .await
        .unwrap();
    let pool = guarded.clone_pool();
    let store = SqliteKbStore::new(pool.clone());
    let creator_holder = nexus_local_db::creator_holder_entry_id(CREATOR);
    let character_holder = nexus_local_db::character_holder_entry_id(&env.character_id);

    let private = |row: &mut KnowledgeEntryRecord, holder: &str| {
        row.holder_entry_id = Some(holder.to_string());
        row.disclosure = Some(DISCLOSURE_OWNER_PRIVATE.to_string());
    };

    let shared = KnowledgeEntryRecord::new(WORLD_A, BlockType::Item, "WorldSharedRow");
    let mut creator_private =
        KnowledgeEntryRecord::new(WORLD_A, BlockType::Item, "CreatorPrivateRow");
    private(&mut creator_private, &creator_holder);
    let mut character_private =
        KnowledgeEntryRecord::new(WORLD_A, BlockType::Item, "CharacterPrivateRow");
    private(&mut character_private, &character_holder);
    let character_shared = KnowledgeEntryRecord::for_character(
        &env.character_id,
        BlockType::Item,
        "CharacterSharedRow",
    );
    let mut binding_row =
        KnowledgeEntryRecord::for_binding(&env.binding_id, BlockType::Item, "BindingLocalRow");
    private(&mut binding_row, &character_holder);

    let character_shared_entry_id = character_shared.entry_id.clone();
    for row in [
        shared.clone(),
        creator_private,
        character_private,
        character_shared,
        binding_row,
    ] {
        store.insert_knowledge_entry(row).await.unwrap();
    }
    // Release the engine writer before the core opens.
    drop(store);
    drop(pool);
    drop(guarded);

    ViewFixture {
        shared_row: shared.entry_id,
        creator_private_row: "CreatorPrivateRow".to_string(),
        character_private_row: "CharacterPrivateRow".to_string(),
        character_shared_row: "CharacterSharedRow".to_string(),
        character_shared_entry_id,
        binding_row: "BindingLocalRow".to_string(),
    }
}

fn dto<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> T {
    serde_json::from_value(value).expect("wire-valid request DTO")
}

fn directive_request(
    action: &str,
    kind: &str,
    id: &str,
    body: Option<&str>,
) -> MomentDirectiveRequest {
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
    assert!(
        !shown.contains(WORLD_DIRECTIVE_BODY),
        "no world leak: {shown}"
    );

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
    assert!(
        inherited.contains(WORLD_DIRECTIVE_BODY),
        "inherit: {inherited}"
    );

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
        .inspect_moment(&principal, dto(serde_json::json!({ "world_id": WORLD_A })))
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
    assert!(
        matches!(err, CoreError::ForbiddenReason { .. }),
        "got {err:?}"
    );
}

/// Read-only inspect invariant: with an active directive in scope, the packet
/// carries its status/metadata (`scope_id` + `ttl_remaining`) but **never** the
/// body (AC-I3), and inspecting leaves TTL, the scene anchor and the chapter
/// anchors untouched (the inspector is an observation surface).
#[tokio::test]
async fn inspect_is_read_only_and_never_renders_the_directive_body() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;

    // Distinctive TTL so the metadata assertion is unambiguous.
    core.moment_directive(
        &principal,
        dto(serde_json::json!({
            "action": "set",
            "scope": { "kind": "world", "id": WORLD_A },
            "body": WORLD_DIRECTIVE_BODY,
            "insert_depth": "head",
            "ttl_kind": "generations",
            "ttl_remaining": 4242,
            "clear_on_scene_change": true,
            "replace": true,
        })),
    )
    .await
    .expect("world directive");

    let before = directive_row_state(&env, WORLD_A)
        .await
        .expect("active directive row");
    let packet = core
        .inspect_moment(&principal, dto(serde_json::json!({ "world_id": WORLD_A })))
        .await
        .expect("inspect owned world");
    let packet = serde_json::to_string(&packet).unwrap();

    assert!(
        packet.contains(WORLD_A),
        "the active directive's scope is visible: {packet}"
    );
    assert!(
        packet.contains("4242"),
        "ttl_remaining metadata is visible: {packet}"
    );
    assert!(
        !packet.contains(WORLD_DIRECTIVE_BODY),
        "the directive body must never be on the wire: {packet}"
    );

    let after = directive_row_state(&env, WORLD_A)
        .await
        .expect("active directive row after inspect");
    assert_eq!(before, after, "inspection must not mutate directive state");
    assert_eq!(
        chapter_anchor_count(&env).await,
        0,
        "inspection must not write chapter anchors"
    );
}

/// The read-only store's write half is a no-op while the lifecycle store's is
/// not: identical `after_injection` input leaves the row untouched through
/// `ReadOnlyDirectiveStore` and burns TTL + re-anchors through
/// `LocalDirectiveStore` (differential regression for the inspector's
/// "no writes" contract).
#[tokio::test]
async fn read_only_directive_store_after_injection_is_a_noop() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;

    core.moment_directive(
        &principal,
        dto(serde_json::json!({
            "action": "set",
            "scope": { "kind": "world", "id": WORLD_A },
            "body": WORLD_DIRECTIVE_BODY,
            "insert_depth": "head",
            "ttl_kind": "generations",
            "ttl_remaining": 7,
            "clear_on_scene_change": false,
            "replace": true,
        })),
    )
    .await
    .expect("world directive");

    let before = directive_row_state(&env, WORLD_A)
        .await
        .expect("active directive row");
    let directive_id = before.0.clone();

    let pool = plain_pool(&env).await;
    let read_only = ReadOnlyDirectiveStore::new(pool.clone());

    // A read still resolves the active directive …
    let loaded = read_only
        .load_active(Some(CREATOR), None, Some(WORLD_A))
        .await
        .expect("read-only store resolves the active directive");
    assert_eq!(loaded.directive_id, directive_id);

    // … while the write half changes nothing.
    read_only
        .after_injection(&directive_id, Some("evt_new_scene"), Some(WORK_IN_B))
        .await;
    let after_noop = directive_row_state(&env, WORLD_A)
        .await
        .expect("active directive row after the read-only write half");
    assert_eq!(
        before, after_noop,
        "ReadOnlyDirectiveStore::after_injection must not mutate the row"
    );

    // Differential: the lifecycle store DOES burn TTL through the same call.
    LocalDirectiveStore::new(pool.clone())
        .after_injection(&directive_id, Some("evt_new_scene"), Some(WORK_IN_B))
        .await;
    let after_lifecycle = directive_row_state(&env, WORLD_A)
        .await
        .expect("active directive row after the lifecycle write half");
    pool.close().await;
    assert_eq!(
        after_lifecycle.1,
        before.1 - 1,
        "the lifecycle store burns TTL (proving the read-only no-op is meaningful)"
    );
    assert_ne!(after_noop, after_lifecycle);
}

/// The admitted-Actor projection carries the viewpoint anchors and the stored
/// Character epoch for a Character admission, and the null anchors for a
/// Creator admission.
#[tokio::test]
async fn actor_context_projects_viewpoint_and_epoch() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let chr = env.character_id.clone();
    let binding = env.binding_id.clone();

    let context = core
        .actor_context(
            &principal,
            &nexus_core::AdmittedActor::Character {
                character_id: chr.clone(),
            },
            &nexus_core::ActorViewpoint {
                world_id: WORLD_A.to_string(),
                binding_id: Some(binding.clone()),
                branch_id: None,
                event_id: None,
            },
        )
        .await
        .expect("character admission");
    let value = serde_json::to_value(&context).unwrap();
    assert_eq!(value["owner_creator_id"], CREATOR);
    assert_eq!(value["world_id"], WORLD_A);
    assert_eq!(value["binding_id"], binding);
    assert_eq!(value["branch_id"], serde_json::Value::Null);
    assert_eq!(value["event_id"], serde_json::Value::Null);
    assert_eq!(value["character_epoch"], 0);
    assert_eq!(value["actor_ref"]["actor_kind"], "character");
    assert_eq!(value["actor_ref"]["character_id"], chr);

    let creator_context = core
        .actor_context(
            &principal,
            &nexus_core::AdmittedActor::Creator {
                creator_id: CREATOR.to_string(),
            },
            &nexus_core::ActorViewpoint {
                world_id: WORLD_A.to_string(),
                binding_id: None,
                branch_id: None,
                event_id: None,
            },
        )
        .await
        .expect("creator admission");
    let value = serde_json::to_value(&creator_context).unwrap();
    assert_eq!(value["actor_ref"]["actor_kind"], "creator");
    assert_eq!(value["actor_ref"]["creator_id"], CREATOR);
    assert_eq!(value["binding_id"], serde_json::Value::Null);
    assert_eq!(value["character_epoch"], serde_json::Value::Null);
}

/// v1.191 P1 T11 (durable §4.1/§4.2/§4.3): the inspect/model consumer reads the
/// admitted ActorView snapshot only — the Creator's own private fact and every
/// Character-global shared row are in it, another holder's private World row
/// and a binding-local row are not, although the management selection over the
/// same World does hold the Character's private row. A material governance
/// change then advances the stored Character knowledge revision, so the
/// snapshot identity a cached session was keyed on no longer matches and the
/// next read serves the new governance instead of the stale rows.
#[tokio::test]
async fn v1191_holder_context_inspect_reads_actor_view_and_retires_a_stale_snapshot() {
    use nexus_knowledge::world_kb::knowledge_entry::KnowledgeAudience;
    use nexus_knowledge::world_kb::KnowledgeOwnerRef;
    use nexus_local_db::kb_store::SqliteKbStore;

    let env = seed_env().await;
    let fixture = seed_view_fixture(&env).await;
    let (core, principal) = open_core(&env).await;

    // ── The inspect packet is the admitted ActorView, not management ─────
    let packet = core
        .inspect_moment(&principal, dto(serde_json::json!({ "world_id": WORLD_A })))
        .await
        .expect("owned world inspect");
    let packet_json = serde_json::to_string(&packet).unwrap();

    assert!(
        packet_json.contains(&fixture.shared_row),
        "a Character-global shared row is in the ActorView: {packet_json}"
    );
    assert!(
        packet_json.contains(&fixture.creator_private_row),
        "the Creator's own holder row is in its own ActorView: {packet_json}"
    );
    assert!(
        !packet_json.contains(&fixture.character_private_row),
        "another holder's private World row must not reach the model input: {packet_json}"
    );
    assert!(
        !packet_json.contains(&fixture.binding_row),
        "a binding-local row must not reach the model input: {packet_json}"
    );

    // Differential: the Creator *management* selection over the same World does
    // hold the Character's private row — the exclusion above is the ActorView
    // policy, not an empty store. Management review is never model input.
    let pool = plain_pool(&env).await;
    let management = core
        .creator_management_read_scope(&principal, WORLD_A)
        .await
        .expect("management selection");
    let management_rows = SqliteKbStore::new(pool.clone())
        .list_by_owner_complete(&KnowledgeOwnerRef::world(WORLD_A), &management)
        .await
        .unwrap();
    assert!(
        management_rows
            .iter()
            .any(|row| row.canonical_name == fixture.character_private_row),
        "management review sees the owned Character's private row"
    );
    assert!(
        !management_rows
            .iter()
            .any(|row| row.canonical_name == fixture.binding_row),
        "the binding container is not a World-container row"
    );

    // ── Revision binding: a stale snapshot is retired, not reused ────────
    let chr = env.character_id.clone();
    let character_actor = nexus_core::AdmittedActor::Character {
        character_id: chr.clone(),
    };
    let viewpoint = nexus_core::ActorViewpoint {
        world_id: WORLD_A.to_string(),
        binding_id: Some(env.binding_id.clone()),
        branch_id: None,
        event_id: None,
    };

    let before = core
        .admit_actor_knowledge_view(&principal, &character_actor, viewpoint.clone())
        .await
        .expect("Character ActorView admission");
    let before_identity = before.identity();
    let before_rows = SqliteKbStore::new(pool.clone())
        .list_by_owner_complete(&KnowledgeOwnerRef::character(&chr), &before.scope())
        .await
        .unwrap();
    let shared_before = before_rows
        .iter()
        .find(|row| row.canonical_name == fixture.character_shared_row)
        .expect("the Character's shared row is in its own ActorView");
    assert_eq!(
        shared_before.disclosure, None,
        "a shared row carries no disclosure"
    );
    // Release the shared leases before the exclusive governance edit.
    drop(before);

    core.patch_actor_knowledge_entry(
        &principal,
        chr.clone(),
        fixture.character_shared_entry_id.clone(),
        i64::try_from(shared_before.revision.unwrap_or(0)).unwrap_or(0),
        None,
        nexus_local_db::FieldPatch::<&str>::Keep,
        Some(KnowledgeAudience::CharacterPrivate {
            character_id: chr.clone(),
        }),
    )
    .await
    .expect("author the Character-private audience");

    let after = core
        .admit_actor_knowledge_view(&principal, &character_actor, viewpoint)
        .await
        .expect("re-admission after the governance change");
    assert!(
        after.identity().character_revision() > before_identity.character_revision(),
        "a material governance change advances the Character knowledge revision \
         (the signal a cached session is retired on)"
    );
    assert_ne!(
        after.identity(),
        before_identity,
        "the session-key identity changes, so a snapshot cached under the old \
         identity is stale on next use"
    );

    let after_rows = SqliteKbStore::new(pool.clone())
        .list_by_owner_complete(&KnowledgeOwnerRef::character(&chr), &after.scope())
        .await
        .unwrap();
    let shared_after = after_rows
        .iter()
        .find(|row| row.canonical_name == fixture.character_shared_row)
        .expect("the Character still sees its own private row");
    assert_eq!(
        shared_after.disclosure.as_deref(),
        Some(nexus_knowledge::world_kb::knowledge_entry::DISCLOSURE_OWNER_PRIVATE),
        "the fresh read carries the new governance — no stale private text is reused"
    );

    // Once private to the Character, the row leaves the Creator's ActorView
    // (the model-input policy), while it stays in the Character's own.
    let creator_scope = core
        .creator_view_read_scope(&principal, WORLD_A)
        .await
        .expect("Creator ActorView selection");
    let creator_character_rows = SqliteKbStore::new(pool.clone())
        .list_by_owner_complete(&KnowledgeOwnerRef::character(&chr), &creator_scope)
        .await
        .unwrap();
    assert!(
        !creator_character_rows
            .iter()
            .any(|row| row.canonical_name == fixture.character_shared_row),
        "a Character-private fact is excluded from the Creator's model input"
    );

    // The Creator's own holder keeps its World rows readable (the exact-holder
    // half of ActorView), and the Character-global shared row stays visible.
    let creator_world_rows = SqliteKbStore::new(pool)
        .list_by_owner_complete(&KnowledgeOwnerRef::world(WORLD_A), &creator_scope)
        .await
        .unwrap();
    let names: Vec<&str> = creator_world_rows
        .iter()
        .map(|row| row.canonical_name.as_str())
        .collect();
    assert!(
        names.contains(&"WorldSharedRow"),
        "Character-global sharing stays visible: {names:?}"
    );
    assert!(
        names.contains(&fixture.creator_private_row.as_str()),
        "the Creator's own private World fact stays readable to its holder: {names:?}"
    );
    assert!(
        !names.contains(&fixture.character_private_row.as_str()),
        "another holder's private World row stays excluded: {names:?}"
    );
}
