//! Workspace management handlers (V1.20 Batch 4, T21–T24).
//!
//! Replaces the old single-workspace `GET /v1/daemon/workspace` and `POST /v1/daemon/workspace/init`
//! with a multi-workspace CRUD API under `/v1/daemon/workspaces`.

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Query, State};
use axum::Json;
use nexus_contracts::generated::daemon_api::workspace::{
    list_workspaces_response::{NexusPaginationInfo, NexusWorkspaceSummary},
    ActiveWorkspaceResponse, ListWorkspacesQuery, ListWorkspacesResponse,
    SetActiveWorkspaceRequest, SetActiveWorkspaceResponse,
};
use nexus_core::CoreHomeService;
use nexus_home_layout::{
    operational_workspace_dir, validate_creator_id_safe, workspace_state_db_path,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::Mutex;

// ─── Request / Response types ──────────────────────────────────────────────
//
// Wire DTOs are the schema-generated `nexus-contracts` workspace types
// (v1.190 P2-T0: the handler keeps only transport translation). The daemon
// retains its own query-parsing default (`limit` defaults to 50) and page
// cap on top of the core-discovered workspace list.

/// Maximum items per page.
const MAX_LIMIT: usize = 250;

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

// ─── Workspace List Cache (QC3 W-004) ──────────────────────────────────────

/// TTL for workspace list cache (60 seconds).
const WORKSPACE_CACHE_TTL_SECS: u64 = 60;

/// Cached workspace list with a timestamp.
type WorkspaceCache = (std::time::Instant, Vec<NexusWorkspaceSummary>);

/// Module-level workspace list cache.
/// Stores unfiltered list + timestamp; invalidated on create/delete.
static WORKSPACE_CACHE: LazyLock<Mutex<Option<WorkspaceCache>>> =
    LazyLock::new(|| Mutex::new(None));

/// Invalidate the workspace list cache (call after create/delete).
fn invalidate_workspace_cache() {
    *WORKSPACE_CACHE
        .lock()
        .expect("workspace cache lock should not be poisoned") = None;
}

/// Try to get cached workspaces. Returns `None` if cache is empty or expired.
/// The cache stores the full unfiltered list; callers apply their own filters.
fn get_cached_workspaces() -> Option<Vec<NexusWorkspaceSummary>> {
    let cache = WORKSPACE_CACHE
        .lock()
        .expect("workspace cache lock should not be poisoned");
    match *cache {
        Some((instant, ref items)) if instant.elapsed().as_secs() < WORKSPACE_CACHE_TTL_SECS => {
            Some(items.clone())
        }
        _ => None,
    }
}

/// Store workspaces in cache (unfiltered, full list).
fn cache_workspaces(items: Vec<NexusWorkspaceSummary>) {
    *WORKSPACE_CACHE
        .lock()
        .expect("workspace cache lock should not be poisoned") =
        Some((std::time::Instant::now(), items));
}

// ─── Helpers ───────────────────────────────────────────────────────────────

/// Validate a slug: non-empty, single path segment, no `.` / `..`.
pub(crate) fn validate_slug(label: &str, value: &str) -> Result<(), NexusApiError> {
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

/// Read `creative_root` from operational `meta.json`.
fn read_meta_creative_root(op_dir: &std::path::Path) -> Option<String> {
    let meta_path = op_dir.join("meta.json");
    let content = std::fs::read_to_string(&meta_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json.get("local_root")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
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

    // Use nexus_local_db for schema init (same as CLI path; P2 QC3 F-001:
    // init_pool carries the co-boot migration retry/backoff).
    nexus_local_db::init_pool(&db_path)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DB_INIT_ERROR".into(),
            message: format!("Failed to initialize state DB: {e}"),
        })?;

    Ok(db_path)
}

// ─── Handlers ──────────────────────────────────────────────────────────────

/// `GET /v1/daemon/workspaces` — list workspaces (T21)
///
/// Workspace discovery is owned by `CoreHomeService` (v1.190 P2-T0); this
/// route applies the retained query semantics (creator filter, page cap,
/// opaque cursor) on the core-discovered list and keeps the QC3 W-004 cache.
pub async fn list_workspaces(
    State(state): State<WorkspaceState>,
    Query(query): Query<ListWorkspacesQuery>,
) -> Result<Json<ListWorkspacesResponse>, NexusApiError> {
    // Validate creator_id filter if provided
    if let Some(cid) = &query.creator_id {
        validate_creator_id_safe(cid).map_err(|reason| NexusApiError::InvalidInput {
            field: "creator_id".to_string(),
            reason,
        })?;
    }

    let limit = query.limit.unwrap_or(50).clamp(1, MAX_LIMIT as i64) as usize;

    let home = CoreHomeService::open(user_home_of(&state)?).map_err(NexusApiError::from)?;
    let all_items = if let Some(items) = get_cached_workspaces() {
        items
    } else {
        let items = home.list_workspaces().await?.items;
        cache_workspaces(items.clone());
        items
    };

    // Apply creator_id filter to the full list.
    let mut items: Vec<NexusWorkspaceSummary> = if let Some(filter) = &query.creator_id {
        all_items
            .into_iter()
            .filter(|i| i.creator_id == *filter)
            .collect()
    } else {
        all_items
    };
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
        pagination: NexusPaginationInfo {
            limit: i64::try_from(limit).unwrap_or(i64::MAX),
            has_more: next_cursor.is_some(),
            next_cursor,
        },
    }))
}

