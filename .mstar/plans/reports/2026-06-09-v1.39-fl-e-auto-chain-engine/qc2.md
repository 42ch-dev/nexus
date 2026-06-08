---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-09-v1.39-fl-e-auto-chain-engine"
verdict: "Approve"
generated_at: "2026-06-09T18:25:00Z"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-09T18:30:00Z

## Revalidation — W-A targeted re-review (initial wave: qc1, qc2, qc3; fix wave 2)
- Reviewer (this re-review): @qc-specialist-2 (qc-specialist-2)
- Date: 2026-06-09T18:25:00Z
- Scope of re-review: W-A only
- Diff basis: 1e10e3ef..84db9a0e (verbatim copy from Assignment)
- Commits / files in this wave: 7 commits, 7 files, +383 / -140
- Tools run:
  - `cd /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.39-p0 && git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.39-p0`
  - `cd ... && git branch --show-current` → `feature/v1.39-fl-e-auto-chain-engine`
  - `cd ... && git log 1e10e3ef..84db9a0e --oneline` → 5 commits shown (style + fix(local-db) W-E + fix(daemon-runtime) W-D + fix(orchestration) W-A 6e505c06 + final style); the W-A helper extraction is commit 6e505c06.
  - `cd ... && git diff --stat 1e10e3ef...84db9a0e` → 7 files, +383/-140 (including the 4 targeted files).
  - Targeted: `git diff 1e10e3ef...84db9a0e -- crates/nexus-orchestration/src/auto_chain.rs crates/nexus-orchestration/src/schedule/supervisor.rs crates/nexus-daemon-runtime/src/boot.rs crates/nexus-orchestration/tests/auto_chain.rs` → confirms helper extraction at auto_chain.rs:309+, delegation in supervisor.rs:474 and boot.rs:600, removal of duplicated INSERT+set_driver blocks, addition of two new unit tests.
  - `cargo clippy --all -- -D warnings 2>&1 | tail -5` → clean ("Finished `dev` profile...").
  - `cargo test -p nexus-orchestration --lib auto_chain 2>&1 | tail -5` → 17 passed (explicitly includes `enqueue_helper_success_path` and `enqueue_helper_error_path_no_mapping`).
  - `cargo test -p nexus-orchestration --test auto_chain 2>&1 | tail -5` → 21 passed (including boot resume simulation tests now calling the shared helper).
  - `cargo +nightly fmt --all -- --check` → clean (no output).

### Revalidation Analysis (W-A correctness questions)

**1. Fresh DB read before enqueue vs. trusting the `&WorkRecord` parameter?**

The helper `pub async fn enqueue_auto_chain_schedule` (auto_chain.rs:325) **trusts the caller-supplied `work: &WorkRecord`**. It immediately calls `build_auto_chain_schedule(stage, creator_id, work, chapter)` (line 333) with no internal `get_work` reload or re-check of `driver_schedule_id IS NULL`.

However, both production call sites perform a **fresh reload immediately before** delegating:
- Supervisor path (`process_auto_chain_after_terminal`, supervisor.rs:361): does `nexus_local_db::works::get_work(&self.pool, creator_id, &work.work_id).await` (line 384 in the fix-wave diff context, confirmed in current code), then passes the fresh `&work` to `enqueue_auto_chain_step` (lines 404, 421) which forwards to the helper (474).
- Boot recovery path (boot.rs:228): after `find_resumable_works`, does `let fresh = nexus_local_db::works::get_work(...)` (line 237), checks `if let Ok(Some(latest)) = fresh`, then calls `resume_auto_chain_work` (now thin delegate at 600) with `&latest`.

The stale-snapshot concern raised in the initial W-1 is therefore mitigated at the call sites (the same discipline that existed before the extraction), but the helper itself does not enforce or re-validate the snapshot. A future direct caller could still pass a stale record.

**2. Atomicity of INSERT + set_driver?**

