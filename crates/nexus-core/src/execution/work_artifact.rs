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
use nexus_local_db::works;
use sqlx::SqlitePool;

use crate::content::{create_parent_dirs, fsync_write_atomic, WorkLock};
use crate::error::CoreError as NexusApiError;
use crate::slug::validate_slug_name;

/// A raw tool path classified against the `Works/<work_ref>/` shape.
#[derive(Debug)]
pub(crate) enum WorkPath<'a> {
    /// Not under `Works/` — the caller keeps its existing guarded write.
    NonWork,
    /// A case variant of `Works` (`works/…`, `WORKS/…`): the port writes the
    /// literal `Works` tree and the read path never classifies, so a different
    /// spelling is refused instead of routed.
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
    /// Returns [`NexusApiError::Forbidden`] for a missing active creator, an
    /// unknown `work_ref`, any other held lock (`work_locked:…`, propagated
    /// unmapped) or a resolved path escaping the `Works/<work_ref>/` boundary,
    /// and [`NexusApiError::Internal`] for a lookup, directory or write failure.
    async fn commit(&self, write: WorkArtifactWrite<'_>) -> Result<(), NexusApiError>;
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
}