/// `POST /v1/daemon/workspaces` — create/materialize workspace (T22)
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

    // Invalidate workspace cache after creating a new workspace (QC3 W-004).
    invalidate_workspace_cache();

    Ok(Json(CreateWorkspaceResponse {
        creator_id: req.creator_id,
        workspace_slug: req.workspace_slug,
        creative_root: creative_root.display().to_string(),
        operational_dir: op_dir.display().to_string(),
        state_db_path: db_path.display().to_string(),
    }))
}

/// `GET /v1/daemon/workspaces/active` — return active workspace selection (T23)
///
/// The resolved home configuration is owned by `CoreHomeService` (v1.190
/// P2-T0); this route keeps the retained 409 UNINITIALIZED envelope when no
/// active creator is configured and projects the operational paths.
pub async fn get_active_workspace(
    State(state): State<WorkspaceState>,
) -> Result<Json<ActiveWorkspaceResponse>, NexusApiError> {
    let home = CoreHomeService::open(user_home_of(&state)?)?;
    let config = home.configuration().await?;

    let creator_id = config
        .active_creator_id
        .ok_or(NexusApiError::Uninitialized)?;
    let workspace_slug = config
        .active_workspace_slug
        .unwrap_or_else(|| "default".to_string());

    let user_home = user_home_of(&state)?;
    let op_dir = operational_workspace_dir(&user_home, &creator_id, &workspace_slug);
    let creative_root = read_meta_creative_root(&op_dir);

    Ok(Json(ActiveWorkspaceResponse {
        creator_id,
        workspace_slug,
        creative_root,
        operational_dir: op_dir.display().to_string(),
    }))
}

/// `PUT /v1/daemon/workspaces/active` — set active workspace (T24)
///
/// Selection persistence and validation (existence plus foreign-workspace
/// denial) are owned by `CoreHomeService::select_workspace`; this route is
/// status/DTO translation only (v1.190 P2-T0).
pub async fn set_active_workspace(
    State(state): State<WorkspaceState>,
    Json(req): Json<SetActiveWorkspaceRequest>,
) -> Result<Json<SetActiveWorkspaceResponse>, NexusApiError> {
    let home = CoreHomeService::open(user_home_of(&state)?)?;
    let response = home.select_workspace(req).await?;

    Ok(Json(response))
}

/// The raw user home behind the daemon's `.nexus42` nexus home (boot.rs
/// raw-user-home precedent). Fails honestly when the nexus home has no
/// parent.
fn user_home_of(state: &WorkspaceState) -> Result<PathBuf, NexusApiError> {
    state
        .nexus_home()
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| NexusApiError::Internal {
            code: "HOME_DIR_ERROR".into(),
            message: "Cannot determine home directory".to_string(),
        })
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
            limit: Some(50),
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
            workspace_slug: String::new(),
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
        assert_eq!(err.error_code(), "conflict");
    }
}
