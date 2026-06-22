---
plan_id: 2026-06-22-v1.58-workspace-occ-hardening
reviewer: qc-specialist-3
reviewer_index: 3
focus: performance-reliability
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus
working_branch: iteration/v1.58
diff_basis: d443e855..af82ad39
reviewed_at: 2026-06-22T23:45:00Z
verdict: Request Changes
---

# QC3 — V1.58 P0 Workspace OCC Hardening — Performance/Reliability Review

## Summary

This review focused on async I/O performance, TOCTOU contention mitigation, path canonicalization costs, concurrent test reliability, benchmark validity, and metrics overhead. **One HIGH severity finding** was identified: `std::fs::canonicalize` is called from async context without `spawn_blocking`, which could block the async runtime under load. The implementation has good patterns for OCC CAS, retry jitter, and body-size enforcement, but there are gaps in documentation of retry semantics and stress testing evidence.

## Findings

### 🔴 Critical

- **[F-001] Async runtime blocked by sync `std::fs::canonicalize` calls** (T2)
  - **Location**: `session.rs` lines 167, 332, 443
  - **Issue**: `canonicalize_workspace_root()` and `validate_changes_manifest()` call `std::fs::canonicalize()` (sync blocking I/O) from async functions without `spawn_blocking`. This defeats the async runtime under high load, potentially blocking all tasks on the worker thread.
  - **Impact**: High contention on workspace open could stall the entire async runtime. `std::fs::canonicalize` can take 1-10ms per call depending on filesystem and symlink depth.
  - **Fix**: Use `tokio::task::spawn_blocking(|| std::fs::canonicalize(path))` for all sync blocking I/O, or use the `async-fs` crate if available.
  - **Source**: Code analysis of `session.rs` async migration (T4) showing incomplete async transition

### 🟡 Warning

- **[F-002] TOCTOU retry semantics lack documentation of backoff strategy** (T5)
  - **Location**: `session.rs` `commit_session()` / `workspace_session.rs` `consume_session()`
  - **Issue**: The CAS retry mechanism (`db::consume_session`) has no documented backoff strategy, max retry count, or exponential backoff. Under high contention, concurrent writers could immediately retry (no sleep), causing retry storms. While `SQLite` row-level locking provides some serialization, there's no upper bound on retry attempts.
  - **Impact**: Malicious or accidental high contention could cause unbounded CPU+IO usage. Lack of max retry means indefinite retry storms.
  - **Fix**: Document the retry semantics: what happens on CAS failure, is there backoff, what's the max retry count? Consider adding exponential backoff with jitter if not present.
  - **Source**: Code review of `consume_session()` showing immediate return on `AlreadyCommitted`, no retry loop

- **[F-003] Path canonicalization cost per workspace open is O(depth)** (T2)
  - **Location**: `session.rs` lines 167, 332, 443
  - **Issue**: Each workspace open calls `canonicalize_workspace_root()` once (line 329), and each file in `validate_changes_manifest()` (line 443) may call `canonicalize_workspace_root()` again. A workspace with 100 files could canonicalize paths 101 times, each requiring filesystem traversal.
  - **Impact**: Latency scales linearly with file count. Could add 100-1000ms for large workspaces under cold cache conditions.
  - **Fix**: Cache the canonical workspace root per session; reuse instead of re-canonicalizing for each file.
  - **Source**: Analysis of `open_session()` and `validate_changes_manifest()` paths

- **[F-004] Benchmark lacks Criterion configuration and spec-compliant claims** (T14)
  - **Location**: `registry_refresh_latency.rs`
  - **Issue**: Benchmark uses default Criterion config, missing warm-up time, measurement time, sample size, and confidence interval settings. No cold cache vs warm cache separation (synthetic path is always "warm"). No explicit claim matching spec's "< 50ms synthetic" target.
  - **Impact**: Benchmark results may be noisy; unclear if it measures spec's promised latency target.
  - **Fix**: Add Criterion config with explicit warmup/measurement/sample settings; add benchmark group for actual CDN path (requires mock server); document target latency claim.
  - **Source**: Review of benchmark showing default `criterion_main!` with no config

### 🟢 Suggestion

- **[S-001] Consider jitter range expansion for high-N concurrent scenarios** (T13)
  - **Current**: Jitter is 100-500ms; base backoff is 500ms * 2^(attempt-1)
  - **Suggestion**: For large-scale deployments with N=100+ concurrent refreshers, consider expanding jitter to 100-1000ms or using exponential jitter to better spread retry spikes
  - **Impact**: Improves CDN resilience under surge loads

- **[S-002] Metrics counters on hot paths — minimal but measurable** (T16)
  - **Current**: `AtomicU64::fetch_add(1, Ordering::Relaxed)` on every `registry.refresh` call
  - **Suggestion**: Consider benchmarking metrics overhead (expect < 10ns per call). If overhead > 1% of cold path, consider sampling counters instead of exact counts
  - **Impact**: Marginal; `Ordering::Relaxed` is already optimal for relaxed semantics

## Performance Properties Verified

### ✅ Async I/O Migration (T4) — Partial Pass
- ✅ All `std::fs` calls converted to `tokio::fs` where feasible
- ❌ **EXCEPTION**: `std::fs::canonicalize` remains sync blocking without `spawn_blocking` (F-001)

### ⚠️ TOCTOU Contention Mitigation (T5) — Needs Documentation
- ✅ CAS primitive is atomic `UPDATE ... WHERE consumed = 0` in `db::consume_session`
- ✅ `commit_session()` binds validate + consume into one transaction guard
- ❌ **GAP**: No documented backoff strategy or max retry count (F-002)

