//! Creator handlers — local creator listing and management.
//!
//! Thin translation over the core `CoreHomeService` creator identity family
//! (v1.190 P2-T1): the Profile-membership SSOT scan, identity cache,
//! auth-store resolution and the workspace `creators` SQL enrichment live in
//! [`nexus_core`]; the handlers keep only route/verb handling, the
//! handler-local wire structs and status codes. HTTP status and response
//! envelopes are unchanged. Registration proxy routes were removed in V1.21
//! (Batch D); registration now lives in the CLI via `nexus-cloud-sync`.

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use nexus_contracts::generated::daemon_api::creators::{
    list_creators_query::ListCreatorsQuery as CoreListCreatorsQuery,
    set_active_creator_request::SetActiveCreatorRequest as CoreSetActiveCreatorRequest,
};
use nexus_contracts::PaginationInfo;
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

// ── Helpers ─────────────────────────────────────────────────────────

/// Resolve the home-control entry for this workspace (raw user home: the
/// home-layout appends `.nexus42` exactly once).
fn home_service(state: &WorkspaceState) -> Result<nexus_core::CoreHomeService, NexusApiError> {
    let user_home = state
        .nexus_home()
        .parent()
        .ok_or_else(|| NexusApiError::Internal {
            code: "NEXUS_HOME_INVALID".into(),
            message: "Workspace nexus home has no parent directory".to_string(),
        })?
        .to_path_buf();
    nexus_core::CoreHomeService::open(user_home).map_err(NexusApiError::from)
}

fn creator_detail_from_core(detail: nexus_contracts::CreatorDetail) -> CreatorDetail {
    CreatorDetail {
        creator_id: detail.creator_id,
        handle: detail.handle,
        display_name: detail.display_name,
        has_api_key: detail.has_api_key,
        has_cached_token: detail.has_cached_token,
        is_active: detail.is_active,
    }
}

fn creator_info_from_core(
    item: nexus_contracts::generated::daemon_api::creators::list_creators_response::NexusCreatorInfo,
) -> CreatorInfo {
    CreatorInfo {
        creator_id: item.creator_id,
        display_name: item.display_name,
        status: item.status,
        cached_at: item.cached_at,
    }
}

// ── Handlers ────────────────────────────────────────────────────────

///
/// # Errors
///
/// Returns [`NexusApiError`] when the creator/workspace guard rejects the
/// request, the core authority denies it (ownership, admission or validation),
/// or the bounded store read/write fails.
/// GET /v1/daemon/creators
pub async fn list(
    State(state): State<WorkspaceState>,
    Query(params): Query<ListCreatorsQuery>,
) -> Result<Json<ListCreatorsResponse>, NexusApiError> {
    info!("Handling list creators request");

    let limit = params.limit.clamp(1, MAX_LIMIT);
    let home = home_service(&state)?;
    let response = home
        .list_creators(CoreListCreatorsQuery {
            limit: Some(i64::try_from(limit).unwrap_or(i64::MAX)),
            cursor: params.cursor,
        })
        .await
        .map_err(NexusApiError::from)?;

    debug!(count = response.items.len(), "Creators retrieved");
    info!("List creators completed");
    Ok(Json(ListCreatorsResponse {
        items: response
            .items
            .into_iter()
            .map(creator_info_from_core)
            .collect(),
        pagination: PaginationInfo {
            limit: response.pagination.limit,
            has_more: response.pagination.has_more,
            next_cursor: response.pagination.next_cursor,
        },
    }))
}

///
/// # Errors
///
/// Returns [`NexusApiError`] when the creator/workspace guard rejects the
/// request, the core authority denies it (ownership, admission or validation),
/// or the bounded store read/write fails.
/// `POST /v1/daemon/creators` — create a new local creator profile (V1.129 P0).
///
/// Generates a `ctr_local…` id (matching the `CreatorId` pattern), validates the
/// `display_name`, materializes the membership-SSOT Profile home, INSERTs the
/// workspace row, and returns a `CreatorDetail`-shaped 201 response.
///
/// Tier-1 (API key) only — do not gate on active creator (architect lock #2).
/// The handler reuses the canonical `NexusApiError` envelope for all failures.
pub async fn create_creator(
    State(state): State<WorkspaceState>,
    Json(req): Json<CreateCreatorRequest>,
) -> Result<(StatusCode, Json<CreatorDetail>), NexusApiError> {
    info!("Handling create creator request");

    let home = home_service(&state)?;
    let detail = home
        .create_creator(req.display_name)
        .await
        .map_err(NexusApiError::from)?;

    debug!(creator_id = %detail.creator_id, "Creator created");
    info!(creator_id = %detail.creator_id, "Create creator completed");
    Ok((StatusCode::CREATED, Json(creator_detail_from_core(detail))))
}

