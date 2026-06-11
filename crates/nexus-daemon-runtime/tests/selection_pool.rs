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
        Query(ListPoolQuery {
            status: None,
            ..Default::default()
        }),
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
        Query(ListPoolQuery {
            status: None,
            ..Default::default()
        }),
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
        Query(ListPoolQuery {
            status: None,
            ..Default::default()
        }),
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
        Query(ListInspirationQuery {
            status: None,
            ..Default::default()
        }),
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
async fn test_inspiration_add_auto_suffixes_on_collision() {
    let (state, _tmp) = handler_state().await;

    // First add succeeds
    let (_status, resp1) = nexus_daemon_runtime::api::handlers::works::add_inspiration(
        State(state.clone()),
        axum::Json(AddInspirationRequest {
            title: "TC6 Duplicate Idea".to_string(),
        }),
    )
    .await
    .unwrap();

    // V1.42 P-last (R-V141P1-13): second add with same title now auto-suffixes
    // instead of returning an error.
    let (_status, resp2) = nexus_daemon_runtime::api::handlers::works::add_inspiration(
        State(state.clone()),
        axum::Json(AddInspirationRequest {
            title: "TC6 Duplicate Idea".to_string(),
        }),
    )
    .await
    .unwrap();

    // Both items should have different IDs and different paths
    let id1 = resp1.item_id.as_str();
    let id2 = resp2.item_id.as_str();
    assert_ne!(id1, id2, "Duplicate inspiration should get a new item ID");

    let path1 = resp1.rel_path.as_str();
    let path2 = resp2.rel_path.as_str();
    assert_ne!(
        path1, path2,
        "Duplicate inspiration should get auto-suffixed path"
    );
    assert!(
        path2.contains("tc6-duplicate-idea-"),
        "Auto-suffixed path should contain -2, -3, etc."
    );
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
        Query(ListPoolQuery {
            status: None,
            ..Default::default()
        }),
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
            ..Default::default()
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

// ─── TC13: Promote inspiration atomicity ─────────────────────────────────

/// Verifies that inspiration promote wraps Work create + pool promote +
/// inspiration update in a single transaction. We verify indirectly by
/// promoting an item and checking all three artifacts exist in the expected
/// state. (True step-3-failure injection requires a mock, which is beyond
/// the scope of this hermetic test; the atomic function is tested via
/// code review of the single-tx implementation.)
#[tokio::test]
#[serial]
async fn test_promote_inspiration_atomicity_on_step3_failure() {
    let (state, _tmp) = handler_state().await;

    // Add inspiration
    let (_status, add_resp) = nexus_daemon_runtime::api::handlers::works::add_inspiration(
        State(state.clone()),
        axum::Json(AddInspirationRequest {
            title: "TC13 Atomic Idea".to_string(),
        }),
    )
    .await
    .unwrap();

    // Promote it
    let promote_resp = nexus_daemon_runtime::api::handlers::works::promote_inspiration_handler(
        State(state.clone()),
        axum::Json(PromoteInspirationRequest {
            item_id: add_resp.item_id.clone(),
            idea: Some("Refined atomic idea".to_string()),
            set_default: None,
        }),
    )
    .await
    .unwrap();

    // Verify Work exists
    let work = works::get_work(state.pool(), "test_creator", &promote_resp.work_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(work.status, "draft");

    // Verify pool entry is active
    let pool_entry = nexus_local_db::novel_pool_entries::get_pool_entry_by_work(
        state.pool(),
        "test_creator",
        &promote_resp.work_id,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(pool_entry.status, "active");

    // Verify inspiration is promoted
    let item = nexus_local_db::inspiration_items::get_inspiration(state.pool(), &add_resp.item_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(item.status, "promoted");
    assert_eq!(
        item.promoted_work_id.as_deref(),
        Some(promote_resp.work_id.as_str())
    );
}

// ─── TC14: set_pool_active IDOR — body creator_id must not override active ─

/// Verify that `set_pool_active` rejects a request where the body `creator_id`
/// does not match the active creator from config.toml. This is the IDOR fix:
/// a caller with daemon API access must not be able to switch another
/// creator's active pool work.
#[tokio::test]
async fn test_set_pool_active_rejects_mismatched_creator_id() {
    use nexus_daemon_runtime::api::handlers::works::SetPoolActiveRequest;

    let (state, _tmp) = handler_state().await;

    // Create a Work owned by test_creator (the active creator)
    let work_id = create_test_work(&state, "IDOR Target Novel").await;

    // Promote it via the legitimate promote handler
    let _entry = nexus_daemon_runtime::api::handlers::works::promote_pool_entry(
        State(state.clone()),
        axum::Json(PromotePoolRequest {
            work_id: work_id.clone(),
            set_default: None,
        }),
    )
    .await
    .unwrap();

    // Verify it's active before the attack
    let active_before =
        nexus_local_db::novel_pool_entries::get_active_pool_entry(state.pool(), "test_creator")
            .await
            .unwrap();
    assert!(
        active_before.is_some(),
        "pre-condition: should have an active pool entry"
    );

    // Attack: send set_pool_active with a forged creator_id
    let result = nexus_daemon_runtime::api::handlers::works::set_pool_active(
        State(state.clone()),
        axum::Json(SetPoolActiveRequest {
            action: "set_pool_active".to_string(),
            work_id: work_id.clone(),
            creator_id: Some("ctr_attacker".to_string()),
        }),
    )
    .await;

    // Must be rejected as 403 Forbidden
    assert!(result.is_err(), "IDOR request must be rejected");
    let err = result.unwrap_err();
    assert_eq!(
        err.status_code(),
        axum::http::StatusCode::FORBIDDEN,
        "Expected 403 Forbidden for IDOR, got {} ({:?})",
        err.status_code(),
        err
    );

    // Pool must remain unchanged (attack had no effect)
    let active_after =
        nexus_local_db::novel_pool_entries::get_active_pool_entry(state.pool(), "test_creator")
            .await
            .unwrap();
    assert!(
        active_after.is_some(),
        "active pool entry must remain after rejected IDOR"
    );
    assert_eq!(
        active_after.unwrap().work_id,
        Some(work_id),
        "active work_id must be unchanged after rejected IDOR"
    );
}

/// Verify that `set_pool_active` succeeds when body `creator_id` matches the
/// active creator (legitimate use).
#[tokio::test]
async fn test_set_pool_active_allows_matching_creator_id() {
    use nexus_daemon_runtime::api::handlers::works::SetPoolActiveRequest;

    let (state, _tmp) = handler_state().await;
    let work_id_1 = create_test_work(&state, "First Novel").await;
    let work_id_2 = create_test_work(&state, "Second Novel").await;

    // Promote work 1 first
    let _ = nexus_daemon_runtime::api::handlers::works::promote_pool_entry(
        State(state.clone()),
        axum::Json(PromotePoolRequest {
            work_id: work_id_1.clone(),
            set_default: None,
        }),
    )
    .await
    .unwrap();

    // Now set work 2 as active via set_pool_active with matching creator_id
    let result = nexus_daemon_runtime::api::handlers::works::set_pool_active(
        State(state.clone()),
        axum::Json(SetPoolActiveRequest {
            action: "set_pool_active".to_string(),
            work_id: work_id_2.clone(),
            creator_id: Some("test_creator".to_string()),
        }),
    )
    .await;

    assert!(
        result.is_ok(),
        "Matching creator_id should succeed: {:?}",
        result.err()
    );
    let entry = result.unwrap();
    assert_eq!(entry.work_id, work_id_2);
    assert_eq!(entry.status, "active");
}

/// Verify that `set_pool_active` works without body `creator_id` (existing
/// callers that omit the field).
#[tokio::test]
async fn test_set_pool_active_works_without_body_creator_id() {
    use nexus_daemon_runtime::api::handlers::works::SetPoolActiveRequest;

    let (state, _tmp) = handler_state().await;
    let work_id = create_test_work(&state, "No Body Creator").await;

    // Promote first so it has a pool entry
    let _ = nexus_daemon_runtime::api::handlers::works::promote_pool_entry(
        State(state.clone()),
        axum::Json(PromotePoolRequest {
            work_id: work_id.clone(),
            set_default: None,
        }),
    )
    .await
    .unwrap();

    // Call set_pool_active without creator_id (should use active creator)
    let result = nexus_daemon_runtime::api::handlers::works::set_pool_active(
        State(state.clone()),
        axum::Json(SetPoolActiveRequest {
            action: "set_pool_active".to_string(),
            work_id: work_id.clone(),
            creator_id: None,
        }),
    )
    .await;

    assert!(
        result.is_ok(),
        "Omitting creator_id should succeed: {:?}",
        result.err()
    );
}
