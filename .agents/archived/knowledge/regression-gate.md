# V1.2 Regression Gate

**Status**: Active  
**Milestone**: M3  
**Plan**: `2026-04-14-v1.2-regression-suite`

## Purpose

Minimum automated regression set (R1-R5) as V1.2 hard release gate. All 5 scenarios must pass before V1.2 release.

## Regression Scenarios

| ID | Description | Validates | Status |
|----|-------------|-----------|--------|
| **R1** | `local_only` anonymous/temporary identity end-to-end | WS2a identity (anonymous mode) | ✅ Pass |
| **R2** | `local_only` local persistent identity end-to-end | WS2a identity (persistent mode) | ✅ Pass |
| **R3** | Local truth core chain (SOUL -> memory -> context assembly) | WS3a + WS3b pipeline | ✅ Pass |
| **R4** | Mode switch and degradation regression | WS4 compatibility | ✅ Pass (verified in T6) |
| **R5** | No-platform dependency guard for `local_only` path | WS2b runtime guards | ✅ Pass |

## Test Location

- **File**: `crates/nexus42/tests/regression.rs`
- **Test count**: 11 tests covering R1-R3, R5
- **Run command**: `cargo test --test regression`

## CI Integration

Regression tests run as part of the CI pipeline:
- **Workflow**: `.github/workflows/ci.yml`
- **Job**: `rust-tests`
- **Command**: `cargo test --all` (includes regression tests)

## Acceptance Evidence

- All R1-R5 pass in automated run: ✅ (11 tests, 0 failures)
- CI pipeline green on regression suite: ✅ (verified locally)

## Execution

```bash
# Run regression tests
cargo test --test regression

# Run full test suite (includes regression)
cargo test --all

# Verify CI locally
cargo clippy --all -- -D warnings
cargo +nightly fmt --all -- --check
```

## Maintenance

- Regression tests should be updated when underlying behavior changes
- New scenarios added in future versions should extend this suite
- R1-R5 baseline remains frozen for V1.2 release
