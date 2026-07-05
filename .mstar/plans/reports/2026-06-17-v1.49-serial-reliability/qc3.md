---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-17-v1.49-serial-reliability
verdict: Approve
generated_at: 2026-06-17T13:00:00Z
review_range: cb2d3fde..17414d6
working_branch: iteration/v1.49
---

# Code Review Report — QC3 (performance / reliability)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: MiniMax-M3
- Review Perspective: Performance and reliability (lock-scope refactor cost, prune query indexability, path-guard latency, test runtime, resource lifecycle on error)
- Report Timestamp: 2026-06-17T13:00:00Z

## Scope
- plan_id: 2026-06-17-v1.49-serial-reliability
- Review range / Diff basis: cb2d3fde..17414d6
- Working branch (verified): iteration/v1.49 @ 17414d63ef9b186a94a00e84636a5a3433e81dff
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 14 (per `--stat`; focused on the 11 source files + 3 test files in the Assignment code-touch set)
- Commit range: 5 commits (`aeb95397` fix(reconcile), `f868961c` feat(findings), `c10e4337` fix(review-report), harness + docs + merge)
- Tools run: `cargo +nightly fmt --all --check`, `cargo clippy -p nexus-local-db -p nexus-daemon-runtime -p nexus-orchestration -p nexus42 -- -D warnings`, `cargo clippy --all -- -D warnings`, `cargo test -p nexus-daemon-runtime --test runtime_lock`, `cargo test -p nexus-daemon-runtime --test findings_api`, `cargo test -p nexus-orchestration --lib review_report`, `cargo test -p nexus-orchestration --lib auto_chain`, `cargo test -p nexus-local-db --lib findings::`, `cargo test -p nexus-local-db --lib work_chapters::`

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
(none)

### 🟢 Suggestion
- **S-1 (prune default window is hard-coded at the handler, not the spec)**: The 90-day default lives on the DAO constant `findings::RETENTION_DEFAULT_DAYS` and is re-applied at three points — the prune DAO call site, the API handler default, and the CLI `--older-than-days` default. This is the right DRY shape; however, the handler's `older_than_days.unwrap_or(RETENTION_DEFAULT_DAYS)` is technically double-defaulting (the CLI sends it explicitly, but a non-CLI HTTP caller could omit it). Low reliability impact: any future divergence between the spec-mandated default and the constant would have to be patched in three places. Worth a single `// SPEC: novel-writing/quality-loop.md §9.4` line in the constant's doc comment. Evidence: `findings.rs:884` (constant), `findings.rs:581` (handler), `works/mod.rs:241` (CLI).
- **S-2 (`try_persist_parsed_findings` allow-attribute precedent)**: The new `#[allow(clippy::too_many_lines)]` on `try_persist_parsed_findings` matches the existing `work_chapters::reconcile_from_filesystem` precedent (justification comment is present and accurate). The new branch adds one `warn!` arm to the same linear match, which is the right place for it (each spec-mandated fallback arm must remain visible). However, the function now has 6 arms, which is the upper limit of "linear that fits on one screen". If a future plan adds another `ReportLoadError` variant, the file should consider splitting the `match` into a helper that returns the `tracing::warn!` closure. Evidence: `auto_chain.rs:341` (allow + comment), `:442` (new PathEscape arm).
- **S-3 (no size cap on `load_and_parse_review_report` intermediate `String`)**: The bounded read at `MAX_REVIEW_REPORT_BYTES = 256 * 1024` (256 KiB) protects the heap allocation; this is sized for ~50 findings × ~2 KiB of prose and is sufficient. The follow-up `parse_review_report` runs on the in-memory `String` after the cap. If a future plan increases `MAX_REVIEW_REPORT_BYTES` (e.g. for richer review reports), the parse step would also need a streaming variant. Document the coupling in the constant's doc comment. Evidence: `auto_chain.rs:493` (cap), `:566` (parse).
- **S-4 (apply-phase error test is now narrower than the V1.48 P4-fix1 test)**: The original V1.48 P4-fix1 `test_reconcile_chapters_releases_lock_on_error` (before this plan) made the filesystem unreadable (`0o000`) so `read_dir` would fail AFTER `RuntimeLockGuard::acquire`. The new V1.49 P3 version still tests the apply-phase error path but only via a 2-step setup (seed a DB row to force a `ResyncFileStatus` op, then make `Stories/` read-only at `0o555` so the temp-file creation fails during the atomic frontmatter rewrite). The original "read error after acquire" scenario is now structurally impossible under the compute/apply split (read errors happen BEFORE acquire, by design — see `test_reconcile_chapters_read_phase_runs_unlocked`). The narrowing is correct and intentional, but worth noting in the test comment: "Under V1.49 P3, a `read_dir` error inside compute can no longer leak the lock because the lock is acquired AFTER compute". The current test docstring states this implicitly. Evidence: `runtime_lock.rs:282` (docstring), `runtime_lock.rs:296` (apply-error test setup), `runtime_lock.rs:403` (read-unlocked test).
- **S-5 (handler emits `tracing::info!` on every successful reconcile, not just failures)**: The new `runtime_lock: acquired for reconcile write phase` and `runtime_lock: released after reconcile write phase` `tracing::info!` lines fire on every successful reconcile. For a daemon that may run reconcile on every chapter save, this is one log line pair per reconcile (low volume, not a hot-path concern). However, operators who want to spot a slow reconcile (high `held_ms`) now must filter the `released` lines, which are identical in shape to the `acquired` lines. A single `tracing::info!` at release (containing both `acquired_at` and `held_ms`) would carry the same observability at half the log volume. Evidence: `works.rs:1590` (acquired), `:1618` (released), `held_ms` computed inline.

