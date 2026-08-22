//! User-authored capability descriptor (DR-10, V1.172 P0).
//!
//! The descriptor is the on-disk contract a developer writes at
//! `~/.nexus42/capabilities/<name>/capability.json` (AR-35) to declare a new
//! capability: a dot-separated identity, declared input/output JSON Schemas
//! (discovery-only; runtime validation is the module manifest's job, AR-37),
//! optional sandbox overrides, and a wasm module reference.
//!
//! It is a **closed contract** (`deny_unknown_fields`): unknown fields are
//! authoring errors, not forward-compat hints (AR-34).
//!
//! The registered capability leaks the three `String` fields to `&'static str`
//! once at construction (T2/AR-44) — one bounded allocation per admitted user
//! capability per boot, same lifetime as builtin literal constants.

use crate::capability::{Capability, CapabilityError};
use nexus_contracts::generated::daemon_api::compute::compute_input::{
    ComputeInputWorldRef, ComputeInputWorldRefWorldId,
};
use nexus_wasm_host::{ComputeInput, ComputeOutput};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::num::NonZeroU64;
use thiserror::Error;

/// Validation errors for a [`UserCapabilityDescriptor`] (AR-34 vocabulary).
///
/// All variants are Display-message-only — no structured error payload is
/// needed this iteration. Field-level messages mirror
/// `nexus-module-manifest` `ModuleManifest::validate()`.
#[derive(Error, Debug, PartialEq, Eq)]
pub enum CapabilityDescriptorError {
    /// A required field is absent or empty.
    #[error("missing field: {0}")]
    MissingField(&'static str),
    /// `name` violates the AR-34 name contract.
    #[error("invalid name: {0}")]
    InvalidName(String),
    /// An input/output schema string is empty or not a JSON object.
    #[error("invalid schema: {0}")]
    InvalidSchema(String),
    /// A present sandbox override is not `> 0`.
    #[error("invalid sandbox: {0}")]
    InvalidSandbox(String),
    /// `wasm.moduleId` is not path-safe or `wasm.wasmSha256` is not 64
    /// lowercase hex characters.
    #[error("invalid wasm ref: {0}")]
    InvalidWasmRef(String),
}

/// Optional per-invocation sandbox overrides (AR-34, AR-38).
///
/// Absent fields mean "use host defaults"; values are clamped DOWN to the host
/// maxima at admission (`min(override, DEFAULT)` — the existing
/// `WasmEngine::resolve_sandbox` semantics). Presence does not imply a raise.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SandboxOverrides {
    /// Instruction-level fuel budget; `> 0` when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fuel: Option<u64>,
    /// Maximum linear memory in MiB; `> 0` when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_mib: Option<u32>,
    /// Maximum wall-clock time in milliseconds; `> 0` when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall_time_ms: Option<u64>,
}

/// Reference to the compute module backing a user capability (AR-34).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WasmRef {
    /// The compute module id (the `<module-id>.wasm` filename stem and the
    /// `manifest.json` `module_id`). Path-safe: it becomes a directory name.
    pub module_id: String,
    /// Expected SHA-256 of the module's `.wasm` bytes — exactly 64 lowercase
    /// hex characters (same format rule as `ModuleManifest::validate`).
    pub wasm_sha256: String,
}

/// A user-authored capability descriptor (AR-34).
///
/// Parsed from `~/.nexus42/capabilities/<name>/capability.json` by the
/// T2 scanner; validated by [`validate`](Self::validate) before admission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserCapabilityDescriptor {
    /// Dot-separated capability name, e.g. `"sync.pull"` — identity for
    /// registration and the `<name>/` directory (AR-35).
    pub name: String,
    /// JSON Schema (draft 2020-12) string describing valid capability inputs.
    /// Discovery-only; runtime validation is the module manifest's job.
    pub input_schema: String,
    /// JSON Schema string describing the capability's output envelope.
    pub output_schema: String,
    /// Optional sandbox overrides; absent → host defaults.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<SandboxOverrides>,
    /// The compute module id + expected wasm SHA-256.
    pub wasm: WasmRef,
}

