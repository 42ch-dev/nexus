//! Daemon-owned `WorkspaceExecutor` (v1.188 P3 L2).

use std::sync::Arc;

use async_trait::async_trait;
use nexus_contracts::local::orchestration::{
    WorkspaceCommitInput, WorkspaceCommitOutput, WorkspaceOpenInput, WorkspaceOpenOutput,
    WorkspaceOpenSnapshot,
};
use nexus_home_layout;
use nexus_orchestration::capability::{CapabilityError, WorkspaceExecutor};

use super::session::{SessionError, SessionId, WorkspaceSessionManager};

/// Production workspace executor bound to one canonical workspace root.
pub struct DaemonWorkspaceExecutor {
    session_manager: Arc<WorkspaceSessionManager>,
    canonical_workspace_root: String,
}

impl DaemonWorkspaceExecutor {
    #[must_use]
    pub fn new(
        session_manager: Arc<WorkspaceSessionManager>,
        canonical_workspace_root: String,
    ) -> Self {
        Self {
            session_manager,
            canonical_workspace_root,
        }
    }
}

fn map_session_error(err: SessionError) -> CapabilityError {
    match err {
        SessionError::NotFound(_) => CapabilityError::PermanentExternal("session not found".into()),
        SessionError::AlreadyCommitted(_) => {
            CapabilityError::PermanentExternal("stale session".into())
        }
        SessionError::Expired(_) => CapabilityError::PermanentExternal("session expired".into()),
        SessionError::HashConflict { .. } => {
            CapabilityError::PermanentExternal("content hash conflict".into())
        }
        SessionError::CorruptSnapshot(_) => {
            CapabilityError::PermanentExternal("corrupt workspace snapshot".into())
        }
        SessionError::ManifestInvalid(msg) => CapabilityError::InputInvalid(msg),
        SessionError::RecoveryConflict(_) => {
            CapabilityError::PermanentExternal("workspace recovery conflict".into())
        }
        SessionError::PathEscape { .. } => CapabilityError::InputInvalid("path not allowed".into()),
        SessionError::Database(_) | SessionError::Io(_) | SessionError::Internal(_) => {
            CapabilityError::Internal("workspace storage error".into())
        }
    }
}

#[async_trait]
impl WorkspaceExecutor for DaemonWorkspaceExecutor {
    async fn open(
        &self,
        input: WorkspaceOpenInput,
    ) -> Result<WorkspaceOpenOutput, CapabilityError> {
        nexus_home_layout::validate_workspace_path_safe(&input.path)
            .map_err(|reason| CapabilityError::InputInvalid(reason))?;
        let target_path =
            std::path::PathBuf::from(&self.canonical_workspace_root).join(&input.path);
        let existed = target_path.exists();
        let session_id = self
            .session_manager
            .open_session(&self.canonical_workspace_root, &input.path, existed)
            .await
            .map_err(map_session_error)?;
        let row = self
            .session_manager
            .validate_session(&session_id)
            .await
            .map_err(map_session_error)?;
        let file_hashes = super::session::parse_snapshot_hashes(&row.file_hashes_json)
            .map_err(map_session_error)?;
        Ok(WorkspaceOpenOutput {
            session_id: session_id.to_string(),
            snapshot: WorkspaceOpenSnapshot {
                workspace_root: self.canonical_workspace_root.clone(),
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
        let session_id = SessionId(input.session_id.clone());
        let outcome = WorkspaceSessionManager::commit_session_durable_owned(
            Arc::clone(&self.session_manager),
            session_id,
            input.changes,
        )
        .await
        .map_err(map_session_error)?;
        Ok(WorkspaceCommitOutput {
            revision: outcome.revision,
            committed: outcome.committed,
        })
    }
}
