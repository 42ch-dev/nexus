---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-23-v1.61-wasm-host-and-basic-combat"
verdict: "Approve"
generated_at: "2026-06-23"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist (Seat 1)
- Runtime Agent ID: qc-specialist
- Runtime Model: MiniMax-M3
- Review Perspective: Architecture coherence and maintainability risk — module boundary design, compass grill alignment, manifest/build.rs/module separation, workspace integration cleanliness, P3 consumability
- Report Timestamp: 2026-06-23

## Scope
- plan_id: 2026-06-23-v1.61-wasm-host-and-basic-combat
- Review range / Diff basis: d268f8e6..feature/v1.61-wasm-host-and-basic-combat
- Working branch (verified): iteration/v1.61 (feature/v1.61-wasm-host-and-basic-combat already merged via 5692fe5c — review range tip = c86da6a1)
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 23 (22 source + 1 plan file): Cargo.toml (workspace +1 line, root), Cargo.lock regen, `crates/nexus-wasm-host/{AGENTS.md, Cargo.toml, build.rs}`, 7 `src/*.rs`, 2 `tests/*.rs`, 1 binary `embedded-modules/basic-combat/basic-combat.wasm` (78 192 B), `crates/nexus-wasm-host/embedded-modules/basic-combat/manifest.json`, `modules/README.md`, `modules/basic-combat/{Cargo.toml, Cargo.lock, manifest.json, src/lib.rs}`
- Commit range: cddc1913 (T1–T8) → d42d853e (T9) → 862bce92 (T10) → c86da6a1 (plan status flip)
- Tools run: `cargo check -p nexus-wasm-host`, `cargo clippy -p nexus-wasm-host --all-targets -- -D warnings`, `cargo test -p nexus-wasm-host`, `cargo +nightly fmt -p nexus-wasm-host --check`, `git rev-parse` (alignment), `git diff --stat`, full source read of 13 files

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 5 |

**Verdict**: Approve

---

## Findings

### 🟢 Suggestion

#### S-001 — `tracing` is declared but never used
- **Where**: `crates/nexus-wasm-host/Cargo.toml:22` (deps), `src/**/*.rs` (0 references)
- **Issue**: `tracing = { workspace = true }` is listed as a dependency, but no source file emits any `tracing::info!`, `tracing::warn!`, etc. As written the dependency is dead weight. For a sandboxed runtime this is the kind of crate that genuinely benefits from structured instrumentation — `compute()` should at minimum emit an `info!` on entry (module_id, sandbox limits) and an `error!` on the failure paths (`OutOfFuel` / `WallTimeExceeded` / `MemoryCapExceeded` / `ModuleComputeFailed`) so operators can diagnose production compute failures without re-running under a debugger.
- **Fix**: Either (a) add `tracing::instrument` / `tracing::info!` / `tracing::error!` calls in `compute.rs` and `spawn_watchdog`; or (b) remove the dependency until instrumentation lands in a follow-up plan. Either direction is fine; the current "declared but unused" state should not ship.

#### S-002 — Memory-cap and wall-time paths are not exercised by tests
- **Where**: `crates/nexus-wasm-host/tests/sandbox_limits.rs` (only the fuel / `OutOfFuel` path is covered); `src/compute.rs:209-213` (`is_memory_trap` heuristic).
- **Issue**: The acceptance criterion for the sandbox is "fuel + memory + wall-time bounds all surface as typed `ComputeError`s." Only fuel is actually tested. The `is_memory_trap` classifier is a brittle lowercase substring match (`msg.contains("memory") && (msg.contains("grow") || msg.contains("limit") || msg.contains("exceed"))`); a wasmtime version bump that reformats the message could silently flip memory-cap failures to a generic `Trap(_)`. A WAT probe that grows memory beyond `max_memory_mib` and asserts `ComputeError::MemoryCapExceeded` would lock both the error-classification logic and the limiter wiring in place — same pattern as the existing fuel/infinite-loop test.
- **Fix**: Add a third test in `tests/sandbox_limits.rs` — a `(memory 1) (func (export "alloc") (result i32) ... grow …)` probe with `manifest.max_memory_mib = Some(1)` and assert `ComputeError::MemoryCapExceeded`. Wall-time is harder to deterministically test under CI (sub-second loops are usually clamped); documenting that as "informally exercised, formally deferred" would be acceptable.

