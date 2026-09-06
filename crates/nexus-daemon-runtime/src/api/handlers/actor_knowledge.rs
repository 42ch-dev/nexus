//! Actor `KnowledgeView` HTTP handlers (v1.184 P1 Task 3).
//!
//! Routes call [`ActorKnowledgeViewService`] directly. Admission is stored
//! owners only; clients never send `owner_creator_id`.

#![allow(clippy::missing_errors_doc)]

use crate::actor_knowledge_view::{
    ActorKnowledgePage, ActorKnowledgeViewQuery, ActorKnowledgeViewService, AdmittedActor,
};
use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::require_creator;
use crate::workspace::WorkspaceState;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Utc};
use nexus_contracts::daemon_api::actor_knowledge::{
    add_knowledge_entry_request::{AddKnowledgeEntryRequest, AddKnowledgeEntryRequestOwnerKind},
    add_knowledge_entry_response::{
        AddKnowledgeEntryResponse, NexusActorKnowledgeViewItem as CreatedItem,
    },
    delete_knowledge_entry_query::DeleteKnowledgeEntryQuery,
    knowledge_entry_detail::{
        KnowledgeEntryDetail, NexusActorKnowledgeViewItem as DetailItem,
    },
    knowledge_view_item::KnowledgeViewItem,
    list_character_knowledge_query::ListCharacterKnowledgeQuery,
    list_character_knowledge_response::{
        ListCharacterKnowledgeResponse, NexusActorKnowledgeViewItem as ListedItem,
        NexusPaginationInfo as ListedPagination,
    },
    update_knowledge_entry_request::UpdateKnowledgeEntryRequest,
    view_request::{NexusActorRef, ViewRequest},
    view_response::{
        NexusActorKnowledgeViewItem as ViewItem, NexusPaginationInfo as ViewPagination,
        ViewResponse,
    },
};
use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::knowledge_entry::{
    parse_stored_created_at, KnowledgeEntryBody, KnowledgeEntryRecord, KnowledgeOwnerRef,
};
use nexus_knowledge::world_kb::store::{KbStore, KbStoreError};
use nexus_local_db::{
    kb_store::SqliteKbStore, ActorKnowledgePatch, FieldPatch, LocalDbError,
};
use serde::de::DeserializeOwned;
use serde::Serialize;

fn wire_err(err: impl std::fmt::Display) -> NexusApiError {
    NexusApiError::Internal {
        code: "ACTOR_KNOWLEDGE_WIRE_INVALID".into(),
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

fn optional_str(value: Option<&impl std::ops::Deref<Target = String>>) -> Option<&str> {
    value.map(|s| s.as_str())
}

fn parse_rfc3339(raw: &str) -> Result<DateTime<Utc>, NexusApiError> {
    parse_stored_created_at(raw).map_err(wire_err)
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

fn summary_wire_value(record: &KnowledgeEntryRecord) -> Option<&str> {
    record
        .body
        .as_ref()
        .and_then(|body| body.summary.as_deref())
}

fn detail_from_record(record: &KnowledgeEntryRecord) -> Result<KnowledgeEntryDetail, NexusApiError> {
    let item = item_from_record(record)?;
    let summary = summary_wire_value(record);
    let value = serde_json::json!({
        "item": map_wire::<DetailItem>(item)?,
        "summary": summary,
    });
    serde_json::from_value(value).map_err(wire_err)
}

fn knowledge_patch_is_empty(raw: &serde_json::Map<String, serde_json::Value>) -> bool {
    !raw.contains_key("canonical_name") && !raw.contains_key("summary")
}

fn build_knowledge_summary_patch<'a>(
    raw: &'a serde_json::Map<String, serde_json::Value>,
    req: &'a UpdateKnowledgeEntryRequest,
) -> Result<FieldPatch<&'a str>, NexusApiError> {
    match raw.get("summary") {
        None => Ok(FieldPatch::Keep),
        Some(serde_json::Value::Null) => Ok(FieldPatch::Clear),
        Some(_) => Ok(FieldPatch::Set(
            req.summary
                .as_ref()
                .ok_or_else(|| NexusApiError::BadRequest {
                    code: "invalid_input".into(),
                    message: "summary must be a string or null when present".into(),
                })?
                .as_str(),
        )),
    }
}

fn build_actor_knowledge_patch<'a>(
    raw: &'a serde_json::Map<String, serde_json::Value>,
    req: &'a UpdateKnowledgeEntryRequest,
) -> Result<ActorKnowledgePatch<'a>, NexusApiError> {
    let canonical_name = if raw.contains_key("canonical_name") {
        Some(
            req.canonical_name
                .as_ref()
                .ok_or_else(|| NexusApiError::BadRequest {
                    code: "invalid_input".into(),
                    message: "canonical_name must be a non-null string when present".into(),
                })?
                .as_str(),
        )
    } else {
        None
    };
    Ok(ActorKnowledgePatch {
        canonical_name,
        summary: build_knowledge_summary_patch(raw, req)?,
    })
}

