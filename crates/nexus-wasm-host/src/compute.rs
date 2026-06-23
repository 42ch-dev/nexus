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
use crate::manifest::ModuleManifest;
use crate::{ComputeInput, ComputeOutput, HostContext, WasmEngine, WasmModule};

/// Size of the output buffer the host reserves in module memory. Generous by
/// design — a basic-combat `ComputeOutput` is a few KiB; 1 MiB covers far richer
/// modules without forcing a second round-trip.
const OUTPUT_BUFFER_BYTES: u32 = 1 << 20; // 1 MiB

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
        let sandbox = self.resolve_sandbox(manifest);
        let input_bytes = serde_json::to_vec(input)?;
        self.run_invocation(module, manifest, &input_bytes, sandbox)
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