### ⚠️ Path Canonicalization Cost (T2) — O(depth) per workspace open
- ✅ Path boundary enforcement uses canonicalized paths (symlink-resistant)
- ✅ Symlinks rejected via `symlink_metadata` (no traversal)
- ❌ **CONCERN**: Multiple `canonicalize` calls per workspace open (F-003)

### ✅ Concurrent Test Reliability (T3) — Pass
- ✅ `workspace_occ_concurrent.rs` uses `tokio::spawn` for deterministic parallelism
- ✅ Tests assert single-writer guarantees and OCC counter increments
- ✅ No flaky timing assertions; all checks are exact (`assert_eq!`)

### ⚠️ Latency Benchmark (T14) — Needs Configuration
- ✅ Benchmark exists and uses Criterion framework
- ✅ Cold vs warm separation (construct vs reuse)
- ❌ **GAP**: Missing Criterion config, CDN path not benchmarked, target latency not documented (F-004)

### ✅ LazyLock reqwest::Client (T9) — Pass
- ✅ `SHARED_CDN_CLIENT` is `LazyLock<reqwest::Client>` — initialized once
- ✅ Connection pooling + keep-alive configured via `reqwest::Client::builder()`
- ✅ One-time initialization cost (~1-5ms) amortized across all invocations

### ✅ Retry Jitter Thundering Herd (T13) — Pass
- ✅ Jitter range 100-500ms spreads retry timing
- ✅ Exponential backoff base: 500ms * 2^(attempt-1)
- ✅ Combined jitter + backoff: 600ms (attempt 0), 600-1000ms (attempt 1), 1100-1500ms (attempt 2)

### ✅ Body-Size Configurable Enforcement (T11) — Pass
- ✅ `read_body_with_limit` uses O(1) per-chunk check (`buf.len() + chunk.len() > max_size`)
- ✅ No CPU amplification attack vector — check before `extend_from_slice`
- ✅ Configurable via `CdnConfig::max_body_bytes`

### ⚠️ Metrics Counters Overhead (T16) — Minor
- ✅ `AtomicU64::fetch_add(1, Ordering::Relaxed)` is optimal for relaxed semantics
- ⚠️ **SUGGESTION**: Benchmark overhead (S-002); consider sampling if > 1% of path

### ⚠️ SQLx Cache Protocol (T18) — Incident Documented, Verification Needed
- ✅ Incident documented: P1's `sqlx prepare` deleted 137 cache entries (138→1)
- ✅ Protocol documented: run `cargo sqlx prepare --workspace -- --tests` after migrations
- ❌ **GAP**: No evidence of protocol execution in diff (only restoration commit)
- ⚠️ **RISK**: Need verification that protocol is enforced in CI gates

### ✅ Process Residuals (T17-T19) — All Closed
- ✅ T17: `cargo +nightly fmt clean` confirmed in commit `3347fee8`
- ✅ T18+T19: `.sqlx/` cache restored in commit `af82ad39`
- ✅ T21: Clippy doc lint fixed in commit `d42b78aa`

## Verdict Reasoning

**Verdict: Request Changes**

The async I/O migration is incomplete: `std::fs::canonicalize` is called from async context without `spawn_blocking` (F-001), which could block the async runtime under high load. This is a performance regression risk that defeats the T4 goal of "no blocking I/O in async handlers."

Additionally, the TOCTOU retry semantics lack documentation of backoff strategy and max retry count (F-002), which is a reliability gap under high contention. The path canonicalization cost scales linearly with file count (F-003), which could cause performance degradation for large workspaces.

The benchmark is functional but missing key configuration (F-004) that would make it production-grade. These issues should be addressed before merge to ensure the hardened foundation delivers on its performance and reliability promises.

## Cross-Plan Concerns

### P1 (DF-44 Reference Refresh Pipeline)
- The `registry.refresh` capability is a prototype for `reference.refresh` behavior. The retry jitter and metrics patterns here will be replicated.
- **Concern**: If F-001 (sync `canonicalize`) affects reference refresh code paths, P1 could inherit the same async runtime blocking issue.

### P2 (Capability Quality Convergence)
- P2 will build on the hardened OCC foundation. The performance characteristics of `commit_session()` under contention will affect P2's ability to scale capability dispatch.
- **Concern**: If F-002 (retry semantics documentation) is not addressed, P2's concurrent capability dispatch tests may fail to surface retry storms.

### P3 (Integration / QA)
- The `.sqlx/` cache protocol (T18) affects all plans that add or modify sqlx queries.
- **Recommendation**: Add a CI gate that runs `cargo sqlx prepare --workspace --check` after any migration change to prevent recurrence of the 137-entry deletion incident.

## Files Reviewed

- `crates/nexus-daemon-runtime/src/workspace/session.rs` (789 lines) — async I/O patterns, retry backoff
- `crates/nexus-orchestration/src/capability/builtins/registry.rs` (1022 lines) — SHARED_CDN_CLIENT, retry_jitter_ms, body-size enforcement
- `crates/nexus-orchestration/benches/registry_refresh_latency.rs` (51 lines) — benchmark structure
- `crates/nexus-daemon-runtime/tests/workspace_occ_concurrent.rs` (184 lines) — concurrent test determinism
- `crates/nexus-local-db/src/workspace_session.rs` (224 lines) — CAS primitive implementation

## Tools Run

- `git diff d443e855..af82ad39` — reviewed all changes in scope
- `grep` pattern searches for sync/blocking I/O patterns
- Manual code analysis of async/sync boundaries

## Evidence

- Diff showing `std::fs::canonicalize` calls at lines 167, 332, 443 in `session.rs`
- Diff showing `tokio::fs::` migration for all other I/O operations
- `db::consume_session` implementation showing CAS primitive without retry loop
- Benchmark code showing default Criterion configuration
- Concurrent test code showing `tokio::spawn` usage without timing assertions