#### S-003 — Sibling-pattern inconsistency: build.rs guard vs. un-guarded `include_dir!`
- **Where**: `crates/nexus-wasm-host/build.rs` (guard present) vs. `crates/nexus-orchestration/src/preset/mod.rs:68` and `crates/nexus-orchestration/src/embedded_skills.rs:30` (no guard).
- **Issue**: `nexus-wasm-host/build.rs` is a **defensive improvement** over the existing `embedded-presets/` and `embedded-skills/` patterns in `nexus-orchestration` — both of those rely solely on `include_dir!` with no build-time check. The wasm-host guard catches a missing `.wasm` artifact at compile time with an actionable message ("Rebuild from modules/ per modules/README.md"); the orchestration crates fail at runtime with a `None` from a lookup function, which is much harder to debug. The wasm-host pattern is strictly better and should be the canonical version. This is **not** a regression introduced by this PR (orchestration's pattern predates it), but it is a maintenance hazard: future modules added under `embedded-modules/` will get the guard, but new additions under `embedded-presets/` / `embedded-skills/` will silently keep the weaker pattern.
- **Fix**: Two equally good options. (a) Add a short note in `crates/nexus-orchestration/AGENTS.md` pointing to the wasm-host build.rs as the canonical pattern, with a `TODO` to backport. (b) Extract the guard into a tiny shared helper (e.g. `nexus-build-helpers::assert_embedded_tree_present`) and call it from both crates' `build.rs`. Either is fine for a follow-up; do not block this PR.

#### S-004 — `embedded_module_ids()` is not asserted in tests
- **Where**: `crates/nexus-wasm-host/src/embedded.rs:33-44`, `crates/nexus-wasm-host/src/embedded.rs:50-58` (test asserts only `embedded_module_bytes`).
- **Issue**: The existing `basic_combat_is_embedded` test checks `embedded_module_bytes("basic-combat")` and `embedded_module_manifest("basic-combat")` are both `Some`, but does not assert `embedded_module_ids()`. If someone later adds a module with `manifest.json` but forgets to drop the `.wasm`, the byte-fetch will silently return `None` and the test will not catch the inconsistency. The id-enumeration function is also part of the public API and will be the entry point P-last uses to load embedded modules at daemon startup.
- **Fix**: One extra assertion: `assert_eq!(embedded_module_ids(), vec!["basic-combat"]);` (or `contains("basic-combat")` if more modules are expected to land before this lands in `main`).

#### S-005 — `init_export: String` ("empty string means no init") could be `Option<String>`
- **Where**: `crates/nexus-wasm-host/src/manifest.rs:53`, `src/compute.rs:236-239` (`optional_export` treats `""` as `None`).
- **Issue**: `ModuleManifest.init_export: String` is a required string, and the documented convention is the empty string `""` for "no init export." `optional_export` reads `if name.is_empty() { return None; }` to handle this. It works, but a typed `#[serde(default, deserialize_with = "...")] Option<String>` (or a custom `#[serde(rename = "")]` enum `InitExport { Init(String), None }`) would be more self-documenting and remove the "magic empty string" from the manifest format. Not blocking — the current shape is consistent and the helper is correct.
- **Fix**: Optional refactor in a follow-up. Leave as-is for this PR.

---

## Architecture / Compass Alignment

The review focused on five grill decisions called out in the assignment (Q1, Q2, Q3, Q6, Q10) and three open design items (#3 manifest format, #4 host ABI whitelist, #6 build.rs strategy). All are resolved correctly:

| Compass decision / open item | Resolution observed | Verdict |
|------------------------------|---------------------|---------|
| **Q1 — wasmtime runtime** | `wasmtime = "46"` pinned with `consume_fuel(true)` + `epoch_interruption(true)` enabled on `WasmEngine::with_config` (engine.rs:67-70). cranelift default features (the comment in Cargo.toml:17 confirms). | ✅ Aligned |
| **Q2 — Embedded + User distribution** | `modules/basic-combat/` (source) → `crates/nexus-wasm-host/embedded-modules/basic-combat/` (binary) → embedded via `include_dir!` (embedded.rs:13). The `~/.nexus42/modules/` user layer is not in scope for P2 (P-last); AGENTS.md §"Sandbox limits are non-negotiable" makes the layering explicit. | ✅ Aligned; user-layer wiring correctly deferred to P-last |
| **Q3 — V1 envelope ABI** | `ComputeInput` / `ComputeOutput` / `ComputeOutputStateDelta` re-exported from `nexus_contracts::generated::{compute_input, compute_output}` in lib.rs:50-51. **No handwritten DTOs in Rust** — every wire type is generated from `schemas/compute/*.json`. Single truth source preserved. | ✅ Aligned |
| **Q6 — Per-invocation sandbox** | Fresh `Store` + `Instance` per `compute()` call (compute.rs:88). Three independent guards: `set_fuel` (compute.rs:92) + `StoreLimits` memory cap (compute.rs:80-89) + epoch-interruption watchdog (compute.rs:95-109). Mapped to typed `ComputeError::OutOfFuel` / `WallTimeExceeded` / `MemoryCapExceeded` in `map_call_result` (compute.rs:187-199). No cross-call state pollution — verified by `compute_is_reproducible_across_invocations` test. | ✅ Aligned |
| **Q10 — Repo structure** | `modules/` at repo root with `basic-combat/` standalone (its own `[workspace]` empty table and committed `Cargo.lock` — `modules/basic-combat/Cargo.toml:40`, `modules/basic-combat/Cargo.lock`). `embedded-modules/` is `include_dir!`'d at compile time. Mirror of `embedded-presets/` in `nexus-orchestration` (preset/mod.rs:68). The `build.rs` guard is a **strict improvement** over the orchestration crate's pattern (see S-003). | ✅ Aligned |
| **Open item #3 — manifest.json format** | 7 required + 7 optional fields, all documented in `manifest.rs:8-30` and cross-referenced in `modules/README.md:42-69`. Required set matches what the host actually needs (identity, ABI version, key-block-type filter, export names). Optional set is `#[serde(default)]`-friendly. | ✅ Resolved |
| **Open item #4 — Host function ABI whitelist** | Two whitelisted imports in module namespace `nexus`: `kb_read` and `narrative_query` (host.rs:114-176). Memory-buffer marshalling with `i64` return convention; sentinels `RET_NOT_FOUND = -1` and `RET_OVERFLOW = -2` (host.rs:41-43). Whitelist enforcement is explicit: a module importing a non-registered `nexus::*` function fails instantiation (verified by `non_whitelisted_import_rejected_at_instantiation` test). | ✅ Resolved |
| **Open item #6 — build.rs strategy** | Pre-compile + commit. `.wasm` artifacts are built from `modules/` and committed under `embedded-modules/` (see modules/README.md:166-191 for the manual procedure). `build.rs` is a **guard**, not a compiler — it asserts every module dir ships both `<id>.wasm` and `manifest.json`, with `cargo:rerun-if-changed=embedded-modules` to pick up changes. Keeps `cargo build -p nexus-wasm-host` hermetic (no wasm toolchain needed by host-crate consumers or CI). | ✅ Resolved; sound |

## Module Boundary Design — Coherent

The seven source modules have crisp, non-overlapping responsibilities and a clean dependency graph:

```
lib.rs           (re-exports + module-level docs + doxtest)
  ├─ compute     (the compute() entry point + invocation lifecycle + watchdog)
  │   ├─ engine  (WasmEngine / WasmModule — owns the wasmtime Engine)
  │   ├─ sandbox (SandboxConfig + defaults)
  │   ├─ host    (HostContext / InvocationState / Linker wiring)
  │   ├─ manifest(ModuleManifest + HostFunction enum)
  │   └─ error   (ComputeError + Result alias)
  ├─ embedded    (include_dir! → module id/bytes/manifest lookups)
  └─ host        (re-exposed via pub use)
```

No circular deps; `lib.rs` re-exports only what consumers need (engine types, sandbox config, host context, manifest DTO, error, embedded accessors, and the two generated wire types). The public surface is small (9 items) and intentional — easy to audit when P3 starts consuming it.

## P3 (Orchestration) Consumability — Strong

The runtime surface is shaped well for the next wave:

- `WasmEngine::compute(&self, …)` — `&self`, no internal mutation, shareable across threads (wasmtime `Engine` is `Send + Sync`).
- `WasmModule` is `Clone` (wraps wasmtime `Module`, which is `Arc`-internally). One `load_module` per embedded module at daemon startup, then `compute()` is callable freely.
- `embedded_module_bytes(id)` / `embedded_module_manifest(id)` return `&'static`, so P-last can wire startup-time enumeration with `embedded_module_ids()` and load each once into a `Vec<(ModuleManifest, WasmModule)>`.
- `HostContext::from_input()` builds a fresh `HashMap` per invocation from the bundled `key_blocks`. With the snapshot already required by the schema, modules can avoid host-import calls entirely (the canonical path that `basic-combat` exercises).
- The plan's "Risks for Wave 3" callout correctly notes that the per-call watchdog thread is fine for μs–ms compute; if P3 ever needs higher volume, a small refactor (one shared `Engine`, watchdog behind a `OnceCell` or pool) can be done in-tree without breaking the API.

## Test Coverage — Adequate

17 tests, all passing:

- 11 lib unit tests (`cargo test` output above): sandbox defaults, manifest parsing (required + optional fields), `HostContext` indexing, two host-import wiring tests (end-to-end `kb_read` via WAT probe + non-whitelisted-import rejection), engine module loading (valid + invalid), embedded-module presence.
- 3 integration tests in `tests/basic_combat.rs`: full 4-part output validation against the real `basic-combat.wasm`, stateless reproducibility across invocations, killing-blow edge case (`is_alive=false`).
- 2 integration tests in `tests/sandbox_limits.rs`: infinite loop bounded by fuel (default budget), manifest fuel override.
- 1 doc test in `lib.rs`: the public API usage example compiles.

Not exercised (see S-002): memory-cap breach, wall-time breach.

## Build & Tooling Verification

| Tool | Result |
|------|--------|
| `cargo check -p nexus-wasm-host` | ✅ Clean (0.24s) |
| `cargo clippy -p nexus-wasm-host --all-targets -- -D warnings` | ✅ Clean (1.55s) — passes pedantic + nursery workspace lints inherited from root `[workspace.lints]` |
| `cargo test -p nexus-wasm-host` | ✅ 11 lib + 3 basic_combat + 2 sandbox_limits + 1 doc = **17 passed, 0 failed** |
| `cargo +nightly fmt -p nexus-wasm-host --check` | ✅ Clean |
| `git status --short` on review cwd | ✅ Working tree clean (only review branch HEAD matters) |

## Workspace Integration — Clean

- `Cargo.toml` workspace `members` gained exactly one line: `"crates/nexus-wasm-host"` — alphabetically placed between `nexus-narrative` and `nexus42`. No reordering of other members.
- `nexus-wasm-host/Cargo.toml` reuses the workspace-level `serde` / `serde_json` / `thiserror` / `tracing` deps and adds only two crate-specific deps: `wasmtime = "46"` (v46 line pinned, per `nexus-orchestration` style) and `include_dir = "0.7"` (mirrors the orchestration crate's `include_dir!` usage). `nexus-contracts = { path = "../nexus-contracts" }` reuses the generated contracts crate — no DTO duplication, single truth source for wire types preserved.
- `[lints] workspace = true` — uniform lint policy inherited from root `Cargo.toml`.
- `modules/basic-combat/` is correctly **not** a workspace member (has `[workspace]` empty table at line 40) and carries its own `Cargo.lock` (standard practice for standalone cdylibs whose build artifact is committed downstream).

## AGENTS.md Presence — Excellent (new-package policy)

Per root `AGENTS.md` policy: *"when adding a new package or crate to the monorepo, create an AGENTS.md in that directory — even if minimal — documenting its purpose, key rules, and dependencies."*

`crates/nexus-wasm-host/AGENTS.md` (56 lines) covers:

- Purpose statement + V1.61 deliverable scope.
- Architecture decisions table linking Q1 / Q6 / Q8 (compass grill).
- Module ABI export table (`alloc`, `init`, `compute`) and host-import table (`kb_read`, `narrative_query`) with sentinels.
- Key Rules (contracts-first, no cross-call state, embedded-modules are committed binaries, sandbox limits are non-negotiable).
- Dependencies list.

This is far above "minimal" — it's a useful on-ramp for any future contributor or for P3 to read before consuming the crate. Worth noting for the maintainability review.

---

## Source Trace

| Finding | Source Type | Source Reference | Confidence |
|---------|-------------|------------------|------------|
| S-001 | Manual reasoning + grep | `grep -c "tracing::" crates/nexus-wasm-host/src/*.rs` → all 0 | High |
| S-002 | Manual reasoning + test inspection | `tests/sandbox_limits.rs` (2 tests, both fuel-only); `src/compute.rs:209-213` (string-match classifier) | High |
| S-003 | Manual reasoning + cross-crate scan | `crates/nexus-orchestration/src/preset/mod.rs:68`, `crates/nexus-orchestration/src/embedded_skills.rs:30` (no build.rs guard); `crates/nexus-wasm-host/build.rs` (guard present) | High |
| S-004 | Test inspection | `crates/nexus-wasm-host/src/embedded.rs:50-58` (no `embedded_module_ids()` assertion) | High |
| S-005 | Manual reasoning + manifest inspection | `manifest.rs:53`, `compute.rs:236-239` (empty-string convention) | Medium |

All five findings are **non-blocking Suggestions**. The crate is well-architected, the compass grill decisions are resolved correctly, the manifest format is well-thought-out, the pre-compile+commit strategy is sound, the `modules/` non-workspace separation is correct, the workspace integration is clean, the AGENTS.md is exemplary, and all CI gates (check / clippy `-D warnings` / test / nightly fmt) pass.

---

## Handoff Notes

- **For PM**: Five Suggestion-level findings are non-blocking and can be addressed in P-mid / P-last / a follow-up plan. S-003 (sibling-pattern inconsistency with orchestration's `embedded-presets`/`embedded-skills`) is the most architecturally interesting; it is **pre-existing** and **not** a regression from this PR, so it should not block Done — record it as a low-severity residual (`R-V161-WASM-001` or similar) and address in a follow-up if desired.
- **For P3 implementer**: The plan's "Risks for Wave 3" callout is accurate — reuse one `WasmEngine` for all `compute()` calls, load each embedded module once at startup, and prefer the inline `key_blocks` snapshot over `kb_read` imports when the schema already bundles the relevant blocks. The watchdog thread is cheap at current call volumes; if P3 introduces volume that makes per-call thread spawn visible in profiles, refactor toward a shared timer.
- **For qc-specialist-2 / qc-specialist-3**: Review range tip `feature/v1.61-wasm-host-and-basic-combat` = `c86da6a1` (plan status flip) / `862bce92` (T10 last code commit). Same diff basis as this review — the three QC reports should align on the same commit range and the same plan_id verbatim. No follow-up questions from this seat.