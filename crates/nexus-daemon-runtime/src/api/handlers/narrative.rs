//! Narrative read surface handlers (V1.25 Theme C, C1.1).
//!
//! Minimal read-only daemon routes backed by `NarrativeGateway` with
//! in-memory stores. **No persistence across daemon restarts.**
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
use nexus_narrative::{InMemoryNarrativeGateway, NarrativeGateway, WorldState};
use nexus_kb::InMemoryKbStore;
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

/// `GET /v1/local/narrative/worlds` — list all worlds (in-memory, read-only).
///
/// Returns an empty list when no worlds have been seeded into the gateway.
/// The response shape is stable; persistence will be added in a future iteration.
pub async fn list_worlds(
    State(_state): State<WorkspaceState>,
) -> Result<Json<ListWorldsResponse>, NexusApiError> {
    let gateway = InMemoryNarrativeGateway::new(InMemoryKbStore::new());
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
/// state (including fork info) for a known world.
pub async fn get_world(
    State(_state): State<WorkspaceState>,
    Path(world_id): Path<String>,
) -> Result<Json<GetWorldResponse>, NexusApiError> {
    let gateway = InMemoryNarrativeGateway::new(InMemoryKbStore::new());
    let world = gateway
        .get_world_state(&world_id)
        .await
        .map_err(|e| match e {
            nexus_narrative::NarrativeError::ValidationError(msg)
                if msg.contains("not found") =>
            {
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
        let gateway = InMemoryNarrativeGateway::new(InMemoryKbStore::new());
        let worlds = gateway.list_worlds().await.unwrap();
        assert!(worlds.is_empty());
    }

    #[tokio::test]
    async fn get_world_state_returns_error_for_missing() {
        let gateway = InMemoryNarrativeGateway::new(InMemoryKbStore::new());
        let result = gateway.get_world_state("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_world_state_returns_world_when_seeded() {
        use nexus_contracts::{TimePolicy, Visibility};
        use nexus_narrative::world::World;

        let gateway = InMemoryNarrativeGateway::new(InMemoryKbStore::new());
        let world =
            World::new("wld_test", "ctr_test", "Test", "test", Visibility::Private, TimePolicy::Manual);
        gateway.insert_world(world);

        let state = gateway.get_world_state("wld_test").await.unwrap();
        assert_eq!(state.world_id, "wld_test");
        assert_eq!(state.title, "Test");
    }
}
