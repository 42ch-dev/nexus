---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-18-v1.50-cron-review-staggering"
working_branch: "feature/v1.50-cron-review-staggering"
review_cwd: "/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-cron-review-staggering"
review_range: "merge-base c2831fa25ae7732bac1fe1a11a318e5a7b1626b2..44fe074408d7d5f571f50c4d91069d29f2b6c2b3"
verdict: "Approve"
generated_at: "2026-06-17T14:02:21Z"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: zhipuai-coding-plan/glm-5.2
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-17T14:02:21Z

## Scope
- plan_id: 2026-06-18-v1.50-cron-review-staggering
- Review range / Diff basis: merge-base c2831fa25ae7732bac1fe1a11a318e5a7b1626b2..44fe074408d7d5f571f50c4d91069d29f2b6c2b3
- Working branch (verified): feature/v1.50-cron-review-staggering
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-cron-review-staggering
- Files reviewed: 5 business/test files (1 git-rename + 4 content changes)
- Commit range (identical to Review range): c2831fa2..44fe0744 (4 feature commits)
- Tools run: `git diff`, `git log`, `git show`, `cargo clippy` (4 crates, `-D warnings`), `cargo test` (2 suites), manual source inspection (`cron_supervisor.rs`, `auto_chain.rs::enqueue_cron_schedule`, `supervisor.rs::on_schedule_terminal`, `quality_loop.rs`, `preset_ids.rs`, migrations dir)

### Commits in range (4)
| SHA | Type | Subject |
| --- | --- | --- |
| `12495be8` | `fix(nexus-local-db)` | renumber colliding migration to `202606180003` (R-V150P2CRONRV-01) |
| `b7e438b5` | `fix(nexus-local-db)` | drop partial index before column in schedule_json rollback test (R-V150P2CRONRV-02) |
| `f211aced` | `feat(nexus-orchestration)` | wire review cron role into evaluator (T1-T2) |
| `44fe0744` | `test(nexus-orchestration)` | review cron → T-B P1 hook e2e chain (T4) |

### Alignment verified
- `git branch --show-current` → `feature/v1.50-cron-review-staggering` ✓
- `git rev-parse HEAD` → `44fe074408d7d5f571f50c4d91069d29f2b6c2b3` (matches Assignment tip) ✓
- `git merge-base c2831fa2 HEAD` → `c2831fa25ae7732bac1fe1a11a318e5a7b1626b2` (matches Assignment base) ✓

## Findings
### 🔴 Critical
(none)

### 🟡 Warning
(none)

### 🟢 Suggestion

- **S-001 — Plan-vs-implementation text delta on T-B P1 hook (non-blocking, informational).**
  Plan §2 Goal 4 and §4 AC3 describe "T-B P1 hook stub (function signature only; TODO marker)" and the Code-touch list names `quality_loop.rs` as a stub site. The implementation correctly **did not** add a stub because T-B P1 had **already filled** the real hook (`quality_loop::extract_kb_candidates_for_review`, wired in `schedule::supervisor::on_schedule_terminal` keyed on `preset_id == novel-review-master`). The implementer followed the Surgical / Simplicity principle by not authoring dead code, and re-interpreted AC3 as "the hook chain works" (verified by the new e2e test). The *outcome* is strictly stronger than the plan's literal stub requirement. This is raised only so the PM/architect can reconcile the frozen plan text against reality at archive time (the plan is `InReview` and should not be edited now). No code action required.

## Source Trace

- **Finding ID: S-001**
  - Source Type: manual-reasoning + doc-rule
  - Source Reference: plan `2026-06-18-v1.50-cron-review-staggering.md` §2 Goal 4 / §4 AC3 / Code touch list; `supervisor.rs:469-502` (`COORDINATE-WITH-T-A-P2` comment + filled hook); `quality_loop.rs:262` (`extract_kb_candidates_for_review` is a full implementation, not a stub); Completion Report T3 = "verification, no code".
  - Confidence: High

## Architecture Coherence Assessment (reviewer #1 focus)

### Uniform `enqueue_cron_schedule` path (Option A) — coherent, well-documented
The `review` role reuses the **shared** `try_fire_role` pipeline (`cron_supervisor.rs:220-318`) and the single `crate::auto_chain::enqueue_cron_schedule` (`auto_chain.rs:1577-1631`) for all three roles. There is **no** review-specific special-case branch. Per-Work gating (intake / runtime-lock / completion — `gate_reason`) and §4.2 idempotency (`has_active_role_schedule` with `ACTIVE_STATUS_LIST = 'pending','running','paused'`) are inherited uniformly, so review has parity with brainstorm/write. The plan "Notes for QC" documents the Option A rationale (spec §4.1 is generic; the T-B P1 hook keys off `preset_id`, not the enqueue function; the uniform path preserves cron-vs-SLA provenance). Module doc (`cron_supervisor.rs:1-29`) updated to describe all three roles. Design is clean and the decision is auditable.

### Provenance contract — preserved and origin-distinguishable
`enqueue_cron_schedule` stamps a `CRON` prefix (`CRON{ts}{counter}`) and a `cron:{role}:{work_id}` label. For review this yields `cron:review:<work>`, keeping cron-fired reviews distinguishable from the V1.39 stale-findings `auto-review-master` (`RVM` / `auto-review-master:`) path — exactly as the implementer claims. Verified in `cron_fires_review_role_enqueues_review_master` and the e2e test.

