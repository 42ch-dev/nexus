---
module: nexus-agent-host
date: 2026-10-01
problem_type: testing_pattern
category: testing-patterns
severity: medium
plan_id: 2026-10-01-v1.203-p3-windows-provider-fixture-parity
tags:
  - windows
  - fixture-launch
  - cmd-wrapper
  - provider-fixtures
  - ci-owned-proof
  - bounded-disposition
  - platform-behaviour
  - python-shim
applies_when:
  - adding a protocol fixture that must launch on Windows (or any second platform)
  - extending the platform-behaviour CI legs beyond map-only coverage
  - debugging Windows fixture launch/stdio failures in the native-CLI provider cohorts
---

# Windows Fixture-Launch `.cmd` Wrapper Seam (Native-CLI Provider Fixtures)

## Context

The native-CLI provider test cohorts spawn Python protocol mocks (`mock_claude_cli.py`, `mock_codex_app_server.py`, `mock_dsh_agent.py` under `crates/nexus-agent-host/tests/fixtures/native_protocol/`). On unix these launch through `#!/bin/sh` shims or an `env` shebang; neither is executable on Windows. The provider SDK spawns `Command::new(&spec.program)` (deepseek-harness-sdk 0.2.2) with no Windows shebang handling — so a fixture path that "works on unix" is simply not a launchable program on Windows. The v1.202 platform-behaviour leg therefore covered Windows with `map_`-only tests (compile-level mapping checks), recorded as open residual R-V1202-P1T3-001.

## Guidance

1. **Route fixture launch through one per-platform seam.** `test_support::fixture_launch(fixture)` (`crates/nexus-agent-host/src/lib.rs`, `pub(crate)`) maps a `.py` fixture to the program the current platform can spawn: unix returns the fixture path unchanged; Windows generates and returns a `<name>.cmd` wrapper. Test modules keep pointing at the `.py` fixtures; the seam owns the mapping.
2. **The generated wrapper embeds an absolute interpreter.** Body is exactly `@echo off` + `"<abs-python>" "<abs-fixture>" %*` (CRLF line endings). Rust std executes `.bat`/`.cmd` programs through `cmd.exe` automatically (with the post-CVE-2024-24576 argument escaping — safe here because fixture args are test-controlled), so the SDK spawn path and production provider code need **zero** change.
3. **Interpreter resolution is absolute, cached, and fails visibly.** Windows `python_path()` tries `py -3` then `python`, requires exit success + UTF-8 output + an absolute path, and panics with the exact string ``Python is unavailable through `py -3` and `python` `` — never an allowed-fail fallback. The result is cached in a process-wide `LazyLock`, which is why first-use discovery must not run inside another test's PATH-isolation window (see the lock contract below).
4. **Respect crate visibility when placing the seam.** `test_support` is `pub(crate)` — reachable from the in-crate `#[cfg(test)]` provider modules, **not** from `tests/` integration targets. The lib cohorts consume `fixture_launch`; `provider_readiness_acceptance.rs` instead cfg-splits its own local `write_fixture_shim` (the same per-platform pattern, with `set_executable` already a `cfg(not(unix))` no-op). Do not hoist test helpers into production code to share them.
5. **The `DshNativeProvider` single-path constraint admits the wrapper unchanged.** Its constructor takes an explicit `program` and requires empty native args (`dsh.rs` `stub_provider` family); pointing `program` at the generated wrapper satisfies the contract as-is. A launcher design that needs provider-code changes is the wrong design — the seam exists precisely to keep parity work fixture/test-boundary only.
6. **Construction-time lock precondition.** PATH-dependent interpreter discovery happens **before** `fixture_launch` returns, so callers hold `PROCESS_ENV_LOCK` throughout construction — the full contract (lock-owning helper family, the encompassing-guard raw-constructor exception, the one-direction lock ordering) lives in [process-env-lock-fixture-spawn-serialization.md](../workflow-patterns/process-env-lock-fixture-spawn-serialization.md). The seam's process-static shim `TempDir` has no destructor cleanup (deferred, R-V1203P3QC3-S001) — acceptable because allocation is bounded to one directory per test process.
7. **Scope cohorts by reason, and disposition the rest honestly (bounded-disposition rule).** When the first full Windows run of the lib cohort failed (80 passed / 39 failed, classes: fixture process launch with invalid temp working directory, fixture launch/stdio, runtime-discovery assumptions, path-separator assertions, sealed-filesystem capability) and the readiness integration target failed to *compile* on Windows (`process_alive` exists only under `#[cfg(unix)]`, E0425), the shipped step was scoped back to the green `providers::native_cli::map_` subset with the readiness step exercised on macOS. The failure classes are recorded as **triage hypotheses, not root causes**; kill-semantics and Node-peer cohorts stay unix by explicit reason, not by silence; and no production provider semantics or assertion was weakened to reach green. The residual row carries the named re-entry triggers (Windows fixture interpreter/process proof; readiness liveness assertion support; sealed-filesystem capability; production path-separator behavior).
8. **Windows proof is CI-owned.** The local cross-check `cargo check --target x86_64-pc-windows-msvc -p nexus-agent-host --tests` stops in the `ring` crate's build on this macOS host — the MSVC C headers (`assert.h`) are absent — which is an environment fact, not a code defect. Never install toolchains or relax the check to force it locally; record the block and let the `windows-latest` leg of `.github/workflows/platform-behaviour.yml` carry the compile/run proof.

## Why This Matters

Platform parity work fails in two seductive ways: faking it (claiming coverage from map-only tests or from a green scoped subset) and forcing it (editing production provider behavior or weakening assertions until Windows goes green). The seam keeps the honest middle path: fixtures become genuinely launchable on Windows with zero production change, and everything not yet provable is carried as an explicit, named-trigger disposition instead of absorbed as silent allowed-fail. Without the seam being construction-time-aware (rule 6), the first unlucky PATH race also poisons the process-wide interpreter cache, turning one flake into a whole-binary failure.

## When to Apply

- Adding a new protocol mock fixture or a new native provider cohort that must run on Windows.
- Extending any platform-behaviour leg from compile/mapping coverage to real process-launch coverage.
- Any CI round where a platform leg goes red for environment reasons — disposition by named trigger, never by weakening.

## Examples

```rust
// test_support seam (crates/nexus-agent-host/src/lib.rs) — caller holds PROCESS_ENV_LOCK
pub fn fixture_launch(fixture: &str) -> std::path::PathBuf {
    #[cfg(unix)]    { fixture.to_path_buf() }
    #[cfg(windows)] {
        // one process-static shim dir + SHIM_LOCK; body:
        //   @echo off
        //   "<abs-python>" "<abs-fixture>" %*      (CRLF)
    }
}

// lib cohort (in-crate test module): construct through the seam
let provider = stub_provider_locked("test-dsh", /* ... */); // env guard → fixture_launch

// integration target (cannot see pub(crate) test_support): cfg-split local shim
// provider_readiness_acceptance.rs — write_fixture_shim emits the same .cmd shape on Windows
```

## See also

- [windows-launch-cohort-remediation-discipline.md](windows-launch-cohort-remediation-discipline.md) — the v1.204 remediation pass that measured this seam's triage hypotheses and carries the assertion-strength discipline
- [process-env-lock-fixture-spawn-serialization.md](../workflow-patterns/process-env-lock-fixture-spawn-serialization.md) — the env-lock contract this seam's construction-time discovery depends on
- [native-cli-provider-adapter-pattern.md](../architecture-patterns/native-cli-provider-adapter-pattern.md) — provider SDK ownership and the spawn contract the seam satisfies
