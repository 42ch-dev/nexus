//! Round-trip fixtures for the two named strategy samples (Workflow IR v0).
//!
//! Each sample bundle is loaded from `strategy-samples/`, imported into a `WorkflowDocument`, and
//! checked three ways: the committed golden fixture is the adapter's exact document and is its own
//! canonical JSON serialization; the typed `PresetManifest -> WorkflowDocument -> PresetManifest`
//! round trip preserves the manifest; and the asset map carries every referenced template with the
//! bytes and BLAKE3 hash the adapter produced. Executable carriers (`signals`, `roles`,
//! `initial_action`, inner-graph `output_binding`, `exit_when`) are asserted directly, never through
//! YAML formatting.

use std::path::{Path, PathBuf};

use nexus_preset::manifest::PresetManifest;
use nexus_preset::{export_preset, import_preset_bundle, PresetIrImport};
use nexus_workflow_ir::{
    WorkflowDocument, WorkflowEdgeKind, WorkflowExitWhenKind, WorkflowInitialActionKind,
    WorkflowSignalBindingAction,
};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sample_root(id: &str) -> PathBuf {
    repo_root().join("strategy-samples").join(id)
}

fn fixture_path(id: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/workflow-ir")
        .join(format!("{id}.workflow.ir.json"))
}

fn output_binding<'a>(document: &'a WorkflowDocument, node_id: &str) -> Option<&'a str> {
    document
        .nodes
        .iter()
        .find(|node| node.id == node_id)
        .and_then(|node| node.output_binding.as_deref())
}

/// Import one real sample bundle and assert its golden JSON, typed manifest round trip, and asset
/// map.
fn round_trip(id: &str) -> PresetIrImport {
    let bundle = sample_root(id);
    let yaml = std::fs::read_to_string(bundle.join("preset.yaml")).expect("read preset.yaml");
    let manifest: PresetManifest = serde_yaml::from_str(&yaml).expect("parse preset.yaml");
    let imported = import_preset_bundle(&manifest, &bundle).expect("import preset bundle");

    // Golden JSON: deserialize -> serialize is byte-identical to the committed fixture.
    let golden = std::fs::read_to_string(fixture_path(id)).expect("read golden fixture");
    let document: WorkflowDocument =
        serde_json::from_str(&golden).expect("golden fixture deserializes");
    let mut reserialized = serde_json::to_string_pretty(&document).expect("serialize golden");
    reserialized.push('\n');
    assert_eq!(
        reserialized, golden,
        "`{id}` golden fixture is not canonical"
    );

    // The fixture is the adapter's real output, not a hand-edited document.
    assert_eq!(
        serde_json::to_value(&document).expect("golden json"),
        serde_json::to_value(&imported.document).expect("imported json"),
        "`{id}` golden fixture drifted from the adapter output"
    );

    // Typed round trip: PresetManifest -> WorkflowDocument -> PresetManifest.
    let exported = export_preset(&imported.document).expect("export document");
    assert_eq!(
        serde_json::to_value(&exported).expect("exported json"),
        serde_json::to_value(&manifest).expect("manifest json"),
        "`{id}` manifest changed through the IR"
    );

    // Asset map: every referenced template has one entry with its bytes and BLAKE3 hash.
    assert_eq!(
        imported.document.assets.len(),
        imported.assets.len(),
        "`{id}` asset map size"
    );
    for asset in &imported.document.assets {
        assert_eq!(asset.id, asset.path, "`{id}` asset id is its bundle path");
        let bytes = std::fs::read(bundle.join(&asset.path)).expect("read referenced asset");
        assert_eq!(imported.assets[&asset.path], bytes, "`{id}` asset bytes");
        assert_eq!(
            asset.content_hash.as_deref(),
            Some(blake3::hash(&bytes).to_hex().to_string().as_str()),
            "`{id}` asset hash"
        );
    }

    imported
}

#[test]
fn game_narrative_round_trips_preset_ir() {
    let imported = round_trip("game-narrative");
    let document = &imported.document;

    assert_eq!(document.id.as_str(), "game-narrative");
    assert_eq!(document.assets.len(), 6);

    // The declared signal can force the trigger lane.
    assert_eq!(document.signals.len(), 1);
    assert_eq!(document.signals[0].name, "game_event");
    assert_eq!(
        document.signals[0].action,
        WorkflowSignalBindingAction::ForceTransition
    );
    assert_eq!(
        document.signals[0].target.as_deref(),
        Some("trigger_extract")
    );

    assert_eq!(
        document.initial_action.as_ref().map(|action| action.kind),
        Some(WorkflowInitialActionKind::SeedDirect)
    );

    // Both lanes export `extract_characters.text`; the owning inner step carries it.
    assert_eq!(
        output_binding(document, "trigger_graph/extract_characters"),
        Some("extract_characters.text")
    );
    assert_eq!(
        output_binding(document, "sweep_graph/extract_characters"),
        Some("extract_characters.text")
    );

    // The sample declares no roles, so the document invents none.
    assert!(document.roles.is_empty());
}

#[test]
fn react_trpg_turn_round_trips_preset_ir() {
    let imported = round_trip("react-trpg-turn");
    let document = &imported.document;

    assert_eq!(document.id.as_str(), "react-trpg-turn");
    assert_eq!(document.assets.len(), 5);

    // No signals or roles declared; the lane selector is a judge edge pair.
    assert!(document.signals.is_empty());
    assert!(document.roles.is_empty());
    let go = document
        .edges
        .iter()
        .find(|edge| edge.id == "routing->mechanical_turn")
        .expect("mechanical lane edge");
    assert_eq!(go.kind, WorkflowEdgeKind::Go);
    let nogo = document
        .edges
        .iter()
        .find(|edge| edge.id == "routing->natural_language_turn")
        .expect("natural-language lane edge");
    assert_eq!(nogo.kind, WorkflowEdgeKind::Nogo);

    // Both lanes export the narration text; the owning inner step carries it.
    assert_eq!(
        output_binding(document, "mechanical_graph/receipt_narration"),
        Some("receipt_narration.text")
    );
    assert_eq!(
        output_binding(document, "natural_language_graph/receipt_narration"),
        Some("receipt_narration.text")
    );

    // The player response point stays a manual exit (no automatic transition).
    let wait = document
        .nodes
        .iter()
        .find(|node| node.id == "wait_for_player")
        .expect("wait_for_player node");
    assert_eq!(
        wait.exit_when.as_ref().map(|exit| exit.kind),
        Some(WorkflowExitWhenKind::Manual)
    );
}
