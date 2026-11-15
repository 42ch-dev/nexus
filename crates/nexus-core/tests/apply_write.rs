//! The apply write, executed through a real `CoreService`: one approval becomes one immutable
//! apply row; a strategy that is not a user bundle, an unknown content hash, another strategy's
//! approval, an approval whose frozen document no longer hashes to its identity, an approved
//! document the builder refuses, a strategy with no schedule and a repeat apply of the current
//! approval are typed refusals that write nothing; a superseded approval applies again (that is
//! rollback), and two concurrent applies of one approval leave exactly one row.
//!
//! Plan: `.mstar/plans/v241-p35-applied-identity.md` T2; the buildability refusal is
//! `.mstar/plans/p4b-shared-buildability.md` T2 (I-000448).

#![cfg(feature = "execution")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService, Principal};
use nexus_orchestration::preset_runtime::{IrOuterGraphError, IrTemplateSource};
use sqlx::SqlitePool;
use tempfile::TempDir;

/// The user preset every apply here names: a user bundle is the only source the schedule intake
/// reads an applied identity from, so it is the only source an apply may write for.
const STRATEGY: &str = "apply-fixture";

/// A second user preset, so the foreign-approval refusal trips on the pairing, not on the source.
const OTHER_STRATEGY: &str = "other-strategy";

/// An embedded preset: no user bundle exists for it, so the schedule intake would never read an
/// apply recorded against this id.
const EMBEDDED_PRESET: &str = "novel-writing";

struct Setup {
    _tmp: TempDir,
    core: CoreService,
    principal: Principal,
    /// `<tmp>/.nexus42`: the home the core resolves patterns under.
    home: PathBuf,
}

/// Write a minimal user preset bundle at `<nexus_home>/presets/<id>/preset.yaml` — the layout the
/// core resolves as a user (Canvas) strategy.
fn seed_user_preset(nexus_home: &std::path::Path, id: &str) {
    let bundle = nexus_home.join("presets").join(id);
    std::fs::create_dir_all(&bundle).expect("preset bundle dir");
    std::fs::write(
        bundle.join("preset.yaml"),
        format!(
            r#"
preset:
  id: {id}
  version: 1
  kind: creator
  description: "apply write fixture"
  requires_capabilities: []
  initial: draft
  terminal: done
states:
  - id: draft
    enter: []
    exit_when: {{ kind: rule }}
    next: done
  - id: done
    terminal: true
"#
        ),
    )
    .expect("preset yaml");
}

