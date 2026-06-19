---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-19-v1.52-outline-five-q-and-auto-promote"
verdict: "Approve"
generated_at: "2026-06-19"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Performance and reliability risk (hot-path cost, resource lifecycle, unbounded operations, observability, failure-mode behavior)
- Report Timestamp: 2026-06-19

## Scope
- plan_id: 2026-06-19-v1.52-outline-five-q-and-auto-promote
- Review range / Diff basis: b97ec0d9..431aca4c
- Working branch (verified): feature/v1.52-outline-five-q-and-auto-promote
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.52-ta-p0/
- Files reviewed: 18 changed (1327 insertions, 192 deletions)
- Commit range: b97ec0d9..431aca4c (1 implement commit `431aca4c`; `2425b12b` is harness-only PM signoff and contains no diff in source files)
- Tools run:
  - `git rev-parse --show-toplevel` + `git branch --show-current` (alignment)
  - `git diff b97ec0d9..431aca4c --stat`
  - Targeted `git diff` for `quality_loop.rs`, `tasks/mod.rs`, `kb.rs`, `kb_extract_job.rs`, `preset/mod.rs`, `preset.yaml`, `outline-exit.md`, migration SQL
  - `cargo test --no-run` to discover test binaries
  - Unit tests via direct binary: `target/debug/deps/nexus_orchestration-*` for `outline_five_q` (4/4 pass) and `llm_extract` (16/16 pass)
  - Integration tests via direct binary: `target/debug/deps/creator_world_kb-* adopt_auto_promote` (2/2 pass)
  - `cargo clippy --all -- -D warnings` (clean — 0 diagnostics)
  - Reference reads: `mstar-review-qc`, `mstar-branch-worktree`, `mstar-roles/references/qc-specialist-shared.md`, `nexus-local-db/AGENTS.md`, `nexus-orchestration/AGENTS.md`, `nexus42/AGENTS.md`, `kb_extract_job.rs` SELECT/index inventory, `tasks/mod.rs` `llm_judge` throttle path

## Findings
### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
- **S1 (observability, low)**: `kb_adopt_auto` only emits `tracing::warn!` on audit-log write failure; there is no `tracing::info!` summary at the end of the batch (`world_id`, `promoted_count`, `skipped_count`). For a CLI that may process up to ~500 candidates (`DEFAULT_PENDING_LIMIT.clamp(1, 500)`), an end-of-batch info log would aid log aggregation in production deployments. Acceptable for V1.52 P0 (the `--json` payload already carries the same numbers); noted as future enhancement.
- **S2 (hot-path allocation, low)**: `outline_five_q_check` allocates a new lowercased `String` via `trimmed.to_ascii_lowercase()` per call. At ~2000-char outlines and 35 signal words this is ~70k comparisons and one ~2 KB allocation per gate. At the stated 100 works × 24 chapters = 2400 gates per run, total allocation cost is on the order of 5 MB and remains sub-millisecond per gate. Could be optimized with a stack-allocated case-insensitive matcher if the function is ever used as a hot loop, but **the current code path never invokes this function in production** — only `llm_judge` does (see S3). Suggestion is forward-looking.
- **S3 (dead-code maintenance hazard, low)**: `outline_five_q_check` is declared `pub fn` and exported from `nexus_orchestration::quality_loop`, but **has no production caller**. The plan §7.1 documents it as "the pure function is used for deterministic tests and as a no-worker fallback signal," yet the orchestration engine's `llm_judge` path (`tasks/mod.rs:894-979`) does **not** consult it as a fallback when the `judge.llm` capability is unavailable — it logs a warning and returns `WaitForInput`. Today the function is reachable only from its own unit tests. This is acceptable for the P0 scope (the LLM-driven path is the production gate), but it creates a maintenance hazard: a future reader may assume the heuristic is wired in. Recommend either (a) explicitly wire it as a worker-unavailable fallback in the `llm_judge` state evaluator, or (b) gate it with `#[cfg(any(test, feature = "outline-heuristic"))]` and document its status in the module docstring.
- **S4 (per-promotion SQL lookup, low)**: `write_auto_promoted_log` calls `resolve_work_ref_for_log` for every promoted candidate, and each call issues a fresh `SELECT story_ref FROM works WHERE work_id = ?` against the PK index. For N=500 promotions this is N single-row PK lookups (each ~tens of µs), so total wall-clock cost is on the order of tens of milliseconds. Could be batched with a single `WHERE work_id IN (...)` and a `HashMap<String, String>` lookup if the limit ever grows past ~500. Not material at current scale.
- **S5 (audit log durability, low)**: Audit log writes use synchronous `std::fs::write` without an explicit `fsync`. The DB transaction commits with SQLite's `fsync` semantics, so a system crash between audit-log write and DB commit can leave an "orphan" audit log describing a promotion that's not visible to a subsequent DB read. Best-effort durability is documented in the `kb_reject` path and is consistent here. Acceptable for V1.52 P0; a hardened variant could `sync_all()` after write.
- **S6 (auto-promote batch is sequential, low)**: `kb_adopt_auto` iterates `pending` candidates sequentially — one `pool.begin()` / INSERT / UPDATE / `commit` per iteration. SQLite serializes writers anyway (WAL or rollback journal), so true parallelism would not help on a single connection; multi-connection pools would be needed to parallelize, which is intentionally out of scope for local-first. At the current `clamp(1, 500)` upper bound, sequential processing completes in well under a second on commodity hardware. No action required; documented for completeness.

