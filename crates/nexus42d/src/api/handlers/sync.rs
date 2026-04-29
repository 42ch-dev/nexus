//! Complex HTTP handlers with orchestration logic exceed line limits.
#![allow(clippy::too_many_lines)]
//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc)]
//! Sync handler — sync status, push, and resolve endpoints
//!
//! Uses `nexus-sync` **Outbox**, **`BundleBuilder`**, and **precheck** on the push path:
//! builds a minimal bundle (with `canonical_hash`), runs `precheck_bundle_with_auth`,
//! then persists via **`Outbox::stage`** (`ready`). This is **offline-first** by default.
//!
//! **Optional eager platform push (ARCH-SYNC-D1):** when `NEXUS_SYNC_EAGER_PUSH=1` and
//! `NEXUS_SYNC_PLATFORM_URL` + `NEXUS_SYNC_PLATFORM_TOKEN` are set (token must satisfy
//! `SyncClient` validation — min 64 chars), the daemon calls `SyncClient::push_bundle`
//! after a successful stage. Failures are logged; the entry stays `ready` for retry.
//! When the env gate is off or unset, behavior is unchanged.
//!
//! **Pull:** `POST /v1/local/sync/pull` uses the same `NEXUS_SYNC_PLATFORM_*` credentials
//! (no `NEXUS_SYNC_EAGER_PUSH` gate) to call `SyncClient::pull_bundles` (`POST /v1/sync/pull`),
//! then applies bundles with `nexus_sync::apply_pull_response_to_outbox` and
//! `Outbox::stage_if_absent`.

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::State;
use axum::Json;
use nexus_contracts::{
    CommandOrigin, CommandStatus, CommandType, DeltaOperation, DeltaType, ManuscriptPhase,
    SyncCommand, SyncPullRequest,
};
use nexus_sync::delta_bundle::{BundleBuilder, LocalDelta};
use nexus_sync::precheck::{
    precheck_bundle_with_auth, AuthContext, LocalState, PrecheckReport, PrecheckResult,
    PrecheckSeverity,
};
use nexus_sync::pull_apply::apply_pull_response_to_outbox;
use nexus_sync::sync_client::SyncClient;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

// ── Response types ───────────────────────────────────────────────

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SyncStatusResponse {
    /// Number of staged entries (ready to be staged, have command but no bundle yet)
    pub staged_count: u64,
    /// Number of ready entries (bundle staged, ready to send)
    pub ready_count: u64,
    /// Number of sent entries (awaiting server acknowledgment)
    pub sent_count: u64,
    /// Number of acknowledged entries (successfully synced)
    pub acked_count: u64,
    /// Number of conflicted entries (need resolution)
    pub conflicted_count: u64,
    /// Number of failed entries (will be retried)
    pub failed_count: u64,
    /// Timestamp of the last successful sync (RFC 3339)
    pub last_sync_at: Option<String>,
}

/// Request body for sync push endpoint.
#[derive(Debug, Deserialize)]
pub struct SyncPushRequest {
    /// Workspace ID for the bundle
    pub workspace_id: String,
    /// World ID for the bundle
    pub world_id: String,
    /// Creator ID submitting the bundle
    pub creator_id: String,
    /// Whether to force push even if precheck would fail
    #[serde(default)]
    pub force: bool,
}

/// Response from sync push endpoint.
#[derive(Debug, Serialize)]
pub struct SyncPushResponse {
    /// Whether the push was successfully staged
    pub success: bool,
    /// Outbox entry ID for tracking
    pub outbox_entry_id: Option<String>,
    /// Bundle ID generated for this push
    pub bundle_id: Option<String>,
    /// Precheck results (if precheck was run)
    pub precheck_result: Option<PrecheckSummary>,
    /// Error message if push failed
    pub error: Option<String>,
}

/// Summary of precheck validation results.
#[derive(Debug, Serialize)]
pub struct PrecheckSummary {
    /// Whether the bundle passes all prechecks
    pub valid: bool,
    /// Number of error-level issues
    pub error_count: usize,
    /// Number of warning-level issues
    pub warning_count: usize,
    /// Human-readable summary of issues
    pub summary: String,
}

