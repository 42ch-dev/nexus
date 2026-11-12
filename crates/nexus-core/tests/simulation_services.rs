//! The Simulate route's own gates, executed through a real `CoreService`: the path-id check,
//! the candidate check and the wire→IR bridge the daemon route depends on.
#![cfg(feature = "execution")]

use nexus_contracts::{
    SimulationTrajectoryEffectSummary, StrategySimulationRequest,
    WorkflowDocument as WireWorkflowDocument,
};
use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService, PresetError, Principal};
use nexus_workflow_ir::{content_hash, WorkflowDocument};
use tempfile::TempDir;

/// Minimal linear candidate: `start -> done`, no waits and no judges.
const CANDIDATE: &str = r#"{
    "ir_version": 0,
    "id": "sim-route",
    "version": 1,
    "revision": 0,
    "state": "candidate",
    "meta": {
        "description": "simulate route fixture",
        "kind": "creator",
        "run_intents": [],
        "requires_capabilities": []
    },
    "entry": "start",
    "terminal": ["done"],
    "nodes": [
        {"id": "start", "kind": "outer_state", "effect_class": "read", "exit_when": {"kind": "rule"}},
        {"id": "done", "kind": "terminal", "effect_class": "pure"}
    ],
    "edges": [{"id": "start->done", "from": "start", "to": "done", "kind": "linear"}],
    "nests": [],
    "assets": [],
    "signals": [],
    "roles": [],
    "effect_summary": "read",
    "provenance": {"source": "compose"}
}"#;

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

/// The same candidate in another lifecycle state.
fn request(state: &str) -> StrategySimulationRequest {
    let text = CANDIDATE.replacen("\"candidate\"", &format!("\"{state}\""), 1);
    StrategySimulationRequest {
        document: serde_json::from_str(&text).expect("fixture is a valid document"),
    }
}

fn rejection_code(error: &CoreError) -> &str {
    match error {
        CoreError::Preset(PresetError::Rejected { code, .. }) => code,
        other => panic!("expected a preset rejection, got {other:?}"),
    }
}

#[tokio::test]
async fn simulate_rejects_a_body_id_that_disagrees_with_the_path() {
    let (_tmp, core, principal) = setup().await;
    let error = core
        .simulate_strategy(&principal, "other-strategy".into(), request("candidate"))
        .await
        .unwrap_err();
    assert_eq!(rejection_code(&error), "strategy_id_mismatch");
    core.close().await.unwrap();
}

#[tokio::test]
async fn simulate_rejects_a_document_that_is_not_a_candidate() {
    let (_tmp, core, principal) = setup().await;
    let error = core
        .simulate_strategy(&principal, "sim-route".into(), request("approved"))
        .await
        .unwrap_err();
    assert_eq!(rejection_code(&error), "strategy_not_candidate");
    core.close().await.unwrap();
}

#[tokio::test]
async fn simulate_returns_the_trajectory_for_a_candidate_document() {
    let (_tmp, core, principal) = setup().await;
    let trajectory = core
        .simulate_strategy(&principal, "sim-route".into(), request("candidate"))
        .await
        .unwrap();

    assert_eq!(trajectory.nodes_visited, ["done", "start"]);
    assert!(trajectory.predicted_waits.is_empty());
    assert!(trajectory.stubbed_judges.is_empty());
    assert_eq!(
        trajectory.effect_summary,
        SimulationTrajectoryEffectSummary::Read
    );
    assert_eq!(trajectory.simulation_level, "reachability");

    // The hash proves the bridge: the wire document became the IR the walk hashed.
    let ir: WorkflowDocument = serde_json::from_str(CANDIDATE).expect("fixture is a valid IR");
    assert_eq!(
        trajectory.ir_content_hash,
        content_hash(&ir).expect("the fixture hashes")
    );
    core.close().await.unwrap();
}

#[tokio::test]
async fn workflow_ir_reads_the_installed_strategy_as_a_candidate_document() {
    let (_tmp, core, principal) = setup().await;
    let document = core
        .strategy_workflow_ir(&principal, "novel-writing".into())
        .await
        .unwrap();

    assert_eq!(document.id.as_str(), "novel-writing");
    assert_eq!(document.state.to_string(), "candidate");
    assert!(!document.nodes.is_empty());

    // The response IS the schema-owned DTO: a decode of its own serialization proves
    // the document the route serves round-trips through the wire shape.
    let wire = serde_json::to_value(&document).expect("document is serializable");
    let decoded: WireWorkflowDocument =
        serde_json::from_value(wire).expect("document decodes as the schema DTO");
    assert_eq!(decoded.id, document.id);
    assert_eq!(decoded.nodes.len(), document.nodes.len());
    core.close().await.unwrap();
}

#[tokio::test]
async fn workflow_ir_refuses_an_unknown_strategy() {
    let (_tmp, core, principal) = setup().await;
    let error = core
        .strategy_workflow_ir(&principal, "missing-strategy".into())
        .await
        .unwrap_err();
    assert!(
        matches!(error, CoreError::Preset(PresetError::NotFound(_))),
        "expected the existing not-found shape, got {error:?}"
    );
    core.close().await.unwrap();
}
