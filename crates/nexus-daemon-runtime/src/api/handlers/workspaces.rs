//! Workspace management handlers (V1.20 Batch 4, T21–T24).
//!
//! Replaces the old single-workspace `GET /v1/local/workspace` and `POST /v1/local/workspace/init`
//! with a multi-workspace CRUD API under `/v1/local/workspaces`.

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Query, State};
use axum::Json;
use nexus_home_layout::{
    operational_workspace_dir, validate_creator_id_safe, workspace_state_db_path,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ─── Request / Response types ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListWorkspacesQuery {
    /// Optional filter: only show workspaces for this creator.
    pub creator_id: Option<String>,
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

#[derive(Debug, Serialize)]
pub struct ListWorkspacesResponse {
    pub items: Vec<WorkspaceSummary>,
    pub pagination: PaginationEnvelope,
}

/// Cursor-based pagination envelope.
#[derive(Debug, Serialize)]
pub struct PaginationEnvelope {
    pub limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceSummary {
    pub creator_id: String,
    pub workspace_slug: String,
    pub creative_root: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub creator_id: String,
    pub workspace_slug: String,
    /// Absolute or relative creative root. If relative, resolved from cwd.
    /// If absent, defaults to `~/Documents/nexus/<creator_id>/<workspace_slug>`.
    pub creative_root: Option<PathBuf>,
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateWorkspaceResponse {
    pub creator_id: String,
    pub workspace_slug: String,
    pub creative_root: String,
    pub operational_dir: String,
    pub state_db_path: String,
}

#[derive(Debug, Serialize)]
pub struct ActiveWorkspaceResponse {
    pub creator_id: String,
    pub workspace_slug: String,
    pub creative_root: Option<String>,
    pub operational_dir: String,
}

#[derive(Debug, Deserialize)]
pub struct SetActiveWorkspaceRequest {
    pub creator_id: Option<String>,
    pub workspace_slug: String,
}

#[derive(Debug, Serialize)]
pub struct SetActiveWorkspaceResponse {
    pub creator_id: String,
    pub workspace_slug: String,
}

// ─── Helpers ───────────────────────────────────────────────────────────────

/// Validate a slug: non-empty, single path segment, no `.` / `..`.
fn validate_slug(label: &str, value: &str) -> Result<(), NexusApiError> {
    if value.is_empty()
        || value.contains('/')
        || value.contains('\\')
        || value == "."
        || value == ".."
    {
        return Err(NexusApiError::InvalidInput {
            field: label.to_string(),
            reason: "must be a single path segment".to_string(),
        });
    }
    Ok(())
}

/// Read `display_name` from `.nexus42/workspace.json` in the creative root.
/// Returns `None` if the file doesn't exist or can't be parsed.
fn read_workspace_display_name(creative_root: &std::path::Path) -> Option<String> {
    let config_path = creative_root.join(".nexus42").join("workspace.json");
    let content = std::fs::read_to_string(&config_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json.get("name")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
}

/// Read `creative_root` from operational `meta.json`.
fn read_meta_creative_root(op_dir: &std::path::Path) -> Option<String> {
    let meta_path = op_dir.join("meta.json");
    let content = std::fs::read_to_string(&meta_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json.get("local_root")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
}

/// Scan all creator directories under `nexus_home` and collect workspace registrations.
fn scan_workspaces(
    nexus_home: &std::path::Path,
    creator_filter: Option<&str>,
) -> Vec<WorkspaceSummary> {
    let creators_root = nexus_home.join("creators");
    let mut items = Vec::new();

    let Ok(creator_entries) = std::fs::read_dir(&creators_root) else {
        return items;
    };

    for creator_entry in creator_entries.flatten() {
        if !creator_entry.path().is_dir() {
            continue;
        }
        let Ok(creator_id) = creator_entry.file_name().into_string() else {
            continue;
        };

        // Apply optional filter
        if let Some(filter) = creator_filter {
            if creator_id != filter {
                continue;
            }
        }

        // Note: workspace slugs live under nexus_home/creators/<id>/workspaces/
        // This matches the ADR-014 layout created by nexus_home_layout functions
        // when called with the user home (nexus_home = user_home/.nexus42).
        let ws_root = creators_root.join(&creator_id).join("workspaces");
        let Ok(ws_entries) = std::fs::read_dir(&ws_root) else {
            continue;
        };

        for ws_entry in ws_entries.flatten() {
            if !ws_entry.path().is_dir() {
                continue;
            }
            let Ok(slug) = ws_entry.file_name().into_string() else {
                continue;
            };

            // Only include workspaces that have a meta.json
            let op_dir = ws_entry.path();
            if !op_dir.join("meta.json").exists() {
                continue;
            }

            let creative_root = read_meta_creative_root(&op_dir);
            let display_name = creative_root
                .as_deref()
                .and_then(|cr| read_workspace_display_name(std::path::Path::new(cr)));

            items.push(WorkspaceSummary {
                creator_id: creator_id.clone(),
                workspace_slug: slug,
                creative_root: creative_root.unwrap_or_default(),
                display_name,
            });
        }
    }

    items.sort_by(|a, b| {
        a.creator_id
            .cmp(&b.creator_id)
            .then(a.workspace_slug.cmp(&b.workspace_slug))
    });

    items
}

/// Read active `creator_id` from CLI config (config.toml in `nexus_home`).
fn read_active_creator_id(nexus_home: &std::path::Path) -> Option<String> {
    let config_path = nexus_home.join("config.toml");
    let content = std::fs::read_to_string(&config_path).ok()?;
    let config: toml::Value = toml::from_str(&content).ok()?;
    config
        .get("active_creator_id")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
}

/// Read active workspace slug for a creator from CLI config.
fn read_active_workspace_slug(nexus_home: &std::path::Path, creator_id: &str) -> Option<String> {
    let config_path = nexus_home.join("config.toml");
    let content = std::fs::read_to_string(&config_path).ok()?;
    let config: toml::Value = toml::from_str(&content).ok()?;
    config
        .get("active_workspace_slug_by_creator")
        .and_then(|v| v.get(creator_id))
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
}

/// Write active `creator_id` and workspace slug to CLI config (config.toml).
fn write_active_selection(
    nexus_home: &std::path::Path,
    creator_id: &str,
    workspace_slug: &str,
) -> Result<(), NexusApiError> {
    let config_path = nexus_home.join("config.toml");

    // Read existing config or start fresh
    let mut config: toml::Value = if config_path.exists() {
        let content =
            std::fs::read_to_string(&config_path).map_err(|e| NexusApiError::Internal {
                code: "CONFIG_READ_ERROR".into(),
                message: e.to_string(),
            })?;
        if content.trim().is_empty() {
            toml::Value::Table(toml::map::Map::new())
        } else {
            toml::from_str(&content).map_err(|e| NexusApiError::Internal {
                code: "CONFIG_PARSE_ERROR".into(),
                message: e.to_string(),
            })?
        }
    } else {
        toml::Value::Table(toml::map::Map::new())
    };

    let table = config
        .as_table_mut()
        .ok_or_else(|| NexusApiError::Internal {
            code: "CONFIG_ERROR".into(),
            message: "config root is not a table".to_string(),
        })?;

    // Set active_creator_id
    table.insert(
        "active_creator_id".to_string(),
        toml::Value::String(creator_id.to_string()),
    );

    // Set active_workspace_slug_by_creator.<creator_id>
    let slug_table = table
        .entry("active_workspace_slug_by_creator")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    slug_table
        .as_table_mut()
        .ok_or_else(|| NexusApiError::Internal {
            code: "CONFIG_ERROR".into(),
            message: "active_workspace_slug_by_creator is not a table".to_string(),
        })?
        .insert(
            creator_id.to_string(),
            toml::Value::String(workspace_slug.to_string()),
        );

    let toml_str = toml::to_string_pretty(&config).map_err(|e| NexusApiError::Internal {
        code: "CONFIG_SERIALIZE_ERROR".into(),
        message: e.to_string(),
    })?;

    std::fs::write(&config_path, toml_str).map_err(|e| NexusApiError::Internal {
        code: "CONFIG_WRITE_ERROR".into(),
        message: e.to_string(),
    })?;

    Ok(())
}

/// Default creative root: `~/Documents/nexus/<creator_id>/<workspace_slug>`
fn default_creative_root(creator_id: &str, workspace_slug: &str) -> Result<PathBuf, NexusApiError> {
    let docs = dirs::document_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Documents")))
        .ok_or_else(|| NexusApiError::Internal {
            code: "HOME_DIR_ERROR".into(),
            message: "Cannot resolve Documents directory".to_string(),
        })?;
    Ok(docs.join("nexus").join(creator_id).join(workspace_slug))
}

/// Materialize an ADR-014 workspace: creative tree + operational registration + state DB.
async fn materialize_workspace(
    user_home: &std::path::Path,
    creator_id: &str,
    workspace_slug: &str,
    creative_root: &std::path::Path,
    display_name: &str,
) -> Result<PathBuf, NexusApiError> {
    // Creative tree: .nexus42/workspace.json + .gitignore
    let nexus_dir = creative_root.join(".nexus42");
    std::fs::create_dir_all(&nexus_dir).map_err(|e| NexusApiError::Internal {
        code: "DIR_CREATE_ERROR".into(),
        message: format!("Failed to create creative nexus dir: {e}"),
    })?;

    let workspace_config = serde_json::json!({
        "name": display_name,
        "version": 1,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "creator_id": creator_id,
        "workspace_slug": workspace_slug,
    });

    let config_path = creative_root.join(".nexus42").join("workspace.json");
    std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&workspace_config).map_err(|e| NexusApiError::Internal {
            code: "JSON_ERROR".into(),
            message: e.to_string(),
        })?,
    )
    .map_err(|e| NexusApiError::Internal {
        code: "FILE_WRITE_ERROR".into(),
        message: format!("Failed to write workspace.json: {e}"),
    })?;

    let gitignore_content =
        "# Nexus local state (do not commit)\n*.db\n*.db-wal\n*.db-shm\nstate.db\n";
    std::fs::write(nexus_dir.join(".gitignore"), gitignore_content).map_err(|e| {
        NexusApiError::Internal {
            code: "FILE_WRITE_ERROR".into(),
            message: format!("Failed to write .gitignore: {e}"),
        }
    })?;

    // Operational registration: meta.json
    let op_dir = operational_workspace_dir(user_home, creator_id, workspace_slug);
    std::fs::create_dir_all(&op_dir).map_err(|e| NexusApiError::Internal {
        code: "DIR_CREATE_ERROR".into(),
        message: format!("Failed to create operational dir: {e}"),
    })?;

    let op_meta = op_dir.join("meta.json");
    let meta = serde_json::json!({
        "schema_version": 1,
        "creator_id": creator_id,
        "workspace_slug": workspace_slug,
        "local_root": creative_root,
        "workspace_id": null,
        "created_at": chrono::Utc::now().to_rfc3339(),
    });
    std::fs::write(
        &op_meta,
        serde_json::to_string_pretty(&meta).map_err(|e| NexusApiError::Internal {
            code: "JSON_ERROR".into(),
            message: e.to_string(),
        })?,
    )
    .map_err(|e| NexusApiError::Internal {
        code: "FILE_WRITE_ERROR".into(),
        message: format!("Failed to write meta.json: {e}"),
    })?;

    // State DB initialization
    let db_path = workspace_state_db_path(user_home, creator_id, workspace_slug);
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| NexusApiError::Internal {
            code: "DIR_CREATE_ERROR".into(),
            message: format!("Failed to create db dir: {e}"),
        })?;
    }

    // Use nexus_local_db for schema init (same as CLI path)
    let pool = nexus_local_db::open_pool(&db_path)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DB_OPEN_ERROR".into(),
            message: format!("Failed to open state DB: {e}"),
        })?;
    nexus_local_db::run_migrations(&pool)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DB_MIGRATION_ERROR".into(),
            message: format!("Failed to run DB migrations: {e}"),
        })?;
    nexus_local_db::seed_versions(&pool)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DB_SEED_ERROR".into(),
            message: format!("Failed to seed DB versions: {e}"),
        })?;

    Ok(db_path)
}

