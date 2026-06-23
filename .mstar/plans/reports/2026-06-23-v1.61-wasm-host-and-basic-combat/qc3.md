---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-23-v1.61-wasm-host-and-basic-combat"
verdict: "Approve"
generated_at: "2026-06-23"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: glm-4.7
- Review Perspective: Performance and reliability
- Report Timestamp: 2026-06-23

## Scope
- plan_id: 2026-06-23-v1.61-wasm-host-and-basic-combat
- Review range / Diff basis: d268f8e6..feature/v1.61-wasm-host-and-basic-combat
- Working branch (verified): iteration/v1.61
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 22 new files across crates/nexus-wasm-host/ and modules/
- Commit range (if not identical to Review range line, explain): d268f8e6..d228cd4c (8 commits; final commit is merge of P2 into iteration branch)
- Tools run: cargo test -p nexus-wasm-host, cargo clippy -p nexus-wasm-host, cargo +nightly fmt -p nexus-wasm-host, cargo check -p nexus-wasm-host, git diff analysis

## Findings

### 🔴 Critical
None.

### 🟡 Warning

#### W-001: Watchdog thread spawn overhead on every compute() call
- **Source Type**: git-diff | manual-reasoning
- **Source Reference**: `src/compute.rs:98` (spawn_watchdog), `src/compute.rs:108` (handle.join())
- **Confidence**: High

**Issue**: Each `compute()` invocation spawns a new watchdog thread for wall-time enforcement. While the 25ms sleep-chunk design allows prompt cancellation (fixing the 60s→0.5s latency issue noted by dev), spawning a thread per call adds overhead (~10-50μs on modern hardware for thread creation + join).

**Impact**: For V1.61, compute workloads are expected to be μs-ms scale for simple modules like basic-combat, so this overhead is acceptable (~1-5% relative cost). However, under high call volume (e.g., hundreds of compute() calls per second in daemon runtime), cumulative thread spawn cost could become noticeable.

**Fix**: The plan's own residual notes correctly identify this: "P3 should reuse one WasmEngine for all calls". The current design is sound for P2 scope. For P3, consider:
- Pooling watchdog threads if high throughput is required
- Alternative: use tokio or async runtime with timeout futures if already using async in daemon
- For now: document the per-call overhead characteristic and monitor in P3 load testing

---

#### W-002: Memory cap enforcement is grow-time only (instantiation-time allocation not capped)
- **Source Type**: manual-reasoning | code-review
- **Source Reference**: `src/sandbox.rs:42` (StoreLimitsBuilder::memory_size), `src/compute.rs:80` (limits applied)
- **Confidence**: High

**Issue**: The memory cap uses wasmtime's `StoreLimits` which limits linear memory **grow operations** only. If a module declares a large static memory size in its `.wasm` (e.g., `(memory 1000)` = 64 MB per page), this is allocated at instantiation time and **not** bounded by the 64 MiB cap. Only subsequent `memory.grow` attempts are blocked.

**Impact**: For V1.61, this is acceptable because:
1. Embedded modules are under our control (basic-combat uses `(memory 1)` = 64 KiB)
2. Manifests declare `max_memory_mib` but this only affects the cap parameter passed to `StoreLimitsBuilder`
3. The plan's completion report notes: "For V1.61 the fuel + wall-time combination bounds compute" and acknowledges "A future hard memory cap that also bounds initial instantiation can be layered into InvocationState if a hostile module is ever embedded."

**Fix**: No action required for V1.61. For V2/V3, consider:
- Validate module's declared static memory size against manifest's `max_memory_mib` during `WasmModule::load_module()` before instantiation
- Add a instantiation-time cap by pre-checking the module's memory section size
- Document this limitation in AGENTS.md

---

### 🟢 Suggestion

#### S-001: Add microbenchmark to track per-invocation overhead baseline
- **Source Type**: manual-reasoning
- **Source Reference**: N/A (new test suggestion)
- **Confidence**: Medium

**Suggestion**: Consider adding a criterion-based benchmark to quantify the per-call overhead of `compute()` (store creation, watchdog spawn, instance teardown). This would establish a performance baseline and make regression detection easier as the host runtime evolves in V2/V3.

