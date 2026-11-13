//! The Approve write path, executed through a real `CoreService` and a real pattern registry:
//! one approval freezes exactly one snapshot; a missing/stale simulation, a changed pin, a slot a
//! nested pattern requires and a repeat approve are typed refusals that write no snapshot and
//! consume no candidate revision; a compose that lands between the revision check and the insert
//! makes the insert fail instead of freezing a stale revision.
//!
//! Plan: `.mstar/plans/v239-p3-approve-route.md` T1–T6.

#![cfg(feature = "execution")]

use std::path::{Path, PathBuf};

use nexus_contracts::generated::daemon_api::orchestration::strategy_simulation_request::NexusWorkflowDocument as SimulationDocument;
use nexus_contracts::StrategySimulationRequest;
use nexus_core::{
    ComposeDiagnostic, ComposeOp, CoreAccess, CoreError, CoreOpenOptions, CoreService, NestOp,
    PatternPackageRef, Principal,
};
use nexus_workflow_ir::{WorkflowDocument, WorkflowNestBindingExpansion};
use sqlx::SqlitePool;
use tempfile::TempDir;

/// An embedded preset the static walk accepts (the same fixture the compose fence suite uses).
const STRATEGY: &str = "novel-project-init";

struct Setup {
    _tmp: TempDir,
    core: CoreService,
    principal: Principal,
    home: PathBuf,
}

async fn setup() -> Setup {
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
    Setup {
        _tmp: tmp,
        core,
        principal,
        home,
    }
}

fn package(id: &str) -> PatternPackageRef {
    PatternPackageRef {
        id: id.to_owned(),
        version: "1.0.0".to_owned(),
    }
}

/// The IR fragment of a package whose only node is a slot.
fn slot_fragment(id: &str, description: &str) -> serde_json::Value {
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

/// The IR fragment of a package that pins `next` in one nest and records no binding for it.
fn chain_fragment(id: &str, next: &str) -> serde_json::Value {
    serde_json::json!({
        "ir_version": 0,
        "id": id,
        "version": 1,
        "revision": 0,
        "state": "candidate",
        "meta": {
            "description": format!("approve fixture {id}"),
            "kind": "creator",
            "run_intents": [],
            "requires_capabilities": [],
        },
        "entry": "slot",
        "terminal": ["slot"],
        "nodes": [{"id": "slot", "kind": "pattern_slot", "effect_class": "pure"}],
        "edges": [],
        "nests": [{
            "id": format!("{id}-nest"),
            "parent_node": "slot",
            "expansion": "defer_until_apply",
            "pin_policy": "exact",
            "child": {"kind": "pattern", "registry_id": next, "version": "1.0.0"},
        }],
        "assets": [],
        "signals": [],
        "roles": [],
        "effect_summary": "pure",
        "provenance": {"source": "compose"},
    })
}

/// One package directory: `pattern.toml` declaring `slots` (TOML `[[slots]]` entries) plus the
/// given IR fragment.
fn write_declaring_package(home: &Path, id: &str, slots: &str, fragment: &serde_json::Value) {
    let directory = home.join("patterns").join(id);
    std::fs::create_dir_all(&directory).unwrap();
    // The manifest schema requires `slots`; an empty declaration is a top-level key.
    let empty_slots = if slots.is_empty() {
        "slots = []\n\n"
    } else {
        "\n"
    };
    std::fs::write(
        directory.join("pattern.toml"),
        format!(
            "id = \"{id}\"\nversion = \"1.0.0\"\ndescription = \"approve fixture {id}\"\n\
             effect_class = \"pure\"\nrequires_capabilities = []\n{empty_slots}\
             [ir_compat]\nmin = 0\nmax = 0\n{slots}"
        ),
    )
    .unwrap();
    std::fs::write(
        directory.join("workflow.ir.json"),
        serde_json::to_string_pretty(fragment).unwrap(),
    )
    .unwrap();
}

/// One package directory: `pattern.toml` plus the IR fragment it pins.
fn write_package(home: &Path, id: &str, description: &str) {
    write_declaring_package(home, id, "", &slot_fragment(id, description));
}

async fn candidate(core: &CoreService, principal: &Principal) -> nexus_core::StrategyCandidate {
    core.read_strategy_candidate(principal, STRATEGY)
        .await
        .unwrap()
}

/// Simulate the current candidate through the real route, so the approval evidence is the bytes
/// the client would have received.
async fn simulate(
    core: &CoreService,
    principal: &Principal,
    document: &WorkflowDocument,
) -> nexus_contracts::SimulationTrajectory {
    let wire: SimulationDocument =
        serde_json::from_value(serde_json::to_value(document).unwrap()).unwrap();
    core.simulate_strategy(
        principal,
        STRATEGY.to_owned(),
        StrategySimulationRequest { document: wire },
    )
    .await
    .unwrap()
}

async fn snapshot_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM strategy_approvals")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn stored_revision(pool: &SqlitePool) -> Option<i64> {
    sqlx::query_scalar("SELECT revision FROM strategy_candidates WHERE strategy_id = ?")
        .bind(STRATEGY)
        .fetch_optional(pool)
        .await
        .unwrap()
}

