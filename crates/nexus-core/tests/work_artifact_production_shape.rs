//! Production-shape proof for the Work artifact seam (v235 P2 fix round).
//!
//! The production registry arms build every scaffold with the DEFAULT `Works`
//! root — a *relative* path — so the scaffold resolves it against the
//! executor's own workspace root, and a first scaffold starts with
//! `works.work_ref` NULL. Both facts are exercised here against the REAL port
//! (`WorkspaceCommitExecutor` → `FsWorkArtifactPort`), not a recording double:
//! the bytes must land under the executor's canonical workspace root while the
//! scaffold itself only knows `Works`.
//!
//! The process cwd is moved into a separate temp directory and never restored
//! until the guard drops: the scaffolds must resolve their `Works/` tree from
//! the executor, so a cwd-relative read, write or rollback shows up as a
//! leaked tree without the expected canonical bytes.

#![cfg(feature = "execution")]
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use nexus_capability::builtins::{
    EssayProjectScaffold, GameBibleProjectScaffold, NovelProjectScaffold, ScriptProjectScaffold,
};
use nexus_capability::{Capability, CapabilityError, WorkspaceExecutor};
use nexus_contracts::local::orchestration::{
    WorkArtifactCommitInput, WorkArtifactCommitOutput, WorkspaceCommitInput, WorkspaceCommitOutput,
    WorkspaceOpenInput, WorkspaceOpenOutput,
};
use nexus_core::owner::executor::WorkspaceCommitExecutor;
use nexus_execution::session::WorkspaceSessionManager;
use nexus_local_db::works::{self, WorkRecord};
use serde_json::json;
use sqlx::SqlitePool;

const CREATOR: &str = "ctr_prod_shape";
const WORKSPACE: &str = "default";

/// Restores the process cwd on drop, so a panicking assertion cannot leak the
/// temp directory (deleted on drop) into the other tests.
struct CwdGuard {
    previous: PathBuf,
}

impl CwdGuard {
    fn enter(dir: &Path) -> Self {
        let previous = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(dir).expect("chdir");
        Self { previous }
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        if let Err(err) = std::env::set_current_dir(&self.previous) {
            eprintln!("cwd restore failed: {err}");
        }
    }
}

/// The real executor, refusing one named artifact after the earlier ones
/// landed — the failure a later port commit produces.
struct FailOnChild {
    inner: Arc<dyn WorkspaceExecutor>,
    fail_on: &'static str,
}

#[async_trait]
impl WorkspaceExecutor for FailOnChild {
    fn workspace_root(&self) -> Result<PathBuf, CapabilityError> {
        self.inner.workspace_root()
    }

    async fn open(
        &self,
        input: WorkspaceOpenInput,
    ) -> Result<WorkspaceOpenOutput, CapabilityError> {
        self.inner.open(input).await
    }

    async fn commit(
        &self,
        input: WorkspaceCommitInput,
    ) -> Result<WorkspaceCommitOutput, CapabilityError> {
        self.inner.commit(input).await
    }

    async fn commit_work_artifact(
        &self,
        input: WorkArtifactCommitInput,
    ) -> Result<WorkArtifactCommitOutput, CapabilityError> {
        if input.rel_child == self.fail_on {
            return Err(CapabilityError::Forbidden(format!(
                "injected refusal for {}",
                input.rel_child
            )));
        }
        self.inner.commit_work_artifact(input).await
    }
}

/// The pieces one production-shaped run needs: the CLI home the port resolves
/// the active creator/workspace from, the canonical workspace root the executor
/// was built for, the unrelated cwd, and the engine pool both halves share.
struct Fixture {
    pool: Arc<SqlitePool>,
    _home: tempfile::TempDir,
    cwd: tempfile::TempDir,
    root: tempfile::TempDir,
    executor: Arc<dyn WorkspaceExecutor>,
}

