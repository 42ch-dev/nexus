//! Findings API handlers (V1.39 P1 — novel-quality-loop §2).
//!
//! Endpoints:
//! - `POST   /v1/local/works/{work_id}/findings` — Create finding
//! - `GET    /v1/local/works/{work_id}/findings` — List findings (filters: status, severity, limit, offset)
//! - `GET    /v1/local/works/{work_id}/findings/{finding_id}` — Get one finding
//! - `PATCH  /v1/local/works/{work_id}/findings/{finding_id}` — Update finding
//! - `DELETE /v1/local/works/{work_id}/findings/{finding_id}` — Delete finding
//! - `POST   /v1/local/works/{work_id}/findings/from-review` — Create from review verdict (T3)

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use nexus_local_db::findings::{
    self, Finding, FindingListFilters, FindingPatch, ReviewVerdictFinding,
};
use nexus_local_db::works;
use serde::{Deserialize, Serialize};

use super::works::read_active_creator_id;

// ─── Request / Response types ──────────────────────────────────────────────

/// API representation of a Finding record.
#[derive(Debug, Serialize)]
pub struct FindingApiDto {
    pub finding_id: String,
    pub work_id: String,
    pub chapter: Option<i64>,
    pub severity: String,
    pub status: String,
    pub title: String,
    pub description: String,
    pub target_executor: String,
    pub created_at: i64,
    pub updated_at: i64,
    /// Routing hint string for CLI display (T4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routing_hint: Option<String>,
}

impl From<Finding> for FindingApiDto {
    fn from(f: Finding) -> Self {
        Self {
            routing_hint: Some(format_routing_hint(&f.target_executor)),
            finding_id: f.finding_id,
            work_id: f.work_id,
            chapter: f.chapter,
            severity: f.severity,
            status: f.status,
            title: f.title,
            description: f.description,
            target_executor: f.target_executor,
            created_at: f.created_at,
            updated_at: f.updated_at,
        }
    }
}

/// Create finding request body.
#[derive(Debug, Deserialize)]
pub struct CreateFindingRequest {
    pub chapter: Option<i64>,
    pub severity: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_target_executor")]
    pub target_executor: String,
}

fn default_target_executor() -> String {
    "none".to_string()
}

/// Update finding request body (all fields optional).
#[derive(Debug, Deserialize)]
pub struct UpdateFindingRequest {
    pub severity: Option<String>,
    pub status: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub target_executor: Option<String>,
}

/// List findings query parameters.
#[derive(Debug, Deserialize)]
pub struct ListFindingsQuery {
    pub chapter: Option<i64>,
    pub status: Option<String>,
    pub severity: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Findings summary for status endpoint (T5).
#[derive(Debug, Serialize)]
pub struct FindingsSummaryDto {
    /// Total open findings count.
    pub open_count: i64,
    /// Severity breakdown.
    pub by_severity: Vec<SeverityCountDto>,
    /// Top 3 open findings with routing hints.
    pub top_findings: Vec<TopFindingDto>,
}

/// Severity count in summary.
#[derive(Debug, Serialize)]
pub struct SeverityCountDto {
    pub severity: String,
    pub count: i64,
}

/// Top finding in summary (T5 §5.5.6).
#[derive(Debug, Serialize)]
pub struct TopFindingDto {
    pub finding_id: String,
    pub title: String,
    pub severity: String,
    pub target_executor: String,
    pub routing_hint: String,
}

// ─── Routing hint (T4) ──────────────────────────────────────────────────────

/// Convert `target_executor` to a human-readable CLI hint.
///
/// Per novel-quality-loop §2.2:
/// - `write` → `→ write` (re-run novel-writing)
/// - `brainstorm` → `→ brainstorm`
/// - `none` → `→ none` (manual resolution)
/// - `master` → `→ review-master`
#[must_use]
pub fn format_routing_hint(target_executor: &str) -> String {
    match target_executor {
        "write" => "→ write".to_string(),
        "brainstorm" => "→ brainstorm".to_string(),
        "master" => "→ review-master".to_string(),
        _ => "→ none".to_string(),
    }
}

// ─── Handlers ───────────────────────────────────────────────────────────────

/// `POST /v1/local/works/{work_id}/findings` — create a finding.
pub async fn create_finding_handler(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
    Json(body): Json<CreateFindingRequest>,
) -> Result<(StatusCode, Json<FindingApiDto>), NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let now = chrono::Utc::now().timestamp();
    let f = Finding {
        finding_id: format!("fnd_{}", uuid::Uuid::new_v4().simple()),
        work_id: work_id.clone(),
        chapter: body.chapter,
        severity: body.severity,
        status: "open".to_string(),
        title: body.title,
        description: body.description,
        target_executor: body.target_executor,
        creator_id: creator_id.clone(),
        created_at: now,
        updated_at: now,
    };
    findings::create_finding(state.pool(), &f).await?;
    Ok((StatusCode::CREATED, Json(f.into())))
}

