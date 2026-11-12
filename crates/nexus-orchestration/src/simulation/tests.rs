//! Static-walk behavior: reachability, wait classification, nest rollup, determinism and the
//! zero-dispatch proof. Fixtures are IR documents in their wire shape, parsed through the same
//! generated contract the route carries.

use super::*;
use crate::{Capability, CapabilityError};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn document(text: &str) -> WorkflowDocument {
    serde_json::from_str(text).expect("fixture is a valid workflow document")
}

fn simulate(document: &WorkflowDocument) -> SimulationTrajectory {
    simulate_document(
        document,
        SimulationOptions {
            capabilities: CapabilityRegistryHolder::new(),
        },
    )
    .expect("simulation succeeds")
}

/// A judged state that reaches a manual wait node, with a global pause signal.
const WAITS_AND_JUDGE: &str = r#"{
    "ir_version": 0,
    "id": "waits-and-judge",
    "version": 1,
    "revision": 3,
    "state": "candidate",
    "meta": {
        "description": "manual, judge and signal predictions",
        "kind": "creator",
        "run_intents": ["work_init"],
        "requires_capabilities": []
    },
    "entry": "confirm",
    "terminal": ["done"],
    "nodes": [
        {"id": "confirm", "kind": "outer_state", "effect_class": "read", "exit_when": {"kind": "manual"}},
        {"id": "route", "kind": "outer_state", "effect_class": "read", "exit_when": {"kind": "llm_judge"}},
        {"id": "gate", "kind": "manual_wait", "effect_class": "pure"},
        {"id": "done", "kind": "terminal", "effect_class": "pure"}
    ],
    "edges": [
        {"id": "confirm->route", "from": "confirm", "to": "route", "kind": "linear"},
        {"id": "route->gate", "from": "route", "to": "gate", "kind": "default"},
        {"id": "gate->done", "from": "gate", "to": "done", "kind": "linear"}
    ],
    "nests": [],
    "assets": [],
    "signals": [{"name": "approve", "action": "pause"}],
    "roles": [],
    "effect_summary": "read",
    "provenance": {"source": "compose"}
}"#;

#[test]
fn walk_reaches_the_default_branch_and_classifies_waits() {
    let trajectory = simulate(&document(WAITS_AND_JUDGE));

    assert_eq!(
        trajectory.nodes_visited,
        ["confirm", "done", "gate", "route"],
        "every node reachable on the default branch is visited"
    );
    assert_eq!(trajectory.stubbed_judges, ["route"]);
    assert_eq!(trajectory.simulation_level, "reachability");
    assert_eq!(trajectory.schema_version, 1);
    assert_eq!(
        trajectory
            .predicted_waits
            .iter()
            .map(|wait| (wait.reason, wait.subject.as_str()))
            .collect::<Vec<_>>(),
        [
            (SimulationPredictedWaitReason::Manual, "confirm"),
            (SimulationPredictedWaitReason::Manual, "gate"),
            (SimulationPredictedWaitReason::Signal, "approve"),
        ]
    );
    assert_eq!(
        trajectory.effect_summary,
        SimulationTrajectoryEffectSummary::Read
    );
}

/// A deferred slot is one opaque node: its region never joins the walk or the rollup.
const DEFERRED_NEST: &str = r#"{
    "ir_version": 0,
    "id": "deferred-nest",
    "version": 1,
    "revision": 0,
    "state": "candidate",
    "meta": {
        "description": "opaque pattern slot",
        "kind": "creator",
        "run_intents": [],
        "requires_capabilities": []
    },
    "entry": "slot",
    "terminal": ["done"],
    "nodes": [
        {"id": "slot", "kind": "pattern_slot", "effect_class": "pure",
         "pattern": {"registry_id": "patterns/outline", "version": "1.0.0"}},
        {"id": "child", "kind": "inner_step", "effect_class": "external", "step_kind": "acp_prompt"},
        {"id": "done", "kind": "terminal", "effect_class": "pure"}
    ],
    "edges": [{"id": "slot->done", "from": "slot", "to": "done", "kind": "linear"}],
    "nests": [{
        "id": "nest-outline",
        "parent_node": "slot",
        "expansion": "defer_until_apply",
        "pin_policy": "exact",
        "child": {"kind": "pattern", "registry_id": "patterns/outline", "version": "1.0.0"}
    }],
    "assets": [],
    "signals": [],
    "roles": [],
    "effect_summary": "pure",
    "provenance": {"source": "compose"}
}"#;

