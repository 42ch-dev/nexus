---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-26-v1.67-overdue-debt-closure"
verdict: "Request Changes"
generated_at: "2026-06-27"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: volcengine-plan/ark-code-latest
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-27

## Scope
- plan_id: 2026-06-26-v1.67-overdue-debt-closure
- Review range / Diff basis: P2 code commits (138a98fd..ae1b960e) merged at HEAD; diff basis vs `26e477ee`.
- Working branch (verified): iteration/v1.67
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 9 targeted files plus related specs/test output
- Commit range: 138a98fd..ae1b960e (P2 code commits); diff basis also inspected as 26e477ee..HEAD
- Tools run:
  - `git rev-parse --show-toplevel && git branch --show-current && git rev-parse HEAD && git status --short && git log --oneline -10`
  - `git diff --stat 26e477ee..HEAD`, `git diff --name-only 26e477ee..HEAD`, `git log --oneline --reverse 138a98fd..ae1b960e`
  - targeted `git diff 26e477ee..HEAD -- <reviewed paths>` and baseline `git show 138a98fd:crates/nexus-orchestration/src/capability/builtins/world.rs`
  - targeted reads of `world.rs`, `timeline.rs`, `narrative_write.rs`, `work_chapters.rs`, `script_section_status.rs`, `game_bible_section_status.rs`, `capability/mod.rs`, `capability/builtins/mod.rs`, migration SQL, preset YAML, and concurrency spec
  - greps for `script.section_status.update`, `foreign_key_check`, migration entry points, and capability dispatch paths
  - SQLite reproducer for `PRAGMA foreign_key_check` result/exit behavior
  - `SQLX_OFFLINE=true cargo test -p nexus-orchestration -p nexus-local-db`

## Findings

### 🔴 Critical
None.

### 🟡 Warning
- **W-001 — `PRAGMA foreign_key_check` is present but not enforced as a failing migration gate.**
  - Evidence: `crates/nexus-local-db/migrations/202606230001_work_profile_script.sql:56-60` places `PRAGMA foreign_key_check;` after `ALTER TABLE works_new RENAME TO works`, which is the correct ordering for checking the recreated final table name. However, SQLite's `PRAGMA foreign_key_check` returns result rows for violations; it does not raise an error by itself when the result set is ignored. I verified this with a SQLite reproducer: a database with an intentional dangling FK returned rows for `PRAGMA foreign_key_check`, while `executescript('PRAGMA foreign_key_check;')` completed successfully.
  - Impact: this closes the ordering portion of `R-V160P1-QC2-W002`, but not the "actually fail if corruption exists" reliability requirement. `sqlx::migrate!().run(pool)` will execute the statement as part of the migration stream, but there is no evidence that it consumes the returned rows and converts non-empty output into migration failure. The current fix is therefore diagnostic/no-op as an integrity assertion.
  - Requested fix: make the check fail closed. The most robust option is an explicit Rust-side post-migration check in `run_migrations`/init path that runs `PRAGMA foreign_key_check`, fetches rows, and returns `LocalDbError` when non-empty. If the check must live in SQL migration text, use a construct that causes a SQLite constraint/error when rows exist rather than a bare PRAGMA whose result is ignored. Add a regression test with a deliberately dangling FK on a test database if practical.

- **W-002 — `world.delta.apply` batch pre-fetch removes the per-change SELECT, but the new dynamic IN-list remains unbounded and can fail large delta packages.**
  - Evidence: `crates/nexus-orchestration/src/capability/builtins/world.rs:486-525` now collects all `kb_key_block` update IDs and runs one `SELECT key_block_id, body_json FROM kb_key_blocks WHERE key_block_id IN (...)`. This is a real N+1 read elimination relative to the baseline at `138a98fd:world.rs:491-506`, where each update ran `SELECT body_json ... WHERE key_block_id = ?` inside the loop. The per-change `UPDATE`s remain, which is expected because each change can carry a different field/value.
  - Impact: the input schema has no `maxItems`, and the code neither deduplicates IDs nor chunks the placeholder list. A large agent-proposed delta can generate an oversized SQL statement / too many bind parameters and fail the whole capability before applying any changes. The result set is bounded by the input IDs rather than table size, so it is not an unbounded table scan, but it is unbounded relative to caller input.
  - Requested fix: enforce a bounded package size in the schema and runtime, or chunk/deduplicate `update_kids` before querying. A small cap is consistent with local-first delta packages and prevents replacing an N+1 read with a variable-limit reliability failure.

