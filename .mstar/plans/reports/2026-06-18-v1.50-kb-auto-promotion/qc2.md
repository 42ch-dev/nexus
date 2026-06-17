---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: 2026-06-18-v1.50-kb-auto-promotion
working_branch: feature/v1.50-kb-auto-promotion
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-kb-auto-promotion
review_range: merge-base 0ea2995ff45569b541b17097c4c919dabab4bb16..8eec12e5dac2a023a4b4115483505534119c630c
verdict: Approve
generated_at: 2026-06-17T11:59:17Z
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security + correctness (primary); cross-cutting regression, maintainability, and test coverage
- Report Timestamp: 2026-06-17T11:15:18Z

## Scope
- plan_id: 2026-06-18-v1.50-kb-auto-promotion
- Review range / Diff basis: merge-base 0ea2995ff45569b541b17097c4c919dabab4bb16..8eec12e5dac2a023a4b4115483505534119c630c
- Working branch (verified): feature/v1.50-kb-auto-promotion
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-kb-auto-promotion
- Files reviewed: 14 (4 feature commits)
- Commit range: 0ea2995f..8eec12e5 (c616dc11 T1 DAO+migration, 841ec302 T2/T3 heuristic+hook, 13494027 T4/T5/T6 CLI, 8eec12e5 docs+Completion Report)
- Tools run: cargo +nightly fmt --all --check (clean), cargo clippy -p nexus42 -p nexus-local-db -p nexus-orchestration -- -D warnings (clean), cargo test on the three new/updated test binaries (all green: 8 CLI + 7 DAO migration + 5 review-time extraction)

## Findings

### 🔴 Critical
- **None.**

### 🟡 Warning
- **Adopt is not atomic (KeyBlock insert then separate mark_confirmed).**  
  In `kb_adopt` (crates/nexus42/src/commands/creator/world/kb.rs:472-488):  
  1. `store.insert_key_block(kb)` (Novel validation mode) succeeds and writes a `confirmed` KeyBlock.  
  2. `mark_confirmed(pool, extract_job_id)` (conditional `UPDATE ... WHERE promotion_status='pending'`) may return false.  
  The code explicitly documents the outcome ("KeyBlock was not duplicated") and surfaces a clean error, but the KeyBlock now exists in the world while the job row is stuck in a non-pending state. No compensating cleanup or transaction wrapper.  
  **Impact:** Partial-state inconsistency visible to operators; future re-adopt attempts on the same job_id will fail the "must be pending" check in `load_pending_candidate`. Not a data-loss or privilege issue, but a correctness smell for a state-machine transition that the plan describes as atomic-ish.  
  **Evidence:** Diff lines 477-488; `mark_confirmed` implementation (kb_extract_job.rs:680-690) does a single-row conditional UPDATE with no surrounding tx in the CLI path; contrast with `claim_job` which does use `pool.begin()`.

- **Reject audit log path uses internal `work_id` (not human `work_ref`) under `Works/<work_id>/...`.**  
  `write_rejected_log` (kb.rs:700-733) does:  
  ```rust
  let work_ref = candidate.work_id.as_deref().unwrap_or("unknown-work");
  let log_dir = ws_dir.join("Works").join(work_ref).join("Logs/kb/rejected");
  ```
  The entity-scope-model and home-layout convention consistently use human `work_ref` for `Works/<work_ref>/`. The comment acknowledges the choice ("resolving a friendlier work_ref would need a DB round-trip"). `work_id` values are DB-generated and not attacker-controlled, so there is no path traversal, but the resulting audit trail lives under opaque IDs instead of the documented human slugs. Non-fatal (best-effort + warn! only).  
  **Impact:** Audit hygiene / operator discoverability. Low security risk.

### 🟢 Suggestion
- **proposed_payload is parsed with bare `serde_json::from_str` into `KeyBlockBody`.**  
  (kb.rs:482-483)  
  ```rust
  let body: KeyBlockBody =
      serde_json::from_str(candidate.proposed_payload.as_deref().unwrap_or("{}"))?;
  ```  
  The payload originates from the trusted heuristic path (`extract_candidates_from_text`) and is later re-validated by `SqliteKbStore::with_validation_mode(Novel)` on insert. For the current "heuristic-only" V1.50 scope this is acceptable. If a future iteration allows external/LLM-supplied payloads on the same code path, add an explicit schema or allow-list step before deserialization. No current exploit path.

