//! The candidate store's CAS contract, executed through a real `CoreService`:
//! the revision-0 import fallback, the insert at `base_revision: 0`, single-step
//! revision bumps with the embedded document mirror, and the non-mutating
//! typed conflict for a stale base.

use std::path::Path;

use nexus_core::{
    CoreAccess, CoreError, CoreOpenOptions, CoreService, Principal, StrategyCandidateConflictError,
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
    assert_ne!(candidate.content_hash, "");

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

    let (_, document_json, revision, _, _) =
        stored_row(core.pool(), "novel-writing").await.unwrap();
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
            Ok(stale)
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

// ---------------------------------------------------------------------------
// Import: an external partner bundle becomes a revision-1 candidate
// ---------------------------------------------------------------------------

/// The strategy id every import fixture declares.
const PARTNER: &str = "partner-fixture";

/// Write one bundle directory holding a single `preset.yaml` (the fixtures
/// reference no assets, so nothing else belongs in the partner's directory).
fn write_bundle(bundle: &Path, yaml: &str) -> Vec<(String, Vec<u8>)> {
    std::fs::create_dir_all(bundle).unwrap();
    std::fs::write(bundle.join("preset.yaml"), yaml).unwrap();
    bundle_snapshot(bundle)
}

/// Every file under `bundle`, by relative path: an import must read the source
/// and write nothing into it.
fn bundle_snapshot(bundle: &Path) -> Vec<(String, Vec<u8>)> {
    let mut entries: Vec<(String, Vec<u8>)> = std::fs::read_dir(bundle)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().into_owned();
            (name, std::fs::read(entry.path()).unwrap())
        })
        .collect();
    entries.sort();
    entries
}

/// A partner bundle the loader, the shared validator and the static walk all accept.
fn partner_bundle(description: &str) -> String {
    format!(
        r#"preset:
  id: {PARTNER}
  version: 1
  kind: creator
  description: "{description}"
  requires_capabilities: []
  run_intents: [work_init]
  initial: draft
  terminal: done
states:
  - id: draft
    exit_when: {{ kind: rule }}
    next: review
  - id: review
    exit_when: {{ kind: manual }}
    next: done
  - id: done
    terminal: true
"#
    )
}

/// A bundle the loader accepts but the shared validator refuses: two states
/// share an id, so the imported document carries a duplicate node.
fn duplicate_state_bundle() -> String {
    format!(
        r#"preset:
  id: {PARTNER}
  version: 1
  kind: creator
  description: "duplicate state fixture"
  requires_capabilities: []
  run_intents: [work_init]
  initial: draft
  terminal: done
states:
  - id: draft
    exit_when: {{ kind: rule }}
    next: done
  - id: draft
    exit_when: {{ kind: rule }}
    next: done
  - id: done
    terminal: true
"#
    )
}

/// A bundle the loader and the shared validator accept but the static walk
/// cannot follow: a judged state with two labeled branches and no `default`
/// edge, which the walk refuses rather than guessing a branch.
fn unsimulatable_bundle() -> String {
    format!(
        r#"preset:
  id: {PARTNER}
  version: 1
  kind: creator
  description: "unsimulatable fixture"
  requires_capabilities: []
  run_intents: [work_init]
  initial: draft
  terminal: done
states:
  - id: draft
    exit_when:
      kind: llm_judge
    next:
      - label: left
        target: left_branch
      - label: right
        target: right_branch
  - id: left_branch
    exit_when: {{ kind: rule }}
    next: done
  - id: right_branch
    exit_when: {{ kind: rule }}
    next: done
  - id: done
    terminal: true
"#
    )
}

