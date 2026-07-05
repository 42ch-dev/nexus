---
report_kind: qa
reviewer: qa-engineer
plan_id: "2026-06-13-v1.44-manuscript-audit-preset"
verdict: "Approve"
generated_at: "2026-06-13T04:34:55Z"
review_range: "068135ed..43550686"
working_branch: "iteration/v1.44"
review_cwd: "/Users/bibi/workspace/organizations/42ch/nexus"
---

# QA Report — V1.44 P0 `novel-manuscript-audit` preset + CLI entry

## Reviewer Metadata

- Reviewer: @qa-engineer
- Runtime Agent ID: qa-engineer
- Runtime Model: volcengine-plan/ark-code-latest
- QA Mode: report-only acceptance verification
- Report Timestamp: 2026-06-13T04:34:55Z
- Verdict: Approve

## Scope

Verbatim assignment scope:

- plan_id: `2026-06-13-v1.44-manuscript-audit-preset`
- Review range / Diff basis: `068135ed..43550686` (full P0 scope: 5 original commits + 1 original merge + 3 fix commits + 1 fix-merge + 1 plan §6 fix)
- Working branch: `iteration/v1.44`
- Review cwd / Worktree path: `/Users/bibi/workspace/organizations/42ch/nexus`
- In: review commits `068135ed..43550686` on `iteration/v1.44`; verify behavior matches plan §4 AC + spec §3.1 (review) + §3.2 (extract) + DF-69 contract.
- Out: P1 / P2; QC report content (already approved by 3 QC reviewers post fix-wave).

Checkout alignment observed:

```text
git rev-parse --show-toplevel → /Users/bibi/workspace/organizations/42ch/nexus
git branch --show-current → iteration/v1.44
git rev-parse --short HEAD → b023a357
```

The current branch HEAD is after the pinned P0 diff basis because PM/QC bookkeeping continued on `iteration/v1.44`; QA used the assignment's pinned `068135ed..43550686` range for P0 evidence and ignored P1/P2 behavior outside this plan.

## Verification (per Acceptance Criteria)

### AC1 — Spec §3.1 review mode and §3.2 extract mode behaviors implemented

**Status: Verified.**

Evidence and observations:

- Review mode is implemented as split embedded preset `novel-manuscript-audit-review`.
  - `preset.id = novel-manuscript-audit-review`.
  - State machine is explicit: `load_chapter → review_report → done`.
  - Review prompt is the 五問 baseline and states report output under `Works/{{work_ref}}/Logs/review/`.
  - Review preset does not contain `extract_sync` and does not require `kb.extract_work`.
- Extract mode is implemented as split embedded preset `novel-manuscript-audit-extract`.
  - `preset.id = novel-manuscript-audit-extract`.
  - State machine is explicit: `load_chapter → extract_sync → done`.
  - `extract_sync` invokes `kb.extract_work` with `creator_id`, `work_id`, `world_id`, `profile_hint: novel`, `source_kind: work_chapter`, and `source_locator` from `preset.input.body_path`.
  - Extract preset declares `world_binding.mode: required`.
- CLI dispatch selects the preset by typed enum:
  - `AuditMode::Review => "novel-manuscript-audit-review"`
  - `AuditMode::Extract => "novel-manuscript-audit-extract"`
- The legacy unified `novel-manuscript-audit` preset remains for backward compatibility, but the CLI no longer dispatches to it.

Primary test evidence:

```text
cargo test -p nexus-orchestration --test novel_manuscript_audit --test novel_manuscript_audit_review --test novel_manuscript_audit_extract

novel_manuscript_audit.rs: 14 passed; 0 failed
novel_manuscript_audit_extract.rs: 10 passed; 0 failed
novel_manuscript_audit_review.rs: 7 passed; 0 failed
```

### AC2 — No FL-E driver schedule created by audit command

**Status: Verified.**

Evidence and observations:

