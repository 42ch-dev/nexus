//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc)]
//! World fork and snapshot — platform proxy via `SyncClient`.
//!
//! `POST /v1/local/world/fork` → `POST /v1/worlds/fork`
//! `POST /v1/local/world/snapshot` → `POST /v1/worlds/snapshot`
//!
//! Requires `NEXUS_SYNC_PLATFORM_URL` and `NEXUS_SYNC_PLATFORM_TOKEN`. When
//! `workspace_meta` sync binding is set, `parent_world_id` (fork) and `world_id`
//! (snapshot) must match the bound world.

use crate::api::errors::NexusApiError;
use crate::api::handlers::sync::{
    optional_sync_push_binding, try_platform_sync_credentials_from_env,
};
use crate::workspace::WorkspaceState;
use axum::extract::State;
use axum::Json;
use nexus_contracts::{ForkBranch, WorldForkRequest, WorldSnapshotRequest};
use nexus_sync::sync_client::SyncClient;
use serde::Serialize;
use tracing::info;

const fn nonempty(s: &str) -> bool {
    !s.is_empty()
}

fn map_sync_client_error(e: nexus_sync::SyncError) -> NexusApiError {
    NexusApiError::Internal {
        code: e.error_code().to_string(),
        message: e.to_string(),
    }
}

/// Daemon response after `POST /v1/local/world/fork`.
#[derive(Debug, Serialize)]
pub struct WorldForkLocalResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fork_branch: Option<ForkBranch>,
    pub error: Option<String>,
}

/// Daemon response after `POST /v1/local/world/snapshot`.
#[derive(Debug, Serialize)]
pub struct WorldSnapshotLocalResponse {
    pub success: bool,
    pub world_id: String,
    pub world_revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at_event_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub captured_at: Option<String>,
    pub error: Option<String>,
}

/// POST /v1/local/world/fork
pub async fn fork(
    State(state): State<WorkspaceState>,
    Json(mut req): Json<WorldForkRequest>,
) -> Result<Json<WorldForkLocalResponse>, NexusApiError> {
    let parent = req.parent_world_id.as_deref().unwrap_or("");
    let child = req.child_world_id.as_deref().unwrap_or("");
    info!(parent = %parent, child = %child, "Handling world fork request");

    let parent_nonempty = req
        .parent_world_id
        .as_ref()
        .is_some_and(|s| nonempty(s));
    let child_nonempty = req
        .child_world_id
        .as_ref()
        .is_some_and(|s| nonempty(s));
    let fork_evt_nonempty = req
        .forked_from_event_id
        .as_ref()
        .is_some_and(|s| nonempty(s));
    let creator_nonempty = req
        .created_by_creator_id
        .as_ref()
        .is_some_and(|s| nonempty(s));

    if !parent_nonempty || !child_nonempty || !fork_evt_nonempty || !creator_nonempty {
        return Ok(Json(WorldForkLocalResponse {
            success: false,
            fork_branch: None,
            error: Some(
                "parent_world_id, child_world_id, forked_from_event_id, and created_by_creator_id must be set and non-empty for local proxy"
                    .to_string(),
            ),
        }));
    }

    if req.schema_version == 0 {
        req.schema_version = 1;
    }

    if let (Some(ref p), Some(ref c)) = (&req.parent_world_id, &req.child_world_id) {
        if nonempty(p) && nonempty(c) && p == c {
            return Ok(Json(WorldForkLocalResponse {
                success: false,
                fork_branch: None,
                error: Some("child_world_id must differ from parent_world_id".to_string()),
            }));
        }
    }

    if let Some((_, bound_world, _)) = optional_sync_push_binding(&state).await? {
        if req.parent_world_id.as_deref() != Some(bound_world.as_str()) {
            return Ok(Json(WorldForkLocalResponse {
                success: false,
                fork_branch: None,
                error: Some(format!(
                    "parent_world_id does not match workspace sync binding (expected {bound_world})"
                )),
            }));
        }
    }

    let (base_url, token) =
        try_platform_sync_credentials_from_env().ok_or_else(|| NexusApiError::InvalidInput {
            field: "platform_sync".into(),
            reason: "Set NEXUS_SYNC_PLATFORM_URL and NEXUS_SYNC_PLATFORM_TOKEN".into(),
        })?;

    let client = SyncClient::new(&base_url, &token).map_err(map_sync_client_error)?;
    let remote = client
        .fork_world(&req)
        .await
        .map_err(map_sync_client_error)?;

    Ok(Json(WorldForkLocalResponse {
        success: true,
        fork_branch: Some(remote.fork_branch),
        error: None,
    }))
}

