//! Work selection, authoring and selected-context isolation on guarded storage.
#![allow(clippy::too_many_lines)] // one end-to-end scenario per test
use nexus_contracts::{CreateWorkRequest, CreateWorldRequest, ListWorksQuery};
use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService, WorkPatchRequest};
use nexus_local_db::writer_protocol::init_guarded_pool;

fn select_creator(home: &std::path::Path, creator: &str, workspace: &str) {
    std::fs::write(home.join(".nexus42/config.toml"), format!("active_creator_id = \"{creator}\"\n[active_workspace_slug_by_creator]\n\"{creator}\" = \"{workspace}\"\n")).unwrap();
}

fn query(value: serde_json::Value) -> ListWorksQuery {
    serde_json::from_value(value).unwrap()
}

async fn create(
    core: &CoreService,
    principal: &nexus_core::Principal,
    world: &str,
    title: &str,
) -> String {
    let request: CreateWorkRequest = serde_json::from_value(serde_json::json!({
        "title": title, "long_term_goal": "write", "initial_idea": "idea", "world_id": world
    }))
    .unwrap();
    core.create_work(principal, request).await.unwrap().work_id
}

#[tokio::test]
async fn work_selection_invalidates_foreign_context() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    std::fs::create_dir_all(home.join(".nexus42")).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        home, "author", "default",
    ))
    .unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        home, "other", "default",
    ))
    .unwrap();
    select_creator(home, "author", "default");
    {
        let db = nexus_home_layout::workspace_state_db_path(home, "author", "default");
        let seed = init_guarded_pool(&db, "author").await.unwrap();
        let pool = seed.clone_pool();
        nexus_local_db::creators::ensure_creator_row(&pool, "author", "Author")
            .await
            .unwrap();
        pool.close().await;
    }
    let core = CoreService::open(CoreOpenOptions {
        user_home: home.into(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    let world = core
        .create_world(
            &principal,
            serde_json::from_value::<CreateWorldRequest>(serde_json::json!({"title": "World"}))
                .unwrap(),
        )
        .await
        .unwrap()
        .world_id;
    let first = create(&core, &principal, &world, "Alpha").await;
    let second = create(&core, &principal, &world, "Beta").await;
    let all = core
        .list_works(&principal, query(serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(
        all.items
            .iter()
            .map(|work| work.work_id.clone())
            .collect::<std::collections::HashSet<_>>(),
        [first.clone(), second.clone()].into_iter().collect()
    );
    assert_eq!(all.pagination.limit, 100);
    let selected = core.select_work(&principal, first.clone()).await.unwrap();
    assert!(selected.active);
    core.select_work(&principal, second.clone()).await.unwrap();
    assert!(matches!(
        core.select_work(&principal, "missing".into()).await,
        Err(CoreError::NotFound { .. })
    ));

    let db = nexus_home_layout::workspace_state_db_path(home, "author", "default");
    let reader = nexus_local_db::open_pool_read_only(&db).await.unwrap();
    assert_eq!(
        nexus_local_db::novel_pool_entries::get_active_pool_entry(&reader, "author")
            .await
            .unwrap()
            .unwrap()
            .work_id
            .as_deref(),
        Some(second.as_str())
    );
    assert_eq!(
        nexus_local_db::novel_pool_entries::get_pool_entry_by_work(&reader, "author", &first)
            .await
            .unwrap()
            .unwrap()
            .status,
        "queued"
    );

    // A foreign Creator's row in the same database must not become selectable.
    let guarded = init_guarded_pool(&db, "author").await.unwrap();
    let pool = guarded.clone_pool();
    nexus_local_db::creators::ensure_creator_row(&pool, "other", "Other")
        .await
        .unwrap();
    sqlx::query("UPDATE works SET creator_id = 'other' WHERE work_id = ?")
        .bind(&first)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        core.select_work(&principal, first.clone()).await,
        Err(CoreError::NotFound { .. })
    ));
    assert_eq!(
        nexus_local_db::novel_pool_entries::get_active_pool_entry(&reader, "author")
            .await
            .unwrap()
            .unwrap()
            .work_id
            .as_deref(),
        Some(second.as_str())
    );
    let listed = core
        .list_works(&principal, query(serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(
        listed
            .items
            .iter()
            .map(|work| &work.work_id)
            .collect::<Vec<_>>(),
        vec![&second]
    );

    // Core-owned nullable patch keeps absence / clear / set distinct.
    core.patch_work(
        &principal,
        second.clone(),
        "core",
        WorkPatchRequest {
            story_ref: Some(Some("story".into())),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    core.patch_work(
        &principal,
        second.clone(),
        "core",
        WorkPatchRequest {
            title: Some("Changed".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        core.get_work(&principal, second.clone())
            .await
            .unwrap()
            .story_ref
            .as_deref(),
        Some("story")
    );
    core.patch_work(
        &principal,
        second.clone(),
        "core",
        WorkPatchRequest {
            story_ref: Some(None),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        core.get_work(&principal, second.clone())
            .await
            .unwrap()
            .story_ref,
        None
    );
    assert!(matches!(
        core.patch_work(
            &principal,
            second.clone(),
            "core",
            WorkPatchRequest {
                world_id: Some(None),
                ..Default::default()
            }
        )
        .await,
        Err(CoreError::InvalidInput { .. })
    ));
    assert_eq!(
        core.get_work(&principal, second.clone())
            .await
            .unwrap()
            .world_id
            .as_deref(),
        Some(world.as_str())
    );
    assert_eq!(
        core.get_work(&principal, second.clone())
            .await
            .unwrap()
            .runtime_lock_holder,
        None
    );
    assert!(matches!(
        core.patch_work(
            &principal,
            second.clone(),
            "core",
            WorkPatchRequest {
                stage_status: Some("complete".into()),
                title: Some("Rejected".into()),
                ..Default::default()
            }
        )
        .await,
        Err(CoreError::InvalidInput { .. })
    ));
    let unchanged = core.get_work(&principal, second.clone()).await.unwrap();
    assert_eq!(unchanged.title, "Changed");
    assert_eq!(unchanged.runtime_lock_holder, None);

    let lock_path = home.join("creative/Works/book/.completion-lock.json");
    std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
    std::fs::write(&lock_path, "{}").unwrap();
    std::fs::write(
        nexus_home_layout::operational_workspace_dir(home, "author", "default").join("meta.json"),
        serde_json::to_vec(&serde_json::json!({"local_root": home.join("creative")})).unwrap(),
    )
    .unwrap();
    sqlx::query("UPDATE works SET work_ref = 'book' WHERE work_id = ?")
        .bind(&second)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE works SET completion_locked_at = '2026-09-15', novel_completion_status = 'completed', total_planned_chapters = 3 WHERE work_id = ?").bind(&second).execute(&pool).await.unwrap();
    let expected_lock = format!("work_conflict:work {second} is completion-locked since 2026-09-15; use 'creator works completion-lock release' first");
    assert!(
        matches!(core.patch_work(&principal, second.clone(), "core", WorkPatchRequest::default()).await, Err(CoreError::Forbidden { resource }) if resource == expected_lock)
    );
    assert!(
        matches!(core.delete_work(&principal, second.clone(), "core").await, Err(CoreError::Forbidden { resource }) if resource == expected_lock)
    );
    let released = core
        .release_work_completion_lock(
            &principal,
            second.clone(),
            nexus_contracts::ReleaseCompletionLockRequest {
                reason: "continue writing".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(released.completion_locked_at, None);
    assert_eq!(
        released.novel_completion_status.as_deref(),
        Some("reopened")
    );
    assert_eq!(released.total_planned_chapters, Some(3));
    assert!(!lock_path.exists());

    sqlx::query("UPDATE works SET runtime_lock_holder = 'driver', runtime_lock_acquired_at = ? WHERE work_id = ?").bind(chrono::Utc::now().to_rfc3339()).bind(&second).execute(&pool).await.unwrap();
    assert!(
        matches!(core.patch_work(&principal, second.clone(), "core", WorkPatchRequest::default()).await,
        Err(CoreError::Forbidden { resource }) if resource == format!("work_locked:work {second} is locked by 'driver'; wait for release or check 'creator works status'"))
    );
    sqlx::query("UPDATE works SET runtime_lock_holder = NULL, runtime_lock_acquired_at = NULL WHERE work_id = ?").bind(&second).execute(&pool).await.unwrap();

    let inspiration = core
        .add_work_inspiration(
            &principal,
            nexus_core::AddInspirationRequest {
                title: "A new story".into(),
            },
        )
        .await
        .unwrap();
    assert!(
        nexus_home_layout::operational_workspace_dir(home, "author", "default")
            .join(&inspiration.rel_path)
            .is_file()
    );
    let promoted = core
        .promote_work_inspiration(
            &principal,
            nexus_core::PromoteInspirationRequest {
                item_id: inspiration.item_id.clone(),
                idea: None,
                set_default: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        core.get_work(&principal, promoted.work_id.clone())
            .await
            .unwrap()
            .status,
        "draft"
    );
    let archived = core
        .archive_work_pool_entry(
            &principal,
            nexus_core::ArchivePoolRequest {
                entry_id: promoted.pool_entry_id,
            },
        )
        .await
        .unwrap();
    assert_eq!(archived.status, "archived");
    assert_eq!(
        core.archive_work_inspiration(
            &principal,
            nexus_core::ArchiveInspirationRequest {
                item_id: inspiration.item_id
            }
        )
        .await
        .unwrap()
        .status,
        "archived"
    );
    let creative_root = home.join("creative");
    std::fs::create_dir_all(creative_root.join("Works")).unwrap();
    std::fs::create_dir_all(creative_root.join("outside")).unwrap();
    std::fs::write(creative_root.join("outside/keep"), "must survive").unwrap();
    std::fs::write(
        nexus_home_layout::operational_workspace_dir(home, "author", "default").join("meta.json"),
        serde_json::to_vec(&serde_json::json!({"local_root": creative_root})).unwrap(),
    )
    .unwrap();
    sqlx::query("UPDATE works SET work_ref = '../outside' WHERE work_id = ?")
        .bind(&promoted.work_id)
        .execute(&pool)
        .await
        .unwrap();
    core.delete_work(&principal, promoted.work_id.clone(), "core")
        .await
        .unwrap();
    assert!(matches!(
        core.get_work(&principal, promoted.work_id).await,
        Err(CoreError::NotFound { .. })
    ));
    assert_eq!(
        std::fs::read_to_string(creative_root.join("outside/keep")).unwrap(),
        "must survive"
    );

    assert!(
        matches!(core.set_work_pool_active(&principal, nexus_core::SetPoolActiveRequest {
        action: "other".into(), work_id: "missing".into(), creator_id: None,
    }).await, Err(CoreError::InvalidInput { field, .. }) if field == "invalid_action")
    );

    for cursor in ["v1:4294967296", "v1:-1", "invalid"] {
        assert!(matches!(
            core.list_works(&principal, query(serde_json::json!({"cursor":cursor})))
                .await,
            Err(CoreError::InvalidInput { .. })
        ));
    }
    let bounded = core
        .list_works(
            &principal,
            query(serde_json::json!({"limit":501,"sort":" -title, ,status "})),
        )
        .await
        .unwrap();
    assert_eq!(bounded.pagination.limit, 500);
    assert!(matches!(
        core.list_works(&principal, query(serde_json::json!({"sort":"bad"})))
            .await,
        Err(CoreError::InvalidInput { .. })
    ));
    assert_eq!(
        core.list_works(&principal, query(serde_json::json!({"limit":-1})))
            .await
            .unwrap()
            .pagination
            .limit,
        100
    );
    assert_eq!(
        core.list_works(
            &principal,
            query(serde_json::json!({"limit":4_294_967_296_i64}))
        )
        .await
        .unwrap()
        .pagination
        .limit,
        100
    );
    assert_eq!(
        core.list_works(&principal, query(serde_json::json!({"limit":0})))
            .await
            .unwrap()
            .pagination
            .limit,
        0
    );
    assert!(core
        .list_works(
            &principal,
            query(serde_json::json!({"cursor":"v1:4294967295"}))
        )
        .await
        .unwrap()
        .items
        .is_empty());

    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        home, "author", "second",
    ))
    .unwrap();
    select_creator(home, "author", "second");
    assert!(matches!(
        core.get_work(&principal, second.clone()).await,
        Err(CoreError::AuthRequired)
    ));
    let switched = CoreService::open(CoreOpenOptions {
        user_home: home.into(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .unwrap();
    let switched_principal = switched.active_principal().await.unwrap();
    assert!(switched
        .list_works(&switched_principal, query(serde_json::json!({})))
        .await
        .unwrap()
        .items
        .is_empty());
    assert!(matches!(
        switched
            .select_work(&switched_principal, second.clone())
            .await,
        Err(CoreError::NotFound { .. })
    ));
    switched.close().await.unwrap();

    // A stored workspace switch invalidates every operation on the old opened pool.
    select_creator(home, "other", "default");
    assert!(matches!(
        core.list_works(&principal, query(serde_json::json!({})))
            .await,
        Err(CoreError::AuthRequired)
    ));
    assert!(matches!(
        core.select_work(&principal, second).await,
        Err(CoreError::AuthRequired)
    ));
    assert!(matches!(
        core.active_principal().await,
        Err(CoreError::AuthRequired)
    ));
    let reopened = CoreService::open(CoreOpenOptions {
        user_home: home.into(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .unwrap();
    let other = reopened.active_principal().await.unwrap();
    assert!(reopened
        .list_works(&other, query(serde_json::json!({})))
        .await
        .unwrap()
        .items
        .is_empty());
    reopened.close().await.unwrap();
    reader.close().await;
    pool.close().await;
    core.close().await.unwrap();
}

/// Equivalence with the pre-extraction daemon `api::pagination` / `api::sort` grammar,
/// exercised through the public Work list path. Mirrors the daemon helpers' own unit
/// tables (`pagination.rs`, `sort.rs`): absent cursor → offset 0, `v1:`-prefixed
/// non-negative u32 accepted, anything else rejected with the verbatim `invalid_input`
/// message; absent/empty/whitespace sort → default ordering, trimming and empty comma
/// terms ignored, `-key` descending, unknown keys rejected with the verbatim
/// `<resource>_sort_invalid` message.
#[tokio::test]
async fn work_list_grammar_matches_legacy_daemon_contract() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        home, "author", "default",
    ))
    .unwrap();
    select_creator(home, "author", "default");
    {
        let db = nexus_home_layout::workspace_state_db_path(home, "author", "default");
        let seed = init_guarded_pool(&db, "author").await.unwrap();
        let pool = seed.clone_pool();
        nexus_local_db::creators::ensure_creator_row(&pool, "author", "Author")
            .await
            .unwrap();
        pool.close().await;
    }
    let core = CoreService::open(CoreOpenOptions {
        user_home: home.into(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();

    // Cursor parity: absent and `v1:0` decode to offset 0.
    assert_eq!(
        core.list_works(&principal, query(serde_json::json!({})))
            .await
            .unwrap()
            .pagination
            .limit,
        100
    );
    assert_eq!(
        core.list_works(&principal, query(serde_json::json!({"cursor": "v1:0"})))
            .await
            .unwrap()
            .pagination
            .limit,
        100
    );
    let cursor_message = "invalid pagination cursor; pass the `next_cursor` value returned by the previous response unchanged";
    for cursor in ["invalid", "v1:", "v1:x", "v1:-1", "v1:4294967296"] {
        let Err(CoreError::InvalidInput { field, reason }) = core
            .list_works(&principal, query(serde_json::json!({"cursor": cursor})))
            .await
        else {
            panic!("cursor '{cursor}' must be rejected");
        };
        assert_eq!(
            (field.as_str(), reason.as_str()),
            ("invalid_input", cursor_message)
        );
    }

    // Sort parity: unknown keys carry the resource-specific code and verbatim message.
    let Err(CoreError::InvalidInput { field, reason }) = core
        .list_works(&principal, query(serde_json::json!({"sort": "bad"})))
        .await
    else {
        panic!("unknown sort key must be rejected");
    };
    assert_eq!(
        (field.as_str(), reason.as_str()),
        (
            "work_sort_invalid",
            "unsupported sort key 'bad'; allowed: updated_at, title, status, intake_status"
        )
    );
    let Err(CoreError::InvalidInput { field, reason }) = core
        .list_works(&principal, query(serde_json::json!({"sort": "-"})))
        .await
    else {
        panic!("lone '-' must be rejected");
    };
    assert_eq!(
        (field.as_str(), reason.as_str()),
        (
            "work_sort_invalid",
            "unsupported sort key ''; allowed: updated_at, title, status, intake_status"
        )
    );
    // Empty, whitespace-only, and well-formed multi-key inputs fall back to the
    // default ordering instead of erroring.
    for sort in ["", "   ", ",,status,,-title,,"] {
        assert!(
            core.list_works(&principal, query(serde_json::json!({"sort": sort})))
                .await
                .is_ok(),
            "sort '{sort}' must be accepted"
        );
    }
    core.close().await.unwrap();
}

/// The legacy `DATABASE_ERROR` classification must survive the core error carrier
/// instead of collapsing into an uncoded internal error (fix-round 1, finding 3).
#[tokio::test]
async fn internal_database_fault_carries_legacy_code() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        home, "author", "default",
    ))
    .unwrap();
    select_creator(home, "author", "default");
    let db = nexus_home_layout::workspace_state_db_path(home, "author", "default");
    let seed = init_guarded_pool(&db, "author").await.unwrap();
    let pool = seed.clone_pool();
    nexus_local_db::creators::ensure_creator_row(&pool, "author", "Author")
        .await
        .unwrap();
    let core = CoreService::open(CoreOpenOptions {
        user_home: home.into(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    let world = core
        .create_world(
            &principal,
            serde_json::from_value::<CreateWorldRequest>(serde_json::json!({"title": "World"}))
                .unwrap(),
        )
        .await
        .unwrap()
        .world_id;
    pool.close().await;
    // Idempotent migrations only create missing versions — a dropped table stays gone.
    // Drop only `works` while the core is open so the world check passes and the
    // INSERT is what fails, surfacing the legacy DATABASE_ERROR classification.
    let dropper = init_guarded_pool(&db, "author").await.unwrap();
    let drop_pool = dropper.clone_pool();
    sqlx::query("DROP TABLE works")
        .execute(&drop_pool)
        .await
        .unwrap();
    drop(drop_pool);
    drop(dropper);
    let request: CreateWorkRequest = serde_json::from_value(serde_json::json!({
        "title": "Broken", "long_term_goal": "write", "initial_idea": "idea", "world_id": world
    }))
    .unwrap();
    let Err(CoreError::Internal { category }) = core.create_work(&principal, request).await else {
        panic!("dropped storage tables must surface as CoreError::Internal");
    };
    assert!(
        category.starts_with("DATABASE_ERROR: "),
        "legacy classification must ride the carrier, got: {category}"
    );
    core.close().await.unwrap();
}

/// Wording-independent lock-conflict assertion (fix-round 1, finding 1): a
/// refused mutation must report the `work_locked` class for this Work and name
/// the holder that actually holds it. No holder spelling is a contract.
fn assert_lock_conflict(resource: &str, work_id: &str, holder: &str) {
    assert!(
        resource.starts_with("work_locked:"),
        "lock conflict class: {resource}"
    );
    assert!(
        resource.contains(work_id),
        "lock conflict must name the Work: {resource}"
    );
    assert!(
        resource.contains(holder),
        "lock conflict must name the current holder: {resource}"
    );
}

/// Fix-round regression (fix 1, finding 1): a Work held by a runtime lock
/// refuses every lifecycle mutator as a lock conflict — whatever holder label
/// the holder and the contender carry — and leaves the durable Work row
/// untouched; releasing the holder restores writability.
#[tokio::test]
async fn work_lifecycle_locks_refuse_conflicting_holder() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    std::fs::create_dir_all(home.join(".nexus42")).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        home, "author", "default",
    ))
    .unwrap();
    select_creator(home, "author", "default");
    let db = nexus_home_layout::workspace_state_db_path(home, "author", "default");
    {
        let seed = init_guarded_pool(&db, "author").await.unwrap();
        let pool = seed.clone_pool();
        nexus_local_db::creators::ensure_creator_row(&pool, "author", "Author")
            .await
            .unwrap();
        pool.close().await;
    }
    let core = CoreService::open(CoreOpenOptions {
        user_home: home.into(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    let world = core
        .create_world(
            &principal,
            serde_json::from_value::<CreateWorldRequest>(serde_json::json!({"title": "World"}))
                .unwrap(),
        )
        .await
        .unwrap()
        .world_id;
    let work_id = create(&core, &principal, &world, "Locked Work").await;
    // Reconcile resolves story_ref and the creative root BEFORE its lock
    // phase — seed both so the mutator actually reaches lock acquire.
    core.patch_work(
        &principal,
        work_id.clone(),
        "core",
        WorkPatchRequest {
            story_ref: Some(Some("locked-work".into())),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    std::fs::create_dir_all(home.join("creative/Works/locked-work")).unwrap();
    std::fs::write(
        nexus_home_layout::operational_workspace_dir(home, "author", "default").join("meta.json"),
        serde_json::to_vec(&serde_json::json!({"local_root": home.join("creative")})).unwrap(),
    )
    .unwrap();
    // First lock attempt wins; a later attempt under a different holder label
    // is refused as a lock conflict naming the holder that already owns the
    // Work. Neither label is asserted — only the conflict class is.
    let lock_seed = init_guarded_pool(&db, "author").await.unwrap();
    let lock_pool = lock_seed.clone_pool();
    let holder = nexus_local_db::cli_holder("core");
    assert!(matches!(
        nexus_local_db::acquire_runtime_lock(
            &lock_pool,
            principal.creator_id(),
            &work_id,
            &holder,
            nexus_local_db::ttl_from_env(),
            false
        )
        .await
        .unwrap(),
        nexus_local_db::AcquireResult::Acquired { .. }
    ));
    let contender = nexus_local_db::cli_holder("http");
    match nexus_local_db::acquire_runtime_lock(
        &lock_pool,
        principal.creator_id(),
        &work_id,
        &contender,
        nexus_local_db::ttl_from_env(),
        false,
    )
    .await
    .unwrap()
    {
        nexus_local_db::AcquireResult::Locked {
            holder: existing, ..
        } => assert_eq!(existing, holder, "conflict must name the current holder"),
        nexus_local_db::AcquireResult::Acquired { .. } => {
            panic!("a second lock attempt must be refused as a lock conflict");
        }
    }

    let Err(CoreError::Forbidden { resource }) = core
        .patch_work(
            &principal,
            work_id.clone(),
            "http",
            WorkPatchRequest::default(),
        )
        .await
    else {
        panic!("locked Work must reject patch");
    };
    assert_lock_conflict(&resource, &work_id, &holder);
    let Err(CoreError::Forbidden { resource }) = core
        .append_work_inspiration(
            &principal,
            work_id.clone(),
            "http",
            nexus_contracts::AppendInspirationRequest {
                note: "blocked".into(),
            },
        )
        .await
    else {
        panic!("locked Work must reject inspiration append");
    };
    assert_lock_conflict(&resource, &work_id, &holder);
    let Err(CoreError::Forbidden { resource }) =
        core.delete_work(&principal, work_id.clone(), "http").await
    else {
        panic!("locked Work must reject delete");
    };
    assert_lock_conflict(&resource, &work_id, &holder);
    let Err(CoreError::Forbidden { resource }) = core
        .reconcile_work_chapters(
            &principal,
            work_id.clone(),
            "http",
            nexus_core::ReconcileDryRunQuery { dry_run: None },
        )
        .await
    else {
        panic!("locked Work must reject reconcile");
    };
    assert_lock_conflict(&resource, &work_id, &holder);

    // Refusals must leave the durable row consistent: the seeded fields
    // survive and the holder is still the one that acquired the lock.
    let unchanged = core.get_work(&principal, work_id.clone()).await.unwrap();
    assert_eq!(unchanged.title, "Locked Work");
    assert_eq!(unchanged.story_ref.as_deref(), Some("locked-work"));
    assert!(unchanged.inspiration_log.is_empty());
    assert_eq!(
        unchanged.runtime_lock_holder.as_deref(),
        Some(holder.as_str())
    );

    // Releasing the holder restores writability, and the mutator's own holder
    // is gone once it returns.
    assert!(nexus_local_db::release_runtime_lock(
        &lock_pool,
        principal.creator_id(),
        &work_id,
        &holder,
    )
    .await
    .unwrap());
    let unlocked = core
        .patch_work(
            &principal,
            work_id.clone(),
            "http",
            WorkPatchRequest {
                title: Some("Unlocked Work".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(unlocked.title, "Unlocked Work");
    assert_eq!(
        core.get_work(&principal, work_id.clone())
            .await
            .unwrap()
            .runtime_lock_holder,
        None,
        "the mutator's own lock must be released"
    );

    lock_pool.close().await;
    drop(lock_seed);
    core.close().await.unwrap();
}

/// Every Work lifecycle mutation the retained host-tool executors delegate to
/// carries the work-write gate: read-only core access is rejected with the
/// shared `work: read-only core access` Forbidden resource (QC1-F-001
/// read-only denial, verified at the authority the executors now call).
#[tokio::test]
async fn work_lifecycle_mutators_reject_read_only_core() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    std::fs::create_dir_all(home.join(".nexus42")).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        home, "author", "default",
    ))
    .unwrap();
    select_creator(home, "author", "default");
    let db = nexus_home_layout::workspace_state_db_path(home, "author", "default");
    {
        let seed = init_guarded_pool(&db, "author").await.unwrap();
        let pool = seed.clone_pool();
        nexus_local_db::creators::ensure_creator_row(&pool, "author", "Author")
            .await
            .unwrap();
        pool.close().await;
    }
    let writer = CoreService::open(CoreOpenOptions {
        user_home: home.into(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .unwrap();
    let writer_principal = writer.active_principal().await.unwrap();
    let world = writer
        .create_world(
            &writer_principal,
            serde_json::from_value::<CreateWorldRequest>(serde_json::json!({"title": "World"}))
                .unwrap(),
        )
        .await
        .unwrap()
        .world_id;
    let work_id = create(&writer, &writer_principal, &world, "Read Only").await;
    let finding = writer
        .create_finding(
            &writer_principal,
            work_id.clone(),
            nexus_core::CreateFindingRequest {
                chapter: None,
                severity: "minor".into(),
                title: "t".into(),
                description: String::new(),
                target_executor: "none".into(),
                kind: "craft".into(),
                rule_suggestion: None,
            },
        )
        .await
        .unwrap();
    writer.close().await.unwrap();

    let core = CoreService::open(CoreOpenOptions {
        user_home: home.into(),
        access: CoreAccess::ReadOnly,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    assert!(
        matches!(core.patch_work(&principal, work_id.clone(), "http", WorkPatchRequest::default()).await, Err(CoreError::Forbidden { resource }) if resource == "work: read-only core access")
    );
    assert!(
        matches!(core.append_work_inspiration(&principal, work_id.clone(), "http", nexus_contracts::AppendInspirationRequest { note: "n".into() }).await, Err(CoreError::Forbidden { resource }) if resource == "work: read-only core access")
    );
    assert!(
        matches!(core.delete_work(&principal, work_id.clone(), "http").await, Err(CoreError::Forbidden { resource }) if resource == "work: read-only core access")
    );
    assert!(
        matches!(core.reconcile_work_chapters(&principal, work_id.clone(), "http", nexus_core::ReconcileDryRunQuery { dry_run: None }).await, Err(CoreError::Forbidden { resource }) if resource == "work: read-only core access")
    );
    assert!(
        matches!(core.promote_work_pool_entry(&principal, nexus_core::PromotePoolRequest { work_id: work_id.clone(), set_default: None }).await, Err(CoreError::Forbidden { resource }) if resource == "work: read-only core access")
    );
    assert!(
        matches!(core.archive_work_pool_entry(&principal, nexus_core::ArchivePoolRequest { entry_id: "entry".into() }).await, Err(CoreError::Forbidden { resource }) if resource == "work: read-only core access")
    );
    assert!(
        matches!(core.update_finding(&principal, finding.finding_id.clone(), nexus_core::UpdateFindingRequest::default()).await, Err(CoreError::Forbidden { resource }) if resource == "work: read-only core access")
    );
    core.close().await.unwrap();
}

// ─────────────────────────────────────────────────────────────────────────────
// Migrated from the retired daemon HTTP fixtures (P2-T4): `works_api.rs`,
// `findings_api.rs`, `selection_pool.rs`, `delete_routes_api.rs`,
// `multi_work_switch.rs`, `pagination_info_parity.rs`. Only nonduplicated
// domain behavior is preserved: authorization, transitions/CAS, durable state
// and real failure/cleanup. HTTP status envelopes, router registration and
// boot-only expectations retire with the host and are not reproduced here.
// ─────────────────────────────────────────────────────────────────────────────

/// Open a direct-writer core over a fresh guarded workspace for `creator`.
async fn open_work_core(home: &std::path::Path, creator: &str) -> CoreService {
    std::fs::create_dir_all(home.join(".nexus42")).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        home, creator, "default",
    ))
    .unwrap();
    select_creator(home, creator, "default");
    let db = nexus_home_layout::workspace_state_db_path(home, creator, "default");
    let seed = init_guarded_pool(&db, creator).await.unwrap();
    let pool = seed.clone_pool();
    nexus_local_db::creators::ensure_creator_row(&pool, creator, "Author")
        .await
        .unwrap();
    pool.close().await;
    CoreService::open(CoreOpenOptions {
        user_home: home.into(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .unwrap()
}

/// Open an engine-owned core over a fresh guarded workspace for `creator`.
///
/// The `works_idempotency` writer guard admits only `migration`/`engine`
/// writers, so the `client_request_id` replay contract is exercised through the
/// same engine admission the retired daemon used.
async fn open_engine_core(home: &std::path::Path, creator: &str) -> CoreService {
    std::fs::create_dir_all(home.join(".nexus42")).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        home, creator, "default",
    ))
    .unwrap();
    select_creator(home, creator, "default");
    let db = nexus_home_layout::workspace_state_db_path(home, creator, "default");
    let seed = nexus_local_db::writer_protocol::init_engine_pool(
        &db,
        creator,
        nexus_local_db::writer_protocol::GuardedPoolOptions::default(),
    )
    .await
    .unwrap();
    let pool = seed.clone_pool();
    nexus_local_db::creators::ensure_creator_row(&pool, creator, "Author")
        .await
        .unwrap();
    pool.close().await;
    CoreService::open(CoreOpenOptions {
        user_home: home.into(),
        access: CoreAccess::EngineOwner,
    })
    .await
    .unwrap()
}

fn new_work(world: &str, title: &str, client_request_id: Option<&str>) -> CreateWorkRequest {
    serde_json::from_value(serde_json::json!({
        "title": title,
        "long_term_goal": "write",
        "initial_idea": "idea",
        "world_id": world,
        "client_request_id": client_request_id,
    }))
    .unwrap()
}

async fn fresh_world(core: &CoreService, principal: &nexus_core::Principal) -> String {
    core.create_world(
        principal,
        serde_json::from_value::<CreateWorldRequest>(serde_json::json!({"title": "World"}))
            .unwrap(),
    )
    .await
    .unwrap()
    .world_id
}

fn pool_query() -> nexus_core::ListPoolQuery {
    nexus_core::ListPoolQuery {
        status: None,
        limit: None,
        offset: None,
    }
}

fn finding_request(severity: &str, title: &str) -> nexus_core::CreateFindingRequest {
    nexus_core::CreateFindingRequest {
        chapter: None,
        severity: severity.to_string(),
        title: title.to_string(),
        description: String::new(),
        target_executor: "none".to_string(),
        kind: "craft".to_string(),
        rule_suggestion: None,
    }
}

fn status_set(status: &str) -> nexus_core::UpdateFindingRequest {
    nexus_core::UpdateFindingRequest {
        status: Some(status.to_string()),
        ..Default::default()
    }
}

/// Bulk PATCH body, built through the generated contract type (the wire shape
/// the retired daemon route accepted).
fn batch_request(
    finding_ids: Vec<String>,
    patch: serde_json::Value,
) -> nexus_contracts::BatchUpdateFindingsRequest {
    serde_json::from_value(serde_json::json!({
        "finding_ids": finding_ids,
        "patch": patch,
    }))
    .unwrap()
}

/// `client_request_id` replay is idempotent and lineage references are validated
/// before insert (`works_api.rs::create_work_idempotent_replay_returns_200`,
/// `create_work_with_valid_lineage_succeeds`,
/// `create_work_with_nonexistent_lineage_returns_400`,
/// `create_work_with_empty_lineage_returns_400`).
#[tokio::test]
async fn retained_work_create_replay_and_lineage_validation() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let core = open_engine_core(home, "author").await;
    let principal = core.active_principal().await.unwrap();
    let world = fresh_world(&core, &principal).await;

    let first = core
        .create_work(&principal, new_work(&world, "Alpha", Some("crid_replay")))
        .await
        .unwrap();
    assert!(first.work_id.starts_with("wrk_"));
    assert_eq!(first.status, "active");
    let replay = core
        .create_work(&principal, new_work(&world, "Alpha", Some("crid_replay")))
        .await
        .unwrap();
    assert_eq!(replay.work_id, first.work_id, "replay returns the same Work");
    assert_eq!(
        core.list_works(&principal, query(serde_json::json!({})))
            .await
            .unwrap()
            .items
            .len(),
        1,
        "replay must not mint a second Work"
    );

    let mut child_request = new_work(&world, "Beta", None);
    child_request.lineage_from_work_id = Some(first.work_id.clone());
    let child = core.create_work(&principal, child_request).await.unwrap();
    assert_eq!(
        core.get_work(&principal, child.work_id)
            .await
            .unwrap()
            .lineage_from_work_id
            .as_deref(),
        Some(first.work_id.as_str())
    );

    for bad in ["wrk_missing_lineage", ""] {
        let mut request = new_work(&world, "Bad Lineage", None);
        request.lineage_from_work_id = Some(bad.to_string());
        assert!(
            matches!(
                core.create_work(&principal, request).await,
                Err(CoreError::InvalidInput { field, .. }) if field == "invalid_lineage"
            ),
            "lineage '{bad}' must be refused before insert"
        );
    }
    assert_eq!(
        core.list_works(&principal, query(serde_json::json!({})))
            .await
            .unwrap()
            .items
            .len(),
        2,
        "refused lineage creates must not persist rows"
    );
    core.close().await.unwrap();
}

/// Stage advance gate and the runtime-lock release on the stage branch
/// (`works_api.rs::patch_work_intake_status_independent_of_stage_status`,
/// `patch_work_stage_path_releases_runtime_lock`,
/// `patch_work_invalid_stage_value_returns_400`,
/// `patch_stage_status_complete_without_stage_is_rejected`,
/// `patch_stage_status_complete_with_force_is_allowed`).
#[tokio::test]
async fn retained_work_stage_advance_gate_and_lock_release() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let core = open_work_core(home, "author").await;
    let principal = core.active_principal().await.unwrap();
    let world = fresh_world(&core, &principal).await;
    let work_id = create(&core, &principal, &world, "Staged").await;

    let details = core.get_work(&principal, work_id.clone()).await.unwrap();
    assert_eq!(details.current_stage, "intake");
    assert_eq!(details.stage_status, "pending");
    assert!(details.creative_brief.is_none());
    assert!(details.inspiration_log.is_empty());
    assert!(details.schedule_ids.is_empty());

    // A terminal stage_status without an explicit advance is refused.
    let error = core
        .patch_work(
            &principal,
            work_id.clone(),
            "core",
            WorkPatchRequest {
                stage_status: Some("complete".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&error, CoreError::InvalidInput { field, .. } if field == "invalid_status_transition"),
        "got {error:?}"
    );

    // `force` overrides the terminal gate.
    let forced = core
        .patch_work(
            &principal,
            work_id.clone(),
            "core",
            WorkPatchRequest {
                stage_status: Some("complete".into()),
                force: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(forced.stage_status, "complete");

    // The stage branch releases its runtime lock, so the next advance lands.
    // The intake gate stays independent of the stage status: it survives a
    // stage advance unchanged.
    let intake = core
        .patch_work(
            &principal,
            work_id.clone(),
            "core",
            WorkPatchRequest {
                intake_status: Some("complete".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(intake.intake_status, "complete");
    assert_eq!(
        intake.stage_status, "complete",
        "the intake patch must not touch the stage status"
    );
    let advanced = core
        .patch_work(
            &principal,
            work_id.clone(),
            "core",
            WorkPatchRequest {
                current_stage: Some("research".into()),
                stage_status: Some("active".into()),
                force: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(advanced.intake_status, "complete");
    assert_eq!(advanced.current_stage, "research");
    assert_eq!(advanced.stage_status, "active");
    assert_eq!(
        core.get_work(&principal, work_id.clone())
            .await
            .unwrap()
            .runtime_lock_holder,
        None,
        "stage PATCH must release the runtime lock"
    );

    // An unknown stage value is refused and leaves the row untouched.
    let error = core
        .patch_work(
            &principal,
            work_id.clone(),
            "core",
            WorkPatchRequest {
                current_stage: Some("invalid_stage_value".into()),
                stage_status: Some("active".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(&error, CoreError::InvalidInput { field, .. } if field == "invalid_stage"));
    assert_eq!(
        core.get_work(&principal, work_id)
            .await
            .unwrap()
            .current_stage,
        "research"
    );
    core.close().await.unwrap();
}

/// Lazy completion promotion on read is idempotent and writer-only
/// (`works_api.rs::handler_get_work_lazy_promotes_completed_then_is_idempotent`).
#[tokio::test]
async fn retained_work_lazy_completion_promotion_is_idempotent_and_writer_only() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let db = {
        let core = open_work_core(home, "author").await;
        let principal = core.active_principal().await.unwrap();
        let world = fresh_world(&core, &principal).await;
        let work_id = create(&core, &principal, &world, "Novel").await;
        core.close().await.unwrap();
        let db = nexus_home_layout::workspace_state_db_path(home, "author", "default");
        let seed = init_guarded_pool(&db, "author").await.unwrap();
        let pool = seed.clone_pool();
        let now = "2026-06-09T10:00:00Z";
        nexus_local_db::works::patch_work(
            &pool,
            "author",
            &work_id,
            &nexus_local_db::works::WorkPatch {
                work_profile: Some(Some("novel".to_string())),
                total_planned_chapters: Some(Some(2)),
                current_chapter: Some(2),
                intake_status: Some("complete".to_string()),
                work_ref: Some(Some("lazy-novel".to_string())),
                ..Default::default()
            },
            now,
        )
        .await
        .unwrap();
        nexus_local_db::work_chapters::seed_chapters(&pool, &work_id, "lazy-novel", 2, now)
            .await
            .unwrap();
        for chapter in 1..=2 {
            nexus_local_db::work_chapters::update_status(
                &pool, &work_id, chapter, 1, "finalized", Some(4_000), now,
            )
            .await
            .unwrap();
        }
        pool.close().await;
        (db, work_id)
    };
    let (db, work_id) = db;

    // Read-only access must not promote: the guard is writer-only.
    let reader = CoreService::open(CoreOpenOptions {
        user_home: home.into(),
        access: CoreAccess::ReadOnly,
    })
    .await
    .unwrap();
    let read_principal = reader.active_principal().await.unwrap();
    assert_eq!(
        reader
            .get_work(&read_principal, work_id.clone())
            .await
            .unwrap()
            .status,
        "active",
        "read-only reads must not promote completion"
    );
    reader.close().await.unwrap();

    let core = CoreService::open(CoreOpenOptions {
        user_home: home.into(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    let promoted = core.get_work(&principal, work_id.clone()).await.unwrap();
    assert_eq!(promoted.status, "completed");
    let promoted_at = promoted.updated_at.clone();
    let second = core.get_work(&principal, work_id.clone()).await.unwrap();
    assert_eq!(second.status, "completed");
    assert_eq!(
        second.updated_at, promoted_at,
        "a second read must not re-promote"
    );
    assert_eq!(
        core.get_work(&principal, work_id).await.unwrap().updated_at,
        promoted_at
    );
    core.close().await.unwrap();
    let read_back = nexus_local_db::open_pool_read_only(&db).await.unwrap();
    let status: String = sqlx::query_scalar("SELECT status FROM works LIMIT 1")
        .fetch_one(&read_back)
        .await
        .unwrap();
    read_back.close().await;
    assert_eq!(status, "completed");
}

/// Creator-scoped Work addressing: a Work owned by another creator in the same
/// database is invisible to every read and mutation
/// (`works_api.rs::creator_isolation_get_work_returns_404_for_other_creator`,
/// `creator_isolation_patch_work_returns_404_for_other_creator`).
#[tokio::test]
async fn retained_work_foreign_creator_mutations_are_not_found() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let core = open_work_core(home, "author").await;
    let principal = core.active_principal().await.unwrap();
    let world = fresh_world(&core, &principal).await;
    let own = create(&core, &principal, &world, "Own").await;

    let db = nexus_home_layout::workspace_state_db_path(home, "author", "default");
    let seed = init_guarded_pool(&db, "author").await.unwrap();
    let pool = seed.clone_pool();
    nexus_local_db::creators::ensure_creator_row(&pool, "other", "Other")
        .await
        .unwrap();
    let foreign = "wrk_foreign_creator_work".to_string();
    sqlx::query(
        "INSERT INTO works (work_id, creator_id, workspace_slug, status, title, long_term_goal, \
         initial_idea, intake_status, inspiration_log, primary_preset_id, schedule_ids, \
         created_at, updated_at, current_stage, stage_status) VALUES (?, 'other', 'default', \
         'active', 'Foreign', 'g', 'i', 'pending', '[]', 'novel-writing', '[]', ?, ?, 'intake', 'pending')",
    )
    .bind(&foreign)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(&pool)
    .await
    .unwrap();

    assert!(matches!(
        core.get_work(&principal, foreign.clone()).await,
        Err(CoreError::NotFound { .. })
    ));
    assert!(matches!(
        core.patch_work(
            &principal,
            foreign.clone(),
            "core",
            WorkPatchRequest {
                title: Some("Hijacked".into()),
                ..Default::default()
            }
        )
        .await,
        Err(CoreError::NotFound { .. })
    ));
    assert!(matches!(
        core.append_work_inspiration(
            &principal,
            foreign.clone(),
            "core",
            nexus_contracts::AppendInspirationRequest {
                note: "leak".into()
            }
        )
        .await,
        Err(CoreError::NotFound { .. })
    ));
    assert!(matches!(
        core.delete_work(&principal, foreign.clone(), "core").await,
        Err(CoreError::NotFound { .. })
    ));
    let foreign_title: String = sqlx::query_scalar("SELECT title FROM works WHERE work_id = ?")
        .bind(&foreign)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(foreign_title, "Foreign", "foreign row must be untouched");

    // Positive control: the owning creator still mutates its own Work.
    assert!(core.get_work(&principal, own).await.is_ok());
    pool.close().await;
    core.close().await.unwrap();
}

/// Work delete cascade plus the failure path that must not strand a runtime
/// lock (`delete_routes_api.rs::delete_work_cascades_pool_entries_via_fk`,
/// `delete_work_returns_404_for_unknown_id`,
/// `works_api.rs::handler_append_inspiration_404_does_not_acquire_lock`).
#[tokio::test]
async fn retained_work_delete_cascades_pool_entries_and_failure_keeps_no_lock() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let core = open_work_core(home, "author").await;
    let principal = core.active_principal().await.unwrap();
    let world = fresh_world(&core, &principal).await;
    let work_id = create(&core, &principal, &world, "Doomed").await;
    let first_append = core
        .append_work_inspiration(
            &principal,
            work_id.clone(),
            "core",
            nexus_contracts::AppendInspirationRequest {
                note: "First idea".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(first_append.inspiration_count, 1);
    assert_eq!(first_append.work_id, work_id);
    let second_append = core
        .append_work_inspiration(
            &principal,
            work_id.clone(),
            "core",
            nexus_contracts::AppendInspirationRequest {
                note: "Second idea".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(second_append.inspiration_count, 2);
    let logged = core.get_work(&principal, work_id.clone()).await.unwrap();
    assert_eq!(logged.inspiration_log.len(), 2);
    assert_eq!(logged.inspiration_log[0]["note"], "First idea");
    assert_eq!(logged.inspiration_log[1]["note"], "Second idea");
    assert!(logged.schedule_ids.is_empty());
    let entry = core
        .promote_work_pool_entry(
            &principal,
            nexus_core::PromotePoolRequest {
                work_id: work_id.clone(),
                set_default: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(entry.status, "active");

    core.delete_work(&principal, work_id.clone(), "core")
        .await
        .unwrap();
    assert!(matches!(
        core.get_work(&principal, work_id.clone()).await,
        Err(CoreError::NotFound { .. })
    ));

    let db = nexus_home_layout::workspace_state_db_path(home, "author", "default");
    let seed = init_guarded_pool(&db, "author").await.unwrap();
    let pool = seed.clone_pool();
    let pooled: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM novel_pool_entries WHERE work_id = ?")
            .bind(&work_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(pooled, 0, "pool rows cascade with the Work delete");

    assert!(matches!(
        core.append_work_inspiration(
            &principal,
            "wrk_missing_for_append".into(),
            "core",
            nexus_contracts::AppendInspirationRequest {
                note: "nope".into()
            }
        )
        .await,
        Err(CoreError::NotFound { .. })
    ));
    assert!(matches!(
        core.delete_work(&principal, "wrk_missing_for_delete".into(), "core")
            .await,
        Err(CoreError::NotFound { .. })
    ));
    let holders: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM works WHERE runtime_lock_holder IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(holders, 0, "failed mutations must not write a runtime lock");
    pool.close().await;
    core.close().await.unwrap();
}

/// Pool promotion is exclusive and idempotent; archive is a status transition
/// (`selection_pool.rs::test_pool_list_returns_all_statuses`,
/// `test_pool_promote_demotes_prior_active`,
/// `test_pool_promote_idempotent_on_same_target`,
/// `test_pool_archive_marks_archived`).
#[tokio::test]
async fn retained_pool_promotion_exclusivity_idempotence_and_archive() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let core = open_work_core(home, "author").await;
    let principal = core.active_principal().await.unwrap();
    let world = fresh_world(&core, &principal).await;
    let alpha = create(&core, &principal, &world, "Alpha").await;
    let beta = create(&core, &principal, &world, "Beta").await;

    let promoted = |work: String| {
        let core = &core;
        let principal = &principal;
        async move {
            core.promote_work_pool_entry(
                principal,
                nexus_core::PromotePoolRequest {
                    work_id: work,
                    set_default: None,
                },
            )
            .await
            .unwrap()
        }
    };
    assert_eq!(promoted(alpha.clone()).await.status, "active");
    assert_eq!(promoted(beta.clone()).await.status, "active");

    let listed = core.list_work_pool(&principal, pool_query()).await.unwrap();
    let active: Vec<_> = listed
        .entries
        .iter()
        .filter(|entry| entry.status == "active")
        .collect();
    assert_eq!(active.len(), 1, "promotion demotes the prior active entry");
    assert_eq!(active[0].work_id, beta);
    assert_eq!(
        listed
            .entries
            .iter()
            .find(|entry| entry.work_id == alpha)
            .unwrap()
            .status,
        "queued"
    );

    // Promoting the same target again is idempotent — no duplicate row.
    let again = promoted(beta.clone()).await;
    assert_eq!(again.status, "active");
    let listed = core.list_work_pool(&principal, pool_query()).await.unwrap();
    assert_eq!(listed.entries.len(), 2, "re-promotion must not insert a row");

    let archived = core
        .archive_work_pool_entry(
            &principal,
            nexus_core::ArchivePoolRequest {
                entry_id: again.entry_id,
            },
        )
        .await
        .unwrap();
    assert_eq!(archived.status, "archived");
    let listed = core.list_work_pool(&principal, pool_query()).await.unwrap();
    assert!(listed.entries.iter().any(|entry| entry.status == "archived"));
    core.close().await.unwrap();
}

/// Inspiration adds never clobber an existing file, and promotion writes Work,
/// pool entry and inspiration status together
/// (`selection_pool.rs::test_inspiration_add_creates_md_and_db_row_atomically`,
/// `test_inspiration_add_auto_suffixes_on_collision`,
/// `test_inspiration_promote_creates_work_and_pool_row`,
/// `test_promote_inspiration_atomicity_on_step3_failure`).
#[tokio::test]
async fn retained_inspiration_collision_suffixes_and_promote_atomicity() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let core = open_work_core(home, "author").await;
    let principal = core.active_principal().await.unwrap();
    let workspace = nexus_home_layout::operational_workspace_dir(home, "author", "default");

    let add = |title: &str| {
        let core = &core;
        let principal = &principal;
        let title = title.to_string();
        async move {
            core.add_work_inspiration(principal, nexus_core::AddInspirationRequest { title })
                .await
                .unwrap()
        }
    };
    let first = add("Dup Idea").await;
    let second = add("Dup Idea").await;
    assert!(first.item_id.starts_with("npi_"));
    assert_ne!(first.item_id, second.item_id);
    assert_ne!(first.rel_path, second.rel_path, "collision must not clobber");
    assert!(workspace.join(&first.rel_path).is_file());
    assert!(workspace.join(&second.rel_path).is_file());

    let listed = core
        .list_work_inspiration(
            &principal,
            nexus_core::ListInspirationQuery {
                status: None,
                limit: None,
                offset: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(listed.items.len(), 2);
    assert!(listed.items.iter().all(|item| item.status == "idea"));

    let promoted = core
        .promote_work_inspiration(
            &principal,
            nexus_core::PromoteInspirationRequest {
                item_id: second.item_id.clone(),
                idea: Some("Refined".into()),
                set_default: None,
            },
        )
        .await
        .unwrap();
    assert!(promoted.work_id.starts_with("wrk_"));
    assert!(promoted.pool_entry_id.starts_with("npe_"));
    assert_eq!(
        core.get_work(&principal, promoted.work_id.clone())
            .await
            .unwrap()
            .status,
        "draft"
    );
    let pooled = core.list_work_pool(&principal, pool_query()).await.unwrap();
    assert_eq!(
        pooled
            .entries
            .iter()
            .filter(|entry| entry.status == "active")
            .count(),
        1
    );
    let promoted_items = core
        .list_work_inspiration(
            &principal,
            nexus_core::ListInspirationQuery {
                status: Some("promoted".into()),
                limit: None,
                offset: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(promoted_items.items.len(), 1);
    assert_eq!(
        promoted_items.items[0].promoted_work_id.as_deref(),
        Some(promoted.work_id.as_str())
    );
    core.close().await.unwrap();
}

/// Completion marks the pool row and leaves no active entry
/// (`selection_pool.rs::test_completion_updates_pool_row`,
/// `test_completion_demotes_active_pool_row_when_completed`).
#[tokio::test]
async fn retained_completion_demotes_active_pool_entry() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let core = open_work_core(home, "author").await;
    let principal = core.active_principal().await.unwrap();
    let world = fresh_world(&core, &principal).await;
    let work_id = create(&core, &principal, &world, "Completing").await;
    core.promote_work_pool_entry(
        &principal,
        nexus_core::PromotePoolRequest {
            work_id: work_id.clone(),
            set_default: None,
        },
    )
    .await
    .unwrap();

    let db = nexus_home_layout::workspace_state_db_path(home, "author", "default");
    let seed = init_guarded_pool(&db, "author").await.unwrap();
    let pool = seed.clone_pool();
    assert!(
        nexus_local_db::novel_pool_entries::get_active_pool_entry(&pool, "author")
            .await
            .unwrap()
            .is_some()
    );
    nexus_orchestration::auto_chain::mark_work_completed(&pool, "author", &work_id)
        .await
        .unwrap();

    let entry = nexus_local_db::novel_pool_entries::get_pool_entry_by_work(&pool, "author", &work_id)
        .await
        .unwrap()
        .expect("pool row survives completion");
    assert_eq!(entry.status, "completed");
    assert!(
        nexus_local_db::novel_pool_entries::get_active_pool_entry(&pool, "author")
            .await
            .unwrap()
            .is_none(),
        "completed Works must not stay active in the pool"
    );
    pool.close().await;
    core.close().await.unwrap();
}

/// Creator-scoped pool addressing and the `set_pool_active` body-creator guard
/// (`selection_pool.rs::test_archive_pool_rejects_cross_creator`,
/// `test_archive_inspiration_rejects_cross_creator`,
/// `test_promote_inspiration_rejects_cross_creator`,
/// `test_set_pool_active_rejects_mismatched_creator_id`,
/// `test_set_pool_active_allows_matching_creator_id`,
/// `test_set_pool_active_works_without_body_creator_id`).
#[tokio::test]
async fn retained_pool_addressing_and_creator_binding_guards() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let core = open_work_core(home, "author").await;
    let principal = core.active_principal().await.unwrap();
    let world = fresh_world(&core, &principal).await;
    let work_id = create(&core, &principal, &world, "Guarded").await;
    core.promote_work_pool_entry(
        &principal,
        nexus_core::PromotePoolRequest {
            work_id: work_id.clone(),
            set_default: None,
        },
    )
    .await
    .unwrap();

    // A forged body creator_id is refused and the pool is unchanged.
    let error = core
        .set_work_pool_active(
            &principal,
            nexus_core::SetPoolActiveRequest {
                action: "set_pool_active".into(),
                work_id: work_id.clone(),
                creator_id: Some("ctr_attacker".into()),
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(&error, CoreError::Forbidden { .. }), "got {error:?}");
    let db = nexus_home_layout::workspace_state_db_path(home, "author", "default");
    let seed = init_guarded_pool(&db, "author").await.unwrap();
    let pool = seed.clone_pool();
    assert_eq!(
        nexus_local_db::novel_pool_entries::get_active_pool_entry(&pool, "author")
            .await
            .unwrap()
            .unwrap()
            .work_id
            .as_deref(),
        Some(work_id.as_str())
    );

    // Matching and omitted creator_id are the legitimate callers.
    for creator_id in [Some(principal.creator_id().to_string()), None] {
        let entry = core
            .set_work_pool_active(
                &principal,
                nexus_core::SetPoolActiveRequest {
                    action: "set_pool_active".into(),
                    work_id: work_id.clone(),
                    creator_id,
                },
            )
            .await
            .unwrap();
        assert_eq!(entry.status, "active");
    }

    // Rows owned by another creator are unreachable by ID. `archive_*` surfaces
    // the creator-scoped no-row case through the storage carrier (the daemon
    // mapped the same DAO fault onto its 404 envelope); the security-relevant
    // half is that the foreign row is never mutated.
    sqlx::query(
        "INSERT INTO novel_pool_entries (entry_id, creator_id, work_id, status, promoted_at, \
         title, updated_at) VALUES ('npe_foreign', 'other', ?, 'queued', datetime('now'), 'F', \
         datetime('now'))",
    )
    .bind(&work_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO inspiration_items (item_id, creator_id, rel_path, title, status, created_at) \
         VALUES ('npi_foreign', 'other', 'Pool/Ideas/foreign.md', 'Foreign', 'idea', datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();
    let error = core
        .archive_work_pool_entry(
            &principal,
            nexus_core::ArchivePoolRequest {
                entry_id: "npe_foreign".into(),
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&error, CoreError::Internal { category } if category.contains("npe_foreign")),
        "got {error:?}"
    );
    let error = core
        .archive_work_inspiration(
            &principal,
            nexus_core::ArchiveInspirationRequest {
                item_id: "npi_foreign".into(),
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&error, CoreError::Internal { category } if category.contains("npi_foreign")),
        "got {error:?}"
    );
    assert!(matches!(
        core.promote_work_inspiration(
            &principal,
            nexus_core::PromoteInspirationRequest {
                item_id: "npi_foreign".into(),
                idea: None,
                set_default: None
            }
        )
        .await,
        Err(CoreError::NotFound { .. })
    ));
    let foreign_status: String =
        sqlx::query_scalar("SELECT status FROM inspiration_items WHERE item_id = 'npi_foreign'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(foreign_status, "idea");
    let foreign_pool_status: String =
        sqlx::query_scalar("SELECT status FROM novel_pool_entries WHERE entry_id = 'npe_foreign'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(foreign_pool_status, "queued");
    pool.close().await;
    core.close().await.unwrap();
}

/// Findings CRUD, list filters and creator scope
/// (`findings_api.rs::findings_crud_create_and_get`,
/// `findings_list_filter_by_work_id`,
/// `findings_list_filter_by_comma_separated_status`,
/// `findings_creator_isolation_cross_creator_404`, `findings_delete`,
/// `findings_routing_hints_all_executors`).
#[tokio::test]
async fn retained_findings_crud_filters_and_creator_scope() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let core = open_work_core(home, "author").await;
    let principal = core.active_principal().await.unwrap();
    let world = fresh_world(&core, &principal).await;
    let work_id = create(&core, &principal, &world, "Findings").await;

    for (executor, hint) in [
        ("write", "→ write"),
        ("brainstorm", "→ brainstorm"),
        ("master", "→ review-master"),
        ("none", "→ none"),
    ] {
        let mut request = finding_request("info", "routing");
        request.target_executor = executor.to_string();
        let created = core
            .create_finding(&principal, work_id.clone(), request)
            .await
            .unwrap();
        assert_eq!(created.routing_hint.as_deref(), Some(hint));
    }

    let minor = core
        .create_finding(&principal, work_id.clone(), finding_request("minor", "Minor"))
        .await
        .unwrap();
    let blocker = core
        .create_finding(
            &principal,
            work_id.clone(),
            finding_request("blocker", "Blocker"),
        )
        .await
        .unwrap();
    // The review-stage entry point persists chapter, kind and rule suggestion.
    let from_review = core
        .create_finding_from_review(
            &principal,
            work_id.clone(),
            nexus_core::CreateFindingRequest {
                chapter: Some(3),
                severity: "major".to_string(),
                title: "LLM-judge: continuity break".to_string(),
                description: "Age inconsistency between ch2 and ch3".to_string(),
                target_executor: "write".to_string(),
                kind: "continuity".to_string(),
                rule_suggestion: Some("Pin character ages at first appearance.".to_string()),
            },
        )
        .await
        .unwrap();
    assert_eq!(from_review.chapter, Some(3));
    assert_eq!(from_review.kind, "continuity");
    assert_eq!(from_review.status, "open");
    assert_eq!(from_review.routing_hint.as_deref(), Some("→ write"));
    assert!(from_review
        .rule_suggestion
        .as_deref()
        .is_some_and(|text| text.contains("Pin character ages")));
    core.delete_finding(&principal, from_review.finding_id)
        .await
        .unwrap();
    assert!(minor.finding_id.starts_with("fnd_"));
    assert_eq!(minor.status, "open");
    assert_eq!(minor.target_executor, "none");
    assert_eq!(
        core.get_work_finding(&principal, work_id.clone(), minor.finding_id.clone())
            .await
            .unwrap()
            .title,
        "Minor"
    );

    let listed = core
        .list_findings(
            &principal,
            work_id.clone(),
            nexus_core::ListFindingsQuery::default(),
        )
        .await
        .unwrap();
    assert_eq!(listed.items.len(), 6);
    let blockers = core
        .list_findings(
            &principal,
            work_id.clone(),
            nexus_core::ListFindingsQuery {
                severity: Some("blocker".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(blockers.items.len(), 1);
    assert_eq!(blockers.items[0].finding_id, blocker.finding_id);

    // Comma-separated status set: every open row plus the triaged one.
    core.update_finding(&principal, minor.finding_id.clone(), status_set("triaged"))
        .await
        .unwrap();
    let actionable = core
        .list_findings(
            &principal,
            work_id.clone(),
            nexus_core::ListFindingsQuery {
                status: Some("open,triaged".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(actionable.items.len(), 6);
    assert!(actionable
        .items
        .iter()
        .all(|item| item.status == "open" || item.status == "triaged"));
    let open_only = core
        .list_findings(
            &principal,
            work_id.clone(),
            nexus_core::ListFindingsQuery {
                status: Some("open".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(open_only.items.len(), 5);
    let spaced = core
        .list_findings(
            &principal,
            work_id.clone(),
            nexus_core::ListFindingsQuery {
                status: Some("open, triaged".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(spaced.items.len(), 6, "comma + whitespace is tolerated");
    let error = core
        .list_findings(
            &principal,
            work_id.clone(),
            nexus_core::ListFindingsQuery {
                status: Some("open,bogus".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&error, CoreError::InvalidInput { field, reason } if field == "invalid_input" && reason.contains("status")),
        "got {error:?}"
    );

    // Cross-creator scope: another creator's Work is NotFound for list and get.
    let db = nexus_home_layout::workspace_state_db_path(home, "author", "default");
    let seed = init_guarded_pool(&db, "author").await.unwrap();
    let pool = seed.clone_pool();
    nexus_local_db::creators::ensure_creator_row(&pool, "other", "Other")
        .await
        .unwrap();
    sqlx::query("UPDATE works SET creator_id = 'other' WHERE work_id = ?")
        .bind(&work_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        core.list_findings(&principal, work_id.clone(), nexus_core::ListFindingsQuery::default())
            .await,
        Err(CoreError::NotFound { .. })
    ));
    assert!(matches!(
        core.get_work_finding(&principal, work_id.clone(), minor.finding_id.clone())
            .await,
        Err(CoreError::NotFound { .. })
    ));
    sqlx::query("UPDATE works SET creator_id = 'author' WHERE work_id = ?")
        .bind(&work_id)
        .execute(&pool)
        .await
        .unwrap();

    // Creator-scoped row ownership: a finding row belonging to another creator
    // is unreachable by ID, and the author's own row still updates.
    let foreign_row = core
        .create_finding(&principal, work_id.clone(), finding_request("info", "foreign row"))
        .await
        .unwrap();
    sqlx::query("UPDATE findings SET creator_id = 'other' WHERE finding_id = ?")
        .bind(&foreign_row.finding_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        core.get_finding(&principal, foreign_row.finding_id.clone())
            .await,
        Err(CoreError::NotFound { .. })
    ));
    assert!(matches!(
        core.update_finding(&principal, foreign_row.finding_id, status_set("triaged"))
            .await,
        Err(CoreError::NotFound { .. })
    ));
    assert_eq!(
        core.update_finding(&principal, blocker.finding_id.clone(), status_set("triaged"))
            .await
            .unwrap()
            .status,
        "triaged"
    );
    sqlx::query("DELETE FROM findings WHERE creator_id = 'other'")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;

    // Delete removes the row (no soft delete).
    core.delete_finding(&principal, minor.finding_id.clone())
        .await
        .unwrap();
    assert!(matches!(
        core.get_finding(&principal, minor.finding_id).await,
        Err(CoreError::NotFound { .. })
    ));
    core.close().await.unwrap();
}

/// Findings lifecycle transitions, rejection codes and batch-update contracts
/// (`findings_api.rs::findings_lifecycle_*`, `findings_batch_*`).
#[tokio::test]
async fn retained_findings_lifecycle_and_batch_update_contracts() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let core = open_work_core(home, "author").await;
    let principal = core.active_principal().await.unwrap();
    let world = fresh_world(&core, &principal).await;
    let work_id = create(&core, &principal, &world, "Lifecycle").await;

    let walk = core
        .create_finding(&principal, work_id.clone(), finding_request("major", "walk"))
        .await
        .unwrap();
    for status in ["triaged", "in_review", "resolved"] {
        let updated = core
            .update_finding(&principal, walk.finding_id.clone(), status_set(status))
            .await
            .unwrap();
        assert_eq!(updated.status, status);
    }
    for terminal in ["wont_fix", "duplicate"] {
        let seeded = core
            .create_finding(
                &principal,
                work_id.clone(),
                finding_request("minor", terminal),
            )
            .await
            .unwrap();
        assert_eq!(
            core.update_finding(&principal, seeded.finding_id, status_set(terminal))
                .await
                .unwrap()
                .status,
            terminal
        );
    }

    // Terminal → open and self-loops are illegal transitions, not enum errors.
    for (finding_id, target) in [(&walk.finding_id, "open"), (&walk.finding_id, "resolved")] {
        let error = core
            .update_finding(&principal, finding_id.clone(), status_set(target))
            .await
            .unwrap_err();
        assert!(
            matches!(&error, CoreError::InvalidInput { field, .. } if field == "invalid_transition"),
            "got {error:?}"
        );
    }
    assert_eq!(
        core.get_finding(&principal, walk.finding_id.clone())
            .await
            .unwrap()
            .status,
        "resolved",
        "a refused transition must not mutate the row"
    );

    // Unknown status words are membership failures carrying the allowed set.
    let unknown = core
        .create_finding(
            &principal,
            work_id.clone(),
            finding_request("minor", "unknown"),
        )
        .await
        .unwrap();
    let error = core
        .update_finding(&principal, unknown.finding_id.clone(), status_set("closed"))
        .await
        .unwrap_err();
    match error {
        CoreError::InvalidInput { field, reason } => {
            assert_eq!(field, "invalid_input");
            assert!(reason.contains("status"), "{reason}");
            assert!(reason.contains("closed"), "{reason}");
            assert!(reason.contains("open") && reason.contains("resolved"), "{reason}");
        }
        other => panic!("expected InvalidInput, got {other:?}"),
    }
    assert_eq!(
        core.get_finding(&principal, unknown.finding_id.clone())
            .await
            .unwrap()
            .status,
        "open"
    );

    // Batch: partial success collects unknown IDs, illegal transitions conflict.
    let second = core
        .create_finding(&principal, work_id.clone(), finding_request("minor", "second"))
        .await
        .unwrap();
    let batch = |ids: Vec<String>, patch: serde_json::Value| {
        let core = &core;
        let principal = &principal;
        async move {
            core.batch_update_findings(principal, batch_request(ids, patch))
                .await
        }
    };
    let batched = batch(
        vec![
            unknown.finding_id.clone(),
            second.finding_id.clone(),
            walk.finding_id.clone(),
        ],
        serde_json::json!({"status": "triaged"}),
    )
    .await
    .unwrap();
    assert_eq!(batched.updated, 2);
    assert!(batched.not_found.is_empty());
    assert_eq!(batched.conflict, vec![walk.finding_id.clone()]);
    assert_eq!(
        core.get_finding(&principal, second.finding_id.clone())
            .await
            .unwrap()
            .status,
        "triaged"
    );

    // Unknown IDs are reported, not silently skipped.
    let missing = "fnd_00000000000000000000000000".to_string();
    let collected = batch(
        vec![second.finding_id.clone(), missing.clone()],
        serde_json::json!({"status": "in_review"}),
    )
    .await
    .unwrap();
    assert_eq!(collected.updated, 1);
    assert_eq!(collected.not_found, vec![missing]);

    // Target-executor assignment rides the same batch surface.
    let assigned = batch(
        vec![second.finding_id.clone()],
        serde_json::json!({"target_executor": "write"}),
    )
    .await
    .unwrap();
    assert_eq!(assigned.updated, 1);
    assert_eq!(
        core.get_finding(&principal, second.finding_id.clone())
            .await
            .unwrap()
            .target_executor,
        "write"
    );

    // Empty patch is a no-op, not an error.
    let noop = batch(vec![second.finding_id.clone()], serde_json::json!({}))
        .await
        .unwrap();
    assert_eq!(noop.updated, 0);
    assert_eq!(
        core.get_finding(&principal, second.finding_id.clone())
            .await
            .unwrap()
            .status,
        "in_review"
    );

    // Cap, emptiness and duplicates are refused before any write.
    let oversized: Vec<String> = (0..101).map(|i| format!("fnd_{i:024}")).collect();
    for (ids, expected_field) in [
        (oversized, "too_many_findings"),
        (Vec::new(), "invalid_input"),
        (
            vec![second.finding_id.clone(), second.finding_id.clone()],
            "invalid_input",
        ),
    ] {
        let error = batch(ids, serde_json::json!({"status": "triaged"}))
            .await
            .unwrap_err();
        assert!(
            matches!(&error, CoreError::InvalidInput { field, .. } if field == expected_field),
            "got {error:?}"
        );
    }
    core.close().await.unwrap();
}

/// A DAO failure mid-batch fails the request while the already-applied rows stay
/// written (`findings_api.rs::findings_batch_update_mid_batch_dao_error_preserves_prior_updates`).
#[tokio::test]
async fn retained_findings_batch_dao_failure_preserves_prior_updates() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let core = open_work_core(home, "author").await;
    let principal = core.active_principal().await.unwrap();
    let world = fresh_world(&core, &principal).await;
    let work_id = create(&core, &principal, &world, "Batch failure").await;

    let mut ids = Vec::new();
    for title in ["first", "second", "third"] {
        ids.push(
            core.create_finding(&principal, work_id.clone(), finding_request("minor", title))
                .await
                .unwrap()
                .finding_id,
        );
    }

    let db = nexus_home_layout::workspace_state_db_path(home, "author", "default");
    let seed = init_guarded_pool(&db, "author").await.unwrap();
    let pool = seed.clone_pool();
    let trigger = format!(
        "CREATE TRIGGER trg_inject_batch_failure AFTER UPDATE OF status ON findings \
         WHEN NEW.finding_id = '{}' BEGIN SELECT RAISE(FAIL, 'injected mid-batch DAO error'); END",
        ids[1]
    );
    sqlx::query(sqlx::AssertSqlSafe(trigger))
        .execute(&pool)
        .await
        .unwrap();

    let error = core
        .batch_update_findings(
            &principal,
            batch_request(ids.clone(), serde_json::json!({"status": "triaged"})),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&error, CoreError::Internal { category } if category.contains("database_error")),
        "got {error:?}"
    );

    // The handler short-circuits on the first internal fault: the first row was
    // already committed, the third was never reached.
    assert_eq!(
        core.get_finding(&principal, ids[0].clone())
            .await
            .unwrap()
            .status,
        "triaged"
    );
    assert_eq!(
        core.get_finding(&principal, ids[2].clone())
            .await
            .unwrap()
            .status,
        "open",
        "rows after the fault must stay untouched"
    );
    pool.close().await;
    core.close().await.unwrap();
}

/// Retention prune previews without deleting, then deletes only the aged
/// resolved rows (`findings_api.rs::findings_prune_endpoint_dry_run_and_delete`).
#[tokio::test]
async fn retained_findings_prune_dry_run_and_delete() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let core = open_work_core(home, "author").await;
    let principal = core.active_principal().await.unwrap();
    let world = fresh_world(&core, &principal).await;
    let work_id = create(&core, &principal, &world, "Prune").await;

    let aged = core
        .create_finding(&principal, work_id.clone(), finding_request("major", "aged"))
        .await
        .unwrap();
    let recent = core
        .create_finding(&principal, work_id.clone(), finding_request("minor", "recent"))
        .await
        .unwrap();

    let db = nexus_home_layout::workspace_state_db_path(home, "author", "default");
    let seed = init_guarded_pool(&db, "author").await.unwrap();
    let pool = seed.clone_pool();
    let old = chrono::Utc::now().timestamp() - 91 * 24 * 3_600;
    sqlx::query("UPDATE findings SET status = 'resolved', updated_at = ? WHERE finding_id = ?")
        .bind(old)
        .bind(&aged.finding_id)
        .execute(&pool)
        .await
        .unwrap();

    let preview = core.prune_findings(&principal, None, true).await.unwrap();
    assert!(preview.dry_run);
    assert_eq!(preview.count, 1);
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM findings WHERE work_id = ?")
        .bind(&work_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 2, "dry-run must not delete rows");

    let applied = core.prune_findings(&principal, None, false).await.unwrap();
    assert!(!applied.dry_run);
    assert_eq!(applied.count, 1);
    let survivors: Vec<String> =
        sqlx::query_scalar("SELECT finding_id FROM findings WHERE work_id = ? ORDER BY finding_id")
            .bind(&work_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(survivors, vec![recent.finding_id.clone()]);
    assert!(matches!(
        core.get_finding(&principal, aged.finding_id).await,
        Err(CoreError::NotFound { .. })
    ));
    let _ = recent;
    pool.close().await;
    core.close().await.unwrap();
}

/// Work list ordering, cursor walk and the `PaginationInfo` wire shape
/// (`works_api.rs::list_works_sort_by_title_ascending`,
/// `list_works_sort_descending_and_pagination`,
/// `pagination_info_parity.rs::*`).
#[tokio::test]
async fn retained_work_list_ordering_cursor_walk_and_pagination_shape() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let core = open_work_core(home, "author").await;
    let principal = core.active_principal().await.unwrap();
    let world = fresh_world(&core, &principal).await;
    let alpha = create(&core, &principal, &world, "Alpha").await;
    let beta = create(&core, &principal, &world, "Beta").await;
    let charlie = create(&core, &principal, &world, "Charlie").await;

    let ascending = core
        .list_works(&principal, query(serde_json::json!({"sort": "title"})))
        .await
        .unwrap();
    assert_eq!(
        ascending
            .items
            .iter()
            .map(|item| item.work_id.clone())
            .collect::<Vec<_>>(),
        vec![alpha.clone(), beta, charlie]
    );

    let first_page = core
        .list_works(
            &principal,
            query(serde_json::json!({"sort": "-title", "limit": 2})),
        )
        .await
        .unwrap();
    assert_eq!(
        first_page
            .items
            .iter()
            .map(|item| item.title.clone())
            .collect::<Vec<_>>(),
        vec!["Charlie".to_string(), "Beta".to_string()]
    );
    assert!(first_page.pagination.has_more);
    let cursor = first_page
        .pagination
        .next_cursor
        .clone()
        .expect("a truncated page carries a cursor");

    let second_page = core
        .list_works(
            &principal,
            query(serde_json::json!({"sort": "-title", "limit": 2, "cursor": cursor})),
        )
        .await
        .unwrap();
    assert_eq!(
        second_page
            .items
            .iter()
            .map(|item| item.work_id.clone())
            .collect::<Vec<_>>(),
        vec![alpha]
    );
    assert!(!second_page.pagination.has_more);
    assert!(second_page.pagination.next_cursor.is_none());

    // Wire shape consumed by non-Rust clients: `next_cursor` is absent unless
    // there is another page.
    let truncated = nexus_contracts::PaginationInfo {
        limit: 50,
        next_cursor: Some("v2:1:2".to_string()),
        has_more: true,
    };
    let value = serde_json::to_value(&truncated).unwrap();
    assert_eq!(value["limit"], 50);
    assert_eq!(value["next_cursor"], "v2:1:2");
    assert_eq!(value["has_more"], true);
    let complete = nexus_contracts::PaginationInfo {
        limit: 250,
        next_cursor: None,
        has_more: false,
    };
    let encoded = serde_json::to_string(&complete).unwrap();
    assert!(!encoded.contains("next_cursor"));
    let decoded: nexus_contracts::PaginationInfo = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        serde_json::to_value(&decoded).unwrap(),
        serde_json::to_value(&complete).unwrap()
    );
    core.close().await.unwrap();
}
