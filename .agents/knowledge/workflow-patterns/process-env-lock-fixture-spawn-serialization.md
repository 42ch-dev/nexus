---
module: nexus-agent-host
date: 2026-08-17
last_updated: 2026-10-01
problem_type: workflow_issue
category: workflow-patterns
severity: medium
tags:
  - env-mutation
  - test-flake
  - process-env-lock
  - path-isolation
  - python-fixture
  - cargo-test
  - fixture-launch
  - windows
applies_when:
  - tests mutate process env (PATH / HOME / DSH_RUNTIME_BIN) while other tests spawn fixtures through the same env
  - a fixture seam resolves an interpreter or generates a launcher at construction time, before any spawn
---

# Serialize env-mutating tests against env-consuming fixture spawns

## Context

Rust test binaries run cases on multiple threads sharing **one process environment**. V1.168 added protocol-fixture tests that spawn Python mock CLIs (`mock_claude_cli.py`, `mock_codex_app_server.py`, `mock_dsh_agent.py`) while other tests replaced `PATH` for isolation. V1.203 added a per-platform launcher seam (`fixture_launch`, `crates/nexus-agent-host/src/lib.rs` `test_support`) that resolves the Python interpreter and generates a `.cmd` wrapper at fixture-**construction** time on Windows — which moved part of the PATH dependence from spawn time to construction time (see [windows-fixture-launch-cmd-wrapper-seam.md](../testing-patterns/windows-fixture-launch-cmd-wrapper-seam.md)).

## Problem

Fixture scripts use the `#!/usr/bin/env python3` shebang. The kernel resolves `python3` through the **current process `PATH` at spawn time**. A PATH-isolation test that ran concurrently could empty PATH exactly while a fixture test spawned its mock → spawn failed (or found the wrong interpreter) → flaky failures that only appeared under parallel `cargo test`.

**V1.203 extension**: on Windows, `fixture_launch`'s lazily initialized `python_path()` discovery spawns `py -3` / `python`, so it too consults the process `PATH` — but it runs when a test **constructs** its provider stub, not when the fixture is spawned. Without a construction-time guard, a first Windows constructor executing during another test's PATH-isolation window fails both lookups — and because the discovery result is cached in a process-wide `LazyLock`, that single failure poisons interpreter discovery for every later test in the process.

## Guidance

1. Any test that **mutates** process env (PATH/HOME/… guards) or **spawns a subprocess whose resolution depends on env** must serialize with the others: one crate-wide lock (`PROCESS_ENV_LOCK` in `nexus-agent-host/src/lib.rs` `test_support`; an async-aware `tokio::sync::Mutex` — `.lock().await` in async tests, `.blocking_lock()` in sync `#[test]` fns), held for the whole mutate+spawn window.
2. Prefer restoring env in a guard `Drop` so panics cannot leak the mutation to the next case.
3. When the fixture spawn chain is the victim (not the culprit), the lock must be taken by **both** sides — a guard on the mutating test alone does not protect a spawn happening mid-mutation in another thread.
4. Absolute-path fixture interpreters (no `env` shebang) avoid the whole class, but only when the tool really exists at that path on CI.

### V1.203 extension — the lock precondition is construction-time

5. **Hold `PROCESS_ENV_LOCK` throughout fixture construction, not merely before spawn.** Whenever a constructor's work includes PATH-dependent discovery or launcher generation — the Windows arm of `fixture_launch` (`python_path()` + `.cmd` write) is the concrete case — the caller owns the lock from before construction until the constructed value no longer depends on the environment. `fixture_launch` deliberately does **not** self-lock (its caller-ownership contract is documented at the seam); a self-locking seam would deadlock every caller that already holds the lock for its spawn window.
6. **Apply the contract across all provider families.** The claude / codex / dsh lib test modules construct stubs through the same seam. For families with many call sites, provide lock-owning constructor helpers (the dsh `stub_provider_locked` family): acquire `PROCESS_ENV_LOCK`, invoke the **raw** constructor synchronously, drop the guard before returning — so a later `launch_*` helper that acquires the lock itself never deadlocks.
7. **Recursive-acquisition hazard**: a test that already holds an encompassing `PROCESS_ENV_LOCK` guard (e.g. a readiness/probe test guarding its whole mutate+spawn window) must call the **raw** constructor, never the lock-owning helper — the intermediate fix attempt that routed an encompassing-guard caller through the lock-owning helper deadlocked on the non-reentrant mutex. Review heuristic: for every constructor call site, ask "who owns the guard?" — exactly one acquisition per call path.
8. **Lock ordering is one direction only**: `PROCESS_ENV_LOCK` → seam-local locks (`SHIM_LOCK`) → lazily initialized discovery. Never acquire `PROCESS_ENV_LOCK` while holding a seam-local lock, and never add an `.await` (or an env-lock acquisition) inside the synchronous seam.
9. **Process-static shim lifecycle is deferred debt** (register row R-V1203P3QC3-S001): the generated-wrapper directory is a `static LazyLock<TempDir>`, and Rust statics are not dropped at process exit, so the shim directory can outlive the test binary until external cleanup. Allocation is bounded to one directory per test process and the seam carries an explicit caveat comment; an owned teardown hook remains the follow-up when the Windows launch cohort lands.

## Why This Matters

The failure reads as "flaky CI" but is a deterministic race on shared process state. Without the lock you get heisen-failures that pass locally (different scheduling) and block merges intermittently. The construction-time variant is worse than the spawn-time original: a `LazyLock` cache turns one unlucky race into a **process-permanent** poisoned discovery, so every subsequent fixture test fails — the flake no longer heals on retry within the same binary.

## Examples

```rust
// crates/nexus-agent-host/src/lib.rs (test_support)
pub static PROCESS_ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// Callers must hold PROCESS_ENV_LOCK throughout construction:
// PATH-dependent python_path() discovery happens before fixture_launch returns.
pub fn fixture_launch(fixture: &str) -> std::path::PathBuf { /* per-platform mapping */ }

// env-mutating test
let _guard = crate::test_support::PROCESS_ENV_LOCK.lock().await;
let _path_guard = PathGuard::isolate(temp_dir);
// ... probe / scan assertions ...

// fixture-constructing test (lock-owning helper, dsh family)
async fn stub_provider_locked(/* ... */) -> DshNativeProvider {
    let _guard = crate::test_support::PROCESS_ENV_LOCK.lock().await;
    stub_provider(/* ... */) // raw constructor, synchronous
} // guard dropped here — before any launch helper reacquires the lock

// encompassing lock-owning test — calls the RAW constructor instead:
let _guard = crate::test_support::PROCESS_ENV_LOCK.lock().await;
let provider = stub_provider(/* ... */); // never stub_provider_locked here
```

## See also

- [windows-fixture-launch-cmd-wrapper-seam.md](../testing-patterns/windows-fixture-launch-cmd-wrapper-seam.md) — the per-platform launcher seam whose interpreter discovery this lock contract protects
- `workflow-patterns/shared-cargo-target-dir-worktree-stale-manifest-dir.md` — a different env-poisoning flake (compile-time, not runtime)
