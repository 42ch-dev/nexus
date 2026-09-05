//! Character identity and `ActorWorldBinding` Daemon API handlers (v1.184 P0).
//!
//! Active-Creator admission is stored-config only; request bodies never carry
//! `owner_creator_id`. Responses are generated DTOs constructed from typed builders.

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::require_creator;
use crate::api::pagination::{decode_offset_cursor, offset_page_meta};
use crate::workspace::WorkspaceState;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Utc};
use nexus_contracts::daemon_api::characters::{
    add_character_binding_request::AddCharacterBindingRequest,
    add_character_binding_response::{
        AddCharacterBindingResponse, NexusActorWorldBinding as AddedBindingWire,
    },
    character_detail::{CharacterDetail, NexusCharacter as DetailCharacterWire},
    create_character_request::CreateCharacterRequest,
    create_character_response::{
        CreateCharacterResponse, NexusActorWorldBinding as CreatedBindingWire,
        NexusCharacter as CreatedCharacterWire,
    },
    list_character_bindings_query::ListCharacterBindingsQuery,
    list_character_bindings_response::{
        ListCharacterBindingsResponse, NexusActorWorldBinding as ListedBindingWire,
        NexusPaginationInfo as BindingPaginationInfo,
    },
    list_characters_query::ListCharactersQuery,
    list_characters_response::{
        ListCharactersResponse, NexusCharacter as ListedCharacterWire, NexusPaginationInfo,
    },
};
use nexus_contracts::{ActorWorldBinding, Character};
use nexus_local_db::{
    actor_world_binding::ActorWorldBindingRecord, character::CharacterRecord, CreateBindingParams,
    CreateCharacterParams,
};
use serde::de::DeserializeOwned;
use serde::Serialize;

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 100;

fn resolve_limit(raw: Option<i64>) -> Result<u32, NexusApiError> {
    match raw {
        None => Ok(DEFAULT_LIMIT),
        Some(n) if n > 0 && n <= i64::from(MAX_LIMIT) => {
            u32::try_from(n).map_err(|_| NexusApiError::BadRequest {
                code: "invalid_input".into(),
                message: "limit is out of range".into(),
            })
        }
        Some(_) => Err(NexusApiError::BadRequest {
            code: "invalid_input".into(),
            message: format!("limit must be between 1 and {MAX_LIMIT}"),
        }),
    }
}

fn wire_err(err: impl std::fmt::Display) -> NexusApiError {
    NexusApiError::Internal {
        code: "CHARACTER_WIRE_INVALID".into(),
        message: err.to_string(),
    }
}

fn map_wire<T: DeserializeOwned>(value: impl Serialize) -> Result<T, NexusApiError> {
    let json = serde_json::to_value(value).map_err(wire_err)?;
    serde_json::from_value(json).map_err(wire_err)
}

fn parse_rfc3339(raw: &str) -> Result<DateTime<Utc>, NexusApiError> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(wire_err)
}

fn finish_builder<T, E: std::fmt::Display>(value: Result<T, E>) -> Result<T, NexusApiError> {
    value.map_err(wire_err)
}

fn parse_optional<T>(raw: Option<&str>) -> Result<Option<T>, NexusApiError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    raw.map(str::parse).transpose().map_err(wire_err)
}

fn character_from_record(record: &CharacterRecord) -> Result<Character, NexusApiError> {
    let persona: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&record.persona_json).map_err(wire_err)?;
    finish_builder(
        Character::builder()
            .schema_version(1u64)
            .character_id(record.character_id.as_str())
            .owner_creator_id(record.owner_creator_id.as_str())
            .display_name(record.display_name.as_str())
            .status(record.status.as_str())
            .persona(persona)
            .image_uri(parse_optional(record.image_uri.as_deref())?)
            .created_at(parse_rfc3339(&record.created_at)?)
            .updated_at(parse_rfc3339(&record.updated_at)?)
            .try_into(),
    )
}

