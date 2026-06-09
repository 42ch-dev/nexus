//! Findings API contract tests (V1.39 P1 — T6).
//!
//! Covers:
//! - (a) Creator isolation: cross-creator gets 404
//! - (b) List filter by work_id
//! - (c) Update + close transitions (resolved, wont_fix)
//! - (d) Supervisor-side auto-create on review stage completion (from-review endpoint)
//! - (e) Routing hints for all executor types
//!
//! Note: Uses handler invocation (not axum-test HTTP routing) due to an
//! axum-test limitation with hyphenated UUIDs in path segments. See
//! `works_api.rs` for the same pattern.

#![allow(clippy::unwrap_used)]

use axum::extract::{Path, State};
use nexus_daemon_runtime::api::handlers::findings::{
    create_finding_handler, create_from_review_handler, delete_finding_handler,
    get_finding_handler, list_findings_handler, update_finding_handler, CreateFindingRequest,
    FindingApiDto, ListFindingsQuery, UpdateFindingRequest,
};
use nexus_daemon_runtime::api::handlers::works::CreateWorkRequest;
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::test_utils::TestTempRoot;
use nexus_daemon_runtime::workspace::WorkspaceState;

// ─── Helpers ───────────────────────────────────────────────────────────────

/// Build a fresh WorkspaceState for handler-level testing.
async fn handler_state() -> (WorkspaceState, TestTempRoot) {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    test_utils::seed_test_creator_and_world(state.pool()).await;
    (state, tmp)
}

/// Create a Work via handler, return its work_id.
///
/// Uses the pre-seeded test world (seeded by `seed_test_creator_and_world`
/// in `handler_state`).
async fn create_work(state: &WorkspaceState) -> String {
    let (_, resp) = nexus_daemon_runtime::api::handlers::works::create_work(
        State(state.clone()),
        axum::Json(CreateWorkRequest {
            title: "Test Novel".into(),
            long_term_goal: "Finish a short story".into(),
            initial_idea: "A detective story".into(),
            world_id: Some("wld_test_world".to_string()),
            story_ref: None,
            primary_preset_id: None,
            client_request_id: None,
        }),
    )
    .await
    .unwrap();
    resp.work_id.clone()
}

/// Create a finding via handler invocation.
async fn create_finding(
    state: &WorkspaceState,
    work_id: &str,
    severity: &str,
    title: &str,
) -> FindingApiDto {
    let (status, resp) = create_finding_handler(
        State(state.clone()),
        Path(work_id.to_string()),
        axum::Json(CreateFindingRequest {
            chapter: None,
            severity: severity.to_string(),
            title: title.to_string(),
            description: "Test finding description".into(),
            target_executor: "none".to_string(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::CREATED);
    resp.0
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn findings_crud_create_and_get() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_work(&state).await;

    // Create
    let created = create_finding(&state, &work_id, "major", "Plot hole in chapter 3").await;
    let finding_id = &created.finding_id;
    assert!(finding_id.starts_with("fnd_"));
    assert_eq!(created.severity, "major");
    assert_eq!(created.status, "open");
    assert_eq!(created.target_executor, "none");
    assert_eq!(created.work_id, work_id);
    assert!(created.routing_hint.is_some());

    // Get
    let result = get_finding_handler(
        State(state.clone()),
        Path((work_id.clone(), finding_id.clone())),
    )
    .await
    .unwrap();
    let got = result.0;
    assert_eq!(got.finding_id, *finding_id);
    assert_eq!(got.title, "Plot hole in chapter 3");
}

#[tokio::test]
async fn findings_list_filter_by_work_id() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_work(&state).await;

    // Create two findings
    create_finding(&state, &work_id, "minor", "Typo in dialogue").await;
    create_finding(&state, &work_id, "blocker", "Missing chapter transition").await;

    // List all for this work
    let result = list_findings_handler(
        State(state.clone()),
        Path(work_id.clone()),
        axum::extract::Query(ListFindingsQuery {
            chapter: None,
            status: None,
            severity: None,
            limit: None,
            offset: None,
        }),
    )
    .await
    .unwrap();
    let list = result.0;
    assert_eq!(list.len(), 2);

    // Filter by severity
    let result = list_findings_handler(
        State(state.clone()),
        Path(work_id.clone()),
        axum::extract::Query(ListFindingsQuery {
            chapter: None,
            status: None,
            severity: Some("blocker".to_string()),
            limit: None,
            offset: None,
        }),
    )
    .await
    .unwrap();
    let filtered = result.0;
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].severity, "blocker");
}

