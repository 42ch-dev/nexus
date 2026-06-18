---
report_kind: qc_review
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-18-v1.51-llm-extraction
verdict: Request Changes
generated_at: 2026-06-18T15:40:00Z
---

# Code Review Report

## Reviewer Metadata

- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: MiniMax-M3
- Review Perspective: Performance and reliability risk (LLM call latency, worker pool exhaustion, resource lifecycle, DB write idempotency, capability-registry lookup cost, fallback observability, migration safety, V1.50 regression)
- Report Timestamp: 2026-06-18T15:40:00Z

## Scope

- plan_id: `2026-06-18-v1.51-llm-extraction`
- Review range / Diff basis: `iteration/v1.51...HEAD` (= `ca494f03...deed03ff`)
- Working branch (verified): `feature/v1.51-llm-extraction`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.51-t-a-p0`
- Files reviewed: 25 changed (per `git diff --stat`); production boot/wiring paths reviewed in full
- Commit range: `ca494f03...deed03ff` (8 implementation + style commits, +2292/-91)
- Tools run:
  - `cargo test -p nexus-orchestration --lib llm_extract` — **15 passed; 0 failed** (11 builtin + 4 `LlmExtractTask`)
  - `cargo test -p nexus-orchestration --test novel_review_master` — **3 passed; 0 failed** (`review_master_llm_path_writes_llm_payload`, `review_master_llm_path_is_idempotent`, `review_master_no_registry_falls_back_to_heuristic`)
  - `cargo test -p nexus42 --test creator_world_kb_adopt` — **3 passed; 0 failed**
  - `cargo clippy -p nexus-orchestration --lib -- -D warnings` — clean for V1.51-introduced code (pre-existing `tests/review_report.rs` `doc_markdown` warnings exist on `iteration/v1.51` HEAD and are **out of V1.51 diff scope**)
  - `cargo +nightly fmt --all --check` — clean
  - Manual review of every `CapabilityRegistry` constructor call site (1 boot, 1 test_support, 7+ daemon-runtime handlers)

## Findings

### 🔴 Critical

#### F-001 — Production daemon wires `with_builtins()` (no worker provider), so `nexus.llm.extract` ALWAYS returns `WorkerUnavailable` in production; R-V150KBED-01 is not actually resolved in production.

**Evidence (concrete, traced from boot to hook):**

1. **`crates/nexus-daemon-runtime/src/boot.rs:124`** constructs the daemon's capability registry with **no worker provider**:
   ```rust
   let capabilities = Arc::new(CapabilityRegistry::with_builtins());
   ```
   `with_builtins()` is the registry constructor that explicitly does NOT inject a `WorkerHandleProvider` — it builds every LLM builtin (including `LlmExtract`) in **standalone / test mode**:
   ```rust
   // crates/nexus-orchestration/src/capability/mod.rs:148-184 (with_builtins)
   Box::new(builtins::LlmExtract::new()),   // <-- workers: None
   ```
2. The daemon's `WorkerManager` is created at **`boot.rs:201`** (`let workers = Arc::new(WorkerManager::new());`), stored on `state`, but **never** wired into `CapabilityRegistry::with_runtime_deps(...)`. `WorkerManager` does not implement `WorkerHandleProvider` anywhere in the codebase (verified via `grep -rn "impl WorkerHandleProvider" crates/ --include="*.rs"` — only test mock impls exist).
3. The same `with_builtins()` registry is threaded into the supervisor via `state.set_capability_registry(capabilities)` (`boot.rs:205`) and `with_capability_registry(reg)` (`boot.rs:227`). The V1.51 diff that introduced this wiring (`boot.rs` +12/-4) only forwards the existing standalone registry — it does **not** change the registry construction to `with_runtime_deps(...)` with a worker provider.
4. **Production invocation path**:
   - `ScheduleSupervisor::on_schedule_terminal` (supervisor.rs:517) calls `quality_loop::extract_kb_candidates_for_review(&pool, schedule_id, ws_path, reg)` with the standalone registry.
   - `extract_via_llm` (`quality_loop.rs:347`) resolves `nexus.llm.extract` from the registry and calls `cap.run(input).await`.
   - `LlmExtract::run` (`llm_extract.rs:126-129`) immediately checks `self.workers.as_ref().ok_or(CapabilityError::WorkerUnavailable)?` — because `workers` is `None`, every production invocation returns `Err(WorkerUnavailable)`.
   - The hook catches `WorkerUnavailable` and returns `LlmExtractOutcome::Fallback(...)` → falls back to the V1.50 heuristic (`extract_candidates_from_text`).
5. **Net production behavior**: in production, every `novel-review-master` schedule completion runs the heuristic fallback path. The new `nexus.llm.extract` LLM pathway is **never executed** in production, even with a fully-configured worker manager. The shipped code is functionally equivalent to V1.50 with respect to `block_type` extraction.

**Why this is a performance/reliability Critical (not just security/correctness):**

- **Performance**: the plan's headline win ("LLM-judged `block_type` for every chapter") is invisible to the user. Authors still see every heuristic-extracted candidate as `block_type='character'` and still have to correct on adopt for every location/organization/event — the exact failure mode that R-V150KBED-01 was opened for. The "novel-review-master" preset's `requires_capabilities: [..., nexus.llm.extract]` will validate at enqueue (the capability is registered), but the capability is permanently in test mode.
- **Reliability**: the `WorkerUnavailable` path is exercised on **every** production review, not on the documented "no worker configured" branch. There is no observability signal that this is happening in the daemon's normal path — the only log is `tracing::debug!("kb-extract: falling back to heuristic extraction", reason = "nexus.llm.extract worker unavailable")` per terminal transition, which an operator could easily mistake for a one-off race.
- **R-V150KBED-01 closure is wrong**: `status.json` flips the residual to `lifecycle: "resolved"` with `closure_evidence` listing the capability + task + hook swap. In production, **none** of that machinery is reachable. This is a closure of a defect against test code, not production.

**Suggested fix (non-binding, for PM/architect to weigh):**

Either (a) add a production `WorkerHandleProvider` implementation backed by `WorkerManager` and call `CapabilityRegistry::with_runtime_deps(&deps)` in `boot.rs`, then move the `let capabilities = ...` line below the `WorkerManager` construction so the provider is wired, OR (b) if the production wiring is intentionally deferred, demote `R-V150KBED-01` back to `lifecycle: "deferred"` with a concrete target plan_id and add a `medium` residual that explicitly tracks the production wiring as the missing piece.

**Operational regression to flag** (separate from F-001):

The V1.51 completion report claims:
> "Daemon boot passes the registry to the supervisor so production runs the LLM pathway. Falls back to the heuristic when the worker is unavailable (llm-extract.md §5.1)."

The second clause is true; the first clause is not. The wiring step that would make the first clause true is missing.

### 🟡 Warning

#### W-001 — `LlmExtractTask::evaluate` is `pub` but the review-time hook bypasses it and calls the capability directly (V1.50 plan acceptance vs T-A-P0 implementation drift).

**Evidence:**
- `tasks/mod.rs:558` — `LlmExtractTask::evaluate` is `pub async fn`, intended for both the review-time hook and future `exit_when: llm_extract` preset routing.
- `quality_loop.rs:372` — the review-time hook calls `cap.run(input).await` directly, not `LlmExtractTask::evaluate(&ctx)`.
- The completion report acknowledges this ("`LlmExtractTask::evaluate` is `pub` — it is the orchestration seam for the review-time hook (which calls the capability directly for now, since it has a `ReviewContext` not a `graph_flow::Context`)"; "Wiring the hook to call the task instead of the capability directly would require either an `Arc<Registry>` hook signature or a `graph_flow::Context` construction; deferred as non-surgical ... No residual opened — documented in `llm-extract.md` §2.").

**Why this is Warning, not Suggestion:**

Two parallel code paths now exist for the same logical operation ("render template + call capability + parse `Vec<KbCandidate>`"), sharing `candidate_from_llm_json` via `pub(crate)`. They are currently equivalent by construction (the hook uses the same parsing helper), but if `LlmExtractTask::evaluate` later adds preset-routing-specific behavior (e.g. retry / throttling / per-state metrics), the hook path will silently diverge. There is no compile-time guard that keeps them in sync — only an `#[allow(...)]` review comment would catch future drift. This is a maintainability/reliability concern (two surfaces for one behavior), not a current correctness issue.

