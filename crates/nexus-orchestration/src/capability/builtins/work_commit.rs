//! Work byte writes for the scaffold/status builtins (ADR #485 §6.1 P2).
//!
//! Each builtin here writes below its `Works/<work_ref>/` root. The injected
//! [`WorkspaceExecutor`] carries the core `WorkArtifactPort`, so those bytes
//! land under the Work lock and the shared durability path instead of a local
//! temp+rename. Only the byte write moves: template/status rendering, scaffold
//! ordering, and the synchronous `Drop` rollback stay where they were.

use std::path::{Path, PathBuf};

use nexus_contracts::local::orchestration::WorkArtifactCommitInput;
use sqlx::SqlitePool;

use crate::capability::{CapabilityError, WorkspaceExecutor};

/// The injected executor, or the house "provider not wired" refusal.
///
/// # Errors
/// [`CapabilityError::WorkerUnavailable`] when the registry arm attached no
/// executor, so a Work byte write can never fall back to a local writer.
pub fn require_executor(
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
pub fn workspace_root_of(works_root: &Path) -> &Path {
    works_root.parent().unwrap_or_else(|| Path::new(""))
}

/// The absolute `Works/` directory a scaffold run writes into, plus the
/// workspace root the port rebuilds it under.
///
/// Resolved once per run from the executor's own workspace root — never the
/// process cwd — so existence checks, directory creation, the port commit and
/// rollback bookkeeping all address the same tree.
///
/// # Errors
/// [`CapabilityError::WorkerUnavailable`] when the executor carries no
/// workspace root, so no caller falls back to a relative path.
pub fn resolve_works_dirs(
    executor: &dyn WorkspaceExecutor,
    works_root: &Path,
) -> Result<(PathBuf, PathBuf), CapabilityError> {
    let works_dir = executor.workspace_root()?.join(works_root);
    let Some(workspace_root) = works_dir.parent().map(Path::to_path_buf) else {
        return Err(CapabilityError::WorkerUnavailable);
    };
    Ok((workspace_root, works_dir))
}

/// The `work_ref` this run claimed on the works row, kept so a failed scaffold
/// can put the prior value back.
pub struct WorkRefClaim {
    prior: Option<PriorWorkRef>,
}

struct PriorWorkRef {
    creator_id: String,
    work_id: String,
    work_ref: Option<String>,
}

impl WorkRefClaim {
    /// A run that changed nothing (no pool, or the row already held this ref).
    pub const fn none() -> Self {
        Self { prior: None }
    }

    /// Put the prior `work_ref` back after an unsuccessful scaffold.
    ///
    /// # Errors
    /// [`CapabilityError::Internal`] when the compensating update fails; the
    /// caller reports the original scaffold error and logs this one.
    pub async fn restore(self, pool: Option<&SqlitePool>) -> Result<(), CapabilityError> {
        let (Some(pool), Some(prior)) = (pool, self.prior) else {
            return Ok(());
        };
        sqlx::query("UPDATE works SET work_ref = ? WHERE work_id = ? AND creator_id = ?")
            .bind(prior.work_ref)
            .bind(&prior.work_id)
            .bind(&prior.creator_id)
            .execute(pool)
            .await
            .map_err(|e| CapabilityError::Internal(format!("restore work_ref: {e}")))?;
        Ok(())
    }
}

/// Claim `work_ref` for the Work so the port can resolve `Works/<work_ref>/`.
///
/// The port resolves the Work through the works row, so a first scaffold has
/// to publish its ref before the first byte. This publishes only that minimum
/// identity; the profile and any final metadata land after the files do.
///
/// # Errors
/// [`CapabilityError::Forbidden`] when no works row belongs to `creator_id`
/// (nothing is claimed), [`CapabilityError::Internal`] on a database failure.
pub async fn claim_work_ref(
    pool: Option<&SqlitePool>,
    creator_id: &str,
    work_id: &str,
    work_ref: &str,
) -> Result<WorkRefClaim, CapabilityError> {
    let Some(pool) = pool else {
        return Ok(WorkRefClaim::none());
    };
    let prior: Option<Option<String>> =
        sqlx::query_scalar("SELECT work_ref FROM works WHERE work_id = ? AND creator_id = ?")
            .bind(work_id)
            .bind(creator_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| CapabilityError::Internal(format!("read works row: {e}")))?;
    let Some(prior) = prior else {
        return Err(CapabilityError::Forbidden(format!(
            "work '{work_id}' has no works row for creator '{creator_id}'"
        )));
    };
    if prior.as_deref() == Some(work_ref) {
        return Ok(WorkRefClaim::none());
    }
    let updated = sqlx::query("UPDATE works SET work_ref = ? WHERE work_id = ? AND creator_id = ?")
        .bind(work_ref)
        .bind(work_id)
        .bind(creator_id)
        .execute(pool)
        .await
        .map_err(|e| CapabilityError::Internal(format!("claim work_ref: {e}")))?;
    if updated.rows_affected() != 1 {
        return Err(CapabilityError::Forbidden(format!(
            "work '{work_id}' is not owned by creator '{creator_id}'"
        )));
    }
    Ok(WorkRefClaim {
        prior: Some(PriorWorkRef {
            creator_id: creator_id.to_string(),
            work_id: work_id.to_string(),
            work_ref: prior,
        }),
    })
}

/// Publish a scaffold's final profile + ref after its files landed.
///
/// # Errors
/// [`CapabilityError::Forbidden`] when the row is missing or not owned by
/// `creator_id`, [`CapabilityError::Internal`] on a database failure.
pub async fn publish_work_identity(
    pool: Option<&SqlitePool>,
    creator_id: &str,
    work_id: &str,
    work_profile: &str,
    work_ref: &str,
) -> Result<(), CapabilityError> {
    let Some(pool) = pool else {
        return Ok(());
    };
    let updated = sqlx::query(
        "UPDATE works SET work_profile = ?, work_ref = ? WHERE work_id = ? AND creator_id = ?",
    )
    .bind(work_profile)
    .bind(work_ref)
    .bind(work_id)
    .bind(creator_id)
    .execute(pool)
    .await
    .map_err(|e| CapabilityError::Internal(format!("patch works row: {e}")))?;
    if updated.rows_affected() != 1 {
        return Err(CapabilityError::Forbidden(format!(
            "work '{work_id}' is not owned by creator '{creator_id}'"
        )));
    }
    Ok(())
}

/// Commit one Work artifact byte write (`<workspace_root>/Works/<work_ref>/<rel_child>`).
///
/// # Errors
/// Propagates the executor's refusal: [`CapabilityError::Forbidden`] for a
/// missing works row or a path outside the Work, [`CapabilityError::Internal`]
/// on a write failure.
pub async fn commit_work_bytes(
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
pub mod test_support {
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
    pub struct RecordingExecutor {
        commits: std::sync::Mutex<Vec<WorkArtifactCommitInput>>,
        fail_on: Option<&'static str>,
        workspace_root: Option<std::path::PathBuf>,
    }

    impl RecordingExecutor {
        /// A recorder with no bound workspace: `commit_work_artifact` still
        /// writes, but a caller that must resolve `Works/` refuses first.
        pub(crate) fn new() -> std::sync::Arc<Self> {
            std::sync::Arc::new(Self::default())
        }

        /// A recorder bound to `workspace_root` — the root a scaffold run
        /// resolves its `Works/` bookkeeping against.
        pub(crate) fn at(workspace_root: impl Into<std::path::PathBuf>) -> std::sync::Arc<Self> {
            std::sync::Arc::new(Self {
                workspace_root: Some(workspace_root.into()),
                ..Self::default()
            })
        }

        /// A recorder whose commit of `rel_child` refuses like the port does
        /// for a Work with no `works` row.
        pub(crate) fn refusing(rel_child: &'static str) -> std::sync::Arc<Self> {
            std::sync::Arc::new(Self {
                commits: std::sync::Mutex::new(Vec::new()),
                fail_on: Some(rel_child),
                workspace_root: None,
            })
        }

        /// [`Self::refusing`] over a bound workspace root.
        pub(crate) fn refusing_at(
            workspace_root: impl Into<std::path::PathBuf>,
            rel_child: &'static str,
        ) -> std::sync::Arc<Self> {
            std::sync::Arc::new(Self {
                commits: std::sync::Mutex::new(Vec::new()),
                fail_on: Some(rel_child),
                workspace_root: Some(workspace_root.into()),
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
        fn workspace_root(&self) -> Result<std::path::PathBuf, CapabilityError> {
            self.workspace_root
                .clone()
                .ok_or(CapabilityError::WorkerUnavailable)
        }

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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::test_support::RecordingExecutor;
    use super::*;
    use crate::capability::builtins::{EssayProjectScaffold, NovelProjectScaffold};
    use crate::capability::Capability;
    use nexus_local_db::works::{self, WorkRecord};
    use serde_json::json;

    const CREATOR: &str = "ctr_claim";
    const WORK_ID: &str = "wrk_claim";

    fn work_record(work_ref: Option<&str>, work_profile: Option<&str>) -> WorkRecord {
        WorkRecord {
            work_id: WORK_ID.to_string(),
            creator_id: CREATOR.to_string(),
            workspace_slug: "default".to_string(),
            status: "draft".to_string(),
            title: "Claimed Work".to_string(),
            long_term_goal: "Prove the claim".to_string(),
            initial_idea: "A claim".to_string(),
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
            work_profile: work_profile.map(str::to_string),
            work_ref: work_ref.map(str::to_string),
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

    async fn pool_with_work(
        dir: &Path,
        work_ref: Option<&str>,
        work_profile: Option<&str>,
    ) -> SqlitePool {
        let guarded = nexus_local_db::init_engine_pool(&dir.join("state.db"))
            .await
            .expect("engine pool");
        let pool = guarded.clone_pool();
        works::create_work(&pool, &work_record(work_ref, work_profile))
            .await
            .expect("works row");
        pool
    }

    async fn stored_identity(pool: &SqlitePool) -> (Option<String>, Option<String>) {
        sqlx::query_as::<_, (Option<String>, Option<String>)>(
            "SELECT work_ref, work_profile FROM works WHERE work_id = ?",
        )
        .bind(WORK_ID)
        .fetch_one(pool)
        .await
        .expect("read works row")
    }

    #[tokio::test]
    async fn a_refused_first_commit_restores_the_prior_ref_and_profile() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let pool = pool_with_work(tmp.path(), Some("prior-ref"), Some("novel")).await;
        let works_root = tmp.path().join("Works");
        let executor = RecordingExecutor::refusing_at(tmp.path(), "README.md");
        let cap = EssayProjectScaffold::new_with_root(pool.clone(), works_root.clone())
            .with_workspace_executor(Some(executor));

        let err = cap
            .run(json!({
                "creator_id": CREATOR,
                "work_id": WORK_ID,
                "work_ref": "new-ref",
                "title": "Claimed Essay",
            }))
            .await
            .expect_err("the first port commit refuses");
        assert!(
            matches!(err, CapabilityError::Forbidden(_)),
            "expected the port refusal to propagate, got: {err}"
        );

        assert_eq!(
            stored_identity(&pool).await,
            (Some("prior-ref".to_string()), Some("novel".to_string())),
            "a failed scaffold must restore the claimed ref and leave the profile alone"
        );
        assert!(
            !works_root.join("new-ref").exists(),
            "the rolled-back tree must not survive"
        );
    }

    #[tokio::test]
    async fn a_late_novel_transaction_failure_restores_the_prior_ref() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        // The works row names a creator with no `creators` row, so
        // `create_world_tx` fails inside the chapters/world transaction AFTER
        // the scaffold files landed.
        let pool = pool_with_work(tmp.path(), Some("prior-ref"), None).await;
        let works_root = tmp.path().join("Works");
        let executor = RecordingExecutor::at(tmp.path());
        let cap = NovelProjectScaffold::new_with_root(pool.clone(), works_root.clone())
            .with_workspace_executor(Some(executor));

        cap.run(json!({
            "creator_id": CREATOR,
            "work_id": WORK_ID,
            "work_ref": "novel-ref",
            "title": "Claimed Novel",
            "create_world": true,
            "world_title": "Claim World",
            "total_planned_chapters": 2,
        }))
        .await
        .expect_err("create_world_tx fails on the missing creator row");

        assert_eq!(
            stored_identity(&pool).await.0,
            Some("prior-ref".to_string()),
            "the late transaction failure must restore the claimed ref"
        );
        assert!(
            !works_root.join("novel-ref").join("README.md").exists(),
            "the filesystem rollback still removes the files this run wrote"
        );
    }

    #[tokio::test]
    async fn a_partial_reinit_excluding_work_ref_keeps_the_stored_ref() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let pool = pool_with_work(tmp.path(), Some("keep-ref"), Some("novel")).await;
        nexus_local_db::creators::ensure_creator_row(&pool, CREATOR, "Claimed Work")
            .await
            .expect("creator row");
        let works_root = tmp.path().join("Works");
        let executor = RecordingExecutor::at(tmp.path());
        let cap = NovelProjectScaffold::new_with_root(pool.clone(), works_root)
            .with_workspace_executor(Some(executor));

        cap.run(json!({
            "creator_id": CREATOR,
            "work_id": WORK_ID,
            "work_ref": "other-ref",
            "title": "Renamed",
            "create_world": true,
            "world_title": "Claim World",
            "total_planned_chapters": 1,
            "fields_changed": ["title"],
        }))
        .await
        .expect("a partial re-init excluding work_ref succeeds");

        assert_eq!(
            stored_identity(&pool).await.0,
            Some("keep-ref".to_string()),
            "a selection without work_ref must not overwrite the stored ref"
        );
    }
}