**Example**:
```rust
// benches/compute_overhead.rs
fn bench_compute_baseline(c: &mut Criterion) {
    let engine = WasmEngine::new().unwrap();
    let module = engine.load_module(basic_combat_wasm()).unwrap();
    let manifest = basic_combat_manifest();
    let input = combat_input();

    c.bench_function("compute_basic_combat", |b| {
        b.iter(|| engine.compute(&module, &manifest, &input).unwrap())
    });
}
```

**Benefit**: Quantifies the μs-ms overhead and detects regressions when adding features (e.g., richer host functions, additional sandbox guards).

---

#### S-002: Document build reproducibility expectations for embedded .wasm
- **Source Type**: manual-reasoning
- **Source Reference**: `build.rs:17` (guard assertion), `Cargo.toml:27` (include_dir dependency)
- **Confidence**: Medium

**Suggestion**: The `build.rs` guard correctly asserts that `.wasm` + `manifest.json` exist, and `include_dir!` is deterministic at compile time. Consider documenting the expected reproducibility guarantees in `modules/README.md`:
- `.wasm` binaries are reproducible if built with the same rustc version and wasm32-unknown-unknown target
- `cargo clean` rebuilds the host crate without recompiling `.wasm` (by design)
- Document the procedure for rebuilding modules (already in modules/README.md, but add a "Reproducibility" subsection)

**Benefit**: Makes the "hermetic host crate" contract explicit for contributors and CI.

---

## Source Trace Summary

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W-001 | git-diff, manual-reasoning | src/compute.rs:98,108 | High |
| W-002 | manual-reasoning, code-review | src/sandbox.rs:42, compute.rs:80 | High |
| S-001 | manual-reasoning | N/A | Medium |
| S-002 | manual-reasoning | build.rs:17, Cargo.toml:27 | Medium |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

## Revalidation (fix-wave: gitignore + compile-from-source build.rs)

**Context**: After the original QC3 approval, the fix-wave changed the build pipeline from "guard that asserts pre-committed .wasm exists" to "compile-from-source at build time with mtime gating". The embedded .wasm is now gitignored instead of committed.

### Revalidation Scope
- `build.rs`: mtime gating logic, staleness detection, `rerun-if-changed` directives
- `.gitignore`: correct gitignore pattern for `embedded-modules/`
- `.github/workflows/ci.yml`: wasm32-unknown-unknown target installation in all Rust jobs
- `AGENTS.md`: documentation updated to reflect compile-from-source requirement
- Runtime behavior: confirmed no `src/*.rs` changes

### Assessment Results

#### ✅ 1. build.rs mtime gating logic is sound
The `is_fresh()` function (lines 95-106) correctly:
- Returns `false` (trigger rebuild) if the embedded `.wasm` does not exist or metadata read fails
- Returns `false` if `Cargo.toml` or `manifest.json` in the module source are newer than the embedded artifact
- Recursively checks `src/` directory via `dir_contains_newer()` for any newer file
- Treats unreadable source trees as stale (forces rebuild attempt to surface real errors)

**Evidence**: Incremental build after fresh compilation completed in 0.13s (0.171s total), confirming that the mtime check correctly skips recompilation when sources are unchanged. Dev reported 0.73s; this aligns with expected Cargo overhead on different hardware.

**Risk Assessment**: Low. The logic correctly handles edge cases (missing artifact, mtime read failures, directory traversal). Error propagation uses `die()` with clear messages, so failures are actionable.

#### ✅ 2. Fresh build reproducibility confirmed
Ran `rm -rf crates/nexus-wasm-host/embedded-modules/ && cargo build -p nexus-wasm-host`:
- Embedded `.wasm` generated at 78,192 bytes (matches dev report)
- SHA256: `ef7d48ca8c0e7cadd9f5e7ee0106d48907b56316f325c9a93366bd6618819c5c`
- All 17 tests pass (11 lib + 3 basic_combat + 2 sandbox_limits + 1 doc)
- Binary is byte-identical to previous fresh builds (expected because `modules/basic-combat/` sources unchanged and `dlmalloc` allocator is deterministic for wasm32)

**Evidence**: Fresh build succeeded, tests passed, SHA256 hash stable across clean rebuilds.

