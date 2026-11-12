//! The single byte-write port for Work artifacts (`Works/<work_ref>/…`).
//!
//! ADR #485 §4.4/§7.1 (P0+P1): every Work byte write commits through one
//! [`WorkArtifactPort`], durably and under the per-Work [`WorkLock`]. The tool
//! surface classifies its raw path with [`classify_work_path`] and hands the
//! `Works/<work_ref>/<child…>` shape here; every other workspace path keeps
//! its caller's own guarded write — this port never widens to the workspace.

use std::path::{Component, Path, PathBuf};

use nexus_home_layout::active_context::{
    read_active_creator_id, workspace_slug_selection, WorkspaceSlugSelection,
};
use nexus_local_db::work_chapters::{self, ReconcileDiff, ReconcileOp, ReconcileReport};
use nexus_local_db::works;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::content::{create_parent_dirs, fsync_write_atomic, WorkLock};
use crate::error::CoreError as NexusApiError;
use crate::slug::validate_slug_name;
use crate::works::RuntimeLockGuard;

/// A raw tool path classified against the `Works/<work_ref>/` shape.
#[derive(Debug)]
pub(crate) enum WorkPath<'a> {
    /// Not under `Works/` — the caller keeps its existing guarded write.
    NonWork,
    /// A case variant of `Works` (`works/…`, `WORKS/…`): the port writes the
    /// literal `Works` tree, so the read and write halves of the tool surface
    /// both refuse the spelling instead of routing it.
    CaseVariantWorks,
    /// `Works/<work_ref>/<child…>`: a writable Work artifact.
    Artifact {
        /// The Work directory name (or Work id) the path names.
        work_ref: &'a str,
        /// The artifact path below the Work root; non-empty, `..`-free.
        rel_child: &'a Path,
    },
    /// `Works/<work_ref>` itself: a directory, never a writable artifact.
    WorkRoot {
        /// The Work directory name the path names.
        work_ref: &'a str,
    },
    /// A `..` inside the Work subtree: lexically inside, physically not.
    EscapedWork {
        /// The Work directory name the path names.
        work_ref: &'a str,
    },
}

/// Classify a raw tool path against the `Works/<work_ref>/` shape.
///
/// Decided on the raw string, before any I/O: the first component must be
/// `Works` byte-exactly, and the artifact remainder must be non-empty and
/// `..`-free. A lexical prefix check alone would accept `Works/<ref>/../x.md`,
/// so classification itself rejects `..`.
///
/// A case-variant first component is refused rather than routed: this port's
/// target is built with the literal `Works`, and the read path refuses the
/// same spelling, so routing `works/…` would write a *different* tree on a
/// case-sensitive filesystem — bypassing this port's lock and durability path
/// — while the same path stayed unreadable.
#[must_use]
pub(crate) fn classify_work_path(path: &str) -> WorkPath<'_> {
    let mut components = Path::new(path).components();
    // Rust keeps a leading `./` as a `CurDir` component (interior `.` is
    // normalized away), so without dropping it `./Works/<ref>/x.md` would fall
    // through as `NonWork` — the generic guarded write with no port, lock or
    // boundary re-check — and `./works/<ref>/x.md` would skip the case-variant
    // refusal.
    while components.clone().next() == Some(Component::CurDir) {
        components.next();
    }
    let Some(Component::Normal(first)) = components.next() else {
        return WorkPath::NonWork;
    };
    match first.to_str() {
        Some("Works") => {}
        Some(other) if other.eq_ignore_ascii_case("Works") => {
            return WorkPath::CaseVariantWorks;
        }
        _ => return WorkPath::NonWork,
    }
    let Some(second) = components.next() else {
        // `Works`, `Works/`: not under any work.
        return WorkPath::NonWork;
    };
    let work_ref = second.as_os_str().to_str().unwrap_or_default();
    if !matches!(second, Component::Normal(_)) {
        return WorkPath::EscapedWork { work_ref };
    }
    let rel_child = components.as_path();
    if rel_child.as_os_str().is_empty() {
        return WorkPath::WorkRoot { work_ref };
    }
    if rel_child
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return WorkPath::EscapedWork { work_ref };
    }
    WorkPath::Artifact {
        work_ref,
        rel_child,
    }
}

/// One Work artifact byte write handed to a [`WorkArtifactPort`].
///
/// The adapter builds the absolute target itself from
/// `<workspace_root>/Works/<work_ref>/<rel_child>`, so the classification and
/// the constructed target cannot disagree.
pub(crate) struct WorkArtifactWrite<'a> {
    /// The workspace whose `Works/` tree owns the artifact.
    pub(crate) workspace_root: &'a Path,
    /// The Work directory name (or Work id) the caller addressed.
    pub(crate) work_ref: &'a str,
    /// The artifact path below the Work root.
    pub(crate) rel_child: &'a Path,
    /// The bytes to commit.
    pub(crate) content: &'a str,
}

/// One reconcile admission handed to a [`WorkArtifactPort`].
///
/// The caller has already resolved and authorised the Work; this request only
/// names what the port needs to reconcile its index with the tree.
pub(crate) struct WorkReconcile<'a> {
    /// The workspace whose `Works/` tree is the projection being reconciled.
    pub(crate) workspace_root: &'a Path,
    /// Owner of the Work, from the caller's verified principal.
    pub(crate) creator_id: &'a str,
    /// The Work whose `work_chapters` index is reconciled.
    pub(crate) work_id: &'a str,
    /// The Work directory name below `Works/`.
    pub(crate) work_ref: &'a str,
    /// Timestamp stamped on the applied DB index rows.
    pub(crate) now: &'a str,
    /// Label for the minted `cli:<kind>:<uuid>` runtime-lock holder.
    pub(crate) holder_kind: &'a str,
    /// Report the diff without acquiring the lock or writing anything.
    pub(crate) dry_run: bool,
}

