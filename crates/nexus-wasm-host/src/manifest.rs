//! Module manifest (`manifest.json`) deserialization.
//!
//! A compute module ships a `manifest.json` alongside its compiled `.wasm`.
//! The manifest declares the module's identity, its required input surface, and
//! optional sandbox overrides. This resolves open design item #3 (which fields
//! are required vs optional).
//!
//! ## Required fields
//!
//! | Field | Type | Meaning |
//! | --- | --- | --- |
//! | `module_id` | string | Unique module identifier (matches the directory name). |
//! | `name` | string | Human-readable name. |
//! | `version` | string | Module `SemVer` (independent of the Nexus ABI version). |
//! | `nexus_abi_version` | integer | Compute envelope ABI version this module targets (`1` for V1.61). |
//! | `required_key_block_types` | array&lt;string&gt; | `BlockTypes` the module reads from the KB snapshot (e.g. `["character"]`). The host uses this to select which `KeyBlocks` to bundle into `ComputeInput`. |
//! | `compute_export` | string | Name of the WASM export implementing `compute`. |
//! | `init_export` | string | Name of the WASM export implementing `init` (called once after instantiation if present). |
//!
//! ## Optional fields
//!
//! | Field | Type | Default | Meaning |
//! | --- | --- | --- | --- |
//! | `description` | string | — | Free-form description. |
//! | `author` | string | — | Author attribution. |
//! | `host_functions` | array&lt;string&gt; | `[]` | Subset of `["kb_read", "narrative_query"]` the module may call. Only whitelisted names are linked into the instance. |
//! | `battle_report_kind` | string | module-declared | Discriminator the module emits in `battle_report.kind`. |
//! | `max_fuel` | integer | host `SandboxConfig` | Per-invocation fuel override. |
//! | `max_memory_mib` | integer | host `SandboxConfig` | Per-invocation memory-cap override (MiB). |
//! | `max_wall_time_ms` | integer | host `SandboxConfig` | Per-invocation wall-time override (ms). |
//! | `wasm_sha256` | string | — | SHA-256 (64 lowercase hex chars) of the compiled `.wasm` bytes this manifest pairs with. Operators SHOULD set it: the loader then verifies content-based pairing and rejects a mismatched pair before it can be compiled or cached. Omitted (legacy manifests) → stat-fence fallback. |

use std::collections::HashMap;

use nexus_contracts::generated::daemon_api::compute::module_detail::{
    ModuleDetail, ModuleDetailHostFunctionsItem, ModuleDetailSchemas,
};

// V1.170 P0 (AR-8): the manifest types + validation core (`allows`,
// `verify_wasm_sha256`) moved to the shared `nexus-module-manifest` crate.
// This module re-exports them so every existing importer (`host.rs`,
// `compute.rs`, `module_cache.rs`, `registry.rs`, daemon handlers, tests)
// keeps compiling unchanged.
pub use nexus_module_manifest::{HostFunction, ModuleManifest, ModuleSchemas};

fn json_object_to_map(value: &serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    value.as_object().cloned().unwrap_or_default()
}

fn schema_fragment_maps_to_detail(
    src: Option<&HashMap<String, serde_json::Value>>,
) -> HashMap<String, serde_json::Map<String, serde_json::Value>> {
    src.map(|fragments| {
        fragments
            .iter()
            .map(|(k, v)| (k.clone(), json_object_to_map(v)))
            .collect()
    })
    .unwrap_or_default()
}

fn module_schemas_to_detail(schemas: &ModuleSchemas) -> ModuleDetailSchemas {
    ModuleDetailSchemas {
        key_block_attributes: schema_fragment_maps_to_detail(schemas.key_block_attributes.as_ref()),
        key_block_state: schema_fragment_maps_to_detail(schemas.key_block_state.as_ref()),
        invocation: schemas
            .invocation
            .as_ref()
            .map(json_object_to_map)
            .unwrap_or_default(),
        battle_report: schemas
            .battle_report
            .as_ref()
            .map(json_object_to_map)
            .unwrap_or_default(),
    }
}

