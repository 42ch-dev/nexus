//! Thin findings HTTP adapters over the guarded core findings service.
//!
//! Endpoints translate the wire envelope to `nexus_core` findings operations
//! and back; lifecycle validation, enum validation, batch triage semantics,
//! stale reporting and retention prune live in the core (P1-T3). The watcher
//! schedule itself is P3-T2; only the query surface lives here.

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use nexus_contracts::{
    BatchUpdateFindingsRequest, BatchUpdateFindingsResponse, FindingDetailResponse, PaginationInfo,
    UpdateFindingRequest,
};
use nexus_core::execution::schedules::stale_findings::{
    DEFAULT_STALE_THRESHOLD_SECS, ENV_STALE_THRESHOLD_SECS,
};
use serde::{Deserialize, Serialize};

// ─── Request / Response types ──────────────────────────────────────────────

/// API representation of a Finding record (wire copy of the core
/// `FindingDetailResponse` projection).
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

impl From<FindingDetailResponse> for FindingApiDto {
    fn from(f: FindingDetailResponse) -> Self {
        Self {
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
            routing_hint: f.routing_hint,
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

impl CreateFindingRequest {
    fn into_core(self) -> nexus_core::CreateFindingRequest {
        nexus_core::CreateFindingRequest {
            chapter: self.chapter,
            severity: self.severity,
            title: self.title,
            description: self.description,
            target_executor: self.target_executor,
            kind: self.kind,
            rule_suggestion: self.rule_suggestion,
        }
    }
}

/// Wire→core bridge for the findings PATCH: the generated
/// `UpdateFindingRequest` carries the tri-state `rule_suggestion`
/// (`Option<serde_json::Value>`, presence-preserving via the
/// `x-nexus-tri-state` generator attribute); this maps it onto the core
/// `Option<Option<String>>` domain carrier and rejects any non-string value
/// before any stored effect. The handwritten duplicate struct is retired —
/// the schema is the single wire definition (R-V1190-FINDINGS-TRISTATE-DUP).
/// Free function: the carrier type is generated (E0116 forbids a local
/// inherent impl on it), unlike the file's local-struct `into_core` methods.
fn update_finding_request_into_core(
    request: UpdateFindingRequest,
) -> Result<nexus_core::UpdateFindingRequest, NexusApiError> {
    let rule_suggestion = match request.rule_suggestion {
        None => None,
        Some(serde_json::Value::Null) => Some(None),
        Some(serde_json::Value::String(text)) => Some(Some(text)),
        Some(other) => {
            return Err(NexusApiError::BadRequest {
                code: "invalid_input".into(),
                message: format!(
                    "invalid rule_suggestion: expected a string, null, or omission, got {}",
                    if other.is_null() {
                        "null"
                    } else {
                        "a non-string JSON value"
                    }
                ),
            });
        }
    };
    Ok(nexus_core::UpdateFindingRequest {
        severity: request.severity,
        status: request.status,
        title: request.title,
        description: request.description,
        target_executor: request.target_executor,
        kind: request.kind,
        rule_suggestion,
    })
}

/// List findings query parameters.
///
/// R-V149P0-01 (V1.50): `status` accepts either a single status or a
/// comma-separated list (e.g. `?status=open,triaged`). Unknown tokens
/// surface as `INVALID_INPUT` (422).
///
/// F-P2 (V1.64): pagination switched from `offset` to an opaque `cursor`.
#[derive(Debug, Deserialize)]
pub struct ListFindingsQuery {
    pub chapter: Option<i64>,
    pub status: Option<String>,
    pub severity: Option<String>,
    pub limit: Option<u32>,
    /// Opaque cursor returned by the previous response's `pagination.next_cursor`.
    pub cursor: Option<String>,
}

/// F-P2 (V1.64): cursor-paginated findings list response.
///
/// New list endpoints use the canonical `items` array key (convention §4);
/// the `pagination` envelope reuses the shared `PaginationInfo`.
#[derive(Debug, Serialize)]
pub struct ListFindingsResponse {
    pub items: Vec<FindingApiDto>,
    pub pagination: PaginationInfo,
}

// ─── Wire adapters ─────────────────────────────────────────────────────────

/// Map a core findings fault onto the legacy HTTP classification.
///
/// `InvalidInput.field` carries the stable public code (`invalid_input`,
/// `invalid_transition`, `too_many_findings` — HTTP 422); `NotFound`
/// resources round-trip verbatim; the legacy internal codes (`DATABASE_ERROR`,
/// `FINDING_CREATE_FAILED`) ride verbatim as `<CODE>: <message>` and are
/// re-emitted with the original code; the core `local_db_err` lowercase
/// `database_error: …` carrier is re-classified as the legacy
/// `DATABASE_ERROR`; every other internal category keeps the shared
/// `CORE_ERROR` shape.
pub(crate) fn findings_error(error: nexus_core::CoreError) -> NexusApiError {
    match error {
        nexus_core::CoreError::InvalidInput { field, reason } => NexusApiError::BadRequest {
            code: field,
            message: reason,
        },
        nexus_core::CoreError::NotFound { resource } => NexusApiError::NotFound(resource),
        nexus_core::CoreError::Internal { category } => match category.split_once(": ") {
            Some((code, message)) if matches!(code, "DATABASE_ERROR" | "FINDING_CREATE_FAILED") => {
                NexusApiError::Internal {
                    code: code.to_owned(),
                    message: message.to_owned(),
                }
            }
            // Work-lookup storage failures ride the core `local_db_err`
            // lowercase carrier (`database_error: …`); the legacy surface
            // classified them as `DATABASE_ERROR`, so re-emit that code.
            Some(("database_error", message)) => NexusApiError::Internal {
                code: "DATABASE_ERROR".to_owned(),
                message: message.to_owned(),
            },
            _ => nexus_core::CoreError::Internal { category }.into(),
        },
        other => other.into(),
    }
}

/// `POST /v1/daemon/works/{work_id}/findings` — create a finding.
pub async fn create_finding_handler(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
    Json(body): Json<CreateFindingRequest>,
) -> Result<(StatusCode, Json<FindingApiDto>), NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let f = core
        .create_finding(&principal, work_id, body.into_core())
        .await
        .map_err(findings_error)?;
    Ok((StatusCode::CREATED, Json(f.into())))
}

/// `GET /v1/daemon/works/{work_id}/findings` — list findings.
///
/// F-P2 (V1.64): the response is cursor-paginated (`{ items, pagination }`);
/// the opaque cursor encodes the row offset; clients MUST NOT parse it
/// (convention §2).
pub async fn list_findings_handler(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
    Query(query): Query<ListFindingsQuery>,
) -> Result<Json<ListFindingsResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let result = core
        .list_findings(
            &principal,
            work_id,
            nexus_core::ListFindingsQuery {
                chapter: query.chapter,
                status: query.status,
                severity: query.severity,
                limit: query.limit,
                cursor: query.cursor,
            },
        )
        .await
        .map_err(findings_error)?;
    Ok(Json(ListFindingsResponse {
        items: result.items.into_iter().map(Into::into).collect(),
        pagination: result.pagination,
    }))
}

/// `GET /v1/daemon/works/{work_id}/findings/{finding_id}` — get one finding.
pub async fn get_finding_handler(
    State(state): State<WorkspaceState>,
    Path((work_id, finding_id)): Path<(String, String)>,
) -> Result<Json<FindingApiDto>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let f = core
        .get_work_finding(&principal, work_id, finding_id)
        .await
        .map_err(findings_error)?;
    Ok(Json(f.into()))
}

