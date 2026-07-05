---
plan_id: 2026-06-22-v1.58-workspace-occ-hardening
reviewer: qc-specialist
reviewer_index: 1
focus: architecture-maintainability
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus
working_branch: iteration/v1.58
diff_basis: d443e855..af82ad39
reviewed_at: 2026-06-22T21:00:00Z
generated_at: 2026-06-22T21:00:00Z
verdict: Approve
carry_forward_inheritance: >-
  closed-in-v158p0 =
  [R-V156P0-M001..M006,
   R-V156P1-M003..M005,
   R-V156P1-L001..L007,
   R-V156-MIDQA-01,
   R-V156-PROCESS-01,
   R-V156P1-CACHE-01,
   R-V157P0-L001,
   R-V157P0-L002]
---

# QC1 — V1.58 P0 Workspace OCC Hardening — Architecture/Maintainability Review

## Reviewer Metadata
- **Reviewer**: @qc-specialist (Reviewer #1)
- **Runtime Agent ID**: qc-specialist
- **Runtime Model**: deepseek/deepseek-v4-flash
- **Review Perspective**: Architecture coherence and maintainability risk
- **Report Timestamp**: 2026-06-22T21:00:00Z

## Scope
- **plan_id**: `2026-06-22-v1.58-workspace-occ-hardening`
- **Review range / Diff basis**: `d443e855..af82ad39`
- **Working branch (verified)**: `iteration/v1.58`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Files reviewed**: 31 changed files (2256 insertions, 146 deletions)
- **Commit range**: `d443e855..af82ad39` (16 commits: P0 + P1 + .sqlx restoration)
- **Tools run**: `cargo clippy -p nexus-daemon-runtime`, `cargo clippy -p nexus-orchestration`, `cargo +nightly fmt --all --check`, integration test compilation (`workspace_occ_concurrent`, `capability_registry`)

## Summary

All 21 tasks in V1.58 P0 are verified as resolved with sound architectural choices. The OCC workspace hardening (`canonicalize_workspace_root`, `enforce_path_boundary`, `commit_session` transaction guard, async I/O migration, `occ_conflict_total` metrics) is cleanly isolated in the workspace session layer with no ripple coupling. The capability surface quality upgrades (T7–T16) are correctly scoped to `registry.refresh` with a well-structured `LazyLock<reqwest::Client>`, proper retry jitter, structured metrics, and configurable body-size cap. Two medium-severity cross-coupling observations are noted (dual `LazyLock<reqwest::Client>` instances across P0/P1, and `HTTP_CLIENT` panic-vs-fallback inconsistency) but neither blocks approval — they are architectural hygiene notes for P-last or V1.59. All carry-forward residuals are confirmed resolved in the diff. No blocking findings.

**Verdict: Approve**

## Findings

### Medium severity (suggest addressing before Wave 2 or at P-last)

#### M-1: Dual LazyLock<reqwest::Client> instances — P0 SHARED_CDN_CLIENT vs P1 HTTP_CLIENT

**Location**: `crates/nexus-orchestration/src/capability/builtins/registry.rs:71` (P0), `crates/nexus-orchestration/src/capability/builtins/reference_refresh.rs:29` (P1)

**Observation**: P0 introduces `SHARED_CDN_CLIENT` (registry.rs) for the `registry.refresh` capability, and P1 introduces `HTTP_CLIENT` (reference_refresh.rs) for the `nexus.reference.refresh` capability. Both are `LazyLock<reqwest::Client>` with connection pooling, but they are separate instances with different configurations:

| Property | `SHARED_CDN_CLIENT` (P0) | `HTTP_CLIENT` (P1) |
|---|---|---|
| Redirect policy | `limited(0)` | Default (follow) |
| Client-level timeout | None | 30s |
| User-Agent | None | `nexus42/<version> reference-refresh` |
| Build failure handling | `unwrap_or_else(|_| Client::new())` (fallback) | `.expect()` (panic) |

**Evidence**: `registry.rs` line 71–76 vs `reference_refresh.rs` line 29–39.

**Impact**: Two separate connection pools mean redundant TLS sessions and TCP connections to CDN/origin hosts. Connection reuse across capabilities (registry.refresh → reference.refresh) is suboptimal. The `.expect()` in reference_refresh will panic the daemon on builder failure, while the P0 code gracefully degrades.

**Recommendation**: Extract a common `SharedHttpClient` or unify under a single daemon-wide `LazyLock<reqwest::Client>` with per-request overrides (`.timeout()`, `.user_agent()` on the `RequestBuilder`). This is natural P-last or V1.59 WL-A hygiene; not blocking for P0.

#### M-2: Unused import / dead_code in concurrent integration test

**Location**: `crates/nexus-daemon-runtime/tests/workspace_occ_concurrent.rs:21,27`

**Observation**: The test binary compiles with 2 dead_code warnings:
- Line 21: unused import `ChangeOp`
- Line 27: unused constant `OWNER`

**Evidence**: `cargo test -p nexus-daemon-runtime --test workspace_occ_concurrent --no-run` emits:
```
warning: unused import: `ChangeOp`
warning: constant `OWNER` is never used
```

**Impact**: These produce warnings during test compilation. While they don't fail CI (they are `#[warn(dead_code)]`, not `#[deny(dead_code)]`), they represent a hygiene gap. The concurrent test may have been refactored from a broader scope where `ChangeOp` and `OWNER` were used.

**Recommendation**: Remove the unused import and constant. This is a quick surgical fix.

### Low severity (non-blocking; defer to V1.59+ WL-A or P-last hygiene)

#### L-1: canonicalize_workspace_root uses sync std::fs::canonicalize in async context

**Location**: `crates/nexus-daemon-runtime/src/workspace/session.rs:167`

**Observation**: `canonicalize_workspace_root` (T2) uses `std::fs::canonicalize` (synchronous) even though T4 explicitly migrated OCC I/O to `tokio::fs`. Similarly, the `enforce_path_boundary` call in `validate_changes_manifest` (line 443) uses `std::fs::canonicalize`.

**Context**: This is a one-time call at workspace open and per-change validation, not a hot-path blocking issue. `std::fs::canonicalize` is not easily replaced with `tokio::fs` (which doesn't expose `canonicalize` in its stable API as of tokio 1.x). However, the doc comments say "async — V1.58 P0 T4" at line 341 for `compute_content_hashes`, which is genuinely async. The hybrid approach (sync for canonicalize, async for hashing) is technically sound but creates an inconsistency.

**Recommendation**: Add a code comment explaining why `std::fs::canonicalize` is used intentionally (tokio::fs lacks this function), or wrap it in `tokio::task::spawn_blocking` if the workspace root is on a slow filesystem.

#### L-2: T12 naming prefix audit — no test or explicit verification artifact

**Location**: Residual `R-V156P1-L003`

**Observation**: T12 ("naming prefix") was an audit-only task to rename capability-internal symbols to follow the `registry_refresh_*` naming convention. The naming is already consistent in the codebase — no renames appear in the diff. The residual is correctly considered resolved, but there is no explicit test or verification step that proves the naming convention is enforced going forward.

**Recommendation**: If this pattern warrants future-proofing, add a compile-time or test-time assertion that all registry-related public symbols follow the naming convention. Low priority.

#### L-3: Capability-registry.md §2.8 references future spec while daemon-runtime.md coordinates cross-reference points to non-existent concurrency spec section

**Location**: `daemon-runtime.md:217`

**Observation**: The V1.58 P0 Draft overlay in `daemon-runtime.md` coordinates with `concurrency.md §7 (per-row OCC)`. While `concurrency.md` exists as a file, the cross-reference accuracy to §7 was not independently verified (the §7 content about "per-row OCC" may or may not match the workspace-session-level OCC described here).

**Recommendation**: Verify during P-last spec hygiene that the concurrency.md cross-reference is semantically correct.

## Verdict Reasoning

**Verdict: APPROVE**

- **Zero High (Critical) findings**: No blocking architecture or maintainability issues.
- **Two Medium findings (M-1, M-2)**: M-1 is a cross-coupling observation between P0 and P1 that does not block this plan's completion — the two `LazyLock<reqwest::Client>` instances are independently correct, and consolidation is best addressed at P-last or in a dedicated WL-A task. M-2 is minor dead_code hygiene in the test binary.
- **Three Low findings (L-1, L-2, L-3)**: All are architectural notes suitable for V1.59 WL-A or P-last hygiene.

The core architecture decisions are sound:
- OCC concern is correctly isolated in `workspace::session` module with clean public API (`commit_session` as transaction guard)
- Path canonicalization (`canonicalize_workspace_root` + `enforce_path_boundary`) is correctly layered and tested with symlink rejection
- TOCTOU is closed by binding validate+consume into a single method call with SQLite atomic UPDATE
- Capability surface hardening (T7–T16) follows existing project patterns (AtomicU64 counters, `tracing` spans, `LazyLock` for singletons)
- All carry-forward residuals are verified resolved in the diff

## Carry-forward Verification

All inherited residuals from V1.56 and V1.57 are confirmed resolved in the diff range `d443e855..af82ad39`:

| Residual | Task | Verification |
|---|---|---|
| R-V156P0-M001 | T1 | `sha2 = "0.10"` in root `Cargo.toml:92` `[workspace.dependencies]`; all 4 using crates reference `sha2 = { workspace = true }` |
| R-V156P0-M002 | T2 | `canonicalize_workspace_root` (line 166) + `enforce_path_boundary` (line 181) in session.rs; symlink check via `symlink_metadata().file_type().is_symlink()` (line 242–248) |
| R-V156P0-M003 | T3 | `crates/nexus-daemon-runtime/tests/workspace_occ_concurrent.rs` — two concurrent consumers asserting single-winner semantics (lines 47–93, 96–148) |
| R-V156P0-M004 | T4 | `compute_content_hashes` uses `tokio::fs::read_dir` + `tokio::fs::File::open` + `AsyncReadExt` (lines 224–274); `compute_single_file_hash` likewise (lines 615–635) |
| R-V156P0-M005 | T5 | `commit_session` (line 576) validates manifest + consumes atomically; `db::consume_session` atomic `UPDATE WHERE consumed = 0` is the CAS primitive |
| R-V156P0-M006 | T6 | `OCC_CONFLICT_TOTAL` AtomicU64 (line 25); `tracing::warn!` with structured `session_id`/`conflict_type` at AlreadyConsumed (line 547–551) and HashConflict (line 589–594) paths |
| R-V156P1-M003 | T7 | `force` parsed from `RegistryRefreshInput` (line 390); logged via `tracing::info!(force, ...)` (line 407) |
| R-V156P1-M004 | T8 | `tracing::info_span!("registry_refresh", force, cdn_configured, %now)` wrapping run() (line 399–405) |
| R-V156P1-M005 | T9 | `static SHARED_CDN_CLIENT: LazyLock<reqwest::Client>` with connection pooling (line 71–76) |
| R-V156P1-L001 | T10 | `registry_refresh_help_text()` documents HTTPS-only + public-internet requirement + `force` semantics (lines 83–91) |
| R-V156P1-L002 | T11 | `CdnConfig.max_body_bytes` with `DEFAULT_MAX_CDN_BODY_SIZE = 8 * 1024 * 1024` (line 159, 315); configurable per-invocation |
| R-V156P1-L003 | T12 | Audit-only — symbols already follow `registry_refresh_*`/`refresh_*` prefix convention; no renames needed |
| R-V156P1-L004 | T13 | `retry_jitter_ms()` returns 100–500ms (line 100–105); applied in exponential backoff (line 546–548) |
| R-V156P1-L005 | T14 | `crates/nexus-orchestration/benches/registry_refresh_latency.rs` with cold + warm benchmarks |
| R-V156P1-L006 | T15 | `now = Utc::now().to_rfc3339()` captured once before retry loop (line 395) |
| R-V156P1-L007 | T16 | `REFRESH_TOTAL`, `REFRESH_SUCCESS_TOTAL`, `REFRESH_FAILURE_TOTAL`, `REFRESH_CACHE_HIT_TOTAL` AtomicU64 counters with pub readers (lines 36–60) |
| R-V156-MIDQA-01 | T17 | `cargo +nightly fmt --all -- --check` passes clean (verified) |
| R-V156-PROCESS-01 | T18 | `.sqlx/` cache hygiene protocol documented in `daemon-runtime.md` §V1.58 P0 Draft overlay (lines 169–211); `--tests` flag critical note included |
| R-V156P1-CACHE-01 | T18 | Same protocol document above — explicitly calls out `--tests` as critical for test query! macros (lines 181–188, 206–211) |
| R-V157P0-L001 | T19 | V1.57 plan `.mstar/plans/2026-06-22-v1.57-spec-governance-and-registry.md` reconciled: line 51, 77, 80 each reference "18 shipped" with reconciliation note; original "35" corrected |
| R-V157P0-L002 | T20 | `registry_refresh_rejects_invalid_input_type` (non-object → InputInvalid), `registry_refresh_rejects_non_boolean_force` (string force → InputInvalid), `registry_refresh_rejects_unknown_field_strictly` (documents serde-default contract) |

## Cross-Plan Concerns

### P0 + P1 integration (merged)

1. **Dual reqwest::Client (M-1 above)**: P0's `SHARED_CDN_CLIENT` and P1's `HTTP_CLIENT` are separate. Functional conflict: none — each capability has its own client with appropriate configuration. Resource efficiency: suboptimal (two connection pools). Not a blocker.

2. **Boot sequence ordering**: P0's CDN config injection (boot.rs:135) happens in Section 4 (Subsystem orchestration), P1's refresh scheduler spawn (boot.rs:544) happens in Section 4e. No ordering dependency issue — the `LazyLock` pattern defers client creation to first use, not boot time.

3. **Metrics naming**: P0 uses `nexus_registry_refresh_*` counter semantics (doc comments at lines 41, 43, 47, 51, 55, 59). P1's reference_refresh metrics (if any) would need to follow a `nexus_reference_refresh_*` convention. Not verified as P1 was not the focus of this review, but worth noting for naming consistency.

### Spec amendments

- `capability-registry.md`: Status correctly states "Master (V1.57 P-last promote)". The V1.58 P0 Draft overlay (lines 240–284) is clearly marked **Status: Draft (V1.58 P0)** with valid plan cross-reference. **✓**
- `daemon-runtime.md`: Status is "Normative" / Master. Both V1.58 P0 Draft overlays (lines 169 and 213) are correctly marked **Status: Draft (V1.58 P0)** with valid plan references and cross-doc coordinates. **✓**
- No Master-to-Draft contamination — all V1.58 P0 additions are Draft overlays within their respective Master documents, preserving the P-last promotion pattern.
