---
module: modules/nexus-module-sdk + nexus-module-manifest + nexus-module-test
date: 2026-08-21
problem_type: architecture_pattern
category: engineering
severity: high
plan_id: 2026-08-20-v1.170-p0-computable-dx-spine
applies_when: [evolving the official module SDK surface, adding SDK types or accessor helpers, adding validation rules to the manifest contract, adding a host import, authoring new module crates, extending the drift-guard tooling]
tags: [wasm-sdk, compute-module, abi, drift-guard, manifest, nexus-entry, golden-fixtures, byte-compat]
---

# Compute Module SDK Authoring Pattern

How the official WASM module SDK (`nexus-module-sdk` + `nexus-module-manifest` + `nexus-module-test`, V1.170 P0) is designed so third-party module authors write zero ABI code and the SDK drifts neither from the wire nor from the host. The normative ABI contract lives in `.mstar/specs/compute-module-abi.md` — this doc captures the **authoring pattern**, not the wire spec.

## Context

Before V1.170, the only compute module (`modules/basic-combat`) hand-wrote the ABI exports (`alloc`/`init`/`compute` with `#[no_mangle]`), the global allocator, the manifest structs, and the accessor helpers — every line a third-party author would have to copy. The pivot-D goal: an official SDK where module authors write a single `fn` and one macro call, with DR-49 (ABI V2) able to land additively, and a machine-checkable guarantee that SDK mirror types track the wire.

## Guidance

### 1. Export surface: `macro_rules!`, never a proc-macro (yet)

`nexus_entry!(my_compute)` is a `macro_rules!` that expands to the three ABI exports delegating to `shim::*`:

```rust
nexus_module_sdk::nexus_entry!(my_compute);
// expands to #[no_mangle] alloc / init / compute shims calling
// shim::{alloc, init, compute(in_ptr, in_len, out_ptr, out_cap, my_compute)}
```

- Module authors write **zero** `#[no_mangle]` code. The `#[nexus::module]` attribute macro is a documented phase-2 nicety — do not reach for a proc-macro before the `macro_rules!` surface has proven its ergonomics.
- `shim::alloc` keeps the intentional-leak semantics (fresh per-invocation instance; leak the `Vec`, never free — same semantics as the pre-SDK host contract).
- Global allocator wiring is `#[cfg(target_arch = "wasm32")]`-gated (`dlmalloc::GlobalDlmalloc`); host-target builds keep std's allocator so SDK unit tests run on the host.

### 2. Trait entry so ABI V2 lands additively (DR-49)

```rust
pub trait NexusModule {
    fn compute(&self, input: ComputeInput) -> Result<ComputeOutput, ModuleError>;
    fn init(&mut self) {}   // default no-op
}
// blanket impl for fn(ComputeInput) -> Result<ComputeOutput, ModuleError> — V1's only entry form
```

`shim::compute` is generic over `N: NexusModule`. DR-49 posture: V2 adds optional trait methods / a context argument **additively** (ABI §9.1); V1 modules keep compiling. The SDK's manifest `validate()` pins `nexus_abi_version == 1` — the SDK **refuses V2 concepts** until DR-49 lands. Do not design V2 items early (composition, CDN/signing belong to DF-03/DR-49 tracks).

### 3. Typed envelope skeleton + `serde_json::Value` passthrough (drift-surface minimization)

Mirror **only** the small, stable, typed parts of the wire envelope; passthrough the high-churn opaque parts as `serde_json::Value`:

| Part | Representation | Why |
| --- | --- | --- |
| `schema_version: u32` | typed | stable scalar |
| `world_ref: WorldRef` (3 `Option<String>` fields) | typed | stable envelope shape |
| `state_delta: Vec<StateDeltaOp>` (`op`/`path`/`target_key_block_id`/`value`) | typed | the SDK's real payload |
| `key_blocks`, `narrative_state`, `invocation` (input); `timeline_events`, `new_key_blocks`, `battle_report` (output) | `serde_json::Value` | opaque to the SDK; high-churn shapes (spoke `KnowledgeEntry`, module-declared bodies) |