// ─── Handlers ──────────────────────────────────────────────────────────────

/// `GET /v1/local/workspaces` — list workspaces (T21)
///
/// Scans operational workspace directories on disk. Supports optional
/// `creator_id` query parameter for filtering.
pub async fn list_workspaces(
    State(state): State<WorkspaceState>,
    Query(query): Query<ListWorkspacesQuery>,
) -> Result<Json<ListWorkspacesResponse>, NexusApiError> {
    let nexus_home = state.nexus_home();

    // Validate creator_id filter if provided
    if let Some(ref cid) = query.creator_id {
        validate_creator_id_safe(cid).map_err(|reason| NexusApiError::InvalidInput {
            field: "creator_id".to_string(),
            reason,
        })?;
    }

    let limit = query.limit.clamp(1, MAX_LIMIT);
    let all_items = scan_workspaces(nexus_home, query.creator_id.as_deref());

    // Apply cursor-based pagination (cursor = "<creator_id>/<workspace_slug>")
    let mut items = all_items;
    if let Some(ref cursor) = query.cursor {
        // Skip past the cursor entry
        let pos = items
            .iter()
            .position(|i| format!("{}/{}", i.creator_id, i.workspace_slug) == *cursor);
        if let Some(idx) = pos {
            items = items.split_off(idx + 1);
        }
    }

    let next_cursor = if items.len() > limit {
        items.truncate(limit);
        items
            .last()
            .map(|i| format!("{}/{}", i.creator_id, i.workspace_slug))
    } else {
        None
    };

    Ok(Json(ListWorkspacesResponse {
        items,
        pagination: PaginationEnvelope { limit, next_cursor },
    }))
}