/// `GET /v1/local/works/{work_id}/findings` — list findings.
pub async fn list_findings_handler(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
    Query(query): Query<ListFindingsQuery>,
) -> Result<Json<Vec<FindingApiDto>>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let filters = FindingListFilters {
        work_id: Some(work_id),
        chapter: query.chapter,
        status: query.status,
        severity: query.severity,
        limit: query.limit,
        offset: query.offset,
    };
    let rows = findings::list_findings(state.pool(), &creator_id, &filters).await?;
    Ok(Json(rows.into_iter().map(FindingApiDto::from).collect()))
}

/// `GET /v1/local/works/{work_id}/findings/{finding_id}` — get one finding.
pub async fn get_finding_handler(
    State(state): State<WorkspaceState>,
    Path((work_id, finding_id)): Path<(String, String)>,
) -> Result<Json<FindingApiDto>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    // Verify work_id ownership
    let _work = works::get_work(state.pool(), &creator_id, &work_id)
        .await?
        .ok_or_else(|| NexusApiError::NotFound(format!("work {work_id}")))?;
    let f = findings::get_finding(state.pool(), &creator_id, &finding_id)
        .await?
        .ok_or_else(|| NexusApiError::NotFound(format!("finding {finding_id}")))?;
    Ok(Json(f.into()))
}

/// `PATCH /v1/local/works/{work_id}/findings/{finding_id}` — update a finding.
///
/// # Panics
/// Panics if the finding row disappears between successful update and re-fetch
/// (database invariant violation — should never happen).
pub async fn update_finding_handler(
    State(state): State<WorkspaceState>,
    Path((work_id, finding_id)): Path<(String, String)>,
    Json(body): Json<UpdateFindingRequest>,
) -> Result<Json<FindingApiDto>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let _work = works::get_work(state.pool(), &creator_id, &work_id)
        .await?
        .ok_or_else(|| NexusApiError::NotFound(format!("work {work_id}")))?;
    let patch = FindingPatch {
        severity: body.severity,
        status: body.status,
        title: body.title,
        description: body.description,
        target_executor: body.target_executor,
    };
    let now = chrono::Utc::now().timestamp();
    let updated =
        findings::update_finding(state.pool(), &creator_id, &finding_id, &patch, now).await?;
    if !updated {
        return Err(NexusApiError::NotFound(format!("finding {finding_id}")));
    }
    let f = findings::get_finding(state.pool(), &creator_id, &finding_id)
        .await?
        .expect("finding must exist after successful update");
    Ok(Json(f.into()))
}

/// `DELETE /v1/local/works/{work_id}/findings/{finding_id}` — delete a finding.
pub async fn delete_finding_handler(
    State(state): State<WorkspaceState>,
    Path((work_id, finding_id)): Path<(String, String)>,
) -> Result<StatusCode, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let _work = works::get_work(state.pool(), &creator_id, &work_id)
        .await?
        .ok_or_else(|| NexusApiError::NotFound(format!("work {work_id}")))?;
    let deleted = findings::delete_finding(state.pool(), &creator_id, &finding_id).await?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(NexusApiError::NotFound(format!("finding {finding_id}")))
    }
}

/// `POST /v1/local/works/{work_id}/findings/from-review` — create finding from review verdict (T3).
///
/// This endpoint is called by the orchestration layer after a review stage
/// completes. The request body contains the review verdict fields extracted
/// from the terminal schedule context.
///
/// # Panics
/// Panics if the finding row disappears between creation and re-fetch
/// (database invariant violation — should never happen).
pub async fn create_from_review_handler(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
    Json(body): Json<CreateFindingRequest>,
) -> Result<(StatusCode, Json<FindingApiDto>), NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    // Verify work ownership
    let _work = works::get_work(state.pool(), &creator_id, &work_id)
        .await?
        .ok_or_else(|| NexusApiError::NotFound(format!("work {work_id}")))?;

    let verdict = ReviewVerdictFinding {
        work_id: work_id.clone(),
        chapter: body.chapter,
        severity: body.severity,
        title: body.title,
        description: body.description,
        target_executor: body.target_executor,
        creator_id: creator_id.clone(),
    };
    let finding_id = findings::create_finding_from_review(state.pool(), &verdict).await?;
    let f = findings::get_finding(state.pool(), &creator_id, &finding_id)
        .await?
        .expect("finding must exist after creation");
    Ok((StatusCode::CREATED, Json(f.into())))
}