fn apply_create_summary(
    record: &mut KnowledgeEntryRecord,
    summary: Option<&str>,
) -> Result<(), NexusApiError> {
    if let Some(text) = summary {
        record.body = Some(KnowledgeEntryBody {
            summary: Some(text.to_string()),
            ..KnowledgeEntryBody::default()
        });
    }
    Ok(())
}

fn item_from_record(record: &KnowledgeEntryRecord) -> Result<KnowledgeViewItem, NexusApiError> {
    let owner = serde_json::json!({
        "kind": record.owner.kind(),
        "id": record.owner.id(),
    });
    let value = serde_json::json!({
        "entry_id": record.entry_id,
        "owner": owner,
        "creator_only": record.creator_only,
        "block_type": serde_json::to_value(record.block_type).map_err(wire_err)?,
        "canonical_name": record.canonical_name,
        "status": record.status,
        "revision": record.revision.unwrap_or(0),
        "created_at": parse_rfc3339(&record.created_at)?,
    });
    serde_json::from_value(value).map_err(wire_err)
}

fn view_pagination(page: &ActorKnowledgePage) -> Result<ViewPagination, NexusApiError> {
    finish_builder(
        ViewPagination::builder()
            .limit(i64::from(page.limit))
            .has_more(page.has_more)
            .next_cursor(page.next_cursor.clone())
            .try_into(),
    )
}

fn listed_pagination(page: &ActorKnowledgePage) -> Result<ListedPagination, NexusApiError> {
    finish_builder(
        ListedPagination::builder()
            .limit(i64::from(page.limit))
            .has_more(page.has_more)
            .next_cursor(page.next_cursor.clone())
            .try_into(),
    )
}

fn admit_actor(actor_ref: &NexusActorRef) -> AdmittedActor {
    match actor_ref {
        NexusActorRef::CreatorActorRef { creator_id, .. } => AdmittedActor::Creator {
            creator_id: creator_id.to_string(),
        },
        NexusActorRef::CharacterActorRef { character_id, .. } => AdmittedActor::Character {
            character_id: character_id.to_string(),
        },
    }
}

fn map_insert_err(err: KbStoreError) -> NexusApiError {
    match err {
        KbStoreError::Duplicate { .. }
        | KbStoreError::Validation(_)
        | KbStoreError::ValidationLegacy(_) => NexusApiError::BadRequest {
            code: "invalid_input".into(),
            message: err.to_string(),
        },
        other => NexusApiError::Internal {
            code: "ACTOR_KNOWLEDGE_INSERT_FAILED".into(),
            message: other.to_string(),
        },
    }
}