/// Request body for sync resolve endpoint.
#[derive(Debug, Deserialize)]
pub struct SyncResolveRequest {
    /// Outbox entry ID to resolve
    pub outbox_entry_id: String,
    /// Resolution strategy: `auto_accept`, `auto_reject`, `manual_review`
    pub resolution: String,
    /// Skip confirmation (for automation)
    #[serde(default)]
    pub force: bool,
}

/// Response from sync resolve endpoint.
#[derive(Debug, Serialize)]
pub struct SyncResolveResponse {
    /// Whether the resolution was applied
    pub success: bool,
    /// Current delivery state of the entry
    pub delivery_state: Option<String>,
    /// Error message if resolution failed
    pub error: Option<String>,
}

/// Response from sync replay endpoint.
#[derive(Debug, Serialize)]
pub struct SyncReplayResponse {
    /// Number of entries eligible for replay
    pub replayable_count: usize,
    /// Summary of each replayable entry
    pub entries: Vec<OutboxEntrySummary>,
}

/// Response from POST /v1/local/sync/pull (daemon-local summary after platform pull).
#[derive(Debug, Serialize)]
pub struct SyncPullLocalResponse {
    pub success: bool,
    pub world_revision: u64,
    pub confirmed_delta_sequence: u64,
    pub bundles_received: usize,
    pub entries_staged: Vec<String>,
    pub skipped_known_bundles: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_up_to_date: Option<bool>,
    pub error: Option<String>,
}

/// Summary of an outbox entry for API responses.
#[derive(Debug, Serialize)]
pub struct OutboxEntrySummary {
    pub outbox_entry_id: String,
    pub bundle_id: String,
    pub delivery_state: String,
    pub retry_count: i64,
    pub last_error: Option<String>,
    pub created_at: String,
}

// ── Handlers ─────────────────────────────────────────────────────

/// Optional sync context stored in `workspace_meta` (`sync_workspace_id`,
/// `sync_world_id`, `sync_creator_id`). When all three are present, push requests
/// must use the same IDs. When none are present, binding checks are skipped.
pub(crate) async fn optional_sync_push_binding(
    state: &WorkspaceState,
) -> Result<Option<(String, String, String)>, NexusApiError> {
    let pool = state.pool();

    let workspace_id: Option<String> =
        sqlx::query_scalar!("SELECT value FROM workspace_meta WHERE key = 'sync_workspace_id'")
            .fetch_optional(pool)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".into(),
                message: format!("Database error: {e}"),
            })?;

    let world_id: Option<String> =
        sqlx::query_scalar!("SELECT value FROM workspace_meta WHERE key = 'sync_world_id'")
            .fetch_optional(pool)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".into(),
                message: format!("Database error: {e}"),
            })?;

    let creator_id: Option<String> =
        sqlx::query_scalar!("SELECT value FROM workspace_meta WHERE key = 'sync_creator_id'")
            .fetch_optional(pool)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".into(),
                message: format!("Database error: {e}"),
            })?;

    match (workspace_id, world_id, creator_id) {
        (None, None, None) => Ok(None),
        (Some(w), Some(wl), Some(c)) => Ok(Some((w, wl, c))),
        _ => Err(NexusApiError::InvalidInput {
            field: "sync_binding".into(),
            reason: "Set all workspace_meta keys sync_workspace_id, sync_world_id, and sync_creator_id, or none — partial binding is invalid".into(),
        }),
    }
}

/// When `NEXUS_SYNC_EAGER_PUSH=1` and URL + token env vars are set, returns config for
/// an optional immediate `SyncClient::push_bundle` after local staging.
fn try_eager_push_config_from_env() -> Option<(String, String)> {
    if std::env::var("NEXUS_SYNC_EAGER_PUSH").ok().as_deref() != Some("1") {
        return None;
    }
    try_platform_sync_credentials_from_env()
}

