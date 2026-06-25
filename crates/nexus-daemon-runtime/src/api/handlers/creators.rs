//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc)]
//! Creator handlers — local creator listing and management.
//!
//! Registration proxy routes were removed in V1.21 (Batch D);
//! registration now lives in the CLI via `nexus-cloud-sync`.

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, Query, State};
use axum::Json;
use nexus_contracts::PaginationInfo;
use nexus_home_layout::validate_creator_id_safe;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CreatorInfo {
    pub creator_id: String,
    pub display_name: String,
    pub status: String,
    pub cached_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListCreatorsQuery {
    /// Maximum number of items to return (1–250, default 50).
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Opaque cursor for pagination; pass `next_cursor` from the previous page.
    pub cursor: Option<String>,
}

const fn default_limit() -> usize {
    50
}

/// Maximum items per page.
const MAX_LIMIT: usize = 250;

#[derive(Serialize)]
pub struct ListCreatorsResponse {
    pub items: Vec<CreatorInfo>,
    pub pagination: PaginationInfo,
}

// ── Local creator detail types ──────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CreatorDetail {
    pub creator_id: String,
    pub handle: Option<String>,
    pub display_name: Option<String>,
    pub has_api_key: bool,
    pub has_cached_token: bool,
    pub is_active: bool,
}

#[derive(Debug, Deserialize)]
pub struct SetActiveCreatorRequest {
    pub creator_id: String,
}

#[derive(Debug, Serialize)]
pub struct ActiveCreatorResponse {
    pub creator_id: String,
    pub handle: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SetActiveCreatorResponse {
    pub creator_id: String,
}

#[derive(Debug, Serialize)]
pub struct LogoutResponse {
    pub creator_id: String,
    pub cleared: bool,
}

#[derive(Clone)]
struct IdentityEntry {
    handle: Option<String>,
    display_name: Option<String>,
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Read the CLI config from `nexus_home`.
fn read_cli_config(nexus_home: &std::path::Path) -> Result<toml::Value, NexusApiError> {
    let config_path = nexus_home.join("config.toml");
    if !config_path.exists() {
        return Ok(toml::Value::Table(toml::map::Map::new()));
    }
    let content = std::fs::read_to_string(&config_path).map_err(|e| NexusApiError::Internal {
        code: "CONFIG_READ_ERROR".into(),
        message: e.to_string(),
    })?;
    if content.trim().is_empty() {
        return Ok(toml::Value::Table(toml::map::Map::new()));
    }
    toml::from_str(&content).map_err(|e| NexusApiError::Internal {
        code: "CONFIG_PARSE_ERROR".into(),
        message: e.to_string(),
    })
}

/// Write CLI config to `nexus_home`.
fn write_cli_config(
    nexus_home: &std::path::Path,
    config: &toml::Value,
) -> Result<(), NexusApiError> {
    let config_path = nexus_home.join("config.toml");
    let toml_str = toml::to_string_pretty(config).map_err(|e| NexusApiError::Internal {
        code: "CONFIG_SERIALIZE_ERROR".into(),
        message: e.to_string(),
    })?;
    std::fs::write(&config_path, toml_str).map_err(|e| NexusApiError::Internal {
        code: "CONFIG_WRITE_ERROR".into(),
        message: e.to_string(),
    })
}

/// Read active `creator_id` from CLI config.
fn read_active_creator_id(nexus_home: &std::path::Path) -> Option<String> {
    let config = read_cli_config(nexus_home).ok()?;
    config
        .get("active_creator_id")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
}

/// Set active `creator_id` in CLI config.
fn set_active_creator_id(
    nexus_home: &std::path::Path,
    creator_id: &str,
) -> Result<(), NexusApiError> {
    validate_creator_id_safe(creator_id).map_err(|reason| NexusApiError::InvalidInput {
        field: "creator_id".to_string(),
        reason,
    })?;

    let mut config = read_cli_config(nexus_home)?;
    let table = config
        .as_table_mut()
        .ok_or_else(|| NexusApiError::Internal {
            code: "CONFIG_ERROR".into(),
            message: "config root is not a table".to_string(),
        })?;

    table.insert(
        "active_creator_id".to_string(),
        toml::Value::String(creator_id.to_string()),
    );

    write_cli_config(nexus_home, &config)
}

/// Load the creator identity cache.
fn load_identity_cache() -> serde_json::Value {
    let Some(home) = dirs::home_dir() else {
        return serde_json::Value::Null;
    };
    let cache_path = home.join(".nexus42").join("creator_identity_cache.json");
    if !cache_path.exists() {
        return serde_json::Value::Null;
    }
    let Ok(content) = std::fs::read_to_string(&cache_path) else {
        return serde_json::Value::Null;
    };
    serde_json::from_str(&content).unwrap_or(serde_json::Value::Null)
}

/// Get identity cache entry for a creator.
fn get_identity_entry(cache: &serde_json::Value, creator_id: &str) -> Option<IdentityEntry> {
    let creators = cache.get("creators")?.as_object()?;
    let entry = creators.get(creator_id)?;
    Some(IdentityEntry {
        handle: entry
            .get("handle")
            .and_then(|v| v.as_str())
            .map(String::from),
        display_name: entry
            .get("display_name")
            .and_then(|v| v.as_str())
            .map(String::from),
    })
}

/// Load the auth store to check credentials.
fn load_auth_store() -> serde_json::Value {
    let Some(home) = dirs::home_dir() else {
        return serde_json::Value::Null;
    };
    let auth_path = home.join(".nexus42").join("auth.json");
    if !auth_path.exists() {
        return serde_json::Value::Null;
    }
    let Ok(content) = std::fs::read_to_string(&auth_path) else {
        return serde_json::Value::Null;
    };
    serde_json::from_str(&content).unwrap_or(serde_json::Value::Null)
}

/// Check if a creator has an API key stored.
fn has_creator_api_key(auth_store: &serde_json::Value, creator_id: &str) -> bool {
    auth_store
        .get("creators")
        .and_then(|c| c.get(creator_id))
        .and_then(|e| e.get("creator_api_key"))
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty())
}

/// Check if a creator has a cached access token.
fn has_cached_token(auth_store: &serde_json::Value, creator_id: &str) -> bool {
    auth_store
        .get("creators")
        .and_then(|c| c.get(creator_id))
        .and_then(|e| e.get("access_token"))
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty())
}