### Cross-plan coordination (review cron → T-B P1 hook) — contract clear
The handoff contract is the `preset_id` value. `on_schedule_terminal` (`supervisor.rs:485-502`) fires `quality_loop::extract_kb_candidates_for_review` iff `preset_id == NOVEL_REVIEW_MASTER_PRESET_ID`, and the hook itself re-checks the same preset (`quality_loop.rs:334`) and early-returns `Ok(0)` otherwise. The hook is **origin-agnostic**: it fires for cron-launched, V1.39 SLA, and manual `creator run` paths. T-A P2 makes the cron path enqueue exactly that preset_id, completing the chain. A `// COORDINATE-WITH-T-A-P2` comment block (`supervisor.rs:476-482`) explicitly documents the cross-plan dependency. The hook is best-effort + non-blocking (errors logged via `tracing::warn`, do not fail the terminal transition). Coherent and well-instrumented.

### R-V150P2CRONRV-01 — migration renumber is correct and conflict-free
Migrations now read: `202606180001_works_schedule_json.sql` (T-A P0), `202606180002_works_schedule_json_partial_idx.sql` (T-A P1), `202606180003_kb_extract_jobs_extend.sql` (T-B P1, renumbered). The renumber is a pure `git mv` (0 content change, no internal version literal). The next free slot is `202606180004` — no collision risk. The plan's claim that "no DB ever recorded either `…0002` cleanly (the collision prevented any successful apply)" is sound: two migrations sharing `_sqlx_migrations.version = 202606180002` fail atomically on the UNIQUE constraint before either records success, so renumbering the later-merged one is safe. `run_migrations` now succeeds (22 + 2 tests pass at the migration step).

### R-V150P2CRONRV-02 — correctly a latent T-A P1 bug masked by R-01
`rollback_drops_schedule_json_column` does `ALTER TABLE works DROP COLUMN schedule_json`. T-A P1 added partial index `idx_works_schedule_json_nonempty` on that column; SQLite refuses `DROP COLUMN` while an index references it (`error in index … no such column: schedule_json`). The fix drops the index first (`works_schedule_migration.rs:113-123`), mirroring a faithful reverse of the T-A P1 migration. The analysis that R-01 *masked* this is correct: R-01's collision made `run_migrations` fail before the rollback assertion executed, so the latent bug was unreachable. Test-only, zero production change. Bisect-safe (each fix commit leaves the tree green for the tests it affects).

### R-V150P1CRONBW-01 (novel-write preset) — still deferred, no impact
`status.json` shows `R-V150P1CRONBW-01` still **open** (novel-write embedded preset YAML not yet authored). T-A P2 does **not** author the `novel-write` preset — it only maps `review` → `novel-review-master`, which is already a shipped preset (`preset_ids.rs:76`). The deferred `novel-write` preset is therefore untouched. `preset_version_for_id` already has explicit arms for `novel-brainstorm` + `novel-write` (R-V150P1CRONBW-05, closed) and tolerates the absent `novel-write` YAML, so the review path's `preset_version` lookup is correct. No impact.

### Surgical scope — clean
Only 5 business/test files in range (1 rename + 4 content). No piggyback refactors, no unrelated edits, no `schemas/` or generated-code drift. The two `fix(nexus-local-db)` commits are scoped cross-plan blocker resolutions with separate, bisect-safe commits — not mixed into the feature commit.

## Test & Lint Evidence

| Gate | Command | Result |
| --- | --- | --- |
| Clippy | `cargo clippy -p nexus-orchestration -p nexus-local-db -p nexus42 -p nexus-daemon-runtime -- -D warnings` | ✅ exit 0, no warnings |
| cron suite | `cargo test -p nexus-orchestration --test cron_supervisor` | ✅ 22 passed; 0 failed (18 T-A P1 + 4 T-A P2) |
| e2e suite | `cargo test -p nexus-orchestration --test review_cron_e2e` | ✅ 2 passed; 0 failed (chain + negative leg) |

Review-specific coverage confirmed: fire+provenance (`cron_fires_review_role_enqueues_review_master`), per-Work gating (`cron_review_respects_per_work_gating`), idempotency (`cron_review_respects_idempotency`), graceful skip (`cron_review_graceful_when_no_review_role_configured`), and the full cross-plan chain (`review_cron_fire_triggers_kb_extraction_hook`). The one pre-existing assertion adjustment (`cron_skips_disabled_role`: `skipped_no_match` 2→3) is correct — review is now the third evaluated role, and the test's intent ("disabled roles don't fire") still holds via `fired == 0`.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 1 |

**Verdict**: Approve

The review cron role is wired with clean architectural parity to brainstorm/write via the uniform `enqueue_cron_schedule` path (Option A), inheriting all per-Work gating and idempotency with no special-case branch. The cross-plan handoff to the T-B P1 KB-extraction hook is contract-clear (preset_id keying, origin-agnostic, best-effort/non-blocking) and proven by a hermetic end-to-end test. Both cross-plan blocker fixes (R-V150P2CRONRV-01 migration renumber, R-V150P2CRONRV-02 latent rollback-test bug) are correctly diagnosed, scoped, and bisect-safe. No unresolved Critical or Warning findings. The single Suggestion is a non-blocking plan-text reconciliation note for archive time.
