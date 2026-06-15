---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-14-v1.46-spec-cli-hygiene"
verdict: "Approve"
generated_at: "2026-06-15"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-15

## Scope
- plan_id: `2026-06-14-v1.46-spec-cli-hygiene`
- Review range / Diff basis: `merge-base: 1f92016f (P0 Done commit, base of P1 work) → tip: acabca53 (P1 atomic merge) (7 commits + 1 --no-ff merge = 8 total)` — equivalent `git diff 1f92016f..acabca53` or `git show --stat 1f92016f..acabca53`
- Working branch (verified): `iteration/v1.46`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 22 (spec sweep 12 + runtime remediation 6 + ARCHITECTURE.md + shipped-features-tracker.md + quickstart deletion + fmt style)
- Commit range: `1f92016f..acabca53` (T1 `1069a671`, T2 `ac49de8e`, T3 `499a713d`, T4 `9d8482a1`, T5 `dd3eb4d7`, T6 `8f2e630d`, merge `acabca53`)
- Tools run: 4 mechanical ACs (all PASS), `cargo clippy --all -- -D warnings` (clean), `cargo test --all` (green), `cargo +nightly fmt --all --check` (clean), `git diff 1f92016f..acabca53 --stat`, `git show --stat 1f92016f..acabca53`, full read of runtime remediation files (schedules.rs, preset_gates.rs, works/mod.rs, creator/mod.rs, run.rs, errors.rs) + key spec sections + 7 commits

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion

#### S-1: Hard-coded message snapshot in completion-guard test (`schedules.rs:1583–1591`)
**Triggering condition**: The test `completion_guard_message_cites_spec_paths` (added/updated in T4) asserts exact string containment for the two new spec paths inside a multi-line literal that duplicates the production error string in `handle_add_schedule` (lines ~183–186). This is a string-snapshot test.

**Impact**: Future edits to the user-facing completion-guard message will require a matching test update. Low risk (the test is small, co-located, and the string is not generated), but it is a maintainability surface. No security or correctness regression — the test correctly validates that the remediation now cites the normative spec paths instead of the deleted quickstart.

**Suggested fix**: None required for this plan. If a future hygiene pass extracts the canonical remediation strings into a single constant (or a small module), the test can become a simple identity check. Not blocking.

**Source Reference**: `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs:1583–1591` (test) vs production lines 181–187 (T4 commit `9d8482a1`).

**Confidence**: High

#### S-2: Four remediation_* tests in `preset_gates.rs` are string-contains snapshots (lines 954–1071)
**Triggering condition**: The four new/renamed tests (`remediation_work_field_cites_quickstart` → now spec, plus three new `remediation_*_cites_*_spec`) use `contains("creator-run-preset-entry.md")` / `contains("novel-author-experience.md")` on the `remediation` field of `FailedGate`. These are the T4 updates that replaced the old quickstart citations.

**Impact**: Same as S-1 — if any of the three remediation helper functions (`work_field_remediation`, `filesystem_remediation`, `previous_preset_remediation`) are edited, the corresponding test must be kept in sync. The tests are now correct (they validate the P1 hygiene goal), but they are snapshot-style rather than structural.

**Suggested fix**: None required for this plan. The pattern is acceptable for a small set of user-facing strings whose only job is to point at SSOT docs. A future refactor could make the remediation text a pure data table and assert against the table, but that is out of scope.

**Source Reference**: `crates/nexus-orchestration/src/preset_gates.rs:954–1071` (the four tests) in T4 commit `9d8482a1`; production helpers at 351–400.

**Confidence**: High

## Source Trace
- **Finding ID: S-1**
  - Source Type: manual code review + test-vs-production cross-check (security/correctness lens)
  - Source Reference: `git show 9d8482a1 -- crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs` (completion guard + test); runtime read of lines 179–189 (production) and 1579–1594 (test)
  - Confidence: High

- **Finding ID: S-2**
  - Source Type: manual code review + test-vs-remediation-function cross-check
  - Source Reference: `git show 9d8482a1 -- crates/nexus-orchestration/src/preset_gates.rs`; runtime read of remediation fns (351–400) + test block (951–1072)
  - Confidence: High