/// `POST /v1/local/workspaces` — create/materialize workspace (T22)
///
/// Creates creative tree, operational registration, and initializes state DB.
/// Per ADR-014: skeleton-free — no Stories/References directories created.
pub async fn create_workspace(
    State(_state): State<WorkspaceState>,
    Json(req): Json<CreateWorkspaceRequest>,
) -> Result<Json<CreateWorkspaceResponse>, NexusApiError> {
    // Validate inputs
    validate_creator_id_safe(&req.creator_id).map_err(|reason| NexusApiError::InvalidInput {
        field: "creator_id".to_string(),
        reason,
    })?;
    validate_slug("workspace_slug", &req.workspace_slug)?;

    let user_home = dirs::home_dir().ok_or_else(|| NexusApiError::Internal {
        code: "HOME_DIR_ERROR".into(),
        message: "Cannot determine home directory".to_string(),
    })?;

    // Check for existing workspace
    let op_meta = operational_workspace_dir(&user_home, &req.creator_id, &req.workspace_slug)
        .join("meta.json");
    if op_meta.exists() {
        return Err(NexusApiError::Conflict(format!(
            "Workspace {} already exists for creator {}",
            req.workspace_slug, req.creator_id
        )));
    }

    // Resolve creative root
    let creative_root = match req.creative_root {
        Some(p) if p.is_absolute() => p,
        Some(p) => std::env::current_dir()
            .map_err(|e| NexusApiError::Internal {
                code: "CWD_ERROR".into(),
                message: e.to_string(),
            })?
            .join(p),
        None => default_creative_root(&req.creator_id, &req.workspace_slug)?,
    };

    let display_name = req
        .display_name
        .unwrap_or_else(|| req.workspace_slug.clone());

    let db_path = materialize_workspace(
        &user_home,
        &req.creator_id,
        &req.workspace_slug,
        &creative_root,
        &display_name,
    )
    .await?;

    let op_dir = operational_workspace_dir(&user_home, &req.creator_id, &req.workspace_slug);

    Ok(Json(CreateWorkspaceResponse {
        creator_id: req.creator_id,
        workspace_slug: req.workspace_slug,
        creative_root: creative_root.display().to_string(),
        operational_dir: op_dir.display().to_string(),
        state_db_path: db_path.display().to_string(),
    }))
}