/// The single byte-write port for Work artifacts.
#[allow(async_fn_in_trait)] // matches the `KbStore` precedent (nexus-knowledge)
pub(crate) trait WorkArtifactPort {
    /// Commit one artifact write under the per-Work runtime lock.
    ///
    /// An existing `daemon:schedule:<id>` holder (a schedule-driven run owns
    /// the Work from enqueue to terminal) is written under, not re-acquired.
    ///
    /// # Errors
    ///
    /// Returns [`NexusApiError::InvalidInput`] (`field: "rel_child"`) when the
    /// child is absolute or contains a `..` component — refused before any
    /// read, lock or directory creation; [`NexusApiError::Forbidden`] for a
    /// missing active creator, an unknown `work_ref`, any other held lock
    /// (`work_locked:…`, propagated unmapped) or a resolved path escaping the
    /// `Works/<work_ref>/` boundary, and [`NexusApiError::Internal`] for a
    /// lookup, directory or write failure.
    async fn commit(&self, write: WorkArtifactWrite<'_>) -> Result<(), NexusApiError>;

    /// Reconcile one Work's `work_chapters` index with its Markdown tree.
    ///
    /// The read-only diff (filesystem walk + per-chapter DB reads) runs
    /// unlocked; only the write phase — each chapter file's frontmatter status
    /// committed through this port's own byte-write path, then the DB index
    /// operations — holds the per-Work runtime lock. A `dry_run` request
    /// acquires no lock and writes nothing.
    ///
    /// `recheck` is the caller's own principal/workspace validation. It runs
    /// after the unlocked walk and immediately before the lock is acquired, so
    /// the check still covers the walk; a refusal acquires no lock and writes
    /// nothing. The diff may be stale by the time the lock is re-acquired —
    /// that is accepted for the local-first single-writer daemon model (one
    /// active creator, schedule operations serialized through the daemon).
    ///
    /// # Errors
    ///
    /// Returns whatever `recheck` returns, [`NexusApiError::Internal`]
    /// (`DATABASE_ERROR`) for a profile gate, diff or apply failure, and the
    /// lock's own [`NexusApiError::Forbidden`] (`work_locked:…`) when another
    /// holder owns the Work.
    async fn reconcile(
        &self,
        request: WorkReconcile<'_>,
        recheck: &(dyn Fn() -> Result<(), NexusApiError> + Send + Sync),
    ) -> Result<ReconcileReport, NexusApiError>;
}

/// Production [`WorkArtifactPort`]: SQLite-backed Work resolution plus the
/// shared content primitives, built per dispatch from the tool context.
pub(crate) struct FsWorkArtifactPort {
    pool: SqlitePool,
    nexus_home: PathBuf,
}

/// Boundary re-check for one artifact target; runs on the blocking pool.
///
/// Mirrors the guard's canonical walk-up: the nearest existing ancestor must
/// resolve inside `Works/<work_ref>/`, so a symlink anywhere on the way out is
/// refused before any byte lands. Same racy-correct TOCTOU model as
/// `resolve_guarded_path`.
fn check_works_boundary(
    workspace_root: &Path,
    work_ref: &str,
    target: &Path,
) -> Result<(), NexusApiError> {
    let escape = || NexusApiError::Forbidden {
        resource: format!(
            "file: path '{}' escapes the Works/{work_ref}/ boundary",
            target.display()
        ),
    };
    let canonical_root = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf());
    let works_dir = canonical_root.join("Works");
    let allowed = works_dir.join(work_ref);
    // The lexical ancestors a first write may legitimately stop at: with no
    // `Works/<work_ref>` yet, the nearest existing ancestor is the workspace
    // root or its `Works` directory itself.
    let works_path = workspace_root.join("Works");
    let mut probe = target;
    loop {
        match probe.canonicalize() {
            Ok(canonical) => {
                // Inside the Work subtree; or the probe *is* one of those two
                // ancestors, not merely a lower symlink that resolves to one of
                // them (e.g. `Works/demo/Leak -> …/Works`).
                let first_write_ancestor = (probe == workspace_root && canonical == canonical_root)
                    || (probe == works_path && canonical == works_dir);
                if !canonical.starts_with(&allowed) && !first_write_ancestor {
                    return Err(escape());
                }
                return Ok(());
            }
            Err(_) => match probe.parent() {
                Some(parent) => probe = parent,
                None => return Err(escape()),
            },
        }
    }
}

impl FsWorkArtifactPort {
    /// Build the adapter for one dispatch.
    pub(crate) const fn new(pool: SqlitePool, nexus_home: PathBuf) -> Self {
        Self { pool, nexus_home }
    }

    /// Write phase of [`WorkArtifactPort::reconcile`], run while the caller
    /// holds the Work runtime lock: every file resync commits through this
    /// port's own byte-write path, then the DB index operations apply.
    async fn apply_reconcile(
        &self,
        request: &WorkReconcile<'_>,
        diff: &ReconcileDiff,
    ) -> Result<ReconcileReport, NexusApiError> {
        for op in &diff.ops {
            let ReconcileOp::ResyncFileStatus {
                rel_child,
                db_status,
            } = op
            else {
                continue;
            };
            let target = request
                .workspace_root
                .join("Works")
                .join(request.work_ref)
                .join(rel_child);
            let content =
                std::fs::read_to_string(&target).map_err(|e| NexusApiError::Internal {
                    category: format!(
                        "FILE_READ_FAILED: failed to read chapter file {}: {e}",
                        target.display()
                    ),
                })?;
            let Some(rendered) = work_chapters::render_frontmatter_status(&content, db_status)
            else {
                continue;
            };
            self.write_locked(&WorkArtifactWrite {
                workspace_root: request.workspace_root,
                work_ref: request.work_ref,
                rel_child,
                content: &rendered,
            })
            .await?;
        }
        work_chapters::apply_reconcile_diff(&self.pool, request.work_id, request.now, diff)
            .await
            .map_err(|e| NexusApiError::Internal {
                category: format!("DATABASE_ERROR: reconcile apply failed: {e}"),
            })
    }

