//! Selection pool + inspiration pool hermetic tests (DF-61, V1.41 P1 T8).
//!
//! Covers:
//! - Pool list, promote (with demotion), archive
//! - Inspiration add (atomic MD scaffold + DB), list, promote (creates Work + pool row), archive
//! - Completion → pool row status update
//! - Completion demotes active pool row when completed

#![allow(clippy::unwrap_used)]

use axum::extract::{Query, State};
use nexus_daemon_runtime::api::handlers::works::{
    AddInspirationRequest, ArchiveInspirationRequest, ArchivePoolRequest, CreateWorkRequest,
    ListInspirationQuery, ListPoolQuery, PromoteInspirationRequest, PromotePoolRequest,
};
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_local_db::works;

use serial_test::serial;
// ─── Helpers ───────────────────────────────────────────────────────────────

async fn handler_state() -> (WorkspaceState, test_utils::TestTempRoot) {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let user_home = tmp.path();
    let op_dir = nexus_home_layout::operational_workspace_dir(user_home, "test_creator", "default");
    let workspace_path = Some(op_dir.display().to_string());
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, workspace_path).await;
    test_utils::seed_test_creator_and_world(state.pool()).await;
    (state, tmp)
}

async fn create_test_work(state: &WorkspaceState, title: &str) -> String {
    let req = CreateWorkRequest {
        title: title.to_string(),
        long_term_goal: "Test goal".into(),
        initial_idea: "Test idea".into(),
        world_id: Some("wld_test_world".to_string()),
        story_ref: None,
        primary_preset_id: None,
        client_request_id: None,
        lineage_from_work_id: None,
        set_pool_active: None,
    };
    let (_, resp) = nexus_daemon_runtime::api::handlers::works::create_work(
        State(state.clone()),
        axum::Json(req),
    )
    .await
    .unwrap();
    resp.work_id.clone()
}

// ─── TC1: Pool list returns all statuses ────────────────────────────────

#[tokio::test]
async fn test_pool_list_returns_all_statuses() {
    let (state, _tmp) = handler_state().await;
    let work_id_1 = create_test_work(&state, "Novel Alpha").await;
    let work_id_2 = create_test_work(&state, "Novel Beta").await;

    // Promote work 1, then work 2 (demotes 1 to queued)
    let promote_req = PromotePoolRequest {
        work_id: work_id_1.clone(),
        set_default: None,
    };
    let _ = nexus_daemon_runtime::api::handlers::works::promote_pool_entry(
        State(state.clone()),
        axum::Json(promote_req),
    )
    .await
    .unwrap();

    let promote_req = PromotePoolRequest {
        work_id: work_id_2.clone(),
        set_default: None,
    };
    let _ = nexus_daemon_runtime::api::handlers::works::promote_pool_entry(
        State(state.clone()),
        axum::Json(promote_req),
    )
    .await
    .unwrap();

    // List all entries
    let resp = nexus_daemon_runtime::api::handlers::works::list_pool(
        State(state.clone()),
        Query(ListPoolQuery { status: None }),
    )
    .await
    .unwrap();

    assert_eq!(resp.entries.len(), 2);
    // Active entry should be work_id_2
    let active: Vec<_> = resp
        .entries
        .iter()
        .filter(|e| e.status == "active")
        .collect();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].work_id, work_id_2);
}

// ─── TC2: Pool promote demotes prior active ─────────────────────────────

#[tokio::test]
async fn test_pool_promote_demotes_prior_active() {
    let (state, _tmp) = handler_state().await;
    let work_id_1 = create_test_work(&state, "Novel Alpha").await;
    let work_id_2 = create_test_work(&state, "Novel Beta").await;

    // Promote work 1
    let promote_req = PromotePoolRequest {
        work_id: work_id_1.clone(),
        set_default: None,
    };
    let entry1 = nexus_daemon_runtime::api::handlers::works::promote_pool_entry(
        State(state.clone()),
        axum::Json(promote_req),
    )
    .await
    .unwrap();
    assert_eq!(entry1.status, "active");

    // Promote work 2 — should demote work 1
    let promote_req = PromotePoolRequest {
        work_id: work_id_2.clone(),
        set_default: None,
    };
    let entry2 = nexus_daemon_runtime::api::handlers::works::promote_pool_entry(
        State(state.clone()),
        axum::Json(promote_req),
    )
    .await
    .unwrap();
    assert_eq!(entry2.status, "active");

    // Verify only one active
    let resp = nexus_daemon_runtime::api::handlers::works::list_pool(
        State(state.clone()),
        Query(ListPoolQuery { status: None }),
    )
    .await
    .unwrap();

    let active: Vec<_> = resp
        .entries
        .iter()
        .filter(|e| e.status == "active")
        .collect();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].work_id, work_id_2);
}

