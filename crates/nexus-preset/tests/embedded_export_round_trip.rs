//! Round-trip gate: every embedded preset's bundle exports back to the manifest it was imported
//! from, on the fields the round-trip contract names — `preset.world_binding` and every
//! `states[].context_update` block.
//!
//! The comparison basis is the YAML value, not the typed manifest: the typed form folds an absent
//! `body:` into a present empty one, so a typed comparison reports `research` as faithful today.
//! Absence is part of the contract, and only the value representation keeps it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use nexus_preset::manifest::PresetManifest;
use nexus_preset::{export_preset, import_preset_bundle, FsAssetSource};
use serde_yaml::Value;

fn preset_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("embedded-presets")
}

/// Embedded preset ids (sorted): every directory under `embedded-presets/` holding a `preset.yaml`.
fn preset_ids() -> Vec<String> {
    let mut ids: Vec<String> = std::fs::read_dir(preset_root())
        .expect("read embedded-presets/")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("preset.yaml").is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    ids.sort();
    ids
}

/// One bundle's source manifest value and its re-exported manifest value.
fn round_trip(id: &str) -> Result<(Value, Value), String> {
    let bundle = preset_root().join(id);
    let manifest_path = bundle.join("preset.yaml");
    let yaml = std::fs::read_to_string(&manifest_path)
        .map_err(|error| format!("read {}: {error}", manifest_path.display()))?;
    let source: Value = serde_yaml::from_str(&yaml)
        .map_err(|error| format!("parse {} as YAML: {error}", manifest_path.display()))?;
    let manifest: PresetManifest = serde_yaml::from_str(&yaml)
        .map_err(|error| format!("parse {} as a manifest: {error}", manifest_path.display()))?;
    let assets = FsAssetSource::new(&bundle).map_err(|error| format!("open `{id}`: {error}"))?;
    let imported = import_preset_bundle(&manifest, &assets)
        .map_err(|error| format!("import `{id}`: {error}"))?;
    let exported =
        export_preset(&imported.document).map_err(|error| format!("export `{id}`: {error}"))?;
    let exported =
        serde_yaml::to_value(&exported).map_err(|error| format!("serialize `{id}`: {error}"))?;
    Ok((source, exported))
}

/// The contract's named fields of one manifest value, keyed by YAML path. An absent field has no
/// entry, so an export that drops or invents one shows up as a key-set difference.
fn named_fields(manifest: &Value) -> BTreeMap<String, Value> {
    let mut fields = BTreeMap::new();
    if let Some(binding) = manifest
        .get("preset")
        .and_then(|header| header.get("world_binding"))
    {
        fields.insert("preset.world_binding".to_string(), binding.clone());
    }
    for state in manifest
        .get("states")
        .and_then(Value::as_sequence)
        .into_iter()
        .flatten()
    {
        let Some(id) = state.get("id").and_then(Value::as_str) else {
            continue;
        };
        if let Some(update) = state.get("context_update") {
            fields.insert(format!("states.{id}.context_update"), update.clone());
        }
    }
    fields
}

#[test]
fn every_embedded_preset_round_trips_the_contract_fields() {
    let ids = preset_ids();
    let mut failures = Vec::new();
    let mut world_bound = 0;
    let mut bodyless_append = 0;

    for id in &ids {
        let (source, exported) = match round_trip(id) {
            Ok(values) => values,
            Err(failure) => {
                failures.push(failure);
                continue;
            }
        };
        let source = named_fields(&source);
        let exported = named_fields(&exported);

        if source.contains_key("preset.world_binding") {
            world_bound += 1;
        }
        for update in source.values() {
            let Some(op) = update.get("op") else {
                continue;
            };
            if op.get("kind").and_then(Value::as_str) == Some("append") && op.get("body").is_none()
            {
                bodyless_append += 1;
            }
        }

        for (path, expected) in &source {
            match exported.get(path) {
                Some(actual) if actual == expected => {}
                Some(actual) => failures.push(format!(
                    "`{id}` {path} changed through the IR: source {expected:?}, export {actual:?}"
                )),
                None => failures.push(format!("`{id}` {path} lost on export")),
            }
        }
        for path in exported.keys().filter(|path| !source.contains_key(*path)) {
            failures.push(format!("`{id}` {path} invented by export"));
        }
    }

    assert!(
        failures.is_empty(),
        "embedded preset(s) whose export does not reproduce their source manifest:\n{}",
        failures.join("\n")
    );
    // A corpus change that dropped both shapes would let this gate pass while comparing nothing
    // that ever failed; the two counts keep each half of the contract exercised.
    assert!(
        world_bound > 0,
        "no embedded preset declares `preset.world_binding`"
    );
    assert!(
        bodyless_append > 0,
        "no embedded preset declares a bodyless `append` context update"
    );
}