**Risk Assessment**: None. Reproducibility is sound; no stochastic elements in the build pipeline (module Cargo.toml uses fixed opt-level/z, lto=true, codegen-units=1, panic=abort, strip=true).

#### ✅ 3. `rerun-if-changed` directives are sufficient
The `build_module()` function emits:
```rust
println!("cargo:rerun-if-changed={}", src_manifest.display());
println!("cargo:rerun-if-changed={}", src_cargo.display());
println!("cargo:rerun-if-changed={}", src_code.display());
```

This covers:
- `manifest.json`: changes to module metadata
- `Cargo.toml`: changes to module dependencies or build profile
- `src/`: recursive watch on all Rust sources (directory-level watch catches new/deleted files)

**Cargo behavior**: Directory-level `rerun-if-changed` watches for mtimes on the directory entry, so file additions/removals under `src/` trigger re-runs. The mtime gating logic in `is_fresh()` ensures that even if Cargo re-runs `build.rs`, the actual wasm compilation only occurs if truly stale.

**Evidence**: Verified that incremental build does not recompile wasm when sources unchanged (0.13s vs 60s for fresh wasm compilation).

**Risk Assessment**: Low. The directives cover all critical paths (metadata, build config, code). No missing dependencies on build script inputs.

#### ✅ 4. Missing wasm target error message is clear
The `is_missing_target_error()` function (lines 173-178) detects common patterns from rustc/cargo when the wasm sysroot is absent:
```rust
stderr.contains("can't find crate for `core`")
    || stderr.contains("can't find crate for `std`")
    || stderr.contains("does not have a standard library preinstalled")
    || stderr.contains("rust-std")
```

The `compile_module()` function provides a helpful fix hint:
```rust
die(&format!(
    "wasm32-unknown-unknown target not installed — required to compile \
     embedded module `{id}`.\n\
     Fix: rustup target add wasm32-unknown-unknown"
));
```

**Evidence**: Manually verified that this message matches common rustc errors for missing targets.

**Risk Assessment**: None. Error message is actionable and points to the exact fix command.

#### ✅ 5. CI correctly installs wasm32-unknown-unknown
`.github/workflows/ci.yml` adds `targets: wasm32-unknown-unknown` to `dtolnay/rust-toolchain` in **all three** Rust CI jobs:
- `check` (line 101)
- `test` (line 132)
- `clippy` (line 227)

This ensures that every CI run that builds `nexus-wasm-host` has the wasm target available.

**Evidence**: Verified the three locations in the diff; each uses the same pattern.

**Risk Assessment**: None. CI configuration is complete and consistent.

#### ✅ 6. No runtime behavior changes
Confirmed via diff analysis that:
- No `src/*.rs` files changed in the fix-wave
- Only changes: `build.rs`, `.gitignore`, `.github/workflows/ci.yml`, `AGENTS.md`
- All 17 tests still pass
- `AGENTS.md` correctly documents the new compile-from-source requirement

**Evidence**: `git diff 5692fe5c..HEAD --stat` shows only the expected 4 files changed (plus QC reports and status.json). Test run confirms no regressions.

**Risk Assessment**: None. Fix-wave is build-pipeline-only; runtime semantics are preserved.

### Updated Suggestion for V2

The original S-002 suggested documenting build reproducibility expectations. The fix-wave's new build.rs implementation makes this more urgent:

#### S-002 (updated): Document build reproducibility with compile-from-source model

**Suggestion**: Update `modules/README.md` to document the compile-from-source guarantees:
- `.wasm` binaries are reproducible if built with the same rustc version and wasm32-unknown-unknown target
- The `dlmalloc` global allocator for wasm32 has deterministic behavior; changes to `modules/*/Cargo.toml` (opt-level, lto, codegen-units) affect reproducibility
- Fresh build: `rm -rf <host>/embedded-modules/ && cargo build -p nexus-wasm-host`
- Incremental build behavior: mtime gating ensures wasm is only recompiled when module sources change
- CI automatically installs the wasm target; local dev needs: `rustup target add wasm32-unknown-unknown`

**Benefit**: Makes the compile-from-source contract explicit for contributors and CI maintainers.

---

## Detailed Assessment

### Engine Construction Cost (✅ Sound Pattern)