## Source Trace
- Finding ID: (N/A — no blocking findings)
- Source Type: manual code review + targeted test execution + clippy
- Source Reference:
  - `outline_five_q_check` (`crates/nexus-orchestration/src/quality_loop.rs:1212-1335`) — pure function, no caller outside tests
  - `outline_review` preset state (`crates/nexus-orchestration/embedded-presets/novel-writing/preset.yaml:98-105`) — `kind: llm_judge`, `min_interval: "PT6H"`
  - `llm_judge` executor (`crates/nexus-orchestration/src/tasks/mod.rs:894-979`) — no heuristic fallback
  - `run_llm_extract` (`crates/nexus-orchestration/src/quality_loop.rs`) — shared by review-time hook + `LlmExtractTask::evaluate`
  - `LlmExtractOutcome` enum (`crates/nexus-orchestration/src/quality_loop.rs`) — three-way result
  - `mark_auto_promoted_in_tx_with_cas` (`crates/nexus-local-db/src/kb_extract_job.rs:1095`) — CAS UPDATE + audit columns in single tx
  - `kb_adopt_auto` (`crates/nexus42/src/commands/creator/world/kb.rs:892`) — per-candidate transaction loop
  - `write_auto_promoted_log` (`crates/nexus42/src/commands/creator/world/kb.rs:1106`) — `std::fs::write` per promotion
  - `resolve_work_ref_for_log` (`crates/nexus42/src/commands/creator/world/kb.rs:1459`) — single-row PK lookup per call
  - Migration `202606190002_kb_extract_jobs_auto_promote.sql` — three nullable `ALTER TABLE ADD COLUMN`
  - Index `idx_kb_extract_jobs_promotion_status_world` (`migrations/202606180003_kb_extract_jobs_extend.sql:63`) — covers `list_pending_for_world`
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 6 |

**Verdict**: Approve

## Detailed Performance / Reliability Review (per assignment)

### 1. Outline 五问 gate hot path
- **Cost profile**: The `outline_five_q_check` pure function performs a constant 35-signal `String::contains` scan (lowercased once) plus line counting and a suffix check. For a 2000-character outline this is on the order of 70k comparisons and a single ~2 KB `String` allocation. Sub-millisecond per call.
- **Production invocation**: The preset wires the `outline_review` state via `kind: llm_judge` (LLM call), **not** via the heuristic function. The LLM path is throttled by `min_interval: "PT6H"` (`tasks/mod.rs:922-960`) — at most one judge call per chapter per six hours, regardless of how many times the state is re-entered. At 100 works × 24 chapters = 2400 gate windows per run, **the LLM ceiling is ≤2400 calls per 6-hour window, not per run**, which is the intended throttle.
- **Hot-path overhead avoided**: No new LLM call or DB query is introduced per chapter on the happy path beyond the existing single `judge.llm` invocation. The throttle behavior is unchanged from finalize 五问.
- **Caching opportunity**: None material; the LLM judge result is cached in `context._judge_result` for the duration of the schedule's `outline_review` visits inside the throttle window.

### 2. `run_llm_extract` shared helper
- **Caching**: The helper awaits the `nexus.llm.extract` capability per invocation. Callers (`extract_kb_candidates_for_review` and `LlmExtractTask::evaluate`) historically had duplicated input shapes; the unification removes that duplication but does not change call frequency. No new IPC call is introduced.
- **Worker pool saturation**: The helper awaits the capability and returns; saturation risk is bounded by the existing registry's queueing (out of scope for this diff). No new concurrency surface introduced.
- **`LlmExtractOutcome` disambiguation**: `Candidates(vec)` / `WorkerUnavailable` / `CapabilityError(reason)` cleanly distinguishes "no worker" from "zero candidates". This is a correctness improvement and removes a silent fallback contract (R-V151Q3-W002). Tracing distinguishes debug (worker unavailable) from warn (capability error), which is correct severity mapping.