async fn fixture() -> Fixture {
    let (home, pool) = home_and_pool().await;
    let cwd = tempfile::tempdir().expect("temp cwd");
    let root = tempfile::tempdir().expect("temp workspace");
    let executor: Arc<dyn WorkspaceExecutor> = Arc::new(
        WorkspaceCommitExecutor::new(
            Arc::new(WorkspaceSessionManager::new(Arc::clone(&pool))),
            root.path().to_string_lossy().into_owned(),
        )
        .with_work_artifact_home(home.path().to_path_buf()),
    );
    Fixture {
        _home: home,
        cwd,
        root,
        pool,
        executor,
    }
}

/// The CLI home with the active creator/workspace selection, plus the engine
/// pool holding the creator row the scaffold's World creation is FK-bound to.
async fn home_and_pool() -> (tempfile::TempDir, Arc<SqlitePool>) {
    let home = tempfile::tempdir().expect("temp home");
    let config = format!(
        "active_creator_id = \"{CREATOR}\"\n\
         active_workspace_slug_by_creator = {{ {CREATOR} = \"{WORKSPACE}\" }}\n"
    );
    std::fs::write(home.path().join("config.toml"), config).expect("cli config");
    let pool = Arc::new(
        nexus_local_db::init_engine_pool(&home.path().join("state.db"))
            .await
            .expect("engine pool")
            .clone_pool(),
    );
    nexus_local_db::creators::ensure_creator_row(&pool, CREATOR, "Production Shape")
        .await
        .expect("creator row");
    (home, pool)
}

/// The same fixture with the executor a dry run uses: recording Work writes
/// instead of landing them, with a home attached (a simulated executor must
/// stay recording even then).
struct SimFixture {
    pool: Arc<SqlitePool>,
    _home: tempfile::TempDir,
    cwd: tempfile::TempDir,
    root: tempfile::TempDir,
    executor: Arc<WorkspaceCommitExecutor>,
}

async fn simulated_fixture() -> SimFixture {
    let (home, pool) = home_and_pool().await;
    let cwd = tempfile::tempdir().expect("temp cwd");
    let root = tempfile::tempdir().expect("temp workspace");
    let executor = Arc::new(
        WorkspaceCommitExecutor::new_simulated(
            Arc::new(WorkspaceSessionManager::new(Arc::clone(&pool))),
            root.path().to_string_lossy().into_owned(),
        )
        .with_work_artifact_home(home.path().to_path_buf()),
    );
    SimFixture {
        _home: home,
        cwd,
        root,
        pool,
        executor,
    }
}

