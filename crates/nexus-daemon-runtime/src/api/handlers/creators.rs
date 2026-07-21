//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc)]
//! Creator handlers — local creator listing and management.
//!
//! Registration proxy routes were removed in V1.21 (Batch D);
//! registration now lives in the CLI via `nexus-cloud-sync`.

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
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

#[derive(Debug, Deserialize)]
pub struct PatchCreatorRequest {
    /// New display name for the creator.
    pub display_name: Option<String>,
}

/// `POST /v1/daemon/creators` request body (V1.129 P0).
///
/// `display_name` is the only author-supplied field; the daemon generates the
/// `creator_id`, seeds the SQL row, and returns a `CreatorDetail` per the
/// architect lock (spec § Interfaces — `POST /v1/daemon/creators`).
#[derive(Debug, Deserialize)]
pub struct CreateCreatorRequest {
    pub display_name: String,
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

/// Crash-safe file write: write to a sibling temp file on the same filesystem,
/// then rename it into place. `rename` is atomic on POSIX and avoids leaving a
/// truncated file if the process is killed mid-write.
fn atomic_write(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

/// Load the identity cache from disk, reporting parse/read errors instead of
/// treating them as a missing cache.
fn load_identity_cache_strict(
    cache_path: &std::path::Path,
) -> Result<serde_json::Value, NexusApiError> {
    let content = std::fs::read_to_string(cache_path).map_err(|e| NexusApiError::Internal {
        code: "CACHE_READ_ERROR".into(),
        message: e.to_string(),
    })?;
    serde_json::from_str(&content).map_err(|e| NexusApiError::Internal {
        code: "CACHE_PARSE_ERROR".into(),
        message: e.to_string(),
    })
}

/// Write the identity cache to disk with an atomic temp-file + rename.
fn save_identity_cache(
    cache_path: &std::path::Path,
    cache: &serde_json::Value,
) -> Result<(), NexusApiError> {
    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| NexusApiError::Internal {
            code: "CACHE_DIR_ERROR".into(),
            message: e.to_string(),
        })?;
    }
    let json = serde_json::to_string_pretty(cache).map_err(|e| NexusApiError::Internal {
        code: "CACHE_SERIALIZE_ERROR".into(),
        message: e.to_string(),
    })?;
    atomic_write(cache_path, &json).map_err(|e| NexusApiError::Internal {
        code: "CACHE_WRITE_ERROR".into(),
        message: e.to_string(),
    })
}

/// Update the SQL `creators` row for `creator_id`, inserting a minimal active
/// row if one does not exist.
async fn upsert_creator_display_name(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    display_name: &str,
) -> Result<(), NexusApiError> {
    let now = chrono::Utc::now().to_rfc3339();
    let rows = sqlx::query!(
        "UPDATE creators SET display_name = ?, cached_at = ? WHERE creator_id = ?",
        display_name,
        now,
        creator_id
    )
    .execute(pool)
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".into(),
        message: e.to_string(),
    })?;
    if rows.rows_affected() == 0 {
        sqlx::query!(
            "INSERT INTO creators (creator_id, display_name, status, cached_at, data) VALUES (?, ?, 'active', ?, '{}')",
            creator_id,
            display_name,
            now
        )
        .execute(pool)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: e.to_string(),
        })?;
    }
    Ok(())
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

/// GET /v1/daemon/creators
pub async fn list(
    State(state): State<WorkspaceState>,
    Query(params): Query<ListCreatorsQuery>,
) -> Result<Json<ListCreatorsResponse>, NexusApiError> {
    info!("Handling list creators request");

    let limit = params.limit.clamp(1, MAX_LIMIT);
    let all_creators = if let Some(pool) = state.pool() {
        sqlx::query_as!(
            CreatorInfo,
            r#"SELECT creator_id as "creator_id!", display_name, status, cached_at FROM creators ORDER BY cached_at DESC"#
        )
        .fetch_all(pool)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: e.to_string(),
        })?
    } else {
        Vec::new()
    };

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