/// POST /v1/local/world/snapshot
pub async fn snapshot(
    State(state): State<WorkspaceState>,
    Json(mut req): Json<WorldSnapshotRequest>,
) -> Result<Json<WorldSnapshotLocalResponse>, NexusApiError> {
    info!(world_id = %req.world_id, "Handling world snapshot request");

    if req.world_id.is_empty() {
        return Ok(Json(WorldSnapshotLocalResponse {
            success: false,
            world_id: String::new(),
            world_revision: 0,
            at_event_id: None,
            captured_at: None,
            error: Some("world_id must not be empty".to_string()),
        }));
    }

    if req.schema_version == 0 {
        req.schema_version = 1;
    }

    if let Some((_, bound_world, _)) = optional_sync_push_binding(&state).await? {
        if req.world_id != bound_world {
            return Ok(Json(WorldSnapshotLocalResponse {
                success: false,
                world_id: req.world_id.clone(),
                world_revision: 0,
                at_event_id: None,
                captured_at: None,
                error: Some(format!(
                    "world_id does not match workspace sync binding (expected {bound_world})"
                )),
            }));
        }
    }

    if let Some(ref eid) = req.at_event_id {
        if eid.is_empty() {
            return Ok(Json(WorldSnapshotLocalResponse {
                success: false,
                world_id: req.world_id.clone(),
                world_revision: 0,
                at_event_id: None,
                captured_at: None,
                error: Some("at_event_id, when set, must not be empty".to_string()),
            }));
        }
    }

    if let Some(ref bid) = req.branch_id {
        if bid.is_empty() {
            return Ok(Json(WorldSnapshotLocalResponse {
                success: false,
                world_id: req.world_id.clone(),
                world_revision: 0,
                at_event_id: None,
                captured_at: None,
                error: Some("branch_id, when set, must not be empty".to_string()),
            }));
        }
    }

    let (base_url, token) =
        try_platform_sync_credentials_from_env().ok_or_else(|| NexusApiError::InvalidInput {
            field: "platform_sync".into(),
            reason: "Set NEXUS_SYNC_PLATFORM_URL and NEXUS_SYNC_PLATFORM_TOKEN".into(),
        })?;

    let client = SyncClient::new(&base_url, &token).map_err(map_sync_client_error)?;
    let remote = client
        .snapshot_world(&req)
        .await
        .map_err(map_sync_client_error)?;

    Ok(Json(WorldSnapshotLocalResponse {
        success: true,
        world_id: remote.world_id,
        world_revision: remote.world_revision,
        at_event_id: remote.at_event_id,
        captured_at: Some(remote.captured_at),
        error: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_fork_local_response_failure_json() {
        let r = WorldForkLocalResponse {
            success: false,
            fork_branch: None,
            error: Some("bad".into()),
        };
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains("\"success\":false"));
    }

    #[test]
    fn world_snapshot_local_response_success_json() {
        let r = WorldSnapshotLocalResponse {
            success: true,
            world_id: "wld_x".into(),
            world_revision: 7,
            at_event_id: Some("evt_y".into()),
            captured_at: Some("2026-04-10T00:00:00Z".into()),
            error: None,
        };
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains("\"world_revision\":7"));
    }
}