/// Platform URL + token for `SyncClient` (pull, fork/snapshot proxy, opt-in eager push).
pub(crate) fn try_platform_sync_credentials_from_env() -> Option<(String, String)> {
    let base = std::env::var("NEXUS_SYNC_PLATFORM_URL")
        .ok()
        .filter(|s| !s.is_empty())?;
    let token = std::env::var("NEXUS_SYNC_PLATFORM_TOKEN")
        .ok()
        .filter(|s| !s.is_empty())?;
    Some((base, token))
}

fn map_sync_client_error(e: &nexus_sync::SyncError) -> NexusApiError {
    NexusApiError::Internal {
        code: e.error_code().to_string(),
        message: e.to_string(),
    }
}

fn precheck_summary_from_report(report: &PrecheckReport) -> PrecheckSummary {
    let error_count = report
        .issues
        .iter()
        .filter(|i| i.severity == PrecheckSeverity::Error)
        .count();
    let warning_count = report
        .issues
        .iter()
        .filter(|i| i.severity == PrecheckSeverity::Warning)
        .count();
    PrecheckSummary {
        valid: !report.has_errors(),
        error_count,
        warning_count,
        summary: report.summary(),
    }
}

/// POST /v1/local/sync/pull
///
/// Calls the platform `POST /v1/sync/pull`, then stages returned bundles into the local
/// outbox (skipping duplicate `bundle_id`). Requires `NEXUS_SYNC_PLATFORM_URL` and
/// `NEXUS_SYNC_PLATFORM_TOKEN`.
pub async fn pull(
    State(state): State<WorkspaceState>,
    Json(mut req): Json<SyncPullRequest>,
) -> Result<Json<SyncPullLocalResponse>, NexusApiError> {
    info!(world_id = %req.world_id, "Handling sync pull request");

    if req.world_id.is_empty() {
        return Ok(Json(SyncPullLocalResponse {
            success: false,
            world_revision: 0,
            confirmed_delta_sequence: 0,
            bundles_received: 0,
            entries_staged: vec![],
            skipped_known_bundles: 0,
            is_up_to_date: None,
            error: Some("world_id must not be empty".to_string()),
        }));
    }

    if req.schema_version == 0 {
        req.schema_version = 1;
    }

    let outbox = state.outbox().ok_or_else(|| NexusApiError::Internal {
        code: "SYNC_NOT_CONFIGURED".into(),
        message: "Sync outbox not initialized".to_string(),
    })?;

    if let Some((_, bound_world, _)) = optional_sync_push_binding(&state).await? {
        if req.world_id != bound_world {
            return Ok(Json(SyncPullLocalResponse {
                success: false,
                world_revision: 0,
                confirmed_delta_sequence: 0,
                bundles_received: 0,
                entries_staged: vec![],
                skipped_known_bundles: 0,
                is_up_to_date: None,
                error: Some(format!(
                    "world_id does not match workspace sync binding (expected {bound_world})"
                )),
            }));
        }
    }

    let (base_url, token) = try_platform_sync_credentials_from_env().ok_or_else(|| {
        NexusApiError::InvalidInput {
            field: "platform_sync".into(),
            reason: "Set NEXUS_SYNC_PLATFORM_URL and NEXUS_SYNC_PLATFORM_TOKEN to pull from the platform"
                .into(),
        }
    })?;

    let client = SyncClient::new(&base_url, &token).map_err(|e| map_sync_client_error(&e))?;

    let remote = client
        .pull_bundles(&req)
        .await
        .map_err(|e| map_sync_client_error(&e))?;
    let bundles_received = remote.bundles.len();
    let is_up_to_date = remote.is_up_to_date;

    let summary = apply_pull_response_to_outbox(outbox, &remote)
        .await
        .map_err(|e| map_sync_client_error(&e))?;

    let ts = chrono::Utc::now().to_rfc3339();
    if let Err(e) = sqlx::query!(
        "INSERT OR REPLACE INTO workspace_meta (key, value) VALUES ('last_sync_at', ?)", // sqlx R3: use ? instead of ?1
        ts
    )
    .execute(state.pool())
    .await
    {
        tracing::warn!("Failed to update last_sync_at: {}", e);
    }

    Ok(Json(SyncPullLocalResponse {
        success: true,
        world_revision: summary.world_revision,
        confirmed_delta_sequence: summary.confirmed_delta_sequence,
        bundles_received,
        entries_staged: summary.staged_entry_ids,
        skipped_known_bundles: summary.skipped_duplicate_bundles,
        is_up_to_date,
        error: None,
    }))
}