- **Idempotency check (`is_idempotent`) has a classic TOCTOU window before `insert_pending`.**  
  `persist_candidates` (quality_loop.rs:450-466) calls `is_idempotent` then `insert_pending` without a transaction. The DAO comment and the unique index on `(creator_id, work_entry_id, world_id) WHERE status NOT IN ('failed')` (with `work_entry_id` bound to `canonical_name_guess`) provide a backstop. The acceptance criterion (§6) only requires "does not duplicate pending rows" — the guard + index satisfy it in practice. Worth a one-line note in the plan or a follow-up if duplicate-KeyBlock semantics ever tighten.

- **Stable error code reuse is excellent.**  
  All three new subcommands (`pending`/`adopt`/`reject`) and the T-B P0 edit/delete paths funnel through the same `require_world_owner` helper, which emits the stable `WORLD_KB_FORBIDDEN` code on 403. Cross-author tests (both the new promotion tests and the pre-existing `world_kb_authz.rs`) assert on the code string. Good consistency.

- **Heuristic regex is low-risk.**  
  `capitalized_phrase_regex()` is a static `OnceLock<Regex>`, bounded input (chapter prose), `MAX_CANDIDATES_PER_PASS=20`, and a conservative stopword filter. No catastrophic backtracking pattern; ReDoS surface is negligible.

- **Error surfacing for validation failures on adopt is `Other("ValidationError: ...")`.**  
  Not a new 422 code. Existing pattern for KB validation errors; acceptable for CLI surface. If a machine-readable contract ever needs distinct 422 vs 403, this is the spot to evolve.

## Source Trace

- **Finding:** Adopt non-atomic state transition  
  **Source Type:** manual-reasoning + code review  
  **Source Reference:** `git diff ... crates/nexus42/src/commands/creator/world/kb.rs:472-488` + `kb_extract_job.rs:680` (mark_confirmed)  
  **Confidence:** High

- **Finding:** Reject log path uses work_id  
  **Source Type:** git-diff  
  **Source Reference:** `kb.rs:700` (`let work_ref = candidate.work_id...`) + plan §5.5.4 + home-layout `work_ref` convention  
  **Confidence:** High

- **Finding:** WORLD_KB_FORBIDDEN reuse  
  **Source Type:** grep + diff  
  **Source Reference:** `grep -n WORLD_KB_FORBIDDEN` (28 hits), `require_world_owner` at kb.rs:571, called from pending/adopt/reject/edit/delete paths  
  **Confidence:** High

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 4 |

**Verdict**: Approve

All tests green, fmt/clippy clean, author gate consistently reused with stable error code, heuristic is bounded and pure, and the only state-machine deviation (adopt) is explicitly documented with a user-visible error. The two Warnings are hygiene/correctness observations rather than blocking issues for the current scope.

---

## Completion Report v2

**Agent**: qc-specialist-2  
**Task**: Initial QC #2 (security + correctness) for 2026-06-18-v1.50-kb-auto-promotion (T-B P1)  
**Status**: Done  
**Scope Delivered**: Full diff review (4 commits, 14 files), verification of worktree/branch/range, static checks, test execution, targeted security+correctness analysis per assignment bullets (auth gate reuse, adopt atomicity, reject log path, proposed_payload parsing, heuristic ReDoS, idempotency race, error codes).  
**Artifacts**: `.mstar/plans/reports/2026-06-18-v1.50-kb-auto-promotion/qc2.md` (this file)  
**Validation**: 
- `cargo +nightly fmt --all --check` (clean)
- `cargo clippy -p nexus42 -p nexus-local-db -p nexus-orchestration -- -D warnings` (clean)
- `cargo test -p nexus42 --test world_kb_promotion_cli` (8/8 passed)
- `cargo test -p nexus-local-db --test kb_extract_jobs_migration` (7/7 passed)
- `cargo test -p nexus-orchestration --test review_time_extraction` (5/5 passed)
**Issues/Risks**: Two non-blocking Warnings recorded (adopt partial-state on flip failure; reject log under work_id rather than work_ref). No Critical.  
**Plan Update**: N/A (QC role does not mutate plans or status.json).  
**Handoff**: Report committed; ready for PM consolidation with qc1/qc3.  
**Git**: (to be filled after `git add` + `git commit` of only the report path)

