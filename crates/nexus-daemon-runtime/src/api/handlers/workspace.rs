//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc)]
//! Workspace handlers

use crate::api::errors::NexusApiError;
use crate::workspace::session::{SessionError, WorkspaceSessionManager};
use crate::workspace::WorkspaceState;
use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

#[derive(Serialize)]
pub struct WorkspaceInfo {
    pub initialized: bool,
    pub workspace_path: Option<String>,
    pub database_path: String,
}

/// GET /v1/daemon/workspace
pub async fn info(State(state): State<WorkspaceState>) -> Json<WorkspaceInfo> {
    info!("Handling workspace info request");
    Json(WorkspaceInfo {
        initialized: state.is_initialized(),
        workspace_path: state.workspace_path(),
        database_path: state.database_path().unwrap_or_default(),
    })
}

#[derive(Deserialize)]
pub struct InitWorkspaceRequest {
    pub path: String,
}

#[derive(Serialize)]
pub struct InitWorkspaceResponse {
    pub success: bool,
    pub message: String,
}

/// POST /v1/daemon/workspace/init
pub async fn init_workspace(
    State(state): State<WorkspaceState>,
    Json(req): Json<InitWorkspaceRequest>,
) -> Result<Json<InitWorkspaceResponse>, NexusApiError> {
    info!("Handling workspace init request");
    debug!(path = %req.path, "Initializing workspace");

    // Validate input
    if req.path.trim().is_empty() {
        return Err(NexusApiError::InvalidInput {
            field: "path".into(),
            reason: "must not be empty".into(),
        });
    }

    state
        .init_workspace(&req.path)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "WORKSPACE_INIT_FAILED".into(),
            message: e.to_string(),
        })?;

    info!("Workspace init completed");
    Ok(Json(InitWorkspaceResponse {
        success: true,
        message: format!("Workspace initialized at {}", req.path),
    }))
}

// ── Workspace open / commit handlers (DF-31 skeleton) ──────────────────────

/// Request body for `POST /v1/daemon/workspace/open`.
///
/// Opens a workspace path and returns a session with a snapshot.
///
/// # Future expansion (DF-31 → DF-42)
///
/// The current skeleton accepts only a relative `path`. Future iterations
/// may add `scope` (file-level vs directory-level), `mode` (read-only vs
/// read-write), and `include_patterns` for filtered snapshots.
#[derive(Debug, Deserialize)]
pub struct WorkspaceOpenRequest {
    /// Relative path within the workspace creative root (e.g., "Works/my-novel").
    pub path: String,
}

/// Response for `workspace.open`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceOpenResponse {
    /// Unique session identifier for this open operation.
    pub session_id: String,
    /// Snapshot of the workspace state at open time.
    pub snapshot: OpenSnapshot,
}

/// Workspace state snapshot returned by `workspace.open`.
///
/// Contains file hashes for optimistic concurrency control (OCC).
/// Each file tracked in the workspace scope is represented by its
/// relative path and SHA-256 content hash.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSnapshot {
    /// Absolute path to the workspace creative root.
    pub workspace_root: String,
    /// Relative path that was opened.
    pub path: String,
    /// Whether the target path already existed on disk.
    pub existed: bool,
    /// Map of relative path → SHA-256 hex digest for all tracked files.
    /// Empty if the path did not exist or contained no regular files.
    #[serde(default)]
    pub file_hashes: std::collections::HashMap<String, String>,
}