impl UserCapabilityDescriptor {
    /// Validate the descriptor against the AR-34 contract.
    ///
    /// Returns the first failing check (deterministic field order: name →
    /// input schema → output schema → sandbox → wasm ref). Unknown top-level
    /// and nested fields are rejected at parse time (`deny_unknown_fields`).
    ///
    /// # Errors
    ///
    /// Returns [`CapabilityDescriptorError`] with the first violated rule.
    pub fn validate(&self) -> Result<(), CapabilityDescriptorError> {
        validate_name(&self.name)?;
        validate_schema_field("inputSchema", &self.input_schema)?;
        validate_schema_field("outputSchema", &self.output_schema)?;
        if let Some(sandbox) = &self.sandbox {
            validate_sandbox(sandbox)?;
        }
        validate_wasm_ref(&self.wasm)?;
        Ok(())
    }
}

/// A registered user capability (V1.172 P0, DR-10; AR-34/AR-44).
///
/// P0 is **discoverable, not runnable**: discovery (name + schemas) is fully
/// functional; `run()` returns [`CapabilityError::WorkerUnavailable`] until the
/// P1 executor wires the wasm module (`AR-37`). No admission gates at P0
/// (collision/hash/clamp are P1, AR-43).
///
/// **Lifetime**: the descriptor's three `String` fields are leaked once at
/// construction (`Box::leak`) — one bounded allocation per admitted user
/// capability per boot, same process-lifetime semantics as the builtins'
/// literal constants (AR-34/AR-44). Deliberate and documented; do not convert
/// the `Capability` trait to owned types.
pub struct UserCapability {
    name: &'static str,
    input_schema: &'static str,
    output_schema: &'static str,
}

impl UserCapability {
    /// Construct from a validated descriptor, leaking the three catalog
    /// strings once (AR-34/AR-44).
    #[must_use]
    pub fn new(descriptor: &UserCapabilityDescriptor) -> Self {
        Self {
            name: Box::leak(descriptor.name.clone().into_boxed_str()),
            input_schema: Box::leak(descriptor.input_schema.clone().into_boxed_str()),
            output_schema: Box::leak(descriptor.output_schema.clone().into_boxed_str()),
        }
    }