**Suggested fix (non-blocking):** either (a) make the hook delegate to `LlmExtractTask::evaluate` via a `ReviewContext`-to-`graph_flow::Context` adapter (or extract the inner logic to a non-`Task` helper that both call), or (b) add a single `extract_candidates_via_llm(registry, prompt, chapter_prose, creator_id, session_id) -> Vec<KbCandidate>` helper that both `quality_loop::extract_via_llm` and `LlmExtractTask::evaluate` call — this is the smaller surgical fix. Or (c) add a regression test that asserts the two paths produce identical `Vec<KbCandidate>` for the same input.

#### W-002 — `LlmExtractTask::evaluate` returns `WorkerUnavailable` as an empty `Vec` (not an error); combined with F-001, the silent-empty-contract hides the production miswiring.

**Evidence:**
- `tasks/mod.rs:612-619` — on `CapabilityError::WorkerUnavailable`, the task logs at `warn!` and returns `Ok(Vec::new())`. The caller (`extract_kb_candidates_for_review` or future preset routing) cannot distinguish "worker genuinely returned zero candidates" from "no worker configured at all".
- The hook's `extract_via_llm` (`quality_loop.rs:374-376`) DOES surface `WorkerUnavailable` as `Fallback`, which is correct for the hook caller. But `LlmExtractTask::evaluate` does NOT — the contract is "empty vec on no-worker".
- In production today (per F-001), the hook takes the capability-call path (`quality_loop::extract_via_llm`) which correctly returns `Fallback`. So this is not currently a runtime bug. But the `LlmExtractTask::evaluate` contract is misleading for future `exit_when: llm_extract` callers who would get `Ok(vec![])` with no log signal at info level.

