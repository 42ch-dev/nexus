//! Every embedded preset's document builds from its own compiled-in bundle.
//!
//! An embedded run now EXECUTES the document `embedded_workflow_document` constructs, so a
//! preset the IR builder cannot project — an unmappable concept, or an instruction template
//! that does not resolve from the embedded tree — would stop running where the YAML path
//! used to. This is the route's buildability gate; it reads no twin and no filesystem copy
//! of a preset.
// note: the loop covers every embedded preset directory on disk.

use std::path::Path;

use nexus_orchestration::preset_runtime::{validate_ir_buildability, IrTemplateSource};
use nexus_preset::capability_catalog::BuiltinCapabilityCatalog;

fn embedded_ids() -> Vec<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("nexus-preset")
        .join("embedded-presets");
    let mut ids: Vec<String> = std::fs::read_dir(&root)
        .unwrap_or_else(|error| panic!("read {}: {error}", root.display()))
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("preset.yaml").is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    ids.sort();
    ids
}

#[test]
fn every_embedded_document_builds_from_the_compiled_in_bundle() {
    let ids = embedded_ids();
    assert!(
        !ids.is_empty(),
        "embedded-presets/ must hold at least one preset"
    );
    for id in ids {
        let loaded = nexus_preset::load_embedded_preset(&id, &BuiltinCapabilityCatalog)
            .unwrap_or_else(|error| panic!("`{id}` loads: {error}"));
        let document = nexus_preset::embedded_workflow_document(&loaded).unwrap_or_else(|error| {
            panic!("`{id}` constructs from its compiled-in bundle: {error}")
        });
        validate_ir_buildability(&document, IrTemplateSource::Embedded).unwrap_or_else(|error| {
            panic!(
                "`{id}` must build from the document it executes (its embedded bundle is the \
                 document's own source): {error}"
            )
        });
    }
}