---

## Revalidation

```yaml
report_kind: qc-revalidation
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: 2026-06-18-v1.50-kb-auto-promotion
working_branch: feature/v1.50-kb-auto-promotion
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-kb-auto-promotion
review_range: 8eec12e5..bab531a1
fix_wave_commits:
  - 2cfbd49e (R-V150KBED-03 transaction wrap)
  - 02cc52d5 (R-V150KBED-04 index column rename)
  - 125533a8 (R-V150KBED-05 reject log path)
  - bab531a1 (plan completion report)
verdict: Approve
generated_at: 2026-06-17T11:59:17Z
```

### Scope of revalidation
- Targeted re-review (qc-specialist-2 only) of fix-wave delta `8eec12e5..bab531a1` (4 commits).
- Verify resolution of the **two blocking Warnings** raised by qc-specialist-2 in the initial wave:
  - **R-V150KBED-03**: `kb_adopt` not wrapped in transaction (adopt not atomic; risk of orphan KeyBlock on mark_confirmed failure / race).
  - **R-V150KBED-05**: reject log path uses internal `work_id` under `Works/<work_id>/...` instead of human `work_ref`.
- R-V150KBED-04 (migration index column rename) was outside qc2 scope but included in the same fix-wave.
- Re-run full static checks and the promotion test binary.

### R-V150KBED-03 disposition: RESOLVED
**Fix commit**: `2cfbd49e` — "fix(nexus42): R-V150KBED-03 kb_adopt transaction wrap + rollback test"

**Changes**:
- `crates/nexus-local-db/src/kb_store.rs`: added `insert_key_block_in_tx(&mut Transaction, KeyBlock)` — concrete tx-aware sibling of the trait method (validation + INSERT executed against the tx).
- `crates/nexus-local-db/src/kb_extract_job.rs`: added `mark_confirmed_in_tx(&mut Transaction, &str)`.
- `crates/nexus42/src/commands/creator/world/kb.rs:481-517` (`kb_adopt`):
  ```rust
  let mut tx = pool.begin().await?;
  let insert_result = store.insert_key_block_in_tx(&mut tx, kb).await?;
  let flipped = mark_confirmed_in_tx(&mut tx, extract_job_id).await?;
  if !flipped {
      tx.rollback().await.ok();  // best-effort
      return Err(CliError::Other("... The transaction was rolled back; no orphan row created."));
  }
  tx.commit().await?;
  ```
- Error message updated from the misleading "KeyBlock was not duplicated" to the accurate rollback language.
- New regression test `kb_adopt_failure_rolls_back_insert` (world_kb_promotion_cli.rs:320-387):
  - Pre-flips the candidate to `confirmed` (simulates race winner).
  - Exercises the exact `begin → insert_in_tx → mark_in_tx → !flipped → rollback` boundary.
  - Asserts: **no orphan KeyBlock** for the name after rollback; candidate retains the race-winner's `confirmed` state.

**Verification**:
- `cargo test -p nexus42 --test world_kb_promotion_cli` → 11/11 passed (new test + 10 others).
- The test explicitly documents: "R-V150KBED-03 regression: orphan KeyBlock ... MUST NOT persist after rollback".

**Status**: Closed. The state-machine transition is now atomic; partial-state inconsistency is impossible.

### R-V150KBED-05 disposition: RESOLVED
**Fix commit**: `125533a8` — "fix(nexus42): R-V150KBED-05 reject log path uses work_ref not work_id + validation test"

**Changes**:
- New helper `resolve_work_ref_for_log(pool, work_id, workspace_dir)` (kb.rs:801-833):
  - Returns `Ok(None)` when `workspace_dir.is_none()` (hermetic test path — no log needed).
  - Returns `Err(...)` if:
    - `work_id` is None → "candidate has no work_id"
    - `works` row absent → "work_id '...' does not exist"
    - `story_ref` IS NULL → "has no story_ref (work_ref)"
  - Failure happens **before** `mark_rejected`, so no DB side-effect and no log under wrong path.
