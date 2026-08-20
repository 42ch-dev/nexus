//! Module manifest (`manifest.json`) types and validation core.
//!
//! Extracted from `crates/nexus-wasm-host/src/manifest.rs` (V1.170 P0, AR-8)
//! so the host, the `nexus42 compute` CLI, and module tooling share ONE
//! manifest contract. `nexus-wasm-host` re-exports these types unchanged.
//!
//! A compute module ships a `manifest.json` alongside its compiled `.wasm`.
//! The manifest declares the module's identity, its required input surface,
//! and optional sandbox overrides.
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

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

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
    /// SHA-256 of the compiled `.wasm` bytes this manifest pairs with
    /// (64 lowercase hex chars). Operators SHOULD set this: when present,
    /// the loader verifies content-based pairing — the loaded bytes must
    /// hash to this value — and rejects a mismatched pair before it can
    /// be compiled or cached (Greptile P1: an old manifest + new wasm
    /// always mismatches). When absent (legacy manifests), the loader
    /// falls back to the stat fence (size + mtime), which cannot detect
    /// a same-size swap landing outside its observation windows.
    /// `inject_wasm_sha256` maintains this field for staged/embedded
    /// modules, computing it from the compiled artifact at build time.
    #[serde(default)]
    pub wasm_sha256: Option<String>,
}

impl ModuleManifest {
    /// Whether the module is permitted to call the given host function.
    #[must_use]
    pub fn allows(&self, f: HostFunction) -> bool {
        self.host_functions.contains(&f)
    }

    /// Verify content-based pairing: when `wasm_sha256` is set, `wasm_bytes`
    /// must hash to the declared value (64 lowercase hex chars). A mismatch
    /// means the `.wasm` and `manifest.json` are not the pair the manifest
    /// declares — e.g. an operator swapped in a new `.wasm` without updating
    /// the manifest — and must be rejected before the pair is compiled or
    /// cached (Greptile P1: an old manifest + new wasm always mismatches).
    /// `None` (legacy manifest) skips verification; callers then rely on the
    /// stat fence (size + mtime), which cannot detect a same-size swap
    /// landing outside its observation windows.
    ///
    /// # Errors
    ///
    /// Returns the mismatch message; callers classify it as a host fault.
    pub fn verify_wasm_sha256(&self, wasm_bytes: &[u8]) -> Result<(), String> {
        let Some(expected) = &self.wasm_sha256 else {
            return Ok(());
        };
        let digest = Sha256::digest(wasm_bytes);
        let mut computed = String::with_capacity(64);
        for b in digest {
            let _ = write!(computed, "{b:02x}");
        }
        if computed == *expected {
            Ok(())
        } else {
            Err(format!(
                "wasm does not match manifest wasm_sha256 (manifest {expected}, computed {computed})"
            ))
        }
    }

    /// Validate the manifest against the V1 contract (AR-6 checks, CLI-facing
    /// copy).
    ///
    /// Checks: required-field presence (non-empty identity/export strings),
    /// `host_functions ⊆ ["kb_read", "narrative_query"]` (structural — the
    /// [`HostFunction`] enum rejects unknown names at parse time), the DR-49
    /// pin `nexus_abi_version == 1`, `compute_export` non-empty, `wasm_sha256`
    /// format (64 lowercase hex when present), and `schemas` fragments parse
    /// as JSON objects.
    ///
    /// This is the CLI/tooling-facing implementation (`nexus42 compute
    /// validate` path-deps this crate, AR-9). The SDK carries a hand-maintained
    /// mirror under its closed dependency list (AR-1); the tests below pin the
    /// same checks on both sides so the two cannot silently diverge.
    ///
    /// # Errors
    ///
    /// Returns every failing check as a human-readable message.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Required-field presence: the typed struct guarantees the fields
        // exist; the meaningful check is that the identity/export strings
        // are non-empty.
        for (field, value) in [
            ("module_id", self.module_id.as_str()),
            ("name", self.name.as_str()),
            ("version", self.version.as_str()),
            ("compute_export", self.compute_export.as_str()),
            ("init_export", self.init_export.as_str()),
        ] {
            if value.is_empty() {
                errors.push(format!("{field} must be non-empty"));
            }
        }

        // `host_functions ⊆ ["kb_read", "narrative_query"]` is enforced
        // structurally by the `HostFunction` enum (serde rejects unknown
        // names at parse time), so no runtime check is needed here.

        // The DR-49 pin: the ABI refuses V2 concepts.
        if self.nexus_abi_version != 1 {
            errors.push(format!(
                "nexus_abi_version must be 1 (ABI V1), got {}",
                self.nexus_abi_version
            ));
        }

        if let Some(hash) = &self.wasm_sha256 {
            let valid =
                hash.len() == 64 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
            if !valid {
                errors.push("wasm_sha256 must be 64 lowercase hex characters".to_string());
            }
        }

