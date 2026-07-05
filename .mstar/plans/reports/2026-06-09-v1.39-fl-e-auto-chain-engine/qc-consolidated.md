---
report_kind: qc-consolidated
plan_id: "2026-06-09-v1.39-fl-e-auto-chain-engine"
verdict: "Request Changes"
generated_at: "2026-06-09T02:00:00+08:00"
initial_wave: 3
reports:
  - qc1 (qc-specialist, architecture)
  - qc2 (qc-specialist-2, security & correctness)
  - qc3 (qc-specialist-3, performance & reliability)
---

# V1.39 P0 FL-E Auto-Chain Engine — QC Consolidated (Initial Wave)

## Reviewer Metadata
- Plan ID: `2026-06-09-v1.39-fl-e-auto-chain-engine`
- Plan path: `.mstar/plans/2026-06-09-v1.39-fl-e-auto-chain-engine.md`
- Iteration compass: `.mstar/iterations/v1.39-novel-auto-chain-and-quality-loop-delivery-compass-v1.md`
- Integration branch (QC Working branch): `iteration/v1.39`
- Topic branch (feature under review): `feature/v1.39-fl-e-auto-chain-engine` @ `c143da1f`
- Review cwd: `.worktrees/v1.39-p0`
- Review range / Diff basis: `merge-base: c7a3fac1` + `tip: c143da1f` → `git diff c7a3fac1...c143da1f` (15 commits, 14 files, +2034 / -54)
- Initial wave: 3 reports (qc1, qc2, qc3)
- Reviewer verdict breakdown: qc1 Approve, qc2 Approve, qc3 Approve
- Consolidated gate: **Request Changes** (per `mstar-review-qc` rule "存在未解决的 Critical 或 Warning → Request Changes"; reporters classified all 9 Warnings as non-blocking hygiene, but the gate rule is explicit; PM consolidates strictly)

## Scope
- plan_id: `2026-06-09-v1.39-fl-e-auto-chain-engine`
- Review range / Diff basis: `merge-base: c7a3fac1` (iteration/v1.39) + `tip: c143da1f` (feature/v1.39-fl-e-auto-chain-engine HEAD); equivalent to `git diff c7a3fac1...c143da1f` (run in the Review cwd). 15 commits, 14 files, +2034 / -54.
- Working branch (verified): `feature/v1.39-fl-e-auto-chain-engine`
- Review cwd (verified): `.worktrees/v1.39-p0`

## Per-Reviewer Summary

| Reviewer | Focus | Critical | Warning | Suggestion | Verdict |
|---|---|---|---|---|---|
| @qc-specialist (qc1) | Architecture coherence + maintainability | 0 | 3 | 3 | Approve |
| @qc-specialist-2 (qc2) | Security + correctness | 0 | 3 | 3 | Approve |
| @qc-specialist-3 (qc3) | Performance + reliability | 0 | 3 | 3 | Approve |
| **Consolidated** | — | **0** | **9** (6 distinct) | **9** | **Request Changes** |

## Acceptance Criteria Mapping (AC1..AC6)

| AC | Status | Evidence |
|---|---|---|
| AC1: default auto-chains chapter 1 | ✅ | `tests/auto_chain.rs::fix1_terminal_completed_enqueues_next_stage` + `fix1_chapter_loop_after_persist` + `fix1_last_chapter_marks_work_complete` (21 green) |
| AC2: chapter N → N+1 auto-enqueue | ✅ | `fix1_chapter_loop_after_persist` + `fix2_boot_resume_enqueues_next_schedule` |
| AC3: completion stops auto-enqueue | ✅ | `fix1_last_chapter_marks_work_complete` + `ac3_persist_last_chapter_marks_complete` + `ac3_mark_work_completed_in_db` |
| AC4: daemon restart auto-resumes checkpointed driver | ✅ | `fix2_boot_resume_enqueues_next_schedule` + `fix2_boot_resume_interrupted_work_not_resumed` (boot path verified) |
| AC5: `--note` does not fork driver | ✅ | T6 enforcement (409 on side-input with active driver) + 14 prior tests; the diff adds fresh-DB-read discipline (qc2 confirmed) |
| AC6: `--no-auto-chain` disables enqueue but writes checkpoint | ✅ | `ac6_auto_chain_disabled_no_action` + `ac6_checkpoint_fields_persisted_in_db` |

All 6 ACs satisfied at the test level. Warnings are non-blocking hygiene; QC reporting layer recommends them, but the ACs themselves are met.

## Findings (deduplicated, machine severity per `mstar-plan-artifacts`)

### 🟡 Warning (machine: medium, except where noted)

