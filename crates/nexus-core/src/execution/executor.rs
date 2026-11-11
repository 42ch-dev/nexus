//! `WorkspaceExecutor` over the core-owned workspace commit authority
//! (v1.190 P3-T2; moved from the daemon's `workspace::executor`).
//!
//! Binds the orchestration `WorkspaceExecutor` capability surface to the SAME
//! shared [`super::session::WorkspaceSessionManager`] the durable commit path
//! writes through, so a preset's `workspace.commit` observes real durable
//! workspace state.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use nexus_contracts::local::orchestration::{
    WorkArtifactCommitInput, WorkArtifactCommitOutput, WorkspaceCommitInput, WorkspaceCommitOutput,
    WorkspaceOpenInput, WorkspaceOpenOutput, WorkspaceOpenSnapshot,
};
use nexus_home_layout;
use nexus_orchestration::capability::{CapabilityError, WorkspaceExecutor};

use super::session::{SessionError, SessionId, WorkspaceSessionManager};
use super::work_artifact::{FsWorkArtifactPort, WorkArtifactPort, WorkArtifactWrite};
use crate::error::CoreError as NexusApiError;

/// Production workspace executor bound to one canonical workspace root.
pub struct WorkspaceCommitExecutor {
    session_manager: Arc<WorkspaceSessionManager>,
    canonical_workspace_root: String,
    /// Home the Work artifact port resolves the active creator/workspace from.
    ///
    /// `None` leaves [`WorkspaceExecutor::commit_work_artifact`] refusing: the
    /// executor was built without the production tool context (tests).
    work_artifact_home: Option<PathBuf>,
}

impl WorkspaceCommitExecutor {
    #[must_use]
    pub const fn new(
        session_manager: Arc<WorkspaceSessionManager>,
        canonical_workspace_root: String,
    ) -> Self {
        Self {
            session_manager,
            canonical_workspace_root,
            work_artifact_home: None,
        }
    }

    /// Attach the home whose creator/workspace config the Work artifact port
    /// resolves — the same value the hot path hands its port.
    #[must_use]
    pub fn with_work_artifact_home(mut self, nexus_home: PathBuf) -> Self {
        self.work_artifact_home = Some(nexus_home);
        self
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
        SessionError::ActiveWorkspaceMismatch { .. } => {
            CapabilityError::InputInvalid("session workspace root mismatch".into())
        }
        // A held workspace authority is a conflict, not a capability fault; it
        // cannot arise from an executor call (the bundle takes the lease before
        // the executor exists), so this arm is only here for exhaustiveness.
        SessionError::AuthorityBusy => {
            CapabilityError::PermanentExternal("workspace authority busy".into())
        }
        SessionError::Database(_) | SessionError::Io(_) | SessionError::Internal(_) => {
            CapabilityError::Internal("workspace storage error".into())
        }
    }
}

/// Keep the port's refusal class on the capability surface.
fn map_port_error(err: NexusApiError) -> CapabilityError {
    match err {
        NexusApiError::Forbidden { resource } => CapabilityError::Forbidden(resource),
        NexusApiError::InvalidInput { field, reason } => {
            CapabilityError::InputInvalid(format!("{field} — {reason}"))
        }
        other => CapabilityError::Internal(other.to_string()),
    }
}