/// `POST /v1/daemon/actor-knowledge/view`
pub async fn view(
    State(state): State<WorkspaceState>,
    body: Bytes,
) -> Result<Json<ViewResponse>, NexusApiError> {
    let req: ViewRequest = parse_canonical_json(&body)?;
    let owner = require_creator(&state)?;
    let service = ActorKnowledgeViewService::new(state.pool_or_uninit()?.clone());
    let page = service
        .view(
            &owner,
            &admit_actor(&req.actor_ref),
            ActorKnowledgeViewQuery {
                world_id: req.world_id.to_string(),
                binding_id: optional_str(req.binding_id.as_ref()).map(str::to_string),
                limit: ActorKnowledgeViewService::resolve_limit(req.limit)?,
                cursor: req.cursor.clone(),
            },
        )
        .await?;
    let items: Vec<KnowledgeViewItem> = page
        .items
        .iter()
        .map(item_from_record)
        .collect::<Result<_, _>>()?;
    Ok(Json(finish_builder(
        ViewResponse::builder()
            .items(map_wire::<Vec<ViewItem>>(items)?)
            .pagination(view_pagination(&page)?)
            .try_into(),
    )?))
}

/// `POST /v1/daemon/actor-knowledge/entries`
#[allow(clippy::too_many_lines)] // single validated create path
pub async fn add_entry(
    State(state): State<WorkspaceState>,
    body: Bytes,
) -> Result<(StatusCode, Json<AddKnowledgeEntryResponse>), NexusApiError> {
    let raw = parse_request_object(&body)?;
    let req: AddKnowledgeEntryRequest =
        serde_json::from_value(serde_json::Value::Object(raw.clone())).map_err(|err| {
            NexusApiError::BadRequest {
                code: "invalid_input".into(),
                message: err.to_string(),
            }
        })?;
    let creator_id = require_creator(&state)?;
    let pool = state.pool_or_uninit()?.clone();
    let service = ActorKnowledgeViewService::new(pool.clone());
    let creator_only = req.creator_only.unwrap_or(false);
    let owner = match req.owner_kind {
        AddKnowledgeEntryRequestOwnerKind::World => {
            if raw.contains_key("summary") {
                return Err(NexusApiError::BadRequest {
                    code: "invalid_input".into(),
                    message: "summary is not accepted on world-owned knowledge create".into(),
                });
            }
            let world_id =
                optional_str(req.world_id.as_ref()).ok_or_else(|| NexusApiError::BadRequest {
                    code: "invalid_input".into(),
                    message: "world_id is required for world-owned knowledge".into(),
                })?;
            service.require_active_owned_world(&creator_id, world_id).await?;
            KnowledgeOwnerRef::world(world_id)
        }
        AddKnowledgeEntryRequestOwnerKind::Character => {
            if creator_only {
                return Err(NexusApiError::BadRequest {
                    code: "invalid_input".into(),
                    message: "creator_only is World-owned only".into(),
                });
            }
            let character_id = optional_str(req.character_id.as_ref()).ok_or_else(|| {
                NexusApiError::BadRequest {
                    code: "invalid_input".into(),
                    message: "character_id is required for character-owned knowledge".into(),
                }
            })?;
            service
                .require_active_owned_character(&creator_id, character_id)
                .await?;
            KnowledgeOwnerRef::character(character_id)
        }
        AddKnowledgeEntryRequestOwnerKind::ActorWorldBinding => {
            if creator_only {
                return Err(NexusApiError::BadRequest {
                    code: "invalid_input".into(),
                    message: "creator_only is World-owned only".into(),
                });
            }
            let character_id = optional_str(req.character_id.as_ref()).ok_or_else(|| {
                NexusApiError::BadRequest {
                    code: "invalid_input".into(),
                    message: "character_id is required for binding-owned knowledge".into(),
                }
            })?;
            let binding_id =
                optional_str(req.binding_id.as_ref()).ok_or_else(|| NexusApiError::BadRequest {
                    code: "invalid_input".into(),
                    message: "binding_id is required for binding-owned knowledge".into(),
                })?;
            let world_id =
                optional_str(req.world_id.as_ref()).ok_or_else(|| NexusApiError::BadRequest {
                    code: "invalid_input".into(),
                    message: "world_id is required for binding-owned knowledge".into(),
                })?;
            service
                .require_active_owned_character(&creator_id, character_id)
                .await?;
            service.require_active_owned_world(&creator_id, world_id).await?;
            service
                .require_active_binding(character_id, binding_id, world_id)
                .await?;
            KnowledgeOwnerRef::actor_world_binding(binding_id)
        }
    };
    let block_type: BlockType = map_wire(req.block_type)?;
    let mut record = match &owner {
        KnowledgeOwnerRef::World(id) => {
            KnowledgeEntryRecord::new(id, block_type, req.canonical_name.as_str())
        }
        KnowledgeOwnerRef::Character(id) => {
            KnowledgeEntryRecord::for_character(id, block_type, req.canonical_name.as_str())
        }
        KnowledgeOwnerRef::ActorWorldBinding(id) => {
            KnowledgeEntryRecord::for_binding(id, block_type, req.canonical_name.as_str())
        }
    };
    record.creator_only = creator_only;
    let create_summary = optional_str(req.summary.as_ref());
    if matches!(
        &owner,
        KnowledgeOwnerRef::Character(_) | KnowledgeOwnerRef::ActorWorldBinding(_)
    ) {
        apply_create_summary(&mut record, create_summary)?;
    }
    let store = SqliteKbStore::new(pool.clone());
    // Character/binding-owned KE inserts revalidate the stored active owner
    // inside the write transaction (durable §11.3.5) — a route check alone is
    // not sufficient. World-owned inserts keep the existing behavior/texture.
    let inserted = match &owner {
        KnowledgeOwnerRef::World(_) => store
            .insert_knowledge_entry(record.clone())
            .await
            .map_err(map_insert_err)
            .map(|r| r.entry_id.clone()),
        KnowledgeOwnerRef::Character(id) => {
            // Side-effecting Character activity: hold the activity fence across
            // the guarded INSERT (durable §11.3.1).
            let _activity = state
                .actor_sessions()
                .admit_character_activity(&pool, &creator_id, id.as_str())
                .await?;
            store
                .insert_actor_owned_key_block(&creator_id, id, None, record.clone())
                .await
                .map_err(map_local_db_insert_err)
                .map(|r| r.entry_id.clone())
        }
        KnowledgeOwnerRef::ActorWorldBinding(id) => {
            let admitted_character = admitted_character_id(&req);
            let admitted_character = admitted_character.as_deref().ok_or_else(|| {
                NexusApiError::Internal {
                    code: "ACTOR_KNOWLEDGE_INSERT_FAILED".into(),
                    message: "binding-owned KE insert must carry the admitted Character".into(),
                }
            })?;
            let _activity = state
                .actor_sessions()
                .admit_character_activity(&pool, &creator_id, admitted_character)
                .await?;
            store
                .insert_actor_owned_key_block(
                    &creator_id,
                    admitted_character,
                    Some(id),
                    record.clone(),
                )
                .await
                .map_err(map_local_db_insert_err)
                .map(|r| r.entry_id.clone())
        }
    }?;
    let stored = store
        .get_knowledge_entry(&inserted)
        .await
        .map_err(map_insert_err)?;
    Ok((
        StatusCode::CREATED,
        Json(finish_builder(
            AddKnowledgeEntryResponse::builder()
                .item(map_wire::<CreatedItem>(item_from_record(&stored)?)?)
                .try_into(),
        )?),
    ))
}