fn import_refusal(error: CoreError) -> serde_json::Value {
    match error {
        CoreError::InputValidation { details } => details,
        other => panic!("expected an import refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn importing_a_partner_bundle_stores_revision_one_and_approves_with_its_trajectory() {
    let (tmp, core, principal) = setup().await;
    let bundle = tmp.path().join(PARTNER);
    let before = write_bundle(&bundle, &partner_bundle("original bytes"));

    let imported = core
        .import_strategy_candidate(&principal, &bundle)
        .await
        .unwrap();

    assert_eq!(imported.candidate.revision, 1);
    assert_eq!(imported.candidate.document.revision, 1);
    assert_eq!(imported.candidate.document.id.as_str(), PARTNER);
    assert_eq!(
        imported.trajectory.ir_content_hash, imported.candidate.content_hash,
        "the returned trajectory must be the walk of the stored bytes"
    );
    let (_, document_json, revision, _, _) = stored_row(core.pool(), PARTNER).await.unwrap();
    assert_eq!(revision, 1, "an import creates revision 1");
    let stored: WorkflowDocument = serde_json::from_str(&document_json).unwrap();
    assert_eq!(stored.revision, 1);
    assert_eq!(stored.id.as_str(), PARTNER);

    // The trajectory is the caller's Approve evidence, unchanged.
    let content_hash = imported.candidate.content_hash.clone();
    let snapshot = core
        .approve_strategy(&principal, PARTNER, 1, Some(imported.trajectory))
        .await
        .unwrap();
    assert_eq!(snapshot.content_hash.as_str(), content_hash);
    assert_eq!(snapshot.candidate_revision, 1);
    assert!(
        snapshot.pins.is_empty(),
        "an imported preset document pins nothing"
    );

    assert_eq!(
        bundle_snapshot(&bundle),
        before,
        "the source bundle is read in place, never copied or edited"
    );
    core.close().await.unwrap();
}

#[tokio::test]
async fn a_second_import_of_the_same_strategy_is_the_typed_conflict_and_changes_nothing() {
    let (tmp, core, principal) = setup().await;
    let bundle = tmp.path().join(PARTNER);
    write_bundle(&bundle, &partner_bundle("original bytes"));
    core.import_strategy_candidate(&principal, &bundle)
        .await
        .unwrap();
    let stored_before = stored_row(core.pool(), PARTNER).await.unwrap();

    // Different bundle bytes, same strategy: the stored row decides, not the source.
    let before = write_bundle(&bundle, &partner_bundle("different bytes"));
    let conflict = conflict_of(
        core.import_strategy_candidate(&principal, &bundle)
            .await
            .unwrap_err(),
    );

    assert_eq!(conflict.strategy_id, PARTNER);
    assert_eq!(conflict.base_revision, 0, "import always aims at base 0");
    assert_eq!(conflict.current_revision, 1);
    assert_eq!(
        stored_row(core.pool(), PARTNER).await.unwrap(),
        stored_before,
        "a refused import rewrites nothing"
    );
    assert_eq!(bundle_snapshot(&bundle), before);
    core.close().await.unwrap();
}

#[tokio::test]
async fn a_bundle_that_does_not_load_is_refused_as_a_bundle_before_any_row_exists() {
    let (tmp, core, principal) = setup().await;
    let bundle = tmp.path().join("broken-bundle");
    let before = write_bundle(&bundle, "preset:\n  id: broken\n");

    let error = core
        .import_strategy_candidate(&principal, &bundle)
        .await
        .unwrap_err();

    match error {
        CoreError::InvalidInput { field, .. } => assert_eq!(field, "bundle"),
        other => panic!("expected a bundle input refusal, got {other:?}"),
    }
    assert!(stored_row(core.pool(), PARTNER).await.is_none());
    assert_eq!(bundle_snapshot(&bundle), before);
    core.close().await.unwrap();
}

#[tokio::test]
async fn a_document_the_shared_validator_refuses_is_the_invalid_document_refusal() {
    let (tmp, core, principal) = setup().await;
    let bundle = tmp.path().join(PARTNER);
    let before = write_bundle(&bundle, &duplicate_state_bundle());

    let details = import_refusal(
        core.import_strategy_candidate(&principal, &bundle)
            .await
            .unwrap_err(),
    );

    assert_eq!(details["code"], "preset_import_refused");
    assert_eq!(details["reason"], "invalid_document");
    let violations = details["violations"].as_array().unwrap();
    assert!(
        violations.iter().any(|violation| violation
            .as_str()
            .is_some_and(|text| text.contains("duplicate") && text.contains("draft"))),
        "the validator's violations travel with the refusal: {violations:?}"
    );
    assert!(stored_row(core.pool(), PARTNER).await.is_none());
    assert_eq!(bundle_snapshot(&bundle), before);
    core.close().await.unwrap();
}

#[tokio::test]
async fn a_document_the_walk_cannot_simulate_is_refused_with_its_reason() {
    let (tmp, core, principal) = setup().await;
    let bundle = tmp.path().join(PARTNER);
    let before = write_bundle(&bundle, &unsimulatable_bundle());

    let details = import_refusal(
        core.import_strategy_candidate(&principal, &bundle)
            .await
            .unwrap_err(),
    );

    assert_eq!(details["code"], "preset_import_refused");
    assert_eq!(details["reason"], "simulation_unavailable");
    assert!(
        details["simulation_reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("default")),
        "the walk's own refusal travels with the refusal: {details}"
    );
    assert!(stored_row(core.pool(), PARTNER).await.is_none());
    assert_eq!(bundle_snapshot(&bundle), before);
    core.close().await.unwrap();
}
