---
report_kind: qc-consolidated
plan_id: "2026-06-09-v1.39-research-stage-wiring"
verdict: "Request Changes"
generated_at: "2026-06-09T03:00:00+08:00"
initial_wave: 3
reports:
  - qc1 (qc-specialist, architecture)
  - qc2 (qc-specialist-2, security & correctness)
  - qc3 (qc-specialist-3, performance & reliability)
---

# V1.39 P0.5 Research Stage Wiring — QC Consolidated (Initial Wave)

## Reviewer Metadata
- Plan ID: `2026-06-09-v1.39-research-stage-wiring`
- Plan path: `.mstar/plans/2026-06-09-v1.39-research-stage-wiring.md`
- Iteration compass: `.mstar/iterations/v1.39-novel-auto-chain-and-quality-loop-delivery-compass-v1.md`
- Integration branch: `iteration/v1.39` (HEAD `1b68d6ca` after P0 closeout)
- Topic branch: `feature/v1.39-research-stage-wiring` @ `ea129914`
- Review cwd: `.worktrees/v1.39-p05`
- Review range / Diff basis: `merge-base: 1b68d6ca` + `tip: ea129914` → `git diff 1b68d6ca...ea129914` (1 commit, 4 files, +209 / -2)
- Initial wave: 3 reports
- Reviewer verdict breakdown: qc1 Request Changes (C-1), qc2 Approve (W expected), qc3 Request Changes (W-1, W-2 affect AC1, AC3)
- Consolidated gate: **Request Changes**

## Scope
- plan_id: `2026-06-09-v1.39-research-stage-wiring`
- Review range / Diff basis: `merge-base: 1b68d6ca` (iteration/v1.39 HEAD with P0 closed) + `tip: ea129914` (feature/v1.39-research-stage-wiring HEAD); equivalent to `git diff 1b68d6ca...ea129914` (run in the Review cwd). 1 commit, 4 files, +209 / -2.
- Working branch (verified): `feature/v1.39-research-stage-wiring`
- Review cwd (verified): `.worktrees/v1.39-p05`

## Per-Reviewer Summary

| Reviewer | Focus | Critical | Warning | Suggestion | Verdict |
|---|---|---|---|---|---|
| @qc-specialist (qc1) | Architecture | 1 (C-1) | 3 | 3 | Request Changes |
| @qc-specialist-2 (qc2) | Security + correctness | 0 | 2 | 2 | Approve |
| @qc-specialist-3 (qc3) | Performance + reliability | 0 | 3 (1 affects AC) | 2 | Request Changes |
| **Consolidated** | — | **1** | **5** (3 distinct AC-affecting) | **5** | **Request Changes** |

## Acceptance Criteria Mapping

| AC | Status | Issue |
|---|---|---|
| AC1: auto-chain intake→research→produce without manual advance | ❌ | W-1 (qc3): research preset `synthesizing.exit_when: kind: manual` stalls auto-chain indefinitely |
| AC2: at least one reference/KB artifact queryable after research | ⚠ | W-2 (qc3): research artifacts not in produce input; S-1 (qc2): no end-to-end artifact assertion test |
| AC3: produce context includes research-derived material | ❌ | W-2 (qc3): same as AC2 — produce doesn't see research artifacts in input |
| AC4: research completion does not cancel or duplicate auto-chain driver | ✅ | No issue raised; auto-chain helper reuse confirmed correct |

## Findings (deduplicated)

### 🔴 Critical (machine: critical)

- **C-1** (qc1) — *critical*: 4 daemon-runtime integration tests fail because new research preset gates reject schedules without Work records. The new `gates` (`intake_status == complete` + `work_ref` required) are enforced on the `fl_e_schedule_api` direct-create path, but those tests post schedules without Work context (they test the schedule API surface, not the auto-chain path). **Fix**: (a) update the 4 tests to provide Work context, OR (b) make the research gates conditional on `work_id` presence (so direct schedule creation without `work_id` skips the gates — but then auto-chain enqueue still enforces them).

### 🟡 Warning (machine: medium / low)

- **W-1** (qc3) — *medium (AC-affecting)*: Research preset's `synthesizing` state has `exit_when: kind: manual` and `next: done`. This means after the synthesizing prompt runs, the preset WAITS for human approval before transitioning to `done`. The auto-chain driver in P0 considers `synthesizing → done` as a transition that fires the next enqueue — but `manual` exit means the supervisor never observes a terminal transition from synthesizing. **Net effect: auto-chain stalls at research → never reaches produce.** Breaks AC1. **Fix**: change `synthesizing.exit_when` to `kind: llm_judge` (with a synthesizing-exit.md prompt already present in the preset) so the auto-chain progresses autonomously.