/// `POST /v1/daemon/creators` — create a new local creator profile (V1.129 P0).
///
/// Generates a `ctr_local…` id (matching the `CreatorId` pattern), validates the
/// `display_name`, lazily attaches the creator pool when `active_creator_id` is
/// present in config but the pool is not yet open (mirrors `patch_creator`'s
/// V1.119 pool-attach pattern at `creators.rs:533-541`), INSERTs the row, and
/// returns a `CreatorDetail`-shaped 201 response.
///
/// Tier-1 (API key) only — do not gate on active creator (architect lock #2).
/// The handler reuses the canonical `NexusApiError` envelope for all failures.
pub async fn create_creator(
    State(state): State<WorkspaceState>,
    Json(req): Json<CreateCreatorRequest>,
) -> Result<(StatusCode, Json<CreatorDetail>), NexusApiError> {
    info!("Handling create creator request");

    // Validate display_name: non-empty (after trim) and ≤ 256 chars (by char count,
    // mirroring `patch_creator`'s rule so CJK / emoji are counted once).
    let display_name = req.display_name.trim().to_string();
    if display_name.is_empty() {
        return Err(NexusApiError::InvalidInput {
            field: "display_name".to_string(),
            reason: "display_name cannot be empty".to_string(),
        });
    }
    if display_name.chars().count() > 256 {
        return Err(NexusApiError::InvalidInput {
            field: "display_name".to_string(),
            reason: "display_name must be 256 characters or fewer".to_string(),
        });
    }

    // Generate a creator id matching the `^ctr_[a-zA-Z0-9]+$` pattern used by
    // `nexus-creator::local_identity::generate_local_id`. Inline copy here so
    // the daemon-runtime crate does not depend on a private helper that may
    // change shape; the pattern itself is the public contract.
    let random: String = uuid::Uuid::new_v4()
        .to_string()
        .replace('-', "")
        .chars()
        .take(12)
        .collect();
    let creator_id = format!("ctr_local{random}");
    // Defensive: the generated id always matches the safe-id check, but run it
    // anyway so future id-shape changes cannot bypass the path-traversal guard.
    validate_creator_id_safe(&creator_id).map_err(|reason| NexusApiError::Internal {
        code: "CREATOR_ID_GENERATION_ERROR".into(),
        message: reason,
    })?;

    // Lazily attach the creator pool when `active_creator_id` is present in
    // config but the pool is not yet open (mirrors `patch_creator` V1.119 fix).
    // When no creator is active in config at all, the pool may be absent — we
    // still attempt the insert and surface a descriptive `Internal` if the
    // schema is missing (architect lock #3).
    if state.pool().is_none() && read_active_creator_id(state.nexus_home()).is_some() {
        state
            .ensure_creator_pool()
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".into(),
                message: e.to_string(),
            })?;
    }

    let pool = state.pool_or_uninit()?;
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query!(
        "INSERT INTO creators (creator_id, display_name, status, cached_at, data) VALUES (?, ?, 'active', ?, '{}')",
        creator_id,
        display_name,
        now
    )
    .execute(pool)
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".into(),
        message: e.to_string(),
    })?;

    debug!(creator_id = %creator_id, display_name = %display_name, "Creator created");
    info!(creator_id = %creator_id, "Create creator completed");

    Ok((
        StatusCode::CREATED,
        Json(CreatorDetail {
            creator_id,
            handle: None,
            display_name: Some(display_name),
            has_api_key: false,
            has_cached_token: false,
            is_active: false,
        }),
    ))
}

