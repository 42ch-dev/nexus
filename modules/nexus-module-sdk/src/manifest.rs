//! Manifest helper + sandbox defaults (AR-6).
//!
//! [`ModuleManifest`] is a field-for-field mirror of the host-side type in
//! `nexus-module-manifest` (extracted from `crates/nexus-wasm-host/src/manifest.rs`
//! L76–118). The SDK cannot depend on that crate (closed dependency list,
//! AR-1), so the mirror is hand-maintained; the AR-7 mirror-gap drift check
//! is what catches a wire addition the SDK has not yet mirrored.
//!
//! The sandbox default constants are the single public source so manifest
//! generators and validators agree with the host by construction (mirroring
//! `crates/nexus-wasm-host/src/sandbox.rs` L22–28; the manifest JSON carries
//! milliseconds, hence `_MS`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Default fuel budget: 10 million instructions. Mirrors
/// `crates/nexus-wasm-host/src/sandbox.rs` `DEFAULT_FUEL`.
pub const DEFAULT_FUEL: u64 = 10_000_000;

/// Default memory cap: 64 MiB. Mirrors `sandbox.rs` `DEFAULT_MEMORY_MIB`.
pub const DEFAULT_MEMORY_MIB: u32 = 64;

/// Default wall-time limit: 30 seconds (the manifest JSON carries
/// milliseconds). Mirrors `sandbox.rs` `DEFAULT_WALL_TIME`.
pub const DEFAULT_WALL_TIME_MS: u64 = 30_000;

/// Whitelisted host functions a module may import.
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

/// Module manifest (`manifest.json`) — full contract mirror (AR-6).
///
/// Required 7 (`module_id`, `name`, `version`, `nexus_abi_version`,
/// `required_key_block_types`, `compute_export`, `init_export`) + optional 8
/// (`description`, `author`, `host_functions`, `battle_report_kind`,
/// `max_fuel`, `max_memory_mib`, `max_wall_time_ms`, `wasm_sha256`) per ABI
/// §7.1/§7.2, plus the V1.62 `schemas` block (ABI §7.3).
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
    /// (64 lowercase hex chars). When present, the loader verifies
    /// content-based pairing and rejects a mismatched pair.
    #[serde(default)]
    pub wasm_sha256: Option<String>,
}

impl ModuleManifest {
    /// Validate the manifest against the V1 contract (AR-6).
    ///
    /// Checks: required-field presence (non-empty identity/export strings —
    /// `init_export` MAY be empty when the module has no init export, ABI
    /// §7.1), `host_functions ⊆ ["kb_read", "narrative_query"]` (structural —
    /// the [`HostFunction`] enum rejects unknown names at parse time), the
    /// DR-49 pin `nexus_abi_version == 1` (the SDK refuses V2 concepts),
    /// `compute_export` non-empty, `wasm_sha256` format (64 lowercase hex
    /// when present), and `schemas` fragments parse as JSON objects.
    ///
    /// # Errors
    ///
    /// Returns every failing check as a human-readable message.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Required-field presence: the typed struct guarantees the fields
        // exist; the meaningful check is that the identity/export strings
        // are non-empty. `init_export` is deliberately excluded — the ABI
        // allows an empty `init_export` when the module has no init export
        // (the real host and existing host fixtures use `init_export: ""`).
        for (field, value) in [
            ("module_id", self.module_id.as_str()),
            ("name", self.name.as_str()),
            ("version", self.version.as_str()),
            ("compute_export", self.compute_export.as_str()),
        ] {
            if value.is_empty() {
                errors.push(format!("{field} must be non-empty"));
            }
        }

        // `host_functions ⊆ ["kb_read", "narrative_query"]` is enforced
        // structurally by the `HostFunction` enum (serde rejects unknown
        // names at parse time), so no runtime check is needed here.

