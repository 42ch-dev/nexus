//! Actor `KnowledgeView` HTTP handlers (v1.184 P1 Task 3).
//!
//! Thin translation over the core knowledge commands (v1.190 P2-T1):
//! stored-owner admission, the fail-closed view composition and the guarded
//! Character/binding entry writes (with the shared per-Character activity
//! fence and in-transaction revalidation) live in [`nexus_core`]. Admission
//! is stored owners only; clients never send `owner_creator_id`.

#![allow(clippy::missing_errors_doc)]

use crate::actor_knowledge_view::{ActorKnowledgePage, ActorKnowledgeViewQuery, AdmittedActor};
use crate::api::errors::NexusApiError;
use crate::api::handlers::world_kb_guards::resolve_core_principal;
use crate::workspace::WorkspaceState;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, Uri};
use axum::Json;
use chrono::{DateTime, Utc};
use nexus_contracts::daemon_api::actor_knowledge::{
    add_knowledge_entry_request::AddKnowledgeEntryRequest,
    add_knowledge_entry_response::{
        AddKnowledgeEntryResponse, NexusActorKnowledgeViewItem as CreatedItem,
    },
    knowledge_entry_detail::{KnowledgeEntryDetail, NexusActorKnowledgeViewItem as DetailItem},
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
use nexus_knowledge::world_kb::knowledge_entry::{parse_stored_created_at, KnowledgeEntryRecord};
use nexus_local_db::FieldPatch;
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

fn detail_from_record(
    record: &KnowledgeEntryRecord,
) -> Result<KnowledgeEntryDetail, NexusApiError> {
    let item = item_from_record(record)?;
    let summary = summary_wire_value(record);
    let value = serde_json::json!({
        "item": map_wire::<DetailItem>(item)?,
        "summary": summary,
    });
    serde_json::from_value(value).map_err(wire_err)
}

const KNOWLEDGE_DELETE_MAX_EXPECTED_REVISION: i64 = 9_223_372_036_854_775_806;

fn knowledge_delete_invalid_input(message: impl Into<String>) -> NexusApiError {
    NexusApiError::BadRequest {
        code: "invalid_input".into(),
        message: message.into(),
    }
}

fn parse_delete_expected_revision(uri: &Uri) -> Result<i64, NexusApiError> {
    let query = uri
        .query()
        .ok_or_else(|| knowledge_delete_invalid_input("expected_revision is required"))?;
    let mut expected_revision: Option<&str> = None;
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (pair, ""),
        };
        if key == "expected_revision" {
            if expected_revision.is_some() {
                return Err(knowledge_delete_invalid_input(
                    "duplicate expected_revision query parameter",
                ));
            }
            expected_revision = Some(value);
        } else {
            return Err(knowledge_delete_invalid_input(format!(
                "unexpected query parameter '{key}'"
            )));
        }
    }
    let raw = expected_revision
        .ok_or_else(|| knowledge_delete_invalid_input("expected_revision is required"))?;
    let parsed = raw
        .parse::<i64>()
        .map_err(|_| knowledge_delete_invalid_input("expected_revision must be an integer"))?;
    if !(0..=KNOWLEDGE_DELETE_MAX_EXPECTED_REVISION).contains(&parsed) {
        return Err(knowledge_delete_invalid_input(
            "expected_revision is out of range",
        ));
    }
    Ok(parsed)
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
) -> Result<(Option<&'a str>, FieldPatch<&'a str>), NexusApiError> {
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
    Ok((canonical_name, build_knowledge_summary_patch(raw, req)?))
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

/// `POST /v1/daemon/actor-knowledge/view`
pub async fn view(
    State(state): State<WorkspaceState>,
    body: Bytes,
) -> Result<Json<ViewResponse>, NexusApiError> {
    let req: ViewRequest = parse_canonical_json(&body)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    let page = core
        .actor_knowledge_view(
            &principal,
            &admit_actor(&req.actor_ref),
            ActorKnowledgeViewQuery {
                world_id: req.world_id.to_string(),
                binding_id: optional_str(req.binding_id.as_ref()).map(str::to_string),
                limit: nexus_core::ActorKnowledgeViewService::resolve_limit(req.limit)?,
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
    let raw = parse_request_object(&body)?;
    let req: AddKnowledgeEntryRequest =
        serde_json::from_value(serde_json::Value::Object(raw.clone())).map_err(|err| {
            NexusApiError::BadRequest {
                code: "invalid_input".into(),
                message: err.to_string(),
            }
        })?;
    // The retained wire distinction between an absent and a null `summary`
    // member is a request-shape concern, so it stays at the handler.
    let summary_present = raw.contains_key("summary");
    let (core, principal) = resolve_core_principal(&state).await?;
    let stored = core
        .add_actor_knowledge_entry(&principal, req, summary_present)
        .await?;
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
    let (core, principal) = resolve_core_principal(&state).await?;
    let limit = nexus_core::ActorKnowledgeViewService::resolve_limit(query.limit)?;
    let page = core
        .list_character_knowledge(&principal, character_id, limit, query.cursor)
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
    let (core, principal) = resolve_core_principal(&state).await?;
    let record = core
        .actor_knowledge_entry(&principal, character_id, entry_id)
        .await?;
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
    let (canonical_name_patch, summary_patch) = build_actor_knowledge_patch(&raw, &req)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    let record = core
        .patch_actor_knowledge_entry(
            &principal,
            character_id,
            entry_id,
            req.expected_revision,
            canonical_name_patch,
            summary_patch,
        )
        .await?;
    Ok(Json(detail_from_record(&record)?))
}

/// `DELETE /v1/daemon/characters/{character_id}/knowledge/{entry_id}`
pub async fn delete_knowledge_entry(
    State(state): State<WorkspaceState>,
    Path((character_id, entry_id)): Path<(String, String)>,
    uri: Uri,
) -> Result<StatusCode, NexusApiError> {
    let expected_revision = parse_delete_expected_revision(&uri)?;
    let (core, principal) = resolve_core_principal(&state).await?;
    core.delete_actor_knowledge_entry(&principal, character_id, entry_id, expected_revision)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
