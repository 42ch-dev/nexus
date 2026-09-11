//! P4 T4 — in-process daemon public route journey for session events SSE.

#![allow(clippy::unwrap_used)]

use axum::http::StatusCode;
use axum_test::TestServer;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_orchestration::engine::{SessionId, SessionStatus};

#[tokio::test]
async fn session_events_history_unavailable_for_unknown_run() {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let _tmp = tmp;
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path.clone(), None).await;
    let server = TestServer::new(api::create_router(state, DaemonApiConfig::keyless()));
    let resp = server
        .get("/v1/daemon/orchestration/sessions/missing-run/events")
        .await;
    assert_eq!(resp.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn session_events_live_replay_and_terminal_close() {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let _tmp = tmp;
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path.clone(), None).await;
    test_utils::seed_test_creator_and_world(state.pool().unwrap()).await;
    let registry = state.run_event_registry();
    let run_id = "p4-sse-journey";
    let _sink = registry.try_register_live(run_id).expect("register ring");
    registry.publish_run_state(
        run_id,
        &nexus_orchestration::run_state::RunRecord {
            session_id: SessionId(run_id.to_string()),
            status: SessionStatus::Completed,
            state_revision: 2,
            execution_version: 1,
            descriptor: None,
            state: None,
            graph_version: 1,
        },
    );
    registry.mark_terminal(run_id);
    let pool = state.pool().expect("pool");
    sqlx::query(
        "INSERT INTO orchestration_sessions (session_id, creator_id, preset_id, status, execution_version, state_revision, graph_version)
         VALUES (?, 'test_creator', 'novel-writing', 'completed', 1, 2, 1)",
    )
    .bind(run_id)
    .execute(pool)
    .await
    .expect("seed session row");
    let server = TestServer::new(api::create_router(state, DaemonApiConfig::keyless()));
    let resp = server
        .get(&format!("/v1/daemon/orchestration/sessions/{run_id}/events"))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK);
    let body = resp.text();
    assert!(body.contains("event: run_state"), "SSE must replay run_state: {body}");
}

#[tokio::test]
async fn session_events_restart_yields_history_unavailable_after_ring_eviction() {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let _tmp = tmp;
    let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path.clone(), None).await;
    test_utils::seed_test_creator_and_world(state.pool().unwrap()).await;
    let registry = state.run_event_registry();
    let run_id = "p4-restart-gap";
    let _sink = registry.try_register_live(run_id).expect("register");
    registry.publish_run_state(
        run_id,
        &nexus_orchestration::run_state::RunRecord {
            session_id: SessionId(run_id.to_string()),
            status: SessionStatus::Completed,
            state_revision: 1,
            execution_version: 1,
            descriptor: None,
            state: None,
            graph_version: 1,
        },
    );
    registry.mark_terminal(run_id);
    let pool = state.pool().expect("pool");
    sqlx::query(
        "INSERT INTO orchestration_sessions (session_id, creator_id, preset_id, status, execution_version, state_revision, graph_version)
         VALUES (?, 'test_creator', 'novel-writing', 'completed', 1, 1, 1)",
    )
    .bind(run_id)
    .execute(pool)
    .await
    .expect("seed session row");
    let restarted = WorkspaceState::new_for_testing(nexus_home.clone(), db_path.clone(), None).await;
    let server = TestServer::new(api::create_router(restarted, DaemonApiConfig::keyless()));
    let resp = server
        .get(&format!("/v1/daemon/orchestration/sessions/{run_id}/events"))
        .await;
    assert_eq!(resp.status_code(), StatusCode::BAD_REQUEST);
    let json = resp.json::<serde_json::Value>();
    assert_eq!(json["error"]["code"].as_str(), Some("history_unavailable"));
    assert!(json["error"]["details"]["inspect_url"].is_string());
}