- Mechanical AC verification (plan §4 + §6):
  - `test ! -f docs/novel-writing-quickstart.md` → PASS (exit 0)
  - `rg -n 'creator run start|creator run stage|stage advance' .mstar/knowledge/specs/ --glob '*.md' | rg -v 'Removed in V1\.45|Superseded by|changelog'` → zero hits (PASS)
  - `rg 'novel-writing-quickstart' crates/ docs/` → zero hits (PASS)
  - `docs/ARCHITECTURE.md` links only to `.mstar/knowledge/specs/novel-author-experience.md §3` and `creator-run-preset-entry.md` (PASS)

- CI gates (all green, no scope-attributable failures):
  - `cargo clippy --all -- -D warnings` → clean
  - `cargo test --all` → green (full suite)
  - `cargo +nightly fmt --all --check` → clean (EXIT 0)

- Spec sweep (T3) + W-1/W-2 reconcile: `novel-author-experience.md` §4.1 table now correctly documents the three-state `findings` contract and creator-global scope of `findings_stale` (matches P0 runtime behavior that was previously mismatched). All 12 touched specs satisfy the Grill #11 normative zero-hit AC for stale CLI patterns.

- Shipped Master hygiene (T3): `creator-run-preset-entry.md` lines 63/110 received only minimal explanatory-keyword additions ("Removed in V1.45; see changelog"). Body content and normative meaning unchanged — correct non-substantive tag per assignment.

- BL-10 (T5): Supersede note added to `.mstar/archived/shipped-features-tracker.md` "Cancelled / Superseded" section for `docs/novel-writing-quickstart.md`. No new open deferred row created (Grill #15).

- Atomic delivery: all 6 tasks (T1–T6) + W-1/W-2 reconciliation + BL-10 in a single merge (`acabca53`). Review range limited to `1f92016f..acabca53` only.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

All four mechanical ACs pass. CI gates (clippy, test, nightly fmt) are clean. The core P1 goal — retire the quickstart file, delete the stale `cli-spec` §6.2E block, sweep 12 satellite specs to normative zero-hit on deleted CLI patterns, and remediate ~26 runtime user-facing strings + tests to cite the current SSOT specs (`creator-run-preset-entry.md` and `novel-author-experience.md`) instead of the deleted quickstart — is executed correctly and atomically.

From the security/correctness perspective:
- No new injection, path-traversal, or control-flow surfaces were introduced (remediation strings are compile-time literals; no user data is interpolated into the cited paths).
- The completion-guard SQL path (COUNT on completed novels) is unchanged; only the error string was updated.
- `force_gates` / omitted-`work_id` / work-not-found error paths continue to return structured `PresetGatesFailed` with remediation citing the spec.
- W-1/W-2 from P0 (spec/runtime contract mismatch on `findings` three-state and `findings_stale` scope) are reconciled in the spec sweep; the table in `novel-author-experience.md` §4.1 now accurately describes runtime behavior.
- The four remediation tests and the completion-guard test are now semantically valid (they assert the new correct citations).

The two Suggestions are low-impact maintainability notes about string-snapshot tests. They do not affect security, correctness, or the ability of users/automation to follow the remediation instructions. Per `mstar-review-qc` gate rule (Critical = 0 and Warning = 0 ⇒ Approve), this seat returns **Approve**.

## Revalidation
N/A — initial wave for this plan. No prior findings to revalidate; P0 W-1/W-2 were explicitly deferred into this plan's scope and are now closed by the spec amendments.

## Evidence (verification-before-completion)
- 4 mechanical AC commands executed and PASS (see Source Trace).
- `cargo clippy --all -- -D warnings` → clean.
- `cargo test --all` → green.
- `cargo +nightly fmt --all --check` → clean.
- Full diff range read + targeted file reads of all 6 runtime remediation files + key spec sections + 7 commits.
- Report file will be committed (only this path) before Completion Report v2.
