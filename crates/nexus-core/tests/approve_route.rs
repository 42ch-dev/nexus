//! The Approve write path, executed through a real `CoreService` and a real pattern registry:
//! one approval freezes exactly one snapshot; a missing/stale simulation, a changed pin and a
//! repeat approve are typed refusals that write no snapshot and consume no candidate revision.
//!
//! Plan: `.mstar/plans/v239-p3-approve-route.md` T1–T6.

#![cfg(feature = "execution")]

use std::path::{Path, PathBuf};

use nexus_contracts::generated::daemon_api::orchestration::strategy_simulation_request::NexusWorkflowDocument as SimulationDocument;
use nexus_contracts::StrategySimulationRequest;
use nexus_core::{
    ComposeOp, CoreAccess, CoreError, CoreOpenOptions, CoreService, NestOp, PatternPackageRef,
    Principal,
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

/// One package directory: `pattern.toml` plus the IR fragment it pins.
fn write_package(home: &Path, id: &str, description: &str) {
    let directory = home.join("patterns").join(id);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("pattern.toml"),
        format!(
            "id = \"{id}\"\nversion = \"1.0.0\"\ndescription = \"approve fixture {id}\"\n\
             effect_class = \"pure\"\nrequires_capabilities = []\nslots = []\n\
             [ir_compat]\nmin = 0\nmax = 0\n"
        ),
    )
    .unwrap();
    let fragment = serde_json::json!({
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
    });
    std::fs::write(
        directory.join("workflow.ir.json"),
        serde_json::to_string_pretty(&fragment).unwrap(),
    )
    .unwrap();
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