    /// The write itself, run while the caller holds the Work lock:
    /// boundary re-check → parent creation → durable atomic bytes.
    async fn write_locked(&self, write: &WorkArtifactWrite<'_>) -> Result<(), NexusApiError> {
        let target = write
            .workspace_root
            .join("Works")
            .join(write.work_ref)
            .join(write.rel_child);
        let escape = |target: &Path| NexusApiError::Forbidden {
            resource: format!(
                "file: path '{}' escapes the Works/{}/ boundary",
                target.display(),
                write.work_ref
            ),
        };

        // D8 boundary re-check: the walk is blocking `stat`/`canonicalize` I/O,
        // so it runs on the blocking pool, never on an async worker thread.
        let workspace_root = write.workspace_root.to_path_buf();
        let work_ref = write.work_ref.to_string();
        let probe_target = target.clone();
        tokio::task::spawn_blocking(move || {
            check_works_boundary(&workspace_root, &work_ref, &probe_target)
        })
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!("PATH_GUARD_PANIC: path guard task panicked: {e}"),
        })??;

        let Some(parent) = target.parent() else {
            return Err(escape(&target));
        };
        create_parent_dirs(parent)
            .await
            .map_err(|e| NexusApiError::Internal {
                category: format!(
                    "DIR_CREATE_FAILED: failed to create directory {}: {e}",
                    parent.display()
                ),
            })?;
        fsync_write_atomic(target.clone(), write.content)
            .await
            .map_err(|e| NexusApiError::Internal {
                category: format!(
                    "FILE_WRITE_FAILED: failed to write file {}: {e}",
                    target.display()
                ),
            })?;
        Ok(())
    }
}

impl WorkArtifactPort for FsWorkArtifactPort {
    async fn commit(&self, write: WorkArtifactWrite<'_>) -> Result<(), NexusApiError> {
        // ONE gate for every caller (tool surface, scaffolds, manuscript.write,
        // the executor adapter): `rel_child` must name a path BELOW
        // `Works/<work_ref>/`. An absolute path replaces that base, and a `..`
        // walks out of the Work subtree — and the boundary probe below stops at
        // the first EXISTING ancestor, so with `Works/<work_ref>` absent it
        // would approve a target inside a sibling Work's tree.
        if write.rel_child.is_absolute()
            || write
                .rel_child
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(NexusApiError::InvalidInput {
                field: "rel_child".to_string(),
                reason: format!(
                    "'{}' must be a relative path below Works/{}/",
                    write.rel_child.display(),
                    write.work_ref
                ),
            });
        }
        let Some(creator_id) = read_active_creator_id(&self.nexus_home) else {
            return Err(NexusApiError::Forbidden {
                resource:
                    "tool_execution: active creator required to write Works/<work_ref>/ artifacts"
                        .to_string(),
            });
        };
        // A creator registered without `workspace use` records no slug; the
        // rest of the process resolves that same config to the default
        // partition (`workspace_slug_for_creator`), so the port must agree
        // instead of refusing the creator's own Work. A recorded value that
        // names no partition is an explicit (broken) selection: refuse rather
        // than silently act on the default one the user never chose.
        let workspace_slug = match workspace_slug_selection(&self.nexus_home, &creator_id) {
            WorkspaceSlugSelection::Unselected(default_slug) => default_slug,
            WorkspaceSlugSelection::Recorded(slug) => {
                if let Err(reason) = validate_slug_name(&slug) {
                    return Err(NexusApiError::Forbidden {
                        resource: format!(
                            "tool_execution: active workspace required to write \
                             Works/<work_ref>/ artifacts: recorded workspace '{slug}' {reason}"
                        ),
                    });
                }
                slug
            }
        };
        let work_id = works::resolve_work_id_by_ref_or_id(
            &self.pool,
            &creator_id,
            &workspace_slug,
            write.work_ref,
        )
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!("DATABASE_ERROR: work_ref lookup failed: {e}"),
        })?
        .ok_or_else(|| NexusApiError::Forbidden {
            resource: format!(
                "work '{}' has no works row in the active workspace",
                write.work_ref
            ),
        })?;

        // Inspector BUG-01: a schedule-driven run holds `daemon:schedule:<id>`
        // from enqueue until the schedule is terminal, so acquiring here would
        // always refuse the Work's own writes. That holder is the Work's single
        // writer for its lifetime: write under it and leave it untouched.
        let current_holder =
            nexus_local_db::runtime_lock::current_holder(&self.pool, &creator_id, &work_id)
                .await
                .map_err(|e| NexusApiError::Internal {
                    category: format!("DATABASE_ERROR: runtime_lock holder read failed: {e}"),
                })?;
        // F-002 residual: this exemption trusts the stored holder prefix, not
        // the calling run's identity, so it covers every tool writer.
        if current_holder.as_deref().is_some_and(|holder| {
            holder.starts_with(nexus_local_db::runtime_lock::SCHEDULE_HOLDER_PREFIX)
        }) {
            return self.write_locked(&write).await;
        }

        // Not reentrant: every acquire mints a fresh holder uuid, so any
        // existing fresh holder — including this process's own in-flight
        // `tool:fs_write` — refuses with the lock's own `work_locked:` reason.
        let lock = WorkLock::acquire(&self.pool, &creator_id, &work_id, "tool:fs_write").await?;
        let result = self.write_locked(&write).await;
        // No `Drop` guard: a skipped release pins the Work until the TTL.
        lock.release().await;
        result
    }

    async fn reconcile(
        &self,
        request: WorkReconcile<'_>,
        recheck: &(dyn Fn() -> Result<(), NexusApiError> + Send + Sync),
    ) -> Result<ReconcileReport, NexusApiError> {
        // `Stories/` is novel-only: refuse an explicitly non-novel profile
        // before any walk, lock or write.
        work_chapters::ensure_novel_profile(&self.pool, request.work_id)
            .await
            .map_err(|e| NexusApiError::Internal {
                category: format!("DATABASE_ERROR: reconcile profile gate failed: {e}"),
            })?;

        // Phase A (unlocked): the slow filesystem walk + per-chapter reads.
        let diff = work_chapters::compute_reconcile_diff(
            &self.pool,
            request.work_id,
            request.work_ref,
            request.workspace_root,
        )
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!("DATABASE_ERROR: reconcile diff failed: {e}"),
        })?;
        if request.dry_run {
            tracing::info!(work_id = %request.work_id, work_ref = %request.work_ref, "dry-run reconcile");
            return Ok(diff.to_report());
        }

        // Phase B (locked): only the writes, so the window stays short. A
        // read-phase failure above never acquires the lock at all. The
        // unlocked walk is the stale-read window, so the caller's
        // principal/workspace check re-runs immediately before the lock: a
        // refusal takes no lock and writes nothing.
        recheck()?;
        let lock = RuntimeLockGuard::acquire(
            &self.pool,
            request.creator_id,
            request.work_id,
            request.holder_kind,
        )
        .await
        .map_err(NexusApiError::from)?;
        let acquired_at = chrono::Utc::now();
        tracing::info!(
            work_id = %request.work_id,
            acquired_at = %acquired_at.to_rfc3339(),
            pending_ops = diff.ops.len(),
            "runtime_lock: acquired for reconcile write phase"
        );

        let result = self.apply_reconcile(&request, &diff).await;

        // V1.48 P4-fix1 (W-2 qc3): release on both Ok and Err paths — a failed
        // reconcile must never leave the Work unwritable until the TTL.
        lock.release().await;
        let held_ms = (chrono::Utc::now() - acquired_at).num_milliseconds().max(0);
        tracing::info!(
            work_id = %request.work_id,
            held_ms,
            "runtime_lock: released after reconcile write phase"
        );
        result
    }
}

