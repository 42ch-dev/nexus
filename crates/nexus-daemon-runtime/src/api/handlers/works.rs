//! Works API handlers (V1.33 §7.2).
//!
//! Endpoints:
//! - `POST   /v1/local/works` — Create Work (idempotent on `client_request_id`)
//! - `GET    /v1/local/works` — List Works (filters: status, intake_status, limit, offset)
//! - `GET    /v1/local/works/{work_id}` — Get one Work
//! - `PATCH  /v1/local/works/{work_id}` — Partial update
//! - `POST   /v1/local/works/{work_id}/inspiration` — Append inspiration log entry

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use nexus_local_db::works::{self, WorkListFilters, WorkPatch, WorkRecord};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── Request / Response types ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateWorkRequest {
    pub title: String,
    pub long_term_goal: String,
    pub initial_idea: String,
    pub world_id: Option<String>,
    pub story_ref: Option<String>,
    pub primary_preset_id: Option<String>,
    /// If provided and a Work with the same creator + client_request_id exists,
    /// return the existing work_id (idempotent).
    pub client_request_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateWorkResponse {
    pub work_id: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct ListWorksQuery {
    pub status: Option<String>,
    pub intake_status: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ListWorksResponse {
    pub works: Vec<WorkSummary>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct WorkSummary {
    pub work_id: String,
    pub title: String,
    pub status: String,
    pub intake_status: String,
    pub primary_preset_id: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct PatchWorkRequest {
    pub title: Option<String>,
    pub long_term_goal: Option<String>,
    pub creative_brief: Option<String>,
    pub intake_status: Option<String>,
    pub status: Option<String>,
    pub world_id: Option<Option<String>>,
    pub story_ref: Option<Option<String>>,
    pub primary_preset_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AppendInspirationRequest {
    pub note: String,
}

#[derive(Debug, Serialize)]
pub struct AppendInspirationResponse {
    pub work_id: String,
    pub inspiration_count: usize,
}

// ─── Handlers ──────────────────────────────────────────────────────────────

pub async fn create_work(
    State(state): State<WorkspaceState>,
    Json(req): Json<CreateWorkRequest>,
) -> Result<(StatusCode, Json<CreateWorkResponse>), NexusApiError> {
    let creator_id = read_active_creator_id(state.nexus_home())
        .ok_or(NexusApiError::Uninitialized)?;
    let workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::Uninitialized)?;

    let work_id = format!("wrk_{}", Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();
    let preset_id = req
        .primary_preset_id
        .clone()
        .unwrap_or_else(|| "novel-writing".to_string());

    // Idempotency check
    if let Some(ref crid) = req.client_request_id {
        let existing = works::find_work_by_client_request_id(state.pool(), &creator_id, crid)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?;
        if let Some(record) = existing {
            return Ok((
                StatusCode::OK,
                Json(CreateWorkResponse {
                    work_id: record.work_id,
                    status: record.status,
                }),
            ));
        }
    }

    let record = WorkRecord {
        work_id: work_id.clone(),
        creator_id: creator_id.clone(),
        workspace_slug,
        status: "active".to_string(),
        title: req.title,
        long_term_goal: req.long_term_goal,
        initial_idea: req.initial_idea,
        creative_brief: None,
        intake_status: "pending".to_string(),
        world_id: req.world_id,
        story_ref: req.story_ref,
        inspiration_log: String::from("[]"),
        primary_preset_id: preset_id,
        schedule_ids: String::from("[]"),
        created_at: now.clone(),
        updated_at: now.clone(),
    };

    works::create_work(state.pool(), &record)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    // Record idempotency if client_request_id provided
    if let Some(ref crid) = req.client_request_id {
        let _ = works::record_idempotency(state.pool(), &creator_id, crid, &work_id, &now).await;
    }

    Ok((
        StatusCode::CREATED,
        Json(CreateWorkResponse {
            work_id,
            status: "active".to_string(),
        }),
    ))
}

pub async fn list_works(
    State(state): State<WorkspaceState>,
    Query(query): Query<ListWorksQuery>,
) -> Result<Json<ListWorksResponse>, NexusApiError> {
    let creator_id = read_active_creator_id(state.nexus_home())
        .ok_or(NexusApiError::Uninitialized)?;
    let workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::Uninitialized)?;

    let filters = WorkListFilters {
        status: query.status,
        intake_status: query.intake_status,
        limit: query.limit,
        offset: query.offset,
    };

    let records = works::list_works(state.pool(), &creator_id, &workspace_slug, &filters)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    let total = records.len();
    let works_list: Vec<WorkSummary> = records
        .into_iter()
        .map(|r| WorkSummary {
            work_id: r.work_id,
            title: r.title,
            status: r.status,
            intake_status: r.intake_status,
            primary_preset_id: r.primary_preset_id,
            updated_at: r.updated_at,
        })
        .collect();

    Ok(Json(ListWorksResponse {
        works: works_list,
        total,
    }))
}

pub async fn get_work(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
) -> Result<Json<WorkRecord>, NexusApiError> {
    let creator_id = read_active_creator_id(state.nexus_home())
        .ok_or(NexusApiError::Uninitialized)?;

    let record = works::get_work(state.pool(), &creator_id, &work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("work {work_id}")))?;

    Ok(Json(record))
}

pub async fn patch_work(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
    Json(req): Json<PatchWorkRequest>,
) -> Result<Json<WorkRecord>, NexusApiError> {
    let creator_id = read_active_creator_id(state.nexus_home())
        .ok_or(NexusApiError::Uninitialized)?;
    let now = chrono::Utc::now().to_rfc3339();

    let patch = WorkPatch {
        title: req.title,
        long_term_goal: req.long_term_goal,
        creative_brief: req.creative_brief.map(Some),
        intake_status: req.intake_status,
        status: req.status,
        world_id: req.world_id,
        story_ref: req.story_ref,
        primary_preset_id: req.primary_preset_id,
        schedule_ids: None,
    };

    works::patch_work(state.pool(), &creator_id, &work_id, &patch, &now)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    let record = works::get_work(state.pool(), &creator_id, &work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("work {work_id}")))?;

    Ok(Json(record))
}

pub async fn append_inspiration(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
    Json(req): Json<AppendInspirationRequest>,
) -> Result<Json<AppendInspirationResponse>, NexusApiError> {
    let creator_id = read_active_creator_id(state.nexus_home())
        .ok_or(NexusApiError::Uninitialized)?;
    let now = chrono::Utc::now().to_rfc3339();

    // Verify work exists
    let record = works::get_work(state.pool(), &creator_id, &work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("work {work_id}")))?;

    // Build JSON for inspiration entry
    let entry = serde_json::json!({
        "at": now.clone(),
        "note": req.note,
    });
    let entry_json = serde_json::to_string(&entry).unwrap_or_default();

    works::append_inspiration(state.pool(), &creator_id, &work_id, &entry_json, &now)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    // Count existing inspirations
    let count = serde_json::from_str::<serde_json::Value>(&record.inspiration_log)
        .map(|v| v.as_array().map_or(1, |a| a.len() + 1))
        .unwrap_or(1);

    Ok(Json(AppendInspirationResponse {
        work_id,
        inspiration_count: count,
    }))
}

// ─── Helpers ───────────────────────────────────────────────────────────────

/// Read active `creator_id` from CLI config.
fn read_active_creator_id(nexus_home: &std::path::Path) -> Option<String> {
    let config_path = nexus_home.join("config.toml");
    let content = std::fs::read_to_string(&config_path).ok()?;
    let config: toml::Value = toml::from_str(&content).ok()?;
    config
        .get("active_creator_id")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
}

/// Read active workspace slug from CLI config.
fn read_active_workspace_slug(
    nexus_home: &std::path::Path,
    creator_id: &str,
) -> Option<String> {
    let config_path = nexus_home.join("config.toml");
    let content = std::fs::read_to_string(&config_path).ok()?;
    let config: toml::Value = toml::from_str(&content).ok()?;
    config
        .get("active_workspace_slug_by_creator")
        .and_then(|v| v.get(creator_id))
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
}
