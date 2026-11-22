//! The document an embedded preset executes is constructed from its own compiled-in bundle.
//!
//! Two proofs, one file: the embedded route's document is byte-identical to the committed
//! twin for every preset (so it is the same document the fs route and the twin gate already
//! agree on), and every identity that is not this preset's own `Embedded` source is refused
//! instead of executing through the compiled-in tree.

use std::path::{Path, PathBuf};

use nexus_preset::capability_catalog::BuiltinCapabilityCatalog;
use nexus_preset::source_identity::PresetSourceIdentity;
use nexus_preset::{
    canonical_twin_json, embedded_workflow_document, load_embedded_preset, WorkflowIrError,
};

/// Suffix of one twin file: `<id>.workflow.ir.json`, the fixture convention.
const TWIN_SUFFIX: &str = ".workflow.ir.json";

fn preset_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("embedded-presets")
}

fn twin_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("ir-twins")
}

/// Embedded preset ids (sorted): every directory under `embedded-presets/` holding a
/// `preset.yaml`.
fn embedded_ids() -> Vec<String> {
    let mut ids: Vec<String> = std::fs::read_dir(preset_root())
        .expect("read embedded-presets/")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("preset.yaml").is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    ids.sort();
    ids
}

/// The embedded route's document is the committed twin's document, for every preset.
///
/// Equality here is what lets an embedded run execute the constructed document: the twin
/// gate proves the fs route and the committed bytes agree, so this proves the compiled-in
/// path agrees with both.
#[test]
fn every_embedded_preset_constructs_its_committed_twin() {
    let caps = BuiltinCapabilityCatalog;
    for id in embedded_ids() {
        let loaded = load_embedded_preset(&id, &caps)
            .unwrap_or_else(|error| panic!("`{id}` loads: {error}"));
        let document = embedded_workflow_document(&loaded).unwrap_or_else(|error| {
            panic!("`{id}` constructs from its compiled-in bundle: {error}")
        });
        let constructed = canonical_twin_json(&document)
            .unwrap_or_else(|error| panic!("`{id}` serializes: {error}"));
        let twin_path = twin_root().join(format!("{id}{TWIN_SUFFIX}"));
        let twin = std::fs::read_to_string(&twin_path)
            .unwrap_or_else(|error| panic!("read {}: {error}", twin_path.display()));
        assert_eq!(
            constructed, twin,
            "`{id}`: the embedded route must construct the document its committed twin proves"
        );
    }
}

/// A non-embedded identity is refused: a user or system bundle must not execute through the
/// compiled-in tree, and the refusal names what was wrong.
#[test]
fn a_bundle_that_is_not_embedded_is_refused() {
    let caps = BuiltinCapabilityCatalog;
    let mut loaded = load_embedded_preset("novel-writing", &caps).expect("the preset loads");
    let embedded_identity = loaded
        .source_identity
        .clone()
        .expect("an embedded load carries its identity");

    loaded.source_identity = Some(PresetSourceIdentity::Directory {
        root: PathBuf::from("/tmp/not-an-embedded-bundle"),
        content_hash: [0u8; 32],
    });
    let error = embedded_workflow_document(&loaded)
        .expect_err("a directory bundle must not construct from the compiled-in tree");
    assert!(
        matches!(error, WorkflowIrError::NotProjectable { .. }),
        "the refusal is a projection refusal: {error}"
    );

    loaded.source_identity = None;
    assert!(
        embedded_workflow_document(&loaded).is_err(),
        "a load with no source identity is refused"
    );

    loaded.source_identity = Some(PresetSourceIdentity::Embedded {
        preset_id: "novel-write".to_string(),
        content_hash: [0u8; 32],
    });
    let error = embedded_workflow_document(&loaded)
        .expect_err("an embedded identity naming another preset is refused");
    assert!(
        error.to_string().contains("manifest's own id"),
        "the refusal names the mismatch: {error}"
    );

    // the identity and not about the manifest.
    loaded.source_identity = Some(embedded_identity);
    embedded_workflow_document(&loaded).expect("the embedded identity constructs");
}
