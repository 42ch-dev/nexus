//! The stateless `compute` entry point (compass Q6: per-invocation sandbox).
//!
//! [`WasmEngine::compute`] takes a compiled module, its manifest, and a
//! `ComputeInput`, then:
//!
//! 1. Builds a **fresh** `Store` carrying the invocation's fuel, memory cap,
//!    and the host snapshot (served to host functions).
//! 2. Arms the wall-time watchdog (epoch interruption).
//! 3. Instantiates the module with only the whitelisted host imports linked.
//! 4. Calls `init` (if declared), then writes the input JSON, calls `compute`,
//!    reads back the `ComputeOutput` JSON.
//! 5. Tears down the instance — nothing is reused across calls.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use wasmtime::{Instance, Linker, Store, StoreLimitsBuilder, Trap, TypedFunc};

use crate::error::{ComputeError, Result};
use crate::host::{register_host_imports, InvocationState};
use crate::manifest::{ModuleManifest, ModuleSchemas};
use crate::{ComputeInput, ComputeOutput, HostContext, WasmEngine, WasmModule};

/// Size of the output buffer the host reserves in module memory. Generous by
/// design — a basic-combat `ComputeOutput` is a few KiB; 1 MiB covers far richer
/// modules without forcing a second round-trip.
const OUTPUT_BUFFER_BYTES: u32 = 1 << 20; // 1 MiB

/// Maximum recursion depth for JSON-Schema validation (qc3 W-001).
/// Covers any realistic compute-envelope depth (`KeyBlocks` × attributes/state ×
/// nested objects/arrays). Exceeding this returns `ManifestValidationFailed`
/// instead of overflowing the stack.
const MAX_VALIDATION_DEPTH: usize = 64;

impl WasmEngine {
    /// Run a single stateless compute invocation.
    ///
    /// `module` is a compiled [`WasmModule`] (reusable across calls). `manifest`
    /// declares the module's exports + host-function whitelist + optional
    /// sandbox overrides. `input` is the `ComputeInput` envelope serialized to
    /// the module.
    ///
    /// # Errors
    /// See [`ComputeError`]: fuel/wall-time/memory breaches, missing exports,
    /// module traps, or output-envelope mismatches.
    pub fn compute(
        &self,
        module: &WasmModule,
        manifest: &ModuleManifest,
        input: &ComputeInput,
    ) -> Result<ComputeOutput> {
        // --- Pre-invocation manifest-schema validation (V1.62) ------------
        if let Some(schemas) = &manifest.schemas {
            let input_value = serde_json::to_value(input)?;
            validate_compute_input(&input_value, schemas)?;
        }

        let sandbox = self.resolve_sandbox(manifest);
        let input_bytes = serde_json::to_vec(input)?;
        let output = self.run_invocation(module, manifest, &input_bytes, sandbox)?;

        // --- Post-invocation battle_report validation (V1.62) -------------
        if let Some(schemas) = &manifest.schemas {
            if let Some(ref battle_report_schema) = schemas.battle_report {
                validate_battle_report(&output, battle_report_schema)?;
            }
        }

        Ok(output)
    }

    /// Resolve the effective sandbox limits: manifest overrides take precedence,
    /// otherwise the engine default applies.
    fn resolve_sandbox(&self, manifest: &ModuleManifest) -> ResolvedSandbox {
        let base = self.default_sandbox();
        ResolvedSandbox {
            fuel: manifest.max_fuel.unwrap_or(base.fuel),
            max_memory_bytes: manifest
                .max_memory_mib
                .map_or(base.max_memory_bytes, |mib| {
                    usize::try_from(mib)
                        .unwrap_or(0)
                        .saturating_mul(1024 * 1024)
                }),
            wall_time: manifest
                .max_wall_time_ms
                .map_or(base.wall_time, Duration::from_millis),
        }
    }