/// Admitted Character for a binding-owned KE insert (already validated above
/// by `require_active_owned_character` / `require_active_binding`).
fn admitted_character_id(req: &AddKnowledgeEntryRequest) -> Option<String> {
    optional_str(req.character_id.as_ref()).map(str::to_string)
}

/// Map a guarded-create `LocalDbError` to the canonical daemon envelope: a
/// contract conflict maps to its stable 409 code; an actor row we failed to
/// create maps to 404 (existence hidden); everything else is internal.
fn map_local_db_insert_err(err: LocalDbError) -> NexusApiError {
    match err {
        LocalDbError::ActorContractConflict { code } => NexusApiError::ConflictCoded {
            code: code.as_str().to_string(),
            message: code.message().to_string(),
        },
        LocalDbError::ActorNotFound { resource, id } => {
            NexusApiError::NotFound(format!("{resource} {id}"))
        }
        LocalDbError::ConstraintViolation { .. } => NexusApiError::BadRequest {
            code: "invalid_input".into(),
            message: err.to_string(),
        },
        other => NexusApiError::Internal {
            code: "ACTOR_KNOWLEDGE_INSERT_FAILED".into(),
            message: other.to_string(),
        },
    }
}

/// `GET /v1/daemon/characters/{character_id}/knowledge`
pub async fn list_character_knowledge(
    State(state): State<WorkspaceState>,
    Path(character_id): Path<String>,
    Query(query): Query<ListCharacterKnowledgeQuery>,
) -> Result<Json<ListCharacterKnowledgeResponse>, NexusApiError> {
    let owner = require_creator(&state)?;
    let service = ActorKnowledgeViewService::new(state.pool_or_uninit()?.clone());
    let page = service
        .list_character_owned(
            &owner,
            &character_id,
            ActorKnowledgeViewService::resolve_limit(query.limit)?,
            query.cursor,
        )
        .await?;
    let items: Vec<KnowledgeViewItem> = page
        .items
        .iter()
        .map(item_from_record)
        .collect::<Result<_, _>>()?;
    Ok(Json(finish_builder(
        ListCharacterKnowledgeResponse::builder()
            .items(map_wire::<Vec<ListedItem>>(items)?)
            .pagination(listed_pagination(&page)?)
            .try_into(),
    )?))
}

