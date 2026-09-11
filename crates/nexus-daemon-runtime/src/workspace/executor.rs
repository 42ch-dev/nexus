//! Daemon-owned `WorkspaceExecutor` (v1.188 P3).

use std::sync::Arc;

use async_trait::async_trait;
use nexus_contracts::local::orchestration::{
    WorkspaceCommitInput, WorkspaceCommitOutput, WorkspaceOpenInput, WorkspaceOpenOutput,
    WorkspaceOpenSnapshot,
};
use nexus_home_layout;
use nexus_orchestration::capability::{CapabilityError, WorkspaceExecutor};

use super::session::{SessionError, SessionId, WorkspaceSessionManager};

/// Production workspace executor over the daemon session manager.
pub struct DaemonWorkspaceExecutor {
    session_manager: Arc<WorkspaceSessionManager>,
    workspace_root: Arc<std::sync::Mutex<Option<String>>>,
}

impl DaemonWorkspaceExecutor {
    #[must_use]
    pub fn new(
        session_manager: Arc<WorkspaceSessionManager>,
        workspace_root: Arc<std::sync::Mutex<Option<String>>>,
    ) -> Self {
        Self {
            session_manager,
            workspace_root,
        }
    }

    fn workspace_root(&self) -> Result<String, CapabilityError> {
        self.workspace_root
            .lock()
            .map_err(|e| CapabilityError::Internal(format!("workspace root lock: {e}")))?
            .clone()
            .ok_or(CapabilityError::WorkerUnavailable)
    }
}

fn map_session_error(err: SessionError) -> CapabilityError {
    match err {
        SessionError::NotFound(id) => CapabilityError::PermanentExternal(format!("session not found: {id}")),
        SessionError::AlreadyCommitted(id) => {
            CapabilityError::PermanentExternal(format!("stale session: {id}"))
        }
        SessionError::Expired(id) => CapabilityError::PermanentExternal(format!("session expired: {id}")),
        SessionError::HashConflict { path, expected_hash, actual_hash, .. } => {
            CapabilityError::PermanentExternal(format!(
                "hash conflict for {path}: expected {expected_hash}, got {actual_hash}"
            ))
        }
        SessionError::ManifestInvalid(msg) => CapabilityError::InputInvalid(msg),
        SessionError::RecoveryConflict(root) => {
            CapabilityError::PermanentExternal(format!("recovery conflict: {root}"))
        }
        SessionError::PathEscape { path, workspace_root } => {
            CapabilityError::InputInvalid(format!("path escape: {path} outside {workspace_root}"))
        }
        SessionError::Database(msg) | SessionError::Io(msg) | SessionError::Internal(msg) => {
            CapabilityError::Internal(msg)
        }
    }
}

#[async_trait]
impl WorkspaceExecutor for DaemonWorkspaceExecutor {
    async fn open(&self, input: WorkspaceOpenInput) -> Result<WorkspaceOpenOutput, CapabilityError> {
        nexus_home_layout::validate_workspace_path_safe(&input.path)
            .map_err(|reason| CapabilityError::InputInvalid(reason))?;
        let workspace_root = self.workspace_root()?;
        let target_path = std::path::PathBuf::from(&workspace_root).join(&input.path);
        let existed = target_path.exists();
        let session_id = self
            .session_manager
            .open_session(&workspace_root, &input.path, existed)
            .await
            .map_err(map_session_error)?;
        let row = self
            .session_manager
            .validate_session(&session_id)
            .await
            .map_err(map_session_error)?;
        let file_hashes: std::collections::HashMap<String, String> =
            serde_json::from_str(&row.file_hashes_json).unwrap_or_default();
        Ok(WorkspaceOpenOutput {
            session_id: session_id.to_string(),
            snapshot: WorkspaceOpenSnapshot {
                workspace_root,
                path: input.path,
                existed,
                file_hashes,
            },
        })
    }

    async fn commit(
        &self,
        input: WorkspaceCommitInput,
    ) -> Result<WorkspaceCommitOutput, CapabilityError> {
        if input.session_id.trim().is_empty() {
            return Err(CapabilityError::InputInvalid(
                "session_id must not be empty".into(),
            ));
        }
        let workspace_root = self.workspace_root()?;
        let session_id = SessionId(input.session_id.clone());
        let outcome = self
            .session_manager
            .commit_session_durable(&session_id, &input.changes, &workspace_root)
            .await
            .map_err(map_session_error)?;
        Ok(WorkspaceCommitOutput {
            revision: outcome.revision,
            committed: outcome.committed,
        })
    }
}