**Why Warning, not Critical:** today the hook path correctly surfaces the fallback, so users still see heuristic candidates. If F-001 is fixed and a future preset routes via `LlmExtractTask::evaluate`, the empty-vec-on-unavailable contract would silently produce an empty terminal result.

**Suggested fix (non-blocking):** return a new `TaskExecError::WorkerUnavailable` variant (or reuse the existing one in `TaskExecError`) and document the no-worker contract as "errors with `WorkerUnavailable`; callers fall back to heuristic" rather than "returns empty Vec". The hook path is already correct; this is purely a contract cleanup for the future preset-routing seam.

### 🟢 Suggestion

- **S-001** — Per-`schedule_id` terminal hook fires on every `novel-review-master` completion. Each fire reads `kb_key_blocks` (full-world scan) + `creator_schedules` (single row) + `works` (single row) + chapter body file from disk + 1 LLM call (when wired). For a daemon with N works whose `review` cron fires at the same minute, N terminal hooks fire serially within the supervisor's `on_schedule_terminal` (`supervisor.rs:340-542`). The `tick_in_progress` guard prevents re-entrant ticks, but the LLM calls themselves are not rate-limited at the call site — they rely on the worker pool's internal queue. For V1.51 scale this is fine; document for future V1.51+ scale review.

- **S-002** — `quality_loop::MAX_CANDIDATES_PER_PASS = 20` (`quality_loop.rs:51`) is a safety cap that prevents a single chapter from flooding `kb_extract_jobs.pending`. Reasonable, but it's not enforced on the LLM pathway's pre-filter — `extract_via_llm` does `.take(MAX_CANDIDATES_PER_PASS)` AFTER `filter_map` on `candidate_from_llm_json`. The LLM may return 50 candidates, 30 are rejected by `candidate_from_llm_json`, 20 are persisted. Consider logging how many LLM candidates were rejected (empty `canonical_name`) so operators can spot a misbehaving model. Informational; no urgency.

- **S-003** — `quality_loop.rs:715` — `existing_canonical_names` errors are propagated (`?`) but the caller's hook treats extraction as best-effort (`on_schedule_terminal` only logs `warn!` and does NOT fail the terminal transition). A flaky `kb_key_blocks` read therefore silently produces zero candidates. The code does log at `warn!` before propagating (`R-V150-WLA-10` regression guard) — good. No action; informational confirmation that the guard is in place.

- **S-004** — `kb_extract_jobs.proposed_payload` JSON carries all 4 LLM keys (`block_type`, `canonical_name`, `confidence`, `source_quote`) as a redundant copy of the dedicated columns (`llm_confidence`, `llm_source_quote`) + the existing `block_type_guess` + `canonical_name_guess` columns. `kb_adopt` reads columns first, JSON fallback second. The JSON path is dead-on-arrival in normal production use but adds ~200 bytes per row. Acceptable per plan §3.1 design (graceful degradation for V1.50 rows that pre-date the migration); informational only.