        if let Some(schemas) = &self.schemas {
            if let Some(attrs) = &schemas.key_block_attributes {
                for (k, v) in attrs {
                    if !v.is_object() {
                        errors.push(format!(
                            "schemas.key_block_attributes.{k} must be a JSON object"
                        ));
                    }
                }
            }
            if let Some(state) = &schemas.key_block_state {
                for (k, v) in state {
                    if !v.is_object() {
                        errors.push(format!("schemas.key_block_state.{k} must be a JSON object"));
                    }
                }
            }
            if let Some(inv) = &schemas.invocation {
                if !inv.is_object() {
                    errors.push("schemas.invocation must be a JSON object".to_string());
                }
            }
            if let Some(report) = &schemas.battle_report {
                if !report.is_object() {
                    errors.push("schemas.battle_report must be a JSON object".to_string());
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
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

    // ── validate() — mirrors `nexus-module-sdk`'s AR-6 validate tests ─────

    fn valid_manifest() -> ModuleManifest {
        ModuleManifest {
            module_id: "basic-combat".to_string(),
            name: "Basic Combat".to_string(),
            version: "1.0.0".to_string(),
            nexus_abi_version: 1,
            required_key_block_types: vec!["character".to_string()],
            compute_export: "compute".to_string(),
            init_export: "init".to_string(),
            description: None,
            author: None,
            host_functions: vec![],
            schemas: None,
            battle_report_kind: Some("combat".to_string()),
            max_fuel: None,
            max_memory_mib: None,
            max_wall_time_ms: None,
            wasm_sha256: None,
        }
    }

    #[test]
    fn validate_accepts_valid_manifest() {
        assert!(valid_manifest().validate().is_ok());
    }

    #[test]
    fn validate_accepts_wasm_sha256_lowercase_hex() {
        let mut m = valid_manifest();
        m.wasm_sha256 =
            Some("7ed89295ba49e06652eef8e3a085fa35abae5fcccc3de3700b2f03ca098bcd6f".to_string());
        assert!(m.validate().is_ok());
    }

    #[test]
    fn validate_rejects_wrong_abi_version() {
        let mut m = valid_manifest();
        m.nexus_abi_version = 2;
        let errs = m.validate().expect_err("ABI 2 must be rejected");
        assert!(
            errs.iter()
                .any(|e| e.contains("nexus_abi_version must be 1")),
            "{errs:?}"
        );
    }

    #[test]
    fn validate_rejects_bad_wasm_sha256_formats() {
        for bad in [
            "7ED89295BA49E06652EEF8E3A085FA35ABAE5FCCCC3DE3700B2F03CA098BCD6F", // uppercase
            "abc",                                                              // too short
            "7ed89295ba49e06652eef8e3a085fa35abae5fcccc3de3700b2f03ca098bcd6",  // 63 chars
            "7ed89295ba49e06652eef8e3a085fa35abae5fcccc3de3700b2f03ca098bcd6g", // non-hex
        ] {
            let mut m = valid_manifest();
            m.wasm_sha256 = Some(bad.to_string());
            let errs = m.validate().expect_err("bad hash must be rejected");
            assert!(
                errs.iter()
                    .any(|e| e.contains("wasm_sha256 must be 64 lowercase hex")),
                "hash {bad:?} → {errs:?}"
            );
        }
    }

    #[test]
    fn validate_rejects_empty_required_strings() {
        let mut m = valid_manifest();
        m.compute_export = String::new();
        m.module_id = String::new();
        let errs = m.validate().expect_err("empty strings must be rejected");
        assert!(
            errs.iter().any(|e| e == "module_id must be non-empty"),
            "{errs:?}"
        );
        assert!(
            errs.iter().any(|e| e == "compute_export must be non-empty"),
            "{errs:?}"
        );
    }

    #[test]
    fn validate_rejects_non_object_schema_fragments() {
        let mut m = valid_manifest();
        m.schemas = Some(ModuleSchemas {
            key_block_attributes: Some(HashMap::from([(
                "character".to_string(),
                serde_json::json!(["not", "an", "object"]),
            )])),
            key_block_state: None,
            invocation: Some(serde_json::json!("string fragment")),
            battle_report: None,
        });
        let errs = m
            .validate()
            .expect_err("non-object fragments must be rejected");
        assert!(
            errs.iter()
                .any(|e| e == "schemas.key_block_attributes.character must be a JSON object"),
            "{errs:?}"
        );
        assert!(
            errs.iter()
                .any(|e| e == "schemas.invocation must be a JSON object"),
            "{errs:?}"
        );
    }

    #[test]
    fn validate_reports_multiple_errors() {
        let mut m = valid_manifest();
        m.nexus_abi_version = 2;
        m.wasm_sha256 = Some("nope".to_string());
        let errs = m.validate().expect_err("multiple failures");
        assert_eq!(errs.len(), 2, "{errs:?}");
    }
}