    fn run_invocation(
        &self,
        module: &WasmModule,
        manifest: &ModuleManifest,
        input_bytes: &[u8],
        sandbox: ResolvedSandbox,
    ) -> Result<ComputeOutput> {
        // --- 1. Fresh per-invocation store (snapshot + memory cap) ----------
        let limits = StoreLimitsBuilder::new()
            .memory_size(sandbox.max_memory_bytes)
            .build();
        let input: ComputeInput = serde_json::from_slice(input_bytes).unwrap_or_default();
        let state = InvocationState {
            ctx: HostContext::from_input(&input),
            limits,
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|s| &mut s.limits);

        // Fuel budget for this invocation.
        store.set_fuel(sandbox.fuel)?;

        // --- 2. Wall-time watchdog (epoch interruption) ---------------------
        store.epoch_deadline_trap();
        store.set_epoch_deadline(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let watchdog = spawn_watchdog(self.engine.clone(), sandbox.wall_time, cancelled.clone());

        // --- 3–4. Instantiate, init, compute --------------------------------
        let outcome = self.invoke_module(&mut store, module, manifest, input_bytes);

        // --- 5. Reap the watchdog -------------------------------------------
        cancelled.store(true, Ordering::SeqCst);
        if let Some(handle) = watchdog {
            // Best-effort join; the thread exits promptly once `cancelled` is set
            // or after sleeping `wall_time` (whichever comes first).
            let _ = handle.join();
        }
        outcome
    }

    fn invoke_module(
        &self,
        store: &mut Store<InvocationState>,
        module: &WasmModule,
        manifest: &ModuleManifest,
        input_bytes: &[u8],
    ) -> Result<ComputeOutput> {
        let mut linker: Linker<InvocationState> = Linker::new(&self.engine);
        register_host_imports(&mut linker, manifest)?;

        let instance = linker
            .instantiate(&mut *store, &module.module)
            .map_err(map_instantiate_error)?;

        // Optional one-shot `init`.
        if let Some(init) = optional_export::<(), ()>(&mut *store, &instance, &manifest.init_export)
        {
            map_call_result(init.call(&mut *store, ()))?;
        }

        // `alloc` export: host places input and reserves output inside the
        // module's own linear memory.
        let alloc = required_export::<u32, u32>(&mut *store, &instance, "alloc")?;
        let in_len = u32::try_from(input_bytes.len()).unwrap_or(u32::MAX);
        let in_ptr = map_call_result(alloc.call(&mut *store, in_len))?;
        let memory = instance
            .get_memory(&mut *store, "memory")
            .ok_or_else(|| ComputeError::MissingExport("memory".into()))?;
        memory.write(&mut *store, in_ptr as usize, input_bytes)?;

        let out_cap = OUTPUT_BUFFER_BYTES;
        let out_ptr = map_call_result(alloc.call(&mut *store, out_cap))?;

        // `compute`.
        let compute = required_export::<(u32, u32, u32, u32), i64>(
            &mut *store,
            &instance,
            &manifest.compute_export,
        )?;
        let written =
            map_call_result(compute.call(&mut *store, (in_ptr, in_len, out_ptr, out_cap)))?;

        if written < 0 {
            return Err(ComputeError::ModuleComputeFailed(written));
        }
        let written =
            usize::try_from(written).map_err(|_| ComputeError::OutputBufferTooSmall(usize::MAX))?;
        if written > out_cap as usize {
            return Err(ComputeError::OutputBufferTooSmall(written));
        }

        // Read + deserialize the 4-part envelope.
        let mut out_bytes = vec![0u8; written];
        memory.read(&*store, out_ptr as usize, &mut out_bytes)?;
        let output: ComputeOutput = serde_json::from_slice(&out_bytes)
            .map_err(|e| ComputeError::InvalidOutput(e.to_string()))?;
        validate_output_shape(&output)?;
        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// Manifest schema validation (V1.62)
// ---------------------------------------------------------------------------
// Uses a hand-rolled minimal JSON-Schema validator supporting the subset
// needed for module manifests: type, properties, required, additionalProperties,
// minimum, items, const. Chosen over the jsonschema crate to keep compile times
// low and avoid pulling in heavy transitive dependencies for a handful of
// keyword checks (per compass §5 design item #1).

use serde_json::Value;

/// Validate all applicable manifest-schema fragments against `ComputeInput`.
fn validate_compute_input(input: &Value, schemas: &ModuleSchemas) -> Result<()> {
    // -- key_block_attributes -------------------------------------------------
    if let Some(ref kb_attrs) = schemas.key_block_attributes {
        let empty = vec![];
        let key_blocks = input
            .get("key_blocks")
            .and_then(Value::as_array)
            .unwrap_or(&empty);
        for (i, kb) in key_blocks.iter().enumerate() {
            let block_type = kb.get("block_type").and_then(Value::as_str).unwrap_or("");
            if let Some(schema) = kb_attrs.get(block_type) {
                let attrs = kb.get("body").and_then(|b| b.get("attributes"));
                let instance = attrs.unwrap_or(&Value::Null);
                let path = format!("key_blocks[{i}].body.attributes");
                validate_against_schema(instance, &path, schema, &path, MAX_VALIDATION_DEPTH)?;
            }
        }
    }

    // -- key_block_state ------------------------------------------------------
    if let Some(ref kb_state) = schemas.key_block_state {
        let empty = vec![];
        let key_blocks = input
            .get("key_blocks")
            .and_then(Value::as_array)
            .unwrap_or(&empty);
        for (i, kb) in key_blocks.iter().enumerate() {
            let block_type = kb.get("block_type").and_then(Value::as_str).unwrap_or("");
            if let Some(schema) = kb_state.get(block_type) {
                let state = kb
                    .get("body")
                    .and_then(|b| b.get("state"))
                    .and_then(|s| s.get(block_type));
                let instance = state.unwrap_or(&Value::Null);
                let path = format!("key_blocks[{i}].body.state.{block_type}");
                validate_against_schema(instance, &path, schema, &path, MAX_VALIDATION_DEPTH)?;
            }
        }
    }

    // -- invocation -----------------------------------------------------------
    if let Some(ref invocation_schema) = schemas.invocation {
        let inv = input.get("invocation");
        // Skip validation if invocation is absent or null (invocation is
        // optional in ComputeInput).
        if let Some(inv) = inv {
            if !inv.is_null() {
                validate_against_schema(
                    inv,
                    "invocation",
                    invocation_schema,
                    "invocation",
                    MAX_VALIDATION_DEPTH,
                )?;
            }
        }
    }

    Ok(())
}

/// Validate `ComputeOutput.battle_report` against the manifest-declared schema.
fn validate_battle_report(output: &ComputeOutput, schema: &Value) -> Result<()> {
    let report = &output.battle_report;
    if report.is_null() {
        return Err(ComputeError::ManifestValidationFailed {
            path: "battle_report".into(),
            detail: "battle_report must be an object".into(),
        });
    }
    validate_against_schema(
        report,
        "battle_report",
        schema,
        "battle_report",
        MAX_VALIDATION_DEPTH,
    )?;
    Ok(())
}

/// Core hand-rolled validator. Walks `instance` against `schema` (a JSON-Schema
/// object fragment). On the first mismatch, returns `ManifestValidationFailed`
/// with `instance_path` identifying the failing field.
///
/// Supported keywords: `type`, `properties`, `required`, `additionalProperties`,
/// `minimum`, `items`, `const`.
fn validate_against_schema(
    instance: &Value,
    instance_path: &str,
    schema: &Value,
    _schema_path: &str,
    depth_limit: usize,
) -> Result<()> {
    if depth_limit == 0 {
        return Err(ComputeError::ManifestValidationFailed {
            path: instance_path.to_string(),
            detail: "exceeded maximum validation depth (64)".to_string(),
        });
    }
    let Some(obj) = schema.as_object() else {
        return Ok(()); // empty / non-object schema → pass
    };

    // `type` check
    if let Some(expected) = obj.get("type").and_then(Value::as_str) {
        if !check_type(instance, expected) {
            return Err(ComputeError::ManifestValidationFailed {
                path: instance_path.to_string(),
                detail: format!(
                    "expected type {}, got {}",
                    expected,
                    describe_value(instance)
                ),
            });
        }
    }

    // `const` check
    if let Some(const_val) = obj.get("const") {
        if instance != const_val {
            return Err(ComputeError::ManifestValidationFailed {
                path: instance_path.to_string(),
                detail: format!("expected const value {const_val}, got {instance}"),
            });
        }
    }

    // `required` — only for objects
    if let Some(instance_obj) = instance.as_object() {
        if let Some(required) = obj.get("required").and_then(Value::as_array) {
            for req in required {
                let key = req.as_str().unwrap_or("");
                if !instance_obj.contains_key(key) {
                    return Err(ComputeError::ManifestValidationFailed {
                        path: instance_path.to_string(),
                        detail: format!("missing required field: {key}"),
                    });
                }
            }
        }

        // `properties` — validate each declared property, and collect declared
        // keys for the `additionalProperties` check below.
        let declared_keys: Option<std::collections::HashSet<&str>> =
            if let Some(properties) = obj.get("properties").and_then(Value::as_object) {
                for (prop_name, prop_schema) in properties {
                    if let Some(prop_val) = instance_obj.get(prop_name) {
                        let child_path = format!("{instance_path}.{prop_name}");
                        validate_against_schema(
                            prop_val,
                            &child_path,
                            prop_schema,
                            &child_path,
                            depth_limit - 1,
                        )?;
                    }
                }
                Some(properties.keys().map(String::as_str).collect())
            } else {
                None
            };

        // `additionalProperties`: if false and instance has undeclared keys, flag
        if obj.get("additionalProperties") == Some(&Value::Bool(false)) {
            let declared = declared_keys.unwrap_or_default();
            for key in instance_obj.keys() {
                if !declared.contains(key.as_str()) {
                    return Err(ComputeError::ManifestValidationFailed {
                        path: format!("{instance_path}.{key}"),
                        detail: "additional properties not allowed".to_string(),
                    });
                }
            }
        }
    }

    // `minimum` — only for numbers
    if let Some(min) = obj.get("minimum").and_then(Value::as_i64) {
        if let Some(n) = instance.as_i64() {
            if n < min {
                return Err(ComputeError::ManifestValidationFailed {
                    path: instance_path.to_string(),
                    detail: format!("value {n} is less than minimum {min}"),
                });
            }
        }
    }

    // `items` — for arrays, validate each element against the items schema
    if let Some(instance_arr) = instance.as_array() {
        if let Some(items_schema) = obj.get("items") {
            for (i, elem) in instance_arr.iter().enumerate() {
                let child_path = format!("{instance_path}[{i}]");
                validate_against_schema(
                    elem,
                    &child_path,
                    items_schema,
                    &child_path,
                    depth_limit - 1,
                )?;
            }
        }
    }

    Ok(())
}

/// Check whether a JSON value matches the given JSON-Schema type name.
fn check_type(val: &Value, expected: &str) -> bool {
    match expected {
        "object" => val.is_object(),
        "string" => val.is_string(),
        "integer" => val.is_i64() || val.is_u64(),
        "boolean" => val.is_boolean(),
        "array" => val.is_array(),
        "number" => val.is_number(),
        _ => true, // unknown type → pass
    }
}

fn describe_value(val: &Value) -> &'static str {
    if val.is_object() {
        "object"
    } else if val.is_array() {
        "array"
    } else if val.is_string() {
        "string"
    } else if val.is_i64() || val.is_u64() {
        "integer"
    } else if val.is_f64() {
        "number"
    } else if val.is_boolean() {
        "boolean"
    } else if val.is_null() {
        "null"
    } else {
        "unknown"
    }
}

/// Effective per-invocation limits (manifest overrides applied).
#[derive(Clone, Copy)]
struct ResolvedSandbox {
    fuel: u64,
    max_memory_bytes: usize,
    wall_time: Duration,
}

// ---------------------------------------------------------------------------
// Error mapping
// ---------------------------------------------------------------------------

/// Map a wasmtime call result, translating fuel/epoch traps into typed errors.
fn map_call_result<T>(res: wasmtime::Result<T>) -> Result<T> {
    res.map_err(|e| {
        if e.downcast_ref::<Trap>() == Some(&Trap::OutOfFuel) {
            ComputeError::OutOfFuel
        } else if e.downcast_ref::<Trap>() == Some(&Trap::Interrupt) {
            ComputeError::WallTimeExceeded
        } else if is_memory_trap(&e) {
            ComputeError::MemoryCapExceeded
        } else {
            ComputeError::Trap(e.to_string())
        }
    })
}

fn map_instantiate_error(e: wasmtime::Error) -> ComputeError {
    if is_memory_trap(&e) {
        ComputeError::MemoryCapExceeded
    } else {
        ComputeError::Wasmtime(e)
    }
}

fn is_memory_trap(e: &wasmtime::Error) -> bool {
    let msg = e.to_string().to_lowercase();
    msg.contains("memory")
        && (msg.contains("grow") || msg.contains("limit") || msg.contains("exceed"))
}

/// Light-weight post-condition: the required `battle_report` is a real object.
/// (The generated `ComputeOutput` struct already enforces field types; this
/// guards against a module emitting JSON `null` for the report.)
fn validate_output_shape(output: &ComputeOutput) -> Result<()> {
    if output.battle_report.is_null() {
        return Err(ComputeError::OutputSchemaMismatch(
            "battle_report must be an object".into(),
        ));
    }
    Ok(())
}

fn optional_export<Params, Returns>(
    store: &mut Store<InvocationState>,
    instance: &Instance,
    name: &str,
) -> Option<TypedFunc<Params, Returns>>
where
    Params: wasmtime::WasmParams,
    Returns: wasmtime::WasmResults,
{
    if name.is_empty() {
        return None;
    }
    instance.get_typed_func::<Params, Returns>(store, name).ok()
}

fn required_export<Params, Returns>(
    store: &mut Store<InvocationState>,
    instance: &Instance,
    name: &str,
) -> Result<TypedFunc<Params, Returns>>
where
    Params: wasmtime::WasmParams,
    Returns: wasmtime::WasmResults,
{
    instance
        .get_typed_func::<Params, Returns>(store, name)
        .map_err(|_| ComputeError::MissingExport(name.to_string()))
}

// ---------------------------------------------------------------------------
// Wall-time watchdog
// ---------------------------------------------------------------------------

fn spawn_watchdog(
    engine: wasmtime::Engine,
    wall_time: Duration,
    cancelled: Arc<AtomicBool>,
) -> Option<thread::JoinHandle<()>> {
    thread::Builder::new()
        .name("nexus-wasm-watchdog".into())
        .spawn(move || {
            // Sleep in small chunks so that, once `compute()` finishes and sets
            // `cancelled`, this thread exits within `STEP` instead of blocking
            // the caller's `join()` for the full `wall_time`. Epoch is only
            // bumped if the budget truly elapses without cancellation.
            const STEP: Duration = Duration::from_millis(25);
            let mut elapsed = Duration::ZERO;
            while elapsed < wall_time {
                if cancelled.load(Ordering::SeqCst) {
                    return;
                }
                let remaining = wall_time.checked_sub(elapsed).unwrap();
                thread::sleep(STEP.min(remaining));
                elapsed += STEP;
            }
            if !cancelled.load(Ordering::SeqCst) {
                engine.increment_epoch();
            }
        })
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ComputeError;
    use crate::manifest::ModuleSchemas;
    use serde_json::json;

    // ------------------------------------------------------------------
    // validate_against_schema unit tests
    // ------------------------------------------------------------------

    #[test]
    fn valid_object_passes_type_check() {
        let schema = json!({"type": "object"});
        let instance = json!({"a": 1});
        assert!(
            validate_against_schema(&instance, "root", &schema, "root", MAX_VALIDATION_DEPTH)
                .is_ok()
        );
    }

    #[test]
    fn wrong_type_fails_with_path() {
        let schema = json!({"type": "string"});
        let instance = json!(42);
        let err = validate_against_schema(
            &instance,
            "invocation.attacker_id",
            &schema,
            "x",
            MAX_VALIDATION_DEPTH,
        )
        .unwrap_err();
        match err {
            ComputeError::ManifestValidationFailed { path, detail } => {
                assert_eq!(path, "invocation.attacker_id");
                assert!(detail.contains("expected type string"));
                assert!(detail.contains("integer"));
            }
            other => panic!("expected ManifestValidationFailed, got {other:?}"),
        }
    }

    #[test]
    fn missing_required_field_fails() {
        let schema = json!({
            "type": "object",
            "properties": {
                "base_atk": {"type": "integer"},
                "max_hp": {"type": "integer"}
            },
            "required": ["base_atk", "max_hp"]
        });
        // Instance is missing `base_atk`.
        let instance = json!({"max_hp": 100});
        let err = validate_against_schema(
            &instance,
            "key_blocks[0].body.attributes",
            &schema,
            "x",
            MAX_VALIDATION_DEPTH,
        )
        .unwrap_err();
        match err {
            ComputeError::ManifestValidationFailed { path, detail } => {
                assert_eq!(path, "key_blocks[0].body.attributes");
                assert!(detail.contains("missing required field"));
                assert!(detail.contains("base_atk"));
            }
            other => panic!("expected ManifestValidationFailed, got {other:?}"),
        }
    }

    #[test]
    fn valid_object_with_all_required_fields_passes() {
        let schema = json!({
            "type": "object",
            "properties": {
                "base_atk": {"type": "integer", "minimum": 0},
                "max_hp": {"type": "integer", "minimum": 0}
            },
            "required": ["base_atk", "max_hp"]
        });
        let instance = json!({"base_atk": 10, "max_hp": 100});
        assert!(
            validate_against_schema(&instance, "root", &schema, "root", MAX_VALIDATION_DEPTH)
                .is_ok()
        );
    }

    #[test]
    fn minimum_constraint_fails() {
        let schema = json!({"type": "integer", "minimum": 1});
        let instance = json!(0);
        let err = validate_against_schema(&instance, "level", &schema, "x", MAX_VALIDATION_DEPTH)
            .unwrap_err();
        match err {
            ComputeError::ManifestValidationFailed { path, detail } => {
                assert_eq!(path, "level");
                assert!(detail.contains("less than minimum"));
            }
            other => panic!("expected ManifestValidationFailed, got {other:?}"),
        }
    }

    #[test]
    fn const_check_fails() {
        let schema = json!({"type": "string", "const": "combat"});
        let instance = json!("exploration");
        let err = validate_against_schema(
            &instance,
            "battle_report.kind",
            &schema,
            "x",
            MAX_VALIDATION_DEPTH,
        )
        .unwrap_err();
        match err {
            ComputeError::ManifestValidationFailed { path, detail } => {
                assert_eq!(path, "battle_report.kind");
                assert!(detail.contains("expected const"));
            }
            other => panic!("expected ManifestValidationFailed, got {other:?}"),
        }
    }

    #[test]
    fn array_items_validation() {
        let schema = json!({
            "type": "array",
            "items": {"type": "string"}
        });
        let valid = json!(["a", "b"]);
        assert!(validate_against_schema(&valid, "arr", &schema, "x", MAX_VALIDATION_DEPTH).is_ok());

        let invalid = json!(["a", 42]);
        let err = validate_against_schema(&invalid, "arr", &schema, "x", MAX_VALIDATION_DEPTH)
            .unwrap_err();
        match err {
            ComputeError::ManifestValidationFailed { path, detail } => {
                assert!(path.starts_with("arr[1]"));
                assert!(detail.contains("expected type string"));
            }
            other => panic!("expected ManifestValidationFailed, got {other:?}"),
        }
    }

    #[test]
    fn nested_object_validation_path() {
        let schema = json!({
            "type": "object",
            "properties": {
                "body": {
                    "type": "object",
                    "properties": {
                        "attributes": {
                            "type": "object",
                            "properties": {
                                "base_atk": {"type": "integer"}
                            },
                            "required": ["base_atk"]
                        }
                    }
                }
            }
        });
        // Missing body.attributes.base_atk
        let instance = json!({"body": {"attributes": {}}});
        let err = validate_against_schema(
            &instance,
            "key_blocks[0]",
            &schema,
            "x",
            MAX_VALIDATION_DEPTH,
        )
        .unwrap_err();
        match err {
            ComputeError::ManifestValidationFailed { path, detail } => {
                assert!(path.contains("body.attributes"));
                assert!(detail.contains("missing required field"));
            }
            other => panic!("expected ManifestValidationFailed, got {other:?}"),
        }
    }

    // ------------------------------------------------------------------
    // validate_compute_input tests
    // ------------------------------------------------------------------

    fn make_schemas() -> ModuleSchemas {
        ModuleSchemas {
            key_block_attributes: Some(
                [(
                    "character".to_string(),
                    json!({
                        "type": "object",
                        "properties": {
                            "max_hp": {"type": "integer", "minimum": 0},
                            "base_atk": {"type": "integer", "minimum": 0},
                            "base_def": {"type": "integer", "minimum": 0}
                        },
                        "required": ["max_hp", "base_atk", "base_def"]
                    }),
                )]
                .into(),
            ),
            invocation: Some(json!({
                "type": "object",
                "properties": {
                    "attacker_id": {"type": "string"},
                    "defender_id": {"type": "string"}
                }
            })),
            ..Default::default()
        }
    }

    #[test]
    fn valid_compute_input_passes_validation() {
        let schemas = make_schemas();
        let input = json!({
            "key_blocks": [{
                "block_type": "character",
                "body": {
                    "attributes": {
                        "max_hp": 100,
                        "base_atk": 20,
                        "base_def": 10,
                        "speed": 5
                    }
                }
            }]
        });
        let result = validate_compute_input(&input, &schemas);
        assert!(result.is_ok(), "validation failed: {result:?}");
    }

    #[test]
    fn missing_required_attribute_fails_with_json_path() {
        let schemas = make_schemas();
        // Missing `base_atk` (required).
        let input = json!({
            "key_blocks": [{
                "block_type": "character",
                "body": {
                    "attributes": {
                        "max_hp": 100,
                        "base_def": 10
                    }
                }
            }]
        });
        let err = validate_compute_input(&input, &schemas).unwrap_err();
        match err {
            ComputeError::ManifestValidationFailed { path, detail } => {
                assert!(path.contains("key_blocks[0].body.attributes"));
                assert!(detail.contains("missing required field"));
                assert!(detail.contains("base_atk"));
            }
            other => panic!("expected ManifestValidationFailed, got {other:?}"),
        }
    }

    #[test]
    fn invocation_wrong_type_fails() {
        let schemas = make_schemas();
        let input = json!({
            "key_blocks": [],
            "invocation": {
                "attacker_id": 42
            }
        });
        let err = validate_compute_input(&input, &schemas).unwrap_err();
        match err {
            ComputeError::ManifestValidationFailed { path, detail } => {
                assert!(path.contains("invocation"));
                assert!(detail.contains("expected type string"));
            }
            other => panic!("expected ManifestValidationFailed, got {other:?}"),
        }
    }

    #[test]
    fn no_schemas_means_no_validation() {
        // Without schemas declared, validate_compute_input is never called
        // (guarded by `if let Some(schemas)` in compute()).
        // This test verifies that an input that would fail validation passes
        // when schemas have NO matching key.
        let schemas = ModuleSchemas {
            key_block_attributes: Some([("monster".to_string(), json!({"type": "object"}))].into()),
            ..Default::default()
        };
        // key_block type is "character", but schema only covers "monster" →
        // no validation for this block.
        let input = json!({
            "key_blocks": [{
                "block_type": "character",
                "body": {"attributes": {"anything": "goes"}}
            }]
        });
        assert!(validate_compute_input(&input, &schemas).is_ok());
    }

    #[test]
    fn empty_schemas_object_no_validation() {
        // schemas is Some but all sub-fields are None → no validation.
        let schemas = ModuleSchemas::default();
        let input = json!({"key_blocks": [{"block_type": "character", "body": {}}]});
        assert!(validate_compute_input(&input, &schemas).is_ok());
    }

    #[test]
    fn basic_combat_manifest_parses_with_schemas() {
        // Ensure the real basic-combat manifest.json from the module directory
        // deserializes correctly (embedded copy used at runtime).
        let manifest_json = include_str!("../../../modules/basic-combat/manifest.json");
        let m: ModuleManifest = serde_json::from_str(manifest_json).unwrap();
        let schemas = m.schemas.expect("basic-combat should have schemas");
        assert!(schemas.key_block_attributes.is_some());
        assert!(schemas.key_block_state.is_some());
        assert!(schemas.invocation.is_some());
        assert!(schemas.battle_report.is_some());
    }

    // ------------------------------------------------------------------
    // Depth-limit tests (qc3 W-001 fix)
    // ------------------------------------------------------------------

    #[test]
    fn deeply_nested_properties_rejected_by_depth_limit() {
        // Adversarial schema: 100 levels of nested `properties`.
        let mut schema = json!({"type": "integer"});
        for _ in 0..100 {
            schema = json!({"type": "object", "properties": {"a": schema}});
        }
        // Matching instance: 100 levels of nested objects.
        let mut instance = json!(42);
        for _ in 0..100 {
            instance = json!({"a": instance});
        }

        let err = validate_against_schema(&instance, "root", &schema, "x", MAX_VALIDATION_DEPTH)
            .unwrap_err();
        match err {
            ComputeError::ManifestValidationFailed { detail, .. } => {
                assert!(
                    detail.contains("exceeded maximum validation depth"),
                    "expected depth-limit message, got: {detail}"
                );
            }
            other => panic!("expected ManifestValidationFailed, got {other:?}"),
        }
    }

    #[test]
    fn deeply_nested_items_rejected_by_depth_limit() {
        // Adversarial instance: 100 levels of nested arrays.
        // Schema mirrors the depth so each level validates.
        let mut schema = json!({"type": "integer"});
        for _ in 0..100 {
            schema = json!({"type": "array", "items": schema});
        }
        let mut instance = json!(42);
        for _ in 0..100 {
            instance = json!([instance]);
        }

        let err = validate_against_schema(&instance, "root", &schema, "x", MAX_VALIDATION_DEPTH)
            .unwrap_err();
        match err {
            ComputeError::ManifestValidationFailed { detail, .. } => {
                assert!(
                    detail.contains("exceeded maximum validation depth"),
                    "expected depth-limit message, got: {detail}"
                );
            }
            other => panic!("expected ManifestValidationFailed, got {other:?}"),
        }
    }
}
