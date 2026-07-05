---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-09-v1.39-v138-hardening"
verdict: "Approve"
generated_at: "2026-06-09"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: security and correctness risk (focus per role parameters)
- Report Timestamp: 2026-06-09

## Scope
- plan_id: 2026-06-09-v1.39-v138-hardening
- Review range / Diff basis: merge-base: 1b68d6ca + tip: 24919b27; equivalent to `git diff 1b68d6ca...24919b27` (run in the Review cwd)
- Working branch (verified): feature/v1.39-v138-hardening
- Review cwd (verified): .worktrees/v1.39-p5
- Files reviewed: 5
- Commit range: 932097ea (local-db doc+NULL) .. 24919b27 (plan checkboxes); 4 commits total
- Tools run: git rev-parse/branch/log/diff-stat, cargo clippy (clean), cargo test (nexus-local-db::test_is_work_completed 4/4 pass; nexus-daemon-runtime works_api::handler_get_work_lazy_promotes 1/1 pass; nexus-orchestration auto_chain+research pass; nightly fmt --check clean), file reads of plan/status/diff/sources

## Findings

### 🔴 Critical
- (none)

### 🟡 Warning
- (none new from this delta; prior R-V138P0-01/03 medium residuals addressed via documentation + contract tests per plan T3/T4; see analysis below)

### 🟢 Suggestion
- Consider adding a daemon-side post-finalize hook (mentioned in the new get_work doc) in a future iteration to make the lazy promotion a no-op at the HTTP boundary (observability + principle of least surprise). Current implementation is explicitly contracted and safe under the documented single-writer model.
- The `reject_produce_when_novel_complete` guard is CLI-stage_advance only. If direct `POST /v1/local/schedules` (or internal orchestration paths) can target a "produce" stage for a novel-complete Work without going through `creator run stage advance`, an empty-chapter schedule could still be created. Current callers (CLI, stage_gates tests) all flow through the guarded path; no bypass observed in reviewed scope. (Low impact given local-first model and residual was specifically about stage_advance latent.)

## Source Trace
- Finding ID: N/A (accept-with-doc + guard implementation for listed residuals)
- Source Type: git-diff + manual code review + test execution + doc contract review
- Source Reference: git diff 1b68d6ca...24919b27; crates/nexus-local-db/src/work_chapters.rs:509-523 (next_chapter doc), 591-594 (NULL/0 guard); crates/nexus-daemon-runtime/src/api/handlers/works.rs:320-349 (lazy contract doc) + 373-416 (implementation + idempotent guard) + new test at works_api.rs:1220; crates/nexus42/src/commands/creator/run.rs:1011-1025 (reject fn) + 1175 (call site) + 1366-1407 (3 unit tests); .mstar/plans/2026-06-09-v1.39-v138-hardening.md (T1-T4 checkboxes)
- Confidence: High

## Analysis (per Reviewer #2 focus: security + correctness)

