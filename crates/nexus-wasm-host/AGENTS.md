# nexus-wasm-host — Sandboxed WASM Compute Host

Hosts **WASM compute modules** in a sandboxed [wasmtime](https://wasmtime.dev/)
runtime (V1.61 "Programmable Narrative Progression"). Compute modules are
stateless pure functions: they receive a `ComputeInput` envelope, run inside a
per-invocation sandboxed instance, and return a `ComputeOutput` 4-part envelope
(`state_delta`, `timeline_events`, `new_key_blocks`, `battle_report`).

This crate is the **runtime** that P3 (`narrative.compute` capability) calls
into. It does **not** wire itself into the daemon — that is P-last.

## Architecture (compass grill decisions)

| Decision | Resolution |
| --- | --- |
| Q1 Runtime | `wasmtime` (Bytecode Alliance) |
| Q6 Sandbox | **Per-invocation sandbox.** Stateless pure function. Fresh instance per `compute()` call. |
| Q6 Limits | **Fuel** (default 10M instructions) + **memory cap** (default 64 MiB via `ResourceLimiter`) + **wall-time** (default 30s via epoch-interruption watchdog). |
| Q8 Output | Standard 4-part envelope from `schemas/compute/`. |

## Module ABI (V1 envelope)

A compute module targets `wasm32-unknown-unknown` (no WASI required) and exports:

| Export | Signature | Purpose |
| --- | --- | --- |
| `alloc` | `(len: u32) -> u32` | Allocate `len` bytes in linear memory; return pointer. Lets the host place input JSON and reserve output space inside the module. |
| `init` | `() -> ()` | Optional one-shot setup (called once after instantiation if present). |
| `compute` | `(in_ptr: u32, in_len: u32, out_ptr: u32, out_cap: u32) -> i64` | Read `ComputeInput` JSON from `[in_ptr, in_ptr+in_len)`, compute, write `ComputeOutput` JSON to `[out_ptr, out_ptr+written)`, return `written`. Return `< 0` on error (`-1` = generic error, `-2` = output buffer too small). |

The host whitelists two imported host functions (module namespace `nexus`):

| Import | Signature | Behavior |
| --- | --- | --- |
| `nexus::kb_read` | `(id_ptr, id_len, out_ptr, out_cap) -> i64` | Look up a KeyBlock by ID in the invocation's `key_blocks` snapshot; write its JSON to `out`. Returns bytes written, `-1` if not found, `-2` if `out_cap` too small. |
| `nexus::narrative_query` | `(q_ptr, q_len, out_ptr, out_cap) -> i64` | Return narrative context JSON (V1: passes through `narrative_state` from the envelope; full query engine is a later iteration). Same return convention. |

## Key Rules

- **Contracts-first**: `ComputeInput` / `ComputeOutput` / `KeyBlock` / `TimelineEvent`
  come from `nexus-contracts` (generated). Do not hand-write duplicate DTOs.
- **No cross-call state**: each `compute()` builds a fresh `Store` + `Instance`.
  Never cache instance state across calls.
- **Embedded modules are committed binaries**: the `.wasm` blobs under
  `embedded-modules/` are built from `modules/` and committed. `build.rs` is a
  guard that asserts they exist; it does **not** compile WASM (keeps the host
  crate hermetic — no wasm toolchain required to build `nexus-wasm-host`).
  Rebuild procedure: see `modules/README.md`.
- **Sandbox limits are non-negotiable**: a module that exhausts fuel, exceeds the
  memory cap, or runs past the wall-time deadline traps and is reported as a
  `ComputeError`, never crashing the host.

## Dependencies

- `nexus-contracts` (generated wire types)
- `wasmtime` (runtime), `serde`/`serde_json`, `thiserror`, `include_dir`