/// Remove a creator's credentials from the auth store.
fn clear_creator_credentials(creator_id: &str) -> Result<bool, NexusApiError> {
    let home = dirs::home_dir().ok_or_else(|| NexusApiError::Internal {
        code: "HOME_DIR_ERROR".into(),
        message: "Cannot determine home directory".to_string(),
    })?;
    let auth_path = home.join(".nexus42").join("auth.json");
    if !auth_path.exists() {
        return Ok(false);
    }

    let content = std::fs::read_to_string(&auth_path).map_err(|e| NexusApiError::Internal {
        code: "AUTH_READ_ERROR".into(),
        message: e.to_string(),
    })?;
    let mut store: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| NexusApiError::Internal {
            code: "AUTH_PARSE_ERROR".into(),
            message: e.to_string(),
        })?;

    let removed = store
        .get_mut("creators")
        .and_then(|c| c.as_object_mut())
        .is_some_and(|creators| creators.remove(creator_id).is_some());

    if removed {
        let json = serde_json::to_string_pretty(&store).map_err(|e| NexusApiError::Internal {
            code: "AUTH_SERIALIZE_ERROR".into(),
            message: e.to_string(),
        })?;
        std::fs::write(&auth_path, json).map_err(|e| NexusApiError::Internal {
            code: "AUTH_WRITE_ERROR".into(),
            message: e.to_string(),
        })?;
    }

    Ok(removed)
}

// ── Handlers ────────────────────────────────────────────────────────

/// GET /v1/local/creators
pub async fn list(
    State(state): State<WorkspaceState>,
    Query(params): Query<ListCreatorsQuery>,
) -> Result<Json<ListCreatorsResponse>, NexusApiError> {
    info!("Handling list creators request");

    let limit = params.limit.clamp(1, MAX_LIMIT);
    let all_creators = sqlx::query_as!(
        CreatorInfo,
        r#"SELECT creator_id as "creator_id!", display_name, status, cached_at FROM creators ORDER BY cached_at DESC"#
    )
    .fetch_all(state.pool())
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".into(),
        message: e.to_string(),
    })?;

    let mut items = all_creators;

    // Apply cursor-based pagination (cursor = creator_id)
    if let Some(ref cursor) = params.cursor {
        let pos = items.iter().position(|i| i.creator_id == *cursor);
        if let Some(idx) = pos {
            items = items.split_off(idx + 1);
        }
    }

    let next_cursor = if items.len() > limit {
        items.truncate(limit);
        items.last().map(|i| i.creator_id.clone())
    } else {
        None
    };

    debug!(count = items.len(), "Creators retrieved");
    info!("List creators completed");
    Ok(Json(ListCreatorsResponse {
        items,
        pagination: PaginationInfo {
            limit: i64::try_from(limit).unwrap_or(i64::MAX),
            has_more: next_cursor.is_some(),
            next_cursor,
        },
    }))
}