/// `GET /v1/local/workspaces/active` — return active workspace selection (T23)
///
/// Reads from CLI config. Returns 409 UNINITIALIZED if no active creator is set.
pub async fn get_active_workspace(
    State(state): State<WorkspaceState>,
) -> Result<Json<ActiveWorkspaceResponse>, NexusApiError> {
    let nexus_home = state.nexus_home();

    let creator_id = read_active_creator_id(nexus_home).ok_or(NexusApiError::Uninitialized)?;

    let workspace_slug = read_active_workspace_slug(nexus_home, &creator_id)
        .unwrap_or_else(|| "default".to_string());

    let user_home = dirs::home_dir().ok_or_else(|| NexusApiError::Internal {
        code: "HOME_DIR_ERROR".into(),
        message: "Cannot determine home directory".to_string(),
    })?;

    let op_dir = operational_workspace_dir(&user_home, &creator_id, &workspace_slug);
    let creative_root = read_meta_creative_root(&op_dir);

    Ok(Json(ActiveWorkspaceResponse {
        creator_id,
        workspace_slug,
        creative_root,
        operational_dir: op_dir.display().to_string(),
    }))
}

/// `PUT /v1/local/workspaces/active` — set active workspace (T24)
///
/// Persists selection to CLI config. Validates that the workspace exists on disk.
pub async fn set_active_workspace(
    State(state): State<WorkspaceState>,
    Json(req): Json<SetActiveWorkspaceRequest>,
) -> Result<Json<SetActiveWorkspaceResponse>, NexusApiError> {
    let nexus_home = state.nexus_home();

    // Resolve creator_id: use request value or current active
    let creator_id = match req.creator_id {
        Some(cid) => {
            validate_creator_id_safe(&cid).map_err(|reason| NexusApiError::InvalidInput {
                field: "creator_id".to_string(),
                reason,
            })?;
            cid
        }
        None => read_active_creator_id(nexus_home).ok_or(NexusApiError::Uninitialized)?,
    };

    validate_slug("workspace_slug", &req.workspace_slug)?;

    let user_home = dirs::home_dir().ok_or_else(|| NexusApiError::Internal {
        code: "HOME_DIR_ERROR".into(),
        message: "Cannot determine home directory".to_string(),
    })?;

    // Verify workspace exists on disk
    let op_dir = operational_workspace_dir(&user_home, &creator_id, &req.workspace_slug);
    if !op_dir.is_dir() {
        return Err(NexusApiError::NotFound(format!(
            "Workspace {} does not exist for creator {}",
            req.workspace_slug, creator_id
        )));
    }

    // Persist to CLI config
    write_active_selection(nexus_home, &creator_id, &req.workspace_slug)?;

    Ok(Json(SetActiveWorkspaceResponse {
        creator_id,
        workspace_slug: req.workspace_slug,
    }))
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_workspace;
    use crate::workspace::WorkspaceState;
    use axum::extract::State as AxumState;

    #[test]
    fn validate_slug_rejects_empty() {
        assert!(validate_slug("test", "").is_err());
    }

    #[test]
    fn validate_slug_rejects_slash() {
        assert!(validate_slug("test", "foo/bar").is_err());
    }

    #[test]
    fn validate_slug_rejects_dotdot() {
        assert!(validate_slug("test", "..").is_err());
    }

    #[test]
    fn validate_slug_accepts_valid() {
        assert!(validate_slug("test", "my-workspace").is_ok());
    }

    #[tokio::test]
    async fn list_workspaces_returns_test_workspace() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home.clone(), db_path, None).await;

        let query = ListWorkspacesQuery {
            creator_id: None,
            limit: 50,
            cursor: None,
        };
        let result = list_workspaces(AxumState(state), Query(query)).await;
        assert!(result.is_ok(), "list_workspaces should succeed");
        let body = result.expect("ok");
        // The test workspace has test_creator/default registered
        assert!(
            !body.items.is_empty(),
            "should find the test workspace, nexus_home={}",
            nexus_home.display()
        );
    }

    #[tokio::test]
    async fn get_active_without_creator_returns_uninitialized() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let nexus_home = tmp.path().join(".nexus42");
        std::fs::create_dir_all(&nexus_home).expect("create");
        // No config.toml → no active creator

        let db_path = nexus_home.join("state.db");
        let pool = nexus_local_db::open_pool(&db_path).await.expect("pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("migrate");
        nexus_local_db::seed_versions(&pool).await.expect("seed");

        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let result = get_active_workspace(AxumState(state)).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            NexusApiError::Uninitialized => {}
            other => panic!("Expected Uninitialized, got: {other}"),
        }
    }

    #[tokio::test]
    async fn create_workspace_rejects_empty_slug() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = CreateWorkspaceRequest {
            creator_id: "test".to_string(),
            workspace_slug: "".to_string(),
            creative_root: None,
            display_name: None,
        };
        let result = create_workspace(AxumState(state), Json(req)).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            NexusApiError::InvalidInput { field, .. } => {
                assert_eq!(field, "workspace_slug");
            }
            other => panic!("Expected InvalidInput, got: {other}"),
        }
    }

    #[tokio::test]
    async fn set_active_rejects_nonexistent_workspace() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let req = SetActiveWorkspaceRequest {
            creator_id: Some("test_creator".to_string()),
            workspace_slug: "nonexistent_ws".to_string(),
        };
        let result = set_active_workspace(AxumState(state), Json(req)).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            NexusApiError::NotFound(msg) => {
                assert!(msg.contains("nonexistent_ws"));
            }
            other => panic!("Expected NotFound, got: {other}"),
        }
    }

    #[tokio::test]
    async fn conflict_error_maps_correctly() {
        let err = NexusApiError::Conflict("already exists".to_string());
        assert_eq!(err.status_code(), axum::http::StatusCode::CONFLICT);
        assert_eq!(err.error_code(), "CONFLICT");
    }
}
