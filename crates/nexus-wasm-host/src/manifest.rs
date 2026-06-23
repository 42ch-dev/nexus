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

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Whitelisted host functions a module may import (open design item #4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostFunction {
    /// `nexus::kb_read` — read a `KeyBlock` by ID from the invocation snapshot.
    KbRead,
    /// `nexus::narrative_query` — query narrative context.
    NarrativeQuery,
}

/// Module schemas — inline JSON-Schema fragments for per-module
/// input/output validation (V1.62 manifest dynamics).
///
/// Every sub-field is optional: a manifest may declare none, some, or
/// all four fragments. Omitted fields → no validation for that aspect.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[allow(clippy::derive_partial_eq_without_eq)]
// ^ `serde_json::Value` in field types does not implement `Eq`.
pub struct ModuleSchemas {
    /// Per-BlockType attribute shape fragments (immutable compute params).
    /// Keyed by `block_type` (e.g. "character"). Skipped if absent.
    #[serde(default)]
    pub key_block_attributes: Option<HashMap<String, serde_json::Value>>,
    /// Per-BlockType state shape fragments (mutable runtime data).
    #[serde(default)]
    pub key_block_state: Option<HashMap<String, serde_json::Value>>,
    /// Shape for the `ComputeInput.invocation` freeform field.
    #[serde(default)]
    pub invocation: Option<serde_json::Value>,
    /// Shape for the `ComputeOutput.battle_report` freeform field.
    #[serde(default)]
    pub battle_report: Option<serde_json::Value>,
}

/// Module manifest (`manifest.json`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModuleManifest {
    pub module_id: String,
    pub name: String,
    pub version: String,
    pub nexus_abi_version: u32,
    pub required_key_block_types: Vec<String>,
    pub compute_export: String,
    pub init_export: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    /// Whitelisted host functions the module may call. Defaults to none.
    #[serde(default)]
    pub host_functions: Vec<HostFunction>,
    /// Inline JSON-Schema fragments for input/output validation (V1.62).
    /// When declared, the host validates `KeyBlocks`, invocation, and
    /// `battle_report` against these shapes. Omitted → no validation.
    #[serde(default)]
    pub schemas: Option<ModuleSchemas>,
    #[serde(default)]
    pub battle_report_kind: Option<String>,
    #[serde(default)]
    pub max_fuel: Option<u64>,
    #[serde(default)]
    pub max_memory_mib: Option<u32>,
    #[serde(default)]
    pub max_wall_time_ms: Option<u64>,
}

impl ModuleManifest {
    /// Whether the module is permitted to call the given host function.
    #[must_use]
    pub fn allows(&self, f: HostFunction) -> bool {
        self.host_functions.contains(&f)
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
}
