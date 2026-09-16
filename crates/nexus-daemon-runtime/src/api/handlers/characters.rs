//! Character identity and `ActorWorldBinding` Daemon API handlers (v1.184 P0).
//!
//! Thin translation over the core Character/binding family (v1.190 P2-T1):
//! stored ownership validation, the shared per-Character activity fence and
//! the lifecycle transitions live in [`nexus_core`]; the handlers keep only
//! auth resolution, wire parsing and status/envelope translation. Request
//! bodies never carry `owner_creator_id`; responses are generated DTOs
//! constructed from typed builders.

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::resolve_core_principal;
use crate::api::pagination::decode_offset_cursor;
use crate::workspace::WorkspaceState;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use nexus_contracts::daemon_api::characters::{
    add_character_binding_request::AddCharacterBindingRequest,
    add_character_binding_response::AddCharacterBindingResponse,
    character_binding_detail::CharacterBindingDetail,
    character_detail::{CharacterDetail, NexusCharacter as DetailCharacterWire},
    character_lifecycle_request::CharacterLifecycleRequest,
    create_character_request::CreateCharacterRequest,
    create_character_response::CreateCharacterResponse,
    list_character_bindings_query::ListCharacterBindingsQuery,
    list_character_bindings_response::ListCharacterBindingsResponse,
    list_characters_query::ListCharactersQuery,
    list_characters_response::ListCharactersResponse,
    update_character_binding_request::UpdateCharacterBindingRequest,
    update_character_request::UpdateCharacterRequest,
};
use nexus_contracts::generated::core::{
    CoreCharacterTransitionRequest, CoreCharacterTransitionRequestTargetStatus,
};
use nexus_local_db::{CharacterPatch, FieldPatch};
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

fn finish_builder<T, E: std::fmt::Display>(value: Result<T, E>) -> Result<T, NexusApiError> {
    value.map_err(wire_err)
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

fn parse_request_object(
    bytes: &Bytes,
) -> Result<serde_json::Map<String, serde_json::Value>, NexusApiError> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|err| NexusApiError::BadRequest {
            code: "invalid_input".into(),
            message: err.to_string(),
        })?;
    value
        .as_object()
        .cloned()
        .ok_or_else(|| NexusApiError::BadRequest {
            code: "invalid_input".into(),
            message: "request body must be a JSON object".into(),
        })
}

fn build_character_patch<'a>(
    raw: &'a serde_json::Map<String, serde_json::Value>,
    req: &'a UpdateCharacterRequest,
    persona_buf: &'a mut Option<String>,
) -> Result<CharacterPatch<'a>, NexusApiError> {
    let display_name = if raw.contains_key("display_name") {
        Some(
            req.display_name
                .as_ref()
                .ok_or_else(|| NexusApiError::BadRequest {
                    code: "invalid_input".into(),
                    message: "display_name must be a non-null string when present".into(),
                })?
                .as_str(),
        )
    } else {
        None
    };
    let image_uri = if !raw.contains_key("image_uri") {
        FieldPatch::Keep
    } else if req.image_uri.is_none() {
        FieldPatch::Clear
    } else {
        FieldPatch::Set(
            req.image_uri
                .as_ref()
                .expect("image_uri member present implies Some")
                .as_str(),
        )
    };
    let persona_json = if !raw.contains_key("persona") {
        FieldPatch::Keep
    } else if req.persona.is_none() {
        FieldPatch::Clear
    } else {
        let encoded = serde_json::Value::Object(
            req.persona
                .clone()
                .expect("persona member present implies Some"),
        )
        .to_string();
        *persona_buf = Some(encoded);
        FieldPatch::Set(persona_buf.as_deref().expect("persona buffer set"))
    };
    Ok(CharacterPatch {
        display_name,
        image_uri,
        persona_json,
    })
}

const fn character_patch_is_empty(patch: &CharacterPatch<'_>) -> bool {
    patch.is_empty()
}

fn binding_patch_is_empty(raw: &serde_json::Map<String, serde_json::Value>) -> bool {
    !raw.contains_key("world_sheet_entry_id")
}

fn build_binding_sheet_patch<'a>(
    raw: &'a serde_json::Map<String, serde_json::Value>,
    req: &'a UpdateCharacterBindingRequest,
) -> Result<FieldPatch<&'a str>, NexusApiError> {
    match raw.get("world_sheet_entry_id") {
        None => Err(NexusApiError::BadRequest {
            code: "invalid_input".into(),
            message: "patch must include at least one mutable field".into(),
        }),
        Some(serde_json::Value::Null) => Ok(FieldPatch::Clear),
        Some(_) => Ok(FieldPatch::Set(
            req.world_sheet_entry_id
                .as_ref()
                .ok_or_else(|| NexusApiError::BadRequest {
                    code: "invalid_input".into(),
                    message: "world_sheet_entry_id must be a non-null string when present".into(),
                })?
                .as_str(),
        )),
    }
}