No. The helper does the `INSERT INTO creator_schedules ... 'pending'` (lines 343-358) followed by a separate `set_driver(pool, creator_id, work_id, &schedule_id, stage).await?` (line 365). There is no `BEGIN` / transaction wrapper, no `ON CONFLICT` guard, and no re-check inside the helper that `driver_schedule_id IS NULL` at the moment of the INSERT. A crash between the two statements can still leave an "orphan" pending schedule whose `work.driver_schedule_id` was never updated (pre-existing surface; the helper merely relocates the two statements under one roof). The callers (supervisor terminal hook and boot loop) had the identical non-atomic pattern before.

**3. Visibility / callable only from intended sites?**

The helper is declared `pub async fn enqueue_auto_chain_schedule` inside `pub mod auto_chain;` (lib.rs:4 exposes `pub mod auto_chain;`). It is therefore visible to any downstream crate that depends on `nexus-orchestration` (and can construct a `&WorkRecord`). It is not `pub(crate)` or `pub(super)`. 

Current production call sites are only the two intended ones (supervisor::enqueue_auto_chain_step and boot::resume_auto_chain_work). The integration test in `tests/auto_chain.rs` (simulate_boot_auto_resume) and the new unit tests also call it directly. A future surface such as `creator run resume` (or any other code that can reach the orchestration crate) could in principle call it directly without going through the supervisor/boot "evaluate + decide" flow. The extraction improves locality but does not add a visibility gate or a "must come from supervisor or boot recovery" contract.

**4. `enqueue_helper_error_path_no_mapping` test coverage and error typing**

The test (auto_chain.rs: tests::enqueue_helper_error_path_no_mapping, added in this wave) constructs a Work with a nonexistent preset, calls the helper with stage `"unknown_stage_xyz"`, and asserts:
- `result.is_err()`
- `matches!(err, AutoChainError::InvalidState(_))`

It covers exactly the "no schedule mapping for stage" path (the `build_auto_chain_schedule` returns `None` case, now turned into `AutoChainError::InvalidState`). The error is a typed `AutoChainError::InvalidState(String)`, distinguishable from `AutoChainError::Database(...)`.

In the supervisor caller (supervisor.rs:480), `InvalidState` is deliberately turned into a non-error `Ok(())` + warn log (the "no mapping for this stage" terminal case is expected to be a no-op). Boot maps any error to `String`. The typing therefore allows callers to distinguish "fatal / retryable DB error" from "expected no-mapping case".

### Duplication hazard disposition (original W-1 / consolidated W-A)

**Eliminated.** The ~40-line duplicated blocks (build + ACH{ts} mint + INSERT + set_driver + tracing/error mapping) that lived in `boot.rs:resume_auto_chain_work` and `supervisor.rs:enqueue_auto_chain_step` have been removed. Both sites are now thin delegates to the single `enqueue_auto_chain_schedule` implementation (auto_chain.rs:325-376, introduced in 6e505c06). The "Fix A (W-A)" comment and docstring explicitly document the intent.

### Remaining risks (W-A re-review lens)

- **Stale WorkRecord trust**: helper takes `&WorkRecord` with no internal freshness or "still eligible for driver" re-check. Call-site reload discipline is still required (and is present today in both sites).
- **Non-atomic INSERT + set_driver**: crash window between the two statements can still produce orphan pending schedules. The helper does not introduce a transaction or a `driver_schedule_id IS NULL` guard at enqueue time.
- **pub visibility**: the helper is `pub` (exported via the module). Future code paths (e.g. direct CLI resume, tests, or new recovery logic) can invoke it without the supervisor/boot decision flow. No `pub(crate)` or documented "internal only" restriction beyond the doc comment naming the two intended callers.

### New test quality

- `enqueue_helper_success_path`: creates a valid work, calls the helper, asserts (a) returned ID starts with "ACH", (b) row exists in `creator_schedules` with status='pending', (c) the Work's `driver_schedule_id` and `current_stage` were updated by `set_driver`. Good coverage of the happy path through the helper.
- `enqueue_helper_error_path_no_mapping`: as described above — explicitly exercises the `InvalidState` branch with a typed match. The test is narrow but precise for the error variant.

No new blocking finding introduced by the extraction itself.

**W-A resolved — no new blocking finding.**

**Revalidation verdict (W-A scope only): Approve**

