//! Character identity and ActorWorldBinding Daemon API handlers (v1.184 P0).
//!
//! Active-Creator admission is stored-config only; request bodies never carry
//! `owner_creator_id`. Responses are generated DTOs.

#![allow(clippy::missing_errors_doc)]

use super::wire_cast;
use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::require_creator;
use crate::api::pagination::{decode_offset_cursor, offset_page_meta};
use crate::workspace::WorkspaceState;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::de::DeserializeOwned;
use nexus_contracts::daemon_api::characters::{
    add_character_binding_request::AddCharacterBindingRequest,
    add_character_binding_response::AddCharacterBindingResponse,
    character_detail::CharacterDetail, create_character_request::CreateCharacterRequest,
    create_character_response::CreateCharacterResponse,
    list_character_bindings_query::ListCharacterBindingsQuery,
    list_character_bindings_response::ListCharacterBindingsResponse,
    list_characters_query::ListCharactersQuery, list_characters_response::ListCharactersResponse,
};
use nexus_local_db::{
    actor_world_binding::ActorWorldBindingRecord, character::CharacterRecord, CreateBindingParams,
    CreateCharacterParams,
};

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 100;

fn resolve_limit(raw: Option<i64>) -> Result<u32, NexusApiError> {
    match raw {
        None => Ok(DEFAULT_LIMIT),
        Some(n) if n > 0 && n <= i64::from(MAX_LIMIT) => u32::try_from(n).map_err(|_| {
            NexusApiError::BadRequest {
                code: "invalid_input".into(),
                message: "limit is out of range".into(),
            }
        }),
        Some(_) => Err(NexusApiError::BadRequest {
            code: "invalid_input".into(),
            message: format!("limit must be between 1 and {MAX_LIMIT}"),
        }),
    }
}

fn character_json(record: &CharacterRecord) -> Result<serde_json::Value, NexusApiError> {
    let persona: serde_json::Value = serde_json::from_str(&record.persona_json).map_err(|e| {
        NexusApiError::Internal {
            code: "CHARACTER_PERSONA_INVALID".into(),
            message: e.to_string(),
        }
    })?;
    let mut value = serde_json::json!({
        "schema_version": 1,
        "character_id": record.character_id,
        "owner_creator_id": record.owner_creator_id,
        "display_name": record.display_name,
        "status": record.status,
        "persona": persona,
        "created_at": record.created_at,
        "updated_at": record.updated_at,
    });
    if let Some(uri) = &record.image_uri {
        value["image_uri"] = serde_json::Value::String(uri.clone());
    }
    Ok(value)
}

fn binding_json(record: &ActorWorldBindingRecord) -> serde_json::Value {
    let mut value = serde_json::json!({
        "schema_version": 1,
        "binding_id": record.binding_id,
        "character_id": record.character_id,
        "world_id": record.world_id,
        "status": record.status,
        "created_at": record.created_at,
        "updated_at": record.updated_at,
    });
    if let Some(sheet) = &record.world_sheet_entry_id {
        value["world_sheet_entry_id"] = serde_json::Value::String(sheet.clone());
    }
    value
}

fn persona_json_string(map: &serde_json::Map<String, serde_json::Value>) -> String {
    serde_json::Value::Object(map.clone()).to_string()
}


fn parse_canonical_json<T: DeserializeOwned>(bytes: &Bytes) -> Result<T, NexusApiError> {
    serde_json::from_slice(bytes).map_err(|err| NexusApiError::BadRequest {
        code: "invalid_input".into(),
        message: err.to_string(),
    })
}

/// `POST /v1/daemon/characters`
pub async fn create_character(
    State(state): State<WorkspaceState>,
    body: Bytes,
) -> Result<(StatusCode, Json<CreateCharacterResponse>), NexusApiError> {
    let req: CreateCharacterRequest = parse_canonical_json(&body)?;
    let owner = require_creator(&state)?;
    let persona = persona_json_string(&req.persona);
    let created = nexus_local_db::create_character_with_initial_binding(
        state.pool_or_uninit()?,
        CreateCharacterParams {
            owner_creator_id: &owner,
            display_name: req.display_name.as_str(),
            image_uri: req.image_uri.as_deref(),
            persona_json: &persona,
            world_id: req.world_id.as_str(),
            world_sheet_entry_id: req.world_sheet_entry_id.as_deref(),
        },
    )
    .await?;
    let body = wire_cast(serde_json::json!({
        "character": character_json(&created.character)?,
        "binding": binding_json(&created.binding),
    }));
    Ok((StatusCode::CREATED, Json(body)))
}