#[test]
fn deferred_nest_stays_one_opaque_node() {
    let trajectory = simulate(&document(DEFERRED_NEST));
    assert_eq!(trajectory.nodes_visited, ["done", "slot"]);
    assert!(
        !trajectory.nodes_visited.iter().any(|node| node == "child"),
        "an unexpanded slot body is not walked"
    );
    assert_eq!(
        trajectory.effect_summary,
        SimulationTrajectoryEffectSummary::Pure,
        "the slot contributes its own declared class, not the unresolved body"
    );
}

/// Two expanded nest levels; the deepest child writes a candidate.
const TWO_LEVEL_NEST: &str = r#"{
    "ir_version": 0,
    "id": "two-level-nest",
    "version": 1,
    "revision": 0,
    "state": "candidate",
    "meta": {
        "description": "expanded nest chain",
        "kind": "creator",
        "run_intents": [],
        "requires_capabilities": []
    },
    "entry": "outer",
    "terminal": ["done"],
    "nodes": [
        {"id": "outer", "kind": "nest_group", "effect_class": "pure", "child_workflow_id": "wf-outer"},
        {"id": "inner", "kind": "nest_group", "effect_class": "pure", "child_workflow_id": "wf-inner"},
        {"id": "leaf", "kind": "inner_step", "effect_class": "write_candidate", "step_kind": "acp_prompt"},
        {"id": "done", "kind": "terminal", "effect_class": "pure"}
    ],
    "edges": [
        {"id": "outer->inner", "from": "outer", "to": "inner", "kind": "linear"},
        {"id": "inner->leaf", "from": "inner", "to": "leaf", "kind": "linear"},
        {"id": "leaf->done", "from": "leaf", "to": "done", "kind": "linear"}
    ],
    "nests": [
        {"id": "nest-outer", "parent_node": "outer", "expansion": "expand_in_candidate",
         "pin_policy": "exact",
         "child": {"kind": "workflow", "workflow_id": "wf-outer", "workflow_revision": 1,
                   "content_hash": "aa"}},
        {"id": "nest-inner", "parent_node": "inner", "expansion": "expand_in_candidate",
         "pin_policy": "exact",
         "child": {"kind": "workflow", "workflow_id": "wf-inner", "workflow_revision": 1,
                   "content_hash": "bb"}}
    ],
    "assets": [],
    "signals": [],
    "roles": [],
    "effect_summary": "pure",
    "provenance": {"source": "compose"}
}"#;

#[test]
fn expanded_nest_rolls_effects_up_two_levels_without_mutating_the_input() {
    let document = document(TWO_LEVEL_NEST);
    let before = serde_json::to_string(&document).expect("input serializes");

    let trajectory = simulate(&document);

    assert!(
        trajectory.effect_summary >= SimulationTrajectoryEffectSummary::WriteCandidate,
        "a nested candidate write must not be hidden by its parents, got {:?}",
        trajectory.effect_summary
    );
    assert_eq!(trajectory.nodes_visited, ["done", "inner", "leaf", "outer"]);
    assert_eq!(
        serde_json::to_string(&document).expect("input serializes"),
        before,
        "the input document is never mutated"
    );
}