    /// AR-37 envelope mapping: capability input JSON → [`ComputeInput`].
    ///
    /// - `schema_version` = 1 (literal; same as `narrative_compute.rs`
    ///   L249-250).
    /// - `world_ref` = the input's optional `worldId` string as
    ///   `{"world_id": <id>}`, else the empty `WorldRef` default (the wire
    ///   `world_ref` has no required sub-fields).
    /// - `key_blocks` = the input's optional `keyBlocks` array of opaque JSON
    ///   objects, else `[]`.
    /// - `invocation` = the raw input object passthrough (module-declared
    ///   parameter surface; the module's own `manifest.schemas.invocation`
    ///   governs runtime validation — AR-37).
    ///
    /// # Errors
    ///
    /// Returns [`CapabilityError::InputInvalid`] when the input is not a JSON
    /// object, `worldId` violates the wire `WorldId` format (`^wld_…$`), or a
    /// `keyBlocks` element is not a JSON object. Input mismatch is fail-closed
    /// per AR-37 (the declared `inputSchema` is discovery-only, not a second
    /// runtime validator).
    ///
    /// # Panics
    ///
    /// Panics if `schema_version` literal 1 is not representable as
    /// `NonZeroU64` (this can never happen — 1 is always non-zero).
    // The `&self` receiver and owned `Value` are mandated by the locked
    // interface (T2's executor calls it on the capability instance with the
    // run() input); the mapping itself is stateless.
    #[allow(clippy::unused_self, clippy::needless_pass_by_value)]
    pub fn to_compute_input(&self, input: Value) -> Result<ComputeInput, CapabilityError> {
        // The raw input object is the invocation passthrough — a non-object
        // input has no parameter surface to forward.
        let invocation: serde_json::Map<String, Value> =
            input.as_object().cloned().ok_or_else(|| {
                CapabilityError::InputInvalid("capability input must be a JSON object".to_string())
            })?;

        let world_ref = match input.get("worldId").and_then(Value::as_str) {
            Some(world_id) => {
                let world_id_newtype = ComputeInputWorldRefWorldId::try_from(world_id)
                    .map_err(|e| CapabilityError::InputInvalid(format!("invalid worldId: {e}")))?;
                ComputeInputWorldRef {
                    world_id: Some(world_id_newtype),
                    ..Default::default()
                }
            }
            None => ComputeInputWorldRef::default(),
        };

        let key_blocks = match input.get("keyBlocks") {
            None => Vec::new(),
            Some(Value::Array(items)) => items
                .iter()
                .map(|item| {
                    item.as_object().cloned().ok_or_else(|| {
                        CapabilityError::InputInvalid(
                            "keyBlocks elements must be JSON objects".to_string(),
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            Some(_) => {
                return Err(CapabilityError::InputInvalid(
                    "keyBlocks must be a JSON array".to_string(),
                ));
            }
        };

        Ok(ComputeInput {
            schema_version: NonZeroU64::new(1).expect("schema_version literal 1 is non-zero"),
            world_ref,
            key_blocks,
            narrative_state: None,
            invocation,
        })
    }

    /// AR-37 output mapping: [`ComputeOutput`] → capability output JSON.
    ///
    /// Serializes the **4-part wire envelope verbatim** (`state_delta`,
    /// `timeline_events`, `new_key_blocks`, `battle_report`) — the typed
    /// `ComputeOutput` round-trips losslessly through `serde_json`.
    ///
    /// # Errors
    ///
    /// Returns [`CapabilityError::Internal`] if the typed output cannot be
    /// serialized (a host-side invariant violation — the struct is a plain
    /// data type).
    pub fn from_compute_output(output: ComputeOutput) -> Result<Value, CapabilityError> {
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("compute output serialization: {e}")))
    }
}

#[async_trait::async_trait]
impl Capability for UserCapability {
    fn name(&self) -> &'static str {
        self.name
    }

    fn input_schema(&self) -> &'static str {
        self.input_schema
    }

    fn output_schema(&self) -> &'static str {
        self.output_schema
    }

    async fn run(&self, _input: Value) -> Result<Value, CapabilityError> {
        // P0 placeholder: discoverable-not-runnable (compass AC-V172-1
        // invoke half is P1). AR-44 keeps the existing unit
        // `WorkerUnavailable` variant (no new CapabilityError variant — the
        // exhaustive matches in fork.rs / quality_loop.rs / tasks must keep
        // compiling); the named message is emitted at warn! below.
        tracing::warn!(
            capability = %self.name,
            "capability '{}': executor not yet wired (P1)", self.name
        );
        Err(CapabilityError::WorkerUnavailable)
    }
}

/// AR-34 rule 1: non-empty; dot-separated segments each matching
/// `^[a-z0-9_]+$`; no empty/leading/trailing segments; `len ≤ 128`.
///
/// The per-segment charset inherently rejects `/`, `\`, control chars and
/// `..`-style traversal — the name becomes a directory name (AR-35), same
/// path-safety intent as `ModuleManifest::validate` `module_id`.
fn validate_name(name: &str) -> Result<(), CapabilityDescriptorError> {
    if name.is_empty() {
        return Err(CapabilityDescriptorError::MissingField("name"));
    }
    if name.len() > 128 {
        return Err(CapabilityDescriptorError::InvalidName(format!(
            "name exceeds 128 chars (len {})",
            name.len()
        )));
    }
    if name.split('.').any(|seg| {
        seg.is_empty()
            || !seg
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    }) {
        return Err(CapabilityDescriptorError::InvalidName(format!(
            "name must be dot-separated segments of [a-z0-9_], got: {name:?}"
        )));
    }
    Ok(())
}

/// AR-34 rule 2: schema strings are non-empty and parse as a JSON **object**.
fn validate_schema_field(
    field: &'static str,
    value: &str,
) -> Result<(), CapabilityDescriptorError> {
    if value.is_empty() {
        return Err(CapabilityDescriptorError::MissingField(field));
    }
    let parsed: Value = serde_json::from_str(value).map_err(|e| {
        CapabilityDescriptorError::InvalidSchema(format!("{field} must be a JSON object: {e}"))
    })?;
    if !parsed.is_object() {
        return Err(CapabilityDescriptorError::InvalidSchema(format!(
            "{field} must be a JSON object"
        )));
    }
    Ok(())
}

