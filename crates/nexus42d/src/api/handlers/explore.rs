//! Explore read-only proxy — `SyncClient` + platform `NEXUS_SYNC_PLATFORM_*` credentials.
//!
//! `POST /v1/local/explore/browse` → `POST /v1/explore/browse`
//! `POST /v1/local/explore/search` → `POST /v1/explore/search`

use crate::api::errors::NexusApiError;
use crate::api::handlers::sync::try_platform_sync_credentials_from_env;
use axum::Json;
use nexus_contracts::{ExploreBrowseRequest, ExploreFeedResponse, ExploreSearchRequest};
use nexus_sync::sync_client::SyncClient;
use serde::Serialize;
use tracing::info;

fn map_sync_client_error(e: nexus_sync::SyncError) -> NexusApiError {
    NexusApiError::Internal {
        code: e.error_code().to_string(),
        message: e.to_string(),
    }
}

/// JSON envelope for daemon Explore endpoints (mirrors sync/world proxy style).
#[derive(Debug, Serialize)]
pub struct ExploreLocalResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feed: Option<ExploreFeedResponse>,
    pub error: Option<String>,
}

const BROWSE_SCOPES: &[&str] = &["all", "worlds", "creators", "manuscripts"];

/// POST /v1/local/explore/browse
pub async fn browse(
    Json(mut req): Json<ExploreBrowseRequest>,
) -> Result<Json<ExploreLocalResponse>, NexusApiError> {
    info!("Handling explore browse request");
    if req.schema_version == 0 {
        req.schema_version = 1;
    }

    if let Some(ref s) = req.scope {
        if !BROWSE_SCOPES.contains(&s.as_str()) {
            return Ok(Json(ExploreLocalResponse {
                success: false,
                feed: None,
                error: Some(format!(
                    "invalid scope '{s}'; expected one of: {}",
                    BROWSE_SCOPES.join(", ")
                )),
            }));
        }
    }

    let (base_url, token) =
        try_platform_sync_credentials_from_env().ok_or_else(|| NexusApiError::InvalidInput {
            field: "platform_sync".into(),
            reason: "Set NEXUS_SYNC_PLATFORM_URL and NEXUS_SYNC_PLATFORM_TOKEN for Explore proxy"
                .into(),
        })?;

    let client = SyncClient::new(&base_url, &token).map_err(map_sync_client_error)?;
    let feed = client
        .explore_browse(&req)
        .await
        .map_err(map_sync_client_error)?;

    Ok(Json(ExploreLocalResponse {
        success: true,
        feed: Some(feed),
        error: None,
    }))
}

/// POST /v1/local/explore/search
pub async fn search(
    Json(mut req): Json<ExploreSearchRequest>,
) -> Result<Json<ExploreLocalResponse>, NexusApiError> {
    info!(
        query_len = req.query.len(),
        "Handling explore search request"
    );
    if req.schema_version == 0 {
        req.schema_version = 1;
    }

    if req.query.trim().is_empty() {
        return Ok(Json(ExploreLocalResponse {
            success: false,
            feed: None,
            error: Some("query must not be empty".to_string()),
        }));
    }

    let (base_url, token) =
        try_platform_sync_credentials_from_env().ok_or_else(|| NexusApiError::InvalidInput {
            field: "platform_sync".into(),
            reason: "Set NEXUS_SYNC_PLATFORM_URL and NEXUS_SYNC_PLATFORM_TOKEN for Explore proxy"
                .into(),
        })?;

    let client = SyncClient::new(&base_url, &token).map_err(map_sync_client_error)?;
    let feed = client
        .explore_search(&req)
        .await
        .map_err(map_sync_client_error)?;

    Ok(Json(ExploreLocalResponse {
        success: true,
        feed: Some(feed),
        error: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explore_local_response_failure_json() {
        let r = ExploreLocalResponse {
            success: false,
            feed: None,
            error: Some("x".into()),
        };
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains("\"success\":false"));
    }
}