fn judge_with_defaults(edges: &str) -> WorkflowDocument {
    document(&format!(
        r#"{{
        "ir_version": 0, "id": "judge-shape", "version": 1, "revision": 0, "state": "candidate",
        "meta": {{"description": "judge edges", "kind": "creator", "run_intents": [], "requires_capabilities": []}},
        "entry": "judge", "terminal": ["done"],
        "nodes": [
            {{"id": "judge", "kind": "outer_state", "effect_class": "pure", "exit_when": {{"kind": "llm_judge"}}}},
            {{"id": "done", "kind": "terminal", "effect_class": "pure"}}
        ],
        "edges": {edges},
        "nests": [], "assets": [], "signals": [], "roles": [],
        "effect_summary": "pure", "provenance": {{"source": "compose"}}
    }}"#
    ))
}

#[test]
fn a_judged_node_requires_exactly_one_default_edge() {
    let none = judge_with_defaults(r#"[{"id": "j->d", "from": "judge", "to": "done", "kind": "go"}]"#);
    assert_eq!(
        simulate_document(
            &none,
            SimulationOptions {
                capabilities: CapabilityRegistryHolder::new()
            }
        )
        .unwrap_err(),
        SimulationError::JudgeBranch {
            node: "judge".into(),
            count: 0
        }
    );

    let two = judge_with_defaults(
        r#"[
            {"id": "j->d", "from": "judge", "to": "done", "kind": "default"},
            {"id": "j->j", "from": "judge", "to": "judge", "kind": "default"}
        ]"#,
    );
    assert_eq!(
        simulate_document(
            &two,
            SimulationOptions {
                capabilities: CapabilityRegistryHolder::new()
            }
        )
        .unwrap_err(),
        SimulationError::JudgeBranch {
            node: "judge".into(),
            count: 2
        }
    );
}

#[test]
fn the_same_document_serializes_to_identical_bytes_twice() {
    let document = document(WAITS_AND_JUDGE);
    let first = serde_json::to_string(&simulate(&document)).expect("trajectory serializes");
    let second = serde_json::to_string(&simulate(&document)).expect("trajectory serializes");
    assert_eq!(first, second);
}

/// A capability-bearing candidate: the walk sees the `enter` action and never resolves it.
const CAPABILITY_BEARING: &str = r#"{
    "ir_version": 0,
    "id": "capability-bearing",
    "version": 1,
    "revision": 0,
    "state": "candidate",
    "meta": {
        "description": "one capability enter action",
        "kind": "creator",
        "run_intents": [],
        "requires_capabilities": ["record.probe"]
    },
    "entry": "work",
    "terminal": ["done"],
    "nodes": [
        {"id": "work", "kind": "outer_state", "effect_class": "write_candidate",
         "enter": [{"kind": "capability", "name": "record.probe"}],
         "exit_when": {"kind": "rule"}},
        {"id": "done", "kind": "terminal", "effect_class": "pure"}
    ],
    "edges": [{"id": "work->done", "from": "work", "to": "done", "kind": "linear"}],
    "nests": [],
    "assets": [],
    "signals": [],
    "roles": [],
    "effect_summary": "write_candidate",
    "provenance": {"source": "compose"}
}"#;

struct RecordingCap {
    hits: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl Capability for RecordingCap {
    fn name(&self) -> &'static str {
        "record.probe"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object"}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object"}"#
    }

    async fn run(&self, _input: serde_json::Value) -> Result<serde_json::Value, CapabilityError> {
        self.hits.fetch_add(1, Ordering::SeqCst);
        Ok(serde_json::json!({ "recorded": true }))
    }
}

#[test]
fn a_capability_bearing_document_simulates_with_zero_dispatch() {
    let hits = Arc::new(AtomicUsize::new(0));
    let registry = crate::capability::test_support::registry_with(vec![Box::new(RecordingCap {
        hits: Arc::clone(&hits),
    })]);

    let trajectory = simulate_document(
        &document(CAPABILITY_BEARING),
        SimulationOptions {
            capabilities: CapabilityRegistryHolder::with_registry(registry),
        },
    )
    .expect("a capability-bearing document still simulates");

    assert_eq!(trajectory.nodes_visited, ["done", "work"]);
    assert_eq!(
        hits.load(Ordering::SeqCst),
        0,
        "the walk never resolves a capability"
    );
}