- **W-A** (qc1 W-1, qc2 W-1) — *medium*: Duplicate enqueue logic between `boot.rs:resume_auto_chain_work` and `supervisor.rs:enqueue_auto_chain_step` (both mint ACH ID + INSERT pending + `set_driver`). Maintenance hazard for the single-FL-E-driver invariant. → Fix: extract shared helper, e.g. `enqueue_auto_chain_schedule(...)` used by both paths.
- **W-B** (qc2 W-2) — *low*: ACH timestamp ID format `%Y%m%d%H%M%S%3f` lacks entropy. Under concurrent writers that both observe `driver_schedule_id = null`, two ACH IDs could collide in the same millisecond (low probability, but real). → Fix: append a ULID suffix, or add per-creator monotonic counter.
- **W-C** (qc2 W-3) — *low*: `creator run resume` only clears `auto_chain_interrupted`; no synchronous nudge / tick / enqueue. The chain stays idle until the next supervisor cycle. → Fix: after clearing the flag, call `tick()` (or a tiny `kick()`) so the resumed Work progresses in the same request.
- **W-D** (qc3 W-1) — *medium*: `patch_work_stage` Fix-3 split: `apply_non_stage_fields` commits non-stage fields before stage-advance validation runs. If the stage advance transaction fails, partial state is already persisted. → Fix: reorder — run `check_stage_advance` / `check_stage_status_transition` first; only then apply non-stage fields inside the same transaction.
- **W-E** (qc3 W-2) — *medium*: Boot resume query (`find_resumable_works` JOIN) lacks index coverage on `(auto_chain_enabled, auto_chain_interrupted, status)`. The migration adds columns but no index. At 1000+ Works, SQLite will full-scan. → Fix: add `CREATE INDEX works_auto_chain_resume ON works(auto_chain_enabled, auto_chain_interrupted, status)` to the same migration.
- **W-F** (qc3 W-3) — *low*: `tick_inner` loads ALL `creator_schedules` rows regardless of status on every terminal event. Auto-chain increases completion frequency and amplifies this O(N) cost over time. → Fix: scope the SELECT to `pending | running` only.

### 🟢 Suggestion (not blocking; will be triaged after merge)

- S-1 (qc1): Add `on_schedule_terminal` idempotency on rapid duplicate terminal signals
- S-2 (qc1): Document the "single FL-E driver" invariant in the creator-workflow spec §5.4
- S-3 (qc1): CLI `creator run status` auto-chain fields are great — add a `--json` output for scripts
- S-4 (qc2): Add a test for `WorkPatch` neutralization of new fields via PATCH (creator isolation)
- S-5 (qc2): Document the implicit boot filter contract in supervisor.rs doc-comment
- S-6 (qc2): Add partial index on `driver_schedule_id` for fast "find by driver" lookup
- S-7 (qc3): Remove redundant SSOT re-fetch in fix-1 path (the new `get_work` inside `process_auto_chain_after_terminal` is correct but a bit redundant with the supervisor's existing call site — extract or note)
- S-8 (qc3): Migration should migrate to compile-time macros where possible (the DDL exception is OK, but the SELECT/UPDATE in supervisor can move from `sqlx::query!` to `sqlx::query_as!` if a struct exists)
- S-9 (qc3): Silent swallow of missing preset mapping — consider returning a typed error in the boot path so observability catches the misconfig

## Decisions

- **W-A, W-D, W-E** (medium severity) → **fix wave** in this plan before merge. These touch the state-machine correctness surface (W-A single-FL-E invariant, W-D atomic PATCH, W-E boot performance). Targeted re-review by qc1 (W-A) + qc2 (W-A) + qc3 (W-D, W-E) after the fix.
- **W-B, W-C, W-F** (low severity) → **defer** to a hygiene follow-up plan (or V1.40 iteration). These are correct in behavior; just not optimal.
- **S-1..S-9** (Suggestions) → triage at PM consolidation; defer to follow-up if not already addressed.

## Required Fix Wave (before merge)

The implementer must address **W-A, W-D, W-E** in a focused fix wave on the same `feature/v1.39-fl-e-auto-chain-engine` branch. The fix wave is single-dev (not tri-review); after the fix, PM dispatches **targeted re-review** by the 3 QC reviewers that raised these findings (qc1 for W-A architecture; qc2 for W-A correctness; qc3 for W-D + W-E perf/reliability) — per `mstar-review-qc` "targeted re-review" rule (N = listed seats in Assignment; same message).

After targeted re-review Approve, PM merges `feature/v1.39-fl-e-auto-chain-engine` → `iteration/v1.39` and flips plan status to `Done` in `status.json`.

## Source Trace
- qc1: `16263689 qc(review): QC #1 report for V1.39 P0 auto-chain engine — Approve` (committed to feature branch in worktree)
- qc2: `1d68a5a4 qc2: security & correctness review for 2026-06-09-v1.39-fl-e-auto-chain-engine (Approve)` (committed to feature branch in worktree)
- qc3: `2691a393 qc: V1.39 P0 auto-chain engine QC review #3 (performance/reliability)` (cherry-picked from iteration/v1.39 → feature branch in worktree after PM caught a QC branch-discipline violation; the report content is valid)

## Summary
| Severity | Count |
|---|---|
| 🔴 Critical | 0 |
| 🟡 Warning (medium / low) | 9 (6 distinct) |
| 🟢 Suggestion | 9 |

**Verdict**: **Request Changes** — ACs all met at the test level, but the gate rule is strict on unresolved Warnings. Targeted fix wave on W-A, W-D, W-E (medium); then targeted re-review; then merge.

---

*PM consolidated 2026-06-09. Next dispatch: P0 fix wave to `@fullstack-dev` (single dev, scope-locked to W-A + W-D + W-E).*