#[async_trait]
impl WorkspaceExecutor for WorkspaceCommitExecutor {
    async fn open(
        &self,
        input: WorkspaceOpenInput,
    ) -> Result<WorkspaceOpenOutput, CapabilityError> {
        nexus_home_layout::validate_workspace_path_safe(&input.path)
            .map_err(CapabilityError::InputInvalid)?;
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
            self.canonical_workspace_root.clone(),
        )
        .await
        .map_err(map_session_error)?;
        Ok(WorkspaceCommitOutput {
            revision: outcome.revision,
            committed: outcome.committed,
        })
    }

    async fn commit_work_artifact(
        &self,
        input: WorkArtifactCommitInput,
    ) -> Result<WorkArtifactCommitOutput, CapabilityError> {
        let Some(nexus_home) = self.work_artifact_home.as_ref() else {
            return Err(CapabilityError::InputInvalid(
                "work artifact commit not wired".into(),
            ));
        };
        // Same per-dispatch construction the hot path performs: one port over
        // the shared pool, no second writer and no new public surface.
        let pool = self.session_manager.pool();
        FsWorkArtifactPort::new((*pool).clone(), nexus_home.clone())
            .commit(WorkArtifactWrite {
                workspace_root: Path::new(&input.workspace_root),
                work_ref: &input.work_ref,
                rel_child: Path::new(&input.rel_child),
                content: &input.content,
            })
            .await
            .map_err(map_port_error)?;
        Ok(WorkArtifactCommitOutput { committed: true })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use nexus_local_db::works::{self, WorkRecord};

    use super::*;

    const CREATOR: &str = "ctr_seam";
    const WORK_ID: &str = "wrk_seam";
    const WORK_REF: &str = "demo";

    /// The shape a scaffold builtin holds: the orchestration trait handle,
    /// received at construction, never a `nexus_core` type.
    struct BuiltinLikeCaller {
        executor: Arc<dyn WorkspaceExecutor>,
    }

    impl BuiltinLikeCaller {
        async fn write_chapter(
            &self,
            workspace_root: &str,
            work_ref: &str,
            content: &str,
        ) -> Result<(), CapabilityError> {
            let output = self
                .executor
                .commit_work_artifact(WorkArtifactCommitInput {
                    workspace_root: workspace_root.to_string(),
                    work_ref: work_ref.to_string(),
                    rel_child: "Body/chapter-01.md".to_string(),
                    content: content.to_string(),
                })
                .await?;
            assert!(output.committed, "the port reports a committed write");
            Ok(())
        }
    }

    fn work_record() -> WorkRecord {
        WorkRecord {
            work_id: WORK_ID.to_string(),
            creator_id: CREATOR.to_string(),
            workspace_slug: "default".to_string(),
            status: "draft".to_string(),
            title: "Seam Novel".to_string(),
            long_term_goal: "Prove the seam".to_string(),
            initial_idea: "A seam".to_string(),
            creative_brief: None,
            intake_status: "pending".to_string(),
            world_id: None,
            story_ref: None,
            inspiration_log: "[]".to_string(),
            primary_preset_id: "novel-writing".to_string(),
            schedule_ids: "[]".to_string(),
            created_at: "2026-06-04T10:00:00Z".to_string(),
            updated_at: "2026-06-04T10:00:00Z".to_string(),
            current_stage: "intake".to_string(),
            stage_status: "pending".to_string(),
            work_profile: None,
            work_ref: Some(WORK_REF.to_string()),
            total_planned_chapters: None,
            current_chapter: 0,
            auto_chain_enabled: true,
            driver_schedule_id: None,
            auto_chain_interrupted: false,
            auto_review_master_on_timeout: false,
            runtime_lock_holder: None,
            runtime_lock_acquired_at: None,
            completion_locked_at: None,
            novel_completion_status: None,
            lineage_from_work_id: None,
        }
    }

    #[tokio::test]
    async fn a_builtin_receives_the_executor_and_commits_work_bytes() {
        let home = tempfile::tempdir().expect("temp home");
        let workspace = tempfile::tempdir().expect("temp workspace");
        let config = format!(
            "active_creator_id = \"{CREATOR}\"\n\
             active_workspace_slug_by_creator = {{ {CREATOR} = \"default\" }}\n"
        );
        std::fs::write(home.path().join("config.toml"), config).expect("cli config");
        let pool = nexus_local_db::init_engine_pool(&home.path().join("state.db"))
            .await
            .expect("engine pool")
            .clone_pool();
        works::create_work(&pool, &work_record())
            .await
            .expect("works row");

        let executor: Arc<dyn WorkspaceExecutor> = Arc::new(
            WorkspaceCommitExecutor::new(
                Arc::new(WorkspaceSessionManager::new(Arc::new(pool))),
                workspace.path().to_string_lossy().into_owned(),
            )
            .with_work_artifact_home(home.path().to_path_buf()),
        );

        BuiltinLikeCaller { executor }
            .write_chapter(
                &workspace.path().to_string_lossy(),
                WORK_REF,
                "# Chapter 1\n",
            )
            .await
            .expect("the builtin commits through the injected handle");

        let written = std::fs::read_to_string(
            workspace
                .path()
                .join("Works")
                .join(WORK_REF)
                .join("Body/chapter-01.md"),
        )
        .expect("artifact bytes");
        assert_eq!(written, "# Chapter 1\n");
    }
}
