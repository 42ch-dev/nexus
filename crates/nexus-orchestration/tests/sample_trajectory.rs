//! End-to-end Simulate evidence over the repository's `game-narrative` sample.
//!
//! The fixture is the sample's imported candidate IR: `routing` is an `llm_judge` with a declared
//! `default` edge, so the walk proves the judge stub and the whole route payload shape without a
//! model call, a provider or an engine.

#![cfg(feature = "simulate")]

use nexus_orchestration::{simulate_document, SimulationOptions};
use nexus_workflow_ir::WorkflowDocument;

const GAME_NARRATIVE: &str = include_str!("fixtures/game-narrative.workflow.ir.json");

fn sample() -> WorkflowDocument {
    serde_json::from_str(GAME_NARRATIVE).expect("the sample IR parses")
}

fn simulate(document: &WorkflowDocument) -> nexus_contracts::SimulationTrajectory {
    simulate_document(
        document,
        &SimulationOptions {
            capabilities: nexus_orchestration::CapabilityRegistryHolder::new(),
        },
    )
    .expect("the sample simulates")
}

#[test]
fn the_sample_trajectory_walks_the_declared_default_judge_branch() {
    let trajectory = simulate(&sample());

    assert_eq!(trajectory.schema_version, 1);
    assert_eq!(trajectory.simulation_level, "reachability");
    assert_eq!(
        trajectory.nodes_visited,
        ["done", "routing", "trigger_extract"],
        "the judge's conditional lane is invisible to the default-branch walk"
    );
    assert_eq!(trajectory.stubbed_judges, ["routing"]);
    assert!(
        trajectory.predicted_waits.is_empty(),
        "the sample declares a force_transition signal only, never a pause"
    );
    assert_eq!(
        trajectory.effect_summary,
        nexus_contracts::SimulationTrajectoryEffectSummary::External
    );

    // `ir_content_hash` is recomputed from the IR, never echoed from the document.
    assert_eq!(trajectory.ir_content_hash.len(), 64);
    assert!(trajectory
        .ir_content_hash
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
    assert_ne!(
        Some(&trajectory.ir_content_hash),
        sample().content_hash.as_ref(),
        "the recorded hash is not the approval identity"
    );
}

#[test]
fn the_sample_trajectory_is_byte_identical_across_runs() {
    let document = sample();
    let before = serde_json::to_value(&document).expect("document serializes");

    let first = serde_json::to_string(&simulate(&document)).expect("trajectory serializes");
    let second = serde_json::to_string(&simulate(&document)).expect("trajectory serializes");

    assert_eq!(first, second);
    assert_eq!(
        serde_json::to_value(&document).expect("document serializes"),
        before,
        "simulating never rewrites the input document"
    );
}