## Scope
- plan_id: 2026-06-09-v1.39-fl-e-auto-chain-engine
- Review range / Diff basis: merge-base: c7a3fac1 (iteration/v1.39) + tip: c143da1f (feature/v1.39-fl-e-auto-chain-engine HEAD); equivalent to `git diff c7a3fac1...c143da1f`
- Working branch (verified): feature/v1.39-fl-e-auto-chain-engine
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.39-p0
- Files reviewed: 14
- Commit range: c7a3fac1..c143da1f (15 commits, +2034 / -54)
- Tools run: cargo clippy --all -- -D warnings, cargo test -p nexus-orchestration --test auto_chain, cargo test -p nexus-local-db, cargo test -p nexus-daemon-runtime, cargo +nightly fmt --all -- --check, targeted git diff + static analysis of checkpoint mutation sites, fresh DB reads, schedule ID generation, WorkPatch construction, and `format!` SQL usage.

## Findings

### 🔴 Critical

None.

### 🟡 Warning

#### W-1: Duplicate schedule-enqueue logic between boot recovery and supervisor (two writers of `driver_schedule_id`)

- **Files**: `crates/nexus-daemon-runtime/src/boot.rs:589-633` (`resume_auto_chain_work`) and `crates/nexus-orchestration/src/schedule/supervisor.rs:463-531` (`enqueue_auto_chain_step`).
- **Commits**: `e1766b07` (boot helper), `1cd73b31` (supervisor wiring).
- **Issue**: Both paths perform the identical sequence: `build_auto_chain_schedule`, generate `ACH{timestamp}` schedule ID, `INSERT INTO creator_schedules ... 'pending'`, then `auto_chain::set_driver(...)` to wire `driver_schedule_id` + stage on the Work. The boot helper is intentionally a standalone copy because it runs before the supervisor is fully initialized.
- **Correctness / security risk**: The three checkpoint fields (`auto_chain_enabled`, `driver_schedule_id`, `auto_chain_interrupted`) are mutated from multiple call sites (supervisor terminal hook, boot recovery loop, `creator run resume` PATCH, side-input gate read in `append_inspiration`, and `creator run continue` inspiration append). Having two nearly-identical implementations of "create a pending driver schedule and atomically claim the Work checkpoint" creates a maintenance hazard. A future change to the INSERT shape, ID format, `set_driver` semantics, or addition of a uniqueness constraint could be applied in only one path, allowing the invariant "at most one active FL-E driver schedule per Work" to be violated under concurrent resume + terminal events.
- **Evidence**: The two functions are ~40 lines each with only cosmetic differences (tracing labels, error mapping). No shared helper is used. The assignment explicitly called out this concurrency surface.
- **Fix**: Extract a single internal function (e.g. `auto_chain::enqueue_auto_chain_schedule(pool, creator_id, work, stage, chapter) -> Result<String>`) that both the supervisor and the boot helper call. Keep the boot path's "pre-supervisor" usage but eliminate the duplicated SQL + checkpoint write.

#### W-2: Schedule ID generation uses low-entropy timestamp (`ACH{YYYYMMDDHHMMSSmmm}`) instead of ULID

- **Files**: `crates/nexus-orchestration/src/schedule/supervisor.rs:485`, `crates/nexus-daemon-runtime/src/boot.rs:605` (and the identical line in the extracted helper).
- **Commits**: `1cd73b31`, `e1766b07`.
- **Issue**: Auto-chain driver schedule IDs are minted as `format!("ACH{}", Utc::now().format("%Y%m%d%H%M%S%3f"))`. The rest of the schedule system uses ULIDs. Under a daemon restart that triggers boot recovery for N interrupted Works while another schedule terminal hook is also firing (or two boots in the same millisecond), the probability of collision on the 3-digit millisecond suffix is non-zero.
- **Correctness risk**: Primary-key violation on `creator_schedules.schedule_id` (or silent overwrite of a `driver_schedule_id` pointer if the INSERT is not the first writer to win). The side-input 409 gate and `find_work_for_driver` lookup both key off `driver_schedule_id`; a colliding ID would corrupt the single-driver invariant.
- **Threat model note**: Single-user local-first reduces blast radius, but the code still has a latent TOCTOU between ID generation and the INSERT + subsequent `set_driver` update. The assignment specifically asked about TOCTOU windows where two writers can both observe `auto_chain_enabled = true` / `driver_schedule_id = null` and both enqueue.
- **Fix**: Use ULID (or `ACH_<ulid>`) for the schedule ID while retaining the `ACH` prefix for human/observability filtering. This matches the existing schedule ID contract and eliminates the timestamp collision surface.