/// GET /v1/local/sync/status
///
/// Returns real outbox state counts using `nexus-sync::Outbox`.
pub async fn status(
    State(state): State<WorkspaceState>,
) -> Result<Json<SyncStatusResponse>, NexusApiError> {
    info!("Handling sync status request");

    if let Some(outbox) = state.outbox() {
        let staged_count = outbox.count_by_state("staged").await.unwrap_or(0) as u64;
        let ready_count = outbox.count_by_state("ready").await.unwrap_or(0) as u64;
        let sent_count = outbox.count_by_state("sent").await.unwrap_or(0) as u64;
        let acked_count = outbox.count_by_state("acked").await.unwrap_or(0) as u64;
        let conflicted_count = outbox.count_by_state("conflicted").await.unwrap_or(0) as u64;
        let failed_count = outbox.count_by_state("failed").await.unwrap_or(0) as u64;

        // Get last sync timestamp from workspace_meta
        let last_sync_row =
            sqlx::query!("SELECT value FROM workspace_meta WHERE key = 'last_sync_at'")
                .fetch_optional(state.pool())
                .await
                .ok()
                .flatten();
        let last_sync_at = last_sync_row.map(|r| r.value);

        debug!(
            staged_count,
            ready_count,
            sent_count,
            acked_count,
            conflicted_count,
            failed_count,
            last_sync_at = ?last_sync_at,
            "Sync status retrieved from outbox"
        );

        Ok(Json(SyncStatusResponse {
            staged_count,
            ready_count,
            sent_count,
            acked_count,
            conflicted_count,
            failed_count,
            last_sync_at,
        }))
    } else {
        // Outbox not initialized — return zeroed status
        warn!("Sync outbox not initialized, returning empty status");
        Ok(Json(SyncStatusResponse {
            staged_count: 0,
            ready_count: 0,
            sent_count: 0,
            acked_count: 0,
            conflicted_count: 0,
            failed_count: 0,
            last_sync_at: None,
        }))
    }
}

/// POST /v1/local/sync/push
///
/// Build a bundle via [`BundleBuilder`] (includes `canonical_hash`), run
/// `nexus_sync::precheck`, then **`Outbox::stage`** so the entry is `ready`
/// for a later upload. When precheck reports errors and `force` is false, no
/// row is written.
pub async fn push(
    State(state): State<WorkspaceState>,
    Json(req): Json<SyncPushRequest>,
) -> Result<Json<SyncPushResponse>, NexusApiError> {
    info!(
        workspace_id = %req.workspace_id,
        world_id = %req.world_id,
        creator_id = %req.creator_id,
        force = req.force,
        "Handling sync push request"
    );

    let outbox = state.outbox().ok_or_else(|| NexusApiError::Internal {
        code: "SYNC_NOT_CONFIGURED".into(),
        message: "Sync outbox not initialized".to_string(),
    })?;

    // Basic validation before bundle build
    let mut field_errors = Vec::new();
    if req.workspace_id.is_empty() {
        field_errors.push("workspace_id is empty");
    }
    if req.world_id.is_empty() {
        field_errors.push("world_id is empty");
    }
    if req.creator_id.is_empty() {
        field_errors.push("creator_id is empty");
    }
    if !field_errors.is_empty() {
        return Ok(Json(SyncPushResponse {
            success: false,
            outbox_entry_id: None,
            bundle_id: None,
            precheck_result: Some(PrecheckSummary {
                valid: false,
                error_count: field_errors.len(),
                warning_count: 0,
                summary: format!("Issues: {}", field_errors.join("; ")),
            }),
            error: Some(format!("Invalid push request: {}", field_errors.join("; "))),
        }));
    }

    if let Some((bound_wrk, bound_wld, bound_ctr)) = optional_sync_push_binding(&state).await? {
        if req.workspace_id != bound_wrk || req.world_id != bound_wld || req.creator_id != bound_ctr
        {
            return Ok(Json(SyncPushResponse {
                success: false,
                outbox_entry_id: None,
                bundle_id: None,
                precheck_result: Some(PrecheckSummary {
                    valid: false,
                    error_count: 1,
                    warning_count: 0,
                    summary: format!(
                        "Push IDs do not match workspace sync binding (expected workspace_id={bound_wrk}, world_id={bound_wld}, creator_id={bound_ctr})"
                    ),
                }),
                error: Some(
                    "Push workspace_id/world_id/creator_id do not match workspace_meta sync binding"
                        .to_string(),
                ),
            }));
        }
    }

    let command = SyncCommand {
        schema_version: 1,
        command_id: format!("cmd_{}", uuid::Uuid::new_v4().simple()),
        workspace_id: req.workspace_id.clone(),
        world_id: req.world_id.clone(),
        creator_id: req.creator_id.clone(),
        command_type: CommandType::SyncPush,
        origin: CommandOrigin::LocalUser,
        output_manuscript: None,
        status: CommandStatus::Pending,
        requested_by: None,
        started_at: None,
        completed_at: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    let idempotency_key = format!("idk_{}", uuid::Uuid::new_v4().simple());

    let bundle = BundleBuilder::new(&req.workspace_id, &req.world_id, &req.creator_id)
        .submitting_creator_id(&req.creator_id)
        .manuscript_phase(ManuscriptPhase::Draft)
        .output_manuscript(false)
        .command_id(&command.command_id)
        .idempotency_key(&idempotency_key)
        .add_delta(LocalDelta {
            delta_type: DeltaType::StoryManifest,
            operation: DeltaOperation::Update,
            target_entity_type: None,
            target_entity_id: None,
            payload: serde_json::Value::Object(serde_json::Map::default()),
            source_anchor: None,
            local_timestamp: command.created_at.clone(),
        })
        .build()
        .map_err(|e| NexusApiError::Internal {
            code: "BUNDLE_BUILD_ERROR".into(),
            message: format!("Failed to build sync bundle: {e}"),
        })?;

    let local_state = LocalState::new(0);
    let auth = AuthContext::single_creator();
    let precheck_result = precheck_bundle_with_auth(&bundle, &local_state, &auth);

    let precheck_summary = match &precheck_result {
        PrecheckResult::Valid => PrecheckSummary {
            valid: true,
            error_count: 0,
            warning_count: 0,
            summary: "All checks passed.".to_string(),
        },
        PrecheckResult::Invalid(report) => precheck_summary_from_report(report),
    };

    if let PrecheckResult::Invalid(report) = &precheck_result {
        if report.has_errors() && !req.force {
            return Ok(Json(SyncPushResponse {
                success: false,
                outbox_entry_id: None,
                bundle_id: None,
                precheck_result: Some(precheck_summary),
                error: Some("Precheck failed (pass force=true to stage anyway)".to_string()),
            }));
        }
    }

    let entry_id = outbox
        .stage(&bundle)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "OUTBOX_STAGE_ERROR".into(),
            message: format!("Failed to stage bundle in outbox: {e}"),
        })?;

    info!(
        outbox_entry_id = %entry_id,
        bundle_id = %bundle.bundle_id,
        "Bundle staged in outbox (ready)"
    );

    if let Some((base_url, token)) = try_eager_push_config_from_env() {
        match SyncClient::new(&base_url, &token) {
            Ok(client) => match client.push_bundle(&bundle).await {
                Ok(resp) => {
                    if resp.success {
                        match outbox.mark_sent(&entry_id).await {
                            Ok(()) => info!(
                                outbox_entry_id = %entry_id,
                                "Eager platform push succeeded; marked sent"
                            ),
                            Err(e) => warn!(
                                outbox_entry_id = %entry_id,
                                error = %e,
                                "Eager push OK but mark_sent failed"
                            ),
                        }
                    } else {
                        warn!(
                            outbox_entry_id = %entry_id,
                            ?resp,
                            "Eager platform push returned success=false; entry remains ready"
                        );
                    }
                }
                Err(e) => warn!(
                    outbox_entry_id = %entry_id,
                    error = %e,
                    "Eager platform push failed; entry remains ready"
                ),
            },
            Err(e) => warn!(
                error = %e,
                "NEXUS_SYNC_EAGER_PUSH set but SyncClient config invalid; skipping eager push"
            ),
        }
    }

    Ok(Json(SyncPushResponse {
        success: true,
        outbox_entry_id: Some(entry_id),
        bundle_id: Some(bundle.bundle_id),
        precheck_result: Some(precheck_summary),
        error: None,
    }))
}

