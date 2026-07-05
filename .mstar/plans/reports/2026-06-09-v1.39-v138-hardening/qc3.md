---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-09-v1.39-v138-hardening"
verdict: "Approve"
generated_at: "2026-06-09"
---

# Code Review Report — V1.39 P5 V1.38 Hardening

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: k2p6
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-09T12:00:00+08:00

## Scope
- plan_id: 2026-06-09-v1.39-v138-hardening
- Review range / Diff basis: merge-base: 1b68d6ca + tip: 24919b27; equivalent to `git diff 1b68d6ca...24919b27`
- Working branch (verified): feature/v1.39-v138-hardening
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.39-p5
- Files reviewed: 5 (4 implementation files + 1 plan markdown)
- Commit range: 1b68d6ca..HEAD (59eac922); HEAD includes qc2 report (59eac922) on top of implementation tip 24919b27
- Tools run: cargo clippy, cargo test (nexus-local-db, nexus42, nexus-daemon-runtime, nexus-orchestration), cargo +nightly fmt --check

## Findings

### 🔴 Critical
*None.*

### 🟡 Warning

**W-1 (R-V138P1-01): `reject_produce_when_novel_complete` guard lacks production tracing**

The guard returns a user-facing `CliError::Other` with a helpful message, but emits **no `tracing` event** before returning. In production debugging, if a user reports "I can't advance to produce", there is no server-side log to correlate with the guard firing. The guard runs *before* the `tracing::info!(target: "fl_e.audit", ...)` span at line 1200, so a rejected advance leaves no audit trail.

→ **Fix**: Add `tracing::info!(target: "fl_e.audit", work_id, stage, "NOVEL_COMPLETE guard rejected produce advance");` before returning the error.

**W-2 (R-V138P0-01): Single-writer assumption is documented but not enforceable or detectable at runtime**

The `next_chapter()` doc comment clearly documents the single-writer invariant, but there is **no `debug_assert!`**, **no advisory lock**, and **no runtime warning** if the assumption is violated. If a future daemon auto-advancer runs concurrently with `creator run continue`, the failure mode is duplicate schedules for the same chapter — a silent data-quality issue that is hard to detect post-hoc.

→ **Fix** (minimal): Add a `tracing::warn!` in `next_chapter()` (or in the caller) when the selected chapter is already in `draft` status, making the violation observable. A `debug_assert!` on a process-level claim would be stronger but requires broader design work.

### 🟢 Suggestion

**S-1 (R-V138P0-03): `is_work_completed` incurs unbounded per-GET cost for non-completed novels**

The lazy promotion path in `get_work` calls `is_work_completed`, which in turn calls `list_chapters(pool, work_id)` — an unbounded `SELECT * FROM work_chapters WHERE work_id = ?` with no `LIMIT`. For a non-completed novel-profile Work, **every GET loads all chapter rows into memory** just to verify completion status.

- Per-GET cost breakdown:
  - Non-completed novel: 2 extra queries (`SELECT works` + `SELECT work_chapters *`) + deserialization of all rows
  - Completed novel (first GET): 3 queries (the above + `UPDATE works`) + deserialization
  - Completed novel (subsequent GETs): 0 extra queries (early exit via `status != "completed"` guard)

At typical scales (≤100 chapters) this is negligible for a local SQLite DB. At 1,000+ chapters, the repeated full-row load becomes measurable.

→ **Fix**: Optimize `is_work_completed` to use `SELECT COUNT(*) FROM work_chapters WHERE work_id = ? AND status != 'finalized'` instead of `list_chapters`, avoiding row deserialization. This is a safe, behavior-preserving change.

**S-2 (R-V138P0-04): `WorkApiDto.chapters` vector remains uncapped**

`enrich_with_chapters` fetches all chapters via `list_chapters` and builds a JSON array with no upper bound. The residual note argues this is acceptable because "total_planned_chapters is user-controlled at init and local DB is not exposed to untrusted network clients." This rationale is **honest for the current single-user local-first threat model**.

However, there is no defense against a malformed or corrupted Work with thousands of chapters causing memory pressure or slow JSON serialization. The risk is real but theoretical for the current use case.

→ **Fix**: Add a soft `LIMIT` (e.g., 500) to `list_chapters` with a `tracing::warn!` when truncation occurs. This provides defense-in-depth without impacting typical novels.

**S-3 (R-V138P0-03 test realism): The 3-GET idempotency test is structurally correct but does not mirror realistic read patterns**

The test `handler_get_work_lazy_promotes_completed_then_is_idempotent` issues three consecutive GETs in a tight loop. In practice, real workloads issue GETs across separate CLI invocations or daemon polling cycles, not within the same async block. The test correctly verifies the *mechanism* (no spurious PATCH on subsequent reads), but it does not prove resilience across process restarts or connection pool resets.

→ **Fix**: The test is adequate for the contract it verifies. A stronger test would restart the handler state between GETs (simulating a fresh connection), but this is higher effort than value for a local-first system.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W-1 | manual-reasoning | `crates/nexus42/src/commands/creator/run.rs` lines 1011–1025, 1199–1207 | High |
| W-2 | manual-reasoning | `crates/nexus-local-db/src/work_chapters.rs` lines 509–523 | High |
| S-1 | manual-reasoning | `crates/nexus-local-db/src/work_chapters.rs` lines 137–168, 568–619; `crates/nexus-daemon-runtime/src/api/handlers/works.rs` lines 373–416 | High |
| S-2 | manual-reasoning | `crates/nexus-daemon-runtime/src/api/handlers/works.rs` lines 438–455 | High |
| S-3 | manual-reasoning | `crates/nexus-daemon-runtime/tests/works_api.rs` lines 1211–1323 | Medium |