/// `GET /v1/daemon/findings/{finding_id}` — get one finding, creator-scoped.
///
/// V1.48 P2: added so the CLI `creator works findings accept <finding_id>`
/// command can resolve a finding by ID alone (without the caller knowing
/// the `work_id` upfront). Mirrors [`get_finding_handler`] but skips the
/// work-ownership precheck; the core lookup is already creator-scoped.
pub async fn get_finding_creator_scoped_handler(
    State(state): State<WorkspaceState>,
    Path(finding_id): Path<String>,
) -> Result<Json<FindingApiDto>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let f = core
        .get_finding(&principal, finding_id)
        .await
        .map_err(findings_error)?;
    Ok(Json(f.into()))
}

/// `PATCH /v1/daemon/works/{work_id}/findings/{finding_id}` — update a finding.
///
/// V1.49 F6 (`findings-lifecycle.md` §2.1): when the patch moves `status`,
/// the core validates the lifecycle transition. Illegal transitions surface
/// as HTTP `422` with the stable error code `INVALID_TRANSITION`; invalid
/// PATCH enum values surface as `422 INVALID_INPUT`.
///
/// V1.49 P0 W-1 (qc1 S-1): a self-loop — `status: "<current>"` on a finding
/// already in that state — is **rejected** as `INVALID_TRANSITION`.
pub async fn update_finding_handler(
    State(state): State<WorkspaceState>,
    Path((work_id, finding_id)): Path<(String, String)>,
    Json(body): Json<UpdateFindingRequest>,
) -> Result<Json<FindingApiDto>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    // Work-ownership precheck stays on the `{work_id}` route; the core update
    // itself is creator-scoped.
    core.get_work(&principal, work_id)
        .await
        .map_err(findings_error)?;
    let update = update_finding_request_into_core(body)?;
    let f = core
        .update_finding(&principal, finding_id, update)
        .await
        .map_err(findings_error)?;
    Ok(Json(f.into()))
}

/// `PATCH /v1/daemon/findings/batch` — bulk update status and/or `target_executor`.
///
/// V1.91 P1: additive helper for power-user triage. Creator-scoped; caps at
/// 100 IDs; partial success model (each ID updated independently).
pub async fn batch_update_findings_handler(
    State(state): State<WorkspaceState>,
    Json(raw): Json<serde_json::Value>,
) -> Result<Json<BatchUpdateFindingsResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;

    // V1.92 P-1 T5: body.patch is the generated FindingBatchPatch struct.
    // Codegen does not emit #[serde(deny_unknown_fields)], so a handler-side
    // check on the raw JSON enforces `additionalProperties: false` from the
    // schema and returns 422 for unknown patch keys before parsing.
    validate_batch_patch_keys(&raw)?;

    let body: BatchUpdateFindingsRequest =
        serde_json::from_value(raw).map_err(|e| NexusApiError::BadRequest {
            code: "invalid_input".to_string(),
            message: format!("invalid request body: {e}"),
        })?;

    let response = core
        .batch_update_findings(&principal, body)
        .await
        .map_err(findings_error)?;
    Ok(Json(response))
}

/// Enforce the `finding-batch-patch.schema.json` contract that only
/// `status` and `target_executor` are permitted keys.
///
/// The generated `FindingBatchPatch` does not carry
/// `#[serde(deny_unknown_fields)]`, so unknown keys are silently ignored by
/// serde. This helper inspects the raw JSON `patch` object and rejects any
/// keys outside the allowed set with a 422 `invalid_input` error.
///
/// # Errors
///
/// Returns `NexusApiError::BadRequest` (mapped to 422) if `patch` is missing,
/// not an object, or contains any key other than `status` or `target_executor`.
fn validate_batch_patch_keys(raw: &serde_json::Value) -> Result<(), NexusApiError> {
    let patch = raw.get("patch").ok_or_else(|| NexusApiError::BadRequest {
        code: "invalid_input".to_string(),
        message: "missing patch object".to_string(),
    })?;

    let obj = patch.as_object().ok_or_else(|| NexusApiError::BadRequest {
        code: "invalid_input".to_string(),
        message: "patch must be an object".to_string(),
    })?;

    for key in obj.keys() {
        if key != "status" && key != "target_executor" {
            return Err(NexusApiError::BadRequest {
                code: "invalid_input".to_string(),
                message: format!("unknown patch field: {key}"),
            });
        }
    }

    Ok(())
}