/// Typed conversion from the runtime manifest to the generated wire detail.
///
/// (V1.170 P0, AR-8 note: this was `impl From<&ModuleManifest> for ModuleDetail`,
/// but the orphan rule forbids a foreign trait impl over two foreign types —
/// `ModuleManifest` now lives in `nexus-module-manifest` — so it is a free
/// function. The field mapping is unchanged.)
///
/// Because this function maps every field explicitly, manifest↔generated drift
/// becomes a **compile error**:
///
/// * Removing a field from [`ModuleDetail`] without updating this function
///   fails `cargo build -p nexus-wasm-host`.
/// * Adding a field to [`ModuleManifest`] without mapping it here is caught by
///   the `from_manifest_maps_all_fields` test.
///
/// This conversion catches changes to [`ModuleDetail`] (generated) — if a field
/// is removed from the generated type, this function won't compile. It does NOT
/// catch a field added to [`ModuleManifest`] that is not in [`ModuleDetail`]
/// (that would silently drop). The `schema_drift_detection` gate catches
/// schema↔generated drift; this function catches generated-side structural
/// changes.
///
/// This is stronger than the previous JSON round-trip, which silently dropped
/// mismatched fields.
pub fn manifest_to_detail(manifest: &ModuleManifest) -> ModuleDetail {
    ModuleDetail {
        module_id: manifest.module_id.clone(),
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        nexus_abi_version: i64::from(manifest.nexus_abi_version),
        required_key_block_types: manifest.required_key_block_types.clone(),
        compute_export: manifest.compute_export.clone(),
        init_export: manifest.init_export.clone(),
        description: manifest.description.clone(),
        author: manifest.author.clone(),
        host_functions: manifest
            .host_functions
            .iter()
            .map(|f| match f {
                HostFunction::KbRead => ModuleDetailHostFunctionsItem::KbRead,
                HostFunction::NarrativeQuery => ModuleDetailHostFunctionsItem::NarrativeQuery,
            })
            .collect(),
        schemas: manifest.schemas.as_ref().map(module_schemas_to_detail),
        battle_report_kind: manifest.battle_report_kind.clone(),
        max_fuel: manifest
            .max_fuel
            .map(|v| i64::try_from(v).unwrap_or(i64::MAX)),
        max_memory_mib: manifest.max_memory_mib.map(i64::from),
        max_wall_time_ms: manifest
            .max_wall_time_ms
            .map(|v| i64::try_from(v).unwrap_or(i64::MAX)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_combat_manifest() {
        let json = r#"{
            "module_id": "basic-combat",
            "name": "Basic Combat",
            "version": "1.0.0",
            "nexus_abi_version": 1,
            "required_key_block_types": ["character"],
            "compute_export": "compute",
            "init_export": "init",
            "host_functions": ["kb_read"],
            "battle_report_kind": "combat"
        }"#;
        let m: ModuleManifest = serde_json::from_str(json).unwrap();
        assert_eq!(m.module_id, "basic-combat");
        assert_eq!(m.required_key_block_types, vec!["character".to_string()]);
        assert!(m.allows(HostFunction::KbRead));
        assert!(!m.allows(HostFunction::NarrativeQuery));
        assert_eq!(m.battle_report_kind.as_deref(), Some("combat"));
    }

    #[test]
    fn optional_fields_default_to_none() {
        let json = r#"{
            "module_id": "m",
            "name": "M",
            "version": "0.1.0",
            "nexus_abi_version": 1,
            "required_key_block_types": [],
            "compute_export": "compute",
            "init_export": "init"
        }"#;
        let m: ModuleManifest = serde_json::from_str(json).unwrap();
        assert!(m.host_functions.is_empty());
        assert!(m.max_fuel.is_none());
        assert!(m.description.is_none());
    }

    #[test]
    fn parses_manifest_with_schemas_block() {
        let json = r#"{
            "module_id": "test-mod",
            "name": "Test Module",
            "version": "1.0.0",
            "nexus_abi_version": 1,
            "required_key_block_types": ["character"],
            "compute_export": "compute",
            "init_export": "init",
            "schemas": {
                "key_block_attributes": {
                    "character": {
                        "type": "object",
                        "properties": {
                            "max_hp": {"type": "integer", "minimum": 0}
                        },
                        "required": ["max_hp"]
                    }
                },
                "invocation": {
                    "type": "object",
                    "properties": {
                        "attacker_id": {"type": "string"}
                    }
                }
            }
        }"#;
        let m: ModuleManifest = serde_json::from_str(json).unwrap();
        let schemas = m.schemas.expect("schemas should be present");
        assert!(schemas.key_block_attributes.is_some());
        assert!(schemas.key_block_state.is_none());
        assert!(schemas.invocation.is_some());
        assert!(schemas.battle_report.is_none());
        let attrs = schemas.key_block_attributes.unwrap();
        assert!(attrs.contains_key("character"));
        let char_schema = attrs.get("character").unwrap();
        assert_eq!(char_schema["required"][0].as_str().unwrap(), "max_hp");
    }

    #[test]
    fn manifest_without_schemas_is_backward_compat() {
        // V1.61 manifests omit `schemas` → deserializes with schemas = None.
        let json = r#"{
            "module_id": "legacy-mod",
            "name": "Legacy Module",
            "version": "1.0.0",
            "nexus_abi_version": 1,
            "required_key_block_types": [],
            "compute_export": "compute",
            "init_export": "init"
        }"#;
        let m: ModuleManifest = serde_json::from_str(json).unwrap();
        assert!(m.schemas.is_none(), "V1.61 manifest must have schemas=None");
    }

    #[test]
    fn from_manifest_maps_all_fields() {
        let schemas = ModuleSchemas {
            key_block_attributes: Some(HashMap::from([(
                "character".to_string(),
                serde_json::json!({"type": "object"}),
            )])),
            key_block_state: None,
            invocation: Some(serde_json::json!({"type": "object"})),
            battle_report: None,
        };
        let manifest = ModuleManifest {
            module_id: "test-module".to_string(),
            name: "Test Module".to_string(),
            version: "1.2.3".to_string(),
            nexus_abi_version: 1,
            required_key_block_types: vec!["character".to_string()],
            compute_export: "compute".to_string(),
            init_export: "init".to_string(),
            description: Some("A test module".to_string()),
            author: Some("tester".to_string()),
            host_functions: vec![HostFunction::KbRead, HostFunction::NarrativeQuery],
            schemas: Some(schemas),
            battle_report_kind: Some("combat".to_string()),
            max_fuel: Some(42),
            max_memory_mib: Some(128),
            max_wall_time_ms: Some(60_000),
            wasm_sha256: None,
        };

        let detail = manifest_to_detail(&manifest);

        assert_eq!(detail.module_id, "test-module");
        assert_eq!(detail.name, "Test Module");
        assert_eq!(detail.version, "1.2.3");
        assert_eq!(detail.nexus_abi_version, 1);
        assert_eq!(
            detail.required_key_block_types,
            vec!["character".to_string()]
        );
        assert_eq!(detail.compute_export, "compute");
        assert_eq!(detail.init_export, "init");
        assert_eq!(detail.description.as_deref(), Some("A test module"));
        assert_eq!(detail.author.as_deref(), Some("tester"));
        assert_eq!(
            detail
                .host_functions
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["kb_read".to_string(), "narrative_query".to_string()]
        );
        assert!(detail.schemas.is_some());
        assert_eq!(detail.battle_report_kind.as_deref(), Some("combat"));
        assert_eq!(detail.max_fuel, Some(42));
        assert_eq!(detail.max_memory_mib, Some(128));
        assert_eq!(detail.max_wall_time_ms, Some(60_000));
    }

    #[test]
    fn manifest_with_empty_schemas_object() {
        // A manifest with `schemas: {}` should parse with all sub-fields None.
        let json = r#"{
            "module_id": "empty-schemas",
            "name": "Empty Schemas",
            "version": "1.0.0",
            "nexus_abi_version": 1,
            "required_key_block_types": [],
            "compute_export": "compute",
            "init_export": "init",
            "schemas": {}
        }"#;
        let m: ModuleManifest = serde_json::from_str(json).unwrap();
        let schemas = m
            .schemas
            .expect("schemas should be present (even if empty)");
        assert!(schemas.key_block_attributes.is_none());
        assert!(schemas.key_block_state.is_none());
        assert!(schemas.invocation.is_none());
        assert!(schemas.battle_report.is_none());
    }

    #[test]
    fn wasm_sha256_field_is_backward_compat_absent() {
        // V1.154 manifests omit `wasm_sha256` → deserializes with None and
        // verification is skipped (legacy stat-fence fallback).
        let json = r#"{
            "module_id": "legacy-mod",
            "name": "Legacy Module",
            "version": "1.0.0",
            "nexus_abi_version": 1,
            "required_key_block_types": [],
            "compute_export": "compute",
            "init_export": "init"
        }"#;
        let m: ModuleManifest = serde_json::from_str(json).unwrap();
        assert!(
            m.wasm_sha256.is_none(),
            "absent field must deserialize to None"
        );
        assert!(m.verify_wasm_sha256(b"any bytes").is_ok());
    }

    #[test]
    fn verify_wasm_sha256_accepts_matching_bytes() {
        let manifest = ModuleManifest {
            module_id: "m".to_string(),
            name: "M".to_string(),
            version: "1.0.0".to_string(),
            nexus_abi_version: 1,
            required_key_block_types: vec![],
            compute_export: "compute".to_string(),
            init_export: "init".to_string(),
            description: None,
            author: None,
            host_functions: vec![],
            schemas: None,
            battle_report_kind: None,
            max_fuel: None,
            max_memory_mib: None,
            max_wall_time_ms: None,
            wasm_sha256: Some(
                "136f0dec77ef3c5570737642efa4c7e150d23a492a37fc5b2eff183ef7084f02".to_string(),
            ),
        };
        assert!(
            manifest.verify_wasm_sha256(b"hello wasm").is_ok(),
            "matching bytes must verify"
        );
    }

    #[test]
    fn verify_wasm_sha256_rejects_mismatched_bytes() {
        let manifest = ModuleManifest {
            module_id: "m".to_string(),
            name: "M".to_string(),
            version: "1.0.0".to_string(),
            nexus_abi_version: 1,
            required_key_block_types: vec![],
            compute_export: "compute".to_string(),
            init_export: "init".to_string(),
            description: None,
            author: None,
            host_functions: vec![],
            schemas: None,
            battle_report_kind: None,
            max_fuel: None,
            max_memory_mib: None,
            max_wall_time_ms: None,
            wasm_sha256: Some("0".repeat(64)),
        };
        let err = manifest
            .verify_wasm_sha256(b"hello wasm")
            .expect_err("mismatched bytes must fail");
        assert!(
            err.contains("wasm does not match manifest wasm_sha256"),
            "mismatch message must carry the pairing error: {err}"
        );
    }
}