- The audit CLI path constructs a generic `AddScheduleRequest` for the review/extract preset only; it does not call `enqueue_auto_chain_step` or `enqueue_auto_chain_schedule`.
- The audit request sets `preset_id`, `seed`, `label`, `input`, `force_gates: false`, and optional schedule fields; it does not populate FL-E driver fields such as `fl_e_stage`, `driver_schedule_id`, `auto_chain_enabled`, or `auto_chain_interrupted`.
- Inspection found no `auto_chain`, `fl_e_stage`, `driver_schedule_id`, or `enqueue_auto_chain*` references in the split `novel-manuscript-audit-review` / `novel-manuscript-audit-extract` preset directories.
- Both split preset test modules include `*_no_fl_e_driver_fields` checks.

Relevant command observations:

```text
Search in run.rs for audit path: `AddScheduleRequest` at handle_audit_chapter; preset IDs are split review/extract.
Search in split preset directories for `auto_chain|fl_e_stage|driver_schedule_id|enqueue_auto_chain`: no matches.
```

### AC3 — At least 3 hermetic tests (review report, extract success, worldless 422)

**Status: Verified.**

Evidence and observations:

- Hermetic orchestration coverage exists and is green:
  - `novel_manuscript_audit_review`: 7 tests cover review preset load, capabilities, state transitions, no extract state, no FL-E fields, terminal state.
  - `novel_manuscript_audit_extract`: 10 tests cover extract preset load, `kb.extract_work`, state transitions, world/work args, no review state, no FL-E fields, terminal state.
  - `novel_manuscript_audit`: 14 legacy/compatibility tests include legacy preset load, extract capability shape, no FL-E fields, and worldless extract precondition logic.
- CLI unit tests are green:
  - `run::tests`: 20 passed, including `world_required_for_extract_error_display`, volume-aware body path resolution, path validation, and audit mode display tests.
- CLI integration subset is green:
  - `audit_chapter_help_shows_mode_and_chapter`
  - `audit_chapter_requires_mode_and_chapter`
  - `audit_chapter_requires_work_id`
- Worldless extract uses a typed CLI error:
  - `CliError::WorldRequiredForExtract { work_id }`
  - Display includes stable text `422 world_required_for_extract`.

Primary test evidence:

```text
cargo test -p nexus42 --lib -- run::tests

running 20 tests
...
test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 628 filtered out; finished in 0.00s
```

```text
cargo test -p nexus42 --test integration audit_chapter

running 3 tests
test audit_chapter_help_shows_mode_and_chapter ... ok
test audit_chapter_requires_mode_and_chapter ... ok
test audit_chapter_requires_work_id ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 47 filtered out; finished in 1.54s
```

### AC4 — DF-69 row archived on ship / tracker rollup registered

**Status: Verified for this P0 QA gate; final archive remains P-last scope.**

Evidence and observations:

- Assignment clarifies: “DF-69 row archived on ship (status.json touch is P-last scope)” and asks QA to verify DF-69 is registered in `metadata.tech_debt_summary.by_plan.manuscript-audit`.
- `.mstar/status.json` contains:

```json
"tech_debt_summary": {
  "by_plan": {
    "manuscript-audit": 1
  }
}
```

- The plan row is currently `InReview` with `review_sha_range: "068135ed..43550686"` and notes documenting implementation, fix wave, PM plan §6 fix, and targeted QC Approve. No P-last archive mutation was performed in this report-only QA task.

## Regression Behavior

Commands run fresh in this QA session:

```text
cargo test -p nexus-orchestration --test novel_manuscript_audit --test novel_manuscript_audit_review --test novel_manuscript_audit_extract
→ PASS (14 + 10 + 7 tests; 31 total, 0 failed)
```

```text
cargo test -p nexus42 --lib -- run::tests
→ PASS (20 passed; 0 failed; 628 filtered out)
```

```text
cargo test -p nexus42 --test integration audit_chapter
→ PASS (3 passed; 0 failed; 47 filtered out)
```

```text
cargo clippy --all -- -D warnings
→ PASS (`Finished dev profile`; no warnings emitted)
```

```text
cargo +nightly fmt --all --check
→ PASS (exit 0; no output)
```

No behavior regression was observed in the assigned P0 scope.

## Behavior Observation

