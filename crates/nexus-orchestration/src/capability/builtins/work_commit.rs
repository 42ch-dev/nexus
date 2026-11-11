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
/// [`CapabilityError::InputInvalid`] when `works_root` resolves anywhere but
/// this executor's own `Works/` tree (external or nested), refused here before
/// the claim or any directory/byte mutation.
pub fn resolve_works_dirs(
    executor: &dyn WorkspaceExecutor,
    works_root: &Path,
) -> Result<(PathBuf, PathBuf), CapabilityError> {
    let workspace_root = executor.workspace_root()?;
    let works_dir = workspace_root.join("Works");
    // The port always rebuilds `<workspace_root>/Works/<ref>/<child>` and the
    // real executor refuses any other parent only at commit — after the claim
    // and the mkdir. Confining the root here keeps a foreign or nested tree
    // from being mutated at all.
    if workspace_root.join(works_root) != works_dir {
        return Err(CapabilityError::InputInvalid(format!(
            "works_root must resolve to {}: {}",
            works_dir.display(),
            works_root.display()
        )));
    }
    Ok((workspace_root, works_dir))
}

/// The `work_ref` this run claimed on the works row, kept so a failed or
/// cancelled scaffold can put the prior value back.
///
/// The claim stays armed until the scaffold decides: [`Self::disarm`] keeps
/// the claimed ref, [`Self::restore`] reverts it, and the [`Drop`] guard
/// reverts it when the future is dropped before either runs.
pub struct WorkRefClaim {
    pool: Option<SqlitePool>,
    prior: Option<PriorWorkRef>,
    effective: String,
}

struct PriorWorkRef {
    creator_id: String,
    work_id: String,
    prior_ref: Option<String>,
    claimed_ref: String,
}

impl WorkRefClaim {
    /// The ref this run must use for every `Works/<ref>/` read and port commit.
    pub fn effective(&self) -> &str {
        &self.effective
    }

    /// Keep the claimed ref: the scaffold finished, so the guard must not fire.
    pub fn disarm(mut self) {
        self.prior = None;
    }

    /// Put the prior `work_ref` back after an unsuccessful scaffold.
    ///
    /// The update matches only while the row still holds the ref this run
    /// claimed, so a concurrent writer's ref is never clobbered. The guard
    /// keeps its recovery state until the update has landed, so a caller
    /// cancelled while this is pending still reverts through [`Drop`].
    ///
    /// # Errors
    /// [`CapabilityError::Internal`] when the compensating update fails; the
    /// caller reports the original scaffold error and logs this one, and the
    /// still-armed guard retries the revert on drop.
    pub async fn restore(&mut self) -> Result<(), CapabilityError> {
        let (Some(pool), Some(prior)) = (self.pool.as_ref(), self.prior.as_ref()) else {
            return Ok(());
        };
        restore_prior(pool, prior).await?;
        self.prior = None;
        Ok(())
    }
}

impl Drop for WorkRefClaim {
    fn drop(&mut self) {
        let (Some(pool), Some(prior)) = (self.pool.take(), self.prior.take()) else {
            return;
        };
        // A cancelled scaffold never reaches `restore`, so the revert has to
        // run from here. Drop cannot await; the runtime finishes the update.
        // shortcut: needs a live runtime — a cancellation while the runtime is
        // shutting down can miss the revert.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                if let Err(err) = restore_prior(&pool, &prior).await {
                    tracing::warn!(
                        work_id = %prior.work_id,
                        error = %err,
                        "cancelled scaffold: restore prior work_ref failed"
                    );
                }
            });
        } else {
            tracing::warn!(
                work_id = %prior.work_id,
                "cancelled scaffold: no runtime for the prior work_ref restore"
            );
        }
    }
}

async fn restore_prior(pool: &SqlitePool, prior: &PriorWorkRef) -> Result<(), CapabilityError> {
    sqlx::query(
        "UPDATE works SET work_ref = ? \
         WHERE work_id = ? AND creator_id = ? AND work_ref = ?",
    )
    .bind(&prior.prior_ref)
    .bind(&prior.work_id)
    .bind(&prior.creator_id)
    .bind(&prior.claimed_ref)
    .execute(pool)
    .await
    .map_err(|e| CapabilityError::Internal(format!("restore work_ref: {e}")))?;
    Ok(())
}