- `kb_reject` (kb.rs:564-565): resolves `work_ref` **before** the flip, passes it to `write_rejected_log`.
- `write_rejected_log` signature updated to `work_ref: Option<&str>`; path construction now:
  ```rust
  let log_dir = ws_dir.join("Works").join(work_ref).join("Logs/kb/rejected");
  ```
- Log body records both `**work_id**` (for audit) and `**work_ref**` (path authority).
- Two new tests in world_kb_promotion_cli.rs:
  - `kb_reject_writes_log_under_work_ref_path`: seeds proper work, asserts path contains `WORK_REF` (`test-novel`), does **not** contain `WORK_ID`, body records both.
  - `kb_reject_fails_when_work_ref_missing`: inserts candidate with orphan `work_id` ("wrk_orphan"), expects `CliError::Other` mentioning the id, asserts row **remains pending**, and no `Works/` directory is created.

**Verification**:
- `cargo test -p nexus42 --test world_kb_promotion_cli` → both regression tests pass (part of the 11).
- Path now matches home-layout `Works/<work_ref>/` convention.

**Status**: Closed. Audit log hygiene corrected; validation gate prevents partial-state (rejected row + wrong-path log).

### Static checks (full workspace, post-fix-wave)
- `cargo clippy --all -- -D warnings` → clean (no output after "Finished dev profile").
- `cargo +nightly fmt --all --check` → clean (no output; exit 0).

### Test execution (fix-wave relevant)
- `cargo test -p nexus42 --test world_kb_promotion_cli` → **11 passed** (explicitly includes the two new regression tests for R-03 and R-05).
- Supporting runs (as reported by implementer and cross-checked):
  - `cargo test -p nexus-local-db --test kb_extract_jobs_migration` → 8 passed (migration hygiene).
  - `cargo test -p nexus-orchestration --test review_time_extraction` → 5 passed.

### New findings introduced by fix-wave (qc2 lens)
- **None.** The delta is surgical:
  - Transaction wrapper + tx-aware DAO siblings for atomicity.
  - Pre-flip resolution + validation for path correctness.
  - Two targeted regression tests.
- No new security surface, no new correctness holes, no change to authz or heuristic paths.
- R-V150KBED-04 (index column rename + migration) was a maintainability item outside qc2's primary scope; no correctness impact observed.

### Disposition summary

| R#            | Initial qc2 finding                  | Fix-wave evidence                          | Test coverage                     | Status   |
|---------------|--------------------------------------|--------------------------------------------|-----------------------------------|----------|
| R-V150KBED-03 | kb_adopt not atomic (orphan risk)    | `pool.begin()` + `_in_tx` + explicit rollback on !flipped | `kb_adopt_failure_rolls_back_insert` asserts no orphan | RESOLVED |
| R-V150KBED-05 | reject log under work_id not work_ref| `resolve_work_ref_for_log` before flip + work_ref path | `kb_reject_writes_log_under_work_ref_path` + `kb_reject_fails_when_work_ref_missing` | RESOLVED |

### Verdict
**Approve**

Both Warnings originally raised by qc-specialist-2 (R-V150KBED-03 and R-V150KBED-05) are fully resolved in the fix-wave:
- Atomic promotion with proven rollback (no orphan KeyBlock possible).
- Correct `work_ref`-based reject audit logging with pre-flip validation.
- Full workspace clippy + nightly fmt clean.
- 11/11 promotion CLI tests passing, including the two new regression tests that directly encode the invariants.

No new Critical or Warning findings for the qc2 (security + correctness) focus area. The original initial-wave content above remains the immutable record of the first review; this Revalidation section documents only the targeted fix-wave verification.

**Artifacts**:
- Report: `.mstar/plans/reports/2026-06-18-v1.50-kb-auto-promotion/qc2.md` (this file, appended)
- Review range: `8eec12e5..bab531a1`
- Working branch (verified): `feature/v1.50-kb-auto-promotion`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-kb-auto-promotion`