/// `DELETE /v1/daemon/works/{work_id}/findings/{finding_id}` — delete a finding.
pub async fn delete_finding_handler(
    State(state): State<WorkspaceState>,
    Path((work_id, finding_id)): Path<(String, String)>,
) -> Result<StatusCode, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    // Work-ownership precheck stays on the `{work_id}` route.
    core.get_work(&principal, work_id)
        .await
        .map_err(findings_error)?;
    core.delete_finding(&principal, finding_id)
        .await
        .map_err(findings_error)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/daemon/works/{work_id}/findings/from-review` — create finding from review verdict (T3).
///
/// This endpoint is called by the orchestration layer after a review stage
/// completes. The request body contains the review verdict fields extracted
/// from the terminal schedule context.
pub async fn create_from_review_handler(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
    Json(body): Json<CreateFindingRequest>,
) -> Result<(StatusCode, Json<FindingApiDto>), NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let f = core
        .create_finding_from_review(&principal, work_id, body.into_core())
        .await
        .map_err(findings_error)?;
    Ok((StatusCode::CREATED, Json(f.into())))
}

// ─── Stale findings (V1.39 P4 T3) ──────────────────────────────────────────

/// Response shape for `GET /v1/daemon/findings/stale` (wire copy of the core
/// stale report).
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

/// `GET /v1/daemon/findings/stale` — list stale open findings for the active creator (V1.39 P4 T3).
///
/// The threshold respects `NEXUS_DAEMON_STALE_FINDINGS_THRESHOLD_SECS`
/// so that operators tuning the watcher get a matching banner without
/// per-call configuration; the env read stays at the adapter because the
/// watcher schedule (P3-T2) is daemon runtime state.
pub async fn list_stale_findings_handler(
    State(state): State<WorkspaceState>,
) -> Result<Json<StaleFindingsResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;

    let threshold_seconds = std::env::var(ENV_STALE_THRESHOLD_SECS)
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_STALE_THRESHOLD_SECS);

    let report = core
        .list_stale_findings(&principal, threshold_seconds)
        .await
        .map_err(findings_error)?;

    Ok(Json(StaleFindingsResponse {
        stale_count: report.stale_count,
        threshold_seconds: report.threshold_seconds,
        now_epoch: report.now_epoch,
        findings: report
            .findings
            .into_iter()
            .map(|r| StaleFindingDto {
                finding_id: r.finding_id,
                work_id: r.work_id,
                severity: r.severity,
                created_at: r.created_at,
                age_seconds: r.age_seconds,
            })
            .collect(),
    }))
}

// ─── Retention prune (V1.49 P3, quality-loop §9.4) ──────────────────────────

/// Query parameters for `POST /v1/daemon/findings/prune`.
#[derive(Debug, Default, Deserialize)]
pub struct PruneFindingsQuery {
    /// Retention window in days; defaults to 90.
    /// `resolved` findings whose `updated_at` is older than
    /// `now - older_than_days` are eligible.
    #[serde(default)]
    pub older_than_days: Option<i64>,
    /// When `true`, return the count of rows that WOULD be deleted without
    /// deleting them.
    #[serde(default)]
    pub dry_run: Option<bool>,
}

/// Response for `POST /v1/daemon/findings/prune`.
#[derive(Debug, Serialize)]
pub struct PruneFindingsResponse {
    /// Number of `resolved` rows deleted (or, in dry-run, that WOULD be deleted).
    pub count: u64,
    /// Retention window (days) used for the cutoff.
    pub older_than_days: i64,
    /// Whether this was a dry-run (no rows deleted).
    pub dry_run: bool,
    /// Server-side epoch used as `now` for the cutoff calculation.
    pub now_epoch: i64,
}

/// `POST /v1/daemon/findings/prune` — prune (or preview) `resolved` findings
/// older than the retention window (V1.49 P3, `novel-writing/quality-loop.md`
/// §9.4).
pub async fn prune_findings_handler(
    State(state): State<WorkspaceState>,
    Query(query): Query<PruneFindingsQuery>,
) -> Result<Json<PruneFindingsResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let outcome = core
        .prune_findings(
            &principal,
            query.older_than_days,
            query.dry_run.unwrap_or(false),
        )
        .await
        .map_err(findings_error)?;
    Ok(Json(PruneFindingsResponse {
        count: outcome.count,
        older_than_days: outcome.older_than_days,
        dry_run: outcome.dry_run,
        now_epoch: outcome.now_epoch,
    }))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    /// G#3: the hand-maintained allowed-key list in [`validate_batch_patch_keys`]
    /// must stay in sync with the generated `FindingBatchPatch` struct. If a new
    /// field is added to the schema without updating the validator, this test
    /// fails loudly.
    #[test]
    fn batch_patch_allowed_keys_match_generated_struct_fields() {
        let patch = nexus_contracts::FindingBatchPatch {
            status: Some("open".to_string()),
            target_executor: Some("write".to_string()),
        };
        let value = serde_json::to_value(&patch).expect("serialize FindingBatchPatch");
        let object = value.as_object().expect("patch serializes to object");

        let allowed: HashSet<&'static str> = ["status", "target_executor"].into_iter().collect();
        let actual: HashSet<&str> = object.keys().map(String::as_str).collect();

        assert_eq!(
            allowed, actual,
            "validate_batch_patch_keys allowed set must match FindingBatchPatch serde field names"
        );
    }
}
