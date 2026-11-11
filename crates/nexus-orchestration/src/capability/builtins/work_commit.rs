//! Work byte writes for the scaffold/status builtins (ADR #485 §6.1 P2).
//!
//! Each builtin here writes below its `Works/<work_ref>/` root. The injected
//! [`WorkspaceExecutor`] carries the core `WorkArtifactPort`, so those bytes
//! land under the Work lock and the shared durability path instead of a local
//! temp+rename. Only the byte write moves: template/status rendering, scaffold
//! ordering, and the synchronous `Drop` rollback stay where they were.

use std::path::Path;

use nexus_contracts::local::orchestration::WorkArtifactCommitInput;

use crate::capability::{CapabilityError, WorkspaceExecutor};

/// The injected executor, or the house "provider not wired" refusal.
///
/// # Errors
/// [`CapabilityError::WorkerUnavailable`] when the registry arm attached no
/// executor, so a Work byte write can never fall back to a local writer.
pub(crate) fn require_executor(
    executor: Option<&std::sync::Arc<dyn WorkspaceExecutor>>,
) -> Result<&dyn WorkspaceExecutor, CapabilityError> {
    executor
        .map(std::sync::Arc::as_ref)
        .ok_or(CapabilityError::WorkerUnavailable)
}

/// The workspace root owning a builtin's `Works/` directory.
///
/// A builtin's `works_root` (default `"Works"`) is the `Works/` directory
/// itself, so its owning workspace root is the parent path.
pub(crate) fn workspace_root_of(works_root: &Path) -> &Path {
    works_root.parent().unwrap_or_else(|| Path::new(""))
}

/// Commit one Work artifact byte write (`<workspace_root>/Works/<work_ref>/<rel_child>`).
///
/// # Errors
/// Propagates the executor's refusal: [`CapabilityError::Forbidden`] for a
/// missing works row or a path outside the Work, [`CapabilityError::Internal`]
/// on a write failure.
pub(crate) async fn commit_work_bytes(
    executor: &dyn WorkspaceExecutor,
    workspace_root: &Path,
    work_ref: &str,
    rel_child: &Path,
    content: &str,
) -> Result<(), CapabilityError> {
    executor
        .commit_work_artifact(WorkArtifactCommitInput {
            workspace_root: workspace_root.to_string_lossy().into_owned(),
            work_ref: work_ref.to_string(),
            rel_child: rel_child.to_string_lossy().into_owned(),
            content: content.to_string(),
        })
        .await
        .map(|_| ())
}

/// Recording test executor for the builtin modules' own tests.
#[cfg(test)]
pub(crate) mod test_support {
    use std::path::Path;

    use async_trait::async_trait;
    use nexus_contracts::local::orchestration::{
        WorkArtifactCommitInput, WorkArtifactCommitOutput, WorkspaceCommitInput,
        WorkspaceCommitOutput, WorkspaceOpenInput, WorkspaceOpenOutput,
    };

    use crate::capability::{CapabilityError, WorkspaceExecutor};

    /// Writes each artifact where the core port would put it and records the
    /// commit; `fail_on` simulates the port's missing-work-row refusal.
    #[derive(Default)]
    pub(crate) struct RecordingExecutor {
        commits: std::sync::Mutex<Vec<WorkArtifactCommitInput>>,
        fail_on: Option<&'static str>,
    }

    impl RecordingExecutor {
        pub(crate) fn new() -> std::sync::Arc<Self> {
            std::sync::Arc::new(Self::default())
        }

        /// An executor whose commit of `rel_child` refuses like the port does
        /// for a Work with no `works` row.
        pub(crate) fn refusing(rel_child: &'static str) -> std::sync::Arc<Self> {
            std::sync::Arc::new(Self {
                commits: std::sync::Mutex::new(Vec::new()),
                fail_on: Some(rel_child),
            })
        }

        pub(crate) fn commits(&self) -> Vec<WorkArtifactCommitInput> {
            self.commits
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }
    }

    #[async_trait]
    impl WorkspaceExecutor for RecordingExecutor {
        async fn open(
            &self,
            _input: WorkspaceOpenInput,
        ) -> Result<WorkspaceOpenOutput, CapabilityError> {
            Err(CapabilityError::WorkerUnavailable)
        }

        async fn commit(
            &self,
            _input: WorkspaceCommitInput,
        ) -> Result<WorkspaceCommitOutput, CapabilityError> {
            Err(CapabilityError::WorkerUnavailable)
        }

        async fn commit_work_artifact(
            &self,
            input: WorkArtifactCommitInput,
        ) -> Result<WorkArtifactCommitOutput, CapabilityError> {
            if self.fail_on == Some(input.rel_child.as_str()) {
                return Err(CapabilityError::Forbidden(format!(
                    "work '{}' has no works row in the active workspace",
                    input.work_ref
                )));
            }
            let target = Path::new(&input.workspace_root)
                .join("Works")
                .join(&input.work_ref)
                .join(&input.rel_child);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    CapabilityError::Internal(format!("mkdir {}: {e}", parent.display()))
                })?;
            }
            std::fs::write(&target, &input.content).map_err(|e| {
                CapabilityError::Internal(format!("write {}: {e}", target.display()))
            })?;
            self.commits
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(input);
            Ok(WorkArtifactCommitOutput { committed: true })
        }
    }
}