/// `GET /v1/daemon/creators/{creator_id}` — creator status/detail
pub async fn get_creator(
    State(state): State<WorkspaceState>,
    Path(creator_id): Path<String>,
) -> Result<Json<CreatorDetail>, NexusApiError> {
    info!(creator_id = %creator_id, "Getting creator detail");

    reject_colon_verb_segment(&creator_id)?;
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

/// `PATCH /v1/daemon/creators/{creator_id}` — update creator display name.
///
/// Updates the local `creator_identity_cache.json` entry for the given creator,
/// creating a minimal entry if one does not yet exist.
///
/// **Note:** `display_name` updates also upsert into the creator SQL table and require an open pool.
/// When `active_creator_id` is present in config but the pool is not yet open (clean first run
/// after `ensureSetupBootstrap`), the pool is lazily attached here (mirroring
/// `require_active_creator`). When no creator is active in config at all, the request returns
/// HTTP 409 `uninitialized`.
pub async fn patch_creator(
    State(state): State<WorkspaceState>,
    Path(creator_id): Path<String>,
    Json(req): Json<PatchCreatorRequest>,
) -> Result<Json<CreatorDetail>, NexusApiError> {
    info!(creator_id = %creator_id, "Patching creator");

    reject_colon_verb_segment(&creator_id)?;
    validate_creator_id_safe(&creator_id).map_err(|reason| NexusApiError::InvalidInput {
        field: "creator_id".to_string(),
        reason,
    })?;

    if let Some(ref display_name) = req.display_name {
        if display_name.is_empty() {
            return Err(NexusApiError::InvalidInput {
                field: "display_name".to_string(),
                reason: "display_name cannot be empty".to_string(),
            });
        }
        if display_name.chars().count() > 256 {
            return Err(NexusApiError::InvalidInput {
                field: "display_name".to_string(),
                reason: "display_name must be 256 characters or fewer".to_string(),
            });
        }
    }

    let home = dirs::home_dir().ok_or_else(|| NexusApiError::Internal {
        code: "HOME_DIR_ERROR".into(),
        message: "Cannot determine home directory".to_string(),
    })?;
    let cache_path = home.join(".nexus42").join("creator_identity_cache.json");

    // Only initialize a fresh cache when the file does not exist. If the file
    // exists but cannot be parsed, report an error instead of silently wiping
    // all cached identities (QC2-F-002).
    let mut cache = if cache_path.exists() {
        load_identity_cache_strict(&cache_path)?
    } else {
        serde_json::json!({"creators": {}})
    };

    if !cache.is_object() {
        return Err(NexusApiError::Internal {
            code: "CACHE_FORMAT_ERROR".into(),
            message: "Identity cache root is not an object".to_string(),
        });
    }
    let cache_obj = cache
        .as_object_mut()
        .ok_or_else(|| NexusApiError::Internal {
            code: "CACHE_FORMAT_ERROR".into(),
            message: "Identity cache root is not an object".to_string(),
        })?;
    if cache_obj.get("creators").is_none_or(|v| !v.is_object()) {
        return Err(NexusApiError::Internal {
            code: "CACHE_FORMAT_ERROR".into(),
            message: "Identity cache creators field is not an object".to_string(),
        });
    }
    let creators = cache_obj
        .get_mut("creators")
        .and_then(|v| v.as_object_mut())
        .ok_or_else(|| NexusApiError::Internal {
            code: "CACHE_FORMAT_ERROR".into(),
            message: "Identity cache creators field is not an object".to_string(),
        })?;

    let entry = creators
        .entry(creator_id.clone())
        .or_insert_with(|| serde_json::json!({}));
    let entry_obj = entry
        .as_object_mut()
        .ok_or_else(|| NexusApiError::Internal {
            code: "CACHE_FORMAT_ERROR".into(),
            message: "Identity cache entry is not an object".to_string(),
        })?;

    if let Some(display_name) = req.display_name {
        // Lazily attach the creator pool when `active_creator_id` is present in
        // config but the pool is not yet open. On a clean first run the daemon
        // boots without a creator DB (AC-P0-1); `ensureSetupBootstrap` then
        // writes `active_creator_id` to config, but PATCH creator is Tier-1 (no
        // `require_active_creator` middleware) so without this attach the
        // display-name persist returns HTTP 409 `uninitialized`. Mirrors the
        // `require_active_creator` middleware (api/middleware.rs).
        if state.pool().is_none() && read_active_creator_id(state.nexus_home()).is_some() {
            state
                .ensure_creator_pool()
                .await
                .map_err(|e| NexusApiError::Internal {
                    code: "DATABASE_ERROR".into(),
                    message: e.to_string(),
                })?;
        }
        // Write the display name to the SQL `creators` table so that `list_creators`
        // (used by the footer via useCreators()) reflects the rename as well as
        // the JSON identity cache (QC1-F-001).
        upsert_creator_display_name(state.pool_or_uninit()?, &creator_id, &display_name).await?;
        entry_obj.insert(
            "display_name".to_string(),
            serde_json::Value::String(display_name),
        );
    }

    save_identity_cache(&cache_path, &cache)?;

    // Re-read the updated cache so the returned detail reflects the write.
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

/// `PUT /v1/daemon/creators/active` — set active creator
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

    state
        .ensure_creator_pool()
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: e.to_string(),
        })?;

    Ok(Json(SetActiveCreatorResponse {
        creator_id: req.creator_id,
    }))
}

