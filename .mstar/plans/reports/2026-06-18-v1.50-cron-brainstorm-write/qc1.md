---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: 2026-06-18-v1.50-cron-brainstorm-write
working_branch: feature/v1.50-cron-brainstorm-write
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-cron-brainstorm-write
review_range: merge-base 0ea2995ff45569b541b17097c4c919dabab4bb16..f16daaddf616583e1ee85f2a9cfa8c6db7f15b18
verdict: Approve
generated_at: 2026-06-17T19:25:00Z
review_perspective: architecture coherence + maintainability
---

# Code Review Report — V1.50 T-A P1 (cron-brainstorm-write)

## Reviewer Metadata
- Reviewer: @qc-specialist (Reviewer #1)
- Runtime Agent ID: qc-specialist
- Runtime Model: zhipuai-coding-plan/glm-5.2
- Review Perspective: **Architecture coherence + maintainability**
- Report Timestamp: 2026-06-17T19:25:00Z

## Scope
- plan_id: `2026-06-18-v1.50-cron-brainstorm-write`
- Review range / Diff basis: `merge-base 0ea2995ff45569b541b17097c4c919dabab4bb16..f16daaddf616583e1ee85f2a9cfa8c6db7f15b18` (verbatim from Assignment)
- Working branch (verified): `feature/v1.50-cron-brainstorm-write`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-cron-brainstorm-write` (`git rev-parse --show-toplevel`)
- Files reviewed: 12 (4 new files + 8 modified)
- Commit range: identical to Review range — 4 commits: `67db009b` → `ec95eaeb` → `1044020e` → `f16daadd`
- Iteration compass: `.mstar/iterations/v1.50-novel-author-production-loop-and-world-kb-closure-delivery-compass-v1.md` §0.1 + §0.2
- Spec overlay: `.mstar/knowledge/specs/novel-writing/cron-staggering.md` §2.1, §4.1–4.4
- Plan: `.mstar/plans/2026-06-18-v1.50-cron-brainstorm-write.md`
- Tools run: `cargo clippy -p {4 crates} -- -D warnings` (exit 0); `cargo test -p nexus-orchestration --test cron_supervisor` (18/18 pass); `cargo test -p nexus-daemon-runtime --test cron_supervisor_task` (2/2 pass); `cargo test -p nexus-orchestration --lib schedule::cron_supervisor` (13/13 pass); `cargo test -p nexus-orchestration --lib preset_ids` (6/6 pass); `cargo +nightly fmt -p {4 crates} -- --check` (exit 0)

## Architecture assessment

The implementation introduces a **well-bounded new subsystem** with clear separation of concerns:

```
nexus-orchestration::schedule::cron_supervisor  (pure evaluator: read → gate → idempotency-check → enqueue)
nexus-orchestration::auto_chain::enqueue_cron_schedule  (out-of-band INSERT, mirrors enqueue_review_master_schedule)
nexus-local-db::works  (scan DAO + transactional CAS write)
nexus-daemon-runtime::cron_supervisor  (background task: periodic tick → evaluator → admission)
nexus42::commands::creator::works::cron  (CLI CAS write path, closes R-V150P0-W5)
```

### Strengths

1. **Out-of-band fire contract is the right abstraction.** Cron-fired schedules do **not** touch `works.driver_schedule_id`, mirroring the established `enqueue_review_master_schedule` pattern. The contract ("cron fire is an independent production nudge, never disrupts an in-progress FL-E chain") is documented in **three** places: the `cron_supervisor.rs` module doc (lines 19–22), the `enqueue_cron_schedule` function doc (lines 1541–1555), and the `preset_ids.rs` constants doc (lines 38–50). The `runtime_lock_holder` gate in `gate_reason` (lines 304–315) provides the actual serialization: a cron fire cannot land while any schedule holds the Work lock, so cron-vs-FL-E concurrency is impossible by construction.

2. **The plan's stated "extend `evaluate_next_step`" was correctly replaced with a superior design.** `evaluate_next_step` is the FL-E driver evaluator (decides the next in-chain stage); cron fires are explicitly out-of-band. Conflating them would have coupled two concerns. Instead the implementer added a sibling `enqueue_cron_schedule` that mirrors `enqueue_review_master_schedule` — the existing out-of-band enqueue pattern. This is architecturally cleaner than the plan text.

3. **`set_schedule_json_tx` is the right CAS abstraction and handles all three failure modes correctly:**
   - SQL error → `LocalDbError::Sqlx` → CLI maps to `CliError::Other`.
   - Stale preimage → `Ok(false)` → CLI rolls back tx, returns `CliError::Config` with a clear retry hint.
   - Missing Work → `LocalDbError::MissingVersionKey` (re-read inside the tx disambiguates from CAS mismatch) → CLI maps to `CliError::Other`.
   - NULL/empty-string normalization (via `COALESCE`) matches spec §2.3 ("empty = unset = use defaults").
   Closes R-V150P0-W5.

4. **`CRON_COUNTER` atomic** (lines 39–42 + 1568–1572) mirrors the R-V139P0-W-B / R-V147P0-05 collision-resistant ID pattern: `CRON<14-digit-date><3-digit-ms><6-hex-counter>` produces distinct PKs even when two roles fire in the same millisecond. The `& 0x00FF_FFFF` mask (24 bits, 16M fires/process) is adequate.

5. **Surgical changes confirmed.** `boot.rs` section 4c is purely additive (24 lines, inserted between stale-findings-watcher and agent-host). No opportunistic refactor in `supervisor.rs`. `lib.rs` adds one `pub mod`. `cron.rs` (CLI) replaces 4 lines of TOCTOU-unsafe write with 30 lines of CAS-guarded write + removes the TODO marker. The rest of the CLI file is untouched.

6. **Leaky-abstraction risk is mitigated with a documented rationale.** The daemon's minimal `CronConfig` / `CronRoles` / `CronRole` mirror structs (lines 81–106) duplicate the CLI's `WorkSchedule` JSON shape. The rationale (circular-dep avoidance: `nexus42 → nexus-orchestration`) is explicitly documented in the module-level comment (lines 73–79). `Option<CronRole>` fields make partial configs robust.

7. **Observability is well-tiered.** `info!` on fire/sweep, `debug!` on gated/no-match skip, `warn!` on parse/enqueue failure — matches the S-002 tracing requirements. `CronFireSummary` struct (lines 46–71) provides structured counts for tests and future metrics.

8. **Idempotency guard is correct.** `has_active_role_schedule` counts `pending/running/paused` schedules for `(work_id, preset_id)` — matching spec §4.2. The `cron_refires_after_prior_schedule_terminal` integration test (lines 356–393) confirms the guard is "active-only" (terminal schedules allow re-fire).

9. **Hermetic test coverage is thorough.** 33 test cases total: 13 unit tests in the module (cron matching, TZ, normalization, gating, role mapping); 18 integration tests (fire/skip/idempotent/gating/CAS/partial-index); 2 daemon-runtime integration tests (tick → enqueue → admit). All four acceptance criteria from the plan are covered with the prescribed hermetic analogs.

### Concerns raised in Assignment — disposition

| Assignment concern | Disposition |
|---|---|
| New `schedule::cron_supervisor` module is well-bounded; no leaky abstractions | ✅ Confirmed — clean read-only evaluator with a single public `evaluate_cron_fires` entry point + `CronFireSummary` return type. No state, no side effects beyond enqueue. |
| Reuse of V1.39 `auto_chain::evaluate_next_step` for downstream stage execution | ⚠️ See S-005 below: the plan text said "extend `evaluate_next_step`" but the implementer correctly added a sibling `enqueue_cron_schedule` instead. The actual reuse is of `enqueue_review_master_schedule` (out-of-band enqueue pattern) + the existing `ScheduleSupervisor::tick_clocked` admission path + the existing executor. This is architecturally superior; flagging only as a plan-vs-implementation documentation delta. |
| `set_schedule_json_tx` transactional CAS pattern — right abstraction? failure handling? | ✅ Right abstraction; all three failure modes handled correctly (see Strengths #3). |
| "Out-of-band fire" decision — conflict with future FL-E? Contract documented? | ✅ No conflict — `runtime_lock_holder` gate prevents concurrency; contract documented in three places. Future T-A P2 / T-A P3 daemons that also enqueue out-of-band will follow the same pattern. |
| `// COORDINATE-WITH-T-A-P2` code comment is sufficient for handoff | ⚠️ See S-002 below: the literal `COORDINATE-WITH-T-A-P2` marker is **absent**; the handoff IS documented in three places with different wording. Substance is sufficient, marker is not. |
| Surgical changes only — no opportunistic refactor in `boot.rs` or `supervisor.rs` | ✅ Confirmed — `boot.rs` +24 lines additive; `supervisor.rs` untouched. |

## Findings

### 🔴 Critical

(none)

### 🟡 Warning

(none)

### 🟢 Suggestion

#### S-001 — Idempotency-check error path skips summary counter

`try_fire_role` (lines 252–299) has three branches for `has_active_role_schedule`:
- `Ok(true)` → `summary.skipped_idempotent += 1` ✅
- `Ok(false)` → continues to enqueue (increments `summary.fired` on success, or nothing on enqueue error) ✅
- `Err(e)` → logs `warn!` and **does not increment any counter** ⚠️ (lines 291–298)

This means a Work whose idempotency check errors is silently absent from `CronFireSummary::total_evaluated()`. The behaviour (skip + retry next tick, non-fatal) is correct, but the metric understates work done. Trivial fix: increment `summary.skipped_idempotent` (or add a new `skipped_idempotency_check_error` bucket) in the `Err` arm.

**Confidence:** High (direct code reading).
**Impact:** Observability gap only; no functional defect.

#### S-002 — `// COORDINATE-WITH-T-A-P2` literal marker absent

The Assignment expected a grep-friendly `// COORDINATE-WITH-T-A-P2` comment marking the handoff to the T-A P2 review-cron implementer. The handoff **is** documented in three places:

- Module doc, `cron_supervisor.rs:23-26`: `//! Only brainstorm + write roles are evaluated. review cron firing is T-A P2 (non-goal per plan §3).`
- Struct comment, `cron_supervisor.rs:93`: `// review is intentionally absent — T-A P2 (plan §3 non-goal).`
- Function doc, `cron_supervisor.rs:96`: `/// Returns None for roles out of scope for T-A P1 (e.g. review).`

The substance is sufficient for a T-A P2 implementer to find the extension points (add `review` to `CronRoles`, add `ROLE_REVIEW` constant, extend `role_preset` match). A literal `// COORDINATE-WITH-T-A-P2:` marker would make `git grep COORDINATE` discovery faster.

**Confidence:** High (diff inspection).
**Impact:** Minor discoverability; not a blocker.

#### S-003 — `set_schedule_json_tx` rationale prose slightly imprecise

The doc comment (lines 1521–1527 of `works.rs`) says:

> "the daemon-side cron evaluator (T-A P1) is the racing party against the CLI `creator works cron set` writer. Both now go through a CAS-guarded path — the CLI uses this `_tx` variant in `handle_set` ... and the daemon cron evaluator does not write `schedule_json` (it only reads)."

This is internally contradictory: it first calls the daemon evaluator "the racing party" then admits the evaluator "does not write `schedule_json`". The actual race protected by the CAS is between **two concurrent CLI invocations** (or future `works.schedule_json` mutators such as the T-A P3 auto-chronology task). The CAS itself is correctly implemented and necessary; only the rationale prose is misleading.

**Suggested fix:** Reword to "The CAS protects against concurrent CLI invocations and future daemon-side `schedule_json` mutators (e.g. T-A P3 auto-chronology). The T-A P1 cron evaluator is read-only on this column." 

**Confidence:** High (code reading: `cron_supervisor.rs` writes only to `creator_schedules`, never to `works.schedule_json`).
**Impact:** Documentation correctness; no functional defect.

#### S-004 — `enqueue_cron_schedule` near-duplicates `enqueue_review_master_schedule`

The two functions (`auto_chain.rs:1485-1536` and `auto_chain.rs:1538-1614`) share ~90% of their body: counter fetch + mask, ID format string, INSERT column list, error mapping. A future cleanup could extract a shared helper, e.g.:

```rust
async fn enqueue_out_of_band_schedule(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: &str,
    preset_id: &str,
    label_prefix: &str,
    id_prefix: &str,
    counter: &AtomicU32,
) -> Result<String, AutoChainError>
```

Not a blocker — the duplication is explicitly acknowledged in code comments ("Mirrors the `enqueue_review_master_schedule` pattern"), the surgical-changes discipline argues against refactoring in this plan, and the two functions may diverge in T-A P2/P3 (review-master has different preset_version semantics).

**Confidence:** High.
**Impact:** Maintainability — a change to the INSERT shape (e.g. new column) must be applied in two places.

#### S-005 — Plan text says "extend `evaluate_next_step`"; implementation added `enqueue_cron_schedule` instead

Plan §2 Goal 4: "Cron fire enqueues a `Schedule` with `preset_id = <role-preset>` and `work_ref` from source Work." Plan §5 T4: "Schedule enqueue path." Plan `Code touch`: "`crates/nexus-orchestration/src/auto_chain.rs` (extend `evaluate_next_step` for cron-launched schedules)."

The implementation did **not** extend `evaluate_next_step` (which is the FL-E driver evaluator and would have been the wrong place for out-of-band cron fires). Instead it added a sibling `enqueue_cron_schedule` that mirrors the existing `enqueue_review_master_schedule` out-of-band pattern. **The implementation is architecturally superior to the plan text.** No action needed on the code; flagging only so the plan-vs-implementation delta is on record.

**Confidence:** High.
**Impact:** Plan accuracy; no code action.

#### S-006 — No drift guard between CLI `WorkSchedule` and daemon `CronConfig` mirrors

The daemon's minimal `CronConfig` / `CronRoles` / `CronRole` structs (lines 81–106) intentionally duplicate the JSON shape serialized by the CLI's `WorkSchedule` model (in `nexus42::commands::creator::works::cron`). The circular-dep avoidance is documented (lines 73–79). Risk: if the CLI ever adds a new role (e.g. `review` in T-A P2), renames a field, or changes the JSON shape, the daemon will silently skip the new data until its mirror is updated in lockstep. There is no test asserting that a CLI-serialized blob round-trips through the daemon deserializer.

**Suggested follow-up (not a blocker for T-A P1):** Either (a) add a cross-crate serialization compatibility test in `crates/nexus-orchestration/tests/` that constructs a `WorkSchedule` via the CLI's public API, serializes it, and asserts the daemon's `CronConfig` deserializes it with the expected fields; or (b) if the circular-dep can be broken by extracting the wire shape into `nexus-contracts::local`, share a single type.

**Confidence:** High.
**Impact:** Future-proofing; no current defect (the T-A P1 shape is locked by spec §2.1).

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|---|---|---|---|
| S-001 | manual-reasoning | `crates/nexus-orchestration/src/schedule/cron_supervisor.rs:291-298` (Err arm of `has_active_role_schedule` has no counter increment) | High |
| S-002 | git-diff | `git diff 0ea2995f..HEAD -- crates/nexus-orchestration/src/schedule/cron_supervisor.rs` — no `COORDINATE-WITH-T-A-P2` literal; handoff documented at lines 23-26, 93, 96 | High |
| S-003 | manual-reasoning | `crates/nexus-local-db/src/works.rs:1521-1527` doc comment; `cron_supervisor.rs` writes only to `creator_schedules` via `enqueue_cron_schedule` | High |
| S-004 | manual-reasoning | `crates/nexus-orchestration/src/auto_chain.rs:1485-1536` vs `1538-1614` | High |
| S-005 | doc-rule | `.mstar/plans/2026-06-18-v1.50-cron-brainstorm-write.md` §Code touch vs `git diff 0ea2995f..HEAD -- crates/nexus-orchestration/src/auto_chain.rs` | High |
| S-006 | manual-reasoning | `cron_supervisor.rs:73-106` (mirror structs + circular-dep rationale); no cross-crate round-trip test in `crates/nexus-orchestration/tests/` | High |

## Verification evidence

```
$ cargo clippy -p nexus-orchestration -p nexus-local-db -p nexus42 -p nexus-daemon-runtime -- -D warnings
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.29s
(exit 0 — clean)

$ cargo test -p nexus-orchestration --test cron_supervisor
running 18 tests
... all pass ...
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured

$ cargo test -p nexus-daemon-runtime --test cron_supervisor_task
running 2 tests
... all pass ...
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured

$ cargo test -p nexus-orchestration --lib schedule::cron_supervisor
running 13 tests
... all pass ...
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured

$ cargo test -p nexus-orchestration --lib preset_ids
running 6 tests
... all pass (incl. new novel_brainstorm/novel_write frozen-value guards) ...

$ cargo +nightly fmt -p nexus-orchestration -p nexus-local-db -p nexus42 -p nexus-daemon-runtime -- --check
(exit 0 — clean)
```

The implementer's note that full-workspace `cargo clippy --all` was not run locally (R-V150P1CRONBW-02) is acceptable per repo `AGENTS.md` `target/`-hygiene policy for daily iteration; the authoritative `--all` gate runs in CI on the PR. All four touched crates are clean at the scoped level.

## Residual note

- **R-V150P0-W5** (handle_set TOCTOU): marked `resolved` in `status.json` with `closure_note` + `resolution.{commit, plan_id}`. Confirmed closed by this review (CAS path implemented + 4 hermetic tests). ✅
- **R-V150P1CRONBW-01** (`novel-write` preset not authored): correctly registered as `medium / defer`. The evaluator enqueues the right `preset_id` per spec §2.1; the gap is in preset-authoring scope, not evaluator scope. The plan correctly does **not** treat this as a T-A P1 blocker. ✅
- **R-V150P1CRONBW-02** (full `--all` clippy gate deferred to CI): correctly registered as `low / accept`. ✅

No new residuals introduced by this review (all six findings are Suggestion-level).

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 6 |

**Verdict**: **Approve**

The cron-brainstorm-write implementation is architecturally coherent, well-bounded, and surgical. The new `schedule::cron_supervisor` module is a clean read-only evaluator with no leaky abstractions. The out-of-band fire contract is correctly modeled after `enqueue_review_master_schedule` and documented in three places. The `set_schedule_json_tx` CAS closes R-V150P0-W5 with correct handling of all three failure modes. Test coverage is thorough (33 cases across unit + integration + daemon-runtime). The six suggestions are all non-blocking maintainability / documentation / future-proofing observations.

No blocking items. Plan is ready for PM consolidation with QC #2 (security/correctness) and QC #3 (performance/reliability).
