//! Publish workflow — platform proxy via `SyncClient`.
//!
//! `POST /v1/local/publish/story` → `POST /v1/publish/story`
//! `POST /v1/local/publish/history` → `POST /v1/publish/history`
//!
//! Requires `NEXUS_SYNC_PLATFORM_URL` and `NEXUS_SYNC_PLATFORM_TOKEN`. When
//! `workspace_meta` sync binding is set, `world_id` must match the bound world.

use crate::api::errors::NexusApiError;
use crate::api::handlers::sync::{
    optional_sync_push_binding, try_platform_sync_credentials_from_env,
};
use crate::workspace::WorkspaceState;
use axum::extract::State;
use axum::Json;
use nexus_contracts::{
    PublishHistoryRequest, PublishHistoryResponse, PublishStoryRequest, PublishStoryResponse,
};
use nexus_sync::sync_client::SyncClient;
use serde::Serialize;
use tracing::info;

fn map_sync_client_error(e: nexus_sync::SyncError) -> NexusApiError {
    NexusApiError::Internal {
        code: e.error_code().to_string(),
        message: e.to_string(),
    }
}

fn manuscript_id_str(v: &serde_json::Value) -> &str {
    v.as_str().unwrap_or("")
}

/// Daemon response after `POST /v1/local/publish/story`.
#[derive(Debug, Serialize)]
pub struct PublishStoryLocalResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<PublishStoryResponse>,
    pub error: Option<String>,
}

/// Daemon response after `POST /v1/local/publish/history`.
#[derive(Debug, Serialize)]
pub struct PublishHistoryLocalResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<PublishHistoryResponse>,
    pub error: Option<String>,
}

/// POST /v1/local/publish/story
pub async fn story(
    State(state): State<WorkspaceState>,
    Json(mut req): Json<PublishStoryRequest>,
) -> Result<Json<PublishStoryLocalResponse>, NexusApiError> {
    let manuscript_s = manuscript_id_str(&req.manuscript_id);
    info!(
        world_id = %req.world_id,
        manuscript_id = %manuscript_s,
        "Handling publish story request"
    );

    if req.world_id.is_empty() || manuscript_s.is_empty() {
        return Ok(Json(PublishStoryLocalResponse {
            success: false,
            result: None,
            error: Some("world_id and manuscript_id must be non-empty strings".into()),
        }));
    }

    if req.schema_version == 0 {
        req.schema_version = 1;
    }

    if let Some((_, bound_world, _)) = optional_sync_push_binding(&state).await? {
        if req.world_id != bound_world {
            return Ok(Json(PublishStoryLocalResponse {
                success: false,
                result: None,
                error: Some(format!(
                    "world_id does not match workspace sync binding (expected {bound_world})"
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
        .publish_story(&req)
        .await
        .map_err(map_sync_client_error)?;

    Ok(Json(PublishStoryLocalResponse {
        success: true,
        result: Some(remote),
        error: None,
    }))
}

/// POST /v1/local/publish/history
pub async fn history(
    State(state): State<WorkspaceState>,
    Json(mut req): Json<PublishHistoryRequest>,
) -> Result<Json<PublishHistoryLocalResponse>, NexusApiError> {
    let manuscript_s = manuscript_id_str(&req.manuscript_id);
    info!(
        world_id = %req.world_id,
        manuscript_id = %manuscript_s,
        "Handling publish history request"
    );

    if req.world_id.is_empty() || manuscript_s.is_empty() {
        return Ok(Json(PublishHistoryLocalResponse {
            success: false,
            history: None,
            error: Some("world_id and manuscript_id must be non-empty strings".into()),
        }));
    }

    if req.schema_version == 0 {
        req.schema_version = 1;
    }

    if let Some((_, bound_world, _)) = optional_sync_push_binding(&state).await? {
        if req.world_id != bound_world {
            return Ok(Json(PublishHistoryLocalResponse {
                success: false,
                history: None,
                error: Some(format!(
                    "world_id does not match workspace sync binding (expected {bound_world})"
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
        .publish_history(&req)
        .await
        .map_err(map_sync_client_error)?;

    Ok(Json(PublishHistoryLocalResponse {
        success: true,
        history: Some(remote),
        error: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_story_local_response_failure_json() {
        let r = PublishStoryLocalResponse {
            success: false,
            result: None,
            error: Some("x".into()),
        };
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains("\"success\":false"));
    }
}