fn refusal(error: &CoreError) -> &serde_json::Value {
    match error {
        CoreError::InputValidation { details } => details,
        other => panic!("expected an approval refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn approve_freezes_one_snapshot_bound_to_the_simulated_content() {
    let setup = setup().await;
    let before = candidate(&setup.core, &setup.principal).await;
    let trajectory = simulate(&setup.core, &setup.principal, &before.document).await;

    let snapshot = setup
        .core
        .approve_strategy(
            &setup.principal,
            STRATEGY,
            before.revision,
            Some(trajectory.clone()),
        )
        .await
        .unwrap();

    assert!(!snapshot.approval_id.is_empty());
    assert_eq!(snapshot.strategy_id.as_str(), STRATEGY);
    assert_eq!(snapshot.candidate_revision, before.revision);
    assert_eq!(snapshot.content_hash.as_str(), before.content_hash);
    assert_eq!(snapshot.trajectory.ir_content_hash, before.content_hash);
    assert_eq!(snapshot.approver.as_str(), setup.principal.creator_id());
    assert_eq!(
        serde_json::to_value(&snapshot.document).unwrap(),
        serde_json::to_value(&before.document).unwrap(),
        "the snapshot must freeze the candidate document byte for byte"
    );
    assert!(
        snapshot.pins.is_empty(),
        "the import carries no pattern pin"
    );

    assert_eq!(snapshot_count(setup.core.pool()).await, 1);
    assert_eq!(
        stored_revision(setup.core.pool()).await,
        None,
        "approve does not persist the import projection"
    );
    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn approve_without_simulation_writes_nothing() {
    let setup = setup().await;
    let before = candidate(&setup.core, &setup.principal).await;

    let error = setup
        .core
        .approve_strategy(&setup.principal, STRATEGY, before.revision, None)
        .await
        .unwrap_err();

    let details = refusal(&error);
    assert_eq!(details["reason"], "simulation_missing");
    assert_eq!(
        details["current_content_hash"].as_str(),
        Some(before.content_hash.as_str())
    );
    assert_eq!(snapshot_count(setup.core.pool()).await, 0);
    assert_eq!(stored_revision(setup.core.pool()).await, None);
    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn a_stale_simulation_is_refused_naming_both_hashes() {
    let setup = setup().await;
    let before = candidate(&setup.core, &setup.principal).await;
    let mut stale = simulate(&setup.core, &setup.principal, &before.document).await;
    stale.ir_content_hash = "0".repeat(64);

    let error = setup
        .core
        .approve_strategy(&setup.principal, STRATEGY, before.revision, Some(stale))
        .await
        .unwrap_err();

    let details = refusal(&error);
    assert_eq!(details["reason"], "simulation_stale");
    assert_eq!(
        details["recorded_content_hash"].as_str(),
        Some("0".repeat(64).as_str())
    );
    assert_eq!(
        details["current_content_hash"].as_str(),
        Some(before.content_hash.as_str())
    );
    assert_eq!(snapshot_count(setup.core.pool()).await, 0);
    assert_eq!(stored_revision(setup.core.pool()).await, None);
    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn a_changed_pinned_package_is_refused_naming_package_and_both_hashes() {
    let setup = setup().await;
    write_package(&setup.home, "approve-step", "original bytes");
    let composed = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            0,
            ComposeOp::Nest(NestOp {
                nest_id: "approve-nest".to_owned(),
                parent_node: "approve-slot".to_owned(),
                package: package("approve-step"),
                expansion: WorkflowNestBindingExpansion::ExpandInCandidate,
            }),
        )
        .await
        .unwrap();
    let revision = composed.candidate.revision;
    let recorded_hash = composed.candidate.document.nests[0]
        .child
        .content_hash
        .clone()
        .expect("the nest records the resolved pin");
    let trajectory = simulate(&setup.core, &setup.principal, &composed.candidate.document).await;

    // Same id and version, different fragment bytes: the pin no longer identifies the bytes.
    write_package(&setup.home, "approve-step", "changed bytes");

    let error = setup
        .core
        .approve_strategy(&setup.principal, STRATEGY, revision, Some(trajectory))
        .await
        .unwrap_err();

    let details = refusal(&error);
    assert_eq!(details["reason"], "pin_changed");
    assert_eq!(details["package_id"].as_str(), Some("approve-step"));
    assert_eq!(details["package_version"].as_str(), Some("1.0.0"));
    assert_eq!(
        details["recorded_content_hash"].as_str(),
        Some(recorded_hash.as_str())
    );
    let scanned = details["scanned_content_hash"]
        .as_str()
        .expect("the refusal names the current bytes");
    assert_ne!(scanned, recorded_hash);
    assert_eq!(snapshot_count(setup.core.pool()).await, 0);
    assert_eq!(
        stored_revision(setup.core.pool()).await,
        Some(i64::try_from(revision).unwrap()),
        "a refused approve consumes no candidate revision"
    );
    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn a_repeat_approve_of_the_same_content_is_the_conflict_and_writes_nothing() {
    let setup = setup().await;
    let before = candidate(&setup.core, &setup.principal).await;
    let trajectory = simulate(&setup.core, &setup.principal, &before.document).await;
    let first = setup
        .core
        .approve_strategy(
            &setup.principal,
            STRATEGY,
            before.revision,
            Some(trajectory.clone()),
        )
        .await
        .unwrap();

    let error = setup
        .core
        .approve_strategy(
            &setup.principal,
            STRATEGY,
            before.revision,
            Some(trajectory),
        )
        .await
        .unwrap_err();

    let conflict = match error {
        CoreError::StrategyAlreadyApproved(conflict) => conflict,
        other => panic!("expected the repeat-approve conflict, got {other:?}"),
    };
    assert_eq!(conflict.strategy_id, STRATEGY);
    assert_eq!(conflict.content_hash, before.content_hash);
    assert_eq!(conflict.approval_id, *first.approval_id);
    assert_eq!(conflict.candidate_revision, before.revision);
    assert_eq!(snapshot_count(setup.core.pool()).await, 1);
    assert_eq!(stored_revision(setup.core.pool()).await, None);
    setup.core.close().await.unwrap();
}

/// The revision check and the freeze are one statement: a compose that commits in the window
/// between approve's read and its insert wins, and the approval is the typed conflict.
///
/// The window is parked with SQLite's write lock (the shape the receipt store's racing insert
/// uses): the rival candidate row is written inside an uncommitted transaction — invisible to
/// approve's read — approve's conditional insert parks on the lock, and the commit lets it through
/// to a revision predicate that no longer holds. This is also the absent-row case: the candidate
/// had no stored row (the revision-0 import projection) and now sits at revision 1.
#[tokio::test]
async fn a_compose_that_lands_in_the_insert_window_writes_no_snapshot() {
    let setup = setup().await;
    let before = candidate(&setup.core, &setup.principal).await;
    let trajectory = simulate(&setup.core, &setup.principal, &before.document).await;

    let mut rival = setup.core.pool().begin().await.unwrap();
    let mut document = serde_json::to_value(&before.document).unwrap();
    document["revision"] = serde_json::json!(1);
    let now = "2026-11-13T00:00:00Z";
    sqlx::query(
        "INSERT INTO strategy_candidates \
         (strategy_id, document_json, revision, created_at, updated_at) VALUES (?, ?, 1, ?, ?)",
    )
    .bind(STRATEGY)
    .bind(document.to_string())
    .bind(now)
    .bind(now)
    .execute(&mut *rival)
    .await
    .expect("the rival compose row");

    let raced = async {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        rival.commit().await.expect("the rival compose commits");
    };
    let (result, ()) = tokio::join!(
        setup.core.approve_strategy(
            &setup.principal,
            STRATEGY,
            before.revision,
            Some(trajectory)
        ),
        raced,
    );

    let conflict = match result.unwrap_err() {
        CoreError::StrategyCandidateConflict(conflict) => conflict,
        other => {
            panic!("the compose that landed in the window is the typed conflict, got {other:?}")
        }
    };
    assert_eq!(conflict.base_revision, before.revision);
    assert_eq!(conflict.current_revision, 1);
    assert_eq!(
        snapshot_count(setup.core.pool()).await,
        0,
        "a stale revision must not freeze a snapshot"
    );
    assert_eq!(stored_revision(setup.core.pool()).await, Some(1));
    setup.core.close().await.unwrap();
}

/// A required slot declared by a package the candidate pins *through* another package's body is
/// the same refusal: approve calls the compose path's one slot check, which walks the fragments.
#[tokio::test]
async fn a_required_slot_inside_a_nested_pattern_blocks_the_approval() {
    let setup = setup().await;
    write_declaring_package(
        &setup.home,
        "nested-step",
        "[[slots]]\nname = \"topic\"\nkind = \"string\"\nrequired = true\n",
        &slot_fragment("nested-step", "nested bytes"),
    );
    write_declaring_package(
        &setup.home,
        "outer-chain",
        "",
        &chain_fragment("outer-chain", "nested-step"),
    );

    let composed = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            0,
            ComposeOp::Nest(NestOp {
                nest_id: "outer-nest".to_owned(),
                parent_node: "outer-slot".to_owned(),
                package: package("outer-chain"),
                expansion: WorkflowNestBindingExpansion::ExpandInCandidate,
            }),
        )
        .await
        .unwrap();

    assert!(
        composed.diagnostics.iter().any(|finding| matches!(
            finding,
            ComposeDiagnostic::UnboundRequiredSlot { nest_id, package, slot }
                if nest_id == "outer-chain-nest" && package.id == "nested-step" && slot == "topic"
        )),
        "compose reports the nested slot from the same check: {:?}",
        composed.diagnostics
    );

    let trajectory = simulate(&setup.core, &setup.principal, &composed.candidate.document).await;
    let error = setup
        .core
        .approve_strategy(
            &setup.principal,
            STRATEGY,
            composed.candidate.revision,
            Some(trajectory),
        )
        .await
        .unwrap_err();

    let details = refusal(&error);
    assert_eq!(details["reason"], "slot_unbound");
    assert_eq!(details["nest_id"], "outer-chain-nest");
    assert_eq!(details["package"], "nested-step@1.0.0");
    assert_eq!(details["slot"], "topic");
    assert_eq!(snapshot_count(setup.core.pool()).await, 0);
    assert_eq!(
        stored_revision(setup.core.pool()).await,
        Some(i64::try_from(composed.candidate.revision).unwrap())
    );
    setup.core.close().await.unwrap();
}