async fn setup() -> Setup {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join(".nexus42");
    std::fs::create_dir_all(&home).unwrap();
    seed_user_preset(&home, STRATEGY);
    seed_user_preset(&home, OTHER_STRATEGY);
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        tmp.path(),
        "test_creator",
        "default",
    ))
    .unwrap();
    std::fs::write(home.join("config.toml"), "active_creator_id = \"test_creator\"\n[active_workspace_slug_by_creator]\n\"test_creator\" = \"default\"\n").unwrap();
    let core = CoreService::open(CoreOpenOptions {
        user_home: tmp.path().to_path_buf(),
        // The engine writer mode is the one admitted for `creator_schedules` (the fixture seeds
        // one), and it is admitted for the apply tables too.
        access: CoreAccess::EngineOwner,
        memory: None,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    Setup {
        _tmp: tmp,
        core,
        principal,
        home,
    }
}

/// The strategy's import candidate with its `version` bumped, so two snapshots of the same
/// strategy differ in content without needing a second preset.
fn with_version(
    base: &nexus_workflow_ir::WorkflowDocument,
    version: u64,
) -> nexus_workflow_ir::WorkflowDocument {
    let mut document = base.clone();
    document.version = version;
    document
}

/// Freeze one approval row exactly as the approve path would, keyed by its recomputed hash.
async fn seed_approval(
    setup: &Setup,
    strategy_id: &str,
    version: u64,
) -> (String, String, nexus_workflow_ir::WorkflowDocument) {
    let base = setup
        .core
        .read_strategy_candidate(&setup.principal, strategy_id)
        .await
        .unwrap()
        .document;
    let document = with_version(&base, version);
    let hash = nexus_workflow_ir::content_hash(&document).unwrap();
    let approval_id = format!("apr_{version}");
    sqlx::query(
        "INSERT INTO strategy_approvals \
         (approval_id, strategy_id, content_hash, candidate_revision, document_json, \
          trajectory_json, pins_json, approver, approved_at) \
         VALUES (?, ?, ?, 0, ?, ?, '[]', 'test_creator', '2026-01-01T00:00:00+00:00')",
    )
    .bind(&approval_id)
    .bind(strategy_id)
    .bind(&hash)
    .bind(serde_json::to_string(&document).unwrap())
    .bind(serde_json::json!({"ir_content_hash": hash}).to_string())
    .execute(setup.core.pool())
    .await
    .unwrap();
    (approval_id, hash, document)
}

/// Freeze one approval at an explicit candidate revision and approval time, so the status read's
/// ordering can be proven against timestamps that disagree with the revision order.
async fn seed_revision_approval(
    setup: &Setup,
    strategy_id: &str,
    revision: u64,
    approved_at: &str,
) -> (String, String) {
    let base = setup
        .core
        .read_strategy_candidate(&setup.principal, strategy_id)
        .await
        .unwrap()
        .document;
    let mut document = with_version(&base, revision);
    document.revision = revision;
    let hash = nexus_workflow_ir::content_hash(&document).unwrap();
    let approval_id = format!("apr_rev{revision}");
    sqlx::query(
        "INSERT INTO strategy_approvals \
         (approval_id, strategy_id, content_hash, candidate_revision, document_json, \
          trajectory_json, pins_json, approver, approved_at) \
         VALUES (?, ?, ?, ?, ?, ?, '[]', 'test_creator', ?)",
    )
    .bind(&approval_id)
    .bind(strategy_id)
    .bind(&hash)
    .bind(i64::try_from(revision).expect("the fixture revision fits an i64"))
    .bind(serde_json::to_string(&document).unwrap())
    .bind(serde_json::json!({"ir_content_hash": hash}).to_string())
    .bind(approved_at)
    .execute(setup.core.pool())
    .await
    .unwrap();
    (approval_id, hash)
}

/// The schedule whose `preset_id` is the strategy identity the apply admits against (plan D10).
async fn seed_schedule(pool: &SqlitePool, strategy_id: &str) {
    sqlx::query(
        "INSERT INTO creator_schedules \
         (schedule_id, creator_id, preset_id, preset_version, status, concurrency_kind, \
          current_core_context_version, created_at, updated_at) \
         VALUES (?, 'test_creator', ?, 1, 'pending', 'serial', 0, 0, 0)",
    )
    .bind(format!("SCH-{strategy_id}"))
    .bind(strategy_id)
    .execute(pool)
    .await
    .unwrap();
}

/// The template the unbuildable fixture's judge gate names. The applied path builds with
/// `IrTemplateSource::OwnSource`, which resolves nothing, so naming any template is enough
/// (I-000449).
const JUDGE_TEMPLATE: &str = "prompts/judge.md";

/// The `entry` the dangling-entry fixture declares: no node carries this id, so the wiring
/// names a task the projection never produced.
const DANGLING_ENTRY: &str = "nowhere";

/// A document the builder refuses, together with the reason it refuses it: an apply of it must
/// carry that text unchanged.
struct Unbuildable {
    /// Distinguishes the fixture's approval row.
    tag: &'static str,
    /// The approved document.
    document: nexus_workflow_ir::WorkflowDocument,
    /// What `build_wired_outer_graph_from_ir` answers for `document`.
    builder_reason: IrOuterGraphError,
}

/// One refused document per builder refusal kind, each frozen from the strategy's own candidate so
/// only the refused part differs. The last row is the `Graph` kind: the document projects cleanly
/// and only its wiring is invalid, which is the I-000448 state apply used to install.
fn unbuildable_documents(base: &nexus_workflow_ir::WorkflowDocument) -> [Unbuildable; 5] {
    [
        Unbuildable {
            tag: "unmappable",
            document: with_draft_kind(base, nexus_workflow_ir::WorkflowNodeKind::PatternSlot),
            builder_reason: IrOuterGraphError::Unmappable {
                concept: "node kind pattern_slot".to_string(),
                location: "node draft".to_string(),
            },
        },
        Unbuildable {
            tag: "unprojected",
            document: with_draft_kind(base, nexus_workflow_ir::WorkflowNodeKind::Judge),
            builder_reason: IrOuterGraphError::standalone_node_kind(
                nexus_workflow_ir::WorkflowNodeKind::Judge,
                "draft",
            ),
        },
        // Passes the concept refusal (an inner step is a projected kind) and only the exporter
        // refuses it: the id is not `<inner_graph>/<step>`.
        Unbuildable {
            tag: "unprojectable",
            document: with_draft_kind(base, nexus_workflow_ir::WorkflowNodeKind::InnerStep),
            builder_reason: IrOuterGraphError::Projection(
                nexus_preset::WorkflowIrError::NotProjectable {
                    reason: "inner step `draft` is not `<inner_graph>/<node_id>`".to_string(),
                },
            ),
        },
        Unbuildable {
            tag: "unresolved_template",
            document: with_judge_template(base),
            builder_reason: IrOuterGraphError::UnresolvedTemplate {
                node: "draft".to_string(),
                template: Some(JUDGE_TEMPLATE.to_string()),
                template_source: IrTemplateSource::OwnSource,
            },
        },
        Unbuildable {
            tag: "dangling_entry",
            document: with_entry(base, DANGLING_ENTRY),
            builder_reason: IrOuterGraphError::Graph(graph_flow::GraphError::TaskNotFound(
                format!(
                    "Start task '{DANGLING_ENTRY}' was not added to graph '{}'",
                    base.id.as_str()
                ),
            )),
        },
    ]
}

/// The candidate declaring another `entry`.
fn with_entry(
    base: &nexus_workflow_ir::WorkflowDocument,
    entry: &str,
) -> nexus_workflow_ir::WorkflowDocument {
    let mut document = base.clone();
    document.entry = entry.to_string();
    document
}

/// The candidate's `draft` node carrying another kind: the shape of the builder's node-kind
/// refusals.
fn with_draft_kind(
    base: &nexus_workflow_ir::WorkflowDocument,
    kind: nexus_workflow_ir::WorkflowNodeKind,
) -> nexus_workflow_ir::WorkflowDocument {
    let mut document = base.clone();
    document
        .nodes
        .iter_mut()
        .find(|node| node.id == "draft")
        .expect("the fixture has a draft state")
        .kind = kind;
    document
}

/// The candidate whose `draft` exits on an `llm_judge` naming [`JUDGE_TEMPLATE`].
fn with_judge_template(
    base: &nexus_workflow_ir::WorkflowDocument,
) -> nexus_workflow_ir::WorkflowDocument {
    let mut document = base.clone();
    let state = document
        .nodes
        .iter_mut()
        .find(|node| node.id == "draft")
        .expect("the fixture has a draft state");
    state.exit_when = Some(nexus_workflow_ir::WorkflowExitWhen {
        kind: nexus_workflow_ir::WorkflowExitWhenKind::LlmJudge,
        template: Some(JUDGE_TEMPLATE.to_string()),
        judge_capability: None,
        min_interval: None,
        duration: None,
    });
    document
}

/// Freeze one approval of `document`, keyed by its recomputed hash.
async fn seed_document_approval(
    setup: &Setup,
    strategy_id: &str,
    tag: &str,
    document: &nexus_workflow_ir::WorkflowDocument,
) -> (String, String) {
    let hash = nexus_workflow_ir::content_hash(document).unwrap();
    let approval_id = format!("apr_{tag}");
    sqlx::query(
        "INSERT INTO strategy_approvals \
         (approval_id, strategy_id, content_hash, candidate_revision, document_json, \
          trajectory_json, pins_json, approver, approved_at) \
         VALUES (?, ?, ?, 0, ?, ?, '[]', 'test_creator', '2026-01-01T00:00:00+00:00')",
    )
    .bind(&approval_id)
    .bind(strategy_id)
    .bind(&hash)
    .bind(serde_json::to_string(document).unwrap())
    .bind(serde_json::json!({"ir_content_hash": hash}).to_string())
    .execute(setup.core.pool())
    .await
    .unwrap();
    (approval_id, hash)
}

async fn apply_rows(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM strategy_applies")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn schedule_rows(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM creator_schedules")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Rows with no successor: the current apply of each strategy.
async fn current_rows(pool: &SqlitePool) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT cur.apply_id, cur.approval_id FROM strategy_applies cur \
          WHERE cur.strategy_id = ? \
            AND NOT EXISTS (SELECT 1 FROM strategy_applies later \
                             WHERE later.supersedes_apply_id = cur.apply_id)",
    )
    .bind(STRATEGY)
    .fetch_all(pool)
    .await
    .unwrap()
}