/// `GET /v1/daemon/creators/active` — get active creator
pub async fn get_active_creator(
    State(state): State<WorkspaceState>,
) -> Result<Json<ActiveCreatorResponse>, NexusApiError> {
    let creator_id = read_active_creator_id(state.nexus_home())
        .ok_or_else(|| NexusApiError::NotFound("No active creator selected".to_string()))?;

    let cache = load_identity_cache();
    let entry = get_identity_entry(&cache, &creator_id);

    Ok(Json(ActiveCreatorResponse {
        creator_id,
        handle: entry.as_ref().and_then(|e| e.handle.clone()),
        display_name: entry.and_then(|e| e.display_name),
    }))
}

/// `POST /v1/daemon/creators/{id}:logout` — clear credentials.
///
/// Routed as `POST /v1/daemon/creators/:creator_id` because matchit 0.7 cannot
/// register `:id:logout` as a separate pattern. The path segment must end with
/// `:logout`; otherwise this returns 404 (plain POST without the verb is not a
/// logout).
pub async fn logout_creator(
    State(state): State<WorkspaceState>,
    Path(segment): Path<String>,
) -> Result<Json<LogoutResponse>, NexusApiError> {
    let creator_id = segment
        .strip_suffix(":logout")
        .ok_or_else(|| NexusApiError::NotFound(format!("Creator route '{segment}' not found")))?
        .to_string();

    info!(creator_id = %creator_id, "Logging out creator");

    if creator_id.is_empty() || creator_id.contains(':') {
        return Err(NexusApiError::InvalidInput {
            field: "creator_id".to_string(),
            reason: "creator_id must not be empty or contain ':'".to_string(),
        });
    }

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

/// Reject path segments that look like Google-AIP custom verbs (`id:verb`).
///
/// Those URLs share the `:creator_id` capture with logout; GET/PATCH must not
/// treat `ctr_x:logout` as a valid creator id (ghost 200).
fn reject_colon_verb_segment(creator_id: &str) -> Result<(), NexusApiError> {
    if creator_id.contains(':') {
        return Err(NexusApiError::InvalidInput {
            field: "creator_id".to_string(),
            reason: "creator_id must not contain ':'".to_string(),
        });
    }
    Ok(())
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_workspace;
    use serial_test::serial;

    /// Temporarily override `HOME` so disk-backed helpers (e.g. `load_identity_cache`)
    /// operate inside the isolated test directory. The original value is restored on drop.
    struct HomeOverride {
        original: Option<String>,
    }

    impl HomeOverride {
        fn set(home: &std::path::Path) -> Self {
            let original = std::env::var("HOME").ok();
            std::env::set_var("HOME", home);
            Self { original }
        }
    }

    impl Drop for HomeOverride {
        fn drop(&mut self) {
            match &self.original {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
        }
    }

    #[test]
    fn validate_creator_id_rejects_traversal() {
        assert!(validate_creator_id_safe("../etc").is_err());
    }

    #[test]
    fn validate_creator_id_accepts_valid() {
        assert!(validate_creator_id_safe("crt_abc123").is_ok());
    }

    #[tokio::test]
    #[serial]
    async fn list_without_active_creator_returns_empty_list_not_uninitialized() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let user_home = tmp.path();
        let nexus_home = user_home.join(".nexus42");
        nexus_home_layout::ensure_system_layout(&nexus_home).expect("system layout");

        let _home_override = HomeOverride::set(user_home);

        let state = crate::workspace::WorkspaceState::initialize()
            .await
            .expect("initialize");
        assert!(state.pool().is_none());

        let result = list(
            State(state),
            Query(ListCreatorsQuery {
                limit: 50,
                cursor: None,
            }),
        )
        .await;

        let body = result.expect("list should succeed without pool, not return 409");
        assert!(body.0.items.is_empty());
    }

    #[tokio::test]
    #[serial]
    async fn set_active_creator_opens_pool_on_attach() {
        const CREATOR_ID: &str = "crt_set_active_pool";

        let tmp = tempfile::TempDir::new().expect("temp dir");
        let user_home = tmp.path();
        let nexus_home = user_home.join(".nexus42");
        nexus_home_layout::ensure_system_layout(&nexus_home).expect("system layout");

        let cache = serde_json::json!({
            "creators": {
                CREATOR_ID: { "handle": "set-active" }
            }
        });
        std::fs::write(
            nexus_home.join("creator_identity_cache.json"),
            serde_json::to_string_pretty(&cache).expect("cache json"),
        )
        .expect("write cache");

        let op_dir = nexus_home_layout::operational_workspace_dir(user_home, CREATOR_ID, "default");
        std::fs::create_dir_all(&op_dir).expect("operational dir");
        let meta = serde_json::json!({
            "schema_version": 1,
            "creator_id": CREATOR_ID,
            "workspace_slug": "default",
            "local_root": user_home.join("creative"),
            "created_at": "2020-01-01T00:00:00Z"
        });
        std::fs::write(
            op_dir.join("meta.json"),
            serde_json::to_string(&meta).expect("meta json"),
        )
        .expect("meta.json");

        let _home_override = HomeOverride::set(user_home);

        let state = crate::workspace::WorkspaceState::initialize()
            .await
            .expect("initialize");
        assert!(state.pool().is_none());

        set_active_creator(
            State(state.clone()),
            Json(SetActiveCreatorRequest {
                creator_id: CREATOR_ID.to_string(),
            }),
        )
        .await
        .expect("set_active_creator should succeed");

        assert!(
            state.pool().is_some(),
            "set_active_creator should open pool on shared creator_db slot (H1)"
        );
    }

    #[tokio::test]
    async fn get_active_without_creator_returns_not_found() {
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
            NexusApiError::NotFound(_) => {}
            other => panic!("Expected NotFound, got: {other}"),
        }
    }

    #[tokio::test]
    #[serial]
    async fn patch_creator_updates_display_name() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let home = tmp.path();
        let _home_override = HomeOverride::set(home);

        let cache_path = home.join(".nexus42").join("creator_identity_cache.json");
        let cache = serde_json::json!({
            "creators": {
                "crt_abc123": { "handle": "old_handle" }
            }
        });
        std::fs::write(
            &cache_path,
            serde_json::to_string_pretty(&cache).expect("serialize cache"),
        )
        .expect("write cache");

        let req = PatchCreatorRequest {
            display_name: Some("New Display Name".to_string()),
        };
        let result = patch_creator(
            State(state.clone()),
            Path("crt_abc123".to_string()),
            Json(req),
        )
        .await;
        assert!(result.is_ok(), "patch_creator should succeed");
        let detail = result.expect("result should be Ok").0;
        assert_eq!(detail.creator_id, "crt_abc123");
        assert_eq!(detail.display_name, Some("New Display Name".to_string()));

        let get_result = get_creator(State(state), Path("crt_abc123".to_string())).await;
        assert!(get_result.is_ok(), "get_creator should succeed");
        let get_detail = get_result.expect("result should be Ok").0;
        assert_eq!(
            get_detail.display_name,
            Some("New Display Name".to_string())
        );
    }

    #[tokio::test]
    #[serial]
    async fn patch_creator_rejects_empty_display_name() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let home = tmp.path();
        let _home_override = HomeOverride::set(home);

        let req = PatchCreatorRequest {
            display_name: Some("".to_string()),
        };
        let result = patch_creator(State(state), Path("crt_abc123".to_string()), Json(req)).await;
        assert!(result.is_err(), "empty display_name should be rejected");
        match result.unwrap_err() {
            NexusApiError::InvalidInput { field, .. } => {
                assert_eq!(field, "display_name");
            }
            other => panic!("Expected InvalidInput, got: {other}"),
        }
    }

    #[tokio::test]
    #[serial]
    async fn patch_creator_creates_entry_when_missing() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let home = tmp.path();
        let _home_override = HomeOverride::set(home);

        // No cache file on disk initially.
        let req = PatchCreatorRequest {
            display_name: Some("Fresh Creator".to_string()),
        };
        let result = patch_creator(
            State(state.clone()),
            Path("crt_fresh".to_string()),
            Json(req),
        )
        .await;
        assert!(result.is_ok(), "patch_creator should create missing entry");
        let detail = result.expect("result should be Ok").0;
        assert_eq!(detail.creator_id, "crt_fresh");
        assert_eq!(detail.display_name, Some("Fresh Creator".to_string()));

        let cache_path = home.join(".nexus42").join("creator_identity_cache.json");
        let cache_content = std::fs::read_to_string(&cache_path).expect("read cache");
        let cache: serde_json::Value = serde_json::from_str(&cache_content).expect("parse cache");
        assert_eq!(
            cache["creators"]["crt_fresh"]["display_name"],
            "Fresh Creator"
        );
    }

    #[tokio::test]
    #[serial]
    async fn patch_creator_writes_display_name_to_sql_creators_table() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let home = tmp.path();
        let _home_override = HomeOverride::set(home);

        let req = PatchCreatorRequest {
            display_name: Some("Renamed Profile".to_string()),
        };
        let result =
            patch_creator(State(state.clone()), Path("crt_sql".to_string()), Json(req)).await;
        assert!(result.is_ok(), "patch_creator should succeed");
        let detail = result.expect("result should be Ok").0;
        assert_eq!(detail.display_name, Some("Renamed Profile".to_string()));

        let response = list(
            State(state),
            Query(ListCreatorsQuery {
                limit: 50,
                cursor: None,
            }),
        )
        .await
        .expect("list creators should succeed")
        .0;
        let item = response
            .items
            .into_iter()
            .find(|i| i.creator_id == "crt_sql")
            .expect("creator should be present in SQL list");
        assert_eq!(item.display_name, "Renamed Profile");
    }

    #[tokio::test]
    #[serial]
    async fn patch_creator_accepts_256_character_multibyte_display_name() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let home = tmp.path();
        let _home_override = HomeOverride::set(home);

        let name = "中".repeat(256);
        assert_eq!(name.chars().count(), 256);
        assert!(
            name.len() > 256,
            "byte length should exceed 256 for CJK chars"
        );

        let req = PatchCreatorRequest {
            display_name: Some(name.clone()),
        };
        let result = patch_creator(State(state), Path("crt_multi".to_string()), Json(req)).await;
        assert!(
            result.is_ok(),
            "256-character multibyte display_name should be accepted"
        );
        assert_eq!(result.unwrap().0.display_name, Some(name));
    }

    #[tokio::test]
    #[serial]
    async fn patch_creator_rejects_257_character_multibyte_display_name() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let home = tmp.path();
        let _home_override = HomeOverride::set(home);

        let name = "🎨".repeat(257);
        assert_eq!(name.chars().count(), 257);

        let req = PatchCreatorRequest {
            display_name: Some(name),
        };
        let result = patch_creator(State(state), Path("crt_multi".to_string()), Json(req)).await;
        assert!(
            result.is_err(),
            "257-character display_name should be rejected"
        );
        match result.unwrap_err() {
            NexusApiError::InvalidInput { field, .. } => {
                assert_eq!(field, "display_name");
            }
            other => panic!("Expected InvalidInput, got: {other}"),
        }
    }

    #[tokio::test]
    #[serial]
    async fn patch_creator_rejects_corrupt_cache_without_wiping_it() {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let home = tmp.path();
        let _home_override = HomeOverride::set(home);

        let cache_path = home.join(".nexus42").join("creator_identity_cache.json");
        std::fs::create_dir_all(cache_path.parent().unwrap()).expect("mkdir cache dir");
        std::fs::write(&cache_path, "not valid json {").expect("write corrupt cache");

        let req = PatchCreatorRequest {
            display_name: Some("New Name".to_string()),
        };
        let result = patch_creator(State(state), Path("crt_corrupt".to_string()), Json(req)).await;
        assert!(result.is_err(), "corrupt cache should be rejected");
        match result.unwrap_err() {
            NexusApiError::Internal { code, .. } => {
                assert_eq!(code, "CACHE_PARSE_ERROR");
            }
            other => panic!("Expected Internal CACHE_PARSE_ERROR, got: {other}"),
        }

        let preserved = std::fs::read_to_string(&cache_path).expect("read cache");
        assert_eq!(preserved, "not valid json {");
    }
}