// ─── TC3: Pool promote idempotent on same target ────────────────────────

#[tokio::test]
async fn test_pool_promote_idempotent_on_same_target() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_test_work(&state, "Novel Alpha").await;

    let entry1 = nexus_daemon_runtime::api::handlers::works::promote_pool_entry(
        State(state.clone()),
        axum::Json(PromotePoolRequest {
            work_id: work_id.clone(),
            set_default: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(entry1.status, "active");

    // Promote same work again — should be idempotent
    let entry2 = nexus_daemon_runtime::api::handlers::works::promote_pool_entry(
        State(state.clone()),
        axum::Json(PromotePoolRequest {
            work_id: work_id.clone(),
            set_default: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(entry2.status, "active");

    // Should have exactly one entry
    let resp = nexus_daemon_runtime::api::handlers::works::list_pool(
        State(state.clone()),
        Query(ListPoolQuery { status: None }),
    )
    .await
    .unwrap();
    assert_eq!(resp.entries.len(), 1);
}

// ─── TC4: Pool archive marks archived ───────────────────────────────────

#[tokio::test]
async fn test_pool_archive_marks_archived() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_test_work(&state, "Novel Alpha").await;

    let entry = nexus_daemon_runtime::api::handlers::works::promote_pool_entry(
        State(state.clone()),
        axum::Json(PromotePoolRequest {
            work_id: work_id.clone(),
            set_default: None,
        }),
    )
    .await
    .unwrap();

    let archived = nexus_daemon_runtime::api::handlers::works::archive_pool_entry_handler(
        State(state.clone()),
        axum::Json(ArchivePoolRequest {
            entry_id: entry.entry_id.clone(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(archived.status, "archived");
}

// ─── TC5: Inspiration add creates MD and DB row atomically ──────────────

#[tokio::test]
#[serial]
async fn test_inspiration_add_creates_md_and_db_row_atomically() {
    let (state, _tmp) = handler_state().await;

    let (_status, resp) = nexus_daemon_runtime::api::handlers::works::add_inspiration(
        State(state.clone()),
        axum::Json(AddInspirationRequest {
            title: "TC5 My Great Idea".to_string(),
        }),
    )
    .await
    .unwrap();

    assert!(
        resp.item_id.starts_with("npi_"),
        "item_id should have npi_ prefix"
    );
    assert!(
        resp.rel_path.contains("tc5-my-great-idea"),
        "rel_path should contain slug: {}",
        resp.rel_path
    );

    // Verify DB row exists
    let list_resp = nexus_daemon_runtime::api::handlers::works::list_inspiration(
        State(state.clone()),
        Query(ListInspirationQuery { status: None }),
    )
    .await
    .unwrap();
    assert_eq!(list_resp.items.len(), 1);
    assert_eq!(list_resp.items[0].title, "TC5 My Great Idea");
    assert_eq!(list_resp.items[0].status, "idea");
}

// ─── TC6: Inspiration add rejects existing path ─────────────────────────

#[tokio::test]
#[serial]
async fn test_inspiration_add_rejects_existing_path() {
    let (state, _tmp) = handler_state().await;

    // First add succeeds
    let _ = nexus_daemon_runtime::api::handlers::works::add_inspiration(
        State(state.clone()),
        axum::Json(AddInspirationRequest {
            title: "TC6 Duplicate Idea".to_string(),
        }),
    )
    .await
    .unwrap();

    // Second add with same title should fail (slug collision → constraint violation)
    let result = nexus_daemon_runtime::api::handlers::works::add_inspiration(
        State(state.clone()),
        axum::Json(AddInspirationRequest {
            title: "TC6 Duplicate Idea".to_string(),
        }),
    )
    .await;
    assert!(result.is_err(), "Duplicate inspiration should be rejected");
}

// ─── TC7: Inspiration promote creates Work and pool row ─────────────────

#[tokio::test]
#[serial]
async fn test_inspiration_promote_creates_work_and_pool_row() {
    let (state, _tmp) = handler_state().await;

    // Add an inspiration item
    let (_status, add_resp) = nexus_daemon_runtime::api::handlers::works::add_inspiration(
        State(state.clone()),
        axum::Json(AddInspirationRequest {
            title: "TC7 Promotable Idea".to_string(),
        }),
    )
    .await
    .unwrap();

    // Promote it
    let promote_resp = nexus_daemon_runtime::api::handlers::works::promote_inspiration_handler(
        State(state.clone()),
        axum::Json(PromoteInspirationRequest {
            item_id: add_resp.item_id.clone(),
            idea: Some("A more refined idea".to_string()),
            set_default: None,
        }),
    )
    .await
    .unwrap();

    assert!(
        promote_resp.work_id.starts_with("wrk_"),
        "should create a Work"
    );
    assert!(
        promote_resp.pool_entry_id.starts_with("npe_"),
        "should create a pool entry"
    );

    // Verify the new Work exists
    let work = works::get_work(state.pool(), "test_creator", &promote_resp.work_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(work.status, "draft");

    // Verify pool entry is active
    let pool_resp = nexus_daemon_runtime::api::handlers::works::list_pool(
        State(state.clone()),
        Query(ListPoolQuery { status: None }),
    )
    .await
    .unwrap();
    let active_entries: Vec<_> = pool_resp
        .entries
        .iter()
        .filter(|e| e.status == "active")
        .collect();
    assert_eq!(active_entries.len(), 1);

    // Verify inspiration item is promoted
    let insp_resp = nexus_daemon_runtime::api::handlers::works::list_inspiration(
        State(state.clone()),
        Query(ListInspirationQuery {
            status: Some("promoted".to_string()),
        }),
    )
    .await
    .unwrap();
    assert_eq!(insp_resp.items.len(), 1);
}

// ─── TC8: Completion updates pool row ───────────────────────────────────

#[tokio::test]
async fn test_completion_updates_pool_row() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_test_work(&state, "Completing Novel").await;

    // Promote to active
    let _ = nexus_daemon_runtime::api::handlers::works::promote_pool_entry(
        State(state.clone()),
        axum::Json(PromotePoolRequest {
            work_id: work_id.clone(),
            set_default: None,
        }),
    )
    .await
    .unwrap();

    // Mark completed via auto_chain (T7 hook)
    nexus_orchestration::auto_chain::mark_work_completed(state.pool(), "test_creator", &work_id)
        .await
        .unwrap();

    // Check pool entry is now completed
    let pool_entry = nexus_local_db::novel_pool_entries::get_pool_entry_by_work(
        state.pool(),
        "test_creator",
        &work_id,
    )
    .await
    .unwrap()
    .expect("pool entry should exist");
    assert_eq!(pool_entry.status, "completed");
}

// ─── TC9: Completion demotes active pool row when completed ─────────────

#[tokio::test]
async fn test_completion_demotes_active_pool_row_when_completed() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_test_work(&state, "Active Then Completed").await;

    // Promote to active
    let _ = nexus_daemon_runtime::api::handlers::works::promote_pool_entry(
        State(state.clone()),
        axum::Json(PromotePoolRequest {
            work_id: work_id.clone(),
            set_default: None,
        }),
    )
    .await
    .unwrap();

    // Verify it's active
    let active_before =
        nexus_local_db::novel_pool_entries::get_active_pool_entry(state.pool(), "test_creator")
            .await
            .unwrap();
    assert!(
        active_before.is_some(),
        "should have an active pool entry before completion"
    );

    // Mark completed
    nexus_orchestration::auto_chain::mark_work_completed(state.pool(), "test_creator", &work_id)
        .await
        .unwrap();

    // Verify no active pool entry remains (completed entries are not "active")
    let active_after =
        nexus_local_db::novel_pool_entries::get_active_pool_entry(state.pool(), "test_creator")
            .await
            .unwrap();
    assert!(
        active_after.is_none(),
        "active pool entry should be gone after completion"
    );

    // The completed entry should still exist but with status=completed
    let completed_entry = nexus_local_db::novel_pool_entries::get_pool_entry_by_work(
        state.pool(),
        "test_creator",
        &work_id,
    )
    .await
    .unwrap()
    .expect("completed pool entry should still exist");
    assert_eq!(completed_entry.status, "completed");
}

// ─── TC10: Cross-creator archive guard — pool ───────────────────────────

#[tokio::test]
async fn test_archive_pool_rejects_cross_creator() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_test_work(&state, "Owned Novel").await;

    // Promote to create a pool entry
    let entry = nexus_daemon_runtime::api::handlers::works::promote_pool_entry(
        State(state.clone()),
        axum::Json(PromotePoolRequest {
            work_id: work_id.clone(),
            set_default: None,
        }),
    )
    .await
    .unwrap();

    // Overwrite config to simulate a different creator
    let config_path = state.nexus_home().join("config.toml");
    let other_creator = "other_creator";
    std::fs::write(
        &config_path,
        format!(
            "active_creator_id = \"{other_creator}\"\n[active_workspace_slug_by_creator]\n\"{other_creator}\" = \"default\""
        ),
    )
    .unwrap();

    // Other creator tries to archive — should fail (NotFound because 0 rows updated)
    let result = nexus_daemon_runtime::api::handlers::works::archive_pool_entry_handler(
        State(state.clone()),
        axum::Json(ArchivePoolRequest {
            entry_id: entry.entry_id.clone(),
        }),
    )
    .await;
    assert!(result.is_err(), "Cross-creator archive should be rejected");
}

// ─── TC11: Cross-creator archive guard — inspiration ────────────────────

#[tokio::test]
#[serial]
async fn test_archive_inspiration_rejects_cross_creator() {
    let (state, _tmp) = handler_state().await;

    let (_status, resp) = nexus_daemon_runtime::api::handlers::works::add_inspiration(
        State(state.clone()),
        axum::Json(AddInspirationRequest {
            title: "TC11 Creator Idea".to_string(),
        }),
    )
    .await
    .unwrap();

    // Switch to other creator
    let config_path = state.nexus_home().join("config.toml");
    let other_creator = "other_creator";
    std::fs::write(
        &config_path,
        format!(
            "active_creator_id = \"{other_creator}\"\n[active_workspace_slug_by_creator]\n\"{other_creator}\" = \"default\""
        ),
    )
    .unwrap();

    use nexus_daemon_runtime::api::handlers::works::ArchiveInspirationRequest;
    let result = nexus_daemon_runtime::api::handlers::works::archive_inspiration_handler(
        State(state.clone()),
        axum::Json(ArchiveInspirationRequest {
            item_id: resp.item_id.clone(),
        }),
    )
    .await;
    assert!(
        result.is_err(),
        "Cross-creator inspiration archive should be rejected"
    );
}

// ─── TC12: Cross-creator promote guard — inspiration ────────────────────

#[tokio::test]
#[serial]
async fn test_promote_inspiration_rejects_cross_creator() {
    let (state, _tmp) = handler_state().await;

    let (_status, add_resp) = nexus_daemon_runtime::api::handlers::works::add_inspiration(
        State(state.clone()),
        axum::Json(AddInspirationRequest {
            title: "TC12 Creator Idea".to_string(),
        }),
    )
    .await
    .unwrap();

    // Switch to other creator
    let config_path = state.nexus_home().join("config.toml");
    let other_creator = "other_creator";
    std::fs::write(
        &config_path,
        format!(
            "active_creator_id = \"{other_creator}\"\n[active_workspace_slug_by_creator]\n\"{other_creator}\" = \"default\""
        ),
    )
    .unwrap();

    let result = nexus_daemon_runtime::api::handlers::works::promote_inspiration_handler(
        State(state.clone()),
        axum::Json(PromoteInspirationRequest {
            item_id: add_resp.item_id.clone(),
            idea: None,
            set_default: None,
        }),
    )
    .await;
    assert!(
        result.is_err(),
        "Cross-creator inspiration promote should be rejected"
    );
}