/// `GET /v1/daemon/characters`
pub async fn list_characters(
    State(state): State<WorkspaceState>,
    Query(query): Query<ListCharactersQuery>,
) -> Result<Json<ListCharactersResponse>, NexusApiError> {
    let owner = require_creator(&state)?;
    let offset = decode_offset_cursor(&query.cursor)?;
    let limit = resolve_limit(query.limit)?;
    let all = nexus_local_db::list_characters(state.pool_or_uninit()?, &owner).await?;
    let start = usize::try_from(offset).unwrap_or(usize::MAX);
    let page: Vec<_> = all.into_iter().skip(start).take(limit as usize + 1).collect();
    let (next_cursor, has_more) = offset_page_meta(page.len(), limit, offset);
    let items: Vec<serde_json::Value> = page
        .into_iter()
        .take(limit as usize)
        .map(|row| character_json(&row))
        .collect::<Result<_, _>>()?;
    Ok(Json(wire_cast(serde_json::json!({
        "items": items,
        "pagination": {
            "limit": i64::from(limit),
            "has_more": has_more,
            "next_cursor": next_cursor,
        }
    }))))
}

/// `GET /v1/daemon/characters/{character_id}`
pub async fn get_character(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
) -> Result<Json<CharacterDetail>, NexusApiError> {
    let owner = require_creator(&state)?;
    let row = nexus_local_db::get_character(state.pool_or_uninit()?, &owner, &character_id)
        .await?
        .ok_or_else(|| NexusApiError::NotFound(format!("character {character_id}")))?;
    Ok(Json(wire_cast(serde_json::json!({
        "character": character_json(&row)?,
    }))))
}

/// `POST /v1/daemon/characters/{character_id}/bindings`
pub async fn add_binding(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    body: Bytes,
) -> Result<(StatusCode, Json<AddCharacterBindingResponse>), NexusApiError> {
    let req: AddCharacterBindingRequest = parse_canonical_json(&body)?;
    let owner = require_creator(&state)?;
    let binding = nexus_local_db::add_actor_world_binding(
        state.pool_or_uninit()?,
        CreateBindingParams {
            owner_creator_id: &owner,
            character_id: &character_id,
            world_id: req.world_id.as_str(),
            world_sheet_entry_id: req.world_sheet_entry_id.as_deref(),
        },
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(wire_cast(serde_json::json!({
            "binding": binding_json(&binding),
        }))),
    ))
}

/// `GET /v1/daemon/characters/{character_id}/bindings`
pub async fn list_bindings(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    Query(query): Query<ListCharacterBindingsQuery>,
) -> Result<Json<ListCharacterBindingsResponse>, NexusApiError> {
    let owner = require_creator(&state)?;
    let offset = decode_offset_cursor(&query.cursor)?;
    let limit = resolve_limit(query.limit)?;
    let all =
        nexus_local_db::list_bindings_for_character(state.pool_or_uninit()?, &owner, &character_id)
            .await?;
    let start = usize::try_from(offset).unwrap_or(usize::MAX);
    let page: Vec<_> = all.into_iter().skip(start).take(limit as usize + 1).collect();
    let (next_cursor, has_more) = offset_page_meta(page.len(), limit, offset);
    let items: Vec<serde_json::Value> = page
        .into_iter()
        .take(limit as usize)
        .map(|row| binding_json(&row))
        .collect();
    Ok(Json(wire_cast(serde_json::json!({
        "items": items,
        "pagination": {
            "limit": i64::from(limit),
            "has_more": has_more,
            "next_cursor": next_cursor,
        }
    }))))
}

/// `DELETE /v1/daemon/characters/{character_id}/bindings/{binding_id}`
pub async fn remove_binding(
    State(state): State<WorkspaceState>,
    Path((character_id, binding_id)): Path<(String, String)>,
) -> Result<StatusCode, NexusApiError> {
    let owner = require_creator(&state)?;
    nexus_local_db::remove_binding(state.pool_or_uninit()?, &owner, &character_id, &binding_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