/// `POST /v1/daemon/workspace/open`
///
/// Opens a workspace session for the given relative path. Validates that
/// the path is safe (no traversal, no absolute paths) and that the workspace
/// is initialized. Returns a session ID and a snapshot of the workspace state.
///
/// # Errors
///
/// Returns:
/// - 400 if the path is invalid (empty, absolute, contains traversal)
/// - 409 if the workspace is not initialized
/// - 500 on internal errors
pub async fn open_workspace(
    State(state): State<WorkspaceState>,
    Json(req): Json<WorkspaceOpenRequest>,
) -> Result<Json<WorkspaceOpenResponse>, NexusApiError> {
    info!("Handling workspace.open request");
    debug!(path = %req.path, "Opening workspace path");

    // Validate path safety (no traversal, no absolute paths, no control chars)
    nexus_home_layout::validate_workspace_path_safe(&req.path).map_err(|reason| {
        NexusApiError::InvalidInput {
            field: "path".into(),
            reason,
        }
    })?;

    // Ensure workspace is initialized
    let workspace_root = state.workspace_path().ok_or(NexusApiError::Uninitialized)?;
    debug!(workspace_root = %workspace_root, "Workspace root resolved");

    // Check if the target path exists on disk
    let target_path = std::path::PathBuf::from(&workspace_root).join(&req.path);
    let existed = target_path.exists();
    debug!(?target_path, existed, "Resolved workspace target path");

    // Open session (async, DB-backed with content hashes)
    let session_mgr = state
        .session_manager()
        .ok_or(NexusApiError::Uninitialized)?;
    let session_id = session_mgr
        .open_session(&workspace_root, &req.path, existed)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "SESSION_OPEN_FAILED".into(),
            message: e.to_string(),
        })?;

    // Read back the session to get file hashes
    let session_row = session_mgr
        .validate_session(&session_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "SESSION_READ_FAILED".into(),
            message: e.to_string(),
        })?;

    // Parse file hashes from stored JSON
    let file_hashes: std::collections::HashMap<String, String> =
        serde_json::from_str(&session_row.file_hashes_json).unwrap_or_default();

    info!(
        session_id = %session_id,
        file_count = file_hashes.len(),
        "Workspace session opened"
    );

    Ok(Json(WorkspaceOpenResponse {
        session_id: session_id.to_string(),
        snapshot: OpenSnapshot {
            workspace_root,
            path: req.path,
            existed,
            file_hashes,
        },
    }))
}

/// Request body for `POST /v1/daemon/workspace/commit`.
///
/// Commits changes against a workspace session. The `changes[]` manifest
/// is validated against the session snapshot for OCC conflicts.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCommitRequest {
    /// The session ID returned by `workspace.open`.
    pub session_id: String,
    /// Manifest of changes to commit, each with path, content hash, and operation.
    #[serde(default)]
    pub changes: Vec<nexus_contracts::local::orchestration::WorkspaceChangeEntry>,
}

/// Response for `workspace.commit`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCommitResponse {
    /// Revision identifier for this commit.
    pub revision: String,
    /// Whether the commit was accepted (always true for successful commits).
    pub committed: bool,
}