### 🟢 Suggestion
- **S-001 — `script.section_status.update` is durable for a single writer, but it does not provide file-level OCC/locking for concurrent file edits.**
  - Evidence: `script_section_status.rs:139-158` reads the full file, replaces `section_status`, then writes via shared `atomic_write`; `game_bible_section_status.rs:232-250` implements temp+rename. This prevents torn writes and is safe for the current orchestration capability path's local/single-daemon posture, but it can still overwrite unrelated concurrent edits made between read and rename. This mirrors the existing `game_bible.section_status.update` behavior, so I am not treating it as a new blocker for this P2 closure. If this capability is later exposed to CLI/manual multi-writer paths, wrap it with the repo's work-level advisory lock or add content-hash/OCC semantics.
- **S-002 — Timeline append+explicit-id rename is transactionally grouped, but failure-path coverage could be stronger.**
  - Evidence: `timeline.rs:108-171` begins a transaction, runs collision check, `append_event_in_tx`, optional rename, then commits. Returning before commit drops the transaction and rolls back. The regression test `timeline_event_append_explicit_id_persists_atomically` verifies the success path leaves only the explicit ID and no auto-ID orphan. Consider adding a forced rename/commit failure test if a practical injection seam appears.

## Assigned Item Verification Notes
- **R-V160P0-QC3-W001 (N+1 in `world.delta.apply`)**: Partially closed. The per-update SELECT loop is genuinely removed and replaced by one pre-fetch query, but W-002 remains for unbounded input / dynamic bind-list reliability. The pre-fetch result is bounded by requested IDs, not by table size.
- **R-V160P1-QC2-W002 (migration `PRAGMA foreign_key_check`)**: Partially closed. Ordering is correct post-rename and sqlx migration execution will run the statement, but W-001 remains because a bare `PRAGMA foreign_key_check` does not fail on returned violation rows.
- **R-V160P0-QC2-W002 (atomic timeline append+rename in tx)**: Closed from a code-path perspective. Insert, rename, and commit now share one transaction; early error paths return before commit and transaction drop rolls back. Success-path regression coverage exists.
- **`script.section_status.update` reliability**: Single-writer-local safe and temp+rename durable; concurrent manual/file-writer OCC is not implemented (S-001).
- **No broad performance regression observed**: The reviewed diff reduces the hot `kb_key_block` read path from N SELECTs to one SELECT. Remaining per-row updates are necessary for distinct values. No other new unbounded table scans were identified in the focused P2 diff.

## Source Trace
- W-001: `crates/nexus-local-db/migrations/202606230001_work_profile_script.sql:56-60`; `crates/nexus-local-db/src/lib.rs:261-265`; SQLite reproducer showed `PRAGMA foreign_key_check` returns rows but ignored execution succeeds.
- W-002: `crates/nexus-orchestration/src/capability/builtins/world.rs:486-525` (new dynamic IN-list), `138a98fd:crates/nexus-orchestration/src/capability/builtins/world.rs:491-506` (old per-change SELECT), `world.rs:527-605` (per-change update loop), test `world_delta_apply_batch_kb_updates_prefetch`.
- S-001: `crates/nexus-orchestration/src/capability/builtins/script_section_status.rs:139-158`; `crates/nexus-orchestration/src/capability/builtins/game_bible_section_status.rs:232-250`; `.mstar/knowledge/specs/concurrency.md:20-36` for the broader multi-writer model.
- Timeline tx: `crates/nexus-local-db/src/narrative_write.rs:354-459`, `crates/nexus-orchestration/src/capability/builtins/timeline.rs:108-171`, test `timeline_event_append_explicit_id_persists_atomically`.
- Validation: `SQLX_OFFLINE=true cargo test -p nexus-orchestration -p nexus-local-db` → `nexus-local-db` 273 passed; `nexus-orchestration` unit tests 957 passed / 3 ignored; integration/doc tests completed with all non-ignored tests passing.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes
