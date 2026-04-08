//! Sync handler — sync status, push, and resolve endpoints
//!
//! Uses `nexus-sync` **Outbox**, **BundleBuilder**, and **precheck** on the push path:
//! builds a minimal bundle (with `canonical_hash`), runs `precheck_bundle_with_auth`,
//! then persists via **`Outbox::stage`** (`ready`). This is **offline-first**: the
//! entry is queued locally; **`SyncClient::push_bundle`** to the platform is not
//! invoked here yet (optional follow-up when URL + token are available).

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::State;
use axum::Json;
use nexus_contracts::{
    CommandOrigin, CommandStatus, CommandType, DeltaOperation, DeltaType, ManuscriptPhase,
    SyncCommand,
};
use nexus_sync::delta_bundle::{BundleBuilder, LocalDelta};
use nexus_sync::precheck::{
    precheck_bundle_with_auth, AuthContext, LocalState, PrecheckReport, PrecheckResult,
    PrecheckSeverity,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

// ── Response types ───────────────────────────────────────────────

#[derive(Debug, Serialize, PartialEq)]
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
    /// Resolution strategy: auto_accept, auto_reject, manual_review
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

/// GET /v1/local/sync/status
///
/// Returns real outbox state counts using nexus-sync::Outbox.
pub async fn status(
    State(state): State<WorkspaceState>,
) -> Result<Json<SyncStatusResponse>, NexusApiError> {
    info!("Handling sync status request");

    match state.outbox() {
        Some(outbox) => {
            let staged_count = outbox.count_by_state("staged").await.unwrap_or(0) as u64;
            let ready_count = outbox.count_by_state("ready").await.unwrap_or(0) as u64;
            let sent_count = outbox.count_by_state("sent").await.unwrap_or(0) as u64;
            let acked_count = outbox.count_by_state("acked").await.unwrap_or(0) as u64;
            let conflicted_count = outbox.count_by_state("conflicted").await.unwrap_or(0) as u64;
            let failed_count = outbox.count_by_state("failed").await.unwrap_or(0) as u64;

            // Get last sync timestamp from workspace_meta
            let last_sync_at: Option<String> = state
                .db()
                .await
                .map_err(|e| NexusApiError::Internal {
                    code: "DATABASE_UNAVAILABLE".into(),
                    message: format!("Database connection error: {}", e),
                })?
                .query_row(
                    "SELECT value FROM workspace_meta WHERE key = 'last_sync_at'",
                    [],
                    |row| row.get(0),
                )
                .await
                .unwrap_or(None);

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
        }
        None => {
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

    let placeholder = LocalDelta {
        delta_type: DeltaType::StoryManifest,
        operation: DeltaOperation::Update,
        target_entity_type: None,
        target_entity_id: None,
        payload: serde_json::json!({ "nexus": "sync_push_heartbeat" }),
        source_anchor: None,
        local_timestamp: command.created_at.clone(),
    };

    let idempotency_key = format!("idk_{}", uuid::Uuid::new_v4().simple());

    let bundle = BundleBuilder::new(&req.workspace_id, &req.world_id, &req.creator_id)
        .submitting_creator_id(&req.creator_id)
        .manuscript_phase(ManuscriptPhase::Draft)
        .output_manuscript(false)
        .command_id(&command.command_id)
        .idempotency_key(&idempotency_key)
        .add_delta(placeholder)
        .build()
        .map_err(|e| NexusApiError::Internal {
            code: "BUNDLE_BUILD_ERROR".into(),
            message: format!("Failed to build sync bundle: {}", e),
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
            message: format!("Failed to stage bundle in outbox: {}", e),
        })?;

    info!(
        outbox_entry_id = %entry_id,
        bundle_id = %bundle.bundle_id,
        "Bundle staged in outbox (ready)"
    );

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
/// Supports auto_accept, auto_reject, and marks entries accordingly.
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
        .map_err(|e| NexusApiError::NotFound(format!("Outbox entry not found: {}", e)))?;

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
                            message: format!("Failed to resolve entry as auto_accept: {}", e),
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
                        "Cannot auto_accept entry in '{}' state (requires 'conflicted' or 'sent')",
                        state_str
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
                            message: format!("Failed to resolve entry as auto_reject: {}", e),
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
                    let msg = format!("Cannot auto_reject entry in '{}' state", state_str);
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
            error: Some(format!("Unknown resolution strategy: {}", other)),
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
        message: format!("Failed to replay outbox entries: {}", e),
    })?;

    let summaries: Vec<OutboxEntrySummary> = entries
        .iter()
        .map(|e| OutboxEntrySummary {
            outbox_entry_id: e.outbox_entry_id.clone(),
            bundle_id: e.bundle_id.clone(),
            delivery_state: e.delivery_state.as_str().to_string(),
            retry_count: e.retry_count.unwrap_or(0u64) as i64,
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
        let json = serde_json::to_string(&resp).unwrap();
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
        let json = serde_json::to_string(&resp).unwrap();
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
        let req: SyncPushRequest = serde_json::from_str(json).unwrap();
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
        let req: SyncPushRequest = serde_json::from_str(json).unwrap();
        assert!(!req.force);
    }

    #[test]
    fn test_sync_resolve_request_deserialization() {
        let json = r#"{
            "outbox_entry_id": "obe_abc123",
            "resolution": "auto_accept",
            "force": false
        }"#;
        let req: SyncResolveRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.outbox_entry_id, "obe_abc123");
        assert_eq!(req.resolution, "auto_accept");
        assert!(!req.force);
    }

    #[tokio::test]
    async fn test_sync_status_with_real_outbox() {
        use crate::test_utils::create_test_workspace;
        use crate::workspace::WorkspaceState;

        let (_tmp, nexus_home, db_path) = create_test_workspace();
        let state = WorkspaceState::new_for_testing_with_outbox(nexus_home, db_path, None).await;

        // Initially, all counts should be 0
        let result = status(State(state)).await;
        assert!(result.is_ok());
        let body = result.unwrap();
        assert_eq!(body.staged_count, 0);
        assert_eq!(body.ready_count, 0);
        assert_eq!(body.conflicted_count, 0);
        assert!(body.last_sync_at.is_none());
    }

    #[tokio::test]
    async fn test_sync_push_stages_command() {
        use crate::test_utils::create_test_workspace;
        use crate::workspace::WorkspaceState;

        let (_tmp, nexus_home, db_path) = create_test_workspace();
        let state = WorkspaceState::new_for_testing_with_outbox(nexus_home, db_path, None).await;

        let req = SyncPushRequest {
            workspace_id: "wrk_test".to_string(),
            world_id: "wld_test".to_string(),
            creator_id: "ctr_test".to_string(),
            force: false,
        };

        let result = push(State(state), Json(req)).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert!(resp.success);
        assert!(resp.outbox_entry_id.is_some());
        assert!(resp.bundle_id.is_some());
    }

    #[tokio::test]
    async fn test_sync_status_after_push_shows_count() {
        use crate::test_utils::create_test_workspace;
        use crate::workspace::WorkspaceState;

        let (_tmp, nexus_home, db_path) = create_test_workspace();
        let state = WorkspaceState::new_for_testing_with_outbox(nexus_home, db_path, None).await;

        // Push a command
        let req = SyncPushRequest {
            workspace_id: "wrk_test".to_string(),
            world_id: "wld_test".to_string(),
            creator_id: "ctr_test".to_string(),
            force: false,
        };
        let _ = push(State(state.clone()), Json(req)).await.unwrap();

        // Check status
        let result = status(State(state)).await;
        assert!(result.is_ok());
        let body = result.unwrap();
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
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("\"valid\":true"));
        assert!(json.contains("\"warning_count\":1"));
    }
}
