//! Findings API handlers (V1.39 P1 — novel-quality-loop §2).
//!
//! Endpoints:
//! - `POST   /v1/local/works/{work_id}/findings` — Create finding
//! - `GET    /v1/local/works/{work_id}/findings` — List findings (filters: status, severity, limit, offset)
//! - `GET    /v1/local/works/{work_id}/findings/{finding_id}` — Get one finding
//! - `PATCH  /v1/local/works/{work_id}/findings/{finding_id}` — Update finding
//! - `DELETE /v1/local/works/{work_id}/findings/{finding_id}` — Delete finding
//! - `POST   /v1/local/works/{work_id}/findings/from-review` — Create from review verdict (T3)
//! - `GET    /v1/local/findings/{finding_id}` — Get one finding, creator-scoped (V1.48 P2 — accept path)
//! - `GET    /v1/local/findings/stale` — Stale open-findings count for active creator (V1.39 P4 T3)

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::stale_findings_watcher::{DEFAULT_STALE_THRESHOLD_SECS, ENV_STALE_THRESHOLD_SECS};
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
    /// V1.47 §2.1: finding category (`craft`, `continuity`, …).
    pub kind: String,
    /// V1.47 §8.2: optional prose rule suggestion (omitted when `None`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_suggestion: Option<String>,
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
            kind: f.kind,
            rule_suggestion: f.rule_suggestion,
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
    /// V1.47 §2.1: finding category; defaults to `"craft"`.
    #[serde(default = "default_kind")]
    pub kind: String,
    /// V1.47 §8.2: optional prose rule suggestion.
    pub rule_suggestion: Option<String>,
}

fn default_target_executor() -> String {
    "none".to_string()
}

/// V1.47: default `kind` value when the request omits the field.
fn default_kind() -> String {
    "craft".to_string()
}

/// Update finding request body (all fields optional).
#[derive(Debug, Deserialize)]
pub struct UpdateFindingRequest {
    pub severity: Option<String>,
    pub status: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub target_executor: Option<String>,
    /// V1.47: optional new `kind`.
    pub kind: Option<String>,
    /// V1.47: optional new `rule_suggestion`.
    pub rule_suggestion: Option<String>,
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
    // R-V139P1-W-2: delegate ID mint to findings module (single source of truth).
    let finding_id = findings::mint_finding_id();
    let now = chrono::Utc::now().timestamp();
    let f = Finding {
        finding_id: finding_id.clone(),
        work_id: work_id.clone(),
        chapter: body.chapter,
        severity: body.severity,
        status: "open".to_string(),
        title: body.title,
        description: body.description,
        target_executor: body.target_executor,
        creator_id: creator_id.clone(),
        kind: body.kind,
        rule_suggestion: body.rule_suggestion,
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

/// `GET /v1/local/findings/{finding_id}` — get one finding, creator-scoped.
///
/// V1.48 P2: added so the CLI `creator works findings accept <finding_id>`
/// command can resolve a finding by ID alone (without the caller knowing
/// the `work_id` upfront). Mirrors [`get_finding_handler`] but skips the
/// work-ownership precheck; the DAO lookup is already creator-scoped.
pub async fn get_finding_creator_scoped_handler(
    State(state): State<WorkspaceState>,
    Path(finding_id): Path<String>,
) -> Result<Json<FindingApiDto>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
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
        kind: body.kind,
        rule_suggestion: body.rule_suggestion,
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
        kind: body.kind,
        rule_suggestion: body.rule_suggestion,
        // Manual API path — no originating schedule; no idempotency guard.
        source_schedule_id: None,
    };
    let finding_id = findings::create_finding_from_review(state.pool(), &verdict)
        .await
        .map_err(|e| {
            // R-V139P1-W-6: explicitly log from-review hook errors for production debugging.
            tracing::warn!(
                work_id = %work_id,
                error = %e,
                "from-review: failed to create finding"
            );
            NexusApiError::Internal {
                code: "FINDING_CREATE_FAILED".to_string(),
                message: e.to_string(),
            }
        })?;
    let f = findings::get_finding(state.pool(), &creator_id, &finding_id)
        .await?
        .expect("finding must exist after creation");
    Ok((StatusCode::CREATED, Json(f.into())))
}

// ─── Stale findings (V1.39 P4 T3) ──────────────────────────────────────────

/// Response shape for `GET /v1/local/findings/stale`.
///
/// Lists open findings for the active creator that have aged past the
/// stale threshold (default 96h, overridable via `NEXUS_DAEMON_STALE_FINDINGS_THRESHOLD_SECS`).
/// The CLI status banner reads `stale_count` and only renders the banner
/// when it is > 0.
#[derive(Debug, Serialize)]
pub struct StaleFindingsResponse {
    /// Number of open findings older than `threshold_seconds`.
    pub stale_count: u64,
    /// Threshold (seconds) used for the query.
    pub threshold_seconds: i64,
    /// Server-side epoch used as `now` for the cutoff calculation.
    pub now_epoch: i64,
    /// Per-finding summaries (oldest first), used by the CLI to surface
    /// the most-aged item in the banner hint.
    pub findings: Vec<StaleFindingDto>,
}

/// Per-finding summary entry for `StaleFindingsResponse`.
#[derive(Debug, Serialize)]
pub struct StaleFindingDto {
    pub finding_id: String,
    pub work_id: String,
    pub severity: String,
    pub created_at: i64,
    pub age_seconds: i64,
}

/// `GET /v1/local/findings/stale` — list stale open findings for the active creator (V1.39 P4 T3).
///
/// Per-creator scoped (uses `read_active_creator_id`). Returns an empty
/// `findings` list and `stale_count = 0` when no findings have aged past
/// the threshold — the CLI suppresses the banner in that case.
///
/// The threshold respects `NEXUS_DAEMON_STALE_FINDINGS_THRESHOLD_SECS`
/// so that operators tuning the watcher get a matching banner without
/// per-call configuration.
pub async fn list_stale_findings_handler(
    State(state): State<WorkspaceState>,
) -> Result<Json<StaleFindingsResponse>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;

    let threshold_seconds = std::env::var(ENV_STALE_THRESHOLD_SECS)
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_STALE_THRESHOLD_SECS);
    let now_epoch = chrono::Utc::now().timestamp();

    let rows = nexus_local_db::findings::list_stale_open_findings(
        state.pool(),
        &creator_id,
        now_epoch,
        threshold_seconds,
    )
    .await?;

    let findings: Vec<StaleFindingDto> = rows
        .into_iter()
        .map(|r| StaleFindingDto {
            finding_id: r.finding_id,
            work_id: r.work_id,
            severity: r.severity,
            created_at: r.created_at,
            age_seconds: r.age_seconds,
        })
        .collect();

    Ok(Json(StaleFindingsResponse {
        stale_count: findings.len() as u64,
        threshold_seconds,
        now_epoch,
        findings,
    }))
}