///
/// # Errors
///
/// Returns [`NexusApiError`] when the creator/workspace guard rejects the
/// request, the core authority denies it (ownership, admission or validation),
/// or the bounded store read/write fails.
/// `GET /v1/daemon/creators/{creator_id}` — creator status/detail
pub async fn get_creator(
    State(state): State<WorkspaceState>,
    Path(creator_id): Path<String>,
) -> Result<Json<CreatorDetail>, NexusApiError> {
    info!(creator_id = %creator_id, "Getting creator detail");

    let home = home_service(&state)?;
    let detail = home
        .creator_detail(&creator_id)
        .map_err(NexusApiError::from)?;
    Ok(Json(creator_detail_from_core(detail)))
}

///
/// # Errors
///
/// Returns [`NexusApiError`] when the creator/workspace guard rejects the
/// request, the core authority denies it (ownership, admission or validation),
/// or the bounded store read/write fails.
/// `PATCH /v1/daemon/creators/{creator_id}` — update creator display name.
///
/// Updates the creator's display name in the identity cache and the workspace
/// `creators` SQL row (display-name members only; the materialization and
/// cache-parse discipline live in the core family).
pub async fn patch_creator(
    State(state): State<WorkspaceState>,
    Path(creator_id): Path<String>,
    Json(req): Json<PatchCreatorRequest>,
) -> Result<Json<CreatorDetail>, NexusApiError> {
    info!(creator_id = %creator_id, "Patching creator");

    let home = home_service(&state)?;
    let detail = home
        .patch_creator(&creator_id, req.display_name)
        .await
        .map_err(NexusApiError::from)?;
    Ok(Json(creator_detail_from_core(detail)))
}

///
/// # Errors
///
/// Returns [`NexusApiError`] when the creator/workspace guard rejects the
/// request, the core authority denies it (ownership, admission or validation),
/// or the bounded store read/write fails.
/// `PUT /v1/daemon/creators/active` — set active creator
pub async fn set_active_creator(
    State(state): State<WorkspaceState>,
    Json(req): Json<SetActiveCreatorRequest>,
) -> Result<Json<SetActiveCreatorResponse>, NexusApiError> {
    info!(creator_id = %req.creator_id, "Setting active creator");

    let home = home_service(&state)?;
    let response = home
        .use_creator(CoreSetActiveCreatorRequest {
            creator_id: req.creator_id.clone(),
        })
        .map_err(NexusApiError::from)?;

    // Daemon-process side effect (H1): the shared creator_db slot attaches
    // eagerly on selection so later Tier-1 handlers never see a missing pool.
    state
        .ensure_creator_pool()
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: e.to_string(),
        })?;

    Ok(Json(SetActiveCreatorResponse {
        creator_id: response.creator_id,
    }))
}

///
/// # Errors
///
/// Returns [`NexusApiError`] when the creator/workspace guard rejects the
/// request, the core authority denies it (ownership, admission or validation),
/// or the bounded store read/write fails.
/// `GET /v1/daemon/creators/active` — get active creator
pub async fn get_active_creator(
    State(state): State<WorkspaceState>,
) -> Result<Json<ActiveCreatorResponse>, NexusApiError> {
    let home = home_service(&state)?;
    let active = home.active_creator().map_err(NexusApiError::from)?;
    Ok(Json(ActiveCreatorResponse {
        creator_id: active.creator_id,
        handle: active.handle,
        display_name: active.display_name,
    }))
}

