//! Character `ToM` HTTP handlers (v1.184 P4 Task 2) — thin translation over
//! the core Character `ToM` carrier family (v1.190 P2-T2). The bounded
//! carrier probes, atomic CAS + derivative `MindState` and keyset pagination
//! live in [`nexus_core`]; the handlers keep only auth resolution, wire
//! parsing and status/envelope translation. ToM record is a mutation and
//! holds the core per-Character activity lease inside the core call.

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::resolve_core_principal;
use crate::workspace::WorkspaceState;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::Json;
use nexus_contracts::daemon_api::characters::tom::list_character_tom_query::ListCharacterTomQuery;
use nexus_contracts::daemon_api::characters::tom::list_character_tom_response::ListCharacterTomResponse;
use nexus_contracts::daemon_api::characters::tom::record_character_tom_request::RecordCharacterTomRequest;
use nexus_contracts::daemon_api::characters::tom::record_character_tom_response::RecordCharacterTomResponse;

fn parse_canonical_json<T: serde::de::DeserializeOwned>(bytes: &Bytes) -> Result<T, NexusApiError> {
    serde_json::from_slice(bytes).map_err(|err| NexusApiError::BadRequest {
        code: "invalid_input".into(),
        message: err.to_string(),
    })
}

/// `POST /v1/daemon/characters/{character_id}/tom`
pub async fn record_tom(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    body: Bytes,
) -> Result<Json<RecordCharacterTomResponse>, NexusApiError> {
    let req: RecordCharacterTomRequest = parse_canonical_json(&body)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core
        .record_character_tom(&principal, character_id, req)
        .await?;
    Ok(Json(response))
}

/// `GET /v1/daemon/characters/{character_id}/tom`
pub async fn list_tom(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    Query(query): Query<ListCharacterTomQuery>,
) -> Result<Json<ListCharacterTomResponse>, NexusApiError> {
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core
        .list_character_tom(&principal, character_id, query)
        .await?;
    Ok(Json(response))
}