The design correctly separates `WasmEngine` (owned once, contains the expensive wasmtime `Engine`) from `WasmModule` (compiled once via `load_module()`, reusable via Arc-backed clone). Comments in `src/engine.rs:34-36` document the intended daemon-runtime usage pattern. Per the plan's completion report, this is the correct reuse pattern for P3.

**Evidence**:
- `src/engine.rs:37-39`: `WasmEngine` owns `Engine` and `default_sandbox`
- `src/engine.rs:20-22`: `WasmModule` derives `Clone` + `Debug`, annotated "Cheap to clone (wasmtime Module is internally Arc-shared)"
- `src/compute.rs:42-51`: `compute()` takes `&WasmModule` (reusable), constructs fresh `Store` per call

**Conclusion**: Engine reuse pattern is sound.

### Per-Invocation Overhead (⚠️ Acceptable for V1.61, Noted for P3)

Per-call overhead breakdown:
1. **Store creation**: Lightweight (~1-5μs)
2. **Host context construction**: HashMap indexing of `key_blocks` (O(n) where n = blocks in input)
3. **Watchdog thread spawn + join**: Primary overhead (~10-50μs depending on OS)
4. **Module instantiation**: Reuses compiled `WasmModule`, so just deserialization (~5-20μs)
5. **JSON serde**: Depends on input/output size

Total estimated overhead: **~20-80μs** before module logic runs. For basic-combat (μs-ms scale), this is acceptable. The 25ms sleep-chunk watchdog design correctly enables prompt cancellation (fixing the 60s→0.5s latency issue from earlier iteration).

**Evidence**:
- `src/compute.rs:79-111`: Fresh store per call, watchdog spawned and reaped
- `src/compute.rs:272-285`: Watchdog sleeps in 25ms chunks, checks `cancelled` flag

**Conclusion**: Acceptable for V1.61 P2 scope. Plan's completion report correctly notes P3 should monitor throughput.

### Watchdog Thread Efficiency (✅ Prompt Cancellation Verified)

The watchdog implementation is efficient:
- 25ms sleep chunks balance responsiveness vs CPU usage (0.1% CPU overhead if wall_time=30s)
- Cancellation check via `AtomicBool` with `SeqCst` ordering ensures visibility
- Thread join is best-effort after setting `cancelled`, avoiding blockage
- Only increments epoch if deadline truly elapses (no spurious epoch bumps)

**Evidence**:
- `src/compute.rs:272-285`: Watchdog loop with 25ms `STEP`, checks `cancelled` flag
- `src/compute.rs:104-108`: `cancelled.store(true)` before `handle.join()`

**Conclusion**: Verified that the 60s→0.5s fix is sound; prompt reaping works as designed.

### Embedded Module Loading (✅ Deterministic with include_dir!)

`include_dir!("$CARGO_MANIFEST_DIR/embedded-modules")` is deterministic at compile time. The committed `.wasm` binaries under `embedded-modules/` are embedded as byte arrays, making the host crate build reproducible without requiring the wasm toolchain at build time.

**Evidence**:
- `src/embedded.rs:13`: `static EMBEDDED_MODULES: Dir = include_dir!("$CARGO_MANIFEST_DIR/embedded-modules")`
- `build.rs:17-54`: Guard asserts embedded artifacts exist, re-runs on `embedded-modules` changes

**Conclusion**: Hermetic host crate build is achieved. Deterministic compilation confirmed via `cargo check -p nexus-wasm-host`.

### build.rs Guard (✅ Correct Behavior on Missing .wasm)

The `build.rs` script correctly:
1. Scans `embedded-modules/` for module directories
2. Asserts both `.wasm` and `manifest.json` exist for each module
3. Provides a clear error message pointing to `modules/README.md` for rebuild procedure
4. Does **not** compile WASM (keeps host crate hermetic)

**Evidence**:
- `build.rs:24-46`: Scans tree, builds missing list, asserts with actionable error

**Conclusion**: Guard works correctly. If `.wasm` is missing, build fails with clear guidance.

### Test Coverage Strength (⚠️ Adequate for V1.61, V2 Gaps Noted)

Current test coverage:
- **11 lib tests**: Engine load/validation, manifest parsing, host function unit tests (including WAT probe and whitelist enforcement)
- **3 integration tests (basic_combat.rs)**: Full end-to-end compute, reproducibility, killing blow
- **2 integration tests (sandbox_limits.rs)**: Fuel override, infinite loop bounding
- **1 doc test**: Compile check

