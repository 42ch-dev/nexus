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

/// Module manifest (`manifest.json`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
}