- Review mode is prompt-driven: the preset explicitly routes to a 五問 `review_report` state and the prompt instructs writing the human-readable report under `Works/<work_ref>/Logs/review/`. QC noted that this is not an explicit report-persistence capability and left it as a deferred suggestion, not a blocking finding.
- Extract mode is capability-driven: `extract_sync` directly invokes `kb.extract_work`; there is no `kb_extract_jobs` queue ceremony in the preset or audit CLI path.
- Audit command creates a daemon schedule and reports asynchronous execution accurately. This resolves the earlier QC concern that the CLI messaging over-promised synchronous completion while preserving the plan’s no-FL-E-driver invariant.
- The pinned range command `git log --oneline 068135ed..43550686` on the shared iteration branch includes unrelated P1/P2/QC commits due parallel integration. The P0-relevant commit subset within that output is the expected 11-commit P0 chain: 5 original commits, original P0 merge, 3 fix commits, fix merge, and plan §6 fix.

P0-relevant commits verified within the pinned range:

```text
43550686 fix(v1.44 P0): R-V144P0-006 plan §6 verification update (test file paths corrected)
44a12a6e merge(v1.44 P0): fix wave — split preset + harden CLI for QC Request Changes
fc9f2f6d fix(v1.44): R-V144P0-006,009 plan verification fix + CLI integration tests
3297d925 fix(v1.44): R-V144P0-002..005,007,008 CLI handler hardening
d6b9400e fix(v1.44): R-V144P0-001 split preset into review + extract, fix CLI dispatch
9d471bdc merge(v1.44 P0): manuscript-audit preset + CLI entry
bce2e81a feat(v1.44): T7 — amend cli-spec.md §6.2D with audit-chapter IA
6428ba15 feat(v1.44): T6 — audit tests + structural fixes
97321916 feat(v1.44): T4+T5 — review mode report + extract mode sync wiring
863e2069 feat(v1.44): T3 — audit-chapter CLI handler + daemon schedule wiring
83905581 feat(v1.44): T1+T2 — novel-manuscript-audit embedded preset + prompts
```

## Full Required Evidence

### `git log --oneline 068135ed..43550686`

Captured in this QA session. The raw command returned the broader iteration history in that range, including P1/P2/QC commits; the P0-relevant 11 commits are listed above. The first and last P0 scope commits are `83905581` and `43550686`.

### Hermetic tests

```text
Finished `test` profile [unoptimized + debuginfo] target(s) in 0.17s
Running tests/novel_manuscript_audit.rs
running 14 tests
...
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

Running tests/novel_manuscript_audit_extract.rs
running 10 tests
...
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

Running tests/novel_manuscript_audit_review.rs
running 7 tests
...
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

### CLI unit tests

```text
cargo test -p nexus42 --lib -- run::tests
running 20 tests
...
test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 628 filtered out; finished in 0.00s
```

### CLI integration tests

```text
cargo test -p nexus42 --test integration audit_chapter
running 3 tests
...
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 47 filtered out; finished in 1.54s
```

### Workspace lint and format gates

```text
cargo clippy --all -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.37s
```

```text
cargo +nightly fmt --all --check
(exit 0; no output)
```

## Summary

| Acceptance Criterion | Result | Evidence |
| --- | --- | --- |
| AC1: Spec §3.1 review + §3.2 extract behaviors | Verified | Split presets, explicit state machines, 五問 review prompt, `kb.extract_work` extract state, CLI typed dispatch, 31 orchestration tests green |
| AC2: No FL-E driver schedule created | Verified | No audit-path `enqueue_auto_chain*`; no audit preset FL-E fields; no `auto_chain` in created audit request |
| AC3: Hermetic tests for review/extract/worldless 422 | Verified | 31 orchestration tests + 20 `run::tests` + 3 integration tests green; typed `CliError::WorldRequiredForExtract` present |
| AC4: DF-69 archived on ship / rollup registered | Verified for P0 QA gate | `status.json.metadata.tech_debt_summary.by_plan.manuscript-audit = 1`; final archive remains P-last scope |

## Verdict

**Approve.**

All four assigned P0 acceptance criteria were verified against the plan/spec scope after the fix wave and post-fix QC Approve. Required hermetic, unit, integration, clippy, and nightly-fmt checks were run fresh and passed. No P0 behavior regression was observed.
