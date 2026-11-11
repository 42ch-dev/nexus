//! Production-shape proof for the Work artifact seam (v235 P2 fix round).
//!
//! The production registry arms build every scaffold with the DEFAULT `Works`
//! root — a *relative* path — so `workspace_root_of` hands the executor an
//! empty workspace root, and a first scaffold starts with `works.work_ref`
//! NULL. Both facts are exercised here against the REAL port
//! (`WorkspaceCommitExecutor` → `FsWorkArtifactPort`), not a recording double:
//! the bytes must land under the executor's canonical workspace root while the
//! scaffold itself only knows `Works`.
//!
//! The process cwd is moved into a separate temp directory: the scaffolds'
//! directory bookkeeping is cwd-relative, so any byte that landed without going
//! through the port would land there instead of in the canonical root.

#![cfg(feature = "execution")]
#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use nexus_core::execution::executor::WorkspaceCommitExecutor;
use nexus_core::execution::session::WorkspaceSessionManager;
use nexus_local_db::works::{self, WorkRecord};
use nexus_orchestration::capability::builtins::{
    EssayProjectScaffold, GameBibleProjectScaffold, NovelProjectScaffold, ScriptProjectScaffold,
};
use nexus_orchestration::capability::{Capability, WorkspaceExecutor};
use serde_json::json;
use sqlx::SqlitePool;

const CREATOR: &str = "ctr_prod_shape";
const WORKSPACE: &str = "default";

/// The pieces one production-shaped run needs: the CLI home the port resolves
/// the active creator/workspace from, the canonical workspace root the executor
/// was built for, the unrelated cwd, and the engine pool both halves share.
struct Fixture {
    // Held so the config the port resolves its active creator from outlives the run.
    _home: tempfile::TempDir,
    cwd: tempfile::TempDir,
    root: tempfile::TempDir,
    pool: Arc<SqlitePool>,
    executor: Arc<dyn WorkspaceExecutor>,
}

async fn fixture() -> Fixture {
    let home = tempfile::tempdir().expect("temp home");
    let cwd = tempfile::tempdir().expect("temp cwd");
    let root = tempfile::tempdir().expect("temp workspace");
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
    // The port resolves the active creator from this home; the World the novel
    // scaffold creates is FK-bound to that creator row.
    nexus_local_db::creators::ensure_creator_row(&pool, CREATOR, "Production Shape")
        .await
        .expect("creator row");
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
        // A work row is created without a ref; the scaffold claims it.
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

    // The port's target: `<canonical root>/Works/<work_ref>/<child>`.
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
    // Nothing landed beside the cwd-relative `Works` tree instead.
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

#[tokio::test(flavor = "multi_thread")]
async fn first_scaffolds_land_bytes_through_the_real_port() {
    let fx = fixture().await;
    // The scaffolds derive their `Works` tree from the cwd; keep that tree out
    // of the canonical root so a non-port write cannot masquerade as a hit.
    let previous_cwd = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(fx.cwd.path()).expect("chdir");

    let essay_id = "wrk_prod_essay";
    let script_id = "wrk_prod_script";
    let bible_id = "wrk_prod_bible";
    let novel_id = "wrk_prod_novel";
    for id in [essay_id, script_id, bible_id, novel_id] {
        works::create_work(&fx.pool, &work_record(id))
            .await
            .expect("works row");
    }

    // The default constructor path: `with_pool` ⇒ relative `Works` root, which
    // is exactly what the production registry arms use.
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
    std::env::set_current_dir(previous_cwd).expect("restore cwd");
}
