# Nexus Compute Modules

Nexus compute modules are **WebAssembly** modules that run narrative "compute"
steps — combat resolution, economy ticks, AI decisions — inside a sandboxed
[`wasmtime`](https://wasmtime.dev/) host. They are **stateless pure functions**:
each call receives a fresh `ComputeInput` envelope and returns a 4-part
`ComputeOutput` envelope. This directory holds their **source**; compiled
`.wasm` artifacts are embedded into the `nexus-wasm-host` crate (see
[Embedding a module](#embedding-a-module)).

> Spec context: V1.61 "Programmable Narrative Progression" — see
> `.mstar/iterations/v1.61-programmable-narrative-progression-delivery-compass-v1.md`
> (grill decisions Q2/Q6/Q8/Q9/Q10) and `schemas/compute/` for the wire contracts.

## The V1 ABI at a glance

A module targets **`wasm32-unknown-unknown`** (no WASI required) and exports:

| Export | Signature | Required | Purpose |
| --- | --- | --- | --- |
| `memory` | exported linear memory | yes | Host reads/writes JSON buffers here. |
| `alloc` | `(len: u32) -> u32` | yes | Allocate `len` bytes in linear memory; return pointer. Lets the host place input JSON and reserve output space inside the module. |
| `compute` | `(in_ptr: u32, in_len: u32, out_ptr: u32, out_cap: u32) -> i64` | yes | Read `ComputeInput` JSON from `[in_ptr, in_ptr+in_len)`, compute, write `ComputeOutput` JSON to `[out_ptr, out_ptr+written)`, return `written`. Negative sentinels: `-1` = error, `-2` = output buffer too small. |
| `init` | `() -> ()` | optional | One-shot setup, called once after instantiation if declared in the manifest. |

The host whitelists up to two **imported host functions** (module namespace
`nexus`). Importing one the host did not register fails instantiation — the
explicit enforcement of the manifest's `host_functions` list.

| Import | Signature | Behavior |
| --- | --- | --- |
| `nexus::kb_read` | `(id_ptr, id_len, out_ptr, out_cap) -> i64` | Look up a KeyBlock by ID in the invocation's `key_blocks` snapshot; write its JSON to `out`. Returns bytes written, `-1` if not found, `-2` if `out_cap` too small. |
| `nexus::narrative_query` | `(q_ptr, q_len, out_ptr, out_cap) -> i64` | Return narrative context JSON (V1: passes through `narrative_state` from the envelope). Same return convention. |

**Canonical data path:** the host always bundles the relevant KeyBlocks into
`ComputeInput.key_blocks` (the schema makes this array required). Most modules —
including the sample `basic-combat` — read combatants straight from that inline
snapshot and do not need the host imports at all. Use `kb_read` /
`narrative_query` only when a module needs to look up *additional* blocks or
narrative context beyond what the host pre-selected.

## `manifest.json`

Every module ships a `manifest.json` next to its `.wasm`. It declares identity,
the required input surface, the export names, and optional sandbox overrides.

### Required fields

| Field | Type | Meaning |
| --- | --- | --- |
| `module_id` | string | Unique module id (matches the directory name). |
| `name` | string | Human-readable name. |
| `version` | string | Module SemVer (independent of the Nexus ABI version). |
| `nexus_abi_version` | integer | Compute envelope ABI version (`1` for V1.61). |
| `required_key_block_types` | array&lt;string&gt; | BlockTypes the module reads (e.g. `["character"]`). The host uses this to select which KeyBlocks to bundle into `ComputeInput`. |
| `compute_export` | string | Name of the WASM export implementing `compute`. |
| `init_export` | string | Name of the WASM export implementing `init` (empty string if none). |

### Optional fields

| Field | Type | Default | Meaning |
| --- | --- | --- | --- |
| `description` | string | — | Free-form description. |
| `author` | string | — | Author attribution. |
| `host_functions` | array&lt;string&gt; | `[]` | Subset of `["kb_read", "narrative_query"]` the module may call. |
| `battle_report_kind` | string | module-declared | Discriminator the module emits in `battle_report.kind`. |
| `max_fuel` | integer | host `SandboxConfig` | Per-invocation fuel override. |
| `max_memory_mib` | integer | host `SandboxConfig` | Per-invocation memory-cap override (MiB). |
| `max_wall_time_ms` | integer | host `SandboxConfig` | Per-invocation wall-time override (ms). |
| `schemas` | object | — | **V1.62+**: Inline JSON-Schema fragments for per-module input/output validation (see [The `schemas` block](#the-schemas-block-v162)). Omit for no validation (backward-compatible with V1.61). |

### The `schemas` block (V1.62+)

Example (`basic-combat/manifest.json`):

```json
{
  "module_id": "basic-combat",
  "name": "Basic Combat",
  "version": "1.0.0",
  "nexus_abi_version": 1,
  "required_key_block_types": ["character"],
  "compute_export": "compute",
  "init_export": "init",
  "host_functions": [],
  "battle_report_kind": "combat"
}
```

#### The `schemas` block (V1.62+)

V1.62 adds an optional `schemas` field to `manifest.json`. When declared, the
host validates the module's input and output against inline JSON-Schema
fragments **before** and **after** each `compute()` call. This keeps per-module
shape declarations self-contained in the module's own manifest (rather than
centralized in product-level schemas).

The `schemas` block contains four optional sub-fields:

| Field | Validates | When |
| --- | --- | --- |
| `key_block_attributes` | `key_blocks[i].body.attributes` per `block_type` | Pre-invocation |
| `key_block_state` | `key_blocks[i].body.state.<block_type>` per `block_type` | Pre-invocation |
| `invocation` | `ComputeInput.invocation` (when non-null) | Pre-invocation |
| `battle_report` | `ComputeOutput.battle_report` | Post-invocation |

Each sub-field is a JSON-Schema object (fragment). The host validates only
the sub-fields that are declared. If a sub-field is omitted or the entire
`schemas` block is absent, **no validation** is performed for that aspect —
manifests without `schemas` continue to work as in V1.61.

**Validation failure**: on the first mismatch, the host returns
`ComputeError::ManifestValidationFailed` with a JSON path (e.g.
`key_blocks[1].body.attributes.base_atk: missing required field`) and
aborts the invocation immediately. Only the first error is reported
(fail-fast).

The host supports a minimal JSON-Schema subset:
`type`, `properties`, `required`, `additionalProperties`, `minimum`, `items`,
`const`. This covers the needs of module input/output validation without
pulling in a full validator.

**Authoring example** (`basic-combat/manifest.json` schemas block):

```json
{
  "schemas": {
    "key_block_attributes": {
      "character": {
        "type": "object",
        "properties": {
          "max_hp": {"type": "integer", "minimum": 0},
          "base_atk": {"type": "integer", "minimum": 0},
          "base_def": {"type": "integer", "minimum": 0},
          "speed": {"type": "integer", "minimum": 0},
          "level": {"type": "integer", "minimum": 1}
        },
        "required": ["max_hp", "base_atk", "base_def"],
        "additionalProperties": true
      }
    },
    "key_block_state": {
      "character": {
        "type": "object",
        "properties": {
          "current_hp": {"type": "integer", "minimum": 0},
          "status_effects": {"type": "array", "items": {"type": "string"}},
          "is_alive": {"type": "boolean"}
        },
        "additionalProperties": true
      }
    },
    "invocation": {
      "type": "object",
      "properties": {
        "attacker_id": {"type": "string"},
        "defender_id": {"type": "string"}
      },
      "additionalProperties": true
    },
    "battle_report": {
      "type": "object",
      "properties": {
        "kind": {"type": "string", "const": "combat"},
        "attacker_id": {"type": "string"},
        "defender_id": {"type": "string"},
        "damage": {"type": "integer"},
        "defender_hp_before": {"type": "integer"},
        "defender_hp_after": {"type": "integer"}
      },
      "required": ["kind", "attacker_id", "defender_id", "damage"],
      "additionalProperties": true
    }
  }
}
```

## The 4-part `ComputeOutput` envelope

A module's `compute` must emit a JSON object with exactly these top-level keys
(see `schemas/compute/compute-output.schema.json`):

```json
{
  "schema_version": 1,
  "state_delta": [
    { "op": "sub", "path": "character.current_hp",
      "target_key_block_id": "kb-def", "value": 15 }
  ],
  "timeline_events": [ { "...": "a TimelineEvent object (event_type=state_update)" } ],
  "new_key_blocks": [],
  "battle_report": { "kind": "combat", "...": "module-declared fields" }
}
```

- `state_delta` — ordered `add` / `sub` / `set` operations on nested state paths
  of computable KeyBlock bodies (compass Q5: `state.character.current_hp`).
- `timeline_events` — events to append (V1.60 `timeline.event.append`); use
  `event_type: "state_update"`, `status: "canon"` for compute outcomes. Valid
  enum values come from `schemas/domain/`.
- `new_key_blocks` — new blocks the module creates (e.g. a spawned item).
- `battle_report` — module-declared freeform report; `kind` discriminates the
  payload. The host applies deltas/blocks/events, then surfaces this report.

> Modules cannot read a wall clock on `wasm32-unknown-unknown`; emit a
> placeholder `created_at` and let the host stamp authoritative timestamps when
> it applies the event.

## Writing a module (Rust)

1. Create `modules/<your-module>/` with its own `Cargo.toml` (it is **not** a
   workspace member — give it an empty `[workspace]` table so `cargo` treats it
   as a standalone root). Use `crate-type = ["cdylib"]` and a global allocator
   (`std` is available on `wasm32-unknown-unknown`; only I/O, threads, and the
   wall clock are absent).

   ```toml
   [package]
   name = "my-module"
   version = "0.1.0"
   edition = "2021"
   publish = false

   [lib]
   crate-type = ["cdylib"]

   [dependencies]
   serde = { version = "1", features = ["derive"] }
   serde_json = "1"
   dlmalloc = { version = "0.2", features = ["global"] }

   [profile.release]
   opt-level = "z"
   lto = true
   codegen-units = 1
   panic = "abort"
   strip = true

   [workspace]  # standalone: not part of the nexus workspace
   ```

2. Export `alloc`, `init` (optional), and `compute`. See
   [`basic-combat/src/lib.rs`](basic-combat/src/lib.rs) for a complete reference
   implementation — copy its `alloc` and `compute` marshalling and replace the
   combat logic with yours.

3. Add a `manifest.json` (see above).

4. Build: from `modules/<your-module>/` —

   ```bash
   cargo build --release --target wasm32-unknown-unknown
   ```

   (You need the target installed: `rustup target add wasm32-unknown-unknown`.)

## Embedding a module

Compiled `.wasm` artifacts live under
`crates/nexus-wasm-host/embedded-modules/<id>/` and are embedded into the host
crate at compile time via `include_dir!` (compass Q10). The
`embedded-modules/` tree is **generated and gitignored** — those artifacts are
**compiled from source by `crates/nexus-wasm-host/build.rs`**, not committed.

This is a **compile-from-source** strategy (open design item #6, resolved): the
build script invokes `cargo build --release --target wasm32-unknown-unknown`
for each registered module id whenever its embedded copy is missing or older
than its source. This keeps `cargo build -p nexus-wasm-host` reproducible
without committing binary blobs; the only extra requirement is the
`wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`,
installed automatically in CI via `dtolnay/rust-toolchain` `targets:`).

To add a new embedded module:

1. Author the module crate under `modules/<id>/` (see
   [Writing a module](#writing-a-module-rust)) and add a `manifest.json`.

2. Register the module id in the `MODULE_IDS` array at the top of
   `crates/nexus-wasm-host/build.rs`.

3. Build the workspace (or just the host crate):

   ```bash
   cargo build -p nexus-wasm-host
   ```

   `build.rs` compiles the module, stages `<id>.wasm` + `manifest.json` into
   `embedded-modules/<id>/`, and emits `cargo:rerun-if-changed=` directives so
   incremental rebuilds only recompile a module when its source changes. If a
   module dir is missing its `<id>.wasm` or `manifest.json` after the build
   script runs, the build fails with a clear message.

To update an existing embedded module, edit its source under `modules/<id>/`
and rebuild — `build.rs` detects the newer mtime and recompiles automatically.

## Sandbox guarantees (compass Q6)

Each `compute()` call runs in a **fresh, isolated instance** with:

- **Fuel** — default 10M instructions; traps with `OutOfFuel` when depleted.
- **Memory cap** — default 64 MiB (via wasmtime `StoreLimits`).
- **Wall-time** — default 30s, enforced via epoch interruption (a watchdog
  thread bumps the epoch after the deadline, trapping the instance).

A module that breaches any limit traps and is reported as a `ComputeError`; it
never crashes the host. Manifest overrides (`max_fuel`, `max_memory_mib`,
`max_wall_time_ms`) tighten the defaults per module.

## Reference

- Sample module: [`basic-combat/`](basic-combat/) — simple ATK−DEF resolution.
- Host crate: [`crates/nexus-wasm-host/`](../crates/nexus-wasm-host/) — engine,
  sandbox, host-function ABI, embedded-module loader.
- Wire contracts: [`schemas/compute/`](../schemas/compute/).