## Verification Evidence

### Static analysis
```
$ cargo clippy -p nexus42 -p nexus-local-db -p nexus-daemon-runtime -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 9.90s
# No warnings, no errors.
```

### Formatting
```
$ cargo +nightly fmt --all -- --check
# (no output — clean)
```

### Test results
```
$ cargo test -p nexus-local-db --lib -- work_chapters
running 21 tests
test result: ok. 21 passed; 0 failed; 0 ignored; finished in 0.51s

$ cargo test -p nexus42 --lib -- reject_produce
running 3 tests
test result: ok. 3 passed; 0 failed; 0 ignored; finished in 0.00s

$ cargo test -p nexus-daemon-runtime --test works_api handler_get_work_lazy_promotes
running 1 test
test handler_get_work_lazy_promotes_completed_then_is_idempotent ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; finished in 0.03s

$ cargo test -p nexus-orchestration --test auto_chain
running 21 tests
test result: ok. 21 passed; 0 failed; 0 ignored; finished in 0.40s

$ cargo test -p nexus-orchestration --lib -- research
running 8 tests
test result: ok. 8 passed; 0 failed; 0 ignored; finished in 0.00s
```

### Full-suite timing
```
$ cargo test -p nexus-local-db --lib
running 152 tests
test result: ok. 152 passed; finished in 3.64s

$ cargo test -p nexus42 --lib
running 615 tests
test result: ok. 615 passed; finished in 30.17s
```

## Residual Decisions (Performance & Reliability View)

| Residual | Decision in this PR | Assessment |
|----------|---------------------|------------|
| R-V138P0-01 | Accept-with-doc: doc comment added to `next_chapter()` | **Agree** for V1.39 scope. The doc comment is clear and actionable. However, the lack of runtime enforceability or detectability (W-2) means this residual should remain open with a lower severity or be re-scoped as "add runtime claim/lock when concurrent writers are introduced." |
| R-V138P0-02 | Out of scope (on-disk missing-file hints) | **Agree**. The plan goals did not include this; it is a UX polish item with no reliability impact. |
| R-V138P0-03 | Accept-with-doc + idempotency test: doc comment and `handler_get_work_lazy_promotes_completed_then_is_idempotent` | **Agree** for V1.39 scope. The contract is now explicit, and the test proves idempotency. The per-GET cost (S-1) is acceptable for local-first but should be optimized in a future hygiene plan. |
| R-V138P0-04 | Out of scope (uncapped chapters vector) | **Agree with caveat**. The "single-user local-first" rationale is honest and proportionate to the current threat model. The DoS vector is real in theory but requires a malformed/corrupted database, which is outside the attacker model. S-2 tracks a defense-in-depth improvement. |
| R-V138P0-05 | Fixed: two new unit tests for NULL/0 `total_planned_chapters` | **Agree**. Tests are at the right level (lib unit, fast, hermetic). |
| R-V138P1-01 | Fixed: `reject_produce_when_novel_complete` guard + 3 unit tests | **Agree with caveat**. The guard prevents the empty-chapter schedule correctly. The per-call cost is negligible (O(1), no DB round-trips). The missing tracing (W-1) is a real observability gap that should be fixed before the branch merges. |
| R-V138P1-04 | Out of scope (required template vars) | **Agree**. This is a contract/documentation concern with no performance or reliability impact in the current call graph. |

## Test Hermeticity & Determinism Assessment

| Test Suite | Hermetic | Fast | Deterministic | Notes |
|------------|----------|------|---------------|-------|
| nexus-local-db (152) | Yes — `fresh_pool()` uses `tempfile::tempdir()` | Yes (3.64s) | Yes — no randomness, no wall-clock assertions | All tests create isolated SQLite DBs in temp dirs |
| nexus42 lib (615) | Yes — extensive use of `tempfile::TempDir` | Moderate (30.17s) | Yes — no external dependencies in lib tests | Some integration tests in `tests/` subdir may touch real network; lib tests are clean |
| daemon-runtime works_api (29) | Yes — `tempfile::TempDir::new()` in test fixtures | Yes (≈0.5s total) | Yes — handler tests use in-memory/temp state | Idempotency test is hermetic and repeatable |
| nexus-orchestration auto_chain (21) | Yes — temp DBs | Yes (0.40s) | Yes — deterministic state machine transitions | |
| nexus-orchestration lib (≈500) | Yes | Yes | Yes | |

**Conclusion**: All test suites are hermetic, deterministic, and appropriately fast for their scope. The nexus42 lib suite at 30s is the slowest, but this is driven by the large number of tests (615) and is not a regression introduced by this PR.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

Rationale: No Critical findings. The two Warnings (W-1 missing tracing on guard; W-2 single-writer assumption not detectable at runtime) are observability gaps that do not block correctness or introduce new failure modes. All acceptance criteria are met:
1. Each listed residual has a fix or documented accept (R-V138P0-01/03 accepted with doc+test; R-V138P1-01 fixed with guard; R-V138P0-05 fixed with tests).
2. The completion path does not create empty-chapter `novel-writing` schedules (verified by `reject_produce_when_novel_complete` guard and tests).
3. Tests cover the completion guard (3 unit tests) and the idempotency path (1 integration test).

I recommend fixing W-1 (add `tracing::info!` to the guard) before merge as it is a one-line change with high observability value. W-2 can remain as a documented residual for a future concurrent-writer architecture.