fn binding_from_record(
    record: &ActorWorldBindingRecord,
) -> Result<ActorWorldBinding, NexusApiError> {
    finish_builder(
        ActorWorldBinding::builder()
            .schema_version(1u64)
            .binding_id(record.binding_id.as_str())
            .character_id(record.character_id.as_str())
            .world_id(record.world_id.as_str())
            .status(record.status.as_str())
            .world_sheet_entry_id(parse_optional(record.world_sheet_entry_id.as_deref())?)
            .created_at(parse_rfc3339(&record.created_at)?)
            .updated_at(parse_rfc3339(&record.updated_at)?)
            .try_into(),
    )
}

fn pagination_info(
    limit: u32,
    has_more: bool,
    next_cursor: Option<String>,
) -> Result<NexusPaginationInfo, NexusApiError> {
    finish_builder(
        NexusPaginationInfo::builder()
            .limit(i64::from(limit))
            .has_more(has_more)
            .next_cursor(next_cursor)
            .try_into(),
    )
}

fn binding_pagination_info(
    limit: u32,
    has_more: bool,
    next_cursor: Option<String>,
) -> Result<BindingPaginationInfo, NexusApiError> {
    finish_builder(
        BindingPaginationInfo::builder()
            .limit(i64::from(limit))
            .has_more(has_more)
            .next_cursor(next_cursor)
            .try_into(),
    )
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

fn optional_str(value: Option<&impl std::ops::Deref<Target = String>>) -> Option<&str> {
    value.map(|s| s.as_str())
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
            image_uri: optional_str(req.image_uri.as_ref()),
            persona_json: &persona,
            world_id: req.world_id.as_str(),
            world_sheet_entry_id: optional_str(req.world_sheet_entry_id.as_ref()),
        },
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(finish_builder(
            CreateCharacterResponse::builder()
                .character(map_wire::<CreatedCharacterWire>(character_from_record(
                    &created.character,
                )?)?)
                .binding(map_wire::<CreatedBindingWire>(binding_from_record(
                    &created.binding,
                )?)?)
                .try_into(),
        )?),
    ))
}

/// `GET /v1/daemon/characters`
pub async fn list_characters(
    State(state): State<WorkspaceState>,
    Query(query): Query<ListCharactersQuery>,
) -> Result<Json<ListCharactersResponse>, NexusApiError> {
    let owner = require_creator(&state)?;
    let offset = decode_offset_cursor(&query.cursor)?;
    let limit = resolve_limit(query.limit)?;
    let fetch_limit = i64::from(limit) + 1;
    let page = nexus_local_db::list_characters(
        state.pool_or_uninit()?,
        &owner,
        fetch_limit,
        i64::from(offset),
    )
    .await?;
    let (next_cursor, has_more) = offset_page_meta(page.len(), limit, offset);
    let items: Vec<Character> = page
        .into_iter()
        .take(limit as usize)
        .map(|row| character_from_record(&row))
        .collect::<Result<_, _>>()?;
    Ok(Json(finish_builder(
        ListCharactersResponse::builder()
            .items(map_wire::<Vec<ListedCharacterWire>>(items)?)
            .pagination(pagination_info(limit, has_more, next_cursor)?)
            .try_into(),
    )?))
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
    Ok(Json(finish_builder(
        CharacterDetail::builder()
            .character(map_wire::<DetailCharacterWire>(character_from_record(
                &row,
            )?)?)
            .try_into(),
    )?))
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
            world_sheet_entry_id: optional_str(req.world_sheet_entry_id.as_ref()),
        },
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(finish_builder(
            AddCharacterBindingResponse::builder()
                .binding(map_wire::<AddedBindingWire>(binding_from_record(
                    &binding,
                )?)?)
                .try_into(),
        )?),
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
    let fetch_limit = i64::from(limit) + 1;
    let page = nexus_local_db::list_bindings_for_character(
        state.pool_or_uninit()?,
        &owner,
        &character_id,
        fetch_limit,
        i64::from(offset),
    )
    .await?;
    let (next_cursor, has_more) = offset_page_meta(page.len(), limit, offset);
    let items: Vec<ActorWorldBinding> = page
        .into_iter()
        .take(limit as usize)
        .map(|row| binding_from_record(&row))
        .collect::<Result<_, _>>()?;
    Ok(Json(finish_builder(
        ListCharacterBindingsResponse::builder()
            .items(map_wire::<Vec<ListedBindingWire>>(items)?)
            .pagination(binding_pagination_info(limit, has_more, next_cursor)?)
            .try_into(),
    )?))
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