/// AR-34 rule 3: every present sandbox override is `> 0`.
///
/// Overrides are clamped DOWN to host maxima at admission (AR-38); the
/// zero-guard here only rejects meaningless zero/absent-vs-zero values.
fn validate_sandbox(sandbox: &SandboxOverrides) -> Result<(), CapabilityDescriptorError> {
    if sandbox.fuel.is_some_and(|v| v == 0) {
        return Err(CapabilityDescriptorError::InvalidSandbox(
            "sandbox.fuel must be > 0 when present".to_string(),
        ));
    }
    if sandbox.memory_mib.is_some_and(|v| v == 0) {
        return Err(CapabilityDescriptorError::InvalidSandbox(
            "sandbox.memoryMib must be > 0 when present".to_string(),
        ));
    }
    if sandbox.wall_time_ms.is_some_and(|v| v == 0) {
        return Err(CapabilityDescriptorError::InvalidSandbox(
            "sandbox.wallTimeMs must be > 0 when present".to_string(),
        ));
    }
    Ok(())
}

/// AR-34 rules 4-5: `moduleId` is path-safe; `wasmSha256` is exactly 64
/// lowercase hex characters (same rule as `ModuleManifest::validate`).
fn validate_wasm_ref(wasm: &WasmRef) -> Result<(), CapabilityDescriptorError> {
    if wasm.module_id.is_empty() {
        return Err(CapabilityDescriptorError::MissingField("wasm.moduleId"));
    }
    if wasm.module_id.contains('/')
        || wasm.module_id.contains('\\')
        || wasm.module_id.contains("..")
        || wasm.module_id.chars().any(char::is_control)
    {
        return Err(CapabilityDescriptorError::InvalidWasmRef(format!(
            "wasm.moduleId is not path-safe: {:?}",
            wasm.module_id
        )));
    }
    if wasm.wasm_sha256.len() != 64
        || !wasm
            .wasm_sha256
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(CapabilityDescriptorError::InvalidWasmRef(format!(
            "wasm.wasmSha256 must be 64 lowercase hex characters, got {:?}",
            wasm.wasm_sha256
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_SHA256: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn minimal_json() -> String {
        format!(
            r#"{{
                "name": "sync.pull",
                "inputSchema": "{{\"type\":\"object\"}}",
                "outputSchema": "{{\"type\":\"object\"}}",
                "wasm": {{
                    "moduleId": "basic-combat",
                    "wasmSha256": "{VALID_SHA256}"
                }}
            }}"#
        )
    }

    fn parse(json: &str) -> Result<UserCapabilityDescriptor, serde_json::Error> {
        serde_json::from_str::<UserCapabilityDescriptor>(json)
    }

    /// AR-34 rejection set: every invalid case must fail, either at parse
    /// (serde) or in `validate()` (descriptor error).
    fn assert_rejected(json: &str) {
        if let Ok(descriptor) = parse(json) {
            assert!(
                descriptor.validate().is_err(),
                "expected validation failure for: {json}"
            );
        }
    }

    #[test]
    fn parses_minimal_descriptor() {
        let descriptor = parse(&minimal_json()).expect("minimal descriptor must parse");
        assert_eq!(descriptor.name, "sync.pull");
        assert_eq!(descriptor.input_schema, r#"{"type":"object"}"#);
        assert_eq!(descriptor.output_schema, r#"{"type":"object"}"#);
        assert_eq!(descriptor.sandbox, None);
        assert_eq!(descriptor.wasm.module_id, "basic-combat");
        assert_eq!(descriptor.wasm.wasm_sha256, VALID_SHA256);
        descriptor
            .validate()
            .expect("minimal descriptor must validate");
    }

    #[test]
    fn maps_camel_case_fields() {
        let json = format!(
            r#"{{
                "name": "sync.pull",
                "inputSchema": "{{}}",
                "outputSchema": "{{}}",
                "sandbox": {{ "fuel": 1000, "memoryMib": 32, "wallTimeMs": 5000 }},
                "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{VALID_SHA256}" }}
            }}"#
        );
        let descriptor = parse(&json).expect("camelCase descriptor must parse");
        let sandbox = descriptor.sandbox.as_ref().expect("sandbox present");
        assert_eq!(sandbox.fuel, Some(1_000));
        assert_eq!(sandbox.memory_mib, Some(32));
        assert_eq!(sandbox.wall_time_ms, Some(5_000));

        // Round-trip serialization emits the same camelCase field names.
        let round = serde_json::to_value(&descriptor).expect("serializable");
        assert_eq!(round["inputSchema"], "{}");
        assert_eq!(round["outputSchema"], "{}");
        assert_eq!(round["sandbox"]["fuel"], 1_000);
        assert_eq!(round["sandbox"]["memoryMib"], 32);
        assert_eq!(round["sandbox"]["wallTimeMs"], 5_000);
        assert_eq!(round["wasm"]["moduleId"], "basic-combat");
        assert_eq!(round["wasm"]["wasmSha256"], VALID_SHA256);
    }

    #[test]
    fn serializes_absent_optionals_as_absent() {
        let descriptor = parse(&minimal_json()).expect("minimal descriptor must parse");
        let round = serde_json::to_value(&descriptor).expect("serializable");
        assert!(
            round.get("sandbox").is_none(),
            "absent sandbox stays absent"
        );
    }

    #[test]
    fn reject_missing_name() {
        let json = format!(
            r#"{{ "inputSchema": "{{}}", "outputSchema": "{{}}",
                 "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{VALID_SHA256}" }} }}"#
        );
        assert_rejected(&json);
    }

    #[test]
    fn reject_missing_schemas() {
        let json = format!(
            r#"{{ "name": "sync.pull",
                 "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{VALID_SHA256}" }} }}"#
        );
        assert_rejected(&json);
    }

    #[test]
    fn reject_missing_output_schema() {
        let json = format!(
            r#"{{ "name": "sync.pull", "inputSchema": "{{}}",
                 "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{VALID_SHA256}" }} }}"#
        );
        assert_rejected(&json);
    }

    #[test]
    fn reject_empty_name() {
        let json = format!(
            r#"{{ "name": "", "inputSchema": "{{}}", "outputSchema": "{{}}",
                 "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{VALID_SHA256}" }} }}"#
        );
        let descriptor = parse(&json).expect("empty name still parses (string field)");
        assert_eq!(
            descriptor.validate(),
            Err(CapabilityDescriptorError::MissingField("name"))
        );
    }

    #[test]
    fn reject_names_not_dot_separated() {
        for name in [
            "bad/name",  // path separator
            "BadName",   // uppercase
            "bad name",  // whitespace
            "trailing.", // empty trailing segment
            ".leading",  // empty leading segment
            "a..b",      // empty middle segment
        ] {
            let json = format!(
                r#"{{ "name": "{name}", "inputSchema": "{{}}", "outputSchema": "{{}}",
                     "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{VALID_SHA256}" }} }}"#
            );
            assert_rejected(&json);
        }
    }

    #[test]
    fn reject_name_over_128_chars() {
        let long_name = "a".repeat(129);
        let json = format!(
            r#"{{ "name": "{long_name}", "inputSchema": "{{}}", "outputSchema": "{{}}",
                 "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{VALID_SHA256}" }} }}"#
        );
        let descriptor = parse(&json).expect("long name parses (length is a validate check)");
        assert!(matches!(
            descriptor.validate(),
            Err(CapabilityDescriptorError::InvalidName(_))
        ));
    }

    #[test]
    fn accept_128_char_name_boundary() {
        let long_name = "a".repeat(128);
        let json = format!(
            r#"{{ "name": "{long_name}", "inputSchema": "{{}}", "outputSchema": "{{}}",
                 "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{VALID_SHA256}" }} }}"#
        );
        parse(&json)
            .expect("name at the 128-char boundary must parse")
            .validate()
            .expect("name at the 128-char boundary must validate");
    }

    #[test]
    fn rejects_non_object_schema_strings() {
        for schema in ["[]", "\"string\"", "42", "not-json"] {
            let json = format!(
                r#"{{ "name": "sync.pull", "inputSchema": "{schema}", "outputSchema": "{{}}",
                     "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{VALID_SHA256}" }} }}"#
            );
            assert_rejected(&json);
        }
    }

    #[test]
    fn rejects_empty_schema_strings() {
        let json = format!(
            r#"{{ "name": "sync.pull", "inputSchema": "", "outputSchema": "{{}}",
                 "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{VALID_SHA256}" }} }}"#
        );
        let descriptor = parse(&json).expect("empty schema string parses (validate check)");
        assert_eq!(
            descriptor.validate(),
            Err(CapabilityDescriptorError::MissingField("inputSchema"))
        );
    }

    #[test]
    fn rejects_malformed_wasm_sha256() {
        let bad = [
            "ABC",                                                               // too short
            "G0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", // invalid hex char
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdeF",  // uppercase hex
            "",                                                                  // empty
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcde",   // 63 chars
        ];
        for sha in bad {
            let json = format!(
                r#"{{ "name": "sync.pull", "inputSchema": "{{}}", "outputSchema": "{{}}",
                     "wasm": {{ "wasmSha256": "{sha}" }} }}"#
            );
            assert_rejected(&json);
        }
    }

    #[test]
    fn rejects_path_unsafe_module_ids() {
        for module_id in ["../evil", "a/b", "a\\b", "a\u{0}b"] {
            let json = format!(
                r#"{{ "name": "sync.pull", "inputSchema": "{{}}", "outputSchema": "{{}}",
                     "wasm": {{ "moduleId": "{module_id}", "wasmSha256": "{VALID_SHA256}" }} }}"#
            );
            assert_rejected(&json);
        }
    }

    #[test]
    fn rejects_empty_module_id() {
        let json = format!(
            r#"{{ "name": "sync.pull", "inputSchema": "{{}}", "outputSchema": "{{}}",
                 "wasm": {{ "moduleId": "", "wasmSha256": "{VALID_SHA256}" }} }}"#
        );
        let descriptor = parse(&json).expect("empty moduleId must parse (validate check)");
        assert_eq!(
            descriptor.validate(),
            Err(CapabilityDescriptorError::MissingField("wasm.moduleId"))
        );
    }

    #[test]
    fn rejects_unknown_top_level_fields() {
        let json = format!(
            r#"{{ "name": "sync.pull", "inputSchema": "{{}}", "outputSchema": "{{}}",
                 "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{VALID_SHA256}" }}, "extra": true }}"#
        );
        assert!(
            parse(&json).is_err(),
            "unknown top-level field must fail parse"
        );
    }

    #[test]
    fn rejects_unknown_nested_fields() {
        let json = format!(
            r#"{{ "name": "sync.pull", "inputSchema": "{{}}", "outputSchema": "{{}}",
                 "sandbox": {{ "fuel": 1, "bogus": true }},
                 "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{VALID_SHA256}" }} }}"#
        );
        assert!(
            parse(&json).is_err(),
            "unknown sandbox field must fail parse"
        );

        let json = format!(
            r#"{{ "name": "sync.pull", "inputSchema": "{{}}", "outputSchema": "{{}}",
                 "wasm": {{ "wasmSha256": "{VALID_SHA256}", "extra": 1 }} }}"#
        );
        assert!(parse(&json).is_err(), "unknown wasm field must fail parse");
    }

    #[test]
    fn accepts_optional_sandbox_overrides() {
        let json = format!(
            r#"{{ "name": "sync.pull", "inputSchema": "{{}}", "outputSchema": "{{}}",
                 "sandbox": {{ "fuel": 1000000, "memoryMib": 32, "wallTimeMs": 15000 }},
                 "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{VALID_SHA256}" }} }}"#
        );
        let descriptor = parse(&json).expect("sandbox overrides must parse");
        descriptor
            .validate()
            .expect("positive sandbox overrides must validate");
    }

    #[test]
    fn accepts_empty_sandbox_object() {
        let json = format!(
            r#"{{ "name": "sync.pull", "inputSchema": "{{}}", "outputSchema": "{{}}",
                 "sandbox": {{}},
                 "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{VALID_SHA256}" }} }}"#
        );
        let descriptor = parse(&json).expect("empty sandbox object must parse");
        descriptor
            .validate()
            .expect("empty sandbox object must validate (no overrides)");
    }

    #[test]
    fn rejects_zero_sandbox_overrides() {
        for (field, value) in [("fuel", 0_u64), ("memoryMib", 0_u64), ("wallTimeMs", 0_u64)] {
            let json = format!(
                r#"{{ "name": "sync.pull", "inputSchema": "{{}}", "outputSchema": "{{}}",
                     "sandbox": {{ "{field}": {value} }},
                     "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{VALID_SHA256}" }} }}"#
            );
            assert_rejected(&json);
        }
    }

    // ── UserCapability impl (T2 / AR-34 / AR-44) ────────────────────────

    /// Discovery returns the leaked `&'static str` catalog fields.
    #[test]
    fn user_capability_discovery_returns_declared_fields() {
        let descriptor = parse(&minimal_json()).expect("minimal descriptor must parse");
        let cap = UserCapability::new(&descriptor);
        assert_eq!(cap.name(), "sync.pull");
        assert_eq!(cap.input_schema(), r#"{"type":"object"}"#);
        assert_eq!(cap.output_schema(), r#"{"type":"object"}"#);
    }

    /// P0 stub `run()` returns the named `WorkerUnavailable` — discoverable,
    /// not runnable (AR-44: existing unit variant, no new `CapabilityError`).
    #[tokio::test]
    async fn stub_run_returns_worker_unavailable() {
        let descriptor = parse(&minimal_json()).expect("minimal descriptor must parse");
        let cap = UserCapability::new(&descriptor);
        let err = cap.run(serde_json::json!({})).await.unwrap_err();
        assert!(
            matches!(err, CapabilityError::WorkerUnavailable),
            "expected WorkerUnavailable, got {err:?}"
        );
    }

    // ── AR-37 envelope mapping (P1 T1) ───────────────────────────────────

    fn cap() -> UserCapability {
        UserCapability::new(&parse(&minimal_json()).expect("minimal descriptor must parse"))
    }

    /// Round-trip: a full input maps `worldId` → `world_ref.world_id`,
    /// `keyBlocks` → `key_blocks`, and the whole object passes through as the
    /// raw `invocation` (module-declared parameter surface, AR-37).
    #[test]
    fn to_compute_input_round_trips_envelope() {
        let input = serde_json::json!({
            "worldId": "wld_w1",
            "keyBlocks": [
                {"key_block_id": "kb_a", "body": {"state": {"character": {"current_hp": 80}}}},
                {"key_block_id": "kb_b", "body": {"state": {"character": {"current_hp": 120}}}}
            ],
            "seed": 42,
        });

        let compute_input = cap()
            .to_compute_input(input.clone())
            .expect("valid input maps");

        // schema_version literal 1 (AR-37, narrative_compute.rs L249-250).
        assert_eq!(compute_input.schema_version.get(), 1);
        assert_eq!(
            compute_input
                .world_ref
                .world_id
                .as_deref()
                .map(String::as_str),
            Some("wld_w1"),
            "worldId maps to world_ref.world_id"
        );
        // key_blocks items are opaque JSON objects, preserved in order.
        let expected_blocks = input["keyBlocks"]
            .as_array()
            .expect("keyBlocks is an array");
        assert_eq!(compute_input.key_blocks.len(), expected_blocks.len());
        for (got, want) in compute_input.key_blocks.iter().zip(expected_blocks) {
            assert_eq!(serde_json::Value::Object(got.clone()), *want);
        }
        // invocation is the raw input object passthrough — verbatim.
        assert_eq!(
            serde_json::Value::Object(compute_input.invocation),
            input,
            "invocation must carry the full raw input object"
        );
    }

    /// Empty input → `world_ref: {}`, `key_blocks: []`, `invocation: {}`.
    #[test]
    fn to_compute_input_empty_input_uses_envelope_defaults() {
        let compute_input = cap()
            .to_compute_input(serde_json::json!({}))
            .expect("empty object input maps");
        assert!(compute_input.world_ref.world_id.is_none());
        assert!(compute_input.world_ref.branch_id.is_none());
        assert!(compute_input.world_ref.timeline_head_event_id.is_none());
        assert!(compute_input.key_blocks.is_empty());
        assert!(compute_input.invocation.is_empty());
    }

    /// Missing optional fields default: no `worldId`/`keyBlocks` in a
    /// non-empty invocation leaves the envelope empty and passes the body
    /// through untouched.
    #[test]
    fn to_compute_input_omits_absent_optional_fields() {
        let compute_input = cap()
            .to_compute_input(serde_json::json!({"seed": 7}))
            .expect("input with only invocation fields maps");
        assert!(compute_input.world_ref.world_id.is_none());
        assert!(compute_input.key_blocks.is_empty());
        assert_eq!(
            compute_input.invocation.get("seed"),
            Some(&serde_json::json!(7)),
            "invocation body preserved without envelope fields"
        );
    }

    /// Fail-closed (AR-37): a malformed `worldId` (wire requires `^wld_…$`)
    /// is an `InputInvalid`, never silently dropped.
    #[test]
    fn to_compute_input_rejects_malformed_world_id() {
        let err = cap()
            .to_compute_input(serde_json::json!({"worldId": "nope"}))
            .expect_err("malformed worldId must be rejected");
        assert!(
            matches!(err, CapabilityError::InputInvalid(_)),
            "expected InputInvalid, got {err:?}"
        );
    }

    /// `keyBlocks` must be an array of JSON objects — a scalar element is a
    /// fail-closed `InputInvalid`.
    #[test]
    fn to_compute_input_rejects_non_object_key_block() {
        let err = cap()
            .to_compute_input(serde_json::json!({"keyBlocks": ["not-an-object"]}))
            .expect_err("non-object keyBlock must be rejected");
        assert!(
            matches!(err, CapabilityError::InputInvalid(_)),
            "expected InputInvalid, got {err:?}"
        );
    }

    /// A non-object capability input has no invocation surface — `InputInvalid`.
    #[test]
    fn to_compute_input_rejects_non_object_input() {
        let err = cap()
            .to_compute_input(serde_json::json!("naked string"))
            .expect_err("non-object input must be rejected");
        assert!(
            matches!(err, CapabilityError::InputInvalid(_)),
            "expected InputInvalid, got {err:?}"
        );
    }

    /// `from_compute_output` maps the 4-part wire envelope verbatim
    /// (`state_delta`, `timeline_events`, `new_key_blocks`, `battle_report`).
    #[test]
    fn from_compute_output_serializes_four_part_envelope_verbatim() {
        let output_json = serde_json::json!({
            "schema_version": 1,
            "state_delta": [{
                "op": "sub",
                "path": "character.current_hp",
                "target_key_block_id": "kb_def",
                "value": 15
            }],
            "timeline_events": [{
                "schema_version": 1,
                "timeline_event_id": "evt_1",
                "world_id": "wld_w1",
                "branch_id": "root",
                "event_type": "state_update",
                "status": "canon",
                "sequence_no": 1,
                "created_at": "2026-01-01T00:00:00Z",
                "title": "Guardian takes 15 damage",
                "summary": "Guardian takes 15 damage (kb_def)",
                "affected_key_block_ids": ["kb_atk", "kb_def"]
            }],
            "new_key_blocks": [],
            "battle_report": {"kind": "combat"}
        });
        // The host deserializes module output into the typed struct; our
        // mapping then re-serializes it — the envelope must round-trip.
        let output: nexus_wasm_host::ComputeOutput = serde_json::from_value(output_json.clone())
            .expect("fixture must deserialize into ComputeOutput");

        let mapped = UserCapability::from_compute_output(output).expect("output maps");
        assert_eq!(
            mapped, output_json,
            "from_compute_output must reproduce the 4-part envelope verbatim"
        );
    }
}