/// Material lifecycle transition ordering (durable §11.3.2–§11.3.3), fenced by
/// the core per-Character lease **and** the daemon's in-process session fence:
/// 1. daemon `try_character_transition` exclusive registry fence (busy refusal
///    before DB — retained §11.3.1 interlock with the session/memory paths),
/// 2. `acquire_character_transition` exclusive core fence (busy refusal
///    before DB, ownership + pre-transition epoch re-read under the fence),
/// 3. `commit_character_transition` revision-checked commit (epoch increment),
/// 4. while both fences are still held, `retire_character_sessions` only
///    when the committed epoch differs from the lease's pre-transition epoch,
/// 5. drop both fences, then one Host shutdown attempt per retired id outside
///    registry locks (failures are logged and cannot undo the commit),
/// 6. return the committed record projected to the daemon detail envelope.
///
/// Both fences are held because the in-process session/memory effects
/// (`character_memory`, `character_tom`, `agent_host`) still admit activity on
/// the registry fence; retiring that fence is P4-T2's cutover (§11.3).
async fn execute_character_lifecycle(
    state: &WorkspaceState,
    character_id: &str,
    expected_revision: i64,
    target: CoreCharacterTransitionRequestTargetStatus,
) -> Result<CharacterDetail, NexusApiError> {
    let (core, principal) = resolve_core_principal(state).await?;
    let registry = state.actor_sessions();
    let (committed, retired_ids) = {
        let registry_guard = registry
            .try_character_transition(
                state.pool_or_uninit()?,
                principal.creator_id(),
                character_id,
            )
            .await?;
        let mut lease = core
            .acquire_character_transition(&principal, character_id.to_string())
            .await?;
        let pre_epoch = lease.epoch();
        let request: CoreCharacterTransitionRequest = CoreCharacterTransitionRequest::builder()
            .character_id(character_id.to_string())
            .expected_revision(expected_revision)
            .target_status(target)
            .try_into()
            .map_err(wire_err)?;
        let committed = core
            .commit_character_transition(&principal, &mut lease, request)
            .await?;

        // §11.3.3: retire old session keys while the exclusive fences are
        // still held — no new session can be admitted at the new epoch in
        // this window, so the blanket retire cannot hit a fresh session.
        // Both guards are released before the (fallible, one-attempt) Host
        // shutdown loop below.
        let retired_ids = if lease.epoch() == pre_epoch {
            Vec::new()
        } else {
            registry.retire_character_sessions(character_id)
        };
        drop(lease);
        drop(registry_guard);
        (committed, retired_ids)
    };

    if !retired_ids.is_empty() {
        if let Some(host) = state.agent_host() {
            for session_id in retired_ids {
                if let Err(err) = host.shutdown_session(session_id.clone()).await {
                    tracing::warn!(
                        character_id = %character_id,
                        session_id = %session_id,
                        error = %err,
                        "lifecycle host shutdown attempt failed (transition already committed)"
                    );
                }
            }
        }
    }

    finish_builder(
        CharacterDetail::builder()
            .character(map_wire::<DetailCharacterWire>(committed.character)?)
            .try_into(),
    )
}

/// `POST /v1/daemon/characters`
pub async fn create_character(
    State(state): State<WorkspaceState>,
    body: Bytes,
) -> Result<(StatusCode, Json<CreateCharacterResponse>), NexusApiError> {
    let req: CreateCharacterRequest = parse_canonical_json(&body)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core.create_character(&principal, req).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

/// `GET /v1/daemon/characters`
pub async fn list_characters(
    State(state): State<WorkspaceState>,
    Query(query): Query<ListCharactersQuery>,
) -> Result<Json<ListCharactersResponse>, NexusApiError> {
    let (core, principal) = resolve_core_principal(&state).await?;
    let offset = decode_offset_cursor(&query.cursor)?;
    let limit = resolve_limit(query.limit)?;
    let response = core.list_characters(&principal, limit, offset).await?;
    Ok(Json(response))
}

/// `GET /v1/daemon/characters/{character_id}`
pub async fn get_character(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
) -> Result<Json<CharacterDetail>, NexusApiError> {
    let (core, principal) = resolve_core_principal(&state).await?;
    let detail = core.character(&principal, character_id).await?;
    Ok(Json(detail))
}

/// `POST /v1/daemon/characters/{character_id}/bindings`
pub async fn add_binding(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    body: Bytes,
) -> Result<(StatusCode, Json<AddCharacterBindingResponse>), NexusApiError> {
    let req: AddCharacterBindingRequest = parse_canonical_json(&body)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    let response = core
        .add_binding(
            &principal,
            character_id,
            req.world_id.to_string(),
            optional_str(req.world_sheet_entry_id.as_ref()).map(str::to_string),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(response)))
}

