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
            builder_reason: IrOuterGraphError::Unprojected {
                concept: "node kind judge".to_string(),
                location: "node draft".to_string(),
            },
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