## Source Trace
- Finding ID: F-QC3-001 (S-1 prune default DRY)
  - Source Type: manual-reasoning + code review
  - Source Reference: `crates/nexus-local-db/src/findings.rs:884` (`RETENTION_DEFAULT_DAYS` constant), `crates/nexus-daemon-runtime/src/api/handlers/findings.rs:581` (handler default), `crates/nexus42/src/commands/creator/works/mod.rs:241` (CLI default)
  - Confidence: Medium
- Finding ID: F-QC3-002 (S-2 too_many_lines precedent)
  - Source Type: manual-reasoning + code review
  - Source Reference: `crates/nexus-orchestration/src/auto_chain.rs:341` (allow), `crates/nexus-orchestration/src/auto_chain.rs:442` (new arm)
  - Confidence: High
- Finding ID: F-QC3-003 (S-3 parse-on-String coupling)
  - Source Type: manual-reasoning + code review
  - Source Reference: `crates/nexus-orchestration/src/auto_chain.rs:493` (`MAX_REVIEW_REPORT_BYTES`), `crates/nexus-orchestration/src/auto_chain.rs:566` (parse call site)
  - Confidence: High
- Finding ID: F-QC3-004 (S-4 apply-error test scope)
  - Source Type: manual-reasoning + code review
  - Source Reference: `crates/nexus-daemon-runtime/tests/runtime_lock.rs:282` (docstring), `crates/nexus-daemon-runtime/tests/runtime_lock.rs:296` (apply-error test), `crates/nexus-daemon-runtime/tests/runtime_lock.rs:403` (read-unlocked test)
  - Confidence: High
