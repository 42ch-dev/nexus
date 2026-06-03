# R-V113-005 UpstreamTimeout E2E Variance

**Residual:** `R-V113-005`
**Decision:** Accept
**Target:** Backlog
**Scope:** `crates/nexus42/tests/creator_register_e2e.rs`

## Evidence Matrix

| Environment | Proxy/Network Condition | Observed Duration | Result | Notes |
|-------------|-------------------------|-------------------|--------|-------|
| macOS (darwin, local dev) | Direct network, no proxy | ~30.87s (binary test), ~31.64s (lib test) | PASS (2/2 tests) | `creator_register_e2e_handles_platform_happy_path` and `creator_register_e2e_surfaces_platform_failure_context` both pass deterministically. Duration within the previously observed ~30s envelope. |

### Raw Test Output Summary

```
test commands::creator::tests::creator_register_e2e_handles_platform_happy_path ... ok
test commands::creator::tests::creator_register_e2e_surfaces_platform_failure_context ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 553 filtered out; finished in 31.64s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 553 filtered out; finished in 30.87s
```

## Rationale

The `UpstreamTimeout` path validates deterministic CLI error handling under a simulated upstream timeout. Runtime duration can vary by OS scheduling and proxy/network configuration, and the residual is accepted because the behavioral assertion is deterministic while wall-clock duration is environmental.

The observed durations (~30–32s on macOS without proxy) are consistent with the originally reported ~30s envelope. Both tests pass reliably and exercise the intended error-handling logic without nondeterministic outcomes.

## Revisit Trigger

Re-open if the test becomes nondeterministic in result, exceeds CI timeout budgets, or stops surfacing timeout/platform context in the CLI-visible error.
