---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-16-v1.48-serial-hardening"
verdict: "Request Changes"
generated_at: "2026-06-16"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: k2p7
- Review Perspective: Performance and reliability risk (Reviewer #3, P4 tri-review)
- Report Timestamp: 2026-06-16

## Scope
- plan_id: `2026-06-16-v1.48-serial-hardening`
- Review range / Diff basis: `merge-base: 975899e7895cacc34f4966c1e872c93cac670ace (origin/main pre-V1.48) + tip: 1c70b7c2 (iteration/v1.48 HEAD)`; P4 scope commits `2b75aa81..bfc1f332`
- Working branch (verified): `iteration/v1.48`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 2
  - `crates/nexus-local-db/src/work_chapters.rs`
  - `crates/nexus-local-db/tests/v148_serial_hardening.rs`
- Commit range (P4): `2b75aa81..bfc1f332`
- Tools run:
  - `cargo clippy --all -- -D warnings`
  - `cargo test -p nexus-local-db -- v148_serial` (×3)
  - `cargo test -p nexus42 -- reconcile` (×3)
  - `cargo test -p nexus-local-db --lib test_reconcile_update_and_idempotent`

## Findings

### 🔴 Critical
_None._

### 🟡 Warning

- **W-1 — `reconcile_from_filesystem` performs a full-file rewrite for every status conflict, and the `ReconcileReport` counters no longer reflect file mutations.**
  - `sync_frontmatter_status` reads the entire chapter file, splits it line-by-line, replaces the `status:` key, joins, and writes the whole file back. For a Work with many chapters whose frontmatter disagrees with the DB, this is O(N × file_size) write amplification.
  - More importantly, when only a status conflict is repaired, the existing-row branch falls through to `preserved += 1` (lines 629–631). The file on disk has been modified, but the report claims the row was "preserved unchanged". This misrepresents the operation to the user and to any downstream observability that consumes `ReconcileReport`.
  - **Fix**: Either increment a distinct counter for "frontmatter re-synced" (preferred), or count status-conflict rewrites under `updated`. Also consider a lighter in-place frontmatter edit to avoid rewriting body content.

- **W-2 — `RuntimeLockGuard` is leaked on the daemon reconcile error path, and P4 increases the probability of hitting it.**
  - `crates/nexus-daemon-runtime/src/api/handlers/works.rs::reconcile_chapters` acquires the runtime lock, then calls `reconcile_from_filesystem`. On success it calls `lock.release().await`; on error the early `?` return bypasses release.
  - P4 adds blocking file I/O (`sync_frontmatter_status`) and path-canonicalization failure modes to `reconcile_from_filesystem`, increasing the chance that the daemon will leak the lock and leave the Work unwritable until restart.
  - **Fix**: Restructure the handler with a `match` that releases the lock on both `Ok` and `Err` arms, mirroring the V1.42.1 hotfix pattern codified in `crates/nexus-daemon-runtime/AGENTS.md`.

- **W-3 — `reconcile-chapters` is synchronous and holds the Work runtime lock for the entire filesystem walk + DB + file I/O duration.**
  - The daemon handler and CLI both invoke `reconcile_from_filesystem` inline. With N chapters, the lock is held for N directory entries × (read + optional full-file rewrite + DB round-trips). At 50 ms/chapter, 100 chapters ≈ 5 s of lock-held time, blocking other mutating commands on the same Work.
  - **Fix**: For V1.48 this is acceptable as a manual command, but document the scaling limit and consider batching / async chunked processing in a follow-up. At minimum, do not hold the runtime lock while performing best-effort file rewrites that do not mutate `work_chapters` rows.

### 🟢 Suggestion

- **S-1 — Hermetic test #5 should explicitly assert idempotency (second reconcile is a no-op).**
  - `v148_serial_reconcile_preserves_db_status_and_creates_missing` verifies DB-as-SSOT and missing-row creation, but it does not run `reconcile_from_filesystem` a second time and assert `created == 0 && updated == 0 && preserved == N`.
  - Idempotency is currently demonstrated only by the updated unit test `test_reconcile_update_and_idempotent`. For §4.5.7 #5 acceptance, the hermetic test should also cover it.
  - **Fix**: Append a second reconcile call + assertions to `v148_serial_reconcile_preserves_db_status_and_creates_missing`.

- **S-2 — `sync_frontmatter_status` normalizes line endings and does not preserve original `\r\n` files.**
  - It uses `content.lines()` + `join("\n")`, which converts CRLF to LF. On Windows-checked-out workspaces this is a benign but observable side effect.
  - **Fix**: If cross-platform line-ending preservation is desired, iterate over raw bytes or record original terminators. Low priority for pre-1.0 local-first use case.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W-1 | manual-reasoning | `crates/nexus-local-db/src/work_chapters.rs:399–468` (`sync_frontmatter_status`), `reconcile_from_filesystem` lines 600–632 | High |
| W-2 | manual-reasoning | `crates/nexus-daemon-runtime/src/api/handlers/works.rs:1478–1547` | High |
| W-3 | manual-reasoning | `crates/nexus-daemon-runtime/src/api/handlers/works.rs:1522–1544`, `crates/nexus42/src/commands/creator/works/mod.rs:744–780` | Medium |
| S-1 | manual-reasoning | `crates/nexus-local-db/tests/v148_serial_hardening.rs:100–231` | High |
| S-2 | manual-reasoning | `crates/nexus-local-db/src/work_chapters.rs:421–460` | Medium |

## Validation Evidence

### Lint
```text
$ cargo clippy --all -- -D warnings 2>&1 | tail -10
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.25s
```
**Result:** clean (no warnings/errors).

### `cargo test -p nexus-local-db -- v148_serial` (×3)
All three runs passed with 2/2 hermetic tests green:
- `v148_serial_resume_draft_no_duplicate_row`
- `v148_serial_reconcile_preserves_db_status_and_creates_missing`

Run times: ~0.09–0.10 s for the target binary each run. **Flake assessment:** no flakes observed across 3 runs.

### `cargo test -p nexus42 -- reconcile` (×3)
All three runs completed with `0 passed; 0 failed` — no test name matches the `reconcile` filter in `nexus42`. Reconcile coverage is provided by `nexus-local-db` tests and the updated unit test below.

### `cargo test -p nexus-local-db --lib test_reconcile_update_and_idempotent`
```text
test work_chapters::tests::test_reconcile_update_and_idempotent ... ok
```
The updated unit test correctly verifies: DB status wins over filesystem frontmatter, `word_count` is mirrored, file frontmatter is re-synced to DB status, and a second reconcile is idempotent (`preserved == 3`, `updated == 0`).

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes

P4 correctly implements the §4.5.3 DB-as-status-SSOT semantics and the §4.5.7 #4/#5 test scaffolding. However, three unresolved reliability/performance warnings must be addressed before merge:
1. The `ReconcileReport` must accurately account for frontmatter re-sync operations.
2. The daemon reconcile handler must release the runtime lock on error paths.
3. The synchronous, lock-held reconcile command needs a documented scaling ceiling and preferably lighter I/O.

The two suggestions (idempotency assertion in the hermetic test and CRLF preservation) are optional but recommended.