### 3. Auto-promote batch (`kb_adopt_auto`)
- **Sequential vs parallel**: Iterates `list_pending_for_world` results in a single `for` loop; no `futures::join_all`, `buffer_unordered`, or similar. Each iteration is its own transaction (`pool.begin()` → INSERT → CAS UPDATE → `commit` or `rollback`).
- **Lock contention**: Per-candidate transactions minimize blast radius — at most one row in `kb_extract_jobs` and one row in `kb_key_blocks` is touched per transaction. The pre-existing `idx_kb_extract_jobs_promotion_status_world` index covers `list_pending_for_world`; no new index needed.
- **Latency at N=100**: ~100 separate transactions × ~1 ms each (SQLite WAL, in-process) ≈ ~100 ms total, dominated by `fsync` on `COMMIT`. Acceptable for a CLI command; not a hot path.
- **Latency at N=500**: upper bound from `clamp(1, 500)` → ~500 ms ceiling. Still acceptable.
- **Mid-batch failure behavior**: Each iteration's transaction is atomic; on `Duplicate`, `Validation`, `CAS` failure, or commit failure, `tx.rollback()` is called, the candidate is reported in the `skipped` array, and the loop continues. **No mid-batch abortion**; partial success is explicitly surfaced via `promoted[]` / `skipped[]` in the JSON output.

### 4. CAS retry / thundering herd
- **No in-process retry loop**: `mark_auto_promoted_in_tx_with_cas` performs exactly one `UPDATE` per candidate. If the CAS fails (status flipped by another process, or `version` advanced), the candidate is rolled back and skipped.
- **Retry mechanism**: The author invokes `creator world kb adopt --auto` again; any still-`pending` candidate meeting criteria will be re-tried. There is no exponential-backoff logic needed because there is no in-process retry loop.
- **Thundering herd risk**: Concurrent `--auto` invocations on the same world will race on the same pending rows. The CAS version guard (`version = ?`) serializes the flips; only one writer succeeds per row. Other writers see either `version != expected` (→ `VersionMismatch`) or `promotion_status != 'pending'` (→ `Ok(false)` — already promoted). Both paths are handled and recorded in `skipped[]` with a clear reason. No risk of duplicate promotion.
- **Connection pool**: `pool.begin()` borrows a connection from the sqlx pool per transaction; on `commit`/`rollback` the connection is returned. No pool exhaustion at N=500 because each transaction is short-lived and pool size scales with `SqlitePool::connect` defaults.

### 5. Migration performance
- **DDL cost**: `202606190002` adds three nullable `TEXT` columns with no `DEFAULT` and no `NOT NULL`. SQLite `ALTER TABLE ADD COLUMN` of nullable text is O(1) on the schema and effectively O(rows) for the catalog update; on the data side, existing rows see `NULL` without a rewrite.
- **Lock duration**: SQLite takes a brief EXCLUSIVE lock for the ALTER, blocking other writers for milliseconds. No data backfill, no constraint changes, no index changes.
- **Index impact**: No new indexes needed for the auto-promote batch path (the existing `(promotion_status, world_id, created_at)` index from `202606180003` covers the dominant list query). The three new columns are not part of any current query path. No index on `auto_promoted_at` is required at this scope; a future "list auto-promoted candidates" query could benefit, but no such query exists in the diff.
- **Backfill**: None required. Existing rows remain valid; columns default to `NULL`.

### 6. Resource lifecycle
- **File handles**: `std::fs::create_dir_all` opens and closes the path resolution immediately. `std::fs::write` opens, writes, closes per audit log entry. No long-lived FDs.
- **LLM worker handle**: The capability is invoked via `cap.run(input).await`; the handle is owned by the registry (`Arc`) and not consumed. No leak.
- **SQLite connection pool**: `pool.begin()` returns a `sqlx::Transaction<'_, Sqlite>`. On `commit`/`rollback`/`Drop` the connection is returned to the pool. No connection leak even on early returns via `?`.
- **Audit log directory**: `create_dir_all` is idempotent and cached by the filesystem after first success. No per-call syscall cost after first invocation.

### 7. Observability
- **Tracing levels**:
  - `tracing::debug!` for worker-unavailable (low signal; common in hermetic tests).
  - `tracing::warn!` for capability errors, missing work_ref resolution, audit log write failure.
  - No `tracing::info!` for end-of-batch summary → see S1.
- **Audit log rotation**: Not implemented (logs accumulate in `Works/<work_ref>/Logs/kb/auto-promoted/YYYY-MM-DD-<job_id>.md`). Per-day file naming gives natural rotation once a day passes; long-running workspaces will accumulate files. Acceptable for V1.52 P0; a future P-1 could add a retention policy.
- **Counter for total auto-promotions per session**: None — `tracing::warn!` per failure, but no aggregate counter. The `--json` payload provides the count for the current invocation; cross-invocation aggregation is delegated to log scraping or a future metric.

### 8. Failure modes
- **Empty world**: `list_pending_for_world` returns `vec![]` → `promoted_count = 0`, `skipped_count = 0`, clean exit.
- **Cross-author attempt**: `require_world_owner` fails first → `CliError::Api { status: 403, message: containing WORLD_KB_FORBIDDEN_CODE }`. Test `adopt_auto_promote_cross_author_returns_403` asserts this.
- **Mid-batch validation failure**: `insert_key_block_in_tx` returns `Err(KbStoreError::Validation | KbStoreError::Duplicate)` → `tx.rollback()`, candidate recorded in `skipped[]`, loop continues.
- **Mid-batch CAS failure**: `mark_auto_promoted_in_tx_with_cas` returns `Ok(false)` (already not pending) or `Err(VersionMismatch)` → both paths roll back and skip.
- **Commit failure**: Returns `CliError::Other` for the specific candidate; loop continues.
- **System crash mid-batch**: Per-candidate transactions ensure partial success is recoverable. Re-running `--auto` after restart is idempotent for already-promoted rows (CAS will skip them) and continues with the remaining pending rows.
- **Audit log write failure**: Non-fatal `tracing::warn!` (consistent with `kb_reject`). Promotion outcome is unaffected.

## Verification Evidence
- All four `outline_five_q` unit tests passed via direct binary: `outline_five_q_passes_on_complete_outline`, `outline_five_q_fails_on_empty_outline`, `outline_five_q_fails_without_arc_or_hook`, `outline_five_q_detects_hook_via_question` (0.01 s).
- All 16 LLM-extract tests passed via direct binary (`llm_extract_*` and `tasks::tests::llm_extract_*`), including the new `llm_extract_unified_path_uses_quality_loop_mapping` regression test for R-V151Q3-W001 (0.02 s).
- Both `adopt_auto_promote` integration tests passed via direct binary (`adopt_auto_promote`, `adopt_auto_promote_cross_author_returns_403`) (0.13 s).
- `cargo clippy --all -- -D warnings` produced zero diagnostics.
- Manual review confirms per-candidate transaction isolation, CAS discipline, ordering of `require_world_owner` before any mutation, and migration additive-only shape.

## Revalidation Notes
N/A (initial tri-review wave).

---

## Completion Report v2

**Agent**: qc-specialist-3
**Task**: V1.52 T-A P0 tri-review (qc3) — performance/reliability focus on outline 五问 gate + KB auto-promote
**Status**: Done
**Scope Delivered**: Full review of diff range `b97ec0d9..431aca4c` in worktree `.worktrees/v1.52-ta-p0/`. Verified alignment fields. Executed targeted unit + integration tests via direct binary invocation (clippy + test discovery first to avoid full `--all` rebuild per `AGENTS.md` daily-iteration guidance). Manual analysis of outline 五问 gate hot path, `run_llm_extract` shared helper, `kb_adopt_auto` batch loop, CAS retry discipline, migration cost, resource lifecycle, observability, and failure modes. Produced qc3.md report.
**Artifacts**:
- Report: `.mstar/plans/reports/2026-06-19-v1.52-outline-five-q-and-auto-promote/qc3.md`
- Git commit of report (see below)
**Validation**:
- `cargo clippy --all -- -D warnings`: clean (0 diagnostics)
- 4/4 `outline_five_q` unit tests pass
- 16/16 LLM-extract unit tests pass (incl. `llm_extract_unified_path_uses_quality_loop_mapping`)
- 2/2 `creator_world_kb::adopt_auto_*` integration tests pass
- No Critical or Warning findings; six low-impact Suggestions recorded
**Issues/Risks**: None blocking. Suggestions S1-S6 are forward-looking enhancements (observability, allocation micro-opt, dead-code hygiene, per-promotion SQL batchability, audit log durability, batch parallelism).
**Plan Update**: N/A (reviewer does not mutate plans)
**Handoff**: Report committed per workflow. PM may now proceed to consolidate tri-review (`qc-consolidated.md`) and route to QA for verification, or trigger targeted re-review if other QC seats raise items.
**Git**: `d09597e0 qc(v1.52-ta-p0): qc3 performance/reliability review` (`git log -1 --oneline` on `feature/v1.52-outline-five-q-and-auto-promote` after `git add` + `git commit` of `.mstar/plans/reports/2026-06-19-v1.52-outline-five-q-and-auto-promote/qc3.md`).