- **W-2** (qc3) — *medium (AC-affecting)*: Research artifacts written to `{$workspace_dir}/.nexus42/references/{$run_id}/report.md` and artifacts/ are not referenced in the produce stage's `AddScheduleRequest.input`. AC2 (artifact queryable) and AC3 (produce sees research) both fail without this wiring. **Fix**: when research stage transitions to produce (in `build_schedule_for_stage` for `produce` after `research`), populate `preset.input.research_artifacts_dir` or similar with the path to the latest research output. The context-assembly layer should expose this to the produce prompt.

- **W-3** (qc1, qc2) — *low*: `run_intents` change from `work_init` to `knowledge_ingest` not impact-assessed. Does any existing CLI command or API call expect `work_init` to surface research? (Likely no, but unverified.) **Fix**: grep the codebase for `work_init` references and document the result. If any consumer needs the change, update it in this slice.

- **W-4** (qc1) — *low*: `version: 2` bump on the research preset lacks a documented versioning policy. What does v2 signal to consumers? **Fix**: add a comment in the preset or in `knowledge/specs/preset-conditional-routing.md` (or wherever versioning policy lives).

- **W-5** (qc1, qc3) — *low*: Status hint in `creator run status` format (`research: in progress` / `research: complete` / `research: done`) is free-form text, not structured. Future i18n / machine consumers will need a structured shape. **Fix**: PM-level decision; defer to V1.40 UX iteration if not already a tracked residual.

### 🟢 Suggestion (not blocking)

- S-1 (qc1): Test coverage gap for research enqueue integration end-to-end (through supervisor + boot paths, not just the pure logic module).
- S-2 (qc1): Consider keeping `work_init` on the research preset for standalone use, in addition to `knowledge_ingest`. The `run_intents` field may be list-friendly.
- S-3 (qc1): i18n extraction for the status hint.
- S-4 (qc2): `enqueue_auto_chain_schedule` hard-codes `preset_version = 1` in the INSERT while the research preset is now v2. Latent hygiene.
- S-5 (qc2): AC2 end-to-end artifact assertion test missing — the new tests are wiring-level, not behavioral at the artifact level.

## Decisions

- **C-1** (critical) → **fix wave** before merge.
- **W-1, W-2** (medium, AC-affecting) → **fix wave** before merge.
- **W-3, W-4, W-5** (low) → triage at PM consolidation; defer to V1.40 hygiene or accept.
- **S-1..S-5** → defer; not blocking.

## Required Fix Wave (before merge)

The implementer must address **C-1 + W-1 + W-2** in a focused fix wave on the same `feature/v1.39-research-stage-wiring` branch. The fix wave is single-dev (not tri-review); after the fix, PM dispatches **targeted re-review** by the 3 QC reviewers (qc1 for C-1 + W-3 + W-4; qc2 for C-1 + W-2 cross-check; qc3 for W-1 + W-2) — per `mstar-review-qc` "targeted re-review" rule (N = 3; same message).

After targeted re-review Approve, PM merges `feature/v1.39-research-stage-wiring` → `iteration/v1.39` and flips plan status to `Done` in `status.json`.

## Source Trace
- qc1: `df0075f6 qc(v1.39-p05): QC1 report — Request Changes (C-1 gate regression in daemon-runtime tests)` (feature/v1.39-research-stage-wiring)
- qc2: `1f1a88c4 qc(qc-specialist-2): initial review for 2026-06-09-v1.39-research-stage-wiring (P0.5 gates, auto-chain wiring, status hint, 6 tests)` (feature/v1.39-research-stage-wiring)
- qc3: `927ac406 qc: V1.39 P0.5 research-stage-wiring — qc3 review (performance/reliability)` (feature/v1.39-research-stage-wiring)

## Summary
| Severity | Count |
|---|---|
| 🔴 Critical | 1 |
| 🟡 Warning (AC-affecting) | 2 |
| 🟡 Warning (low) | 3 |
| 🟢 Suggestion | 5 |

**Verdict**: **Request Changes** — C-1 + 2 AC-affecting Warnings must be fixed before merge.

---

*PM consolidated 2026-06-09. Next dispatch: P0.5 fix wave to `@fullstack-dev` (single dev, scope-locked to C-1 + W-1 + W-2).*
