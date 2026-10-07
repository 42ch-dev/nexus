//! Work-entry extract queue authority (v1.207 P1 item 6,
//! R-V1190-KB-EXTRACT-QUEUE-GAP).
//!
//! Pins the core `queue_kb_extract` / `list_kb_extract_jobs` /
//! `get_kb_extract_job` producer the `creator kb queue-extract` and
//! `creator kb extract-status` CLI leaves now run on: the job lands under the
//! admitted principal's creator/workspace binding, the `--chapter` sugar
//! derives the artifact locator, the enqueue is idempotent on its key, and a
//! foreign creator's job is refused rather than reported absent.
//!
//! `kb_extract_jobs` is guarded to a migration/engine writer at the schema
//! level, so the enqueue is exercised over the engine-owned admission the
//! CLI leaf now takes.
use nexus_core::{
    CoreAccess, CoreError, CoreOpenOptions, CoreService, Principal, QueueKbExtractParams,
};

/// Creator + workspace the fixture materializes and selects.
const CREATOR: &str = "author";
const SLUG: &str = "default";

fn select_creator(home: &std::path::Path) {
    std::fs::write(
        home.join(".nexus42/config.toml"),
        format!(
            "active_creator_id = \"{CREATOR}\"\n[active_workspace_slug_by_creator]\n\"{CREATOR}\" = \"{SLUG}\"\n"
        ),
    )
    .unwrap();
}

/// Materialize the fixture workspace, its creator row and its schema, then
/// release the seed writer so a later open can take its own admission.
///
/// `foreign_job` additionally inserts one job owned by another creator behind
/// the same workspace, so the ownership classification can be exercised on a
/// row the admitted principal does not own. Returns its job id.
async fn seed_workspace(home: &std::path::Path, foreign_job: bool) -> Option<String> {
    std::fs::create_dir_all(home.join(".nexus42")).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        home, CREATOR, SLUG,
    ))
    .unwrap();
    select_creator(home);

    let db = nexus_home_layout::workspace_state_db_path(home, CREATOR, SLUG);
    let foreign_job_id = {
        let seed = nexus_local_db::init_engine_pool(&db).await.unwrap();
        let pool = seed.clone_pool();
        nexus_local_db::creators::ensure_creator_row(&pool, CREATOR, "Author")
            .await
            .unwrap();
        let foreign_job_id = if foreign_job {
            let job = nexus_local_db::enqueue_extract_job_with_artifact(
                &pool,
                "other",
                SLUG,
                "kb_foreign",
                "wld_foreign",
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
            Some(job.job_id)
        } else {
            None
        };
        pool.close().await;
        foreign_job_id
    };
    nexus_local_db::writer_protocol::release_retained_writer_guards(&db);
    foreign_job_id
}

async fn open_core(home: &std::path::Path, access: CoreAccess) -> (CoreService, Principal) {
    let core = CoreService::open(CoreOpenOptions {
        user_home: home.into(),
        access,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    (core, principal)
}

/// The engine-owned admission the enqueue leaf takes (the queue table is
/// guarded to a migration/engine writer).
async fn core_with_creator(home: &std::path::Path) -> (CoreService, Principal) {
    seed_workspace(home, false).await;
    open_core(home, CoreAccess::EngineOwner).await
}

fn params(work_entry_id: &str, world_id: &str) -> QueueKbExtractParams {
    QueueKbExtractParams {
        work_entry_id: work_entry_id.to_string(),
        world_id: world_id.to_string(),
        work_id: None,
        chapter: None,
    }
}

/// Queue → get → list round trip: the row lands under the principal's
/// creator/workspace binding and a re-queue returns the same job.
#[tokio::test]
async fn queue_get_and_list_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let (core, principal) = core_with_creator(temp.path()).await;

    let job = core
        .queue_kb_extract(&principal, params("kb_alpha", "wld_1"))
        .await
        .unwrap();
    assert!(job.job_id.starts_with("xj_"), "{}", job.job_id);
    assert_eq!(job.status, "queued");
    assert_eq!(job.creator_id, CREATOR);
    // The operational workspace binding is the admitted principal's slug.
    assert_eq!(job.workspace_id, SLUG);
    assert_eq!(job.work_entry_id, "kb_alpha");
    assert_eq!(job.world_id, "wld_1");
    assert!(job.source_kind.is_none());
    assert!(job.source_locator.is_none());
    assert!(job.profile_hint.is_none());
    assert!(job.work_id.is_none());
    assert!(!job.created_at.is_empty());

    // Idempotent on `(creator, work_entry_id, world_id)`: the existing
    // non-failed job comes back unchanged.
    let again = core
        .queue_kb_extract(&principal, params("kb_alpha", "wld_1"))
        .await
        .unwrap();
    assert_eq!(again.job_id, job.job_id);

    let fetched = core
        .get_kb_extract_job(&principal, &job.job_id)
        .await
        .unwrap()
        .expect("queued job is readable");
    assert_eq!(fetched.job_id, job.job_id);

    assert!(core
        .get_kb_extract_job(&principal, "xj_missing")
        .await
        .unwrap()
        .is_none());

    let listed = core.list_kb_extract_jobs(&principal, 100).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].job_id, job.job_id);

    // The bound is the caller's, as the CLI's 100-job default is.
    assert_eq!(
        core.list_kb_extract_jobs(&principal, 1)
            .await
            .unwrap()
            .len(),
        1
    );

    core.close().await.unwrap();
}

