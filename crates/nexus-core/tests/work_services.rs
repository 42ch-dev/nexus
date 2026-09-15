//! Work selection, authoring and selected-context isolation on guarded storage.
use nexus_contracts::{CreateWorkRequest, CreateWorldRequest, ListWorksQuery};
use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService, WorkPatchRequest};
use nexus_local_db::writer_protocol::init_guarded_pool;

fn select_creator(home: &std::path::Path, creator: &str, workspace: &str) {
    std::fs::write(home.join(".nexus42/config.toml"), format!("active_creator_id = \"{creator}\"\n[active_workspace_slug_by_creator]\n\"{creator}\" = \"{workspace}\"\n")).unwrap();
}

fn query(value: serde_json::Value) -> ListWorksQuery {
    serde_json::from_value(value).unwrap()
}

async fn create(core: &CoreService, principal: &nexus_core::Principal, world: &str, title: &str) -> String {
    let request: CreateWorkRequest = serde_json::from_value(serde_json::json!({
        "title": title, "long_term_goal": "write", "initial_idea": "idea", "world_id": world
    })).unwrap();
    core.create_work(principal, request).await.unwrap().work_id
}

#[tokio::test]
async fn work_selection_invalidates_foreign_context() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    std::fs::create_dir_all(home.join(".nexus42")).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(home, "author", "default")).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(home, "other", "default")).unwrap();
    select_creator(home, "author", "default");
    {
        let db = nexus_home_layout::workspace_state_db_path(home, "author", "default");
        let seed = init_guarded_pool(&db, "author").await.unwrap();
        let pool = seed.clone_pool();
        nexus_local_db::creators::ensure_creator_row(&pool, "author", "Author").await.unwrap();
        pool.close().await;
    }
    let core = CoreService::open(CoreOpenOptions { user_home: home.into(), access: CoreAccess::DirectWriter }).await.unwrap();
    let principal = core.active_principal().await.unwrap();
    let world = core.create_world(&principal, serde_json::from_value::<CreateWorldRequest>(serde_json::json!({"title": "World"})).unwrap()).await.unwrap().world_id;
    let first = create(&core, &principal, &world, "Alpha").await;
    let second = create(&core, &principal, &world, "Beta").await;
    let all = core.list_works(&principal, query(serde_json::json!({}))).await.unwrap();
    assert_eq!(all.items.iter().map(|work| work.work_id.clone()).collect::<std::collections::HashSet<_>>(), [first.clone(), second.clone()].into_iter().collect());
    assert_eq!(all.pagination.limit, 100);
    let selected = core.select_work(&principal, first.clone()).await.unwrap();
    assert!(selected.active);
    core.select_work(&principal, second.clone()).await.unwrap();
    assert!(matches!(core.select_work(&principal, "missing".into()).await, Err(CoreError::NotFound { .. })));

    let db = nexus_home_layout::workspace_state_db_path(home, "author", "default");
    let reader = nexus_local_db::open_pool_read_only(&db).await.unwrap();
    assert_eq!(nexus_local_db::novel_pool_entries::get_active_pool_entry(&reader, "author").await.unwrap().unwrap().work_id.as_deref(), Some(second.as_str()));
    assert_eq!(nexus_local_db::novel_pool_entries::get_pool_entry_by_work(&reader, "author", &first).await.unwrap().unwrap().status, "queued");

    // A foreign Creator's row in the same database must not become selectable.
    let guarded = init_guarded_pool(&db, "author").await.unwrap();
    let pool = guarded.clone_pool();
    nexus_local_db::creators::ensure_creator_row(&pool, "other", "Other").await.unwrap();
    sqlx::query("UPDATE works SET creator_id = 'other' WHERE work_id = ?").bind(&first).execute(&pool).await.unwrap();
    assert!(matches!(core.select_work(&principal, first.clone()).await, Err(CoreError::NotFound { .. })));
    assert_eq!(nexus_local_db::novel_pool_entries::get_active_pool_entry(&reader, "author").await.unwrap().unwrap().work_id.as_deref(), Some(second.as_str()));
    let listed = core.list_works(&principal, query(serde_json::json!({}))).await.unwrap();
    assert_eq!(listed.items.iter().map(|work| &work.work_id).collect::<Vec<_>>(), vec![&second]);

    // Core-owned nullable patch keeps absence / clear / set distinct.
    core.patch_work(&principal, second.clone(), WorkPatchRequest { story_ref: Some(Some("story".into())), ..Default::default() }).await.unwrap();
    core.patch_work(&principal, second.clone(), WorkPatchRequest { title: Some("Changed".into()), ..Default::default() }).await.unwrap();
    assert_eq!(core.get_work(&principal, second.clone()).await.unwrap().story_ref.as_deref(), Some("story"));
    core.patch_work(&principal, second.clone(), WorkPatchRequest { story_ref: Some(None), ..Default::default() }).await.unwrap();
    assert_eq!(core.get_work(&principal, second.clone()).await.unwrap().story_ref, None);
    assert!(matches!(core.patch_work(&principal, second.clone(), WorkPatchRequest { world_id: Some(None), ..Default::default() }).await, Err(CoreError::InvalidInput { .. })));
    assert_eq!(core.get_work(&principal, second.clone()).await.unwrap().world_id.as_deref(), Some(world.as_str()));
    assert_eq!(core.get_work(&principal, second.clone()).await.unwrap().runtime_lock_holder, None);
    assert!(matches!(core.patch_work(&principal, second.clone(), WorkPatchRequest {
        stage_status: Some("complete".into()), title: Some("Rejected".into()), ..Default::default()
    }).await, Err(CoreError::InvalidInput { .. })));
    let unchanged = core.get_work(&principal, second.clone()).await.unwrap();
    assert_eq!(unchanged.title, "Changed");
    assert_eq!(unchanged.runtime_lock_holder, None);

    let lock_path = home.join("creative/Works/book/.completion-lock.json");
    std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
    std::fs::write(&lock_path, "{}").unwrap();
    std::fs::write(nexus_home_layout::operational_workspace_dir(home, "author", "default").join("meta.json"),
        serde_json::to_vec(&serde_json::json!({"creative_root": home.join("creative")})).unwrap()).unwrap();
    sqlx::query("UPDATE works SET work_ref = 'book' WHERE work_id = ?").bind(&second).execute(&pool).await.unwrap();
    sqlx::query("UPDATE works SET completion_locked_at = '2026-09-15', novel_completion_status = 'completed', total_planned_chapters = 3 WHERE work_id = ?").bind(&second).execute(&pool).await.unwrap();
    let expected_lock = format!("work_conflict:work {second} is completion-locked since 2026-09-15; use 'creator works completion-lock release' first");
    assert!(matches!(core.patch_work(&principal, second.clone(), WorkPatchRequest::default()).await, Err(CoreError::Forbidden { resource }) if resource == expected_lock));
    assert!(matches!(core.delete_work(&principal, second.clone()).await, Err(CoreError::Forbidden { resource }) if resource == expected_lock));
    let released = core.release_work_completion_lock(&principal, second.clone(), nexus_contracts::ReleaseCompletionLockRequest { reason: "continue writing".into() }).await.unwrap();
    assert_eq!(released.completion_locked_at, None);
    assert_eq!(released.novel_completion_status.as_deref(), Some("reopened"));
    assert_eq!(released.total_planned_chapters, Some(3));
    assert!(!lock_path.exists());

    sqlx::query("UPDATE works SET runtime_lock_holder = 'driver', runtime_lock_acquired_at = ? WHERE work_id = ?").bind(chrono::Utc::now().to_rfc3339()).bind(&second).execute(&pool).await.unwrap();
    assert!(matches!(core.patch_work(&principal, second.clone(), WorkPatchRequest::default()).await,
        Err(CoreError::Forbidden { resource }) if resource == format!("work_locked:work {second} is locked by 'driver'; wait for release or check 'creator works status'")));
    sqlx::query("UPDATE works SET runtime_lock_holder = NULL, runtime_lock_acquired_at = NULL WHERE work_id = ?").bind(&second).execute(&pool).await.unwrap();

    let inspiration = core.add_work_inspiration(&principal, nexus_core::AddInspirationRequest { title: "A new story".into() }).await.unwrap();
    assert!(nexus_home_layout::operational_workspace_dir(home, "author", "default").join(&inspiration.rel_path).is_file());
    let promoted = core.promote_work_inspiration(&principal, nexus_core::PromoteInspirationRequest { item_id: inspiration.item_id.clone(), idea: None, set_default: None }).await.unwrap();
    assert_eq!(core.get_work(&principal, promoted.work_id.clone()).await.unwrap().status, "draft");
    let archived = core.archive_work_pool_entry(&principal, nexus_core::ArchivePoolRequest { entry_id: promoted.pool_entry_id }).await.unwrap();
    assert_eq!(archived.status, "archived");
    assert_eq!(core.archive_work_inspiration(&principal, nexus_core::ArchiveInspirationRequest { item_id: inspiration.item_id }).await.unwrap().status, "archived");
    let creative_root = home.join("creative");
    std::fs::create_dir_all(creative_root.join("Works")).unwrap();
    std::fs::create_dir_all(creative_root.join("outside")).unwrap();
    std::fs::write(creative_root.join("outside/keep"), "must survive").unwrap();
    std::fs::write(nexus_home_layout::operational_workspace_dir(home, "author", "default").join("meta.json"),
        serde_json::to_vec(&serde_json::json!({"creative_root": creative_root})).unwrap()).unwrap();
    sqlx::query("UPDATE works SET work_ref = '../outside' WHERE work_id = ?").bind(&promoted.work_id).execute(&pool).await.unwrap();
    core.delete_work(&principal, promoted.work_id.clone()).await.unwrap();
    assert!(matches!(core.get_work(&principal, promoted.work_id).await, Err(CoreError::NotFound { .. })));
    assert_eq!(std::fs::read_to_string(creative_root.join("outside/keep")).unwrap(), "must survive");

    assert!(matches!(core.set_work_pool_active(&principal, nexus_core::SetPoolActiveRequest {
        action: "other".into(), work_id: "missing".into(), creator_id: None,
    }).await, Err(CoreError::InvalidInput { field, .. }) if field == "invalid_action"));

    for cursor in ["v1:4294967296", "v1:-1", "invalid"] {
        assert!(matches!(core.list_works(&principal, query(serde_json::json!({"cursor":cursor}))).await, Err(CoreError::InvalidInput { .. })));
    }
    let bounded = core.list_works(&principal, query(serde_json::json!({"limit":501,"sort":" -title, ,status "}))).await.unwrap();
    assert_eq!(bounded.pagination.limit, 500);
    assert!(matches!(core.list_works(&principal, query(serde_json::json!({"sort":"bad"}))).await, Err(CoreError::InvalidInput { .. })));
    assert_eq!(core.list_works(&principal, query(serde_json::json!({"limit":-1}))).await.unwrap().pagination.limit, 100);
    assert_eq!(core.list_works(&principal, query(serde_json::json!({"limit":4294967296_i64}))).await.unwrap().pagination.limit, 100);
    assert_eq!(core.list_works(&principal, query(serde_json::json!({"limit":0}))).await.unwrap().pagination.limit, 0);
    assert!(core.list_works(&principal, query(serde_json::json!({"cursor":"v1:4294967295"}))).await.unwrap().items.is_empty());

    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(home, "author", "second")).unwrap();
    select_creator(home, "author", "second");
    assert!(matches!(core.get_work(&principal, second.clone()).await, Err(CoreError::AuthRequired)));
    let switched = CoreService::open(CoreOpenOptions { user_home: home.into(), access: CoreAccess::DirectWriter }).await.unwrap();
    let switched_principal = switched.active_principal().await.unwrap();
    assert!(switched.list_works(&switched_principal, query(serde_json::json!({}))).await.unwrap().items.is_empty());
    assert!(matches!(switched.select_work(&switched_principal, second.clone()).await, Err(CoreError::NotFound { .. })));
    switched.close().await.unwrap();

    // A stored workspace switch invalidates every operation on the old opened pool.
    select_creator(home, "other", "default");
    assert!(matches!(core.list_works(&principal, query(serde_json::json!({}))).await, Err(CoreError::AuthRequired)));
    assert!(matches!(core.select_work(&principal, second).await, Err(CoreError::AuthRequired)));
    assert!(matches!(core.active_principal().await, Err(CoreError::AuthRequired)));
    let reopened = CoreService::open(CoreOpenOptions { user_home: home.into(), access: CoreAccess::DirectWriter }).await.unwrap();
    let other = reopened.active_principal().await.unwrap();
    assert!(reopened.list_works(&other, query(serde_json::json!({}))).await.unwrap().items.is_empty());
    reopened.close().await.unwrap();
    reader.close().await;
    pool.close().await;
    core.close().await.unwrap();
}