#### W-3: `creator run resume <work_id>` clears `auto_chain_interrupted` but performs no synchronous re-evaluation or enqueue

- **File**: `crates/nexus42/src/commands/creator/run.rs:812-849` (the PATCH of `{"auto_chain_interrupted": false}` and the success message).
- **Commit**: `f04b16c4`.
- **Issue**: The resume subcommand only does a narrow PATCH. It does not call any "nudge / evaluate / tick" surface on the daemon, nor does it directly invoke `evaluate_next_step + enqueue`. The success text correctly says "The daemon will evaluate the next step automatically." If the supervisor is idle (no other schedules transitioning), the resumed Work may remain stalled until the next periodic tick or external event.
- **Correctness / UX risk**: User runs `creator run resume` after a daemon restart, sees the "resumed" message, and reasonably expects the chain to make progress. In practice it waits for the supervisor's next wake-up. This is not a data corruption bug, but it is a gap in the "daemon restart auto-resumes" story (AC4) from the CLI surface.
- **Server-side note**: The generic Work PATCH handler does not enforce "only allow this PATCH when `auto_chain_interrupted == true && auto_chain_enabled == true`". Any owned Work can have the flag cleared. The DB layer will still reject cross-creator via the creator_id scoping on the UPDATE.
- **Fix options**: (a) After the PATCH, have the CLI (or a new daemon endpoint) request an immediate supervisor evaluation for that work_id; (b) document the "next tick" semantics more prominently and add a `creator run status` hint in the resume output.

### 🟢 Suggestion

#### S-1: No partial index on the new `driver_schedule_id` column

- The hot path `find_work_for_driver` (called from `on_schedule_terminal`) does `SELECT ... FROM works WHERE driver_schedule_id = ? LIMIT 1`.
- The migration only adds the column; no `CREATE INDEX` on `driver_schedule_id` (or a partial index `WHERE driver_schedule_id IS NOT NULL`).
- For a local single-creator DB with a small `works` table this is harmless. As the number of Works grows (or in multi-creator workspaces), the lookup becomes a table scan on every stage completion.
- **Suggestion**: Add a cheap partial index in a follow-up migration (or the same one if still editable): `CREATE INDEX IF NOT EXISTS works_driver_schedule ON works(driver_schedule_id) WHERE driver_schedule_id IS NOT NULL;`. This makes the single-driver lookup O(log N) and documents the intended access pattern.

#### S-2: `find_resumable_works` filter and boot reload discipline are correct but implicit

- The boot recovery does a `find_resumable_works` (which filters for interrupted auto-chain Works whose driver schedule is no longer running), then for each candidate does a **fresh** `get_work` before calling `evaluate_next_step` and `resume_auto_chain_work`.
- This reload is the right defense against TOCTOU between the query and the action.
- The exact definition of "resumable" lives only in the implementation of `find_resumable_works` (and its tests). A future reader or refactored query could accidentally drop the "schedule is no longer running" or "auto_chain_enabled" filter.
- **Suggestion**: Add a short doc comment on `find_resumable_works` (and the test names) that explicitly states the three conditions that must all hold: (1) `auto_chain_enabled = true`, (2) `auto_chain_interrupted = true` or driver schedule no longer active, (3) Work not already completed. This makes the boot safety contract reviewable without reading the full query.

#### S-3: WorkPatch and generic PATCH handler correctly neutralize the new fields