#[cfg(test)]
mod conversion_tests {
    use super::*;

    fn sample_character() -> CharacterRecord {
        CharacterRecord {
            character_id: "chr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            owner_creator_id: "ctr_localabcdef123456".into(),
            display_name: "Ava".into(),
            status: "active".into(),
            image_uri: Some("https://example.test/ava.png".into()),
            persona_json: r#"{"tone":"calm"}"#.into(),
            created_at: "2026-09-05T00:00:00Z".into(),
            updated_at: "2026-09-05T00:00:01Z".into(),
        }
    }

    fn sample_binding() -> ActorWorldBindingRecord {
        ActorWorldBindingRecord {
            binding_id: "awb_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
            character_id: "chr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            world_id: "wld_worldA".into(),
            status: "active".into(),
            world_sheet_entry_id: Some("sheet-1".into()),
            created_at: "2026-09-05T00:00:00Z".into(),
            updated_at: "2026-09-05T00:00:01Z".into(),
        }
    }

    #[test]
    fn character_from_record_uses_generated_builder_fields() {
        let mapped = character_from_record(&sample_character()).expect("valid record");
        assert_eq!(
            mapped.character_id.as_str(),
            "chr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(mapped.owner_creator_id.as_str(), "ctr_localabcdef123456");
        assert_eq!(mapped.display_name.as_str(), "Ava");
        assert_eq!(
            mapped.image_uri.as_ref().map(|u| u.as_str()),
            Some("https://example.test/ava.png")
        );
        assert_eq!(
            mapped.persona.get("tone").and_then(|v| v.as_str()),
            Some("calm")
        );
        let envelope: CreateCharacterResponse = finish_builder(
            CreateCharacterResponse::builder()
                .character(map_wire::<CreatedCharacterWire>(mapped).expect("embed character"))
                .binding(
                    map_wire::<CreatedBindingWire>(
                        binding_from_record(&sample_binding()).expect("binding"),
                    )
                    .expect("embed binding"),
                )
                .try_into(),
        )
        .expect("typed create envelope");
        assert_eq!(
            envelope.character.character_id.as_str(),
            "chr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
    }

    #[test]
    fn invalid_status_is_character_wire_invalid() {
        let mut record = sample_character();
        record.status = "unknown".into();
        let err = character_from_record(&record).expect_err("status must fail");
        match err {
            NexusApiError::Internal { code, .. } => assert_eq!(code, "CHARACTER_WIRE_INVALID"),
            other => panic!("expected CHARACTER_WIRE_INVALID, got {other:?}"),
        }
    }

    #[test]
    fn list_envelope_is_generated_builder() {
        let items = vec![character_from_record(&sample_character()).expect("character")];
        let listed: ListCharactersResponse = finish_builder(
            ListCharactersResponse::builder()
                .items(map_wire::<Vec<ListedCharacterWire>>(items).expect("embed items"))
                .pagination(pagination_info(10, true, Some("v1:10".into())).expect("page"))
                .try_into(),
        )
        .expect("typed list envelope");
        assert_eq!(listed.items.len(), 1);
        assert!(listed.pagination.has_more);
        assert_eq!(listed.pagination.next_cursor.as_deref(), Some("v1:10"));
    }
}
