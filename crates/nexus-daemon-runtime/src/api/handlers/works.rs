//! Works API handlers (V1.33 §7.2).
//!
//! Endpoints:
//! - `POST   /v1/local/works` — Create Work (idempotent on `client_request_id`)
//! - `GET    /v1/local/works` — List Works (filters: status, `intake_status`, limit, offset)
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

/// Stable API representation of a Work record (R-V133P1-10).
///
/// Decoupled from `WorkRecord` (DB row) to prevent leaking internal fields
/// like `creator_id` and `workspace_slug` to API consumers.
///
/// JSON columns (`creative_brief`, `inspiration_log`, `schedule_ids`) are
/// parsed from their stored text form into structured JSON types so that
/// API consumers receive native JSON values rather than escaped strings
/// (T1 contract §7.2).
#[derive(Debug, Serialize)]
pub struct WorkApiDto {
    pub work_id: String,
    pub status: String,
    pub title: String,
    pub long_term_goal: String,
    pub initial_idea: String,
    pub creative_brief: Option<serde_json::Value>,
    pub intake_status: String,
    pub world_id: Option<String>,
    pub story_ref: Option<String>,
    pub inspiration_log: Vec<serde_json::Value>,
    pub primary_preset_id: String,
    pub schedule_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<WorkRecord> for WorkApiDto {
    fn from(r: WorkRecord) -> Self {
        // Parse JSON columns. Best-effort: if a column is malformed, fall back
        // to a sensible default rather than 500. The DB writes these via Rust
        // types so they should be valid JSON.
        let creative_brief = r
            .creative_brief
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());
        let inspiration_log = serde_json::from_str(&r.inspiration_log).unwrap_or_default();
        let schedule_ids = serde_json::from_str(&r.schedule_ids).unwrap_or_default();

        Self {
            work_id: r.work_id,
            status: r.status,
            title: r.title,
            long_term_goal: r.long_term_goal,
            initial_idea: r.initial_idea,
            creative_brief,
            intake_status: r.intake_status,
            world_id: r.world_id,
            story_ref: r.story_ref,
            inspiration_log,
            primary_preset_id: r.primary_preset_id,
            schedule_ids,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkRequest {
    pub title: String,
    pub long_term_goal: String,
    pub initial_idea: String,
    pub world_id: Option<String>,
    pub story_ref: Option<String>,
    pub primary_preset_id: Option<String>,
    /// If provided and a Work with the same creator + `client_request_id` exists,
    /// return the existing `work_id` (idempotent).
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
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::AuthRequired)?;

    let work_id = format!("wrk_{}", Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();
    let preset_id = req
        .primary_preset_id
        .clone()
        .unwrap_or_else(|| "novel-writing".to_string());

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

    // R-V133P1-01: Atomic create + idempotency in single transaction
    let crid = req.client_request_id.as_deref();
    let result = works::create_work_atomic(state.pool(), &record, crid)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    match result {
        // Idempotent replay — existing Work found
        Ok(existing) => Ok((
            StatusCode::OK,
            Json(CreateWorkResponse {
                work_id: existing.work_id,
                status: existing.status,
            }),
        )),
        // New Work created (no client_request_id)
        Err(new) => Ok((
            StatusCode::CREATED,
            Json(CreateWorkResponse {
                work_id: new.work_id,
                status: new.status,
            }),
        )),
    }
}

pub async fn list_works(
    State(state): State<WorkspaceState>,
    Query(query): Query<ListWorksQuery>,
) -> Result<Json<ListWorksResponse>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::AuthRequired)?;

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
) -> Result<Json<WorkApiDto>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    let record = works::get_work(state.pool(), &creator_id, &work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("work {work_id}")))?;

    Ok(Json(WorkApiDto::from(record)))
}

pub async fn patch_work(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
    Json(req): Json<PatchWorkRequest>,
) -> Result<Json<WorkApiDto>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
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

    let updated = works::patch_work(state.pool(), &creator_id, &work_id, &patch, &now)
        .await
        .map_err(|e| match &e {
            nexus_local_db::LocalDbError::MissingVersionKey { .. } => {
                NexusApiError::NotFound(format!("work {work_id}"))
            }
            _ => NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            },
        })?;

    Ok(Json(WorkApiDto::from(updated)))
}

pub async fn append_inspiration(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
    Json(req): Json<AppendInspirationRequest>,
) -> Result<Json<AppendInspirationResponse>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let now = chrono::Utc::now().to_rfc3339();

    // Build JSON for inspiration entry
    let entry = serde_json::json!({
        "at": now.clone(),
        "note": req.note,
    });
    let entry_json = serde_json::to_string(&entry).unwrap_or_default();

    // R-V133P1-04: append_inspiration now uses tx + Rust append and returns updated record
    let updated = works::append_inspiration(state.pool(), &creator_id, &work_id, &entry_json, &now)
        .await
        .map_err(|e| match &e {
            nexus_local_db::LocalDbError::MissingVersionKey { .. } => {
                NexusApiError::NotFound(format!("work {work_id}"))
            }
            _ => NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            },
        })?;

    // Derive count from post-state (not pre-fetch + 1)
    let count = serde_json::from_str::<serde_json::Value>(&updated.inspiration_log)
        .ok()
        .and_then(|v| v.as_array().map(Vec::len))
        .unwrap_or(0);

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
fn read_active_workspace_slug(nexus_home: &std::path::Path, creator_id: &str) -> Option<String> {
    let config_path = nexus_home.join("config.toml");
    let content = std::fs::read_to_string(&config_path).ok()?;
    let config: toml::Value = toml::from_str(&content).ok()?;
    config
        .get("active_workspace_slug_by_creator")
        .and_then(|v| v.get(creator_id))
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
}