/// POST /v1/local/sync/resolve
///
/// Apply a conflict resolution strategy to an outbox entry.
/// Supports `auto_accept`, `auto_reject`, and marks entries accordingly.
pub async fn resolve(
    State(state): State<WorkspaceState>,
    Json(req): Json<SyncResolveRequest>,
) -> Result<Json<SyncResolveResponse>, NexusApiError> {
    info!(
        outbox_entry_id = %req.outbox_entry_id,
        resolution = %req.resolution,
        force = req.force,
        "Handling sync resolve request"
    );

    let outbox = state.outbox().ok_or_else(|| NexusApiError::Internal {
        code: "SYNC_NOT_CONFIGURED".into(),
        message: "Sync outbox not initialized".to_string(),
    })?;

    // Get the current entry to determine state
    let entry = outbox
        .get(&req.outbox_entry_id)
        .await
        .map_err(|e| NexusApiError::NotFound(format!("Outbox entry not found: {e}")))?;

    match req.resolution.as_str() {
        "auto_accept" => {
            // Accept: mark as acked (server state wins)
            // First mark as sent if it was conflicted, then ack
            match entry.delivery_state.as_str() {
                "conflicted" | "sent" => {
                    // Mark as acked — this effectively "accepts" the server state
                    outbox.mark_acked(&req.outbox_entry_id).await.map_err(|e| {
                        NexusApiError::Internal {
                            code: "OUTBOX_RESOLVE_ERROR".into(),
                            message: format!("Failed to resolve entry as auto_accept: {e}"),
                        }
                    })?;

                    info!(outbox_entry_id = %req.outbox_entry_id, "Resolved as auto_accept (acked)");
                    Ok(Json(SyncResolveResponse {
                        success: true,
                        delivery_state: Some("acked".to_string()),
                        error: None,
                    }))
                }
                state_str => {
                    let msg = format!(
                        "Cannot auto_accept entry in '{state_str}' state (requires 'conflicted' or 'sent')"
                    );
                    Ok(Json(SyncResolveResponse {
                        success: false,
                        delivery_state: Some(entry.delivery_state.as_str().to_string()),
                        error: Some(msg),
                    }))
                }
            }
        }
        "auto_reject" => {
            // Reject: mark as failed with appropriate error
            // Safety: caller should have confirmed via CLI --force or interactive prompt
            match entry.delivery_state.as_str() {
                "conflicted" | "sent" | "staged" | "ready" => {
                    outbox
                        .mark_failed(
                            &req.outbox_entry_id,
                            "auto_reject: user chose to discard server changes",
                        )
                        .await
                        .map_err(|e| NexusApiError::Internal {
                            code: "OUTBOX_RESOLVE_ERROR".into(),
                            message: format!("Failed to resolve entry as auto_reject: {e}"),
                        })?;

                    info!(outbox_entry_id = %req.outbox_entry_id, "Resolved as auto_reject (failed/no retry)");
                    // After mark_failed, re-fetch to get the current state
                    let updated = outbox.get(&req.outbox_entry_id).await.ok();
                    Ok(Json(SyncResolveResponse {
                        success: true,
                        delivery_state: updated.map(|e| e.delivery_state.as_str().to_string()),
                        error: None,
                    }))
                }
                state_str => {
                    let msg = format!("Cannot auto_reject entry in '{state_str}' state");
                    Ok(Json(SyncResolveResponse {
                        success: false,
                        delivery_state: Some(entry.delivery_state.as_str().to_string()),
                        error: Some(msg),
                    }))
                }
            }
        }
        "manual_review" => {
            // Manual review: just return the current entry details for user inspection
            Ok(Json(SyncResolveResponse {
                success: false,
                delivery_state: Some(entry.delivery_state.as_str().to_string()),
                error: Some("Manual review: entry requires human decision".to_string()),
            }))
        }
        other => Ok(Json(SyncResolveResponse {
            success: false,
            delivery_state: Some(entry.delivery_state.as_str().to_string()),
            error: Some(format!("Unknown resolution strategy: {other}")),
        })),
    }
}