/// Claim `work_ref` for the Work so the port can resolve `Works/<work_ref>/`.
///
/// The port resolves the Work through the works row, so a first scaffold has
/// to publish its ref before the first byte. This publishes only that minimum
/// identity; the profile and any final metadata land after the files do.
///
/// `requested` is `None` for a partial re-init that does not select
/// `work_ref`: the stored ref is then kept and returned as the effective one,
/// so the port and the filesystem reads address the same directory.
///
/// # Errors
/// [`CapabilityError::Forbidden`] when no works row belongs to `creator_id`
/// (nothing is claimed), [`CapabilityError::InputInvalid`] for a partial
/// re-init of a Work with no stored ref, [`CapabilityError::Internal`] on a
/// database failure.
pub async fn claim_work_ref(
    pool: Option<&SqlitePool>,
    creator_id: &str,
    work_id: &str,
    requested: Option<&str>,
) -> Result<WorkRefClaim, CapabilityError> {
    let Some(pool) = pool else {
        // No row to read: a partial re-init that excludes `work_ref` cannot
        // resolve the stored ref, so refuse instead of guessing.
        let Some(work_ref) = requested else {
            return Err(CapabilityError::InputInvalid(
                "a re-init excluding work_ref needs a works row".to_string(),
            ));
        };
        return Ok(WorkRefClaim {
            pool: None,
            prior: None,
            effective: work_ref.to_string(),
        });
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
    let Some(work_ref) = requested else {
        let Some(stored) = prior else {
            return Err(CapabilityError::InputInvalid(format!(
                "work '{work_id}' has no stored work_ref to re-init"
            )));
        };
        return Ok(WorkRefClaim {
            pool: Some(pool.clone()),
            prior: None,
            effective: stored,
        });
    };
    if prior.as_deref() == Some(work_ref) {
        return Ok(WorkRefClaim {
            pool: Some(pool.clone()),
            prior: None,
            effective: work_ref.to_string(),
        });
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
        pool: Some(pool.clone()),
        prior: Some(PriorWorkRef {
            creator_id: creator_id.to_string(),
            work_id: work_id.to_string(),
            prior_ref: prior,
            claimed_ref: work_ref.to_string(),
        }),
        effective: work_ref.to_string(),
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
        hang: Option<std::sync::Arc<tokio::sync::Notify>>,
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
                ..Self::default()
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
                ..Self::default()
            })
        }

        /// [`Self::at`] whose commits signal [`Self::reached`] and then never
        /// finish — a scaffold suspended mid-write, ready to be cancelled.
        pub(crate) fn hanging_at(
            workspace_root: impl Into<std::path::PathBuf>,
        ) -> std::sync::Arc<Self> {
            std::sync::Arc::new(Self {
                hang: Some(std::sync::Arc::new(tokio::sync::Notify::new())),
                workspace_root: Some(workspace_root.into()),
                ..Self::default()
            })
        }

        pub(crate) fn reached(&self) -> Option<&std::sync::Arc<tokio::sync::Notify>> {
            self.hang.as_ref()
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
            if let Some(reached) = &self.hang {
                reached.notify_one();
                std::future::pending::<()>().await;
            }
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

    /// One pooled connection, no acquire timeout: holding it keeps any
    /// competing statement pending until the test releases it.
    async fn single_connection_pool(dir: &Path, work_ref: Option<&str>) -> SqlitePool {
        let guarded = nexus_local_db::writer_protocol::init_engine_pool(
            &dir.join("state.db"),
            nexus_local_db::BOOTSTRAP_CREATOR_ID,
            nexus_local_db::GuardedPoolOptions {
                max_connections: 1,
                acquire_timeout: None,
            },
        )
        .await
        .expect("engine pool");
        let pool = guarded.clone_pool();
        works::create_work(&pool, &work_record(work_ref, None))
            .await
            .expect("works row");
        pool
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

    #[tokio::test]
    async fn a_custom_works_root_reads_and_commits_the_same_tree() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let pool = pool_with_work(tmp.path(), Some("custom-ref"), Some("essay")).await;
        let elsewhere = tmp.path().join("elsewhere");
        let works_root = elsewhere.join("Works");
        let executor = RecordingExecutor::at(elsewhere.clone());
        let probe = std::sync::Arc::clone(&executor);
        let cap = EssayProjectScaffold::new_with_root(pool.clone(), works_root.clone())
            .with_workspace_executor(Some(executor));

        cap.run(json!({
            "creator_id": CREATOR,
            "work_id": WORK_ID,
            "work_ref": "custom-ref",
            "title": "Custom Root Essay",
        }))
        .await
        .expect("a custom Works root is a supported override");

        let commit = probe.commits().into_iter().next().expect("one commit");
        assert_eq!(
            Path::new(&commit.workspace_root).join("Works"),
            works_root,
            "the port target must be the same tree the scaffold read"
        );
        assert!(works_root.join("custom-ref").join("README.md").is_file());
        assert!(
            !tmp.path().join("Works").exists(),
            "no default Works tree may appear beside the custom root"
        );
    }

    #[tokio::test]
    async fn a_works_root_not_named_works_is_refused_before_mutation() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let pool = pool_with_work(tmp.path(), Some("keep-ref"), Some("essay")).await;
        let works_root = tmp.path().join("Custom");
        let executor = RecordingExecutor::at(tmp.path().to_path_buf());
        let probe = std::sync::Arc::clone(&executor);
        let cap = EssayProjectScaffold::new_with_root(pool.clone(), works_root.clone())
            .with_workspace_executor(Some(executor));

        let err = cap
            .run(json!({
                "creator_id": CREATOR,
                "work_id": WORK_ID,
                "work_ref": "other-ref",
                "title": "Split Root Essay",
            }))
            .await
            .expect_err("a root that is not a Works directory must be refused");
        assert!(
            matches!(err, CapabilityError::InputInvalid(_)),
            "expected the works_root refusal, got: {err}"
        );
        assert!(!works_root.exists(), "no directory may be created");
        assert!(probe.commits().is_empty(), "no byte may be committed");
        assert_eq!(
            stored_identity(&pool).await,
            (Some("keep-ref".to_string()), Some("essay".to_string())),
            "the refusal lands before the claim, so metadata is untouched"
        );
    }

    #[tokio::test]
    async fn a_cancelled_scaffold_restores_the_claimed_ref() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let pool = pool_with_work(tmp.path(), Some("prior-ref"), Some("essay")).await;
        let works_root = tmp.path().join("Works");
        let executor = RecordingExecutor::hanging_at(tmp.path().to_path_buf());
        let reached = executor
            .reached()
            .expect("hanging executor carries a signal")
            .clone();
        let cap = EssayProjectScaffold::new_with_root(pool.clone(), works_root.clone())
            .with_workspace_executor(Some(executor));

        let run = tokio::spawn(async move {
            cap.run(json!({
                "creator_id": CREATOR,
                "work_id": WORK_ID,
                "work_ref": "cancelled-ref",
                "title": "Cancelled Essay",
            }))
            .await
        });
        // The claim is published before the first commit, so reaching the
        // hanging commit proves the claim is set when the future is dropped.
        reached.notified().await;
        run.abort();
        assert!(run.await.is_err(), "the scaffold run was cancelled");

        // The drop guard restores on its own task; give it turns to land.
        let mut restored = false;
        for _ in 0..200 {
            if stored_identity(&pool).await.0.as_deref() == Some("prior-ref") {
                restored = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert!(
            restored,
            "cancellation must restore the prior work_ref, not keep the claim"
        );
        assert!(
            !works_root.join("cancelled-ref").exists(),
            "the cancelled scaffold also rolls back the tree it created"
        );
    }

    #[tokio::test]
    async fn a_cancelled_restore_still_restores_the_prior_ref() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let pool = single_connection_pool(tmp.path(), Some("prior-ref")).await;
        let mut claim = claim_work_ref(Some(&pool), CREATOR, WORK_ID, Some("claimed-ref"))
            .await
            .expect("claim the ref");
        assert_eq!(
            stored_identity(&pool).await.0.as_deref(),
            Some("claimed-ref")
        );

        // Holding the only pooled connection keeps the compensating UPDATE
        // pending, so the restore below is cancelled mid-await.
        let held = pool.acquire().await.expect("occupy the only connection");
        let started = std::sync::Arc::new(tokio::sync::Notify::new());
        let signal = std::sync::Arc::clone(&started);
        let restore = tokio::spawn(async move {
            signal.notify_one();
            claim.restore().await
        });
        started.notified().await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        restore.abort();
        assert!(restore.await.is_err(), "the restore was cancelled");
        drop(held);

        // The guard restores on its own task; give it turns to land.
        let mut restored = false;
        for _ in 0..200 {
            if stored_identity(&pool).await.0.as_deref() == Some("prior-ref") {
                restored = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert!(
            restored,
            "cancelling a pending restore must still revert through the guard"
        );
    }

    /// An alternate `Works` tree is refused by the resolver, before the claim
    /// or any directory/byte mutation.
    async fn refuses_works_root(workspace: &Path, works_root: PathBuf) {
        let dir = tempfile::tempdir().expect("tmpdir");
        let pool = pool_with_work(dir.path(), Some("keep-ref"), Some("essay")).await;
        let executor = RecordingExecutor::at(workspace.to_path_buf());
        let probe = std::sync::Arc::clone(&executor);
        let cap = EssayProjectScaffold::new_with_root(pool.clone(), works_root.clone())
            .with_workspace_executor(Some(executor));

        let err = cap
            .run(json!({
                "creator_id": CREATOR,
                "work_id": WORK_ID,
                "work_ref": "other-ref",
                "title": "Foreign Root Essay",
            }))
            .await
            .expect_err("an unconfined works_root must be refused");
        assert!(
            matches!(err, CapabilityError::InputInvalid(_)),
            "expected the works_root refusal, got: {err}"
        );
        assert!(!works_root.exists(), "no directory may be created");
        assert!(
            !workspace.join("Works").exists(),
            "the executor's own tree must stay untouched"
        );
        assert!(probe.commits().is_empty(), "no byte may be committed");
        assert_eq!(
            stored_identity(&pool).await,
            (Some("keep-ref".to_string()), Some("essay".to_string())),
            "the refusal lands before the claim, so metadata is untouched"
        );
    }

    #[tokio::test]
    async fn external_and_nested_works_roots_are_refused_before_mutation() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let workspace = tmp.path().join("ws");
        std::fs::create_dir_all(&workspace).expect("workspace dir");

        refuses_works_root(&workspace, tmp.path().join("other").join("Works")).await;
        refuses_works_root(&workspace, workspace.join("nested").join("Works")).await;
    }

    #[tokio::test]
    async fn restore_does_not_clobber_a_concurrent_writers_ref() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let pool = pool_with_work(tmp.path(), Some("prior-ref"), None).await;
        let mut claim = claim_work_ref(Some(&pool), CREATOR, WORK_ID, Some("claimed-ref"))
            .await
            .expect("claim the ref");
        assert_eq!(
            stored_identity(&pool).await.0.as_deref(),
            Some("claimed-ref")
        );

        sqlx::query("UPDATE works SET work_ref = ? WHERE work_id = ?")
            .bind("concurrent-ref")
            .bind(WORK_ID)
            .execute(&pool)
            .await
            .expect("concurrent write");

        claim
            .restore()
            .await
            .expect("a guarded restore reports no failure");
        assert_eq!(
            stored_identity(&pool).await.0.as_deref(),
            Some("concurrent-ref"),
            "restore must only match the ref this run claimed"
        );
    }
}