/// One recorded intended write in a [`RecordingWorkArtifactPort`] log.
#[cfg_attr(not(test), allow(dead_code))]
// unused until the first Simulate execution plane selects the stub at the per-dispatch construction point (plan v235-work-ir-p3 reopen condition)
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct RecordedMutation {
    /// The Work directory name the write addressed.
    pub(crate) work_ref: String,
    /// The artifact path below the Work root.
    pub(crate) rel_child: String,
    /// Byte length of the content that *would* have been written.
    pub(crate) bytes: usize,
    /// sha256 hex digest of the content (same convention as the tree digest).
    pub(crate) sha256: String,
}

/// Simulate/dry-run [`WorkArtifactPort`]: records intended mutations in
/// commit order and performs no filesystem, SQLite or lock operation.
///
/// Never constructed on a production path; a future Simulate execution plane
/// selects it at the per-dispatch construction point (`capabilities.rs`,
/// `FsWorkArtifactPort::new`) so one selection covers every Work write.
#[cfg_attr(not(test), allow(dead_code))] // unused until the first Simulate execution plane selects the stub at the per-dispatch construction point (plan v235-work-ir-p3 reopen condition)
pub(crate) struct RecordingWorkArtifactPort {
    mutations: std::sync::Mutex<Vec<RecordedMutation>>,
}

