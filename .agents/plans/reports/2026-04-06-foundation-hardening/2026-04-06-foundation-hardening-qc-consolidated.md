---
report_kind: qc-consolidated
reviewer: project-manager
plan_id: "2026-04-06-foundation-hardening"
verdict: "Approve"
generated_at: "2026-04-06"
---

# QC Consolidated Decision — Foundation Hardening (Plan A)

**PM**: @project-manager
**Date**: 2026-04-06
**Branch**: `feature/v2.0-foundation-hardening`
**Commits**: `1b8e849` (implementation) → `ffaaba8` (QC fixes)

## Decision: **Approve**

All blocking and important findings resolved. 480 tests passing, zero clippy warnings, fmt clean.

---

## Blocking Items (Fixed)

| # | Finding | Source | Fix | Commit |
|---|---------|--------|-----|--------|
| 1 | `#[deny(clippy::unwrap_used)]` missing on daemon crate | QC2-H2, QC3-R1 | Added `#![deny(clippy::unwrap_used)]` to `nexus42d/src/lib.rs` | `ffaaba8` |
| 2 | Production `.unwrap()` in `acp/transport.rs:333,364` | QC2-H3 | Replaced with `.ok_or_else()` error propagation | `ffaaba8` |
| 3 | Context handler returns 200 OK (stub) | QC3-R5 | New `NexusApiError::NotImplemented` variant → 501 Not Implemented | `ffaaba8` |
| 4 | Pool exhaustion not tested | QC3-R2 | Added `pool_exhaustion_returns_error_gracefully` test | `ffaaba8` |

---

## Residual Findings (Tracked, Non-Blocking)

| ID | Severity | Description | Source | Decision | Target |
|----|----------|-------------|--------|----------|--------|
| FH-R1 | MEDIUM | Schema drift between CLI/daemon has no automated detection | QC2-H1 | Defer — add CI drift-detection job in V1.1 | V1.1 |
| FH-R2 | MEDIUM | Pool timeout not explicitly configured (uses deadpool default 30s) | QC1-M2 | Accept — deadpool defaults are reasonable for V1.0 | V1.1 |
| FH-R3 | LOW | Error code inconsistency between `error_code()` and Internal `code` field | QC1-M3 | Defer — document error code convention | V1.1 |
| FH-R4 | MEDIUM | NexusApiError auth variants underspecified (only `AuthRequired`) | QC2-M1 | Defer — Plan D (Auth Flow) will add auth-specific variants | Plan D |
| FH-R5 | LOW | `DbPool::status()` method has no callers | QC2-M2 | Accept — useful for future monitoring endpoint | V1.1 |
| FH-R6 | LOW | Test helper duplication across errors.rs and middleware.rs | QC2-M3 | Defer — extract to shared test utils | V1.1 |
| FH-R7 | LOW | SQLite file locking not tested | QC3-R3 | Defer — complex to test, WAL mode mitigates | V1.1 |
| FH-R8 | LOW | Race condition window between init_workspace and middleware | QC3-R4 | Accept — window is negligible (in-memory mutex) | V1.1+ |
| FH-R9 | LOW | Pool size tuning undocumented | QC2-L1 | Accept — add comment with guidance | V1.1 |
| FH-R10 | LOW | Mutex poisoning uses `.expect()` — could crash daemon | QC1-M1 | Accept — documented as crash-on-poison policy | V1.1 |
| FH-R11 | LOW | Test file organization (large test modules in production files) | QC3-R7 | Defer — split tests into separate files | V1.1 |

**Net**: 4 blocking fixed → 0 open blocking. 11 residuals tracked (0 HIGH, 3 MEDIUM, 8 LOW).

---

## QC三审 Summary

| Reviewer | Verdict | Critical | High | Medium | Low |
|----------|---------|----------|------|--------|-----|
| QC #1 | APPROVE | 0 | 0 | 3 | 4 |
| QC #2 | REQUEST CHANGES | 0 | 3 | 3 | 3 |
| QC #3 | REQUEST CHANGES | 1 | 2 | 2 | 2 |

**Conflicts resolved**: QC2-H2 and QC3-R1 are the same issue (daemon unwrap deny). QC3-R1 was escalated to Critical by reviewer #3 but the underlying issue is the same as QC2-H2 (High). Both fixed in `ffaaba8`.

---

## Verification Evidence

| Check | Result |
|-------|--------|
| `cargo test --all` | **480 passed**, 0 failed, 1 ignored |
| `cargo clippy --all -- -D warnings` | **0 warnings** |
| `cargo +nightly fmt --all -- --check` | **clean** |
| `.unwrap()` in production (commands/) | **0** |
| `.unwrap()` in production (handlers/) | **0** |
| `.unwrap()` in production (workspace/) | **0** |
| `#[deny(clippy::unwrap_used)]` on nexus42 | ✅ 9 modules |
| `#[deny(clippy::unwrap_used)]` on nexus42d | ✅ crate-level |
| Context assemble → 501 | ✅ Not Implemented |

---

## Next Step

→ **@qa-engineer verification** → sign-off → merge to main