This shrinks the drift surface to ~2 small structs + one delta-op enum. **No `#[serde(deny_unknown_fields)]`** on any SDK type: the additive versioning policy means the host may add envelope fields under ABI 1 and the SDK must ignore unknowns — the mirror-gap drift check (§5) is what catches an addition the SDK has not yet mirrored.

Accessor helpers are extracted into a `key_blocks` module, generalized by kind with legacy fallbacks (`entry_id_of`: `entry_id` canonical / `key_block_id` fallback; `is_kind`: `entry_type` / `block_type`; `read_attr_int`: flat-object and spoke ERC721-array forms, int and f64-backed values).

### 4. Wire-required vs lock-assumed (the WorldRef lesson)

The lock spec initially wrote `world_ref` as a struct of strict `String` fields. The real wire says otherwise: `compute-input.schema.json` requires the `world_ref` **object** but **none of its properties**, and the host's generated `ComputeInputWorldRef` is all-`Option` — the `killing_blow_marks_defender_not_alive` fixture sends `{"world_id": …}` only. Strict `String` fields would deserialize-fail (`InputMalformed`, `-1`) where the real host succeeds, breaking byte-compat.

**Rule:** validate SDK mirror types against the host's **generated** types and the **real-host fixtures**, not just the schema prose — a schema-level `required` array on an object says nothing about its properties' optionality. Lock-assumed strictness the wire does not enforce is a byte-compat bug waiting to happen. (Resolution: `Option<String>` + `#[serde(default)]` on all three fields — AR-3 amendment.)

### 5. Mirror types + three-layer drift guard

The mirror strategy is hand-maintained types + a drift guard (typify-codegen-published and `nexus-contracts`-features variants were rejected: typify shape leakage already cost a consumer fix wave in V1.138; publishing `nexus-contracts` turns an internal crate into a public semver surface). The codegen path stays the **named graduation path** when modules > ~5 or DR-49 grows the envelope — the SDK keeps a clean `types/` module for codegen to overwrite.

Three layers, each machine-checked:

1. **Golden fixtures in the SDK crate** — `tests/fixtures/compute-{input,output}.golden.json` extracted from canonical envelope samples; a round-trip test deserializes → reserializes → value-compares and asserts **typed-field survival** (not just opaque round-trip).
2. **Structural mirror-gap gate** — `tooling/check-module-sdk-drift.sh` (python3 parses the compute schemas' `properties` keys, greps the SDK mirror structs, fails when a wire field has no SDK counterpart). Registered as a third script in the existing `schema-consistency-check` CI leg — extends existing tooling, no new gate system.
3. **Behavioral parity leg** — the `module-dx` CI job compiles `basic-combat` against the current SDK and re-runs the real-host fixtures **plus** the mini-host round-trip on the same compiled artifact; the wasm round-trip is ground truth.

### 6. Validator mirroring with a shared corpus

`ModuleManifest::validate()` exists **twice** (SDK + `nexus-module-manifest`). The V1.170 P0 fix-wave lesson (see `.mstar/sdd/2026-08-20-v1.170-p0-computable-dx-spine/review/qc2.md`): `compute validate` ran only `manifest.validate()` while `build`/`install` also ran the path-safety guard (`validate_run_id_safe` on `module_id`) — a manifest with `module_id: "../evil"` reported `valid: true`. **Rule:** every validation rule must live at (a) the CLI command guard, (b) `ModuleManifest::validate()` in `nexus-module-manifest`, and (c) the SDK mirror — defense in depth, aligned by a shared test corpus. Sandbox default constants (`DEFAULT_FUEL`/`DEFAULT_MEMORY_MIB`/`DEFAULT_WALL_TIME_MS`) are public SDK constants so manifest generators and validators agree with the host by construction.

### 7. Host-import wrappers and sentinel mapping

- Raw declarations via `#[link(wasm_import_module = "nexus")] extern "C"` for exactly the two whitelisted imports (`kb_read`, `narrative_query`). Importing any other `nexus::*` function fails instantiation on the real host — the SDK must not make that state easy to reach.
- Safe wrappers allocate the out buffer via the module's own `alloc`, pass `(ptr, len, out_ptr, out_cap)`, and map the return: `>= 0` → bytes written; `-1` → `NotFound`; `-2` → `OutputTooSmall`; any other negative → `Unknown(ret)`.
- Hardening: cap a positive host return at `out_cap` (`checked_written`) **before** `from_raw_parts` — a buggy host claiming more bytes than the allocation must surface as `OutputTooSmall`, not a slice past the leak.
- `ModuleError::to_compute_return()` maps `-1` for every variant except `OutputTooSmall` → `-2` (the host's `ModuleComputeFailed(-1)` / `OutputBufferTooSmall` mapping).

### 8. Mini-host honesty boundary

`nexus-module-test` implements the ABI conformance scope (exports table + the two whitelisted imports against fixture-provided snapshots, `-1`/`-2` sentinel convention) and **documentedly does not** enforce fuel / memory-cap / wall-time — those are `nexus-wasm-host` sandbox duties. The mini-host validates ABI correctness, not sandbox safety; its canonical fixture is the SSOT, drift-checked value-identical against the real-host test's inline JSON.

## Why This Matters

- The SDK is a **public surface** once published: drift in the mirror types breaks every module silently (wrong deserialization, sentinel misreads), and the drift guard is the only thing between a schema evolution and a broken authoring ecosystem.
- The two recurring bug classes this pattern kills: (a) lock-assumed wire strictness that the real host does not enforce (§4), and (b) validation gaps where one entry point checks a rule the others skip (§6).
- The trait entry + `nexus_abi_version` pin is the whole DR-49 strategy — get it wrong and ABI V2 becomes a breaking-change migration instead of an additive release.

## When to Apply

- Extending the SDK envelope (mirror a new wire field: update types + golden fixtures + drift script).
- Adding a validation rule to the manifest contract (all three guard sites + shared corpus).
- Adding a host import (whitelist is ABI §3 — extend deliberately, never opportunistically).
- Writing a new module crate (consume the SDK; never hand-write exports).
- DR-49 V2 design (additive trait methods only; bump the pin deliberately).

## Examples

### Before — lock-assumed WorldRef (broken byte-compat)

```rust
// SDK types.rs as first locked — strict fields
pub struct WorldRef { pub world_id: String, /* ... */ }
// real host succeeds on {"world_ref": {"world_id": "w1"}}, SDK returns InputMalformed (-1)
```

### After — wire-required

```rust
pub struct WorldRef {
    #[serde(default)] pub world_id: Option<String>,
    #[serde(default)] pub branch_id: Option<String>,
    #[serde(default)] pub timeline_head_event_id: Option<String>,
}
// regression: compute_input_world_ref_fields_are_optional (killing-blow fixture shape)
```

### Before — validate-only bypass

```rust
// cmd_validate ran ModuleManifest::validate() only; "../evil" module_id → valid: true
```

### After — every guard site

```rust
// cmd_validate: validate_run_id_safe(&manifest.module_id) after manifest.validate()
// BOTH ModuleManifest::validate() impls mirror the guard (SDK + nexus-module-manifest)
// shared corpus keeps them aligned
```

## References

- Normative ABI: `.mstar/specs/compute-module-abi.md` (ABI §6.3 sentinels, §7 manifest contract, §9.1 versioning)
- Manifest-hash gotcha: [../best-practices/embedded-pinned-wasm-sha256-alignment.md](../best-practices/embedded-pinned-wasm-sha256-alignment.md)
- Consumption side (daemon route + Runs): [../architecture-patterns/compute-pillar-invoke-and-runs-history.md](../architecture-patterns/compute-pillar-invoke-and-runs-history.md)
- Crate topology: [standalone-crate-monorepo-topology.md](standalone-crate-monorepo-topology.md)
- Iteration spec: `.mstar/iterations/v1.170/specs/v1.170-computable-dx-locks.md` AR-2..AR-12