#[cfg_attr(not(test), allow(dead_code))] // unused until the first Simulate execution plane selects the stub at the per-dispatch construction point (plan v235-work-ir-p3 reopen condition)
impl RecordingWorkArtifactPort {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            mutations: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Snapshot of the log in commit order.
    #[must_use]
    pub(crate) fn mutations(&self) -> Vec<RecordedMutation> {
        self.mutations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

#[cfg_attr(not(test), allow(dead_code))] // unused until the first Simulate execution plane selects the stub (plan v235-work-ir-p3 reopen condition)
impl WorkArtifactPort for RecordingWorkArtifactPort {
    // The trait signature is async; the stub records synchronously.
    #[allow(clippy::unused_async_trait_impl)]
    async fn commit(&self, write: WorkArtifactWrite<'_>) -> Result<(), NexusApiError> {
        let mut sha = Sha256::new();
        sha.update(write.content.as_bytes());
        let record = RecordedMutation {
            work_ref: write.work_ref.to_string(),
            rel_child: write.rel_child.display().to_string(),
            bytes: write.content.len(),
            sha256: hex::encode(sha.finalize()),
        };
        self.mutations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(record);
        Ok(())
    }

    // Reconcile diffs the tree against the DB index; this stub performs no
    // read, so it refuses rather than reporting an invented empty diff.
    #[allow(clippy::unused_async_trait_impl)]
    async fn reconcile(
        &self,
        _request: WorkReconcile<'_>,
        _recheck: &(dyn Fn() -> Result<(), NexusApiError> + Send + Sync),
    ) -> Result<ReconcileReport, NexusApiError> {
        Err(NexusApiError::Internal {
            category: "SIMULATE_UNSUPPORTED: reconcile requires the DB index and the recording \
                       port performs no read"
                .to_string(),
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use nexus_local_db::works::WorkRecord;
    use tempfile::TempDir;

    const CREATOR: &str = "ctr_test";
    const WORK_ID: &str = "wrk_demo";
    const WORK_REF: &str = "demo";

    struct Fixture {
        _home: TempDir,
        workspace: TempDir,
        pool: SqlitePool,
        port: FsWorkArtifactPort,
    }

    fn work_record() -> WorkRecord {
        WorkRecord {
            work_id: WORK_ID.to_string(),
            creator_id: CREATOR.to_string(),
            workspace_slug: "default".to_string(),
            status: "draft".to_string(),
            title: "My Novel".to_string(),
            long_term_goal: "Write a great novel".to_string(),
            initial_idea: "A sci-fi thriller".to_string(),
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

    async fn fixture(seed_work: bool) -> Fixture {
        fixture_selecting_workspace(seed_work, Some("default")).await
    }

    /// `active_slug: None` writes a config that records no workspace selection.
    async fn fixture_selecting_workspace(seed_work: bool, active_slug: Option<&str>) -> Fixture {
        let home = tempfile::tempdir().expect("temp home");
        let workspace = tempfile::tempdir().expect("temp workspace");
        let selection = active_slug.map_or_else(String::new, |slug| {
            format!("active_workspace_slug_by_creator = {{ {CREATOR} = \"{slug}\" }}\n")
        });
        let config = format!("active_creator_id = \"{CREATOR}\"\n{selection}");
        std::fs::write(home.path().join("config.toml"), config).expect("cli config");
        let pool = nexus_local_db::init_engine_pool(&home.path().join("state.db"))
            .await
            .expect("engine pool")
            .clone_pool();
        if seed_work {
            works::create_work(&pool, &work_record())
                .await
                .expect("works row");
        }
        let port = FsWorkArtifactPort::new(pool.clone(), home.path().to_path_buf());
        Fixture {
            _home: home,
            workspace,
            pool,
            port,
        }
    }

    fn write<'a>(
        fixture: &'a Fixture,
        rel_child: &'a Path,
        content: &'a str,
    ) -> WorkArtifactWrite<'a> {
        WorkArtifactWrite {
            workspace_root: fixture.workspace.path(),
            work_ref: WORK_REF,
            rel_child,
            content,
        }
    }

    async fn holder(pool: &SqlitePool) -> Option<String> {
        sqlx::query_scalar("SELECT runtime_lock_holder FROM works WHERE work_id = ?")
            .bind(WORK_ID)
            .fetch_one(pool)
            .await
            .expect("holder read")
    }

    fn entry_names(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .expect("read_dir")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    fn chapter_path(workspace: &Path, file: &str) -> PathBuf {
        workspace.join("Works/demo/Stories").join(file)
    }

    fn write_chapter(path: &Path, status: &str, body: &str) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("stories dir");
        std::fs::write(
            path,
            format!("---\nchapter: 1\nstatus: {status}\n---\n{body}"),
        )
        .expect("chapter file");
    }

    async fn seed_chapter(pool: &SqlitePool, status: &str) {
        nexus_local_db::work_chapters::seed_chapters(
            pool,
            WORK_ID,
            WORK_REF,
            1,
            "2026-06-04T10:00:00Z",
        )
        .await
        .expect("seed chapters");
        nexus_local_db::work_chapters::update_status(
            pool,
            WORK_ID,
            1,
            1,
            status,
            None,
            "2026-06-04T10:00:00Z",
        )
        .await
        .expect("chapter status");
    }

    fn reconcile(fixture: &Fixture, dry_run: bool) -> WorkReconcile<'_> {
        WorkReconcile {
            workspace_root: fixture.workspace.path(),
            creator_id: CREATOR,
            work_id: WORK_ID,
            work_ref: WORK_REF,
            now: "2026-06-04T11:00:00Z",
            holder_kind: "core",
            dry_run,
        }
    }

    #[test]
    fn classify_work_path_matches_the_documented_shapes() {
        assert!(matches!(
            classify_work_path("notes/plain.md"),
            WorkPath::NonWork
        ));
        assert!(matches!(classify_work_path("Works"), WorkPath::NonWork));
        assert!(matches!(classify_work_path("Works/"), WorkPath::NonWork));
        assert!(matches!(
            classify_work_path("/Works/demo/x.md"),
            WorkPath::NonWork
        ));
        // A case-variant `Works` is refused, never routed: the port writes the
        // literal `Works` tree and the read path refuses the same spelling.
        assert!(matches!(
            classify_work_path("works/demo/x.md"),
            WorkPath::CaseVariantWorks
        ));
        assert!(matches!(
            classify_work_path("WORKS/demo/x.md"),
            WorkPath::CaseVariantWorks
        ));
        assert!(matches!(
            classify_work_path("Works/demo"),
            WorkPath::WorkRoot { work_ref: "demo" }
        ));
        assert!(matches!(
            classify_work_path("Works/demo/"),
            WorkPath::WorkRoot { work_ref: "demo" }
        ));
        assert!(matches!(
            classify_work_path("Works/demo/../plain.md"),
            WorkPath::EscapedWork { work_ref: "demo" }
        ));
        match classify_work_path("Works/demo/Stories/ch1.md") {
            WorkPath::Artifact {
                work_ref,
                rel_child,
            } => {
                assert_eq!(work_ref, "demo");
                assert_eq!(rel_child, Path::new("Stories/ch1.md"));
            }
            other => panic!("expected an artifact, got {other:?}"),
        }
        // A leading `./` is not part of the path's identity: Rust keeps it as a
        // component, so classifying it away is what stops `./Works/…` from
        // falling through as `NonWork` and skipping the port.
        assert!(matches!(
            classify_work_path("./notes/plain.md"),
            WorkPath::NonWork
        ));
        assert!(matches!(classify_work_path("./Works"), WorkPath::NonWork));
        assert!(matches!(
            classify_work_path("./works/demo/x.md"),
            WorkPath::CaseVariantWorks
        ));
        assert!(matches!(
            classify_work_path("./Works/demo"),
            WorkPath::WorkRoot { work_ref: "demo" }
        ));
        match classify_work_path("./Works/demo/Stories/ch1.md") {
            WorkPath::Artifact {
                work_ref,
                rel_child,
            } => {
                assert_eq!(work_ref, "demo");
                assert_eq!(rel_child, Path::new("Stories/ch1.md"));
            }
            other => panic!("expected an artifact, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn commit_writes_the_artifact_durably_and_creates_missing_parents() {
        let fixture = fixture(true).await;
        let rel_child = Path::new("Stories/volume-1/ch1.md");

        fixture
            .port
            .commit(write(&fixture, rel_child, "# Chapter 1\n"))
            .await
            .expect("commit");

        let target = fixture
            .workspace
            .path()
            .join("Works/demo/Stories/volume-1/ch1.md");
        assert_eq!(
            std::fs::read_to_string(&target).expect("read"),
            "# Chapter 1\n"
        );
        assert_eq!(
            entry_names(target.parent().expect("parent")),
            vec!["ch1.md".to_string()],
            "the atomic temp file is cleaned up"
        );
        assert_eq!(holder(&fixture.pool).await, None, "the lock is released");
    }

    #[tokio::test]
    async fn commit_without_a_works_row_is_forbidden() {
        let fixture = fixture(false).await;

        let error = fixture
            .port
            .commit(write(&fixture, Path::new("Stories/ch1.md"), "body"))
            .await
            .expect_err("a missing works row refuses");

        match error {
            NexusApiError::Forbidden { resource } => {
                assert!(resource.contains("has no works row"), "{resource}");
            }
            other => panic!("expected Forbidden, got {other:?}"),
        }
        assert!(!fixture.workspace.path().join("Works/demo").exists());
    }

    #[tokio::test]
    async fn commit_refuses_an_unusable_recorded_workspace_selection() {
        // A recorded value that names no partition is the user's explicit
        // (broken) selection: the port refuses rather than acting on the
        // default partition they never chose. An empty record is *not* this
        // case — it is the "never chose" state (`Unselected`).
        let fixture = fixture_selecting_workspace(true, Some("..")).await;

        let error = fixture
            .port
            .commit(write(&fixture, Path::new("Stories/ch1.md"), "body"))
            .await
            .expect_err("an unusable recorded selection refuses");

        match error {
            NexusApiError::Forbidden { resource } => {
                assert!(resource.contains("active workspace required"), "{resource}");
            }
            other => panic!("expected Forbidden, got {other:?}"),
        }
        assert!(
            !fixture.workspace.path().join("Works").exists(),
            "no byte or directory was created"
        );
        assert_eq!(
            holder(&fixture.pool).await,
            None,
            "no lock was taken for the refused Work"
        );
    }

    #[tokio::test]
    async fn commit_writes_without_a_recorded_workspace_selection() {
        // A creator registered without `workspace use` records no slug, and a
        // blank recorded value is the same "never chose" state. The rest of
        // the process resolves both to the default partition
        // (`workspace_slug_for_creator`), so the port must agree instead of
        // refusing the creator's own Work.
        for active_slug in [None, Some("")] {
            let fixture = fixture_selecting_workspace(true, active_slug).await;

            fixture
                .port
                .commit(write(
                    &fixture,
                    Path::new("Stories/ch1.md"),
                    "# Chapter 1\n",
                ))
                .await
                .expect("no recorded selection falls back to the default partition");

            assert_eq!(
                std::fs::read_to_string(fixture.workspace.path().join("Works/demo/Stories/ch1.md"))
                    .expect("read"),
                "# Chapter 1\n"
            );
            assert_eq!(holder(&fixture.pool).await, None, "the lock is released");
        }
    }

    #[tokio::test]
    async fn commit_refuses_a_child_that_leaves_the_work_tree() {
        let fixture = fixture(true).await;
        let absolute_child = fixture.workspace.path().join("outside.md");

        // The port is the single gate every caller shares, so both shapes are
        // refused here — before any Work lookup, lock or directory creation.
        for rel_child in [
            PathBuf::from("../sibling/Stories/ch1.md"),
            absolute_child.clone(),
        ] {
            let error = fixture
                .port
                .commit(write(&fixture, rel_child.as_path(), "body"))
                .await
                .expect_err("a child outside Works/<work_ref>/ refuses");
            match error {
                NexusApiError::InvalidInput { field, reason } => {
                    assert_eq!(field, "rel_child");
                    assert!(reason.contains("Works/demo/"), "{reason}");
                }
                other => panic!("expected InvalidInput, got {other:?}"),
            }
        }

        assert!(
            !fixture.workspace.path().join("Works").exists(),
            "no directory may be created for a refused child"
        );
        assert!(
            !absolute_child.exists(),
            "an absolute child may not be written"
        );
        assert_eq!(
            holder(&fixture.pool).await,
            None,
            "the refusal precedes the lock"
        );
    }

    #[tokio::test]
    async fn commit_refuses_a_foreign_lock_and_keeps_its_holder() {
        let fixture = fixture(true).await;
        nexus_local_db::acquire_runtime_lock(
            &fixture.pool,
            CREATOR,
            WORK_ID,
            "cli:http:foreign",
            nexus_local_db::DEFAULT_RUNTIME_LOCK_TTL_SECS,
            false,
        )
        .await
        .expect("foreign lock");

        let error = fixture
            .port
            .commit(write(&fixture, Path::new("Stories/ch1.md"), "body"))
            .await
            .expect_err("a held lock refuses");

        match error {
            NexusApiError::Forbidden { resource } => {
                assert!(resource.contains("work_locked"), "{resource}");
                assert!(resource.contains("cli:http:foreign"), "{resource}");
            }
            other => panic!("expected Forbidden, got {other:?}"),
        }
        assert_eq!(
            holder(&fixture.pool).await.as_deref(),
            Some("cli:http:foreign"),
            "the foreign holder is untouched"
        );
    }

    #[tokio::test]
    async fn commit_writes_under_a_schedule_holder_and_preserves_it() {
        let fixture = fixture(true).await;
        let schedule_holder = nexus_local_db::runtime_lock::schedule_holder("SCH20260611120000");
        nexus_local_db::acquire_runtime_lock(
            &fixture.pool,
            CREATOR,
            WORK_ID,
            &schedule_holder,
            nexus_local_db::DEFAULT_RUNTIME_LOCK_TTL_SECS,
            true,
        )
        .await
        .expect("schedule lock");

        fixture
            .port
            .commit(write(
                &fixture,
                Path::new("Stories/ch1.md"),
                "# Chapter 1\n",
            ))
            .await
            .expect("a schedule-driven run writes under its own holder");

        assert_eq!(
            std::fs::read_to_string(fixture.workspace.path().join("Works/demo/Stories/ch1.md"))
                .expect("read"),
            "# Chapter 1\n"
        );
        assert_eq!(
            holder(&fixture.pool).await.as_deref(),
            Some(schedule_holder.as_str()),
            "the schedule holder is neither re-acquired nor released"
        );
    }

    #[tokio::test]
    async fn commit_releases_the_lock_when_the_write_fails() {
        let fixture = fixture(true).await;
        // A regular file where the artifact's parent directory must go makes
        // `create_dir_all` fail after the lock is held.
        std::fs::create_dir_all(fixture.workspace.path().join("Works/demo")).expect("work dir");
        std::fs::write(fixture.workspace.path().join("Works/demo/Stories"), b"file")
            .expect("blocking file");

        let error = fixture
            .port
            .commit(write(&fixture, Path::new("Stories/ch1.md"), "body"))
            .await
            .expect_err("the write fails");

        match error {
            NexusApiError::Internal { category } => {
                assert!(category.starts_with("DIR_CREATE_FAILED"), "{category}");
            }
            other => panic!("expected Internal, got {other:?}"),
        }
        assert_eq!(
            holder(&fixture.pool).await,
            None,
            "the lock is released on the error path"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn commit_refuses_symlinks_aliasing_the_first_write_ancestors() {
        // A lower symlink resolving to the workspace root or its `Works`
        // directory must not borrow the first-write ancestor exception and
        // create bytes outside the addressed Work.
        for link_to_works_dir in [false, true] {
            let fixture = fixture(true).await;
            std::fs::create_dir_all(fixture.workspace.path().join("Works/demo")).expect("work dir");
            let link_target = if link_to_works_dir {
                fixture.workspace.path().join("Works")
            } else {
                fixture.workspace.path().to_path_buf()
            };
            std::os::unix::fs::symlink(
                &link_target,
                fixture.workspace.path().join("Works/demo/Leak"),
            )
            .expect("symlink");

            let error = fixture
                .port
                .commit(write(&fixture, Path::new("Leak/notes/x.md"), "body"))
                .await
                .expect_err("an ancestor alias refuses");

            match error {
                NexusApiError::Forbidden { resource } => {
                    assert!(
                        resource.contains("escapes the Works/demo/ boundary"),
                        "{resource}"
                    );
                }
                other => panic!("expected Forbidden, got {other:?}"),
            }
            assert_eq!(
                entry_names(&fixture.workspace.path().join("Works")),
                vec!["demo".to_string()],
                "no sibling Work entry was created through the alias"
            );
            assert!(
                !fixture.workspace.path().join("notes").exists(),
                "no workspace-root byte was created through the alias"
            );
            assert!(
                !fixture.workspace.path().join("Works/demo/notes").exists(),
                "no Work byte was created below the refused alias"
            );
            assert_eq!(
                holder(&fixture.pool).await,
                None,
                "the lock is released on the refusal path"
            );
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn commit_refuses_a_symlink_out_of_the_work_subtree() {
        let fixture = fixture(true).await;
        let outside = tempfile::tempdir().expect("outside dir");
        std::fs::create_dir_all(fixture.workspace.path().join("Works/demo")).expect("work dir");
        std::os::unix::fs::symlink(
            outside.path(),
            fixture.workspace.path().join("Works/demo/Leak"),
        )
        .expect("symlink");

        let error = fixture
            .port
            .commit(write(&fixture, Path::new("Leak/pwned.md"), "body"))
            .await
            .expect_err("an escaping symlink refuses");

        match error {
            NexusApiError::Forbidden { resource } => {
                assert!(
                    resource.contains("escapes the Works/demo/ boundary"),
                    "{resource}"
                );
            }
            other => panic!("expected Forbidden, got {other:?}"),
        }
        assert!(!outside.path().join("pwned.md").exists());
        assert_eq!(
            holder(&fixture.pool).await,
            None,
            "the lock is released on the refusal path"
        );
    }

    /// The `fs/write_text_file` production route: the tool surface classifies
    /// a raw `Works/<ref>/…` path and hands the artifact shape to the
    /// production adapter, which lands the real bytes on disk.
    #[tokio::test]
    async fn production_adapter_lands_real_bytes_for_a_work_tool_path() {
        let fixture = fixture(true).await;
        let work_path = match classify_work_path("Works/demo/drafts/ch1.md") {
            WorkPath::Artifact {
                work_ref,
                rel_child,
            } => (work_ref, rel_child),
            other => panic!("expected an artifact, got {other:?}"),
        };
        let content = "# Chapter 1\n";
        let write = WorkArtifactWrite {
            workspace_root: fixture.workspace.path(),
            work_ref: work_path.0,
            rel_child: work_path.1,
            content,
        };

        fixture.port.commit(write).await.expect("production commit");

        let target = fixture.workspace.path().join("Works/demo/drafts/ch1.md");
        let on_disk = std::fs::read(&target).expect("read bytes");
        assert_eq!(on_disk, content.as_bytes());
        assert_eq!(holder(&fixture.pool).await, None, "the lock is released");
    }

    #[tokio::test]
    async fn recording_port_leaves_the_workspace_digest_unchanged() {
        // Digest convention: sha256 over the sorted relative file paths of the
        // workspace, joined by '\n'. An empty workspace hashes the empty input.
        fn tree_digest(root: &Path) -> String {
            fn walk(root: &Path, prefix: &str, paths: &mut Vec<String>) {
                for entry in std::fs::read_dir(root).expect("read_dir") {
                    let entry = entry.expect("entry");
                    let rel = format!("{prefix}{}", entry.file_name().to_string_lossy());
                    if entry.file_type().expect("file_type").is_dir() {
                        walk(&entry.path(), &format!("{rel}/"), paths);
                    } else {
                        paths.push(rel);
                    }
                }
            }
            let mut paths = Vec::new();
            walk(root, "", &mut paths);
            paths.sort();
            let mut sha = Sha256::new();
            sha.update(paths.join("\n"));
            hex::encode(sha.finalize())
        }

        let workspace = tempfile::tempdir().expect("temp workspace");
        // A populated tree proves `tree_digest` actually hashes the listing:
        // one non-Work file makes the digest the sha256 of "notes/a.md"
        // instead of the empty input, so a degenerate (always-empty) digest
        // cannot pass vacuously.
        std::fs::create_dir_all(workspace.path().join("notes")).expect("notes dir");
        std::fs::write(workspace.path().join("notes/a.md"), b"kept").expect("seed file");
        let before = tree_digest(workspace.path());
        assert_eq!(
            before, "b33a8099b01219568f1660eb604d897c0ac5db4fdd84c142337f129d5a0fca4d",
            // sha256("notes/a.md")
            "digest reflects the populated listing"
        );
        assert!(
            !workspace.path().join("Works").exists(),
            "absent before the stub runs"
        );

        let port = RecordingWorkArtifactPort::new();
        port.commit(WorkArtifactWrite {
            workspace_root: workspace.path(),
            work_ref: WORK_REF,
            rel_child: Path::new("drafts/ch1.md"),
            content: "# Chapter 1\n",
        })
        .await
        .expect("recorded commit");
        assert_eq!(port.mutations().len(), 1);

        assert!(
            !workspace.path().join("Works").exists(),
            "absent after the stub runs"
        );
        // Same digest before and after: the stub wrote nothing, so the sorted
        // rel-path list is still exactly ["notes/a.md"].
        assert_eq!(before, tree_digest(workspace.path()), "digest unchanged");
    }

    #[tokio::test]
    async fn recording_port_logs_one_mutation_and_touches_nothing() {
        let workspace = tempfile::tempdir().expect("temp workspace");
        let port = RecordingWorkArtifactPort::new();
        let content = "# chapter one\n";
        port.commit(WorkArtifactWrite {
            workspace_root: workspace.path(),
            work_ref: WORK_REF,
            rel_child: Path::new("drafts/ch1.md"),
            content,
        })
        .await
        .expect("recorded commit");

        let mutations = port.mutations();
        assert_eq!(mutations.len(), 1);
        let record = &mutations[0];
        assert_eq!(record.work_ref, WORK_REF);
        assert_eq!(record.rel_child, "drafts/ch1.md");
        assert_eq!(record.bytes, content.len());
        // sha256("# chapter one\n") — precomputed, not re-mirrored from the stub.
        assert_eq!(
            record.sha256,
            "9ae3ccb02e53b1e85a04d509e94a0658cdb813d2136b91fea0a4b906ad1ca6f2"
        );

        // Zero filesystem change: not even the `Works/` directory exists.
        assert!(!workspace.path().join("Works").exists());
    }

    #[tokio::test]
    async fn reconcile_resyncs_a_conflicting_chapter_and_releases_the_lock() {
        let fixture = fixture(true).await;
        seed_chapter(&fixture.pool, "draft").await;
        let path = chapter_path(fixture.workspace.path(), "ch01-intro.md");
        write_chapter(&path, "finalized", "Body text");

        let report = fixture
            .port
            .reconcile(reconcile(&fixture, false), &|| Ok(()))
            .await
            .expect("reconcile");

        assert_eq!(report.resynced, 1, "the conflicting file is re-synced");
        assert_eq!(report.created, 0);
        assert_eq!(report.updated, 0);
        assert_eq!(report.preserved, 0);
        let content = std::fs::read_to_string(&path).expect("read");
        assert!(content.contains("status: draft"), "{content}");
        assert!(content.contains("Body text"), "{content}");
        assert_eq!(holder(&fixture.pool).await, None, "the lock is released");
    }

    #[tokio::test]
    async fn reconcile_dry_run_reports_the_diff_and_writes_nothing() {
        let fixture = fixture(true).await;
        seed_chapter(&fixture.pool, "draft").await;
        let conflicting = chapter_path(fixture.workspace.path(), "ch01-intro.md");
        write_chapter(&conflicting, "finalized", "Body text");
        write_chapter(
            &chapter_path(fixture.workspace.path(), "ch02-body.md"),
            "not_started",
            "Second body",
        );
        let before = std::fs::read_to_string(&conflicting).expect("read");

        let report = fixture
            .port
            .reconcile(reconcile(&fixture, true), &|| Ok(()))
            .await
            .expect("preview");

        assert_eq!(report.resynced, 1);
        assert_eq!(report.created, 1);
        assert_eq!(
            std::fs::read_to_string(&conflicting).expect("read"),
            before,
            "dry-run writes no frontmatter bytes"
        );
        assert!(
            nexus_local_db::work_chapters::get_chapter(&fixture.pool, WORK_ID, 2, 1)
                .await
                .expect("chapter read")
                .is_none(),
            "dry-run applies no DB index op"
        );
        assert_eq!(holder(&fixture.pool).await, None, "dry-run takes no lock");
    }

    /// The principal is re-validated after the unlocked walk and before the
    /// lock: a refusal there takes no lock and writes nothing, even though the
    /// diff already named a write.
    #[tokio::test]
    async fn reconcile_refuses_before_the_lock_when_the_recheck_fails() {
        let fixture = fixture(true).await;
        seed_chapter(&fixture.pool, "draft").await;
        let path = chapter_path(fixture.workspace.path(), "ch01-intro.md");
        write_chapter(&path, "finalized", "Body text");
        let before = std::fs::read_to_string(&path).expect("read");

        let result = fixture
            .port
            .reconcile(reconcile(&fixture, false), &|| {
                Err(NexusApiError::AuthRequired)
            })
            .await;

        assert!(
            matches!(result, Err(NexusApiError::AuthRequired)),
            "the recheck's own refusal propagates"
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            before,
            "no frontmatter byte is written"
        );
        assert_eq!(
            holder(&fixture.pool).await,
            None,
            "no lock is acquired for the refused admission"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn reconcile_releases_the_lock_when_the_resync_write_fails() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = fixture(true).await;
        seed_chapter(&fixture.pool, "draft").await;
        let stories = fixture.workspace.path().join("Works/demo/Stories");
        write_chapter(&stories.join("ch01-intro.md"), "finalized", "Body text");
        std::fs::set_permissions(&stories, std::fs::Permissions::from_mode(0o555))
            .expect("read-only Stories/");

        let result = fixture
            .port
            .reconcile(reconcile(&fixture, false), &|| Ok(()))
            .await;

        std::fs::set_permissions(&stories, std::fs::Permissions::from_mode(0o755))
            .expect("restore Stories/ permissions");
        assert!(
            result.is_err(),
            "a read-only Stories/ must fail the write phase"
        );
        assert_eq!(
            holder(&fixture.pool).await,
            None,
            "the lock is released on the error path"
        );
    }
}
