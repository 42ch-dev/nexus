# Nexus WASM Compute Host (nexus-wasm-host)

## 0. Document position

| Attribute | Value |
| --- | --- |
| **Status** | Normative — V1.62 Shipped |
| **Document class** | Master |
| **Scope** | `nexus-wasm-host` crate: wasmtime runtime, engine lifecycle, per-invocation sandbox, limits, wall-time watchdog, embedded module loading, user module discovery, error taxonomy, host function implementation |
| **Last updated** | 2026-06-23 — V1.62 P2 |
| **Related** | [compute-module-abi.md](./compute-module-abi.md), [orchestration-engine.md](./orchestration-engine.md) §8 (narrative.compute), [entity-scope-model.md](./entity-scope-model.md) §5.5.9, [`crates/nexus-wasm-host/AGENTS.md`](../../../crates/nexus-wasm-host/AGENTS.md) |

This Master is normative for the `nexus-wasm-host` crate — the sandboxed
WebAssembly runtime that hosts compute modules for the `narrative.compute`
orchestration capability (V1.61 "Programmable Narrative Progression").
It consolidates the crate's `AGENTS.md`, the V1.61 compass grill decisions
(Q1/Q6/Q10), and the source-level implementation details into a single durable
reference.

The `nexus-wasm-host` crate is the **runtime**; it does not wire itself into the
daemon. The `narrative.compute` capability (orchestration) calls into it. Read
[compute-module-abi.md](./compute-module-abi.md) for the module-side contract
that this crate implements on the host side.

---

## 1. Runtime overview