- **S-005** — `parse_extract_response` (`llm_extract.rs:173`) caps the warn-log payload at 120 chars (`&trimmed[..trimmed.len().min(120)]`). This is the right safety cap for LLM noise; an operator diagnosing "LLM returned garbage" has to enable `RUST_LOG=nexus_orchestration=trace` and re-run to see the full body. Acceptable for V1.51.

- **S-006** — `extract_kb_candidates_for_review` is invoked from `on_schedule_terminal` AFTER the terminal status UPDATE and AFTER `release_daemon_schedule_lock`. This ordering is correct (lock released first so the next auto-chain step can take the lock), but `process_auto_chain_after_terminal` runs sequentially in the same `on_schedule_terminal` call. If the LLM call is slow (e.g. 5s for a 4k-token chapter), it delays the auto-chain advancement. The hook is best-effort and non-blocking by contract, so this is a latent latency issue, not a correctness issue. Document for V1.51+ scale.

- **S-007** — `is_idempotent` (`kb_extract_job.rs:906`) keys on `(work_id, canonical_name_guess)`. A chapter rescan of the SAME chapter re-extracts candidates and is correctly skipped by this guard. A chapter rescan of a DIFFERENT chapter with the SAME canonical name is also blocked — correct because `kb_extract_jobs` keys on the per-`(creator, world, canonical_name)` uniqueness. The V1.50 T-B P2 refreshable-scan handled cross-chapter migration by updating `source_chapter_id` on the existing pending row, so this is consistent. No action; informational.

- **S-008** — `cron_supervisor.rs:558` — `has_active_role_schedule` uses a runtime `format!` for `IN ({ACTIVE_STATUS_LIST})` where `ACTIVE_STATUS_LIST` is a constant string. The constant `IN` list lives outside the `// SAFETY` comment but the query body is constant + not user-controlled, so it's acceptable per `nexus-local-db` style. No action; informational consistency check.

- **S-009** — `LlmExtract::run` constructs the extraction prompt via `format!` with the chapter prose interpolated directly (`llm_extract.rs:134-142`). The prompt template is fixed (hardcoded in the capability); chapter prose is treated as data and is bounded only by what the LLM provider accepts. No length cap here — the prompt size is bounded by the worker's IPC payload limits (provider-specific). Acceptable; just noting that a 100MB chapter would produce a 100MB prompt if a worker accepted it. Out of scope for V1.51 (chapters are author-controlled prose, typically <50k words).

- **S-010** — `extract_via_llm` reads `_creator_id` from `ctx.creator_id` (already loaded from `creator_schedules.creator_id`) and `_session_id = ""` (the review-time hook runs outside a preset session). This is documented in `quality_loop.rs:368-369` ("The review-time hook runs outside a preset session; pass an empty session id. The capability only forwards it to the worker IPC for routing — it is not a security identity (SEC-V131-01 covers creator_id)."). Correct — `_session_id` is non-security per the existing pattern. Just confirming the implementation matches the documented intent.

## Source Trace

### Performance trace (per `novel-review-master` schedule completion)

```
novel-review-master schedule completes
  → ScheduleSupervisor::on_schedule_terminal (supervisor.rs:340)
  → extract_kb_candidates_for_review (quality_loop.rs:293)
    → load_review_context: 1 schedule row + 1 work row + 1 chapter row + 1 chapter body file read
    → extract_via_llm (quality_loop.rs:347):
        registry.get("nexus.llm.extract") — O(1) HashMap lookup (capability/mod.rs:372)
        → cap.run(input) — V1.51+ ships 1 LLM call per review pass
    → MAX_CANDIDATES_PER_PASS = 20 (capped via .take() AFTER filter_map)
    → persist_candidates:
        for each candidate:
          is_idempotent check: 1 COUNT query (indexed on work_id)
          insert_pending_with_llm: 1 INSERT (uses work_entry_id uniqueness)
```

