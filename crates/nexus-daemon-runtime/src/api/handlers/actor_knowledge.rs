//! Actor KnowledgeView HTTP handlers (v1.184 P1 Task 3).
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
use nexus_contracts::BlockType;
use nexus_contracts::daemon_api::actor_knowledge::{
    add_knowledge_entry_request::{AddKnowledgeEntryRequest, AddKnowledgeEntryRequestOwnerKind},
    add_knowledge_entry_response::{
        AddKnowledgeEntryResponse, NexusActorKnowledgeViewItem as CreatedItem,
    },
    knowledge_view_item::KnowledgeViewItem,
    list_character_knowledge_query::ListCharacterKnowledgeQuery,
    list_character_knowledge_response::{
        ListCharacterKnowledgeResponse, NexusActorKnowledgeViewItem as ListedItem,
        NexusPaginationInfo as ListedPagination,
    },
    view_request::{NexusActorRef, ViewRequest},
    view_response::{
        NexusActorKnowledgeViewItem as ViewItem, NexusPaginationInfo as ViewPagination,
        ViewResponse,
    },
};
use nexus_knowledge::world_kb::knowledge_entry::{KnowledgeEntryRecord, KnowledgeOwnerRef};
use nexus_knowledge::world_kb::store::{KbStore, KbStoreError};
use nexus_local_db::kb_store::SqliteKbStore;
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
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(wire_err)
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
        "block_type": serde_json::to_value(&record.block_type).map_err(wire_err)?,
        "canonical_name": record.canonical_name,
        "status": record.status,
        "created_at": parse_rfc3339(&record.created_at)?,
    });
    serde_json::from_value(value).map_err(wire_err)
}

fn view_pagination(
    page: &ActorKnowledgePage,
) -> Result<ViewPagination, NexusApiError> {
    finish_builder(
        ViewPagination::builder()
            .limit(i64::from(page.limit))
            .has_more(page.has_more)
            .next_cursor(page.next_cursor.clone())
            .try_into(),
    )
}

fn listed_pagination(
    page: &ActorKnowledgePage,
) -> Result<ListedPagination, NexusApiError> {
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
        KbStoreError::Duplicate { .. } | KbStoreError::Validation(_) | KbStoreError::ValidationLegacy(_) => {
            NexusApiError::BadRequest {
                code: "invalid_input".into(),
                message: err.to_string(),
            }
        }
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
pub async fn add_entry(
    State(state): State<WorkspaceState>,
    body: Bytes,
) -> Result<(StatusCode, Json<AddKnowledgeEntryResponse>), NexusApiError> {
    let req: AddKnowledgeEntryRequest = parse_canonical_json(&body)?;
    let creator_id = require_creator(&state)?;
    let pool = state.pool_or_uninit()?.clone();
    let service = ActorKnowledgeViewService::new(pool.clone());
    let creator_only = req.creator_only.unwrap_or(false);
    let owner = match req.owner_kind {
        AddKnowledgeEntryRequestOwnerKind::World => {
            let world_id = optional_str(req.world_id.as_ref()).ok_or_else(|| {
                NexusApiError::BadRequest {
                    code: "invalid_input".into(),
                    message: "world_id is required for world-owned knowledge".into(),
                }
            })?;
            service
                .require_owned_world(&creator_id, world_id)
                .await?;
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
                .require_owned_character(&creator_id, character_id)
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
            let binding_id = optional_str(req.binding_id.as_ref()).ok_or_else(|| {
                NexusApiError::BadRequest {
                    code: "invalid_input".into(),
                    message: "binding_id is required for binding-owned knowledge".into(),
                }
            })?;
            let world_id = optional_str(req.world_id.as_ref()).ok_or_else(|| {
                NexusApiError::BadRequest {
                    code: "invalid_input".into(),
                    message: "world_id is required for binding-owned knowledge".into(),
                }
            })?;
            service
                .require_owned_character(&creator_id, character_id)
                .await?;
            service.require_owned_world(&creator_id, world_id).await?;
            service
                .require_active_binding(character_id, binding_id, world_id)
                .await?;
            KnowledgeOwnerRef::actor_world_binding(binding_id)
        }
    };
    let block_type: BlockType = map_wire(req.block_type.clone())?;
    let mut record = match &owner {
        KnowledgeOwnerRef::World(id) => KnowledgeEntryRecord::new(id, block_type, req.canonical_name.as_str()),
        KnowledgeOwnerRef::Character(id) => {
            KnowledgeEntryRecord::for_character(id, block_type, req.canonical_name.as_str())
        }
        KnowledgeOwnerRef::ActorWorldBinding(id) => {
            KnowledgeEntryRecord::for_binding(id, block_type, req.canonical_name.as_str())
        }
    };
    record.creator_only = creator_only;
    let store = SqliteKbStore::new(pool);
    store
        .insert_knowledge_entry(record.clone())
        .await
        .map_err(map_insert_err)?;
    let stored = store
        .get_knowledge_entry(&record.entry_id)
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
