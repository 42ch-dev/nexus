# QA Report (Report-only) — V1.61 P-last

**Plan**: 2026-06-23-v1.61-daemon-integration-and-closeout (P-last)
**Agent**: qa-engineer
**Mode**: report-only
**Working branch**: iteration/v1.61 (HEAD 01948556573cc4175a85df67b6726464c6a50f1f)
**Review cwd**: /Users/bibi/workspace/organizations/42ch/nexus
**Generated**: 2026-06-23 (mid-qa closeout verification)

## Scope tested

P-last closeout verification per assignment:
- E2E `narrative.compute` integration (combat-engine preset → WASM module → side effects)
- Daemon boot + WasmEngine singleton + module cache wiring
- Capability registry count (32 builtins)
- Embedded preset validation gate
- Cross-crate regression spot-checks (wasm-host, kb, contracts)
- Graceful degradation on WasmEngine init failure

**Not in scope** (per assignment):
- Full `cargo test --all` (dev already ran 4375 pass / clippy clean)
- Any source code fixes

## Verification matrix

| # | Check | Command | Result | Evidence |
|---|-------|---------|--------|----------|
| 1 | E2E compute cycle (3 tests) | `cargo test -p nexus-orchestration --test compute_e2e` | **PASS** | 3/3 passed (0.77s). `combat_engine_preset_loads_and_resolves_capabilities`, `narrative_compute_e2e_full_cycle_applies_side_effects`, `narrative_compute_e2e_rejects_missing_world`. Full cycle: world + 2 computable chars → `narrative.compute` (basic-combat) → state_delta (HP 120→100), 1 timeline event, battle_report (damage=20). |
| 2 | Daemon boot tests + Wasm wiring | `cargo test -p nexus-daemon-runtime` | **PASS** | 39 tests passed (34+4+1 doc). `boot.rs` (lines 175-239): single `WasmEngine` + `ModuleCache` constructed once, `warm_embedded` + `warm_dir(~/.nexus42/modules/)`, injected via `with_runtime_deps_and_wasm`. On `WasmEngine::new()` Err: warn + `None` → falls back to `with_runtime_deps`. |
| 3 | Capability registry (32 builtins) | `cargo test -p nexus-orchestration --test capability_registry` | **PASS** | `registry_has_thirty_two_builtins` asserts `reg.len() == 32`. Test source documents V1.61 P3 addition of `narrative.compute` (31→32). All lookups pass. |
| 4 | Combat-engine preset validation gate | `cargo test -p nexus-orchestration --test preset_validation` + cross-ref in compute_e2e | **PASS** | 13/13 preset validation tests pass. `compute_e2e` explicitly calls `load_embedded_preset("combat-engine", &registry)` and asserts `requires_capabilities` includes `narrative.compute`. |
| 5a | Regression: wasm-host | `cargo test -p nexus-wasm-host` | **PASS** | 3 (basic_combat) + 2 (sandbox_limits) + 1 doc = 6 tests pass. |
| 5b | Regression: kb | `cargo test -p nexus-kb` | **PASS** | 139 tests pass (validation + structured mode). |
| 5c | Regression: contracts (drift) | `cargo test -p nexus-contracts` | **PASS** | 4 (drift detection) + 5 (rename compliance) pass. |
| 6 | Graceful degradation (Wasm init failure) | Read `boot.rs` + compute_e2e test behavior | **PASS** | `boot.rs:220-227`: `Err(e) => { tracing::warn!(... "WasmEngine init failed; narrative.compute will be unavailable"); None }`. Registry uses non-wasm path. `compute_e2e` documents: module trap → `compute_error` timeline event recorded (no daemon crash). `narrative.compute` returns `WorkerUnavailable` when unavailable. |

## Findings

**None (blocking or otherwise).**

All assigned verification points passed with reproducible command output and source cross-checks. The P-last daemon integration (singleton WasmEngine + cache + embedded/user module loading + `narrative.compute` + graceful fallback) is sound.

## Reproduction steps (for any re-run)

```bash
# From repo root on iteration/v1.61 @ 01948556
cargo test -p nexus-orchestration --test compute_e2e
cargo test -p nexus-daemon-runtime
cargo test -p nexus-orchestration --test capability_registry
cargo test -p nexus-orchestration --test preset_validation
cargo test -p nexus-wasm-host
cargo test -p nexus-kb
cargo test -p nexus-contracts
```

(Also read the test sources and `boot.rs` sections cited above for wiring evidence.)

## Evidence (key excerpts)

- E2E output: `test result: ok. 3 passed`
- Registry: `assert_eq!(reg.len(), 32);` (test source notes P3 addition of `narrative.compute`)
- Boot fallback (boot.rs):
  ```rust
  Err(e) => {
      tracing::warn!(error = %e, "WasmEngine init failed; narrative.compute will be unavailable");
      None
  }
  ```
  Later:
  ```rust
  let capabilities = Arc::new(match wasm_singleton {
      Some((engine, cache)) => CapabilityRegistry::with_runtime_deps_and_wasm(...),
      None => CapabilityRegistry::with_runtime_deps(...),
  });
  ```
- compute_e2e graceful path: on Err, asserts `timeline >= 1` for `compute_error` event; otherwise validates exact side-effects (state_delta, timeline count, battle_report).

## Not tested

- Full workspace `cargo test --all` / clippy (explicitly out of scope; dev pre-ran and reported 4375 pass + clean).
- Production daemon runtime under load or with real user modules.
- Any cross-crate changes outside the 6 spot-check crates.

## Recommended owners

N/A — no issues found. Ready for PM closeout (Profile B compaction + PR).

## Verdict

**PASS**

V1.61 daemon integration (WasmEngine singleton + module cache + `narrative.compute` + graceful degradation + 32-capability registry + combat-engine preset) is verified sound for P-last closeout.