**LLM call frequency per `novel-review-master` execution: exactly 1** (no N+1 amplification). ✓
**Registry lookup cost: O(1)** via pre-built `HashMap<&'static str, usize>` index (`capability/mod.rs:360-366`). ✓

### Reliability trace (worker pool exhaustion)

- Per `nexus-orchestration` cron admission (`schedule/cron_supervisor.rs:148`), only one `novel-review-master` schedule per `(work_id, role)` is admitted at a time (`has_active_role_schedule` idempotency guard). Multiple works can fire simultaneously; `ScheduleConcurrency::Serial` per creator (`admission.rs:170`) serializes same-creator schedules.
- LLM calls within the hook are sequential (`await`); parallel LLM calls would require future preset routing via `LlmExtractTask` directly (not in V1.51).
- **In production today (per F-001)**: every LLM call returns `WorkerUnavailable` → fallback to heuristic → no actual LLM traffic. So "worker pool exhaustion" is moot until F-001 is fixed.

### Migration safety trace (`202606180006_kb_extract_jobs_llm_payload.sql`)

```sql
ALTER TABLE kb_extract_jobs ADD COLUMN llm_confidence REAL;
ALTER TABLE kb_extract_jobs ADD COLUMN llm_source_quote TEXT;
```

- Both columns are **nullable** with implicit `NULL` default. ✓
- Existing V1.50 rows default to `NULL` for both columns — no backfill needed. ✓
- No `NOT NULL` constraints without default — safe. ✓
- 4 DAO round-trip tests (`v151_*`) verify the migration + insert/list/get behavior. ✓
- Migration version (`006`) avoids collision with V1.50's `005` (`works_schedule_json_partial_idx`). ✓

### DB write idempotency trace

- `kb_extract_jobs` unique index `(creator_id, work_entry_id, world_id) WHERE status NOT IN ('failed')` (V1.29 heritage) provides DB-level idempotency for `enqueue` and `enqueue_with_artifact`.
- `is_idempotent` (kb_extract_job.rs:906) provides per-`canonical_name` idempotency for `insert_pending_with_llm` (the review-time hook path).
- `upsert_pending_candidate` (kb_extract_job.rs:580) handles the V1.50 T-B P2 chapter-rescan case (refresh `source_chapter_id` on existing pending row, never mutate `confirmed` rows). ✓
- The review-time hook (`quality_loop.rs:644-696`) calls `insert_pending_with_llm` directly per candidate. Concurrent writers are serialized by SQLite's single-writer model. ✓

### Resource lifecycle trace (LLM worker acquisition / release)

- `LlmExtract::run` calls `workers.call_acp_prompt(...).await` — single-shot async call, no explicit handle to release.
- `WorkerHandleProvider::call_acp_prompt` is `async fn`; resource lifecycle is managed by the WorkerManager's internal session pool (out of V1.51 scope).
- No `Mutex` or long-held locks introduced by V1.51. ✓
- On `WorkerUnavailable`, the hook logs at `debug!` and falls back. ✓
- On non-`WorkerUnavailable` capability error, the hook logs at `warn!` with `error` context and falls back (`quality_loop.rs:377-384`). ✓
- On chapter-prose-read error, the hook returns `Ok(None)` from `load_chapter_prose` and the parent hook returns `Ok(0)` (no candidates, no error propagation). ✓

### Heuristic fallback observability trace

- `extract_via_llm` returns `Fallback(reason: &'static str)` with one of three reasons:
  - `"no capability registry threaded"` (debug log)
  - `"nexus.llm.extract not registered"` (debug log)
  - `"nexus.llm.extract worker unavailable"` (debug log)
  - `"nexus.llm.extract capability error"` (warn log with `error` context)
- Caller (`extract_kb_candidates_for_review:306-313`) logs at `debug!` with the reason: `kb-extract: falling back to heuristic extraction`.
- For malformed LLM JSON, the capability logs at `warn!` and returns empty candidates (no fallback needed because the call succeeded) — observability here is correct.
- **In production today (per F-001)**: every review logs `debug! "kb-extract: falling back to heuristic extraction"` with `reason = "nexus.llm.extract worker unavailable"`. An operator running `RUST_LOG=nexus_orchestration=debug` would see this every minute per work. The log line is correct but the underlying cause (production wiring missing) is silent.

### V1.50 regression check

