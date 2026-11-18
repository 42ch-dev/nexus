//! Static Simulate over two of the repository's own embedded presets.
//!
//! A judged preset's `next` is an unconditional advance, which the import projects as the judged
//! node's `default` edge — the one branch a reachability walk may follow out of an `llm_judge`
//! node. The essay preset's package name is its declared id, so the projected document id resolves
//! back to the bundle whose templates it names (I-000424).

#![cfg(feature = "simulate")]

use nexus_orchestration::preset_runtime::{validate_ir_buildability, IrTemplateSource};
use nexus_capability::CapabilityRegistryHolder;
use nexus_orchestration::{simulate_document, SimulationOptions};
use nexus_preset::capability_catalog::BuiltinCapabilityCatalog;
use nexus_preset::workflow_ir::import_preset_manifest;
use nexus_workflow_ir::{WorkflowDocument, WorkflowEdgeKind, WorkflowExitWhenKind};

fn imported(id: &str) -> WorkflowDocument {
    let loaded = nexus_preset::load_embedded_preset(id, &BuiltinCapabilityCatalog)
        .unwrap_or_else(|error| panic!("`{id}` loads: {error}"));
    import_preset_manifest(&loaded.manifest)
        .unwrap_or_else(|error| panic!("`{id}` projects: {error}"))
}

fn simulate(document: &WorkflowDocument) -> nexus_contracts::SimulationTrajectory {
    simulate_document(
        document,
        &SimulationOptions {
            capabilities: CapabilityRegistryHolder::new(),
        },
    )
    .expect("the preset simulates")
}

#[test]
fn a_judged_preset_imports_its_branch_edges_and_walks_them() {
    let document = imported("novel-writing");
    let mut judged: Vec<&str> = document
        .nodes
        .iter()
        .filter(|node| {
            node.exit_when
                .as_ref()
                .is_some_and(|exit| exit.kind == WorkflowExitWhenKind::LlmJudge)
        })
        .map(|node| node.id.as_str())
        .collect();
    judged.sort_unstable();
    assert_eq!(judged, ["finalize", "outline_review"]);

    for node in &judged {
        let kinds: Vec<WorkflowEdgeKind> = document
            .edges
            .iter()
            .filter(|edge| edge.from == *node)
            .map(|edge| edge.kind)
            .collect();
        assert_eq!(
            kinds,
            [WorkflowEdgeKind::Default],
            "`{node}` keeps the single branch its judge advances along, not a flattened linear edge"
        );
    }

    // The export half closes the round trip, so the builder still sees the same outer edge.
    validate_ir_buildability(&document, IrTemplateSource::Embedded)
        .expect("the judged preset still builds from its own bundle");

    let trajectory = simulate(&document);
    assert_eq!(trajectory.stubbed_judges, ["finalize", "outline_review"]);
    assert_eq!(
        trajectory.nodes_visited,
        [
            "done",
            "draft_chapter",
            "finalize",
            "finalize_commit",
            "outline_chapter",
            "outline_review"
        ],
        "the judged states are stubbed, not skipped: the whole chain is reachable"
    );
}

#[test]
fn the_essay_preset_ids_round_trip_and_simulate() {
    let document = imported("essay-init");
    assert_eq!(document.id.as_str(), "essay-init");

    let trajectory = simulate(&document);
    assert_eq!(
        trajectory.nodes_visited,
        [
            "collecting_details",
            "committing",
            "done",
            "init_intro",
            "summarizing"
        ]
    );
    assert_eq!(trajectory.stubbed_judges, Vec::<String>::new());

    // The bundle answers for the id the document projects; a manufactured id could not.
    validate_ir_buildability(&document, IrTemplateSource::Embedded)
        .expect("the essay preset builds from its own bundle");
}