/// `POST /v1/daemon/workspace/commit`
///
/// Commits changes against a workspace session. Validates the `changes[]` manifest
/// against the session snapshot. Rejects on hash mismatch (OCC conflict), stale
/// sessions, expired sessions, or missing sessions.
///
/// # Error semantics (conflict model)
///
/// - **OCC conflict**: If any change entry's `content_hash` does not match the
///   session snapshot, returns 409 CONFLICT with `HASH_CONFLICT`.
/// - **Stale session**: If the session has already been committed, returns
///   409 CONFLICT with `STALE_SESSION`.
/// - **Expired session**: If the session has exceeded its TTL, returns
///   409 CONFLICT with `SESSION_EXPIRED`.
/// - **Missing session**: If the session ID is not found, returns
///   404 `NOT_FOUND` with `SESSION_NOT_FOUND`.
///
/// # Errors
///
/// Returns:
/// - 400 if `session_id` is empty
/// - 404 if the session is not found
/// - 409 if the session is stale, expired, or has OCC conflicts
/// - 500 on internal errors
pub async fn commit_workspace(
    State(state): State<WorkspaceState>,
    Json(req): Json<WorkspaceCommitRequest>,
) -> Result<Json<WorkspaceCommitResponse>, NexusApiError> {
    info!("Handling workspace.commit request");
    debug!(
        session_id = %req.session_id,
        change_count = req.changes.len(),
        "Committing workspace session"
    );

    // Validate session_id is not empty
    if req.session_id.trim().is_empty() {
        return Err(NexusApiError::InvalidInput {
            field: "session_id".into(),
            reason: "must not be empty".into(),
        });
    }

    let session_mgr = state
        .session_manager()
        .ok_or(NexusApiError::Uninitialized)?;
    let session_id = crate::workspace::session::SessionId(req.session_id.clone());

    // The workspace must be initialized; the owner resolves the root itself
    // from the session row, so only presence is asserted here.
    if state.workspace_path().is_none() {
        return Err(NexusApiError::Uninitialized);
    }

    // V1.58 P0 T5 (R-V156P0-M005): validate + consume in one atomic step.
    // The previous two-call sequence (validate_changes_manifest then
    // consume_session) left a TOCTOU window; the durable commit closes it by
    // binding the two into a single transaction-guarded operation.
    //
    // v1.188 P3: the PUBLIC request path uses the retained-owner entry point.
    // The commit runs on a separately spawned owner that holds the workspace
    // authority; if this request future is dropped (client disconnect, server
    // shutdown), the already-admitted commit still runs to a durable
    // conclusion instead of being abandoned mid-apply. The response is still
    // the owner's real outcome.
    match commit_workspace_owned(session_mgr, session_id.clone(), req.changes.clone()).await {
        Ok(outcome) => {
            info!(
                session_id = %session_id,
                revision = %outcome.revision,
                change_count = req.changes.len(),
                "Workspace commit accepted"
            );
            Ok(Json(WorkspaceCommitResponse {
                revision: outcome.revision,
                committed: outcome.committed,
            }))
        }
        Err(SessionError::HashConflict {
            path,
            expected_hash,
            actual_hash,
            ..
        }) => {
            debug!(
                session_id = %session_id,
                %path,
                %expected_hash,
                %actual_hash,
                "OCC hash conflict"
            );
            Err(NexusApiError::Conflict(format!(
                "content hash conflict for {path}: expected {expected_hash}, got {actual_hash}"
            )))
        }
        Err(e) => Err(map_session_error(&session_id, e)),
    }
}

/// Run a durable commit on the retained owner and await its outcome.
///
/// The owner outlives this future, so request cancellation cannot abandon an
/// admitted commit.
async fn commit_workspace_owned(
    session_mgr: std::sync::Arc<WorkspaceSessionManager>,
    session_id: crate::workspace::session::SessionId,
    changes: Vec<nexus_contracts::local::orchestration::WorkspaceChangeEntry>,
) -> Result<crate::workspace::session_commit::CommitOutcome, SessionError> {
    WorkspaceSessionManager::commit_session_durable_owned(session_mgr, session_id, changes).await
}