///
/// # Errors
///
/// Returns [`NexusApiError`] when the creator/workspace guard rejects the
/// request, the core authority denies it (ownership, admission or validation),
/// or the bounded store read/write fails.
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

    let home = home_service(&state)?;
    let response = home
        .logout_creator(&creator_id)
        .map_err(NexusApiError::from)?;
    Ok(Json(LogoutResponse {
        creator_id: response.creator_id,
        cleared: response.cleared,
    }))
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_workspace;
    use serial_test::serial;

    /// Temporarily override `HOME` so disk-backed state initialization
    /// (`WorkspaceState::initialize`) operates inside the isolated test
    /// directory. The original value is restored on drop.
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
    async fn list_includes_filesystem_profiles_when_sql_is_empty() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let user_home = tmp.path();
        let nexus_home = user_home.join(".nexus42");
        nexus_home_layout::ensure_system_layout(&nexus_home).expect("system layout");

        let creator_a = "ctr_localaaaaaaaa";
        let creator_b = "ctr_localbbbbbbbb";
        std::fs::create_dir_all(nexus_home.join("creators").join(creator_a)).expect("mkdir a");
        std::fs::create_dir_all(nexus_home.join("creators").join(creator_b)).expect("mkdir b");
        std::fs::write(
            nexus_home.join("config.toml"),
            format!("active_creator_id = \"{creator_a}\"\n"),
        )
        .expect("write config");

        let _home_override = HomeOverride::set(user_home);

        let state = crate::workspace::WorkspaceState::initialize()
            .await
            .expect("initialize");

        let body = list(
            State(state),
            Query(ListCreatorsQuery {
                limit: 50,
                cursor: None,
            }),
        )
        .await
        .expect("list should succeed")
        .0;

        let ids: Vec<&str> = body.items.iter().map(|c| c.creator_id.as_str()).collect();
        assert!(
            ids.contains(&creator_a) && ids.contains(&creator_b),
            "SSOT Profile homes must appear even when SQL creators is empty, got {ids:?}"
        );
    }

    #[tokio::test]
    #[serial]
    async fn list_ignores_sql_only_orphan_profiles() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let user_home = tmp.path();
        let nexus_home = user_home.join(".nexus42");
        nexus_home_layout::ensure_system_layout(&nexus_home).expect("system layout");

        let on_disk = "ctr_localondisk0001";
        let orphan_sql = "ctr_localorphan0001";
        std::fs::create_dir_all(
            nexus_home
                .join("creators")
                .join(on_disk)
                .join("workspaces")
                .join("default"),
        )
        .expect("mkdir");
        std::fs::write(
            nexus_home.join("config.toml"),
            format!(
                "active_creator_id = \"{on_disk}\"\n\
                 [active_workspace_slug_by_creator]\n\
                 \"{on_disk}\" = \"default\"\n"
            ),
        )
        .expect("write config");

        let _home_override = HomeOverride::set(user_home);
        let state = crate::workspace::WorkspaceState::initialize()
            .await
            .expect("initialize");

        // Seed an orphan SQL row that has no Profile home (dirty secondary data).
        let pool = state
            .pool()
            .expect("pool should open for on-disk active profile");
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query!(
            "INSERT INTO creators (creator_id, display_name, status, cached_at, data) VALUES (?, ?, 'active', ?, '{}')",
            orphan_sql,
            "Orphan",
            now
        )
        .execute(pool)
        .await
        .expect("insert orphan");

        let body = list(
            State(state),
            Query(ListCreatorsQuery {
                limit: 50,
                cursor: None,
            }),
        )
        .await
        .expect("list should succeed")
        .0;

        let ids: Vec<&str> = body.items.iter().map(|c| c.creator_id.as_str()).collect();
        assert_eq!(
            ids,
            vec![on_disk],
            "list membership is SSOT-only, got {ids:?}"
        );
        assert!(!ids.contains(&orphan_sql));
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

        let mut state = crate::workspace::WorkspaceState::initialize()
            .await
            .expect("initialize");
        // `set_active_creator` attaches the Profile runtime, which composes
        // over the Host facade the daemon wires at boot.
        crate::test_utils::wire_test_agent_host(&mut state);
        assert!(state.pool().is_none());

        // The handler's `Json` response is #[must_use]; the test only checks
        // the side effect (pool opened), so discard the response value.
        let _ = set_active_creator(
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
            display_name: Some(String::new()),
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
            .expect("creator should be present after PATCH materializes SSOT home");
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
