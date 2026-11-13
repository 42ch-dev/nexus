//! The candidate store's CAS contract, executed through a real `CoreService`:
//! the revision-0 import fallback, the insert at `base_revision: 0`, single-step
//! revision bumps with the embedded document mirror, and the non-mutating
//! typed conflict for a stale base.

use nexus_core::{
    CoreAccess, CoreError, CoreOpenOptions, CoreService, Principal,
    StrategyCandidateConflictError,
};
use nexus_workflow_ir::WorkflowDocument;
use sqlx::SqlitePool;
use tempfile::TempDir;

async fn setup() -> (TempDir, CoreService, Principal) {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join(".nexus42");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        tmp.path(),
        "test_creator",
        "default",
    ))
    .unwrap();
    std::fs::write(home.join("config.toml"), "active_creator_id = \"test_creator\"\n[active_workspace_slug_by_creator]\n\"test_creator\" = \"default\"\n").unwrap();
    let core = CoreService::open(CoreOpenOptions {
        user_home: tmp.path().to_path_buf(),
        access: CoreAccess::DirectWriter,
        memory: None,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    (tmp, core, principal)
}

/// The whole stored row, so a refusal can be proven to leave every byte alone.
type StoredRow = (String, String, i64, String, String);

async fn stored_row(pool: &SqlitePool, strategy_id: &str) -> Option<StoredRow> {
    sqlx::query_as(
        "SELECT strategy_id, document_json, revision, created_at, updated_at \
         FROM strategy_candidates WHERE strategy_id = ?",
    )
    .bind(strategy_id)
    .fetch_optional(pool)
    .await
    .unwrap()
}

fn conflict_of(error: CoreError) -> StrategyCandidateConflictError {
    match error {
        CoreError::StrategyCandidateConflict(conflict) => conflict,
        other => panic!("expected a strategy candidate conflict, got {other:?}"),
    }
}

#[tokio::test]
async fn an_unstored_strategy_reads_the_revision_zero_import_and_persists_nothing() {
    let (_tmp, core, principal) = setup().await;
    let candidate = core
        .read_strategy_candidate(&principal, "novel-writing")
        .await
        .unwrap();

    assert_eq!(candidate.revision, 0);
    assert_eq!(candidate.document.revision, 0);
    assert_eq!(candidate.document.state.to_string(), "candidate");
    assert_eq!(candidate.document.id.as_str(), "novel-writing");
    assert!(!candidate.content_hash.is_empty());

    assert!(
        stored_row(core.pool(), "novel-writing").await.is_none(),
        "the canonical read must not persist the import projection"
    );
    core.close().await.unwrap();
}

#[tokio::test]
async fn a_first_write_at_base_zero_inserts_revision_one_and_mirrors_it() {
    let (_tmp, core, principal) = setup().await;
    let committed = core
        .commit_strategy_candidate(&principal, "novel-writing", 0, |document| {
            let mut next = document.clone();
            next.meta.description = "composed".to_string();
            Ok(next)
        })
        .await
        .unwrap();

    // `apply` returned the document still at revision 0; the store is the only
    // writer of the embedded revision.
    assert_eq!(committed.revision, 1);
    assert_eq!(committed.document.revision, 1);
    assert_eq!(committed.document.meta.description, "composed");

    let (_, document_json, revision, _, _) = stored_row(core.pool(), "novel-writing")
        .await
        .expect("the first write inserts the row");
    assert_eq!(revision, 1);
    let stored: WorkflowDocument = serde_json::from_str(&document_json).unwrap();
    assert_eq!(stored.revision, 1);
    assert_eq!(stored.meta.description, "composed");
    core.close().await.unwrap();
}

#[tokio::test]
async fn a_stale_base_is_refused_with_the_typed_conflict_and_changes_nothing() {
    let (_tmp, core, principal) = setup().await;
    core.commit_strategy_candidate(&principal, "novel-writing", 0, |document| {
        Ok(document.clone())
    })
    .await
    .unwrap();
    let before = stored_row(core.pool(), "novel-writing").await.unwrap();

    let conflict = conflict_of(
        core.commit_strategy_candidate(&principal, "novel-writing", 0, |document| {
            let mut next = document.clone();
            next.meta.description = "must not land".to_string();
            Ok(next)
        })
        .await
        .unwrap_err(),
    );

    assert_eq!(conflict.strategy_id, "novel-writing");
    assert_eq!(conflict.base_revision, 0);
    assert_eq!(conflict.current_revision, 1);
    assert_eq!(conflict.conflicting_path, "$");
    assert_eq!(
        stored_row(core.pool(), "novel-writing").await.unwrap(),
        before,
        "a refused CAS is provably non-mutating"
    );
    core.close().await.unwrap();
}

#[tokio::test]
async fn two_sequential_writes_bump_the_revision_once_each() {
    let (_tmp, core, principal) = setup().await;

    for (base, expected) in [(0_u64, 1_u64), (1, 2)] {
        let committed = core
            .commit_strategy_candidate(&principal, "novel-writing", base, |document| {
                assert_eq!(document.revision, base, "apply sees the base revision");
                Ok(document.clone())
            })
            .await
            .unwrap();
        assert_eq!(committed.revision, expected);
        assert_eq!(committed.document.revision, expected);
    }

    let (_, document_json, revision, _, _) = stored_row(core.pool(), "novel-writing")
        .await
        .unwrap();
    assert_eq!(revision, 2);
    let stored: WorkflowDocument = serde_json::from_str(&document_json).unwrap();
    assert_eq!(stored.revision, 2);
    core.close().await.unwrap();
}

#[tokio::test]
async fn a_second_writer_at_the_pre_bump_revision_is_refused_not_merged() {
    let (_tmp, core, principal) = setup().await;
    // Writer A read the candidate at revision 0.
    let read = core
        .read_strategy_candidate(&principal, "novel-writing")
        .await
        .unwrap();
    // Writer B commits first from the same observed base.
    core.commit_strategy_candidate(&principal, "novel-writing", read.revision, |document| {
        let mut next = document.clone();
        next.meta.description = "writer b".to_string();
        Ok(next)
    })
    .await
    .unwrap();

    // Writer A retries with its pre-bump revision: refused, never merged.
    let mut stale = read.document.clone();
    stale.meta.description = "writer a".to_string();
    let conflict = conflict_of(
        core.commit_strategy_candidate(&principal, "novel-writing", read.revision, move |_| {
            Ok(stale.clone())
        })
        .await
        .unwrap_err(),
    );
    assert_eq!(conflict.current_revision, 1);

    let committed = core
        .read_strategy_candidate(&principal, "novel-writing")
        .await
        .unwrap();
    assert_eq!(committed.revision, 1);
    assert_eq!(committed.document.meta.description, "writer b");
    assert_eq!(committed.document.revision, 1);
    core.close().await.unwrap();
}