fn refusal(error: &CoreError) -> &serde_json::Value {
    match error {
        CoreError::InputValidation { details } => details,
        other => panic!("expected an apply refusal, got {other:?}"),
    }
}

fn reason(error: &CoreError) -> &str {
    refusal(error)["reason"].as_str().unwrap()
}

#[tokio::test]
async fn an_unknown_content_hash_is_the_typed_not_found_and_writes_nothing() {
    let setup = setup().await;
    seed_schedule(setup.core.pool(), STRATEGY).await;

    let error = setup
        .core
        .apply_strategy(&setup.principal, STRATEGY, "blake3:no-such-approval")
        .await
        .unwrap_err();

    assert_eq!(reason(&error), "approval_not_found");
    assert_eq!(refusal(&error)["content_hash"], "blake3:no-such-approval");
    assert_eq!(apply_rows(setup.core.pool()).await, 0);
    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn an_embedded_preset_is_the_typed_refusal_and_writes_nothing() {
    let setup = setup().await;
    // Everything else an apply needs is in place: a valid approval of this strategy's own frozen
    // document and a schedule. Only the source is wrong, so without the guard this apply would
    // succeed and record a row the schedule intake never reads.
    let (_, hash, _) = seed_approval(&setup, EMBEDDED_PRESET, 1).await;
    seed_schedule(setup.core.pool(), EMBEDDED_PRESET).await;

    let error = setup
        .core
        .apply_strategy(&setup.principal, EMBEDDED_PRESET, &hash)
        .await
        .unwrap_err();

    assert_eq!(reason(&error), "strategy_not_user_preset");
    assert_eq!(refusal(&error)["strategy_id"], EMBEDDED_PRESET);
    assert_eq!(apply_rows(setup.core.pool()).await, 0);
    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn another_strategys_approval_is_the_typed_mismatch_and_writes_nothing() {
    let setup = setup().await;
    let (_, hash, _) = seed_approval(&setup, STRATEGY, 1).await;
    // Both strategies are scheduled, so the mismatch is the only thing this apply trips on.
    seed_schedule(setup.core.pool(), STRATEGY).await;
    seed_schedule(setup.core.pool(), OTHER_STRATEGY).await;

    let error = setup
        .core
        .apply_strategy(&setup.principal, OTHER_STRATEGY, &hash)
        .await
        .unwrap_err();

    assert_eq!(reason(&error), "approval_strategy_mismatch");
    assert_eq!(refusal(&error)["strategy_id"], OTHER_STRATEGY);
    assert_eq!(refusal(&error)["approval_strategy_id"], STRATEGY);
    assert_eq!(apply_rows(setup.core.pool()).await, 0);
    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn a_document_edited_after_approval_is_the_typed_hash_change_and_writes_nothing() {
    let setup = setup().await;
    let (approval_id, stored_hash, document) = seed_approval(&setup, STRATEGY, 1).await;
    seed_schedule(setup.core.pool(), STRATEGY).await;

    // Edit the frozen bytes behind the approval's back; the row still passes every CHECK, so the
    // only thing that can catch the move is the recomputation apply performs.
    let edited = with_version(&document, 7);
    let edited_hash = nexus_workflow_ir::content_hash(&edited).unwrap();
    sqlx::query("UPDATE strategy_approvals SET document_json = ? WHERE approval_id = ?")
        .bind(serde_json::to_string(&edited).unwrap())
        .bind(&approval_id)
        .execute(setup.core.pool())
        .await
        .unwrap();

    let error = setup
        .core
        .apply_strategy(&setup.principal, STRATEGY, &stored_hash)
        .await
        .unwrap_err();

    assert_eq!(reason(&error), "approval_hash_changed");
    assert_eq!(refusal(&error)["approval_id"], approval_id.as_str());
    assert_eq!(refusal(&error)["stored_content_hash"], stored_hash.as_str());
    assert_eq!(
        refusal(&error)["recomputed_content_hash"],
        edited_hash.as_str()
    );
    assert_eq!(
        apply_rows(setup.core.pool()).await,
        0,
        "the hash refusal must happen before the insert"
    );
    setup.core.close().await.unwrap();
}

/// Every kind the builder refuses — the projection refusals and the `Graph` refusals after them —
/// is the same typed apply refusal, carrying the builder's own reason and writing nothing: apply
/// installs exactly what the run can build (I-000448).
#[tokio::test]
async fn every_document_the_builder_refuses_is_the_typed_refusal_and_writes_nothing() {
    let setup = setup().await;
    // The strategy is a user bundle and owns a schedule, so each document's buildability is the
    // only thing its apply can trip on — exactly the applies that used to land and fail at their
    // first run.
    seed_schedule(setup.core.pool(), STRATEGY).await;
    let base = setup
        .core
        .read_strategy_candidate(&setup.principal, STRATEGY)
        .await
        .unwrap()
        .document;

    for Unbuildable {
        tag,
        document,
        builder_reason,
    } in unbuildable_documents(&base)
    {
        let (_, hash) = seed_document_approval(&setup, STRATEGY, tag, &document).await;

        let error = setup
            .core
            .apply_strategy(&setup.principal, STRATEGY, &hash)
            .await
            .unwrap_err();

        assert_eq!(reason(&error), "document_unbuildable", "{tag}: {error:?}");
        // The reason the RUN would have failed with, verbatim: the builder refuses this same
        // document with the same `IrOuterGraphError`, and apply carries its text through.
        let carried = refusal(&error)["builder_reason"]
            .as_str()
            .unwrap_or_else(|| panic!("{tag}: the refusal carries no builder_reason: {error:?}"));
        assert_eq!(
            carried,
            builder_reason.to_string(),
            "{tag}: the refusal must carry the builder's own reason"
        );
        assert_eq!(
            apply_rows(setup.core.pool()).await,
            0,
            "{tag}: the buildability refusal must happen before the insert"
        );
    }
    setup.core.close().await.unwrap();
}

/// Buildability is checked before the schedule guard, so a strategy that is both unscheduled and
/// unbuildable reports the document: the client's fix is re-approval, not a schedule.
#[tokio::test]
async fn an_unbuildable_document_outranks_the_missing_schedule() {
    let setup = setup().await;
    let base = setup
        .core
        .read_strategy_candidate(&setup.principal, STRATEGY)
        .await
        .unwrap()
        .document;
    let [Unbuildable { tag, document, .. }, ..] = unbuildable_documents(&base);
    // No schedule at all: both guards would refuse this apply, and the document is the one that
    // wins. The premise is asserted so a fixture that grows a schedule reddens here instead of
    // silently testing nothing.
    assert_eq!(
        schedule_rows(setup.core.pool()).await,
        0,
        "the ordering test needs a strategy with no schedule"
    );
    let (_, hash) = seed_document_approval(&setup, STRATEGY, tag, &document).await;

    let error = setup
        .core
        .apply_strategy(&setup.principal, STRATEGY, &hash)
        .await
        .unwrap_err();

    assert_eq!(reason(&error), "document_unbuildable");
    assert_eq!(
        apply_rows(setup.core.pool()).await,
        0,
        "the buildability refusal must happen before the insert"
    );
    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn a_strategy_without_a_schedule_is_the_typed_refusal_and_writes_nothing() {
    let setup = setup().await;
    let (_, hash, _) = seed_approval(&setup, STRATEGY, 1).await;

    let error = setup
        .core
        .apply_strategy(&setup.principal, STRATEGY, &hash)
        .await
        .unwrap_err();

    assert_eq!(reason(&error), "strategy_has_no_schedule");
    assert_eq!(refusal(&error)["strategy_id"], STRATEGY);
    assert_eq!(apply_rows(setup.core.pool()).await, 0);
    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn applying_an_approval_and_applying_it_again_is_the_typed_conflict() {
    let setup = setup().await;
    let (approval_id, hash, _) = seed_approval(&setup, STRATEGY, 1).await;
    seed_schedule(setup.core.pool(), STRATEGY).await;

    let applied = setup
        .core
        .apply_strategy(&setup.principal, STRATEGY, &hash)
        .await
        .unwrap();
    assert_ne!(applied.apply_id, "");
    assert_eq!(applied.approval_id, approval_id);
    assert_eq!(applied.content_hash, hash);
    assert!(applied.supersedes_apply_id.is_none());
    assert_eq!(apply_rows(setup.core.pool()).await, 1);

    let error = setup
        .core
        .apply_strategy(&setup.principal, STRATEGY, &hash)
        .await
        .unwrap_err();
    assert_eq!(reason(&error), "already_applied");
    assert_eq!(refusal(&error)["approval_id"], approval_id.as_str());
    assert_eq!(refusal(&error)["apply_id"], applied.apply_id.as_str());
    assert_eq!(
        apply_rows(setup.core.pool()).await,
        1,
        "the repeat apply must not add a row"
    );
    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn a_superseded_approval_is_not_already_applied() {
    let setup = setup().await;
    let (first_approval, first_hash, _) = seed_approval(&setup, STRATEGY, 1).await;
    let (second_approval, second_hash, _) = seed_approval(&setup, STRATEGY, 2).await;
    seed_schedule(setup.core.pool(), STRATEGY).await;

    let first = setup
        .core
        .apply_strategy(&setup.principal, STRATEGY, &first_hash)
        .await
        .unwrap();
    let second = setup
        .core
        .apply_strategy(&setup.principal, STRATEGY, &second_hash)
        .await
        .unwrap();
    assert_eq!(
        second.supersedes_apply_id.as_deref(),
        Some(first.apply_id.as_str())
    );

    // Rolling back to the superseded approval is a normal apply, never the conflict.
    let rollback = setup
        .core
        .apply_strategy(&setup.principal, STRATEGY, &first_hash)
        .await
        .unwrap();
    assert_eq!(rollback.approval_id, first_approval);
    assert_eq!(
        rollback.supersedes_apply_id.as_deref(),
        Some(second.apply_id.as_str())
    );

    assert_eq!(apply_rows(setup.core.pool()).await, 3);
    let current = current_rows(setup.core.pool()).await;
    assert_eq!(current.len(), 1, "exactly one current apply per strategy");
    assert_eq!(current[0].0, rollback.apply_id);
    assert_eq!(current[0].1, first_approval);
    assert_ne!(first_approval, second_approval);
    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn two_concurrent_applies_of_one_approval_leave_exactly_one_current_row() {
    let setup = setup().await;
    let (approval_id, hash, _) = seed_approval(&setup, STRATEGY, 1).await;
    seed_schedule(setup.core.pool(), STRATEGY).await;

    let (first, second) = tokio::join!(
        setup.core.apply_strategy(&setup.principal, STRATEGY, &hash),
        setup.core.apply_strategy(&setup.principal, STRATEGY, &hash)
    );

    let applied = usize::from(first.is_ok()) + usize::from(second.is_ok());
    assert_eq!(applied, 1, "one writer wins, the other is refused");

    let loser = first
        .as_ref()
        .err()
        .or_else(|| second.as_ref().err())
        .expect("the losing apply is a refusal");
    assert_eq!(reason(loser), "already_applied");

    assert_eq!(apply_rows(setup.core.pool()).await, 1);
    let current = current_rows(setup.core.pool()).await;
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].1, approval_id);
    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn the_status_read_reports_the_current_apply_and_the_highest_candidate_revision() {
    let setup = setup().await;
    // Revision 5 is the newer approval even though revision 2 was approved LATER: `approved_at`
    // must not decide newness (compass D11).
    let (older_approval, older_hash) =
        seed_revision_approval(&setup, STRATEGY, 2, "2026-02-01T00:00:00+00:00").await;
    let (newer_approval, _) =
        seed_revision_approval(&setup, STRATEGY, 5, "2026-01-01T00:00:00+00:00").await;
    seed_schedule(setup.core.pool(), STRATEGY).await;

    let applied = setup
        .core
        .apply_strategy(&setup.principal, STRATEGY, &older_hash)
        .await
        .unwrap();
    let status = setup
        .core
        .strategy_applied(&setup.principal, STRATEGY)
        .await
        .unwrap();

    let current = status.current_apply.expect("the applied row is current");
    assert_eq!(current.apply_id, applied.apply_id);
    assert_eq!(current.approval_id, older_approval);
    assert_eq!(current.content_hash, older_hash);
    assert!(current.supersedes_apply_id.is_none());

    let newest = status.newest_approval.expect("the strategy has approvals");
    assert_eq!(newest.approval_id, newer_approval);
    assert_eq!(newest.candidate_revision, 5);

    // The rollback target read is the same total order: the first item IS `newest_approval`, and
    // the live (older) approval is still listed — nothing is hidden once it has been applied.
    let approvals = setup
        .core
        .strategy_approvals(&setup.principal, STRATEGY)
        .await
        .unwrap();
    assert_eq!(
        approvals
            .items
            .iter()
            .map(|approval| approval.approval_id.as_str())
            .collect::<Vec<_>>(),
        vec![newest.approval_id.as_str(), older_approval.as_str()],
    );
    assert_eq!(approvals.items[1].content_hash, older_hash);

    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn a_strategy_with_no_apply_reports_a_null_current_apply() {
    let setup = setup().await;
    let (approval_id, _) =
        seed_revision_approval(&setup, STRATEGY, 1, "2026-01-01T00:00:00+00:00").await;

    // An approval exists, so the newest side is present while nothing is applied yet.
    let approved_only = setup
        .core
        .strategy_applied(&setup.principal, STRATEGY)
        .await
        .unwrap();
    assert!(approved_only.current_apply.is_none());
    assert_eq!(
        approved_only
            .newest_approval
            .map(|newest| newest.approval_id),
        Some(approval_id)
    );

    // No approval and no apply: both sides are null, not a refusal.
    let empty = setup
        .core
        .strategy_applied(&setup.principal, "strategy-with-nothing")
        .await
        .unwrap();
    assert!(empty.current_apply.is_none());
    assert!(empty.newest_approval.is_none());

    // The rollback target read answers an empty list for the same strategy, never a refusal.
    let no_approvals = setup
        .core
        .strategy_approvals(&setup.principal, "strategy-with-nothing")
        .await
        .unwrap();
    assert!(no_approvals.items.is_empty());
    setup.core.close().await.unwrap();
}

/// One apply row written straight to the store — no guard consulted, no prior row superseded — as
/// only an admitted non-core writer can leave a strategy with two current applies.
async fn insert_successorless_row(
    setup: &Setup,
    apply_id: &str,
    approval_id: &str,
    content_hash: &str,
    applied_at: &str,
) {
    sqlx::query(
        "INSERT INTO strategy_applies \
         (apply_id, strategy_id, approval_id, content_hash, applied_at, applied_by, \
          supersedes_apply_id) \
         VALUES (?, ?, ?, ?, ?, 'non_core_writer', NULL)",
    )
    .bind(apply_id)
    .bind(STRATEGY)
    .bind(approval_id)
    .bind(content_hash)
    .bind(applied_at)
    .execute(setup.core.pool())
    .await
    .unwrap();
}

#[tokio::test]
async fn two_successorless_rows_resolve_to_the_same_row_on_every_read() {
    let setup = setup().await;
    let (approval_id, hash, _) = seed_approval(&setup, STRATEGY, 1).await;
    let (_, next_hash, _) = seed_approval(&setup, STRATEGY, 2).await;
    seed_schedule(setup.core.pool(), STRATEGY).await;
    insert_successorless_row(
        &setup,
        "apl_dup_older",
        &approval_id,
        &hash,
        "2026-01-01T00:00:00+00:00",
    )
    .await;
    insert_successorless_row(
        &setup,
        "apl_dup_newer",
        &approval_id,
        &hash,
        "2026-01-02T00:00:00+00:00",
    )
    .await;

    // Read 1, the status route the Canvas and the intake share.
    let status = setup
        .core
        .strategy_applied(&setup.principal, STRATEGY)
        .await
        .unwrap();
    let winner = status
        .current_apply
        .expect("a successor-less row is the current apply")
        .apply_id;
    assert_eq!(
        winner, "apl_dup_newer",
        "`applied_at DESC, apply_id DESC` names the winner"
    );

    // Read 2, `refuse_apply`'s probe, reached through a repeat apply of the current approval.
    let repeat = setup
        .core
        .apply_strategy(&setup.principal, STRATEGY, &hash)
        .await
        .unwrap_err();
    assert_eq!(reason(&repeat), "already_applied");
    assert_eq!(
        refusal(&repeat)["apply_id"],
        winner.as_str(),
        "the conflict names the row the status read reports"
    );

    // Read 3, the insert's own supersede-link pick.
    let moved = setup
        .core
        .apply_strategy(&setup.principal, STRATEGY, &next_hash)
        .await
        .unwrap();
    assert_eq!(
        moved.supersedes_apply_id.as_deref(),
        Some(winner.as_str()),
        "a new apply supersedes the row the status read reports"
    );
    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn a_repeat_apply_names_its_own_row_and_never_becomes_an_internal_fault() {
    let setup = setup().await;
    let (approval_id, hash, _) = seed_approval(&setup, STRATEGY, 1).await;
    let (other_approval, other_hash, _) = seed_approval(&setup, STRATEGY, 2).await;
    seed_schedule(setup.core.pool(), STRATEGY).await;
    // Two current applies carrying DIFFERENT approvals: the newer one wins every "current apply"
    // read, while the guard refuses on the older one because it is still successor-less.
    insert_successorless_row(
        &setup,
        "apl_older_approval",
        &approval_id,
        &hash,
        "2026-01-01T00:00:00+00:00",
    )
    .await;
    insert_successorless_row(
        &setup,
        "apl_newer_approval",
        &other_approval,
        &other_hash,
        "2026-01-02T00:00:00+00:00",
    )
    .await;

    let repeat = setup
        .core
        .apply_strategy(&setup.principal, STRATEGY, &hash)
        .await
        .unwrap_err();

    assert_eq!(
        reason(&repeat),
        "already_applied",
        "the refusal stays typed, never an internal fault"
    );
    assert_eq!(refusal(&repeat)["apply_id"], "apl_older_approval");
    assert_eq!(refusal(&repeat)["approval_id"], approval_id.as_str());
    assert_ne!(approval_id, other_approval);
    setup.core.close().await.unwrap();
}

// ── The frozen pin snapshot re-compared at Apply (P6) ─────────────────────

/// The package the pinned fixtures nest.
const PINNED_PACKAGE: &str = "pinned-step";
/// The package `PINNED_PACKAGE`'s own fragment nests, so the frozen snapshot carries a pin the
/// applied document does not name.
const NESTED_PACKAGE: &str = "nested-step";
/// The nest the pinned documents carry.
const PINNED_NEST: &str = "pinned-nest";

/// The IR fragment of a package whose only node is a slot.
fn fragment(id: &str, description: &str) -> serde_json::Value {
    serde_json::json!({
        "ir_version": 0,
        "id": id,
        "version": 1,
        "revision": 0,
        "state": "candidate",
        "meta": {
            "description": description,
            "kind": "creator",
            "run_intents": [],
            "requires_capabilities": [],
        },
        "entry": "slot",
        "terminal": ["slot"],
        "nodes": [{"id": "slot", "kind": "pattern_slot", "effect_class": "pure"}],
        "edges": [],
        "nests": [],
        "assets": [],
        "signals": [],
        "roles": [],
        "effect_summary": "pure",
        "provenance": {"source": "compose"},
    })
}

/// One package directory under `<home>/patterns`: `pattern.toml` plus the IR fragment it pins.
fn write_package(home: &Path, id: &str, body: &serde_json::Value) {
    let directory = home.join("patterns").join(id);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("pattern.toml"),
        format!(
            "id = \"{id}\"\nversion = \"1.0.0\"\ndescription = \"apply pin fixture {id}\"\n\
             effect_class = \"pure\"\nrequires_capabilities = []\nslots = []\n\n\
             [ir_compat]\nmin = 0\nmax = 0\n"
        ),
    )
    .unwrap();
    std::fs::write(
        directory.join("workflow.ir.json"),
        serde_json::to_string_pretty(body).unwrap(),
    )
    .unwrap();
}

/// The registry's current content hash for every package, keyed by package id.
fn pattern_hashes(home: &Path) -> BTreeMap<String, String> {
    nexus_pattern::scan_pattern_packages(&home.join("patterns"), nexus_workflow_ir::IR_VERSION)
        .unwrap()
        .into_iter()
        .filter_map(|package| {
            let manifest = package.manifest()?;
            Some((
                manifest.id.as_str().to_owned(),
                package.content_hash()?.to_owned(),
            ))
        })
        .collect()
}

/// The document with one exact pattern pin bound on `parents`' first node, plus — when `slot` is
/// set — the `pattern_slot` node the compose `nest` operation also writes, so buildability would
/// refuse the document on its own.
fn pinned_document(
    base: &nexus_workflow_ir::WorkflowDocument,
    package: &str,
    recorded_hash: &str,
    slot: bool,
) -> nexus_workflow_ir::WorkflowDocument {
    let mut value = serde_json::to_value(base).unwrap();
    let parent = base
        .nodes
        .iter()
        .find(|node| node.id != "done")
        .expect("the fixture has a non-terminal node")
        .id
        .clone();
    value["nests"] = serde_json::json!([{
        "id": PINNED_NEST,
        "parent_node": parent,
        "expansion": "defer_until_apply",
        "pin_policy": "exact",
        "child": {
            "kind": "pattern",
            "registry_id": package,
            "version": "1.0.0",
            "content_hash": recorded_hash,
        },
    }]);
    if slot {
        value["nodes"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "id": "pinned-slot",
                "kind": "pattern_slot",
                "effect_class": "pure",
            }));
    }
    serde_json::from_value(value).unwrap()
}

/// Freeze one approval of `document` carrying the given `pins_json`.
async fn seed_pinned_approval(
    setup: &Setup,
    tag: &str,
    document: &nexus_workflow_ir::WorkflowDocument,
    pins_json: &str,
) -> (String, String) {
    let hash = nexus_workflow_ir::content_hash(document).unwrap();
    let approval_id = format!("apr_{tag}");
    sqlx::query(
        "INSERT INTO strategy_approvals \
         (approval_id, strategy_id, content_hash, candidate_revision, document_json, \
          trajectory_json, pins_json, approver, approved_at) \
         VALUES (?, ?, ?, 0, ?, ?, ?, 'test_creator', '2026-01-01T00:00:00+00:00')",
    )
    .bind(&approval_id)
    .bind(STRATEGY)
    .bind(&hash)
    .bind(serde_json::to_string(document).unwrap())
    .bind(serde_json::json!({"ir_content_hash": hash}).to_string())
    .bind(pins_json)
    .execute(setup.core.pool())
    .await
    .unwrap();
    (approval_id, hash)
}

/// The strategy's own candidate document, the base every pinned fixture starts from.
async fn candidate_document(setup: &Setup) -> nexus_workflow_ir::WorkflowDocument {
    setup
        .core
        .read_strategy_candidate(&setup.principal, STRATEGY)
        .await
        .unwrap()
        .document
}

#[tokio::test]
async fn a_pinned_body_edited_under_an_unchanged_version_refuses_apply_naming_both_hashes() {
    let setup = setup().await;
    write_package(
        &setup.home,
        PINNED_PACKAGE,
        &fragment(PINNED_PACKAGE, "original bytes"),
    );
    let recorded = pattern_hashes(&setup.home)[PINNED_PACKAGE].clone();
    let base = candidate_document(&setup).await;
    // The slot node compose writes makes this document unbuildable on its own, so a `pin_changed`
    // refusal also proves the pin check runs BEFORE buildability.
    let document = pinned_document(&base, PINNED_PACKAGE, &recorded, true);
    let pins = serde_json::json!([{
        "id": PINNED_PACKAGE,
        "version": "1.0.0",
        "content_hash": recorded,
    }]);
    let (_, hash) = seed_pinned_approval(&setup, "pin-changed", &document, &pins.to_string()).await;
    seed_schedule(setup.core.pool(), STRATEGY).await;

    // Same id and version, different bytes: the recorded pin no longer identifies the body.
    write_package(
        &setup.home,
        PINNED_PACKAGE,
        &fragment(PINNED_PACKAGE, "changed bytes"),
    );
    let scanned = pattern_hashes(&setup.home)[PINNED_PACKAGE].clone();
    assert_ne!(scanned, recorded);

    let error = setup
        .core
        .apply_strategy(&setup.principal, STRATEGY, &hash)
        .await
        .unwrap_err();

    let details = refusal(&error);
    assert_eq!(details["reason"], "pin_changed");
    assert_eq!(details["nest_id"], PINNED_NEST);
    assert_eq!(details["package_id"], PINNED_PACKAGE);
    assert_eq!(details["package_version"], "1.0.0");
    assert_eq!(details["recorded_content_hash"], recorded.as_str());
    assert_eq!(details["scanned_content_hash"], scanned.as_str());
    assert_eq!(
        apply_rows(setup.core.pool()).await,
        0,
        "a refusal writes nothing"
    );
    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn an_unchanged_pin_snapshot_with_a_nested_pin_still_applies() {
    let setup = setup().await;
    write_package(
        &setup.home,
        NESTED_PACKAGE,
        &fragment(NESTED_PACKAGE, "nested bytes"),
    );
    let nested = pattern_hashes(&setup.home)[NESTED_PACKAGE].clone();
    // The pinned package's own fragment nests the second one, so the frozen snapshot carries a pin
    // the applied document never names — the case array equality on the document alone misses.
    let mut outer = fragment(PINNED_PACKAGE, "original bytes");
    outer["nests"] = serde_json::json!([{
        "id": "nested-nest",
        "parent_node": "slot",
        "expansion": "defer_until_apply",
        "pin_policy": "exact",
        "child": {
            "kind": "pattern",
            "registry_id": NESTED_PACKAGE,
            "version": "1.0.0",
            "content_hash": nested,
        },
    }]);
    write_package(&setup.home, PINNED_PACKAGE, &outer);
    let recorded = pattern_hashes(&setup.home)[PINNED_PACKAGE].clone();
    let base = candidate_document(&setup).await;
    let document = pinned_document(&base, PINNED_PACKAGE, &recorded, false);
    // Deliberately not in the deterministic order Approve writes: the comparison is a set.
    let pins = serde_json::json!([
        {"id": NESTED_PACKAGE, "version": "1.0.0", "content_hash": nested},
        {"id": PINNED_PACKAGE, "version": "1.0.0", "content_hash": recorded},
    ]);
    let (approval_id, hash) =
        seed_pinned_approval(&setup, "pin-unchanged", &document, &pins.to_string()).await;
    seed_schedule(setup.core.pool(), STRATEGY).await;

    let applied = setup
        .core
        .apply_strategy(&setup.principal, STRATEGY, &hash)
        .await
        .unwrap();

    assert_eq!(applied.approval_id, approval_id);
    assert_eq!(apply_rows(setup.core.pool()).await, 1);
    setup.core.close().await.unwrap();
}

/// A carrier that records no hash of its own still loads a body, and that body can pin packages
/// the approval never recorded. An empty snapshot cannot clear it: the pins are reachable, so the
/// document is exactly the fail-closed case this check exists for.
#[tokio::test]
async fn a_hashless_carrier_over_a_moved_pin_refuses_an_empty_snapshot() {
    let setup = setup().await;
    write_package(
        &setup.home,
        NESTED_PACKAGE,
        &fragment(NESTED_PACKAGE, "nested bytes"),
    );
    let nested = pattern_hashes(&setup.home)[NESTED_PACKAGE].clone();
    // The carrier pins the second package by hash from INSIDE its own fragment, so nothing in the
    // outer document names a hash to compare.
    let mut outer = fragment(PINNED_PACKAGE, "original bytes");
    outer["nests"] = serde_json::json!([{
        "id": "nested-nest",
        "parent_node": "slot",
        "expansion": "defer_until_apply",
        "pin_policy": "exact",
        "child": {
            "kind": "pattern",
            "registry_id": NESTED_PACKAGE,
            "version": "1.0.0",
            "content_hash": nested,
        },
    }]);
    write_package(&setup.home, PINNED_PACKAGE, &outer);

    let base = candidate_document(&setup).await;
    let mut document = pinned_document(&base, PINNED_PACKAGE, "", false);
    document.nests[0].child.content_hash = None;
    // The approval froze no pin at all — the snapshot the shortcut used to read as "pins nothing".
    let (_, hash) = seed_pinned_approval(&setup, "pin-hashless", &document, "[]").await;
    seed_schedule(setup.core.pool(), STRATEGY).await;

    // The nested body moved under an unchanged id and version, after the freeze.
    write_package(
        &setup.home,
        NESTED_PACKAGE,
        &fragment(NESTED_PACKAGE, "nested bytes moved"),
    );
    assert_ne!(pattern_hashes(&setup.home)[NESTED_PACKAGE], nested);

    let error = setup
        .core
        .apply_strategy(&setup.principal, STRATEGY, &hash)
        .await
        .unwrap_err();

    assert_eq!(reason(&error), "approval_pins_unverifiable");
    let detail = refusal(&error)["pin_detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains(PINNED_PACKAGE) && detail.contains("no recorded hash"),
        "the refusal names the carrier it cannot compare: {detail}"
    );
    assert_eq!(
        apply_rows(setup.core.pool()).await,
        0,
        "a refusal writes nothing"
    );
    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn a_document_with_no_recorded_pin_never_reads_the_patterns_root() {
    let setup = setup().await;
    // A regular file where the registry lives: any attempt to scan it is an I/O error, so an
    // admitted apply is proof the pin shortcut never touched the root.
    std::fs::write(setup.home.join("patterns"), "not a directory").unwrap();
    let (_, hash, _) = seed_approval(&setup, STRATEGY, 1).await;
    seed_schedule(setup.core.pool(), STRATEGY).await;

    let applied = setup
        .core
        .apply_strategy(&setup.principal, STRATEGY, &hash)
        .await
        .unwrap();

    assert_eq!(applied.content_hash, hash);
    assert_eq!(apply_rows(setup.core.pool()).await, 1);
    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn an_undecodable_or_document_inconsistent_pin_snapshot_is_unverifiable() {
    let setup = setup().await;
    write_package(
        &setup.home,
        PINNED_PACKAGE,
        &fragment(PINNED_PACKAGE, "original bytes"),
    );
    let recorded = pattern_hashes(&setup.home)[PINNED_PACKAGE].clone();
    let base = candidate_document(&setup).await;
    let document = pinned_document(&base, PINNED_PACKAGE, &recorded, false);
    // The approvals are keyed by content hash, so the second snapshot needs its own document.
    let inconsistent_document = with_version(&document, 2);
    // Two snapshots no comparison can trust: one whose members are not pins at all, and one that
    // records no pin beside a document that pins something.
    let (_, undecodable_hash) = seed_pinned_approval(
        &setup,
        "pins-undecodable",
        &document,
        r#"[{"id": "pinned-step"}]"#,
    )
    .await;
    let (_, inconsistent_hash) =
        seed_pinned_approval(&setup, "pins-inconsistent", &inconsistent_document, "[]").await;
    seed_schedule(setup.core.pool(), STRATEGY).await;

    for hash in [undecodable_hash, inconsistent_hash] {
        let error = setup
            .core
            .apply_strategy(&setup.principal, STRATEGY, &hash)
            .await
            .unwrap_err();
        assert_eq!(reason(&error), "approval_pins_unverifiable");
        assert!(
            !refusal(&error)["pin_detail"]
                .as_str()
                .unwrap_or_default()
                .is_empty(),
            "the refusal says what could not be compared"
        );
    }
    assert_eq!(
        apply_rows(setup.core.pool()).await,
        0,
        "a refusal writes nothing"
    );
    setup.core.close().await.unwrap();
}
