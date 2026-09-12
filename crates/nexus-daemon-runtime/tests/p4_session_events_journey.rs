//! P4 T4 — in-process daemon public route journey for session events SSE.
//!
//! Uses the production `create_router` + `publish_creator_runtime_bundle` wiring
//! so `session_events` exercises the real engine/checkpoint ownership path and
//! the workspace-scoped `RunEventRegistry` (not a handler-only stub).

#![allow(clippy::unwrap_used)]

use axum::http::StatusCode;
use axum_test::TestServer;
use nexus_agent_host::config::{AgentHostConfig, ProviderConfig};
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::run_events::RunEventRegistry;
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::test_utils::TestTempRoot;
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_orchestration::engine::{SessionId, SessionStatus};
use std::collections::HashMap;
use std::path::PathBuf;

const TEST_BINDING_PROVIDER: &str = "test-provider";

struct EventsTestCtx {
    _tmp: TestTempRoot,
    nexus_home: PathBuf,
    db_path: PathBuf,
    state: WorkspaceState,
    pool: sqlx::SqlitePool,
}

fn production_agent_host_config() -> AgentHostConfig {
    AgentHostConfig {
        providers: vec![ProviderConfig {
            id: TEST_BINDING_PROVIDER.to_string(),
            protocol: "native_cli".to_string(),
            command: Some("mock".to_string()),
            args: vec![],
            env: HashMap::new(),
            enabled: true,
        }],
        ..AgentHostConfig::default()
    }
}

async fn publish_production_bundle(state: &WorkspaceState) {
    state
        .publish_creator_runtime_bundle()
        .await
        .expect("publish creator runtime bundle");
    assert!(
        state.engine().is_some(),
        "session_events requires the production engine bundle"
    );
}

async fn new_events_test_ctx() -> EventsTestCtx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let mut state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path.clone(), None).await;
    test_utils::seed_test_creator_and_world(state.pool().unwrap()).await;
    state.set_agent_host_config(production_agent_host_config());
    publish_production_bundle(&state).await;
    let pool = state.pool().expect("workspace pool").clone();
    EventsTestCtx {
        _tmp: tmp,
        nexus_home,
        db_path,
        state,
        pool,
    }
}

async fn restart_workspace_state(ctx: &EventsTestCtx) -> WorkspaceState {
    let mut state =
        WorkspaceState::new_for_testing(ctx.nexus_home.clone(), ctx.db_path.clone(), None).await;
    state.set_agent_host_config(production_agent_host_config());
    publish_production_bundle(&state).await;
    state
}

fn test_server(state: WorkspaceState) -> TestServer {
    TestServer::new(api::create_router(state, DaemonApiConfig::keyless()))
}

async fn seed_terminal_session(pool: &sqlx::SqlitePool, session_id: &str, state_revision: i64) {
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, status,
             current_task_id, context_json, created_at, updated_at,
             execution_version, state_revision, graph_version)
         VALUES (?, 'test_creator', 'novel-writing', 1, 'completed',
                 NULL, '{}', 1, 2, 1, ?, 1)",
    )
    .bind(session_id)
    .bind(state_revision)
    .execute(pool)
    .await
    .expect("seed durable session row");
}

fn publish_terminal_run_state(registry: &RunEventRegistry, run_id: &str, state_revision: u64) {
    let _sink = registry.try_register_live(run_id).expect("register live ring");
    registry.publish_run_state(
        run_id,
        &nexus_orchestration::run_state::RunRecord {
            session_id: SessionId(run_id.to_string()),
            status: SessionStatus::Completed,
            state_revision,
            execution_version: 1,
            descriptor: None,
            state: None,
            graph_version: 1,
        },
    );
    registry.mark_terminal(run_id);
}

#[tokio::test]
async fn session_events_history_unavailable_for_unknown_run() {
    let ctx = new_events_test_ctx().await;
    let server = test_server(ctx.state);
    let resp = server
        .get("/v1/daemon/orchestration/sessions/missing-run/events")
        .await;
    assert_eq!(
        resp.status_code(),
        StatusCode::NOT_FOUND,
        "unknown durable run must 404 after engine ownership check, not 503: {}",
        resp.text()
    );
}