- `tests/review_time_extraction.rs` — all 5 V1.50 tests updated to pass `None` registry (heuristic fallback) ✓
- `tests/review_cron_e2e.rs` — both V1.50 tests updated to pass `None` registry ✓
- `tests/capability_registry.rs` — 4 tests including new 21-builtin count ✓
- `tests/kb_extract_jobs_migration.rs` — 8 V1.50 tests + 4 V1.51 tests, all pass ✓
- `tests/creator_world_kb_adopt.rs` — 3 tests (2 heuristic + 1 LLM), all pass ✓
- **V1.50 behavior preserved**: the heuristic fallback path is exactly the V1.50 logic (`extract_candidates_from_text`), retained verbatim (verified via diff vs `iteration/v1.51`). ✓

### Capability registry lookup cost

- `CapabilityRegistry::build_index` (capability/mod.rs:360-366) builds a `HashMap<&'static str, usize>` at construction time. Lookup is O(1) amortized.
- 21 builtins registered (V1.51 adds `nexus.llm.extract`).
- The hot-path lookup `registry.get("nexus.llm.extract")` is O(1). ✓

### Cron concurrency / worker pool starvation

- Cron fires at 1-min interval per (work, role). Idempotency guard prevents duplicate fires while a previous schedule is active.
- `ScheduleConcurrency::Serial` per creator serializes same-creator schedules; cross-creator schedules run in parallel.
- **If F-001 is fixed and the production wiring works**, the LLM pathway would issue one `worker/acp_prompt` per terminal schedule. Worker pool capacity is governed by `WorkerManager` (V1.32, unchanged). Multiple concurrent LLM calls would queue at the worker pool — backpressure is implicit (worker pool's internal queue depth). No explicit timeout or circuit-breaker at the hook level (mirrors `LlmJudgeTask` pattern, which has the same gap documented in `tasks/mod.rs:471-475`).
- For V1.51 single-creator local-only scale, this is acceptable. For multi-creator / networked scale, this is a known gap (R-V133P3-04 waived).

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 10 |

**Verdict**: Request Changes

## Verdict Reasoning

**Why `Request Changes` (not `Needs Discussion`):**

F-001 is **concrete** (verified by direct code inspection of `boot.rs:124`, `capability/mod.rs:148-184`, `llm_extract.rs:126-129`, `quality_loop.rs:347-398`) and **actionable** (either fix the wiring or demote the residual). It is not a design trade-off that needs PM/architect judgment — it is a missing wiring step that contradicts the completion report and the residual closure evidence. Per `mstar-review-qc` verdict rules: "Unresolved Critical or Warning → Request Changes". F-001 is unresolved at the time of this review.

**Why `Request Changes` (not `Approve` with residual):**

F-001 directly contradicts the explicit residual closure (`R-V150KBED-01 → lifecycle: resolved`) shipped in the same diff range (commit `5ef04b56`). The closure evidence lists commits + tests that are reachable in test fixtures but not in production. Approving-with-residual would mean "this defect is closed in test code but open in production" — which is exactly the failure mode the residual lifecycle exists to prevent. The PM must either (a) fix the wiring and re-review, or (b) re-open the residual to `deferred` with a concrete target plan_id.

**W-001 and W-002 are non-blocking** — they describe maintainability/contract-cleanup issues that do not affect current production correctness (the hook path correctly surfaces the fallback today). They are recorded for PM/architect awareness and can be addressed in a future plan.

**Suggestions are informational** — they describe scale/latency observations that are acceptable at V1.51 scope but worth noting for future iterations.

**CI gates:** V1.51-introduced code passes `cargo clippy -p nexus-orchestration --lib -- -D warnings` and `cargo +nightly fmt --all --check`. All targeted tests pass (15 + 3 + 4 + 3 = 25 tests across the V1.51 surfaces). The pre-existing `tests/review_report.rs` `doc_markdown` clippy warnings are out of the V1.51 diff scope (verified by `git diff ca494f03..deed03ff -- crates/nexus-orchestration/tests/review_report.rs` returning empty).

**Re-review path:**

After the F-001 fix (either production wiring landed, or residual re-opened with a concrete target), this reviewer is willing to verify the fix in a targeted re-review on the same report path (`reports/2026-06-18-v1.51-llm-extraction/qc3.md`) per `mstar-review-qc` targeted re-review convention. No new report file needed.