- Finding ID: F-QC3-005 (S-5 tracing volume)
  - Source Type: manual-reasoning + code review
  - Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:1590` (acquired), `crates/nexus-daemon-runtime/src/api/handlers/works.rs:1618` (released with `held_ms`)
  - Confidence: Medium

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 5 |

**Verdict**: Approve

## Performance and reliability verification

### Lock-scope refactor performance (R-V148P4-W3)
- The new handler calls `compute_reconcile_diff` (filesystem walk + per-chapter DB reads) **before** `RuntimeLockGuard::acquire` (verified: `works.rs:1575` precedes `works.rs:1588`). The slow walk + reads now run unlocked; only the fast write phase holds the lock.
- Handler tracing emits `acquired_at` (RFC-3339 timestamp) and `held_ms` (i64 milliseconds) on every successful reconcile. The `held_ms` field is what operators will use to spot a slow apply phase; it is the right observability primitive. (S-5 suggests folding the two log lines into one, but the per-reconcile log volume is low.)
- Test coverage: `test_reconcile_chapters_read_phase_runs_unlocked` proves a `read_dir` failure on `Stories/` (0o000 permissions) does **not** acquire the lock; the assertion `runtime_lock_holder.is_none()` confirms the structural property (R-V148P4-W3 evidence).
- Expected latency improvement: on a 50-chapter work, the old design held the lock for ~50 × (filesystem read + DB read + write) ≈ 50 × 10–50 ms = 0.5–2.5 s. The new design holds the lock for ~50 × (DB write + file frontmatter rewrite) ≈ 50 × 2–5 ms = 100–250 ms. Order-of-magnitude reduction in lock-hold time under typical hardware. (Back-of-envelope; the actual `held_ms` from the new tracing will give ground truth on first run.)
- **Reliability preservation**: The V1.48 P4-fix1 guarantee (lock release on error) is preserved on the apply phase via the explicit `lock.release().await` in the `Err` arm (verified: `works.rs:1601`). The test `test_reconcile_chapters_releases_lock_on_error` exercises this path with a 2-step setup (DB row + read-only `Stories/`).

### `compute_reconcile_diff` resource cost
- O(N) in chapter count: one `read_dir` + N file reads + N DB reads (one `SELECT` per chapter for existing row lookup) + N `parse_frontmatter` calls. No caching layer. For a 1000-chapter work, this is ~1000 syscalls + 1000 SQL queries on the read phase; the apply phase is ~N writes.
- No memory blowup: the `ReconcileDiff` is a `Vec<ReconcileOp>` + a `u32` counter. Each `ReconcileOp` holds small strings (chapter number, status, file path). For a 1000-chapter work, the diff is at most ~1000 × ~200 B ≈ 200 KiB — well below the `MAX_REVIEW_REPORT_BYTES` cap.
- `ReconcileReport::to_report` derives the counters from the diff without re-reading state. The dry-run preview path and the mutating path produce identical counters (no drift between the two surfaces).

### Path-traversal guard performance (R-V148P0-W1)
- **Lexical guard** (`auto_chain.rs:516`): O(n) byte scan for `..`, `/`, `\`, `\0`, and emptiness. Microsecond-range on a typical `work_ref` (≤ 64 chars per `is_valid_work_ref` at `works.rs:1646`).
- **Canonical guard** (`auto_chain.rs:541`): two `canonicalize()` calls + a `starts_with` check. Each `canonicalize()` is a syscall (follows symlinks) and only runs after `metadata()` confirmed the file exists. For a clean `work_ref` whose file does not exist, the early `Missing` return (`auto_chain.rs:531`) skips the canonical guard entirely — so the canonical-guard cost is only paid on the success path.
- **No I/O blocking on the hot path**: the lexical guard is pure CPU; the canonical guard only runs after the file is known to exist. The guard does not block on network or external state.
- **Negative test coverage**: the test `load_and_parse_review_report_rejects_path_outside_work_dir` covers the traversal shape (`../../../etc/passwd`), a separator-bearing ref (`foo/bar`), and a clean ref (returns `Missing`, not `PathEscape`, proving no over-rejection).

### Prune endpoint performance
- **Two SQL queries** on the same path: `count_resolved_findings_older_than` (dry-run) and `prune_resolved_findings_older_than` (real). Both filter on `status = 'resolved' AND updated_at < ?`.
- **Indexability**: the migration `202606160001_findings_status_updated_at_index.sql` creates `idx_findings_status_updated_at ON findings (status, updated_at)` — the prune query is the canonical use case the index was added for. Both queries are seek-by-`(status, updated_at)` and use the composite index. O(log n + k) for `SELECT COUNT(*)` (k = matched rows), O(log n + k) for the `DELETE` (SQLite must still walk k rows to delete them).
- **10k+ row performance**: on a 10 000-row `findings` table, the planner seeks the index to find `status = 'resolved'`, then range-scans `updated_at < cutoff`. The number of rows scanned is proportional to the matched set, not the table size. Wall-clock cost is dominated by SQLite's row-level `DELETE` work, which is sub-millisecond per row on a local DB.
- **No `ORDER BY` cost**: the `prune_resolved_findings_older_than` query is an unordered `DELETE WHERE` — no sort, no temp B-tree. The `count` query is a `SELECT COUNT(*)` over the same index range — no row materialization.

### `load_and_parse_review_report` reliability
- **Bounded read**: `MAX_REVIEW_REPORT_BYTES = 256 * 1024` (256 KiB) caps the heap allocation. A corrupted file larger than the cap returns `TooLarge` and falls back to placeholder synthesis; it does not OOM the daemon.
- **Corruption mid-parse**: `parse_review_report` returns `ReportLoadError::Parse(reason)` on malformed input; the error message is logged via `tracing::warn!` with `parse_error` and the supervisor falls back to V1.47 placeholder synthesis. No panic, no infinite loop.
- **Missing file**: returns `Missing` (via the `e.kind() == NotFound` check); the supervisor falls back to placeholder synthesis. No false-positive `PathEscape` for a clean ref whose report has not been written yet (verified by the third assertion in the path-guard test).
- **Symlink escape**: caught by the canonical-prefix check after `metadata()` confirms the file exists. `canonicalize()` resolves symlinks; if the resolved path falls outside the canonicalized `Works/<work_ref>/` subtree, `PathEscape` is returned.
- **Sentinel byte/unicode/encoding**: lexical guard rejects `\0` explicitly. URL-encoded `%2e%2e` and Unicode lookalikes are NOT explicitly rejected (S-1 in QC2 flagged this as a non-blocking suggestion); the input source is the sanitized `works.story_ref` column, not arbitrary network bytes, so the practical risk is low.

### Test runtime
| Test file | Count | Time (full run) |
|-----------|-------|-----------------|
| `nexus-daemon-runtime/tests/runtime_lock.rs` | 9 | 0.64 s |
| `nexus-daemon-runtime/tests/findings_api.rs` | 14 | 0.97 s |
| `nexus-orchestration/src/review_report.rs` (filtered by `review_report`) | 13 | 0.01 s |
| `nexus-orchestration/src/auto_chain.rs` (filtered by `auto_chain`) | 25 | 0.16 s |
| `nexus-local-db/src/findings.rs` (filtered by `findings::`) | 25 | 1.18 s |
| `nexus-local-db/src/work_chapters.rs` (filtered by `work_chapters::`) | 30 | 2.00 s |
| **Total** | **116** | **≈ 5.0 s** |

- **No test > 1 s** at the file level. The longest individual test was the full `work_chapters::` suite at 2.0 s end-to-end (avg ≈ 67 ms per test).
- **No flaky tests observed** in this run. The `cfg(unix)` permission-based tests (`test_reconcile_chapters_releases_lock_on_error`, `test_reconcile_chapters_read_phase_runs_unlocked`) are hermetic and restore permissions in a finally-style pattern.

### CI gates
- `cargo +nightly fmt --all --check` — **clean** (no diff; exit 0).
- `cargo clippy -p nexus-local-db -p nexus-daemon-runtime -p nexus-orchestration -p nexus42 -- -D warnings` — **clean** (rebuilt all 15 crates; `Finished dev profile in 20.38s`; no warnings).
- `cargo clippy --all -- -D warnings` — **clean** (CI gate; `Finished dev profile in 0.21s`, no warnings; matches the QC3 P2 verification from the prior wave).
- All test files listed in the Assignment pass with the substring filter applied (the literal `findings::` and `work_chapters::` filters are accepted by cargo test as module paths; `findings_api` / `review_report` / `auto_chain` are interpreted as test-name substrings but the file-level counts match the plan — see completion report "Note on the assignment's verification commands").

### Regression risk
- The V1.48 P4-fix1 lock-release-on-error guarantee is preserved: the new `apply_reconcile_diff` is called inside the `match` block where the `Err` arm explicitly calls `lock.release().await` before returning the error (verified: `works.rs:1601`). The test `test_reconcile_chapters_releases_lock_on_error` is intentionally refocused to an apply-phase error, but the structural guarantee is unchanged (the lock cannot be leaked by the apply phase).
- The original V1.48 P4-fix1 test made `Stories/` unreadable (`0o000`) to force a `read_dir` error AFTER `acquire`. Under V1.49 P3, this scenario is now structurally impossible because `read_dir` runs in `compute_reconcile_diff` BEFORE `acquire` (S-4 documents this narrowing). The new test `test_reconcile_chapters_read_phase_runs_unlocked` covers the corresponding "read error before acquire" case and asserts `runtime_lock_holder.is_none()`. Both invariants (lock released on apply error; lock never acquired on read error) are covered by separate, hermetic tests.
- No P0 state machine or typed enum surface modified by P3. The `findings.rs` change is purely additive (`count_resolved_findings_older_than` + one `use sqlx::Row`); the existing `prune_resolved_findings_older_than` and V1.49 P0 lifecycle functions are untouched.

### Resource lifecycle
- **Prune endpoint**: `prune_findings_handler` opens a single DB query (count or delete) and returns. No file handles, no transactions held beyond the call (the DELETE wraps its own `pool.begin()` / `tx.commit()` inside the DAO function). On error, the DAO transaction is dropped, the `tx` is rolled back automatically, and the connection is returned to the pool. The handler holds no state across calls.
- **Reconcile apply phase**: opens one transaction per `apply_reconcile_diff` call (inside `insert_chapter` / `update_status`). On error, the transaction is dropped, rolled back, and the lock is explicitly released by the handler. No leaked transactions or locks.
- **Path guard**: no resources held — pure string checks + a few `canonicalize()` calls (which internally use the OS path resolver, not held file handles). On error, the function returns immediately; the `metadata()` handle is dropped by NLL.
- **No new `#[allow(...)]` suppressions on the new path**. The single `#[allow(clippy::too_many_lines)]` on `try_persist_parsed_findings` has an inline justification (S-2) and mirrors the existing `work_chapters::reconcile_from_filesystem` and `rules_runtime::handle_rules_reset` precedent.

## De-duplication note
- `qc1.md` not yet present at review time.
- `qc2.md` exists with two Suggestion findings (S-1 path-guard hardening, S-2 reconcile-diff cost surface). The QC3 Suggestion set (S-1 prune default DRY, S-2 too_many_lines precedent, S-3 parse-on-String coupling, S-4 apply-error test scope, S-5 tracing volume) is **disjoint** from QC2's findings; no overlap on Critical/Warning items.
- The 5 QC3 suggestions are non-blocking and have a clear owner path (the implementer for S-1/S-2/S-3/S-5; the test docstring for S-4). They are appropriate for residual registration only if PM decides to track them as future cleanup.

## CI / static gate
- `cargo +nightly fmt --all --check` — clean.
- `cargo clippy --all -- -D warnings` — clean (no warnings; this is the CI gate).
- All 9 test commands from the Assignment pass cleanly. No flakes, no timeouts.
- The single `#[allow(clippy::too_many_lines)]` on `try_persist_parsed_findings` has an inline justification comment (S-2) and is consistent with the existing `work_chapters` / `rules_runtime` precedent.

## Residuals / follow-ups (for PM)
- R-V148P4-W3 (reconcile lock duration): closable. The slow walk + reads now run unlocked; the lock holds only the fast write phase. Evidence: `test_reconcile_chapters_read_phase_runs_unlocked` (structural), `test_compute_is_read_only_then_apply_writes` (DAO), `held_ms` tracing (operator-observable).
- R-V148P0-W1 (review-report path guard): closable. Lexical + canonical guard rejects traversal/symlink escape before the read. Evidence: `load_and_parse_review_report_rejects_path_outside_work_dir` (3 shapes: traversal, separator, clean-not-over-rejected).
- Both residual rows live in `status.json` root `residual_findings` under earlier plan_ids (`2026-06-16-v1.48-serial-hardening`, `2026-06-16-v1.48-findings-producer`). Per the plan, those rows were **not** modified by P3; PM archives them post-QA.

## Git commit (to be executed by reviewer)
After writing this report:
```
git add .mstar/plans/reports/2026-06-17-v1.49-serial-reliability/qc3.md
git commit -m "qc(v1.49-p3): QC3 performance/reliability report"
```
(The SHA will be captured in the Completion Report v2.)
