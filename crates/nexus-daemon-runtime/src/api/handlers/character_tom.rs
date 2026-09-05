//! Character ToM HTTP handlers (v1.184 P4 Task 2).

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::require_creator;
use crate::character_tom::{
    record_input_from_request, CharacterTomListQuery, CharacterTomService,
};
use crate::workspace::WorkspaceState;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{DateTime, Utc};
use nexus_contracts::daemon_api::characters::tom::list_character_tom_query::ListCharacterTomQuery;
use nexus_contracts::daemon_api::characters::tom::list_character_tom_response::{
    ListCharacterTomResponse, NexusCharacterTomBeliefItem as ListedBeliefItem,
    NexusPaginationInfo as ListedPagination,
};
use nexus_contracts::daemon_api::characters::tom::record_character_tom_request::RecordCharacterTomRequest;
use nexus_contracts::daemon_api::characters::tom::record_character_tom_response::RecordCharacterTomResponse;
use nexus_knowledge::world_kb::knowledge_entry::parse_stored_created_at;
use serde::de::DeserializeOwned;
use serde::Serialize;

fn wire_err(err: impl std::fmt::Display) -> NexusApiError {
    NexusApiError::Internal {
        code: "CHARACTER_TOM_WIRE_INVALID".into(),
        message: err.to_string(),
    }
}

fn finish_builder<T, E: std::fmt::Display>(value: Result<T, E>) -> Result<T, NexusApiError> {
    value.map_err(wire_err)
}

fn map_wire<T: DeserializeOwned>(value: impl Serialize) -> Result<T, NexusApiError> {
    let json = serde_json::to_value(value).map_err(wire_err)?;
    serde_json::from_value(json).map_err(wire_err)
}

fn parse_canonical_json<T: DeserializeOwned>(bytes: &Bytes) -> Result<T, NexusApiError> {
    serde_json::from_slice(bytes).map_err(|err| NexusApiError::BadRequest {
        code: "invalid_input".into(),
        message: err.to_string(),
    })
}

fn parse_rfc3339(raw: &str) -> Result<DateTime<Utc>, NexusApiError> {
    parse_stored_created_at(raw).map_err(wire_err)
}

fn item_from_row(row: &crate::character_tom::CharacterTomBeliefRow) -> Result<serde_json::Value, NexusApiError> {
    let mut value = serde_json::json!({
        "carrier_entry_id": row.carrier_entry_id,
        "row_ordinal": i64::from(row.row_ordinal),
        "holder": row.belief.holder,
        "proposition": row.belief.proposition,
        "order": row.belief.order,
        "truth": row.belief.truth,
        "access": row.belief.access,
        "representation": row.belief.representation,
        "content_type": row.belief.content_type,
        "source": row.belief.source,
        "context": row.belief.context,
    });
    if let Some(raw) = &row.carrier_recorded_at {
        value["carrier_recorded_at"] = serde_json::json!(parse_rfc3339(raw)?);
    }
    Ok(value)
}

fn list_pagination(page: &crate::character_tom::CharacterTomPage) -> Result<ListedPagination, NexusApiError> {
    finish_builder(
        ListedPagination::builder()
            .limit(i64::from(page.limit))
            .has_more(page.has_more)
            .next_cursor(page.next_cursor.clone())
            .try_into(),
    )
}

/// `POST /v1/daemon/characters/{character_id}/tom`
pub async fn record_tom(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    body: Bytes,
) -> Result<Json<RecordCharacterTomResponse>, NexusApiError> {
    let req: RecordCharacterTomRequest = parse_canonical_json(&body)?;
    let owner = require_creator(&state)?;
    let expected_revision = i64::try_from(req.expected_revision).map_err(|_| NexusApiError::BadRequest {
        code: "invalid_input".into(),
        message: "expected_revision is out of range".into(),
    })?;
    let input = record_input_from_request(&req, expected_revision)?;
    let service = CharacterTomService::new(state.pool_or_uninit()?.clone());
    let (carrier_entry_id, revision, mind_state_id) = service
        .record(&owner, &character_id, input)
        .await?;
    Ok(Json(finish_builder(
        RecordCharacterTomResponse::builder()
            .carrier_entry_id(carrier_entry_id)
            .mind_state_id(mind_state_id)
            .revision(revision)
            .try_into(),
    )?))
}

/// `GET /v1/daemon/characters/{character_id}/tom`
pub async fn list_tom(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    Query(query): Query<ListCharacterTomQuery>,
) -> Result<Json<ListCharacterTomResponse>, NexusApiError> {
    let owner = require_creator(&state)?;
    let service = CharacterTomService::new(state.pool_or_uninit()?.clone());
    let page = service
        .list(
            &owner,
            &character_id,
            CharacterTomListQuery {
                world_id: query.world_id.to_string(),
                binding_id: query.binding_id.to_string(),
                limit: CharacterTomService::resolve_limit(query.limit)?,
                cursor: query.cursor.clone(),
                order: None,
            },
        )
        .await?;
    let items: Vec<ListedBeliefItem> = page
        .items
        .iter()
        .map(|row| map_wire(item_from_row(row)?))
        .collect::<Result<_, _>>()?;
    Ok(Json(finish_builder(
        ListCharacterTomResponse::builder()
            .items(items)
            .pagination(list_pagination(&page)?)
            .try_into(),
    )?))
}
