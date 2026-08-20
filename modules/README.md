# Nexus Compute Modules

Nexus compute modules are **WebAssembly** modules that settle narrative
"compute" steps — combat resolution, economy ticks, dice rolls, rule checks —
inside a sandboxed [`wasmtime`](https://wasmtime.dev/) host. They are
**stateless pure functions**: each call receives a fresh `ComputeInput`
envelope and returns a 4-part `ComputeOutput` envelope (state deltas,
timeline events, new key blocks, battle report). This directory holds their
**source**.

> Spec context: the normative ABI contract is
> [`.mstar/specs/compute-module-abi.md`](../.mstar/specs/compute-module-abi.md);
> the integrator-facing authoring guide is
> [`docs/module-authoring.md`](../docs/module-authoring.md).

## Authoring is SDK-first

The official [`nexus-module-sdk`](nexus-module-sdk/) owns **every** ABI-facing
symbol: `nexus_entry!` generates the three exports (`alloc`, `init`,
`compute`) and wires the global allocator, and the typed envelope skeleton +
key-block accessors + host-import wrappers + manifest helper cover the full V1
surface. A module is a plain Rust function plus a `manifest.json` — zero
`#[no_mangle]` code, no hand-copied marshalling:

```rust
use nexus_module_sdk::{nexus_entry, ComputeInput, ComputeOutput, ModuleError};

fn my_compute(input: ComputeInput) -> Result<ComputeOutput, ModuleError> {
    // ... module logic ...
}

nexus_entry!(my_compute);
```

The [`nexus42` CLI `compute` group](../apps/nexus42/) turns the authoring loop
into commands: `build` / `validate` / `install` are **daemon-free** (the
author loop needs no runtime); `run` is the one daemon-backed command.

> **Guided authoring:** use the `compute-module-author` skill from the
> [`42ch-dev/agent-toolkit`](https://github.com/42ch-dev/agent-toolkit)
> repository (external — **no** agent skill ships in this repo).

## Quick start (scaffold → build → validate → install → run)

1. **Scaffold** — copy the template module (`modules/_template/`, an SDK
   hello-world "dice tick") and rename it:

   ```bash
   cp -R modules/_template modules/my-mod
   ```

   Adapt three things:

   - `manifest.json` — set `module_id` (e.g. `my-mod`), `name`, and
     `required_key_block_types` to the block types your module reads.
   - `Cargo.toml` — set `[package] name`; the compiled artifact is named
     after the crate (e.g. crate `my-mod` → `my_mod.wasm`).
   - `src/lib.rs` — replace the demo logic with yours.

2. **Build** (daemon-free) — compiles the wasm, stages the pair under
   `<module-dir>/dist/<module_id>/`, and injects `wasm_sha256` into the staged
   manifest (the source manifest is never mutated):

   ```bash
   nexus42 compute build --manifest modules/my-mod/manifest.json --release
   ```

3. **Validate** (daemon-free) — exit 0 on a valid manifest; add `--wasm` to
   also verify the `wasm_sha256` pairing against the compiled bytes:

   ```bash
   nexus42 compute validate --manifest modules/my-mod/manifest.json
   nexus42 compute validate --manifest modules/my-mod/manifest.json \
     --wasm modules/my-mod/dist/my-mod/my-mod.wasm
   ```

4. **Install** (daemon-free) — re-verifies pairing, then copies the pair into
   `~/.nexus42/modules/<id>/` (`<id>/<id>.wasm` + `<id>/manifest.json` — the
   exact pair the daemon's module cache scans at boot):

   ```bash
   nexus42 compute install --module-id my-mod \
     --manifest modules/my-mod/dist/my-mod/manifest.json \
     --wasm modules/my-mod/dist/my-mod/my-mod.wasm
   ```

5. **Run** (daemon-backed) — thin client over
   `POST /v1/daemon/compute/run`; `--accept` additionally posts
   `/v1/daemon/compute/runs/:run_id/accept` to apply the run's proposals:

   ```bash
   nexus42 compute run --world <world-id> --input input.json \
     --module-id my-mod [--accept]
   ```

   The input is a `ComputeInput` envelope (its `invocation` field is sent) or
   a raw `invocation_params` object. Requires a running `nexus42` daemon.
   Output format follows the CLI-wide `--output text|json` flag.

The CLI exit-code vocabulary (AR-9): `0` success · `1` build/toolchain or
install I/O failure · `2` manifest validation failure · `3` `wasm_sha256`
pairing mismatch · `4` daemon unreachable / run rejected.

## The SDK at a glance

| Piece | What it gives you |
| --- | --- |
| `nexus_entry!(my_compute)` | The three ABI exports (`alloc`, `init`, `compute`) + global allocator wiring (dlmalloc, wasm-target only). |
| `ComputeInput` / `ComputeOutput` | Typed envelope skeleton: `schema_version`, `world_ref` (`WorldRef`), `state_delta` (`StateDeltaOp` + `DeltaOp`); high-churn parts (`key_blocks`, `invocation`, `narrative_state`, `timeline_events`, `battle_report`) pass through as `serde_json::Value`. |
| `key_blocks` accessors | `entry_id_of`, `is_kind`, `read_attr_int`, `read_int(_f64)`, `timeline_event_id` — read the bundled key-block snapshot (spoke `entry_id`/`entry_type` with legacy `key_block_id`/`block_type` fallbacks). |
| `host` wrappers | Safe `kb_read` / `narrative_query` over the two whitelisted `nexus::` imports, with the `-1`/`-2` sentinel mapping. |
| `ModuleError` | `InputMalformed` / `SerializeFailed` / `OutputTooSmall` / `Host(..)` with `to_compute_return()` sentinel mapping (`-1` / `-2`). |
| `ModuleManifest` + `validate()` | Full manifest contract mirror + validation (pins `nexus_abi_version == 1`) + sandbox default constants (`DEFAULT_FUEL`, `DEFAULT_MEMORY_MIB`, `DEFAULT_WALL_TIME_MS`). |

## `manifest.json`

Every module ships a `manifest.json` next to its `.wasm`. It declares
identity, the required input surface, the export names, and optional sandbox
overrides.

### Required fields

| Field | Type | Meaning |
| --- | --- | --- |
| `module_id` | string | Unique module id (matches the install directory name). |
| `name` | string | Human-readable name. |
| `version` | string | Module SemVer (independent of the Nexus ABI version). |
| `nexus_abi_version` | integer | Compute envelope ABI version — **`1`** (the SDK refuses anything else). |
| `required_key_block_types` | array&lt;string&gt; | BlockTypes the module reads (e.g. `["character"]`). The host uses this to select which KnowledgeEntries to bundle into `ComputeInput`. |
| `compute_export` | string | Name of the WASM export implementing `compute`. |
| `init_export` | string | Name of the WASM export implementing `init` (empty string if none). |

### Optional fields

| Field | Type | Default | Meaning |
| --- | --- | --- | --- |
| `description` | string | — | Free-form description. |
| `author` | string | — | Author attribution. |
| `host_functions` | array&lt;string&gt; | `[]` | Subset of `["kb_read", "narrative_query"]` the module may call. |
| `battle_report_kind` | string | — | Discriminator the module emits in `battle_report.kind`. |
| `max_fuel` | integer | host default (10M) | Per-invocation fuel override. |
| `max_memory_mib` | integer | host default (64) | Per-invocation memory-cap override (MiB). |
| `max_wall_time_ms` | integer | host default (30000) | Per-invocation wall-time override (ms). |
| `wasm_sha256` | string | — | SHA-256 of the exact `.wasm` bytes this manifest pairs with (64 lowercase hex). |
| `schemas` | object | — | Inline JSON-Schema fragments — `key_block_attributes`, `key_block_state`, `invocation`, `battle_report` — the host validates before/after invocation. Omit to disable validation. |

See the template's own [`manifest.json`](_template/manifest.json) for a
minimal complete example, and
[`basic-combat/manifest.json`](basic-combat/manifest.json) for the full
`schemas` block.

### `wasm_sha256` — content-based pairing

`wasm_sha256` is the SHA-256 of the **exact `.wasm` bytes** the manifest ships
with. When present, the loader hashes the loaded bytes and rejects a mismatch
**before the pair is compiled or cached** — an old manifest + new `.wasm`
always mismatches. When absent (legacy manifests), the loader falls back to a
stat fence (size + mtime) that cannot detect a same-size swap.

**The CLI keeps the pairing honest for you:** `compute build` injects the hash
of the staged `.wasm` into the staged manifest; `compute validate --wasm` and
`compute install` verify it and exit `3` on a mismatch. Hand-installed pairs
must set it themselves (compute it with `shasum -a 256 <module>.wasm`) or
omit it to fall back to the stat fence.

## Where a module runs: `module_scope` allowlist

Compute is gated by the peer allowlist. A Connect peer may invoke a module
only when its allowlist entry's `module_scope` contains the module id —
**missing or empty `module_scope` denies ALL compute** with
`module_not_scoped`, fail-closed, before any WASM execution:

```json
{
  "peer_ids": [
    {
      "peer_id": "12D3KooW…",
      "world_scope": ["wld_…"],
      "op_scope": ["upsert", "promote", "relate", "check", "assemble", "compute"],
      "module_scope": ["basic-combat"]
    }
  ]
}
```

`module_scope` ids are **host-local module names** (`~/.nexus42/modules/`),
never peer-supplied bytes. Full allowlist mechanics:
[`docs/nexus-runtime.md`](../docs/nexus-runtime.md#allowlist-and-module-scope).

## Three invoke paths

A module you install is reachable through three lanes:

1. **Preset `narrative.compute`** — the daemon's built-in compute capability,
   invoked by name from a strategy preset lane (input
   `{world_id, creator_id, module_id, invocation_params}`). The preset stages
   the compute session, the host bundles key blocks per the module manifest,
   runs the wasm, and **applies** the result inline (state deltas, timeline
   events, new key blocks). This is the full settling path inside a preset
   run.
2. **Connect read-only** — a Connect peer names the host-local module id in a
   scoped entry's `body.computable.module_id`; the module runs **read-only**
   (`settle: true` is rejected). The confirmed receipt comes back to the
   caller, who commits it through the write path (world-aware CAS, never a
   forced overwrite) and narrates confirmed receipts only.
3. **Control Room run + accept + discard** — `POST /v1/daemon/compute/run`
   stages a run with proposals; `POST /v1/daemon/compute/runs/:run_id/accept`
   applies them atomically; `.../discard` discards them; the `GET` routes list
   and inspect runs. `nexus42 compute run [--accept]` is the CLI client for
   this lane.

## The reference module: `basic-combat`

[`basic-combat/`](basic-combat/) is the in-repo reference implementation — a
sample ATK−DEF combat resolution that doubles as the host's integration test.
It is authored on the SDK and **embedded** into `nexus-wasm-host`:
`crates/nexus-wasm-host/build.rs` compiles its source into
`embedded-modules/basic-combat/` and injects `wasm_sha256` at build time (the
staged pair is always content-consistent). To embed a new module, register its
id in the `MODULE_IDS` array at the top of `crates/nexus-wasm-host/build.rs` —
embedding is optional; operator install (above) needs no host change.

## Sandbox guarantees

Each `compute()` call runs in a **fresh, isolated instance** with:

- **Fuel** — default 10M instructions; traps with `OutOfFuel` when depleted.
- **Memory cap** — default 64 MiB (via wasmtime `StoreLimits`).
- **Wall-time** — default 30s, enforced via epoch interruption.

A module that breaches any limit traps and is reported as a `ComputeError`; it
never crashes the host. Manifest overrides (`max_fuel`, `max_memory_mib`,
`max_wall_time_ms`) tighten the defaults per module. `alloc` may leak
intentionally: the per-invocation instance is discarded right after the call.

## Reference

- Template scaffold: [`_template/`](_template/) — SDK hello-world (dice tick).
- Sample module: [`basic-combat/`](basic-combat/) — ATK−DEF combat resolution.
- SDK crate: [`nexus-module-sdk/`](nexus-module-sdk/) — the authoring surface.
- Authoring guide: [`docs/module-authoring.md`](../docs/module-authoring.md).
- ABI spec (normative):
  [`.mstar/specs/compute-module-abi.md`](../.mstar/specs/compute-module-abi.md).
- Host crate: [`crates/nexus-wasm-host/`](../crates/nexus-wasm-host/) — engine,
  sandbox, host-function ABI, embedded-module loader, registry module.
- `compute-module-author` skill:
  [`42ch-dev/agent-toolkit`](https://github.com/42ch-dev/agent-toolkit)
  (external repository).