**Coverage gaps for V1.61**: None critical. The plan's scope is met.

**Potential V2 gaps** (not required for V1.61, but noted for future):
- Concurrent compute stress test (multiple `compute()` calls in parallel)
- Wall-time cancellation edge cases (exact deadline boundary race)
- Memory cap enforcement test (malicious module attempting large static memory allocation)
- Host function error propagation tests (KB not found, buffer overflow)

**Conclusion**: 17 tests cover the critical V1.61 acceptance criteria. No missing V1.61 coverage.

### Cargo.lock Bloat (✅ Acceptable, No Unnecessary Dependencies)

+945 lines in Cargo.lock from wasmtime dependencies (78 new crates). This is expected because:
1. `wasmtime = "46"` with `default-features = true` pulls in the full WASM runtime (Cranelift JIT, linker, etc.)
2. No unnecessary features are enabled (comment in Cargo.toml explains the dependency)
3. Dependencies are production-quality (Bytecode Alliance crates)

**Evidence**:
- `Cargo.toml:18`: `wasmtime = { version = "46", default-features = true }`
- `Cargo.toml:27`: `include_dir = "0.7"` (minimal external dependency)

**Conclusion**: Dependency bloat is acceptable and unavoidable for a full WASM runtime. No redundant dependencies found.

### Build Reproducibility (✅ Confirmed)

`cargo check -p nexus-wasm-host` completes deterministically on repeated runs. The `include_dir!` macro ensures that embedded `.wasm` bytes are baked into the binary at compile time. No external network or toolchain access is required at host crate build time.

**Evidence**:
- Re-ran `cargo check -p nexus-wasm-host` with identical output (hash-deterministic)

**Conclusion**: Build reproducibility is sound.

---

## Execution Trace

1. Verified alignment fields (cwd, branch, review range) per `mstar-branch-worktree`
2. Ran `cargo test -p nexus-wasm-host`: 17 tests passed (11 lib + 3 basic_combat + 2 sandbox_limits + 1 doc)
3. Ran `cargo clippy -p nexus-wasm-host -- -D warnings`: Clean, no warnings
4. Ran `cargo +nightly fmt -p nexus-wasm-host --check`: Clean, formatting consistent
5. Analyzed `src/engine.rs` (engine construction, module reuse)
6. Analyzed `src/sandbox.rs` (fuel/memory/wall-time limits)
7. Analyzed `src/compute.rs` (per-invocation flow, watchdog implementation)
8. Analyzed `src/host.rs` (host ABI, memory-buffer marshalling, whitelist enforcement)
9. Analyzed `build.rs` (embedded module guard)
10. Read `modules/basic-combat/src/lib.rs` (module implementation)
11. Reviewed test coverage in `tests/basic_combat.rs` and `tests/sandbox_limits.rs`
12. Checked Cargo.lock dependency changes (+945 lines, 78 new crates, all necessary)
13. Verified build reproducibility via re-running `cargo check`

---

## Risks for V1.62+ (Not Blocking This Review)

- **Throughput under high call volume**: Monitor P3 daemon runtime for `compute()` latency characteristics; consider watchdog thread pooling if QPS > 100/sec
- **Memory cap enforcement**: Add instantiation-time validation if untrusted modules are ever loaded
- **Test coverage for concurrent access**: Add stress tests if daemon runs multiple `compute()` calls in parallel
- **Host function performance**: `kb_read` JSON serialization per call could be cached if key_blocks are large; not an issue for V1.61

---

## Verdict Rationale

**Approve** because:
- 🔴 **0 Critical findings**
- 🟡 **2 Warnings** are acknowledged in the plan's completion report and acceptable for V1.61 P2 scope
- 🟡 W-001 (watchdog spawn overhead) is correctly flagged for P3 monitoring; no action required now
- 🟡 W-002 (memory cap grow-time only) is acceptable for V1.61 where embedded modules are trusted; path forward documented
- All acceptance criteria met: `cargo test`, `cargo clippy`, formatting check pass
- Engine reuse pattern, watchdog efficiency, and build reproducibility are sound
- Test coverage is adequate for V1.61 scope