The host uses [wasmtime](https://wasmtime.dev/) (Bytecode Alliance) as its WASM
runtime. It enables two wasmtime features at engine construction:

- **Fuel metering** (`Config::consume_fuel(true)`) — per-instruction budget that
  traps the instance when depleted. This is the primary compute-bound guard.
- **Epoch interruption** (`Config::epoch_interruption(true)`) — enables a
  wall-time watchdog thread to trap runaway modules via `Engine::increment_epoch()`.

The host constructs a single `WasmEngine` at daemon startup and reuses it for
every `compute()` call. The engine holds a default `SandboxConfig`; per-call
overrides come from the module's `manifest.json` (see §4).

The host does **not** enable WASI — compute modules are pure functions without
filesystem, networking, or clock access. The only host functions available are
`kb_read` and `narrative_query`, and only when the module's manifest whitelists
them (see §9).

---

## 2. Engine lifecycle

### 2.1 Single daemon-wide engine

The daemon constructs exactly one `WasmEngine` instance at boot (P-last V1.61
T1). The engine is created with the default `SandboxConfig` and the
fuel + epoch-interruption features enabled. All compute modules — both
embedded and user-discovered — are compiled and cached against this single
engine.

```rust
// Construct once, reuse for the lifetime of the daemon.
let engine = WasmEngine::new()?; // enables fuel + epoch-interruption
```

### 2.2 Module cache

Compiled modules are cached in an `Arc<RwLock<HashMap<String, WasmModule>>>`
keyed by `module_id`. On first load, the host compiles the module from bytes
and inserts it into the cache. Subsequent `compute()` calls retrieve the cached
`WasmModule` (a thin `Arc` wrapper around wasmtime's `Module`). Compilation is
a one-time cost per module; instantiation (per-call) is cheap because wasmtime
shares compiled code across instances.

The module cache is populated at daemon boot in two phases:

1. **Embedded modules** — loaded from `include_dir!` (compiled into the binary).
2. **User modules** — discovered by scanning `~/.nexus42/modules/`.

Cache warmup failures are aggregated into a single `ComputeError::CacheWarmup`
so a single bad module does not block the daemon. Individual module errors are
logged at `warn!`.

---

## 3. Per-invocation sandbox

Every `compute()` call builds a **fresh** wasmtime `Store` + `Instance`
(compass V1.61 Q6). No state carries over between invocations. The host follows
this sequence for each call:

```text
1. Load the compiled WasmModule from cache (by module_id).
2. Construct a fresh Store<InvocationState> with:
   - HostContext (snapshot of key_blocks + narrative_state, see §9).
   - StoreLimits (memory/instance/table caps, see §4).
3. Set fuel budget on the Store.
4. Spawn wall-time watchdog thread (see §5).
5. Link host imports via Linker<InvocationState> (see §9).
6. Instantiate the module into the Store.
7. Call init() if the module exports it and the manifest declares it.
8. Allocate input buffer via alloc(); write ComputeInput JSON.
9. Allocate output buffer via alloc().
10. Call compute(in_ptr, in_len, out_ptr, out_cap).
11. Read output JSON from [out_ptr, out_ptr+written).
12. Deserialize and validate against ComputeOutput schema.
13. Cancel watchdog; return ComputeOutput or ComputeError.
```

The fresh-Store guarantee means a module that corrupts its own memory, leaks
allocations, or exhausts fuel does not affect subsequent invocations.
The wasmtime `Store` is dropped after each call, freeing all instance resources.

---

## 4. Sandbox limits

Three independent limits are enforced on every `compute()` call. Each limit
starts from a clean slate (per-invocation sandbox). A breach of any one traps
the instance and is reported as a `ComputeError` variant — it never crashes the
host.

| Limit | Default | Manifest override | Error variant | Mechanism |
| --- | --- | --- | --- | --- |
| Fuel (instruction count) | 10,000,000 | `max_fuel` | `OutOfFuel` | `Store::set_fuel()` + `consume_fuel(true)` |
| Memory cap | 64 MiB | `max_memory_mib` | `MemoryCapExceeded` | `StoreLimitsBuilder::memory_size()` resource limiter |
| Wall-time | 30 seconds | `max_wall_time_ms` | `WallTimeExceeded` | Epoch-interruption watchdog (see §5) |

The defaults are defined as constants in `crates/nexus-wasm-host/src/sandbox.rs`:

```rust
pub const DEFAULT_FUEL: u64 = 10_000_000;
pub const DEFAULT_MEMORY_MIB: u32 = 64;
pub const DEFAULT_WALL_TIME: Duration = Duration::from_secs(30);
```

### 4.1 Manifest overrides

If a module's `manifest.json` declares `max_fuel`, `max_memory_mib`, or
`max_wall_time_ms`, those values **tighten** the host defaults. A module
cannot request limits *greater* than the host defaults — the host uses
`min(manifest_override, host_default)` for each limit. This prevents a
malicious or buggy manifest from requesting unbounded resources.

---

## 5. Wall-time watchdog mechanism

The wall-time limit is enforced via a watchdog thread spawned per invocation.
This is necessary because wasmtime's fuel metering bounds instruction count,
not real time — an infinite loop of cheap instructions could run forever
without the epoch-interruption watchdog.

### 5.1 Mechanism

1. Before calling `compute()`, the host spawns a watchdog thread with a
   shared `Arc<AtomicBool>` cancellation flag.
2. The watchdog sleeps in 25 ms steps, checking the cancellation flag each
   cycle. If the flag is set (computation completed), the watchdog exits
   cleanly.
3. If the deadline is reached before the flag is set, the watchdog calls
   `Engine::increment_epoch()` on the host's wasmtime engine. This traps
   the running instance with `Trap::Interrupt`, which the `compute()` call
   surface maps to `ComputeError::WallTimeExceeded`.
4. After `compute()` returns (success or error), the host sets the
   cancellation flag and joins the watchdog thread.

The 25 ms step size balances responsiveness (trapping a runaway module within
~25 ms of the deadline) against CPU overhead (one wakeup per 40 Hz).

### 5.2 Cancellation safety

The watchdog uses only `Arc<AtomicBool>` for cancellation signalling — no
channels, no mutexes held across await points. The thread is joined
unconditionally after `compute()` returns, so a dropped future still cleans up
the watchdog.

---

## 6. Embedded module loading

Embedded modules are compiled from source at build time and embedded into the
host binary at compile time via `include_dir!`.

### 6.1 Build pipeline (`build.rs`)

The `build.rs` script in `crates/nexus-wasm-host/` implements a
**compile-from-source** strategy (V1.61 open design item #6, resolved):

1. For each module id in the `MODULE_IDS` array (e.g., `["basic-combat"]`):
   - Locate the module source under `modules/<id>/`.
   - Check whether the embedded artifact (`embedded-modules/<id>/<id>.wasm`)
     exists and is newer than the source.
   - If missing or stale, invoke `cargo build --release --target wasm32-unknown-unknown`
     from the module directory and copy `<id>.wasm` + `manifest.json` into
     `embedded-modules/<id>/`.
2. Emit `cargo:rerun-if-changed=` directives so incremental rebuilds only
   recompile a module when its source changes.

The `embedded-modules/` directory is **generated and gitignored** — no binary
blobs are committed. The `wasm32-unknown-unknown` target is required to build
this crate (`rustup target add wasm32-unknown-unknown`; CI installs it via
`dtolnay/rust-toolchain` with `targets:` in every Rust job).

### 6.2 `MODULE_IDS` registration

The `MODULE_IDS` array at the top of `build.rs` is the authoritative list of
embedded modules. To add a new embedded module:

1. Author the module crate under `modules/<id>/` (see `modules/README.md`).
2. Add the module id to `MODULE_IDS` in `build.rs`.
3. Rebuild: `cargo build -p nexus-wasm-host`.

### 6.3 Runtime loading

At daemon boot, the host reads all embedded modules from `include_dir!` and
compiles them into the module cache (§2.2). Embedded modules are identified by
the `MODULE_IDS` constant and are always available — they do not depend on the
user's filesystem.

---

## 7. User module discovery

The host scans `~/.nexus42/modules/` at daemon boot for user-installed compute
modules. The scan is a single-level directory walk: each subdirectory is expected
to contain `<id>.wasm` and `manifest.json`.

### 7.1 Scan rules

1. List subdirectories of `~/.nexus42/modules/`.
2. For each subdirectory, check for `<dirname>.wasm` + `manifest.json`.
3. Parse `manifest.json`; validate required fields.
4. Compile the `.wasm` and insert into the module cache (§2.2).
5. On parse/compile failure, log a `warn!` and skip the module — individual
   failures do not block the daemon.

### 7.2 Priority

User modules with the same `module_id` as an embedded module **replace** the
embedded version. The user scan runs after embedded loading; if a user module
has the same `module_id`, its entry overwrites the embedded entry in the cache.
This lets users override or patch shipped modules.

---

## 8. Error taxonomy

All errors from the WASM compute host are variants of `ComputeError` (defined in
`crates/nexus-wasm-host/src/error.rs`). The taxonomy is organized by failure
category.

### 8.1 Module loading and compilation

| Variant | Trigger |
| --- | --- |
| `InvalidModule(String)` | The bytes are not a valid WebAssembly module. |
| `Io(std::io::Error)` | Filesystem error reading a `.wasm` or `manifest.json`. |
| `CacheWarmup(String)` | Aggregated failures during module cache warmup at boot (so a single bad module does not abort daemon start). |

### 8.2 Instantiation and linking

| Variant | Trigger |
| --- | --- |
| `MissingExport(String)` | A required module export (`memory`, `alloc`, `compute`, `init`) is missing. |
| `Wasmtime(wasmtime::Error)` | Internal wasmtime error during engine creation, instantiation, or linking. |

### 8.3 Sandbox enforcement

| Variant | Trigger |
| --- | --- |
| `OutOfFuel` | The module exhausted its fuel budget before completing. |
| `WallTimeExceeded` | The module exceeded the configured wall-time deadline (epoch-interruption watchdog). |
| `MemoryCapExceeded` | The module exceeded its memory cap (`StoreLimits`). |
| `Trap(String)` | The module trapped for any other reason (out-of-bounds access, division by zero, unreachable, etc.). |

### 8.4 Execution and output

| Variant | Trigger |
| --- | --- |
| `ModuleComputeFailed(i64)` | The module's `compute` export returned a negative status code (`-1` = generic error). |
| `OutputBufferTooSmall(usize)` | The module's `compute` export returned `-2` (output buffer too small). |
| `InvalidOutput(String)` | The bytes returned by the module are not valid UTF-8 or valid JSON. |
| `OutputSchemaMismatch(String)` | The deserialized output does not match the `ComputeOutput` envelope schema. |
| `MemoryAccess(wasmtime::MemoryAccessError)` | The host could not read/write the instance's linear memory. |
| `Json(serde_json::Error)` | A JSON (de)serialization error on the host side. |

### 8.5 Manifest validation (V1.62 P1 — NEW)

| Variant | Trigger |
| --- | --- |
| `ManifestValidationFailed { path, detail }` | Host-side validation against a `manifest.json` `schemas` block failed. `path` is a JSON path into the validated value (e.g., `$.key_blocks[1].body.attributes`). `detail` is a human-readable message. |

This variant is added by V1.62 P1 (see `compute-module-abi.md` §7.3).
Validation occurs at three points:
- **Before invocation**: each KeyBlock in `ComputeInput.key_blocks` is validated
  against `schemas.key_block_attributes[block_type]` and
  `schemas.key_block_state[block_type]` if declared.
  Also validates `ComputeInput.invocation` against `schemas.invocation` if declared.
- **After invocation**: `ComputeOutput.battle_report` is validated against
  `schemas.battle_report` if declared.
Validation is fail-fast: the first error stops the invocation and returns
`ManifestValidationFailed`.

---

## 9. Host function implementation

The host whitelists two imported host functions (module namespace `nexus`).
These are registered on a wasmtime `Linker<InvocationState>` based on the
module's `manifest.json` `host_functions` field. A module that imports a
function the host did not register fails instantiation — the whitelist is
explicitly enforced.

### 9.1 `kb_read`

```text
nexus::kb_read(id_ptr: u32, id_len: u32, out_ptr: u32, out_cap: u32) -> i64
```

Implementation (in `crates/nexus-wasm-host/src/host.rs`):

1. Read `id_len` bytes from the instance's linear memory at `[id_ptr, id_ptr+id_len)`.
2. Parse as UTF-8 → `id_str`.
3. Look up `id_str` in `InvocationState.ctx.key_blocks` (a `HashMap<String, serde_json::Value>`
   built from the `ComputeInput.key_blocks` array at invocation start).
4. If found, serialize the `KeyBlock` JSON and write it to
   `[out_ptr, out_ptr+written)`; return `written`.
5. If not found, return `-1`. If `out_cap` too small, return `-2`.

KeyBlocks are indexed by ID at invocation start for O(1) lookup. The snapshot is
immutable for the duration of the call — the host function reads from the
pre-built `HostContext`, not from a live database.

### 9.2 `narrative_query`

```text
nexus::narrative_query(q_ptr: u32, q_len: u32, out_ptr: u32, out_cap: u32) -> i64
```

Implementation:

1. Read `q_len` bytes from `[q_ptr, q_ptr+q_len)`.
2. Parse as JSON → `query`.
3. In V1, ignore the query and return `InvocationState.ctx.narrative_state` verbatim.
   A richer query engine is planned for a later iteration.
4. Serialize the response and write to `[out_ptr, out_ptr+written)`; return `written`.
5. On errors, return `-1`. If `out_cap` too small, return `-2`.

The `HostContext` is built from `ComputeInput` at invocation start (see
`HostContext::from_input()` in `host.rs`). Both host functions read from the
same immutable snapshot, ensuring deterministic behavior across calls.

### 9.3 Memory-buffer ABI

Both host functions follow the same memory convention (see
`compute-module-abi.md` §6.4). The module allocates its own output buffer via
`alloc()` and passes `(out_ptr, out_cap)`. The host writes the UTF-8 JSON
response into `[out_ptr, out_ptr+written)`.

---

*Normative Master. V1.62 P2 (2026-06-23). Source material: V1.61 compass grill
decisions Q1/Q6/Q10, `crates/nexus-wasm-host/AGENTS.md`,
`crates/nexus-wasm-host/src/{engine,sandbox,host,error}.rs`,
`modules/README.md`. See `compute-module-abi.md` for the module-side ABI
contract; `orchestration-engine.md` §8 for the `narrative.compute` capability
that calls this crate.*