/// GET /v1/local/sync/replay
///
/// Returns entries eligible for replay (staged, ready, or failed with retry due).
pub async fn replay(
    State(state): State<WorkspaceState>,
) -> Result<Json<SyncReplayResponse>, NexusApiError> {
    info!("Handling sync replay request");

    let outbox = state.outbox().ok_or_else(|| NexusApiError::Internal {
        code: "SYNC_NOT_CONFIGURED".into(),
        message: "Sync outbox not initialized".to_string(),
    })?;

    let entries = outbox.replay().await.map_err(|e| NexusApiError::Internal {
        code: "OUTBOX_REPLAY_ERROR".into(),
        message: format!("Failed to replay outbox entries: {e}"),
    })?;

    let summaries: Vec<OutboxEntrySummary> = entries
        .iter()
        .map(|e| OutboxEntrySummary {
            outbox_entry_id: e.outbox_entry_id.clone(),
            bundle_id: e.bundle_id.clone(),
            delivery_state: e.delivery_state.as_str().to_string(),
            retry_count: e.retry_count.unwrap_or(0u64).cast_signed(),
            last_error: e.last_error.clone(),
            created_at: e.created_at.clone(),
        })
        .collect();

    let replayable_count = summaries.len();

    Ok(Json(SyncReplayResponse {
        replayable_count,
        entries: summaries,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_status_response_serialization() {
        let resp = SyncStatusResponse {
            staged_count: 2,
            ready_count: 1,
            sent_count: 0,
            acked_count: 5,
            conflicted_count: 3,
            failed_count: 1,
            last_sync_at: Some("2026-04-07T00:00:00Z".to_string()),
        };
        let json = serde_json::to_string(&resp).expect("SyncStatusResponse should serialize");
        assert!(json.contains("\"staged_count\":2"));
        assert!(json.contains("\"ready_count\":1"));
        assert!(json.contains("\"conflicted_count\":3"));
        assert!(json.contains("\"failed_count\":1"));
        assert!(json.contains("last_sync_at"));
    }

    #[test]
    fn test_sync_status_response_no_last_sync() {
        let resp = SyncStatusResponse {
            staged_count: 0,
            ready_count: 0,
            sent_count: 0,
            acked_count: 0,
            conflicted_count: 0,
            failed_count: 0,
            last_sync_at: None,
        };
        let json = serde_json::to_string(&resp).expect("SyncStatusResponse should serialize");
        assert!(json.contains("\"last_sync_at\":null"));
    }

    #[test]
    fn test_sync_push_request_deserialization() {
        let json = r#"{
            "workspace_id": "wrk_test",
            "world_id": "wld_test",
            "creator_id": "ctr_test",
            "force": true
        }"#;
        let req: SyncPushRequest = serde_json::from_str(json).expect("SyncPushRequest should deserialize from valid JSON");
        assert_eq!(req.workspace_id, "wrk_test");
        assert_eq!(req.world_id, "wld_test");
        assert_eq!(req.creator_id, "ctr_test");
        assert!(req.force);
    }

    #[test]
    fn test_sync_push_request_force_default_false() {
        let json = r#"{
            "workspace_id": "wrk_test",
            "world_id": "wld_test",
            "creator_id": "ctr_test"
        }"#;
        let req: SyncPushRequest = serde_json::from_str(json).expect("SyncPushRequest should deserialize from valid JSON");
        assert!(!req.force);
    }

    #[test]
    fn test_sync_pull_request_deserialization() {
        let json = r#"{
            "schema_version": 1,
            "world_id": "wld_x",
            "after_confirmed_delta_sequence": 4
        }"#;
        let req: SyncPullRequest = serde_json::from_str(json).expect("SyncPullRequest should deserialize from valid JSON");
        assert_eq!(req.schema_version, 1);
        assert_eq!(req.world_id, "wld_x");
        assert_eq!(req.after_confirmed_delta_sequence, Some(4));
    }

    #[test]
    fn test_sync_resolve_request_deserialization() {
        let json = r#"{
            "outbox_entry_id": "obe_abc123",
            "resolution": "auto_accept",
            "force": false
        }"#;
        let req: SyncResolveRequest = serde_json::from_str(json).expect("SyncResolveRequest should deserialize from valid JSON");
        assert_eq!(req.outbox_entry_id, "obe_abc123");
        assert_eq!(req.resolution, "auto_accept");
        assert!(!req.force);
    }

    #[tokio::test]
    async fn test_sync_status_with_real_outbox() {
        use crate::test_utils::create_test_workspace;
        use crate::workspace::WorkspaceState;

        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing_with_outbox(nexus_home, db_path, None).await;

        // Initially, all counts should be 0
        let result = status(State(state)).await;
        assert!(result.is_ok());
        let body = result.expect("status should return Ok for initialized workspace");
        assert_eq!(body.staged_count, 0);
        assert_eq!(body.ready_count, 0);
        assert_eq!(body.conflicted_count, 0);
        assert!(body.last_sync_at.is_none());
    }

    #[tokio::test]
    async fn test_sync_push_stages_command() {
        use crate::test_utils::create_test_workspace;
        use crate::workspace::WorkspaceState;

        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing_with_outbox(nexus_home, db_path, None).await;

        let req = SyncPushRequest {
            workspace_id: "wrk_test".to_string(),
            world_id: "wld_test".to_string(),
            creator_id: "ctr_test".to_string(),
            force: false,
        };

        let result = push(State(state), Json(req)).await;
        assert!(result.is_ok());
        let resp = result.expect("push should succeed for valid request");
        assert!(resp.success);
        assert!(resp.outbox_entry_id.is_some());
        assert!(resp.bundle_id.is_some());
    }

    #[tokio::test]
    async fn test_sync_status_after_push_shows_count() {
        use crate::test_utils::create_test_workspace;
        use crate::workspace::WorkspaceState;

        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing_with_outbox(nexus_home, db_path, None).await;

        // Push a command
        let req = SyncPushRequest {
            workspace_id: "wrk_test".to_string(),
            world_id: "wld_test".to_string(),
            creator_id: "ctr_test".to_string(),
            force: false,
        };
        let _ = push(State(state.clone()), Json(req)).await.expect("push should succeed in test");

        // Check status
        let result = status(State(state)).await;
        assert!(result.is_ok());
        let body = result.expect("status should return Ok for initialized workspace");
        // Push uses Outbox::stage → delivery_state `ready` (not `staged`)
        assert_eq!(body.ready_count, 1);
        assert_eq!(body.staged_count, 0);
    }

    #[test]
    fn test_precheck_summary_serialization() {
        let summary = PrecheckSummary {
            valid: true,
            error_count: 0,
            warning_count: 1,
            summary: "All checks passed.".to_string(),
        };
        let json = serde_json::to_string(&summary).expect("PrecheckSummary should serialize");
        assert!(json.contains("\"valid\":true"));
        assert!(json.contains("\"warning_count\":1"));
    }
}
