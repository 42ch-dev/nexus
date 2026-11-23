//! Round-trip gate: every embedded preset's bundle exports back to the manifest it was imported
//! from, on the fields the round-trip contract names — `preset.world_binding` and every
//! `states[].context_update` block.
//!
//! The comparison basis is the YAML value, not the typed manifest: the typed form hides the
//! defaults and skipped fields of the source, so a typed comparison cannot say the export
//! reproduced the author's file. Presence is part of the contract, and only the value
//! representation keeps it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use nexus_preset::manifest::PresetManifest;
use nexus_preset::{export_preset, import_preset_bundle, AssetSource, FsAssetSource};
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
    assert!(
        world_bound > 0,
        "no embedded preset declares `preset.world_binding`"
    );
    assert!(
        bodyless_append > 0,
        "no embedded preset declares a bodyless `append` context update"
    );
}

/// Asset source for [`payload_shapes_round_trip`]: the shape fixture owns no files, so every
/// referenced template reads as empty bytes.
struct EmptyAssets;

impl AssetSource for EmptyAssets {
    fn read_asset(&self, path: &str) -> std::io::Result<Option<Vec<u8>>> {
        Ok(Some(path.as_bytes().to_vec()))
    }
}

/// Payload presence gate: the five `context_update` payload shapes the embedded corpus does not
/// carry — absent body, empty string, non-empty patch, explicit `null` patch, and a `struct_merge`
/// with no `patch` key at all — reach the export unchanged.
///
/// The two `struct_merge` payload forms are the ones that regressed: a single `Option<Value>`
/// folds an explicit `null` into the absent key, and an unconditional reverse map invents
/// `patch: null` where the author wrote no patch.
#[test]
fn payload_shapes_round_trip() {
    const FIXTURE: &str = r#"
preset:
  id: payload-shapes
  version: 1
  kind: creator
  description: context_update payload presence
  initial: absent
  terminal: done
states:
  - id: absent
    next: done
    context_update:
      op: { kind: append }
      template_file: prompts/absent.md
  - id: empty
    next: done
    context_update:
      op: { kind: append, body: "" }
      template_file: prompts/empty.md
  - id: merged
    next: done
    context_update:
      op: { kind: struct_merge, patch: { tone: terse } }
      template_file: prompts/merged.md
  - id: nulled
    next: done
    context_update:
      op: { kind: struct_merge, patch: null }
      template_file: prompts/nulled.md
  - id: unpatched
    next: done
    context_update:
      op: { kind: struct_merge }
      template_file: prompts/unpatched.md
  - id: done
    terminal: true
"#;

    let manifest: PresetManifest = serde_yaml::from_str(FIXTURE).expect("fixture is a manifest");
    let imported = import_preset_bundle(&manifest, &EmptyAssets).expect("import the shape fixture");
    let exported = export_preset(&imported.document).expect("export the shape fixture");
    let source: Value = serde_yaml::from_str(FIXTURE).expect("fixture is YAML");
    let exported = serde_yaml::to_value(&exported).expect("serialize the shape export");

    let source = named_fields(&source);
    let exported = named_fields(&exported);
    assert_eq!(
        source, exported,
        "context_update payload shapes changed through the IR"
    );

    let op = |state: &str| {
        source[&format!("states.{state}.context_update")]
            .get("op")
            .expect("context_update carries an op")
    };
    assert_eq!(op("absent").get("body"), None, "absent body");
    assert_eq!(
        op("empty").get("body").and_then(Value::as_str),
        Some(""),
        "empty-string body"
    );
    assert!(
        op("merged").get("patch").is_some_and(Value::is_mapping),
        "non-empty patch"
    );
    assert_eq!(
        op("nulled").get("patch"),
        Some(&Value::Null),
        "explicit null patch"
    );
    assert_eq!(
        exported["states.unpatched.context_update"]
            .get("op")
            .expect("context_update carries an op")
            .get("patch"),
        None,
        "absent patch exported as a key"
    );
}
