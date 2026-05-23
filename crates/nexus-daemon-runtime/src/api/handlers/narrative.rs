//! Narrative read surface handlers (V1.25 Theme C, C1.1 → V1.26 persistence).
//!
//! Read-only daemon routes backed by `NarrativeGateway` with
//! `SQLite` persistence via `SqliteNarrativeGateway`.
//!
//! # Endpoints
//!
//! - `GET /v1/local/narrative/worlds` — list all worlds
//! - `GET /v1/local/narrative/worlds/{world_id}` — get a single world state
//!
//! These are **narrative state** routes, distinct from the work-scope
//! `/v1/local/kb/*` file-index routes.

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use nexus_narrative::{NarrativeGateway, WorldState};
use serde::Serialize;

// ─── Response types ────────────────────────────────────────────────────────

/// `GET /v1/local/narrative/worlds` response.
#[derive(Debug, Serialize)]
pub struct ListWorldsResponse {
    pub worlds: Vec<WorldState>,
}

/// `GET /v1/local/narrative/worlds/{world_id}` response.
#[derive(Debug, Serialize)]
pub struct GetWorldResponse {
    pub world: WorldState,
}

// ─── Handlers ──────────────────────────────────────────────────────────────

/// `GET /v1/local/narrative/worlds` — list all worlds.
///
/// Returns worlds from the persistent `SQLite` gateway. Empty list when
/// no worlds have been seeded into the database.
pub async fn list_worlds(
    State(state): State<WorkspaceState>,
) -> Result<Json<ListWorldsResponse>, NexusApiError> {
    let gateway = state.narrative_gateway();
    let worlds = gateway
        .list_worlds()
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "NARRATIVE_ERROR".into(),
            message: e.to_string(),
        })?;
    Ok(Json(ListWorldsResponse { worlds }))
}

/// `GET /v1/local/narrative/worlds/{world_id}` — get a single world state.
///
/// Returns 404 for an unknown world ID. Returns the projected world
/// state for a known world from the persistent gateway.
pub async fn get_world(
    State(state): State<WorkspaceState>,
    Path(world_id): Path<String>,
) -> Result<Json<GetWorldResponse>, NexusApiError> {
    let gateway = state.narrative_gateway();
    let world = gateway
        .get_world_state(&world_id)
        .await
        .map_err(|e| match e {
            nexus_narrative::NarrativeError::ValidationError(msg) if msg.contains("not found") => {
                NexusApiError::NotFound(format!("World {world_id} not found"))
            }
            _ => NexusApiError::Internal {
                code: "NARRATIVE_ERROR".into(),
                message: e.to_string(),
            },
        })?;
    Ok(Json(GetWorldResponse { world }))
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_worlds_returns_empty_for_fresh_gateway() {
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let gateway = state.narrative_gateway();
        let worlds = gateway.list_worlds().await.unwrap();
        assert!(worlds.is_empty());
        drop(state);
        drop(tmp);
    }

    #[tokio::test]
    async fn get_world_state_returns_error_for_missing() {
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let gateway = state.narrative_gateway();
        let result = gateway.get_world_state("nonexistent").await;
        assert!(result.is_err());
        drop(state);
        drop(tmp);
    }

    #[tokio::test]
    async fn get_world_state_returns_world_when_seeded() {
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        // Seed a world directly into the DB
        crate::db::narrative_gateway::seed::world(
            state.pool(),
            "wld_test",
            "ctr_test",
            "Test",
            "test",
            "private",
            "manual",
        )
        .await;

        let gateway = state.narrative_gateway();
        let s = gateway.get_world_state("wld_test").await.unwrap();
        assert_eq!(s.world_id, "wld_test");
        assert_eq!(s.title, "Test");
        drop(state);
        drop(tmp);
    }
}