#[tokio::test]
async fn session_events_live_replay_and_terminal_close() {
    let ctx = new_events_test_ctx().await;
    let run_id = "p4-sse-journey";
    publish_terminal_run_state(&ctx.state.run_event_registry(), run_id, 2);
    seed_terminal_session(&ctx.pool, run_id, 2).await;
    let server = test_server(ctx.state);
    let resp = server
        .get(&format!("/v1/daemon/orchestration/sessions/{run_id}/events"))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "{}", resp.text());
    let body = resp.text();
    assert!(
        body.contains("event: run_state"),
        "SSE must replay terminal run_state from the production registry: {body}"
    );
}

#[tokio::test]
async fn session_events_restart_yields_history_unavailable_after_ring_eviction() {
    let ctx = new_events_test_ctx().await;
    let run_id = "p4-restart-gap";
    publish_terminal_run_state(&ctx.state.run_event_registry(), run_id, 1);
    seed_terminal_session(&ctx.pool, run_id, 1).await;

    let restarted = restart_workspace_state(&ctx).await;
    let server = test_server(restarted);
    let resp = server
        .get(&format!("/v1/daemon/orchestration/sessions/{run_id}/events"))
        .await;
    assert_eq!(
        resp.status_code(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "history_unavailable maps to 422 via NexusApiError semantic coded BadRequest: {}",
        resp.text()
    );
    let json = resp.json::<serde_json::Value>();
    assert_eq!(json["error"]["code"].as_str(), Some("history_unavailable"));
    let inspect_url = format!("/v1/daemon/orchestration/sessions/{run_id}");
    assert_eq!(
        json["error"]["details"]["inspect_url"].as_str(),
        Some(inspect_url.as_str())
    );
}

async fn seed_foreign_terminal_session(pool: &sqlx::SqlitePool, session_id: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data)
         VALUES ('other_creator', 'Other', 'active', datetime('now'), '{}')",
    )
    .execute(pool)
    .await
    .expect("seed foreign creator");
    sqlx::query(
        "INSERT INTO orchestration_sessions
            (session_id, creator_id, preset_id, preset_version, status,
             current_task_id, context_json, created_at, updated_at,
             execution_version, state_revision, graph_version)
         VALUES (?, 'other_creator', 'novel-writing', 1, 'completed',
                 NULL, '{}', 1, 2, 1, 1, 1)",
    )
    .bind(session_id)
    .execute(pool)
    .await
    .expect("seed foreign session");
}

#[tokio::test]
async fn session_events_foreign_owner_returns_404() {
    let ctx = new_events_test_ctx().await;
    let run_id = "p4-foreign-owner";
    publish_terminal_run_state(&ctx.state.run_event_registry(), run_id, 1);
    seed_foreign_terminal_session(&ctx.pool, run_id).await;
    let server = test_server(ctx.state);
    let resp = server
        .get(&format!("/v1/daemon/orchestration/sessions/{run_id}/events"))
        .await;
    assert_eq!(
        resp.status_code(),
        StatusCode::NOT_FOUND,
        "foreign-owned session must 404 via owner path, not existence-only: {}",
        resp.text()
    );
}

#[tokio::test]
async fn session_events_replay_exceeds_pending_cap_without_subscriber_error() {
    let ctx = new_events_test_ctx().await;
    let run_id = "p4-replay-cap";
    let _sink = ctx
        .state
        .run_event_registry()
        .try_register_live(run_id)
        .expect("register");
    for i in 0..20 {
        ctx.state.run_event_registry().publish_run_state(
            run_id,
            &nexus_orchestration::run_state::RunRecord {
                session_id: SessionId(run_id.to_string()),
                status: SessionStatus::Running,
                state_revision: i as u64 + 1,
                execution_version: 1,
                descriptor: None,
                state: None,
                graph_version: 1,
            },
        );
    }
    seed_terminal_session(&ctx.pool, run_id, 20).await;
    ctx.state.run_event_registry().mark_terminal(run_id);
    let server = test_server(ctx.state);
    let resp = server
        .get(&format!("/v1/daemon/orchestration/sessions/{run_id}/events"))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "{}", resp.text());
    let body = resp.text();
    let run_state_count = body.matches("event: run_state").count();
    assert!(
        run_state_count >= 20,
        "replay must deliver all retained frames, got {run_state_count}: {body}"
    );
}