- In both the stage-aware path (`apply_non_stage_fields`) and the generic `patch_work` handler, the three new checkpoint fields are explicitly initialized to `None` in the `WorkPatch` sent to the DB layer.
- The public `PatchWorkRequest` / DTO does not expose setters for them in the current diff.
- This means a caller (even a compromised or misbehaving agent using the Local API) cannot use the generic PATCH surface to set `auto_chain_enabled = true` or inject a fake `driver_schedule_id` for a Work it does not own.
- Creator isolation is preserved because every DB write path still requires the `creator_id` from the local config file.
- **Suggestion**: Add a unit or integration test that attempts a PATCH with the three fields present in the JSON body and asserts they are ignored (or produce a validation error if you later decide to accept them explicitly). This documents the intended "these fields are daemon-internal" boundary.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W-1 | git-diff + manual reasoning + assignment callout | `boot.rs:589-633` vs `supervisor.rs:463-531`; both call `build_auto_chain_schedule` + INSERT + `set_driver` | High |
| W-2 | git-diff + code inspection | Schedule ID mint sites + assignment explicit question on TOCTOU for `driver_schedule_id = null` | High |
| W-3 | git-diff + CLI surface review | `run.rs:812-849` (PATCH only, no nudge); message correctly hedges but still a gap vs "auto-resume" expectation | Medium |
| S-1 | Performance / query plan review | `find_work_for_driver` SELECT on new column; migration has no index | Medium |
| S-2 | Code review of boot path | `boot.rs:236-241` fresh `get_work` reload before `evaluate_next_step`; filter logic lives only in `find_resumable_works` | High |
| S-3 | Patch handler construction | `works.rs:484-486`, `apply_non_stage_fields`, and the main patch_work path all force the three fields to `None` | High |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 3 |

### Security & Correctness Observations (qc2 focus)

**Positive (no Criticals):**
- All six acceptance criteria (AC1–AC6) have direct passing integration tests in `nexus-orchestration/tests/auto_chain.rs` (21 tests total). The side-input 409 gate (AC5) is explicitly tested.
- The side-input enforcement in `append_inspiration` performs a **fresh** `get_work` read immediately before the check `auto_chain_enabled && driver_schedule_id.is_some()`. This is the correct pattern for the single-driver invariant.
- Boot recovery also reloads the Work fresh (`get_work`) after `find_resumable_works` before any `evaluate_next_step` or enqueue. This closes the most obvious TOCTOU between query and action.
- Migration is purely additive `ALTER TABLE ADD COLUMN ... NOT NULL DEFAULT ...`. No data backfill, no non-idempotent statements. Uses the documented DDL/runtime-`query` exception with a clear comment.
- No new `format!` of untrusted data into DML statements. The two dynamic-SQL sites (`find_work_for_driver` column-list + schedule INSERT) use `?` placeholders and carry explicit `// SAFETY: dynamic SQL` comments. All other new queries in the changed paths are either through existing safe helpers or the `sqlx::query!` style already present in the crate.
- `WorkPatch` construction in the PATCH handlers explicitly nulls the three new fields; they are not accepted from the request DTO in the public surfaces. Creator isolation (every path reads `creator_id` from local config and scopes the DB call) continues to hold.
- Status DTO (`WorkApiDto`) now includes the three fields, but they are only ever populated from a creator-scoped WorkRecord. Unauthorized callers receive 404 before any response is built.
- `creator run resume` correctly requires the Work to be owned by the active creator (the PATCH will fail at the DB layer for cross-creator attempts).

**Residual correctness risks (Warnings, not Critical under local-first model):**
- The duplication between the two enqueue paths (W-1) is the most material long-term risk to the "single driver" state machine. It directly matches the assignment's question about multiple writers mutating the three checkpoint columns.
- The ACH timestamp ID (W-2) is a latent collision vector between concurrent writers that both see a null `driver_schedule_id`.
- The resume surface (W-3) is "correct by message" but creates an observable window where the user has cleared the interrupt flag yet no driver schedule has been enqueued yet.

**Branch & scope discipline**: Verified on `feature/v1.39-fl-e-auto-chain-engine`. All 14 files touched are within the P0 plan scope. No P0.5/P1+ creep. V1.38 chapter selection / status UX paths are not altered.

**Verification evidence (reproduced in this session)**:
```
cargo clippy --all -- -D warnings  → clean
cargo test -p nexus-orchestration --test auto_chain  → 21 passed
cargo test -p nexus-local-db  → all binaries clean
cargo test -p nexus-daemon-runtime  → all binaries clean (265+ passed)
cargo +nightly fmt --all -- --check  → clean
git rev-parse + branch + log + diff --stat all match Assignment
```

**Verdict**: Approve
