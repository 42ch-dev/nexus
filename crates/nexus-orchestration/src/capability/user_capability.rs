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

use crate::capability::{Capability, CapabilityError, CapabilityOrigin};
use nexus_contracts::generated::daemon_api::compute::compute_input::{
    ComputeInputWorldRef, ComputeInputWorldRefWorldId,
};
use nexus_wasm_host::{ComputeError, ComputeInput, ComputeOutput, ModuleCache, WasmEngine};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::num::NonZeroU64;
use std::path::PathBuf;
use std::sync::Arc;
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

/// Tighten sandbox overrides to the host maxima via `min(override, default)`
/// — the same semantics as `WasmEngine::resolve_sandbox` (compute.rs L71-80).
///
/// The maxima are read from [`nexus_wasm_host::SandboxConfig::default()`]
/// (AR-38: read, never duplicate). Absent fields stay absent (host defaults).
/// `pub(crate)` because both the admission gate 4 (`admission.rs`) and
/// `UserCapability::new` (F1 — re-clamp so a directly-constructed capability
/// carries clamped bounds) apply it.
#[must_use]
pub(crate) fn clamp_sandbox(overrides: &SandboxOverrides) -> SandboxOverrides {
    let defaults = nexus_wasm_host::SandboxConfig::default();
    let max_wall_time_ms = u64::try_from(defaults.wall_time.as_millis()).unwrap_or(u64::MAX);
    SandboxOverrides {
        fuel: overrides.fuel.map(|fuel| fuel.min(defaults.fuel)),
        memory_mib: overrides
            .memory_mib
            .map(|memory_mib| memory_mib.min(defaults.memory_mib())),
        wall_time_ms: overrides
            .wall_time_ms
            .map(|wall_time_ms| wall_time_ms.min(max_wall_time_ms)),
    }
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

/// A registered user capability (V1.172 P1, DR-10; AR-34/AR-37/AR-44).
///
/// Discovery (name + schemas) is fully functional; `run()` executes the
/// referenced wasm module through the existing compute sandbox (AR-37): the
/// capability dir's `manifest.json` + `<module-id>.wasm` are read lazily at
/// first `run()`, compiled once through the shared [`ModuleCache`]
/// (hash-keyed compile-once), then invoked via [`WasmEngine::compute`].
///
/// **Lifetime**: the descriptor's three `String` fields are leaked once at
/// construction (`Box::leak`) — one bounded allocation per admitted user
/// capability per boot, same process-lifetime semantics as the builtins'
/// literal constants (AR-34/AR-44). Deliberate and documented; do not convert
/// the `Capability` trait to owned types.
///
/// The executor handle quartet (`dir`, `module_id`/`wasm_sha256`, `engine`,
/// `module_cache`) is `Option`al where the handle needs a runtime so the
/// **engine-absent boot arm** (AR-44) can still register the capability
/// discoverable; `run()` then returns the existing
/// [`CapabilityError::WorkerUnavailable`] variant (no new variant — exhaustive
/// matches like `fork.rs` keep compiling). The admitted-with-engine path never
/// returns it (PL-10 closed).
///
/// `Clone` (V1.176 P1, AR-92 #4): the hot-reload watcher keeps a last-admitted
/// mirror of concrete `UserCapability` entries so the merge rule can carry the
/// last good admission across rebuilds. Cloning is cheap — the three catalog
/// strings are `&'static str` (shared, no re-leak), the handles are `Arc`s,
/// and the remaining fields are plain data.
#[derive(Clone)]
pub struct UserCapability {
    name: &'static str,
    input_schema: &'static str,
    output_schema: &'static str,
    /// The capability's own directory (`~/.nexus42/capabilities/<name>/`,
    /// AR-35) — source of `manifest.json` + `<module-id>.wasm`.
    dir: PathBuf,
    /// The descriptor's `wasm.moduleId` — the `<module-id>.wasm` filename
    /// stem. Owned (no trait-owned method needs it `&'static`), unlike the
    /// three leaked catalog strings.
    module_id: String,
    /// The descriptor's `wasm.wasmSha256` — the admitted module's expected
    /// content hash. Every lazy load re-verifies the on-disk bytes against it
    /// (F2, AR-39 single hash path) so a capability dir edited after
    /// admission can never execute unverified bytes.
    wasm_sha256: String,
    /// The **clamped** sandbox overrides from admission (gate 4, AR-38),
    /// carried so `run()` can apply them to the invocation (F1 — the
    /// descriptor's bounds must reach the sandbox, not just be validated).
    /// `None` → host defaults / manifest values. Clamping is re-applied at
    /// `new()` so a directly-constructed (un-admitted) capability stays
    /// fail-closed.
    sandbox: Option<SandboxOverrides>,
    /// The daemon-wide wasm engine (absent on the engine-less boot arm).
    engine: Option<Arc<WasmEngine>>,
    /// The daemon-wide compilation cache (absent on the engine-less boot arm).
    module_cache: Option<Arc<ModuleCache>>,
}

impl UserCapability {
    /// Construct from a validated descriptor, leaking the three catalog
    /// strings once (AR-34/AR-44), and carrying the capability dir + the
    /// descriptor's wasm hash + sandbox overrides + the shared engine/cache
    /// (AR-37/AR-38/AR-39).
    ///
    /// `dir` is the capability's own directory (`<scan_root>/<name>/`); the
    /// executor reads `manifest.json` + `<module-id>.wasm` from it at first
    /// `run()`. The sandbox overrides are **re-clamped here** (F1/AR-38) so a
    /// capability constructed without going through `admit()` (unit tests,
    /// future direct registration) can never carry un-clamped bounds.
    /// `engine`/`module_cache` are `None` on the engine-less boot arm
    /// (AR-44): the capability stays discoverable and `run()` returns
    /// `WorkerUnavailable`.
    #[must_use]
    pub fn new(
        descriptor: &UserCapabilityDescriptor,
        dir: PathBuf,
        engine: Option<Arc<WasmEngine>>,
        module_cache: Option<Arc<ModuleCache>>,
    ) -> Self {
        Self {
            name: Box::leak(descriptor.name.clone().into_boxed_str()),
            input_schema: Box::leak(descriptor.input_schema.clone().into_boxed_str()),
            output_schema: Box::leak(descriptor.output_schema.clone().into_boxed_str()),
            dir,
            module_id: descriptor.wasm.module_id.clone(),
            wasm_sha256: descriptor.wasm.wasm_sha256.clone(),
            sandbox: descriptor.sandbox.as_ref().map(clamp_sandbox),
            engine,
            module_cache,
        }
    }

    /// The admitted module's expected content hash (AR-39 single hash path).
    ///
    /// Test-only accessor: the hot-reload boot-equivalence test (AR-95 #1)
    /// compares name + `wasm_sha256` between the boot-constructor and
    /// hot-rebuild user-cap sets. Runtime enforcement is the executor's
    /// lazy-load re-verification (F2) — no production caller needs the value.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn wasm_sha256(&self) -> &str {
        &self.wasm_sha256
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

    fn origin(&self) -> CapabilityOrigin {
        CapabilityOrigin::User
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        // Engine-absent boot arm (AR-44): still discoverable, not runnable —
        // the ONLY path that returns the existing WorkerUnavailable variant
        // (no new CapabilityError; exhaustive matches keep compiling).
        let (Some(engine), Some(module_cache)) = (&self.engine, &self.module_cache) else {
            tracing::warn!(
                capability = %self.name,
                "capability '{}': executor not wired; daemon booted without WASM engine",
                self.name
            );
            return Err(CapabilityError::WorkerUnavailable);
        };

        // AR-37 envelope mapping (T1) — fail-closed on malformed input.
        let compute_input = self.to_compute_input(input)?;

        // Module resolution: read the capability's own manifest + wasm from
        // its dir (self-contained per-AR-35), compile once through the shared
        // hash-keyed cache (L211-224), then invoke. A module missing at
        // run()-time (dir edited after admission) is an InputInvalid.
        let manifest_path = self.dir.join("manifest.json");
        let manifest_json = std::fs::read_to_string(&manifest_path).map_err(|e| {
            CapabilityError::InputInvalid(format!(
                "module '{}' not loaded in capability '{}': read {}: {e}",
                self.module_id,
                self.name,
                manifest_path.display()
            ))
        })?;
        let wasm_path = self.dir.join(format!("{}.wasm", self.module_id));
        let wasm_bytes = std::fs::read(&wasm_path).map_err(|e| {
            CapabilityError::InputInvalid(format!(
                "module '{}' not loaded in capability '{}': read {}: {e}",
                self.module_id,
                self.name,
                wasm_path.display()
            ))
        })?;

        // F2 / AR-39 single hash path: every lazy load re-verifies the
        // on-disk module bytes before `get_or_compile`. Admission (gate 3)
        // guaranteed manifest `wasm_sha256` == descriptor `wasmSha256` == the
        // bytes' digest; this closes the TOCTOU gap where a capability dir is
        // edited AFTER admission — the cache is keyed by bytes-hash and would
        // otherwise compile unverified bytes. `verify_wasm_sha256` is the one
        // content-hash implementation (`nexus-module-manifest`); a mismatch —
        // or a manifest whose declared hash no longer equals the descriptor's
        // admitted hash — fails closed as `InputInvalid`.
        let manifest: nexus_wasm_host::ModuleManifest = serde_json::from_str(&manifest_json)
            .map_err(|e| {
                CapabilityError::InputInvalid(format!(
                    "module '{}' not loaded in capability '{}': parse {}: {e}",
                    self.module_id,
                    self.name,
                    manifest_path.display()
                ))
            })?;
        manifest.verify_wasm_sha256(&wasm_bytes).map_err(|e| {
            CapabilityError::InputInvalid(format!(
                "module '{}' hash changed for capability '{}': {e}",
                self.module_id, self.name
            ))
        })?;
        if manifest.wasm_sha256.as_deref() != Some(self.wasm_sha256.as_str()) {
            return Err(CapabilityError::InputInvalid(format!(
                "module '{}' hash changed for capability '{}'",
                self.module_id, self.name
            )));
        }

        let cached = module_cache
            .get_or_compile(engine, &self.module_id, &wasm_bytes, &manifest_json)
            .map_err(|e| map_compute_error(&e))?;
        let module = cached.module.clone();
        let mut manifest = cached.manifest.clone();

        // F1/AR-38: apply the clamped descriptor sandbox overrides onto the
        // manifest before `compute` — the invocation sandbox is resolved as
        // `min(manifest_override, host_default)` in `WasmEngine::resolve_sandbox`
        // (compute.rs L71-80), so folding each present descriptor bound in
        // via `min(existing, descriptor)` tightens the effective sandbox to
        // the capability's ceiling. Absent descriptor fields leave the
        // manifest/host value untouched. No `sandbox.rs` change (AR-38).
        if let Some(sandbox) = &self.sandbox {
            if let Some(fuel) = sandbox.fuel {
                manifest.max_fuel = Some(manifest.max_fuel.map_or(fuel, |m| m.min(fuel)));
            }
            if let Some(memory_mib) = sandbox.memory_mib {
                manifest.max_memory_mib = Some(
                    manifest
                        .max_memory_mib
                        .map_or(memory_mib, |m| m.min(memory_mib)),
                );
            }
            if let Some(wall_time_ms) = sandbox.wall_time_ms {
                manifest.max_wall_time_ms = Some(
                    manifest
                        .max_wall_time_ms
                        .map_or(wall_time_ms, |m| m.min(wall_time_ms)),
                );
            }
        }

        // Invoke the sandboxed module and map the 4-part output envelope.
        let output = engine
            .compute(&module, &manifest, &compute_input)
            .map_err(|e| map_compute_error(&e))?;
        Self::from_compute_output(output)
    }
}

/// Map a [`ComputeError`] to a [`CapabilityError`] per the AR-37 table
/// (distinct variants; fail-closed):
///
/// | `ComputeError` | `CapabilityError` |
/// |---|---|
/// | `OutOfFuel` / `WallTimeExceeded` / `MemoryCapExceeded` / `Trap` | `Forbidden("sandbox breach: <detail>")` |
/// | `InputValidationFailed` / `ManifestValidationFailed` / `InvalidOutput` | `InputInvalid(...)` |
/// | `MissingExport` / `ModuleComputeFailed` / `InvalidModule` / `OutputSchemaMismatch` | `PermanentExternal(...)` |
/// | `OutputBufferTooSmall` | `TransientExternal(...)` (retryable) |
/// | `CacheWarmup` / `Wasmtime` / `MemoryAccess` / `Io` / `Json` | `Internal(...)` |
///
/// S-1a: `OutputSchemaMismatch` sits in the `PermanentExternal` bucket — the
/// module emitted a malformed 4-part envelope (a module fault, not a caller
/// input problem); retrying with the same input cannot fix it.
fn map_compute_error(e: &ComputeError) -> CapabilityError {
    match e {
        ComputeError::OutOfFuel
        | ComputeError::WallTimeExceeded
        | ComputeError::MemoryCapExceeded
        | ComputeError::Trap(_) => CapabilityError::Forbidden(format!("sandbox breach: {e}")),
        ComputeError::InputValidationFailed(_)
        | ComputeError::ManifestValidationFailed { .. }
        | ComputeError::InvalidOutput(_) => {
            CapabilityError::InputInvalid(format!("module rejected input/output: {e}"))
        }
        ComputeError::MissingExport(_)
        | ComputeError::ModuleComputeFailed(_)
        | ComputeError::InvalidModule(_)
        | ComputeError::OutputSchemaMismatch(_) => {
            CapabilityError::PermanentExternal(format!("module fault: {e}"))
        }
        ComputeError::OutputBufferTooSmall(_) => {
            CapabilityError::TransientExternal(format!("module output buffer too small: {e}"))
        }
        ComputeError::CacheWarmup(_)
        | ComputeError::Wasmtime(_)
        | ComputeError::MemoryAccess(_)
        | ComputeError::Io(_)
        | ComputeError::Json(_) => CapabilityError::Internal(format!("compute host error: {e}")),
    }
}

/// AR-34 rule 1: non-empty; dot-separated segments each matching
/// `^[a-z0-9_]+$`; no empty/leading/trailing segments; `len ≤ 128`.
///
/// The per-segment charset inherently rejects `/`, `\`, control chars and
/// `..`-style traversal — the name becomes a directory name (AR-35), same
/// path-safety intent as `ModuleManifest::validate` `module_id`.
///
/// The first segment must NOT start with `_`: `install` would write it under
/// `~/.nexus42/capabilities/_<name>/`, which the boot scanner skips
/// (`_`-prefixed dirs — preset-scanner precedent, `scan.rs`) — a silent dead
/// install. Underscores INSIDE segments stay valid (`my_cap.pull`).
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
    if name
        .split('.')
        .next()
        .is_some_and(|first| first.starts_with('_'))
    {
        return Err(CapabilityDescriptorError::InvalidName(format!(
            "name first segment must not start with '_' (the boot scanner skips \
             '_'-prefixed dirs, so such a name would install dead; underscores \
             inside segments are fine, e.g. 'my_cap.pull'), got: {name:?}"
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
    fn reject_underscore_leading_first_segment() {
        // The boot scanner skips `_`-prefixed dirs (preset-scanner precedent),
        // so a `_`-leading first segment would install dead — rejected at the
        // authoring boundary (PR #227 Bugbot fix).
        for name in ["_my.cap", "_private.pull", "_", "_foo"] {
            let json = format!(
                r#"{{ "name": "{name}", "inputSchema": "{{}}", "outputSchema": "{{}}",
                     "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{VALID_SHA256}" }} }}"#
            );
            assert_rejected(&json);
        }
    }

    #[test]
    fn accept_underscores_inside_segments() {
        // Underscores INSIDE segments remain valid — only a `_`-leading FIRST
        // segment is rejected (`my_cap.pull` installs to `my_cap.pull/`, which
        // the scanner reads).
        for name in ["my_cap.pull", "normal.cap", "foo._bar", "a.b_c"] {
            let json = format!(
                r#"{{ "name": "{name}", "inputSchema": "{{}}", "outputSchema": "{{}}",
                     "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{VALID_SHA256}" }} }}"#
            );
            parse(&json)
                .unwrap_or_else(|e| panic!("{name} must parse: {e}"))
                .validate()
                .unwrap_or_else(|e| panic!("{name} must validate: {e}"));
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
        let cap = UserCapability::new(&descriptor, PathBuf::new(), None, None);
        assert_eq!(cap.name(), "sync.pull");
        assert_eq!(cap.input_schema(), r#"{"type":"object"}"#);
        assert_eq!(cap.output_schema(), r#"{"type":"object"}"#);
    }

    /// AR-44 engine-absent fallback: a capability constructed without a wasm
    /// engine/cache (engine-less boot arm) stays discoverable; `run()` returns
    /// the existing `WorkerUnavailable` variant (no new `CapabilityError`).
    /// This is the ONLY path that returns it — the admitted-with-engine path
    /// never does (PL-10 closed).
    #[tokio::test]
    async fn engine_absent_run_returns_worker_unavailable() {
        let descriptor = parse(&minimal_json()).expect("minimal descriptor must parse");
        let cap = UserCapability::new(&descriptor, PathBuf::new(), None, None);
        let err = cap.run(serde_json::json!({})).await.unwrap_err();
        assert!(
            matches!(err, CapabilityError::WorkerUnavailable),
            "expected WorkerUnavailable, got {err:?}"
        );
    }

    // ── AR-37 envelope mapping (P1 T1) ───────────────────────────────────

    /// A capability with no dir/engine/cache (envelope-mapping tests only —
    /// mapping is stateless, T2's executor supplies the handles).
    fn cap() -> UserCapability {
        UserCapability::new(
            &parse(&minimal_json()).expect("minimal descriptor must parse"),
            PathBuf::new(),
            None,
            None,
        )
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

    // ── Real executor (P1 T2 / AR-37 / AR-44) ────────────────────────────
    //
    // Integration tests stage a real capability dir (the AR-35 trio:
    // capability.json + manifest.json + <module-id>.wasm) from the EMBEDDED
    // basic-combat module + the canonical combat-input fixture — zero new
    // wasm fixtures (brief §Fixture). The expected 4-part output mirrors the
    // existing `crates/nexus-wasm-host/tests/basic_combat.rs` assertions:
    // state_delta `-`/`character.current_hp`/15 on `kb_def`, one
    // `state_update` event, empty `new_key_blocks`, `kind: "combat"`.

    /// True when the embedded module tree was compiled (wasm target
    /// installed); `nexus_no_wasm_target` (R-V1139P0-005) switches embedded
    /// lookups to empty stubs.
    fn embedded_available() -> bool {
        nexus_wasm_host::embedded_module_bytes("basic-combat").is_some()
    }

    /// Stage `<tmp>/<name>/` with `capability.json` (real sha of the embedded
    /// module), `manifest.json` + `<module-id>.wasm` from the embedded tree.
    /// `sandbox` (when given) is embedded verbatim into the descriptor.
    fn stage_capability_dir(tmp: &std::path::Path, name: &str, sandbox: Option<&str>) {
        use sha2::{Digest, Sha256};
        let dir = tmp.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let wasm = nexus_wasm_host::embedded_module_bytes("basic-combat").unwrap();
        let manifest = nexus_wasm_host::embedded_module_manifest("basic-combat").unwrap();
        let sha: String = {
            let mut hex = String::with_capacity(64);
            for b in Sha256::digest(wasm) {
                use std::fmt::Write as _;
                let _ = write!(hex, "{b:02x}");
            }
            hex
        };
        let sandbox_json = sandbox.map_or_else(String::new, |s| format!("\"sandbox\": {s},"));
        let json = format!(
            r#"{{
                "name": "{name}",
                "inputSchema": "{{\"type\":\"object\"}}",
                "outputSchema": "{{\"type\":\"object\"}}",
                {sandbox_json}
                "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{sha}" }}
            }}"#
        );
        std::fs::write(dir.join("capability.json"), json).unwrap();
        std::fs::write(dir.join("manifest.json"), manifest).unwrap();
        std::fs::write(dir.join("basic-combat.wasm"), wasm).unwrap();
    }

    /// The canonical combat fixture re-enveloped for the capability input
    /// surface: `keyBlocks` carries the two combatant blocks, `attacker_id` /
    /// `defender_id` select them (`snake_case` — the exact keys basic-combat's
    /// `select_combatants` reads from `invocation`, S-2 fix wave), the rest of
    /// the fixture body stays intact. `to_compute_input` lifts `keyBlocks`
    /// into the envelope and passes the whole object through as the raw
    /// invocation (AR-37).
    fn combat_capability_input() -> Value {
        let raw: Value = serde_json::from_str(include_str!(
            "../../../../modules/nexus-module-test/fixtures/combat-input.json"
        ))
        .expect("canonical fixture parses");
        let mut input = raw.as_object().cloned().unwrap();
        input.insert("keyBlocks".to_string(), raw["key_blocks"].clone());
        input.remove("key_blocks");
        input.remove("schema_version");
        input.remove("world_ref");
        input.remove("narrative_state");
        // The fixture's world_id lives inside world_ref (wire-valid `wld_…`);
        // re-express it on the capability input surface so the AR-37 mapping
        // carries it into the compute envelope's world_ref (the module emits
        // it on the timeline event, which the wire validates as `^wld_…$`).
        input.insert("worldId".to_string(), raw["world_ref"]["world_id"].clone());
        // The fixture's invocation carries `attacker_id` / `defender_id`
        // (snake_case) — the keys basic-combat's `select_combatants` reads
        // (manifest `schemas.invocation`). Lifted to the capability input
        // surface so the real selector branch is exercised (S-2: the previous
        // inert camelCase `attackerId`/`defenderId` only hit the fallback).
        input.insert("attacker_id".to_string(), serde_json::json!("kb_atk"));
        input.insert("defender_id".to_string(), serde_json::json!("kb_def"));
        Value::Object(input)
    }

    /// Real executor: `run()` on a staged capability dir executes the embedded
    /// basic-combat module and returns the expected 4-part output (the
    /// `basic_combat.rs` assertions). The admitted-with-engine path no longer
    /// returns the P0 stub's `WorkerUnavailable` (PL-10 closed).
    #[tokio::test]
    async fn run_executes_embedded_module_and_returns_four_part_output() {
        if !embedded_available() {
            eprintln!("skipping: embedded wasm target not installed (nexus_no_wasm_target)");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        stage_capability_dir(tmp.path(), "combat.cap", None);
        let engine = std::sync::Arc::new(nexus_wasm_host::WasmEngine::new().unwrap());
        let cache = std::sync::Arc::new(nexus_wasm_host::ModuleCache::new());
        let descriptor =
            parse(&std::fs::read_to_string(tmp.path().join("combat.cap/capability.json")).unwrap())
                .expect("staged descriptor parses");
        let cap = UserCapability::new(
            &descriptor,
            tmp.path().join("combat.cap"),
            Some(engine),
            Some(cache),
        );

        let out = cap
            .run(combat_capability_input())
            .await
            .expect("run() executes the module");

        // 1) battle_report carries the combat discriminator.
        assert_eq!(out["battle_report"]["kind"], "combat");

        // 2) Combat math: delta `-`/`character.current_hp`/15 on `kb_def`
        //    (matches basic_combat.rs L79-86).
        let delta = out["state_delta"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["target_key_block_id"] == "kb_def")
            .expect("delta targeting defender present");
        assert_eq!(delta["op"], "sub");
        assert_eq!(delta["path"], "character.current_hp");
        assert_eq!(delta["value"], serde_json::json!(15));

        // 3) timeline_events: one state_update event recording the outcome.
        let events = out["timeline_events"].as_array().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["event_type"], "state_update");
        assert!(
            events[0]["summary"]
                .as_str()
                .is_some_and(|s| s.contains("15") && s.contains("kb_def")),
            "event summary should mention damage and defender: {:?}",
            events[0]["summary"]
        );
        assert_eq!(
            events[0]["affected_key_block_ids"],
            serde_json::json!(["kb_atk", "kb_def"])
        );

        // 4) new_key_blocks empty for basic combat.
        assert_eq!(out["new_key_blocks"], serde_json::json!([]));

        // The full 4-part envelope is present verbatim.
        for key in [
            "state_delta",
            "timeline_events",
            "new_key_blocks",
            "battle_report",
        ] {
            assert!(out.get(key).is_some(), "missing envelope part '{key}'");
        }
    }

    /// AR-37 module-missing at `run()`: a capability whose dir lacks
    /// `manifest.json`/`<module-id>.wasm` (edited after admission) fails
    /// fail-closed with `InputInvalid`, never the engine path.
    #[tokio::test]
    async fn run_module_missing_after_admission_returns_input_invalid() {
        if !embedded_available() {
            eprintln!("skipping: basic wasm target not installed (nexus_no_wasm_target)");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        stage_capability_dir(tmp.path(), "combat.cap", None);
        // Simulate a post-admission edit: remove the module pair.
        std::fs::remove_file(tmp.path().join("combat.cap/basic-combat.wasm")).unwrap();
        let engine = std::sync::Arc::new(nexus_wasm_host::WasmEngine::new().unwrap());
        let cache = std::sync::Arc::new(nexus_wasm_host::ModuleCache::new());
        let descriptor =
            parse(&std::fs::read_to_string(tmp.path().join("combat.cap/capability.json")).unwrap())
                .expect("staged descriptor parses");
        let cap = UserCapability::new(
            &descriptor,
            tmp.path().join("combat.cap"),
            Some(engine),
            Some(cache),
        );

        let err = cap
            .run(combat_capability_input())
            .await
            .expect_err("missing module must fail");
        assert!(
            matches!(err, CapabilityError::InputInvalid(_)),
            "expected InputInvalid, got {err:?}"
        );
        assert!(
            err.to_string()
                .contains("not loaded in capability 'combat.cap'"),
            "named module/capability message, got: {err}"
        );
    }

    /// F1 / AR-38 runtime enforcement: a descriptor `sandbox.fuel` below the
    /// module/host budget must reach the invocation. basic-combat with the
    /// default budget runs to completion (the test above); a fuel budget of
    /// `100_000` (still > 0, so it admits) lets the module instantiate but
    /// traps inside `compute` -> `OutOfFuel` -> `CapabilityError::Forbidden`
    /// ("sandbox breach: ...") — proving the clamped descriptor override is
    /// applied at `run()`, not just validated. (A fuel sweep showed budgets
    /// below `~50_000` are consumed during instantiation and surface as
    /// `Internal`; `100_000` is the smallest round budget on the
    /// `compute`-trap side.)
    #[tokio::test]
    async fn descriptor_fuel_override_is_enforced_at_run() {
        if !embedded_available() {
            eprintln!("skipping: basic wasm target not installed (nexus_no_wasm_target)");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        stage_capability_dir(tmp.path(), "combat.cap", Some(r#"{"fuel": 100000}"#));
        let engine = std::sync::Arc::new(nexus_wasm_host::WasmEngine::new().unwrap());
        let cache = std::sync::Arc::new(nexus_wasm_host::ModuleCache::new());
        let descriptor =
            parse(&std::fs::read_to_string(tmp.path().join("combat.cap/capability.json")).unwrap())
                .expect("staged descriptor parses");
        let cap = UserCapability::new(
            &descriptor,
            tmp.path().join("combat.cap"),
            Some(engine),
            Some(cache),
        );

        let err = cap
            .run(combat_capability_input())
            .await
            .expect_err("fuel-capped invocation must trap");
        assert!(
            matches!(err, CapabilityError::Forbidden(_)),
            "expected Forbidden (sandbox breach), got {err:?}"
        );
        assert!(
            err.to_string().contains("sandbox breach"),
            "message names the sandbox breach, got: {err}"
        );
    }

    /// F2 / AR-39 TOCTOU: a capability dir edited after admission — the
    /// `.wasm` bytes swapped while `manifest.json` and `capability.json` keep
    /// the admitted hash — must fail closed at `run()` with `InputInvalid`
    /// before any compile/cache path executes unverified bytes.
    #[tokio::test]
    async fn run_rejects_module_bytes_changed_after_admission() {
        if !embedded_available() {
            eprintln!("skipping: embedded wasm target was not installed (nexus_no_wasm_target)");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        stage_capability_dir(tmp.path(), "combat.cap", None);
        // Post-admission edit: swap the wasm bytes (manifest + descriptor
        // still declare the ORIGINAL admitted hash — the admission-time
        // pairing no longer matches the on-disk pair).
        std::fs::write(
            tmp.path().join("combat.cap/basic-combat.wasm"),
            b"tampered module bytes",
        )
        .unwrap();
        let engine = std::sync::Arc::new(nexus_wasm_host::WasmEngine::new().unwrap());
        let cache = std::sync::Arc::new(nexus_wasm_host::ModuleCache::new());
        let descriptor =
            parse(&std::fs::read_to_string(tmp.path().join("combat.cap/capability.json")).unwrap())
                .expect("staged descriptor parses");
        let cap = UserCapability::new(
            &descriptor,
            tmp.path().join("combat.cap"),
            Some(engine),
            Some(cache),
        );

        let err = cap
            .run(combat_capability_input())
            .await
            .expect_err("swapped bytes must fail closed");
        assert!(
            matches!(err, CapabilityError::InputInvalid(_)),
            "expected InputInvalid, got {err:?}"
        );
        assert!(
            err.to_string().contains("hash changed for capability"),
            "message names the hash change, got: {err}"
        );
    }

    /// S-1a: `OutputSchemaMismatch` is an output-side module fault (the
    /// module emitted a malformed 4-part envelope) — mapped to
    /// `PermanentExternal`, never `InputInvalid` (retrying the same input
    /// cannot fix a module fault).
    #[test]
    fn maps_output_schema_mismatch_to_permanent_external() {
        let err = map_compute_error(&ComputeError::OutputSchemaMismatch(
            "battle_report missing".to_string(),
        ));
        assert!(
            matches!(err, CapabilityError::PermanentExternal(_)),
            "expected PermanentExternal, got {err:?}"
        );
        assert!(
            err.to_string().contains("module fault"),
            "message names the module fault, got: {err}"
        );
    }

    /// AR-40: the user capability overrides `origin()` → `User`.
    #[test]
    fn user_capability_origin_is_user() {
        let descriptor = parse(&minimal_json()).expect("minimal descriptor must parse");
        let cap = UserCapability::new(&descriptor, PathBuf::new(), None, None);
        assert!(matches!(cap.origin(), CapabilityOrigin::User));
    }
}