/// `GET /v1/daemon/characters/{character_id}/bindings`
pub async fn list_bindings(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    Query(query): Query<ListCharacterBindingsQuery>,
) -> Result<Json<ListCharacterBindingsResponse>, NexusApiError> {
    let (core, principal) = resolve_core_principal(&state).await?;
    let offset = decode_offset_cursor(&query.cursor)?;
    let limit = resolve_limit(query.limit)?;
    let response = core
        .list_bindings(&principal, character_id, limit, offset)
        .await?;
    Ok(Json(response))
}

/// `GET /v1/daemon/characters/{character_id}/bindings/{binding_id}`
pub async fn get_binding(
    State(state): State<WorkspaceState>,
    Path((character_id, binding_id)): Path<(String, String)>,
) -> Result<Json<CharacterBindingDetail>, NexusApiError> {
    let (core, principal) = resolve_core_principal(&state).await?;
    let detail = core.binding(&principal, character_id, binding_id).await?;
    Ok(Json(detail))
}

/// `PATCH /v1/daemon/characters/{character_id}/bindings/{binding_id}`
pub async fn patch_binding(
    State(state): State<WorkspaceState>,
    Path((character_id, binding_id)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<CharacterBindingDetail>, NexusApiError> {
    let raw = parse_request_object(&body)?;
    let req: UpdateCharacterBindingRequest =
        serde_json::from_value(serde_json::Value::Object(raw.clone())).map_err(|err| {
            NexusApiError::BadRequest {
                code: "invalid_input".into(),
                message: err.to_string(),
            }
        })?;
    if binding_patch_is_empty(&raw) {
        return Err(NexusApiError::BadRequest {
            code: "invalid_input".into(),
            message: "patch must include at least one mutable field".into(),
        });
    }
    let sheet_patch = build_binding_sheet_patch(&raw, &req)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    let detail = core
        .patch_binding(
            &principal,
            character_id,
            binding_id,
            req.expected_revision,
            sheet_patch,
        )
        .await?;
    Ok(Json(detail))
}

/// `DELETE /v1/daemon/characters/{character_id}/bindings/{binding_id}`
pub async fn remove_binding(
    State(state): State<WorkspaceState>,
    Path((character_id, binding_id)): Path<(String, String)>,
) -> Result<StatusCode, NexusApiError> {
    let (core, principal) = resolve_core_principal(&state).await?;
    core.remove_binding(&principal, character_id, binding_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PATCH /v1/daemon/characters/{character_id}`
pub async fn patch_character(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    body: Bytes,
) -> Result<Json<CharacterDetail>, NexusApiError> {
    let raw = parse_request_object(&body)?;
    let mut persona_buf = None;
    let req: UpdateCharacterRequest =
        serde_json::from_value(serde_json::Value::Object(raw.clone())).map_err(|err| {
            NexusApiError::BadRequest {
                code: "invalid_input".into(),
                message: err.to_string(),
            }
        })?;
    let patch = build_character_patch(&raw, &req, &mut persona_buf)?;
    // Wire-shape validation intentionally precedes owner/404 admission so an
    // empty patch on a foreign/missing Character returns 400, not 404.
    if character_patch_is_empty(&patch) {
        return Err(NexusApiError::BadRequest {
            code: "invalid_input".into(),
            message: "patch must include at least one mutable field".into(),
        });
    }
    let (core, principal) = resolve_core_principal(&state).await?;
    let detail = core
        .patch_character(&principal, character_id, req.expected_revision, patch)
        .await?;
    Ok(Json(detail))
}

/// `POST /v1/daemon/characters/{character_id}/archive`
pub async fn archive_character(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    body: Bytes,
) -> Result<Json<CharacterDetail>, NexusApiError> {
    let req: CharacterLifecycleRequest = parse_canonical_json(&body)?;
    let detail = execute_character_lifecycle(
        &state,
        &character_id,
        req.expected_revision,
        CoreCharacterTransitionRequestTargetStatus::Archived,
    )
    .await?;
    Ok(Json(detail))
}

/// `POST /v1/daemon/characters/{character_id}/restore`
pub async fn restore_character(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    body: Bytes,
) -> Result<Json<CharacterDetail>, NexusApiError> {
    let req: CharacterLifecycleRequest = parse_canonical_json(&body)?;
    let detail = execute_character_lifecycle(
        &state,
        &character_id,
        req.expected_revision,
        CoreCharacterTransitionRequestTargetStatus::Active,
    )
    .await?;
    Ok(Json(detail))
}
