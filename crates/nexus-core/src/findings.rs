//! Work findings (quality-loop §2), including batch updates, stale reporting,
//! retention prune and rule-suggestion authoring, over the guarded core
//! workspace. Extracted from the legacy daemon `findings.rs` handlers
//! (P1-T3); the watcher schedule itself is P3-T2 while mutation/query live
//! here. HTTP envelopes stay in adapters.

use nexus_contracts::{
    BatchUpdateFindingsRequest, BatchUpdateFindingsResponse, FindingDetailResponse, PaginationInfo,
};
use nexus_local_db::findings::{
    self, Finding, FindingListFilters, FindingPatch, ReviewVerdictFinding,
};

use crate::error::local_db_err;
use crate::{CoreError, CoreResult, CoreService, Principal};

/// Findings-family fault carrying the legacy classification until the adapter
/// boundary. `BadRequest.code` holds the stable public code (`invalid_input`,
/// `invalid_transition`, `too_many_findings` — HTTP 422).
#[derive(Debug, thiserror::Error)]
enum FindingsFault {
    #[error("{message}")]
    BadRequest { code: String, message: String },
    #[error("{0}")]
    NotFound(String),
    /// Legacy internal classification (`DATABASE_ERROR`,
    /// `FINDING_CREATE_FAILED`) carried verbatim as `<CODE>: <message>`.
    #[error("{code}: {message}")]
    Internal { code: String, message: String },
    #[error(transparent)]
    Core(#[from] CoreError),
}

impl From<FindingsFault> for CoreError {
    fn from(error: FindingsFault) -> Self {
        match error {
            FindingsFault::BadRequest { code, message } => Self::InvalidInput {
                field: code,
                reason: message,
            },
            FindingsFault::NotFound(resource) => Self::NotFound { resource },
            FindingsFault::Internal { code, message } => Self::Internal {
                category: format!("{code}: {message}"),
            },
            FindingsFault::Core(error) => error,
        }
    }
}

/// Map a findings DAO error onto the family fault, mirroring the legacy
/// single-PATCH/list mapping: typed lifecycle/enum variants ride their stable
/// public codes, validation rejections ride `invalid_input`, and everything
/// else keeps the shared storage carrier (`database_error: …`, re-classified
/// to `DATABASE_ERROR` by the daemon adapter).
fn findings_db_err(error: nexus_local_db::LocalDbError) -> FindingsFault {
    match error {
        nexus_local_db::LocalDbError::IllegalTransition { from, to } => FindingsFault::BadRequest {
            code: "invalid_transition".to_string(),
            message: format!("invalid status transition '{from}' → '{to}'"),
        },
        nexus_local_db::LocalDbError::InvalidEnum {
            field,
            value,
            allowed,
        } => FindingsFault::BadRequest {
            code: "invalid_input".to_string(),
            message: format!(
                "invalid {field} value '{value}'; allowed: {}",
                allowed.join(", ")
            ),
        },
        nexus_local_db::LocalDbError::ValidationError(message) => FindingsFault::BadRequest {
            code: "invalid_input".to_string(),
            message,
        },
        other => FindingsFault::Core(local_db_err(other)),
    }
}

/// Create a finding request body (core-owned; the wire DTO keeps serde
/// defaults at the adapter).
#[derive(Debug, Clone)]
pub struct CreateFindingRequest {
    pub chapter: Option<i64>,
    pub severity: String,
    pub title: String,
    pub description: String,
    pub target_executor: String,
    /// V1.47 §2.1: finding category; defaults to `"craft"` at the wire.
    pub kind: String,
    /// V1.47 §8.2: optional prose rule suggestion.
    pub rule_suggestion: Option<String>,
}

/// Update finding request body (all fields optional; core-owned tri-state).
///
/// V1.48 P3 T3 (R-V147P0-03): `rule_suggestion` distinguishes **absent**
/// (`None` — do not touch the column), **null** (`Some(None)` — clear to SQL
/// NULL) and **value** (`Some(Some(value))` — set). The generated wire type
/// cannot carry this distinction, so the tri-state shape lives here until the
/// `schemas/core/findings-api.schema.json` definition lands (P5-T0).
#[derive(Debug, Clone, Default)]
pub struct UpdateFindingRequest {
    pub severity: Option<String>,
    pub status: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub target_executor: Option<String>,
    pub kind: Option<String>,
    #[allow(clippy::option_option)]
    pub rule_suggestion: Option<Option<String>>,
}

/// Findings list query (cursor-paginated; F-P2 V1.64).
#[derive(Debug, Clone, Default)]
pub struct ListFindingsQuery {
    pub chapter: Option<i64>,
    /// Single status or comma-separated list (R-V149P0-01).
    pub status: Option<String>,
    pub severity: Option<String>,
    pub limit: Option<u32>,
    /// Opaque `v1:`-prefixed offset cursor returned by the previous page.
    pub cursor: Option<String>,
}

/// Cursor-paginated findings list response.
#[derive(Debug)]
pub struct ListFindingsResponse {
    pub items: Vec<FindingDetailResponse>,
    pub pagination: PaginationInfo,
}

/// Per-finding summary entry for [`StaleFindingsResponse`].
#[derive(Debug)]
pub struct StaleFindingEntry {
    pub finding_id: String,
    pub work_id: String,
    pub severity: String,
    pub created_at: i64,
    pub age_seconds: i64,
}

/// Stale open-findings report for the active creator (V1.39 P4 T3).
///
/// The stale threshold is resolved by the caller (the legacy surface reads
/// `NEXUS_DAEMON_STALE_FINDINGS_THRESHOLD_SECS`, default 96h — daemon env
/// concerns stay at the adapter).
#[derive(Debug)]
pub struct StaleFindingsResponse {
    pub stale_count: u64,
    pub threshold_seconds: i64,
    /// Server-side epoch used as `now` for the cutoff calculation.
    pub now_epoch: i64,
    /// Oldest first.
    pub findings: Vec<StaleFindingEntry>,
}

/// Retention prune outcome (V1.49 P3, quality-loop §9.4).
#[derive(Debug)]
pub struct PruneFindingsOutcome {
    /// Rows deleted (or, in dry-run, that would be deleted).
    pub count: u64,
    pub older_than_days: i64,
    pub dry_run: bool,
    pub now_epoch: i64,
}

/// Cursor token prefix (legacy daemon `api/pagination.rs` grammar: `v1:<offset>`).
const CURSOR_PREFIX: &str = "v1:";

/// Decode an opaque offset cursor; `None` decodes to 0. Malformed tokens are
/// rejected with the verbatim legacy `invalid_input` message.
fn decode_offset_cursor(cursor: Option<&String>) -> Result<u32, FindingsFault> {
    let invalid = || FindingsFault::BadRequest {
        code: "invalid_input".to_string(),
        message: "invalid pagination cursor; pass the `next_cursor` value returned by the \
                  previous response unchanged"
            .to_string(),
    };
    match cursor {
        None => Ok(0),
        Some(raw) => {
            let stripped = raw.strip_prefix(CURSOR_PREFIX).ok_or_else(invalid)?;
            stripped.parse::<u32>().map_err(|_| invalid())
        }
    }
}

/// Compute `(next_cursor, has_more)` from a `limit + 1` fetch.
fn offset_page_meta(fetched: usize, limit: u32, offset: u32) -> (Option<String>, bool) {
    let limit_us = usize::try_from(limit).unwrap_or(usize::MAX);
    if fetched > limit_us {
        (
            Some(format!("{CURSOR_PREFIX}{}", offset.saturating_add(limit))),
            true,
        )
    } else {
        (None, false)
    }
}

/// Convert `target_executor` to a human-readable CLI hint (novel-quality-loop
/// §2.2, T4).
#[must_use]
pub fn format_routing_hint(target_executor: &str) -> String {
    match target_executor {
        "write" => "→ write".to_string(),
        "brainstorm" => "→ brainstorm".to_string(),
        "master" => "→ review-master".to_string(),
        _ => "→ none".to_string(),
    }
}

/// Map a DAO finding row onto the contract detail DTO.
fn to_finding_detail(f: Finding) -> FindingDetailResponse {
    FindingDetailResponse {
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

fn finding_patch_from_update(request: UpdateFindingRequest) -> FindingPatch {
    FindingPatch {
        severity: request.severity,
        status: request.status,
        title: request.title,
        description: request.description,
        target_executor: request.target_executor,
        kind: request.kind,
        rule_suggestion: request.rule_suggestion,
    }
}

impl CoreService {
    /// Create a finding on a Work (201 at the adapter).
    ///
    /// The target Work must exist and belong to the active creator
    /// (`resolve_owned_work` before insert, QC2-F-002 — the legacy
    /// create-route hole that accepted arbitrary `work_id` values is closed
    /// here, not at the adapter); ID minting stays delegated to the findings
    /// DAO (R-V139P1-W-2).
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification, [`CoreError::Forbidden`] under read-only core access,
    /// [`CoreError::InvalidInput`] with the legacy public code for validation
    /// rejections, and the storage carrier otherwise.
    pub async fn create_finding(
        &self,
        principal: &Principal,
        work_id: String,
        request: CreateFindingRequest,
    ) -> CoreResult<FindingDetailResponse> {
        self.verify_principal(principal)?;
        self.resolve_owned_work(principal, &work_id).await?;
        self.require_work_write()?;
        // R-V139P1-W-2: delegate ID mint to findings module (single source of truth).
        let finding_id = findings::mint_finding_id();
        let now = chrono::Utc::now().timestamp();
        let f = Finding {
            finding_id: finding_id.clone(),
            work_id,
            chapter: request.chapter,
            severity: request.severity,
            status: "open".to_string(),
            title: request.title,
            description: request.description,
            target_executor: request.target_executor,
            creator_id: principal.creator_id().to_string(),
            kind: request.kind,
            rule_suggestion: request.rule_suggestion,
            created_at: now,
            updated_at: now,
        };
        findings::create_finding(&self.inner.pool, &f)
            .await
            .map_err(findings_db_err)?;
        self.verify_principal(principal)?;
        Ok(to_finding_detail(f))
    }

    /// Create a finding from a review verdict (T3), after Work-ownership
    /// verification.
    ///
    /// # Errors
    /// As [`CoreService::create_finding`]; additionally
    /// [`CoreError::NotFound`] for an unknown or foreign Work and
    /// [`CoreError::Internal`] with the legacy `FINDING_CREATE_FAILED` code
    /// when the verdict insert fails (R-V139P1-W-6 keeps the failure logged).
    ///
    /// # Panics
    /// Panics if the finding row disappears between creation and re-fetch
    /// (database invariant violation — should never happen).
    pub async fn create_finding_from_review(
        &self,
        principal: &Principal,
        work_id: String,
        request: CreateFindingRequest,
    ) -> CoreResult<FindingDetailResponse> {
        self.verify_principal(principal)?;
        self.resolve_owned_work(principal, &work_id).await?;
        self.require_work_write()?;
        let verdict = ReviewVerdictFinding {
            work_id: work_id.clone(),
            chapter: request.chapter,
            severity: request.severity,
            title: request.title,
            description: request.description,
            target_executor: request.target_executor,
            creator_id: principal.creator_id().to_string(),
            kind: request.kind,
            rule_suggestion: request.rule_suggestion,
            // Manual API path — no originating schedule; no idempotency guard.
            source_schedule_id: None,
        };
        let finding_id = findings::create_finding_from_review(&self.inner.pool, &verdict)
            .await
            .map_err(|e| {
                // R-V139P1-W-6: explicitly log from-review hook errors for production debugging.
                tracing::warn!(work_id = %work_id, error = %e, "from-review: failed to create finding");
                FindingsFault::Internal {
                    code: "FINDING_CREATE_FAILED".to_string(),
                    message: e.to_string(),
                }
            })?;
        let f = findings::get_finding(&self.inner.pool, principal.creator_id(), &finding_id)
            .await
            .map_err(findings_db_err)?
            .expect("finding must exist after creation");
        self.verify_principal(principal)?;
        Ok(to_finding_detail(f))
    }

    /// Cursor-paginated findings list for a Work with status/severity filters.
    ///
    /// # Errors
    /// As [`CoreService::create_finding`]; additionally
    /// [`CoreError::NotFound`] for an unknown or foreign Work and
    /// [`CoreError::InvalidInput`] for unknown filter enum values or a
    /// malformed pagination cursor.
    pub async fn list_findings(
        &self,
        principal: &Principal,
        work_id: String,
        query: ListFindingsQuery,
    ) -> CoreResult<ListFindingsResponse> {
        self.verify_principal(principal)?;
        self.resolve_owned_work(principal, &work_id).await?;

        // F-P2 (V1.64): cursor-based pagination. Decode the opaque cursor into
        // the underlying offset, fetch `limit + 1` to detect `has_more`.
        let offset = decode_offset_cursor(query.cursor.as_ref())?;
        let limit = query.limit.unwrap_or(100).min(500);
        let fetch_limit = limit.saturating_add(1);

        let filters = FindingListFilters {
            work_id: Some(work_id),
            chapter: query.chapter,
            status: query.status,
            severity: query.severity,
            limit: Some(fetch_limit),
            offset: Some(offset),
        };
        let mut rows = findings::list_findings(&self.inner.pool, principal.creator_id(), &filters)
            .await
            .map_err(|err| match err {
                nexus_local_db::LocalDbError::InvalidEnum {
                    field,
                    value,
                    allowed,
                } => {
                    tracing::warn!(
                        creator_id = %principal.creator_id(),
                        work_id = %filters.work_id.as_deref().unwrap_or(""),
                        field = %field,
                        value = %value,
                        "findings LIST: invalid enum value in query filter"
                    );
                    findings_db_err(nexus_local_db::LocalDbError::InvalidEnum {
                        field,
                        value,
                        allowed,
                    })
                }
                other => findings_db_err(other),
            })?;

        let (next_cursor, has_more) = offset_page_meta(rows.len(), limit, offset);
        rows.truncate(usize::try_from(limit).unwrap_or(rows.len()));

        let items = rows.into_iter().map(to_finding_detail).collect();
        self.verify_principal(principal)?;
        Ok(ListFindingsResponse {
            items,
            pagination: PaginationInfo {
                limit: i64::from(limit),
                next_cursor,
                has_more,
            },
        })
    }

    /// One finding, Work-ownership verified first.
    ///
    /// The returned row is also bound to the path `work_id`: a finding
    /// stored under a different Work is reported as `NotFound` with the
    /// legacy shape (QC2-F-003 — no cross-work disclosure through the
    /// Work-scoped route).
    ///
    /// # Errors
    /// As [`CoreService::list_findings`] (no filter/cursor faults).
    pub async fn get_work_finding(
        &self,
        principal: &Principal,
        work_id: String,
        finding_id: String,
    ) -> CoreResult<FindingDetailResponse> {
        self.verify_principal(principal)?;
        self.resolve_owned_work(principal, &work_id).await?;
        let f = findings::get_finding(&self.inner.pool, principal.creator_id(), &finding_id)
            .await
            .map_err(findings_db_err)?
            .ok_or_else(|| FindingsFault::NotFound(format!("finding {finding_id}")))?;
        if f.work_id != work_id {
            // Path `work_id` does not match the stored row: keep the legacy
            // 404 shape so the mismatch is indistinguishable from absence.
            return Err(FindingsFault::NotFound(format!("finding {finding_id}")).into());
        }
        Ok(to_finding_detail(f))
    }

    /// One finding, creator-scoped (V1.48 P2 — the CLI accept path resolves a
    /// finding by ID alone; the DAO lookup is already creator-scoped so the
    /// Work-ownership precheck is skipped exactly as before).
    ///
    /// # Errors
    /// As [`CoreService::create_finding`].
    pub async fn get_finding(
        &self,
        principal: &Principal,
        finding_id: String,
    ) -> CoreResult<FindingDetailResponse> {
        self.verify_principal(principal)?;
        let f = findings::get_finding(&self.inner.pool, principal.creator_id(), &finding_id)
            .await
            .map_err(findings_db_err)?
            .ok_or_else(|| FindingsFault::NotFound(format!("finding {finding_id}")))?;
        self.verify_principal(principal)?;
        Ok(to_finding_detail(f))
    }

    /// Patch a finding (V1.49 F6 lifecycle validation; tri-state
    /// `rule_suggestion`). Creator-scoped per the selected core API; Work
    /// ownership on the `{work_id}` route is verified by the caller via
    /// [`CoreService::get_work`] before delegating here.
    ///
    /// # Errors
    /// As [`CoreService::create_finding`]; additionally
    /// [`CoreError::InvalidInput`] with `invalid_transition` for illegal
    /// lifecycle moves (self-loop included, V1.49 P0 W-1) and
    /// [`CoreError::NotFound`] for an unknown finding.
    ///
    /// # Panics
    /// Panics if the finding row disappears between successful update and
    /// re-fetch (database invariant violation — should never happen).
    pub async fn update_finding(
        &self,
        principal: &Principal,
        finding_id: String,
        request: UpdateFindingRequest,
    ) -> CoreResult<FindingDetailResponse> {
        self.verify_principal(principal)?;
        self.require_work_write()?;
        let patch = finding_patch_from_update(request);
        let now = chrono::Utc::now().timestamp();
        let updated = findings::update_finding(
            &self.inner.pool,
            principal.creator_id(),
            &finding_id,
            &patch,
            now,
        )
        .await
        .map_err(|err| match err {
            // V1.49 P0 W-1: the DAO emits typed variants for the PATCH surface.
            // `IllegalTransition` → INVALID_TRANSITION (422); `InvalidEnum` →
            // INVALID_INPUT (422). Both are observed with a structured
            // `tracing::warn!` (qc3 S-2). No string-sniffing.
            nexus_local_db::LocalDbError::IllegalTransition { from, to } => {
                tracing::warn!(
                    creator_id = %principal.creator_id(),
                    finding_id = %finding_id,
                    from = %from,
                    to = %to,
                    "findings PATCH: illegal lifecycle transition"
                );
                findings_db_err(nexus_local_db::LocalDbError::IllegalTransition { from, to })
            }
            nexus_local_db::LocalDbError::InvalidEnum {
                field,
                value,
                allowed,
            } => {
                tracing::warn!(
                    creator_id = %principal.creator_id(),
                    finding_id = %finding_id,
                    field = %field,
                    value = %value,
                    "findings PATCH: invalid enum value"
                );
                findings_db_err(nexus_local_db::LocalDbError::InvalidEnum {
                    field,
                    value,
                    allowed,
                })
            }
            other => findings_db_err(other),
        })?;
        if !updated {
            return Err(CoreError::NotFound {
                resource: format!("finding {finding_id}"),
            });
        }
        let f = findings::get_finding(&self.inner.pool, principal.creator_id(), &finding_id)
            .await
            .map_err(findings_db_err)?
            .expect("finding must exist after successful update");
        self.verify_principal(principal)?;
        Ok(to_finding_detail(f))
    }

    /// Delete a finding. Creator-scoped per the selected core API; Work
    /// ownership on the `{work_id}` route is verified by the caller via
    /// [`CoreService::get_work`] before delegating here.
    ///
    /// # Errors
    /// Returns [`CoreError::NotFound`] for an unknown finding and the same
    /// faults as [`CoreService::create_finding`] otherwise.
    pub async fn delete_finding(
        &self,
        principal: &Principal,
        finding_id: String,
    ) -> CoreResult<()> {
        self.verify_principal(principal)?;
        self.require_work_write()?;
        let deleted =
            findings::delete_finding(&self.inner.pool, principal.creator_id(), &finding_id)
                .await
                .map_err(findings_db_err)?;
        if !deleted {
            return Err(CoreError::NotFound {
                resource: format!("finding {finding_id}"),
            });
        }
        self.verify_principal(principal)
    }

    /// Bulk update status and/or `target_executor` (V1.91 P1 additive triage
    /// helper). Creator-scoped; caps at 100 IDs; partial success model — each
    /// ID updates independently through the same DAO path as the single PATCH
    /// so enum validation and lifecycle enforcement stay identical. A body
    /// with both patch fields absent reports `updated: 0` per the contract.
    ///
    /// # Errors
    /// As [`CoreService::create_finding`]; additionally
    /// [`CoreError::InvalidInput`] for an empty or duplicated ID list and
    /// [`CoreError::InvalidInput`] with `too_many_findings` beyond the cap.
    pub async fn batch_update_findings(
        &self,
        principal: &Principal,
        request: BatchUpdateFindingsRequest,
    ) -> CoreResult<BatchUpdateFindingsResponse> {
        const BATCH_CAP: usize = 100;

        self.verify_principal(principal)?;
        self.require_work_write()?;

        let patch = request.patch;

        if request.finding_ids.is_empty() {
            return Err(FindingsFault::BadRequest {
                code: "invalid_input".to_string(),
                message: "finding_ids must not be empty".to_string(),
            }
            .into());
        }

        if request.finding_ids.len() > BATCH_CAP {
            return Err(FindingsFault::BadRequest {
                code: "too_many_findings".to_string(),
                message: format!(
                    "batch update is capped at {BATCH_CAP} findings; received {}",
                    request.finding_ids.len()
                ),
            }
            .into());
        }

        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        if !request
            .finding_ids
            .iter()
            .all(|id| seen.insert(id.as_str()))
        {
            return Err(FindingsFault::BadRequest {
                code: "invalid_input".to_string(),
                message: "finding_ids must not contain duplicates".to_string(),
            }
            .into());
        }

        // If both fields are absent, the contract says return 200 with updated: 0.
        if patch.status.is_none() && patch.target_executor.is_none() {
            return Ok(BatchUpdateFindingsResponse {
                updated: 0,
                not_found: Vec::new(),
                conflict: Vec::new(),
            });
        }

        let finding_patch = FindingPatch {
            severity: None,
            status: patch.status,
            title: None,
            description: None,
            target_executor: patch.target_executor,
            kind: None,
            rule_suggestion: None,
        };
        let now = chrono::Utc::now().timestamp();

        let mut updated: i64 = 0;
        let mut not_found: Vec<String> = Vec::new();
        let mut conflict: Vec<String> = Vec::new();

        for finding_id in &request.finding_ids {
            match findings::update_finding(
                &self.inner.pool,
                principal.creator_id(),
                finding_id,
                &finding_patch,
                now,
            )
            .await
            {
                Ok(true) => updated += 1,
                Ok(false) => not_found.push(finding_id.clone()),
                Err(nexus_local_db::LocalDbError::IllegalTransition { .. }) => {
                    conflict.push(finding_id.clone());
                }
                Err(other) => {
                    tracing::warn!(
                        creator_id = %principal.creator_id(),
                        finding_id = %finding_id,
                        error = %other,
                        "findings batch PATCH: internal error updating finding"
                    );
                    // Legacy batch surface: validation rejections ride
                    // `invalid_input`; every other fault (invalid enum values
                    // included) classified as `DATABASE_ERROR` via the shared
                    // storage carrier.
                    return Err(match other {
                        nexus_local_db::LocalDbError::ValidationError(message) => {
                            FindingsFault::BadRequest {
                                code: "invalid_input".to_string(),
                                message,
                            }
                        }
                        other => FindingsFault::Core(local_db_err(other)),
                    }
                    .into());
                }
            }
        }

        self.verify_principal(principal)?;
        Ok(BatchUpdateFindingsResponse {
            updated,
            not_found,
            conflict,
        })
    }

    /// Stale open-findings report for the active creator. `threshold_seconds`
    /// is resolved by the caller (daemon env default 96h — see
    /// [`StaleFindingsResponse`]).
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification and the storage carrier otherwise.
    pub async fn list_stale_findings(
        &self,
        principal: &Principal,
        threshold_seconds: i64,
    ) -> CoreResult<StaleFindingsResponse> {
        self.verify_principal(principal)?;
        let now_epoch = chrono::Utc::now().timestamp();
        let rows = findings::list_stale_open_findings(
            &self.inner.pool,
            principal.creator_id(),
            now_epoch,
            threshold_seconds,
        )
        .await
        .map_err(findings_db_err)?;
        let stale: Vec<StaleFindingEntry> = rows
            .into_iter()
            .map(|r| StaleFindingEntry {
                finding_id: r.finding_id,
                work_id: r.work_id,
                severity: r.severity,
                created_at: r.created_at,
                age_seconds: r.age_seconds,
            })
            .collect();
        self.verify_principal(principal)?;
        Ok(StaleFindingsResponse {
            stale_count: stale.len() as u64,
            threshold_seconds,
            now_epoch,
            findings: stale,
        })
    }

    /// Prune (or preview) `resolved` findings older than the retention window
    /// (V1.49 P3). The DAO is global across creators, matching the local-first
    /// single-creator model (one active creator per workspace); principal
    /// verification keeps auth parity with the other endpoints.
    ///
    /// # Errors
    /// As [`CoreService::list_stale_findings`]; additionally
    /// [`CoreError::Forbidden`] under read-only core access — pruning
    /// mutates resolved findings.
    pub async fn prune_findings(
        &self,
        principal: &Principal,
        older_than_days: Option<i64>,
        dry_run: bool,
    ) -> CoreResult<PruneFindingsOutcome> {
        self.verify_principal(principal)?;
        self.require_work_write()?;
        let older_than_days = older_than_days.unwrap_or(findings::RETENTION_DEFAULT_DAYS);
        let retention_seconds = older_than_days.saturating_mul(86_400);
        let now_epoch = chrono::Utc::now().timestamp();

        let count: u64 = if dry_run {
            let n = findings::count_resolved_findings_older_than(
                &self.inner.pool,
                now_epoch,
                retention_seconds,
            )
            .await
            .map_err(findings_db_err)?;
            u64::try_from(n).unwrap_or(0)
        } else {
            u64::from(
                findings::prune_resolved_findings_older_than(
                    &self.inner.pool,
                    now_epoch,
                    retention_seconds,
                )
                .await
                .map_err(findings_db_err)?,
            )
        };
        self.verify_principal(principal)?;
        Ok(PruneFindingsOutcome {
            count,
            older_than_days,
            dry_run,
            now_epoch,
        })
    }
}