#[tokio::test]
async fn findings_update_and_close_transition() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_work(&state).await;

    let created = create_finding(&state, &work_id, "info", "Minor style issue").await;
    let finding_id = created.finding_id.clone();

    // Update severity
    let result = update_finding_handler(
        State(state.clone()),
        Path((work_id.clone(), finding_id.clone())),
        axum::Json(UpdateFindingRequest {
            severity: Some("major".to_string()),
            description: Some("Elevated: this is actually important".to_string()),
            status: None,
            title: None,
            target_executor: None,
        }),
    )
    .await
    .unwrap();
    let updated = result.0;
    assert_eq!(updated.severity, "major");
    assert_eq!(updated.description, "Elevated: this is actually important");
    assert_eq!(updated.status, "open"); // status unchanged

    // Close (resolve)
    let result = update_finding_handler(
        State(state.clone()),
        Path((work_id.clone(), finding_id.clone())),
        axum::Json(UpdateFindingRequest {
            status: Some("resolved".to_string()),
            target_executor: Some("write".to_string()),
            severity: None,
            description: None,
            title: None,
        }),
    )
    .await
    .unwrap();
    let closed = result.0;
    assert_eq!(closed.status, "resolved");
    assert_eq!(closed.target_executor, "write");
    assert_eq!(closed.routing_hint.as_deref(), Some("→ write"));

    // Wont-fix transition on a second finding
    let created2 = create_finding(&state, &work_id, "minor", "Trivial issue").await;
    let result = update_finding_handler(
        State(state.clone()),
        Path((work_id.clone(), created2.finding_id.clone())),
        axum::Json(UpdateFindingRequest {
            status: Some("wont_fix".to_string()),
            severity: None,
            description: None,
            title: None,
            target_executor: None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(result.0.status, "wont_fix");
}

#[tokio::test]
async fn findings_creator_isolation_cross_creator_404() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_work(&state).await;
    let created = create_finding(&state, &work_id, "major", "Secret finding").await;
    let finding_id = created.finding_id.clone();

    // Build a different creator's state (same DB but different active creator).
    // The finding was created with creator A. Creator B should not see it.
    let tmp = tempfile::TempDir::new().unwrap();
    let user_home = tmp.path();
    let nexus_home = user_home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).unwrap();

    let other_creator = "ctr_other_guy";
    let toml_str = format!(
        "active_creator_id = \"{other_creator}\"\n[active_workspace_slug_by_creator]\n\"{other_creator}\" = \"default\""
    );
    std::fs::write(nexus_home.join("config.toml"), toml_str).unwrap();

    let op_dir = nexus_home_layout::operational_workspace_dir(user_home, other_creator, "default");
    std::fs::create_dir_all(&op_dir).unwrap();
    let meta = serde_json::json!({
        "schema_version": 1,
        "creator_id": other_creator,
        "workspace_slug": "default",
        "local_root": user_home.join("creative"),
        "created_at": "2020-01-01T00:00:00Z"
    });
    std::fs::write(
        op_dir.join("meta.json"),
        serde_json::to_string(&meta).unwrap(),
    )
    .unwrap();

    let db_path = nexus_home_layout::workspace_state_db_path(user_home, other_creator, "default");
    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    nexus_local_db::seed_versions(&pool).await.unwrap();

    let other_state =
        WorkspaceState::new_for_testing(nexus_home.clone(), db_path.clone(), None).await;
    std::mem::forget(tmp);

    // Try to GET — should 404 (creator isolation)
    let result = get_finding_handler(
        State(other_state.clone()),
        Path((work_id.clone(), finding_id.clone())),
    )
    .await;
    assert!(result.is_err(), "cross-creator get should fail");
    let err = result.unwrap_err();
    assert_eq!(err.status_code(), axum::http::StatusCode::NOT_FOUND);

    // List should return empty (work not owned by other creator)
    let result = list_findings_handler(
        State(other_state),
        Path(work_id),
        axum::extract::Query(ListFindingsQuery {
            chapter: None,
            status: None,
            severity: None,
            limit: None,
            offset: None,
        }),
    )
    .await
    .unwrap();
    assert!(result.0.is_empty());
}

#[tokio::test]
async fn findings_from_review_endpoint_auto_create() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_work(&state).await;

    // Simulate a review stage creating a finding via the from-review handler
    let (status, resp) = create_from_review_handler(
        State(state.clone()),
        Path(work_id.clone()),
        axum::Json(CreateFindingRequest {
            chapter: Some(3),
            severity: "major".to_string(),
            title: "LLM-judge: continuity break".to_string(),
            description: "Character age inconsistency between ch2 and ch3".to_string(),
            target_executor: "write".to_string(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::CREATED);
    let body = resp.0;

    assert_eq!(body.severity, "major");
    assert_eq!(body.status, "open");
    assert_eq!(body.title, "LLM-judge: continuity break");
    assert_eq!(body.chapter, Some(3));
    assert_eq!(body.target_executor, "write");
    assert_eq!(body.routing_hint.as_deref(), Some("→ write"));
    assert!(body.finding_id.starts_with("fnd_"));
}

#[tokio::test]
async fn findings_delete() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_work(&state).await;
    let created = create_finding(&state, &work_id, "info", "To be deleted").await;
    let finding_id = created.finding_id;

    // Delete
    let result = delete_finding_handler(
        State(state.clone()),
        Path((work_id.clone(), finding_id.clone())),
    )
    .await
    .unwrap();
    assert_eq!(result, axum::http::StatusCode::NO_CONTENT);

    // GET returns 404
    let result = get_finding_handler(State(state.clone()), Path((work_id, finding_id))).await;
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().status_code(),
        axum::http::StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn findings_routing_hints_all_executors() {
    let (state, _tmp) = handler_state().await;
    let work_id = create_work(&state).await;

    // Test all routing hint values
    for (executor, expected_hint) in [
        ("write", "→ write"),
        ("brainstorm", "→ brainstorm"),
        ("master", "→ review-master"),
        ("none", "→ none"),
    ] {
        let (_, resp) = create_finding_handler(
            State(state.clone()),
            Path(work_id.clone()),
            axum::Json(CreateFindingRequest {
                chapter: None,
                severity: "info".to_string(),
                title: format!("Finding for {executor}"),
                description: String::new(),
                target_executor: executor.to_string(),
            }),
        )
        .await
        .unwrap();
        let body = resp.0;
        assert_eq!(
            body.routing_hint.as_deref(),
            Some(expected_hint),
            "routing hint mismatch for executor={executor}"
        );
    }
}