/// `GET /v1/daemon/characters/{character_id}/knowledge/{entry_id}`
pub async fn get_knowledge_entry(
    State(state): State<WorkspaceState>,
    Path((character_id, entry_id)): Path<(String, String)>,
) -> Result<Json<KnowledgeEntryDetail>, NexusApiError> {
    let owner = require_creator(&state)?;
    let record = nexus_local_db::get_actor_knowledge_entry(
        state.pool_or_uninit()?,
        &owner,
        &character_id,
        &entry_id,
    )
    .await?
    .ok_or_else(|| NexusApiError::NotFound(format!("knowledge_entry {entry_id}")))?;
    Ok(Json(detail_from_record(&record)?))
}

/// `PATCH /v1/daemon/characters/{character_id}/knowledge/{entry_id}`
pub async fn patch_knowledge_entry(
    State(state): State<WorkspaceState>,
    Path((character_id, entry_id)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<KnowledgeEntryDetail>, NexusApiError> {
    let raw = parse_request_object(&body)?;
    let req: UpdateKnowledgeEntryRequest =
        serde_json::from_value(serde_json::Value::Object(raw.clone())).map_err(|err| {
            NexusApiError::BadRequest {
                code: "invalid_input".into(),
                message: err.to_string(),
            }
        })?;
    if knowledge_patch_is_empty(&raw) {
        return Err(NexusApiError::BadRequest {
            code: "invalid_input".into(),
            message: "patch must include at least one mutable field".into(),
        });
    }
    let owner = require_creator(&state)?;
    let pool = state.pool_or_uninit()?;
    let _activity = state
        .actor_sessions()
        .admit_character_activity(pool, &owner, &character_id)
        .await?;
    let patch = build_actor_knowledge_patch(&raw, &req)?;
    let record = nexus_local_db::update_actor_knowledge_entry(
        pool,
        &owner,
        &character_id,
        &entry_id,
        req.expected_revision,
        patch,
    )
    .await?;
    Ok(Json(detail_from_record(&record)?))
}

/// `DELETE /v1/daemon/characters/{character_id}/knowledge/{entry_id}`
pub async fn delete_knowledge_entry(
    State(state): State<WorkspaceState>,
    Path((character_id, entry_id)): Path<(String, String)>,
    Query(query): Query<DeleteKnowledgeEntryQuery>,
) -> Result<StatusCode, NexusApiError> {
    let owner = require_creator(&state)?;
    let pool = state.pool_or_uninit()?;
    let _activity = state
        .actor_sessions()
        .admit_character_activity(pool, &owner, &character_id)
        .await?;
    nexus_local_db::delete_actor_knowledge_entry(
        pool,
        &owner,
        &character_id,
        &entry_id,
        query.expected_revision,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