/// Map a [`SessionError`] to the appropriate [`NexusApiError`] variant.
fn map_session_error(
    session_id: &crate::workspace::session::SessionId,
    err: SessionError,
) -> NexusApiError {
    match err {
        SessionError::NotFound(_) => {
            debug!(session_id = %session_id, "Session not found");
            NexusApiError::NotFound(format!("session {session_id} not found"))
        }
        SessionError::AlreadyCommitted(_) => {
            debug!(session_id = %session_id, "Stale session");
            NexusApiError::Conflict(format!("session {session_id} is stale (already committed)"))
        }
        SessionError::Expired(_) => {
            debug!(session_id = %session_id, "Session expired");
            NexusApiError::Conflict(format!("session {session_id} has expired"))
        }
        SessionError::HashConflict {
            path,
            expected_hash,
            actual_hash,
            ..
        } => NexusApiError::Conflict(format!(
            "content hash conflict for {path}: expected {expected_hash}, got {actual_hash}"
        )),
        SessionError::Database(msg) | SessionError::Io(msg) => NexusApiError::Internal {
            code: "SESSION_ERROR".into(),
            message: msg,
        },
        SessionError::CorruptSnapshot(reason) => {
            NexusApiError::Conflict(format!("corrupt workspace snapshot: {reason}"))
        }
        SessionError::ManifestInvalid(reason) => NexusApiError::InvalidInput {
            field: "changes".into(),
            reason,
        },
        SessionError::RecoveryConflict(root) => {
            NexusApiError::Conflict(format!("workspace recovery conflict: {root}"))
        }
        SessionError::Internal(msg) => NexusApiError::Internal {
            code: "SESSION_INTERNAL".into(),
            message: msg,
        },
        SessionError::PathEscape {
            path,
            workspace_root,
        } => NexusApiError::InvalidInput {
            field: "path".into(),
            reason: format!("path '{path}' escapes canonical workspace root '{workspace_root}'"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::errors::NexusApiError;
    use crate::test_utils::{create_test_workspace, TestTempRoot};
    use crate::workspace::WorkspaceState;
    use axum::extract::State as AxumState;

    /// Helper: create a workspace state that is initialized.
    /// Returns the `TestTempRoot` guard which must be kept alive.
    async fn make_state() -> (TestTempRoot, WorkspaceState) {
        let (tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        // Mark workspace as initialized so open_workspace doesn't return Uninitialized
        state
            .init_workspace("/tmp/test-workspace")
            .await
            .expect("init_workspace should succeed");
        (tmp, state)
    }

    /// A `create` change carrying `content` (base64 as the DTO requires).
    fn create_entry(
        path: &str,
        content: &[u8],
    ) -> nexus_contracts::local::orchestration::WorkspaceChangeEntry {
        use base64::Engine;
        nexus_contracts::local::orchestration::WorkspaceChangeEntry {
            path: path.to_string(),
            op: nexus_contracts::local::orchestration::WorkspaceChangeOp::Create,
            expected_hash: None,
            content_base64: Some(base64::engine::general_purpose::STANDARD.encode(content)),
        }
    }

    /// The commit path requires the scope directory to exist (missing parents
    /// are rejected), so materialize a dedicated scope before opening. A
    /// dedicated scope keeps this fixture from perturbing sibling tests that
    /// assert on `Works/my-novel` not existing.
    const COMMIT_SCOPE: &str = "Works/commit-scope";

    fn ensure_scope_dir() {
        std::fs::create_dir_all(format!("/tmp/test-workspace/{COMMIT_SCOPE}")).expect("scope dir");
    }

    fn committed_file(name: &str) -> std::path::PathBuf {
        std::path::Path::new("/tmp/test-workspace")
            .join(COMMIT_SCOPE)
            .join(name)
    }

    // ── Path bounds tests ─────────────────────────────────────────────

    #[tokio::test]
    async fn open_workspace_rejects_absolute_path() {
        let (_tmp, state) = make_state().await;
        let result = open_workspace(
            AxumState(state),
            Json(WorkspaceOpenRequest {
                path: "/etc/passwd".to_string(),
            }),
        )
        .await;
        match result {
            Err(NexusApiError::InvalidInput { field, reason }) => {
                assert_eq!(field, "path");
                assert!(reason.contains("absolute path"));
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn open_workspace_rejects_traversal_path() {
        let (_tmp, state) = make_state().await;
        let result = open_workspace(
            AxumState(state),
            Json(WorkspaceOpenRequest {
                path: "../../etc/passwd".to_string(),
            }),
        )
        .await;
        match result {
            Err(NexusApiError::InvalidInput { field, reason }) => {
                assert_eq!(field, "path");
                assert!(reason.contains("'..'"));
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn open_workspace_rejects_empty_path() {
        let (_tmp, state) = make_state().await;
        let result = open_workspace(
            AxumState(state),
            Json(WorkspaceOpenRequest {
                path: String::new(),
            }),
        )
        .await;
        match result {
            Err(NexusApiError::InvalidInput { field, reason }) => {
                assert_eq!(field, "path");
                assert!(reason.contains("must not be empty"));
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    // ── Session lifecycle tests ───────────────────────────────────────

    #[tokio::test]
    async fn open_workspace_returns_session_id_and_snapshot() {
        let (_tmp, state) = make_state().await;
        let result = open_workspace(
            AxumState(state),
            Json(WorkspaceOpenRequest {
                path: "Works/my-novel".to_string(),
            }),
        )
        .await
        .expect("open_workspace should succeed");
        assert!(
            result.session_id.starts_with("ws_"),
            "session_id should start with ws_"
        );
        assert_eq!(result.snapshot.path, "Works/my-novel");
        assert_eq!(result.snapshot.workspace_root, "/tmp/test-workspace");
        // The path likely doesn't exist in test, so existed should be false
        assert!(!result.snapshot.existed);
    }

    #[tokio::test]
    async fn open_and_commit_full_session_lifecycle() {
        let (_tmp, state) = make_state().await;
        ensure_scope_dir();
        // Open
        let open_result = open_workspace(
            AxumState(state.clone()),
            Json(WorkspaceOpenRequest {
                path: COMMIT_SCOPE.to_string(),
            }),
        )
        .await
        .expect("open_workspace should succeed");
        let session_id = open_result.session_id.clone();

        // Commit a real change: an empty manifest is invalid input (v1.188 P3
        // requires at least one change — a no-op commit must not consume a
        // session).
        let file = "lifecycle.txt";
        let result = commit_workspace(
            AxumState(state),
            Json(WorkspaceCommitRequest {
                session_id: session_id.clone(),
                changes: vec![create_entry(file, b"lifecycle")],
            }),
        )
        .await
        .expect("commit_workspace should succeed");
        assert!(result.committed);
        assert!(result.revision.starts_with("rev_"));
        let written = committed_file(file);
        assert_eq!(
            std::fs::read(&written).expect("committed file"),
            b"lifecycle"
        );
        let _ = std::fs::remove_file(&written);
    }

    // ── Commit conflict rejection tests ───────────────────────────────

    /// v1.188 P3: an aborted HTTP commit request must not abandon the commit.
    ///
    /// The handler awaits the retained owner, so cancelling the request future
    /// (client disconnect) leaves the admitted commit running to a durable
    /// conclusion.
    #[tokio::test]
    async fn aborted_request_completes_commit_through_owner() {
        let (_tmp, state) = make_state().await;
        ensure_scope_dir();
        // The commit scope is a fixed fixture path shared with sibling tests
        // and with previous runs, so clear this test's target before opening:
        // a stale file would make the `create` an OCC conflict.
        let _ = std::fs::remove_file(committed_file("aborted.txt"));
        let open_result = open_workspace(
            AxumState(state.clone()),
            Json(WorkspaceOpenRequest {
                path: COMMIT_SCOPE.to_string(),
            }),
        )
        .await
        .expect("open_workspace should succeed");
        let session_id = open_result.session_id.clone();

        // Park the retained owner at its admission boundary so the abort lands
        // AFTER the commit was admitted — the worst case for abandonment.
        let gate = std::sync::Arc::new(crate::workspace::test_hooks::OwnerGate::for_session(
            session_id.clone(),
        ));
        crate::workspace::test_hooks::set_owner_gate(Some(std::sync::Arc::clone(&gate)));

        let state2 = state.clone();
        let session_for_count = session_id.clone();
        let request = tokio::spawn(async move {
            commit_workspace(
                AxumState(state2),
                Json(WorkspaceCommitRequest {
                    session_id,
                    changes: vec![create_entry("aborted.txt", b"survives-abort")],
                }),
            )
            .await
        });

        // Owner admitted -> simulate the client going away. Bounded so a
        // pre-admission failure surfaces the real error instead of hanging.
        if tokio::time::timeout(std::time::Duration::from_secs(10), gate.admitted.notified())
            .await
            .is_err()
        {
            crate::workspace::test_hooks::set_owner_gate(None);
            let outcome = request.await.expect("join");
            panic!("owner never reached admission; handler returned: {outcome:?}");
        }
        request.abort();
        // Release the owner; it settles on its own.
        gate.proceed.notify_one();
        tokio::time::timeout(std::time::Duration::from_secs(30), gate.settled.notified())
            .await
            .expect("owner must settle after cancellation");
        crate::workspace::test_hooks::set_owner_gate(None);

        // The commit reached the durable committed transition and the bytes are
        // on disk, despite the aborted request.
        let written = committed_file("aborted.txt");
        assert_eq!(
            std::fs::read(&written).expect("committed file after abort"),
            b"survives-abort"
        );
        let mgr = state.session_manager().expect("session manager");
        // Scoped to THIS session: sibling tests share the fixture workspace DB.
        let committed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM workspace_commit_intents WHERE session_id = ? AND state = 'committed'",
        )
        .bind(&session_for_count)
        .fetch_one(mgr.pool().as_ref())
        .await
        .expect("intent count");
        assert_eq!(committed, 1);
        let _ = std::fs::remove_file(&written);
    }

    #[tokio::test]
    async fn commit_rejects_stale_session() {
        let (_tmp, state) = make_state().await;
        ensure_scope_dir();
        let open_result = open_workspace(
            AxumState(state.clone()),
            Json(WorkspaceOpenRequest {
                path: COMMIT_SCOPE.to_string(),
            }),
        )
        .await
        .expect("open_workspace should succeed");
        let session_id = open_result.session_id.clone();

        // First commit succeeds
        let _ = commit_workspace(
            AxumState(state.clone()),
            Json(WorkspaceCommitRequest {
                session_id: session_id.clone(),
                changes: vec![create_entry("stale.txt", b"first")],
            }),
        )
        .await
        .expect("first commit should succeed");

        // Second commit with same session should be rejected as stale
        let result = commit_workspace(
            AxumState(state),
            Json(WorkspaceCommitRequest {
                session_id: session_id.clone(),
                changes: vec![create_entry("stale.txt", b"second")],
            }),
        )
        .await;
        let _ = std::fs::remove_file(committed_file("stale.txt"));
        match result {
            Err(NexusApiError::Conflict(msg)) => {
                assert!(
                    msg.contains("stale"),
                    "expected stale session error, got: {msg}"
                );
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn commit_rejects_missing_session() {
        let (_tmp, state) = make_state().await;
        let result = commit_workspace(
            AxumState(state),
            Json(WorkspaceCommitRequest {
                session_id: "ws_nonexistent".to_string(),
                changes: vec![],
            }),
        )
        .await;
        match result {
            Err(NexusApiError::NotFound(msg)) => {
                assert!(msg.contains("not found"));
            }
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn commit_rejects_empty_session_id() {
        let (_tmp, state) = make_state().await;
        let result = commit_workspace(
            AxumState(state),
            Json(WorkspaceCommitRequest {
                session_id: String::new(),
                changes: vec![],
            }),
        )
        .await;
        match result {
            Err(NexusApiError::InvalidInput { field, .. }) => {
                assert_eq!(field, "session_id");
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    // ── Missing workspace test ────────────────────────────────────────

    #[tokio::test]
    async fn open_workspace_fails_when_not_initialized() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        // Don't call init_workspace — workspace is not initialized

        let result = open_workspace(
            AxumState(state),
            Json(WorkspaceOpenRequest {
                path: "Works/my-novel".to_string(),
            }),
        )
        .await;
        match result {
            Err(NexusApiError::Uninitialized) => {}
            other => panic!("expected Uninitialized, got {other:?}"),
        }
    }
}