**R-V138P0-01 (race window, accept-with-doc)**: The added docstring in `next_chapter()` explicitly states the "local-first single-user: exactly one writer for any given work_id at a time (one CLI / one creator)" invariant and explains why the single MIN query is safe (no concurrent transaction can advance between read and caller's subsequent UPDATE). Usage patterns (creator run continue/resume, --note side input) all go through the same CLI writer path. No daemon thread currently observes driver_schedule_id=null and concurrently calls next_chapter for chapter status mutation in a way that races the user CLI (auto-chain is stage-level, not chapter-claim). The "claim" alternative was explicitly not implemented (plan T4 chose "document"); this matches the "accept-with-doc" disposition in status. Safe under current model; future concurrent-writer change would require the atomic claim the doc already sketches.

**R-V138P0-03 (write-on-read in get_work)**: Handler now carries a comprehensive contract doc explaining the intentional lazy promotion (derived state, platform requires persisted 'completed', idempotent via `status != "completed"` early guard, failure-tolerant (warn+return unpromoted), single-writer safe). The new `handler_get_work_lazy_promotes_completed_then_is_idempotent` test (hermetic, 3 GETs) directly exercises: pre-GET status != completed, 1st GET promotes + returns completed, 2nd/3rd GET return completed with *unchanged* updated_at (no re-PATCH). This is robust for the documented contract. The test does not simulate "another handler finalizes between GETs" because under single-writer + the early guard, once promoted the check is skipped; any concurrent finalizer would be a violation of the same invariant already documented for next_chapter. Least-surprise is addressed by making the side-effect part of the *published handler contract*. No security issue (local-only, no untrusted input controls the promotion trigger).

**R-V138P0-05 (NULL/0 test)**: The 4 new unit tests (`test_is_work_completed_false_when_total_planned_chapters_null`, `_zero`, `one_draft`, `all_finalized`) are at the right level: they call the public `is_work_completed` predicate directly (lib tests) and assert the early-return behavior added in the match on total. They are not trivial (they set up the works row with NULL/0 and verify false before any chapter logic). They make the §6.1 guard self-documenting exactly as the residual requested. Pass in CI verification.

**R-V138P1-01 (completion guard)**: `reject_produce_when_novel_complete` returns `CliError::Other` with actionable, tagged message ("NOVEL_COMPLETE", explicit hint to advance to 'persist' instead, includes work_id). Called inside `stage_advance` (the path that was the latent in the residual) *after* chapter context extraction from the GET /works response (which itself uses the daemon's next_chapter view). The 3 unit tests cover: errors on produce+None, allows with Some(ch), skips non-produce stages. Tests pass. 

  Bypass question (direct POST produce schedule): The guard lives only in the CLI `stage_advance` path + `build_schedule_for_stage` call. Direct daemon API schedule creation (or internal auto-chain produce enqueue for a completed novel) would not hit this Rust fn. However: (a) the residual was specifically "stage_advance for produce creates schedule...", (b) plan AC targets "Completion path" (the documented user path), (c) no other call sites in reviewed diff create empty-chapter novel-writing schedules, (d) next_chapter=None decision comes from daemon (consistent source of truth). If a future path bypasses stage_advance entirely, it would be a new residual (not introduced here). Current implementation closes the reported latent for the primary creator UX.

**R-V138P0-02 (CLI missing-file hints, accept)**: No code change in this delta for the status surface (T9 partial from P0). Rationale in residual remains valid: DB is SSOT, reconcile-chapters exists for remediation, explicit on-disk warning was out-of-scope for P0 delivery. The residual R-V138P0-02 is still open in status.json (tracked, low, defer to backlog). "Out of scope" for this P5 hardening is correct per plan non-goals and triage (T1 marked done without touching this).

**R-V138P0-04 (chapters uncapped, accept)**: No change. DoS mitigated by: total_planned_chapters is set at Work *init* by the *user* (not attacker-controlled), local DB (no untrusted network clients), typical novel scale <<100 chapters. Upstream validation not required for this threat model. Accept stands.

**R-V138P1-04 (template required without defaults, accept)**: No change. Per residual note: "Current callers all populate via stage_advance." Reviewed call sites (CLI stage_advance, stage_gates tests, build_schedule_for_stage) all supply the fields. Non-CLI direct preset.input construction (e2e, future daemon API) could hit required: true, but that is the documented expansion path in the residual. Accept correct; no current bypass in scope.

**Shared baseline**: All verification commands green (clippy -D warnings clean on the 3 crates; relevant tests pass including the new ones; fmt clean). No behavior regression for P0/P1 novel paths. Surgical changes (docs + 1 pure guard fn + 1 handler extension + 1 test module addition). No new security surface (no new inputs, no authz changes, local-only). Maintainability improved by explicit contracts.

**Branch discipline**: Verified in review cwd on the exact Working branch and range. Only report will be committed. Zero source modifications outside reports/.

**Residual decisions**: Agree with all listed (R-V138P0-01/03/05, R-V138P1-01 documented+guarded+tested; R-V138P0-02/04, R-V138P1-04 accept with rationale). No disagreements. T5 (status.json residual_findings update) remains open (expected — per mstar-review-qc / mstar-plan-artifacts, QC does not write status.json; PM/QA own lifecycle closure).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 (new) |
| 🟢 Suggestion | 2 (future hygiene) |

**Verdict**: Approve

## Verification Evidence
- All 5 verification commands executed in Review cwd and passed (see Tools run).
- 4 new tests + 3 guard unit tests exercising the exact contracts added for the residuals.
- Diff limited to documentation of assumptions, explicit error guard at the documented latent path, and idempotency coverage for the write-on-read.
- No new Critical or high-impact correctness issues introduced.

(End of report)