/// `--chapter N` derives the artifact locator triple; a chapter below 1 and an
/// unsafe entry id are refused as invalid input.
#[tokio::test]
async fn chapter_sugar_and_input_refusals() {
    let temp = tempfile::tempdir().unwrap();
    let (core, principal) = core_with_creator(temp.path()).await;

    let job = core
        .queue_kb_extract(
            &principal,
            QueueKbExtractParams {
                work_entry_id: "kb_chapter".to_string(),
                world_id: "wld_2".to_string(),
                work_id: Some("wrk_novel".to_string()),
                chapter: Some(5),
            },
        )
        .await
        .unwrap();
    assert_eq!(job.source_kind.as_deref(), Some("work_chapter"));
    assert_eq!(job.source_locator.as_deref(), Some("chapter:05"));
    assert_eq!(job.profile_hint.as_deref(), Some("novel"));
    assert_eq!(job.work_id.as_deref(), Some("wrk_novel"));

    assert!(matches!(
        core.queue_kb_extract(
            &principal,
            QueueKbExtractParams {
                chapter: Some(0),
                ..params("kb_zero", "wld_2")
            }
        )
        .await,
        Err(CoreError::InvalidInput { ref field, ref reason })
            if field == "chapter" && reason == "Chapter number must be >= 1"
    ));

    assert!(matches!(
        core.queue_kb_extract(&principal, params("kb/../escape", "wld_2"))
            .await,
        Err(CoreError::InvalidInput { ref field, .. }) if field == "work_entry_id"
    ));

    core.close().await.unwrap();
}

/// A job owned by another creator is refused with the work-index ownership
/// classification, never reported absent, and a foreign job is not listed.
#[tokio::test]
async fn foreign_job_is_forbidden_not_absent() {
    let temp = tempfile::tempdir().unwrap();
    let foreign_job_id = seed_workspace(temp.path(), true)
        .await
        .expect("seed inserted the foreign job");
    let (core, principal) = open_core(temp.path(), CoreAccess::EngineOwner).await;

    assert!(matches!(
        core.get_kb_extract_job(&principal, &foreign_job_id).await,
        Err(CoreError::Forbidden { ref resource })
            if resource == &format!("kb_owner:extract job {foreign_job_id}")
    ));
    assert!(core
        .list_kb_extract_jobs(&principal, 100)
        .await
        .unwrap()
        .is_empty());

    core.close().await.unwrap();
}

/// Read-only core access refuses the enqueue, while the status read stays
/// available (the CLI's `extract-status` leaf only reads).
#[tokio::test]
async fn read_only_access_refuses_the_enqueue() {
    let temp = tempfile::tempdir().unwrap();
    seed_workspace(temp.path(), false).await;
    let (core, principal) = open_core(temp.path(), CoreAccess::ReadOnly).await;

    assert!(matches!(
        core.queue_kb_extract(&principal, params("kb_ro", "wld_ro"))
            .await,
        Err(CoreError::Forbidden { ref resource })
            if resource == "work: read-only core access"
    ));
    assert!(core
        .list_kb_extract_jobs(&principal, 100)
        .await
        .unwrap()
        .is_empty());

    core.close().await.unwrap();
}