        // Module-id path-safety (P0 QC fix wave, qc2 W-1 mirror): the id
        // becomes a directory name under `dist/<id>/` (build) and
        // `~/.nexus42/modules/<id>/` (install), so a traversal id must fail
        // validation. Mirrors `nexus-home-layout::validate_run_id_safe`
        // (inlined — the SDK's closed dependency list, AR-1, forbids the
        // nexus-home-layout dep; the mirrored tests pin both sides).
        if self.module_id.contains('/')
            || self.module_id.contains('\\')
            || self.module_id.contains("..")
            || self.module_id.chars().any(char::is_control)
        {
            errors.push(format!(
                "module_id is not path-safe: {:?} — rejected for safety",
                self.module_id
            ));
        }

        // The DR-49 pin: the SDK refuses V2 concepts.
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
    use serde_json::json;

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
    fn validate_accepts_empty_init_export() {
        // ABI §7.1: `init_export` may be empty when the module has no init
        // export (the real host and existing host fixtures use
        // `init_export: ""`). `compute_export` must still be non-empty.
        let mut m = valid_manifest();
        m.init_export = String::new();
        assert!(m.validate().is_ok());
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
                json!(["not", "an", "object"]),
            )])),
            key_block_state: None,
            invocation: Some(json!("string fragment")),
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

    #[test]
    fn validate_rejects_path_traversal_module_id() {
        // qc2 W-1 mirror: the module id becomes a directory name in the
        // store — traversal values must fail validation, not just the CLI
        // guard (same corpus as nexus-module-manifest).
        for bad in ["../evil", "a/b", "a\\b", "a..b", "a\u{0}b"] {
            let mut m = valid_manifest();
            m.module_id = bad.to_string();
            let errs = m.validate().expect_err("unsafe id must be rejected");
            assert!(
                errs.iter()
                    .any(|e| e.contains("module_id is not path-safe")),
                "{bad:?} → {errs:?}"
            );
        }
    }

    /// Shared invalid-manifest corpus (qc1 S-1): this file is asserted by
    /// BOTH validators (this crate and nexus-module-manifest's validate()).
    /// The corpus lives in this crate's fixtures dir and is include_str!'d
    /// from the mirror crate too — a build break on deletion, so the two
    /// cannot silently diverge. Add new invalid cases to the shared file.
    const INVALID_MANIFESTS: &str = include_str!("../tests/fixtures/invalid-manifests.json");

    #[test]
    fn validate_rejects_shared_invalid_manifest_corpus() {
        let corpus: serde_json::Value =
            serde_json::from_str(INVALID_MANIFESTS).expect("corpus parses as JSON");
        let cases = corpus.as_array().expect("corpus is an array");
        assert!(!cases.is_empty(), "corpus must not be empty");
        for case in cases {
            let label = case["label"].as_str().unwrap_or("?");
            let manifest: ModuleManifest = serde_json::from_value(case["manifest"].clone())
                .expect("corpus manifest must deserialize");
            let errs = manifest
                .validate()
                .expect_err("corpus manifest must be invalid");
            let expect = case["expect"].as_str().unwrap_or_default();
            assert!(
                errs.iter().any(|e| e.contains(expect)),
                "{label}: expected an error containing {expect:?}, got {errs:?}"
            );
        }
    }

    #[test]
    fn manifest_round_trips_through_serde() {
        let m = valid_manifest();
        let json = serde_json::to_string(&m).unwrap();
        let back: ModuleManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn host_function_serde_names_are_snake_case() {
        assert_eq!(
            serde_json::to_string(&HostFunction::KbRead).unwrap(),
            "\"kb_read\""
        );
        assert_eq!(
            serde_json::to_string(&HostFunction::NarrativeQuery).unwrap(),
            "\"narrative_query\""
        );
        assert_eq!(
            serde_json::from_str::<HostFunction>("\"kb_read\"").unwrap(),
            HostFunction::KbRead
        );
    }

    #[test]
    fn sandbox_defaults_match_host() {
        // Mirrors crates/nexus-wasm-host/src/sandbox.rs L22–28.
        assert_eq!(DEFAULT_FUEL, 10_000_000);
        assert_eq!(DEFAULT_MEMORY_MIB, 64);
        assert_eq!(DEFAULT_WALL_TIME_MS, 30_000);
    }
}