/// `GET /v1/local/creators/{creator_id}` — creator status/detail
pub async fn get_creator(
    State(state): State<WorkspaceState>,
    Path(creator_id): Path<String>,
) -> Result<Json<CreatorDetail>, NexusApiError> {
    info!(creator_id = %creator_id, "Getting creator detail");

    validate_creator_id_safe(&creator_id).map_err(|reason| NexusApiError::InvalidInput {
        field: "creator_id".to_string(),
        reason,
    })?;

    let cache = load_identity_cache();
    let entry = get_identity_entry(&cache, &creator_id);
    let auth_store = load_auth_store();
    let active_id = read_active_creator_id(state.nexus_home());

    Ok(Json(CreatorDetail {
        creator_id: creator_id.clone(),
        handle: entry.as_ref().and_then(|e| e.handle.clone()),
        display_name: entry.as_ref().and_then(|e| e.display_name.clone()),
        has_api_key: has_creator_api_key(&auth_store, &creator_id),
        has_cached_token: has_cached_token(&auth_store, &creator_id),
        is_active: active_id.as_deref() == Some(creator_id.as_str()),
    }))
}

/// `PUT /v1/local/creators/active` — set active creator
pub async fn set_active_creator(
    State(state): State<WorkspaceState>,
    Json(req): Json<SetActiveCreatorRequest>,
) -> Result<Json<SetActiveCreatorResponse>, NexusApiError> {
    info!(creator_id = %req.creator_id, "Setting active creator");

    validate_creator_id_safe(&req.creator_id).map_err(|reason| NexusApiError::InvalidInput {
        field: "creator_id".to_string(),
        reason,
    })?;

    // Verify the creator has credentials stored
    let auth_store = load_auth_store();
    let cache = load_identity_cache();

    let in_auth = auth_store
        .get("creators")
        .and_then(|c| c.as_object())
        .is_some_and(|obj| obj.contains_key(&req.creator_id));
    let in_cache = get_identity_entry(&cache, &req.creator_id).is_some();

    if !in_auth && !in_cache {
        return Err(NexusApiError::NotFound(format!(
            "Creator {} not found. Register first.",
            req.creator_id
        )));
    }

    set_active_creator_id(state.nexus_home(), &req.creator_id)?;

    // Clear workspace slug for this creator in config (reset to default)
    let mut config = read_cli_config(state.nexus_home())?;
    if let Some(table) = config.as_table_mut() {
        if let Some(slug_table) = table.get_mut("active_workspace_slug_by_creator") {
            if let Some(slugs) = slug_table.as_table_mut() {
                slugs.remove(&req.creator_id);
            }
        }
        write_cli_config(state.nexus_home(), &config)?;
    }

    Ok(Json(SetActiveCreatorResponse {
        creator_id: req.creator_id,
    }))
}

/// `GET /v1/local/creators/active` — get active creator
pub async fn get_active_creator(
    State(state): State<WorkspaceState>,
) -> Result<Json<ActiveCreatorResponse>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::Uninitialized)?;

    let cache = load_identity_cache();
    let entry = get_identity_entry(&cache, &creator_id);

    Ok(Json(ActiveCreatorResponse {
        creator_id,
        handle: entry.as_ref().and_then(|e| e.handle.clone()),
        display_name: entry.and_then(|e| e.display_name),
    }))
}

/// `POST /v1/local/creators/{id}:logout` — clear credentials
pub async fn logout_creator(
    State(state): State<WorkspaceState>,
    Path(creator_id): Path<String>,
) -> Result<Json<LogoutResponse>, NexusApiError> {
    info!(creator_id = %creator_id, "Logging out creator");

    validate_creator_id_safe(&creator_id).map_err(|reason| NexusApiError::InvalidInput {
        field: "creator_id".to_string(),
        reason,
    })?;

    let cleared = clear_creator_credentials(&creator_id)?;

    // If this was the active creator, clear the active selection
    if let Some(active) = read_active_creator_id(state.nexus_home()) {
        if active == creator_id {
            let mut config = read_cli_config(state.nexus_home())?;
            if let Some(table) = config.as_table_mut() {
                table.remove("active_creator_id");
                write_cli_config(state.nexus_home(), &config)?;
            }
        }
    }

    Ok(Json(LogoutResponse {
        creator_id,
        cleared,
    }))
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_creator_id_rejects_traversal() {
        assert!(validate_creator_id_safe("../etc").is_err());
    }

    #[test]
    fn validate_creator_id_accepts_valid() {
        assert!(validate_creator_id_safe("crt_abc123").is_ok());
    }

    #[tokio::test]
    async fn get_active_without_creator_returns_uninitialized() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let nexus_home = tmp.path().join(".nexus42");
        std::fs::create_dir_all(&nexus_home).expect("create");

        let db_path = nexus_home.join("state.db");
        let pool = nexus_local_db::open_pool(&db_path).await.expect("pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("migrate");
        nexus_local_db::seed_versions(&pool).await.expect("seed");

        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let result = get_active_creator(State(state)).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            NexusApiError::Uninitialized => {}
            other => panic!("Expected Uninitialized, got: {other}"),
        }
    }
}