fn work_record(work_id: &str) -> WorkRecord {
    WorkRecord {
        work_id: work_id.to_string(),
        creator_id: CREATOR.to_string(),
        workspace_slug: WORKSPACE.to_string(),
        status: "draft".to_string(),
        title: "Production Shape".to_string(),
        long_term_goal: "Prove the production shape".to_string(),
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
        work_ref: None,
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

fn work_record_with_ref(work_id: &str, work_ref: &str) -> WorkRecord {
    WorkRecord {
        work_ref: Some(work_ref.to_string()),
        ..work_record(work_id)
    }
}

async fn stored_work_ref(pool: &SqlitePool, work_id: &str) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>("SELECT work_ref FROM works WHERE work_id = ?")
        .bind(work_id)
        .fetch_one(pool)
        .await
        .expect("read works row")
}

/// Land one first scaffold and prove the bytes went through the port.
async fn assert_first_scaffold_lands(
    fixture: &Fixture,
    cap: &dyn Capability,
    input: serde_json::Value,
    work_id: &str,
    work_ref: &str,
    rel_child: &str,
) {
    assert_eq!(
        stored_work_ref(&fixture.pool, work_id).await,
        None,
        "{work_id} must start with a NULL work_ref — a first scaffold"
    );

    cap.run(input).await.expect("scaffold runs on a first Work");

    let landed = fixture
        .root
        .path()
        .join("Works")
        .join(work_ref)
        .join(rel_child);
    assert!(
        landed.is_file(),
        "bytes must land under the canonical root: {}",
        landed.display()
    );
    let leaked = fixture
        .cwd
        .path()
        .join("Works")
        .join(work_ref)
        .join(rel_child);
    assert!(
        !leaked.exists(),
        "the port owns the bytes, not a cwd-relative path: {}",
        leaked.display()
    );
    assert_eq!(
        stored_work_ref(&fixture.pool, work_id).await.as_deref(),
        Some(work_ref),
        "the scaffold leaves the work_ref it wrote under"
    );
}

/// Every entry under `root`, relative and ordered: `None` for a directory,
/// `Some(bytes)` for a file.
///
/// A whole-tree snapshot is the only shape that catches an entry the run added
/// or rewrote, so a single-file assertion cannot see a second file land.
fn tree_snapshot(root: &Path) -> BTreeMap<String, Option<Vec<u8>>> {
    fn walk(dir: &Path, base: &Path, out: &mut BTreeMap<String, Option<Vec<u8>>>) {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
            .map(|entry| entry.expect("directory entry").path())
            .collect();
        entries.sort();
        for path in entries {
            let relative = path
                .strip_prefix(base)
                .expect("entry below the snapshot root")
                .to_string_lossy()
                .into_owned();
            if path.is_dir() {
                out.insert(format!("{relative}/"), None);
                walk(&path, base, out);
            } else {
                out.insert(relative, Some(std::fs::read(&path).expect("entry bytes")));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial]
async fn first_scaffolds_land_bytes_through_the_real_port() {
    let fx = fixture().await;
    let _cwd = CwdGuard::enter(fx.cwd.path());

    let essay_id = "wrk_prod_essay";
    let script_id = "wrk_prod_script";
    let bible_id = "wrk_prod_bible";
    let novel_id = "wrk_prod_novel";
    for id in [essay_id, script_id, bible_id, novel_id] {
        works::create_work(&fx.pool, &work_record(id))
            .await
            .expect("works row");
    }

    let essay = EssayProjectScaffold::with_pool((*fx.pool).clone())
        .with_workspace_executor(Some(Arc::clone(&fx.executor)));
    assert_first_scaffold_lands(
        &fx,
        &essay,
        json!({
            "creator_id": CREATOR,
            "work_id": essay_id,
            "work_ref": "prod-essay",
            "title": "Production Essay",
        }),
        essay_id,
        "prod-essay",
        "Drafts/draft.md",
    )
    .await;

    let script = ScriptProjectScaffold::with_pool((*fx.pool).clone())
        .with_workspace_executor(Some(Arc::clone(&fx.executor)));
    assert_first_scaffold_lands(
        &fx,
        &script,
        json!({
            "creator_id": CREATOR,
            "work_id": script_id,
            "work_ref": "prod-script",
            "title": "Production Script",
        }),
        script_id,
        "prod-script",
        "Scripts/script.md",
    )
    .await;

    let bible = GameBibleProjectScaffold::with_pool((*fx.pool).clone())
        .with_workspace_executor(Some(Arc::clone(&fx.executor)));
    assert_first_scaffold_lands(
        &fx,
        &bible,
        json!({
            "creator_id": CREATOR,
            "work_id": bible_id,
            "work_ref": "prod-bible",
            "title": "Production Bible",
        }),
        bible_id,
        "prod-bible",
        "Design/pillars.md",
    )
    .await;

    let novel = NovelProjectScaffold::with_pool((*fx.pool).clone())
        .with_workspace_executor(Some(Arc::clone(&fx.executor)));
    assert_first_scaffold_lands(
        &fx,
        &novel,
        json!({
            "creator_id": CREATOR,
            "work_id": novel_id,
            "work_ref": "prod-novel",
            "title": "Production Novel",
            "create_world": true,
            "world_title": "Production World",
            "total_planned_chapters": 3,
        }),
        novel_id,
        "prod-novel",
        "README.md",
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial]
async fn a_reinit_preserves_an_existing_canonical_novel_file() {
    let fx = fixture().await;
    let _cwd = CwdGuard::enter(fx.cwd.path());

    let work_id = "wrk_prod_novel_keep";
    works::create_work(&fx.pool, &work_record(work_id))
        .await
        .expect("works row");

    let work_dir = fx.root.path().join("Works").join("prod-novel-keep");
    std::fs::create_dir_all(&work_dir).expect("work dir");
    std::fs::write(work_dir.join("README.md"), "# user edits\n").expect("user readme");

    let novel = NovelProjectScaffold::with_pool((*fx.pool).clone())
        .with_workspace_executor(Some(Arc::clone(&fx.executor)));
    novel
        .run(json!({
            "creator_id": CREATOR,
            "work_id": work_id,
            "work_ref": "prod-novel-keep",
            "title": "Production Novel",
            "create_world": true,
            "world_title": "Production World",
            "total_planned_chapters": 2,
        }))
        .await
        .expect("re-init over an existing canonical Work");

    assert_eq!(
        std::fs::read_to_string(work_dir.join("README.md")).expect("canonical readme"),
        "# user edits\n",
        "the idempotency check must protect the canonical file"
    );
    assert!(
        !fx.cwd.path().join("Works").join("prod-novel-keep").exists(),
        "no cwd-relative tree may appear"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial]
async fn a_later_port_refusal_restores_the_canonical_tree() {
    let fx = fixture().await;
    let _cwd = CwdGuard::enter(fx.cwd.path());

    let work_id = "wrk_prod_essay_rollback";
    works::create_work(&fx.pool, &work_record(work_id))
        .await
        .expect("works row");

    let work_dir = fx.root.path().join("Works").join("prod-essay-rollback");
    std::fs::create_dir_all(&work_dir).expect("work dir");
    std::fs::write(work_dir.join("README.md"), "ORIGINAL\n").expect("original readme");

    let executor: Arc<dyn WorkspaceExecutor> = Arc::new(FailOnChild {
        inner: Arc::clone(&fx.executor),
        fail_on: "Outlines/outline.md",
    });
    let essay =
        EssayProjectScaffold::with_pool((*fx.pool).clone()).with_workspace_executor(Some(executor));

    let err = essay
        .run(json!({
            "creator_id": CREATOR,
            "work_id": work_id,
            "work_ref": "prod-essay-rollback",
            "title": "Production Essay",
        }))
        .await
        .expect_err("the later commit refuses");
    assert!(
        matches!(err, CapabilityError::Forbidden(_)),
        "expected the port refusal to propagate, got: {err}"
    );

    assert_eq!(
        std::fs::read_to_string(work_dir.join("README.md")).expect("restored readme"),
        "ORIGINAL\n",
        "rollback must restore the overwritten canonical file"
    );
    assert!(
        !work_dir.join("Outlines").exists() && !work_dir.join("Drafts").exists(),
        "rollback must remove the canonical directories this run created"
    );
    assert!(
        !fx.cwd
            .path()
            .join("Works")
            .join("prod-essay-rollback")
            .exists(),
        "nothing may land in the cwd tree"
    );
    assert_eq!(
        stored_work_ref(&fx.pool, work_id).await,
        None,
        "the failed scaffold restores the claimed work_ref"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial]
async fn a_partial_reinit_uses_the_stored_ref_through_the_real_port() {
    let fx = fixture().await;
    let _cwd = CwdGuard::enter(fx.cwd.path());

    let work_id = "wrk_prod_novel_partial";
    works::create_work(&fx.pool, &work_record_with_ref(work_id, "prod-keep-ref"))
        .await
        .expect("works row with a stored ref");

    let novel = NovelProjectScaffold::with_pool((*fx.pool).clone())
        .with_workspace_executor(Some(Arc::clone(&fx.executor)));
    novel
        .run(json!({
            "creator_id": CREATOR,
            "work_id": work_id,
            "work_ref": "prod-other-ref",
            "title": "Renamed Production Novel",
            "create_world": true,
            "world_title": "Production World",
            "total_planned_chapters": 1,
            "fields_changed": ["title"],
        }))
        .await
        .expect("a partial re-init excluding work_ref succeeds through the real port");

    let stored = fx.root.path().join("Works").join("prod-keep-ref");
    assert!(
        stored.join("README.md").is_file(),
        "the scaffold must write under the stored ref: {}",
        stored.display()
    );
    assert!(
        !fx.root.path().join("Works").join("prod-other-ref").exists(),
        "the unselected requested ref must not become a second tree"
    );
    assert!(
        !fx.cwd.path().join("Works").join("prod-keep-ref").exists(),
        "nothing may land in the cwd tree"
    );
    assert_eq!(
        stored_work_ref(&fx.pool, work_id).await.as_deref(),
        Some("prod-keep-ref"),
        "a selection without work_ref keeps the stored ref"
    );
}

/// The produce-side dry run: the REAL `NovelProjectScaffold` driven through the
/// `WorkspaceExecutor` capability surface over a Work that already holds real
/// chapter bytes. Every Work write it intends is recorded; nothing lands.
#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial]
async fn simulated_scaffold_records_without_touching_work_tree() {
    let fx = simulated_fixture().await;
    let _cwd = CwdGuard::enter(fx.cwd.path());

    let work_id = "wrk_sim_novel";
    works::create_work(&fx.pool, &work_record_with_ref(work_id, "sim-novel"))
        .await
        .expect("works row with a stored ref");

    let work_dir = fx.root.path().join("Works").join("sim-novel");
    let chapter = work_dir.join("Stories/chapter-01.md");
    std::fs::create_dir_all(chapter.parent().expect("chapter dir")).expect("chapter dir");
    std::fs::write(&chapter, "# the existing chapter\n").expect("chapter bytes");

    let before = tree_snapshot(fx.root.path());

    let novel = NovelProjectScaffold::with_pool((*fx.pool).clone())
        .with_workspace_executor(Some(Arc::clone(&fx.executor) as Arc<dyn WorkspaceExecutor>));
    novel
        .run(json!({
            "creator_id": CREATOR,
            "work_id": work_id,
            "work_ref": "sim-novel",
            "title": "Simulated Novel",
            "create_world": true,
            "world_title": "Simulated World",
            "total_planned_chapters": 1,
        }))
        .await
        .expect("the simulated scaffold runs on a real workspace");

    assert_eq!(
        tree_snapshot(fx.root.path()),
        before,
        "a simulated scaffold must leave the whole Work tree as it found it"
    );
    assert_eq!(
        std::fs::read(&chapter).expect("chapter still readable"),
        b"# the existing chapter\n",
        "the existing chapter bytes are untouched"
    );

    let recorded = fx
        .executor
        .recorded_work_artifact_mutations()
        .expect("a simulated executor records its Work writes");
    let writes: Vec<(String, String)> = recorded
        .iter()
        .map(|mutation| (mutation.work_ref.clone(), mutation.rel_child.clone()))
        .collect();
    assert_eq!(
        writes,
        vec![
            ("sim-novel".to_string(), "README.md".to_string()),
            (
                "sim-novel".to_string(),
                "Outlines/volume-outline.md".to_string()
            ),
            (
                "sim-novel".to_string(),
                "Outlines/foreshadowing.md".to_string()
            ),
            (
                "sim-novel".to_string(),
                "Outlines/event-index.md".to_string()
            ),
            ("sim-novel".to_string(), "AGENTS.md".to_string()),
        ],
        "the dry run records every intended Work write, in order"
    );
    assert!(
        fx.executor.records_work_artifacts(),
        "the executor stays recording even with a Work artifact home attached"
    );
}

/// A recording executor whose port refuses the last Work write, so a dry run
/// fails after it recorded the earlier ones. A dry run now runs no database
/// step, so the failure has to come from the port itself.
struct RecordingThenRefusal {
    inner: Arc<WorkspaceCommitExecutor>,
}

#[async_trait]
impl WorkspaceExecutor for RecordingThenRefusal {
    fn workspace_root(&self) -> Result<PathBuf, CapabilityError> {
        self.inner.workspace_root()
    }

    async fn open(
        &self,
        input: WorkspaceOpenInput,
    ) -> Result<WorkspaceOpenOutput, CapabilityError> {
        self.inner.open(input).await
    }

    async fn commit(
        &self,
        input: WorkspaceCommitInput,
    ) -> Result<WorkspaceCommitOutput, CapabilityError> {
        self.inner.commit(input).await
    }

    fn records_work_artifacts(&self) -> bool {
        self.inner.records_work_artifacts()
    }

    async fn commit_work_artifact(
        &self,
        input: WorkArtifactCommitInput,
    ) -> Result<WorkArtifactCommitOutput, CapabilityError> {
        if input.rel_child == "AGENTS.md" {
            return Err(CapabilityError::Forbidden(format!(
                "work '{}' has no works row in the active workspace",
                input.work_ref
            )));
        }
        self.inner.commit_work_artifact(input).await
    }
}

/// A dry run that fails after it recorded Work paths must not touch the tree
/// it was given: the dangling `README.md` belongs to the user, and unlinking
/// it would make an error path of a simulation mutate the tree.
#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial]
async fn a_failed_simulated_scaffold_leaves_the_work_tree_alone() {
    let fx = simulated_fixture().await;
    let _cwd = CwdGuard::enter(fx.cwd.path());

    let work_id = "wrk_sim_fail_novel";
    works::create_work(&fx.pool, &work_record_with_ref(work_id, "sim-fail-novel"))
        .await
        .expect("works row with a stored ref");

    let work_dir = fx.root.path().join("Works").join("sim-fail-novel");
    std::fs::create_dir_all(&work_dir).expect("work dir");
    let readme = work_dir.join("README.md");
    std::os::unix::fs::symlink("never-written.md", &readme).expect("dangling symlink");
    assert!(!readme.exists(), "the seeded link must dangle");

    let executor: Arc<dyn WorkspaceExecutor> = Arc::new(RecordingThenRefusal {
        inner: Arc::clone(&fx.executor),
    });
    let novel =
        NovelProjectScaffold::with_pool((*fx.pool).clone()).with_workspace_executor(Some(executor));

    let err = novel
        .run(json!({
            "creator_id": CREATOR,
            "work_id": work_id,
            "work_ref": "sim-fail-novel",
            "title": "Failed Simulated Novel",
            "create_world": true,
            "world_title": "Failed Simulated World",
            "total_planned_chapters": 1,
        }))
        .await
        .expect_err("the port refusal behind the recorded writes must fail the dry run");
    assert!(
        matches!(err, CapabilityError::Forbidden(_)),
        "expected the port refusal to propagate, got: {err}"
    );

    let link = std::fs::symlink_metadata(&readme)
        .expect("a failed dry run must leave the pre-existing entry in place");
    assert!(
        link.file_type().is_symlink(),
        "the pre-existing symlink must survive the failed dry run"
    );
    assert_eq!(
        std::fs::read_link(&readme).expect("link target"),
        Path::new("never-written.md"),
        "the link target is untouched"
    );
    let mut entries: Vec<String> = std::fs::read_dir(&work_dir)
        .expect("work dir readable")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    entries.sort();
    assert_eq!(
        entries,
        vec!["README.md".to_string()],
        "a failed dry run must not add or remove entries in the Work tree"
    );
}
