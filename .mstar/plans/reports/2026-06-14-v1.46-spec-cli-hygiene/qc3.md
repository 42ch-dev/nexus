---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-14-v1.46-spec-cli-hygiene"
verdict: "Approve"
generated_at: "2026-06-15"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: kimi-for-coding/k2p7
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-15T00:00:00+08:00

## Scope
- plan_id: `2026-06-14-v1.46-spec-cli-hygiene`
- Review range / Diff basis: `merge-base: 1f92016f (P0 Done commit, base of P1 work) → tip: acabca53 (P1 atomic merge) (7 commits + 1 --no-ff merge = 8 total)` — equivalent `git diff 1f92016f..acabca53` or `git show --stat 1f92016f..acabca53`
- Working branch (verified): `iteration/v1.46`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 22
- Commit range: `1f92016f..acabca53`
- Tools run:
  - `git diff 1f92016f..acabca53 --stat`
  - `git show --stat 1f92016f..acabca53`
  - `test ! -f docs/novel-writing-quickstart.md`
  - `rg -n 'creator run start|creator run stage|stage advance' .mstar/knowledge/specs/ --glob '*.md' | rg -v 'Removed in V1\.45|Superseded by|changelog'`
  - `rg 'novel-writing-quickstart' crates/ docs/`
  - `rg -n 'novel-writing-quickstart|quickstart' docs/ARCHITECTURE.md`
  - `cargo test --all`
  - `cargo clippy --all -- -D warnings`
  - `cargo +nightly fmt --all --check`

## Findings

### 🔴 Critical

None.

### 🟡 Warning

None.

### 🟢 Suggestion

#### S-1: `intake_status` gate remediation command is semantically misleading for an existing Work

**Triggering condition**: In `crates/nexus-orchestration/src/preset_gates.rs`, the `work_field_remediation("intake_status")` string now reads:

```text
Complete intake via `creator bootstrap --preset creative-brief-intake`.
See .mstar/knowledge/specs/novel-writing/author-experience.md §3
```

The same phrasing appears in the spec example at `.mstar/knowledge/specs/novel-writing/workflow-profile.md` §5.3.5.

**Impact**: `creator bootstrap` creates a **new** Work; `--preset` overrides the *production* preset, not the intake preset (which is always `creative-brief-intake`). For an existing Work whose `intake_status` is `pending`, running this command will not complete intake on that Work — it will spin up a second Work with `creative-brief-intake` as its production preset. The remediation therefore fails its stated purpose of telling the user how to recover from the gate failure in-place.

This is not a new hot-path I/O regression, but it is a reliability gap in error-recovery copy introduced by the P1 string migration.

**Suggested fix**: Either
- change the remediation to `creator run creative-brief-intake <work_id>` if the generic runner can satisfy the intake preset's input contract, or
- rephrase as a new-Work fallback (e.g., "Intake is incomplete; start a new Work with `creator bootstrap --idea ...`") and update the spec example accordingly.

#### S-2: Remediation tests assert only spec filenames, not command validity

**Triggering condition**: The updated tests in `crates/nexus-orchestration/src/preset_gates.rs`, `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs`, `crates/nexus42/src/errors.rs`, and `crates/nexus42/src/commands/creator/run.rs` check that remediation strings contain the new spec filenames (e.g., `creator-run-preset-entry.md`), but do not assert the concrete command being recommended.

**Impact**: A future edit could replace a valid command with an invalid one while still keeping the spec-filename assertion green (S-1 is an example of what such a test would miss).

**Suggested fix**: Add substring assertions for the actual recommended command in each remediation test, e.g.:

```rust
assert!(remediation.contains("creator bootstrap --init-preset novel-project-init"));
```

This locks both the destination spec *and* the actionable next step.

#### S-3: Stale test name still references deleted quickstart

**Triggering condition**: `crates/nexus42/src/errors.rs` line ~605 contains a test named `daemon_not_reachable_quickstart_cites_section_1`. The test body was updated to assert `creator-run-preset-entry.md`, but the function name still refers to the deleted quickstart.

**Impact**: Minor maintainability friction — the name no longer describes what the test verifies.

**Suggested fix**: Rename to `daemon_not_reachable_cites_preset_entry_spec`.

#### S-4: Runtime remediation strings cite repo-internal spec paths

**Triggering condition**: All updated user-facing remediation strings point at `.mstar/knowledge/specs/<file>.md`. These paths exist in the open-source repo but are not shipped with a compiled CLI binary.

**Impact**: Low — pre-release users are expected to consult the repo, but a shipped binary consumer would not have these files locally, making the remediation less actionable.

**Suggested fix**: Track this as a future UX follow-up (P-last or V1.47) to either embed stable public URLs or include spec excerpts in `--help`/error output.

## Source Trace

- **Finding ID: S-1**
  - Source Type: manual-reasoning + spec-runtime cross-check
  - Source Reference: `crates/nexus-orchestration/src/preset_gates.rs` lines 355–360; `.mstar/knowledge/specs/novel-writing/workflow-profile.md` lines 531–536; `cargo run -p nexus42 -- creator bootstrap --help`
  - Confidence: Medium

- **Finding ID: S-2**
  - Source Type: manual-reasoning + test-coverage review
  - Source Reference: `crates/nexus-orchestration/src/preset_gates.rs` tests (~975–1065); `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs` test `completion_guard_message_cites_spec_paths`; `crates/nexus42/src/errors.rs` test `daemon_not_reachable_quickstart_cites_section_1`; `crates/nexus42/src/commands/creator/run.rs` test `reject_produce_when_novel_complete_cites_quickstart_section_6`
  - Confidence: High

- **Finding ID: S-3**
  - Source Type: manual-reasoning (naming drift)
  - Source Reference: `crates/nexus42/src/errors.rs` line ~605
  - Confidence: High

- **Finding ID: S-4**
  - Source Type: manual-reasoning (deployment context)
  - Source Reference: all T4 runtime remediation string changes in `crates/nexus42/src/commands/creator/{mod.rs,run.rs,works/mod.rs}`, `crates/nexus42/src/errors.rs`, `crates/nexus-orchestration/src/preset_gates.rs`, `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs`
  - Confidence: Low

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 4 |

**Verdict**: Approve

The P1 change set is a clean, atomic spec-hygiene sweep:

- All four mechanical acceptance criteria pass (`quickstart` absent, stale CLI patterns zero-hit in normative spec bodies, runtime zero-hits, `ARCHITECTURE.md` links to specs only).
- BL-10 is correctly retired with an archive supersede note and no new open deferred row.
- The runtime remediation (6 files, ~26 string sites) is string-only: no new I/O, no new allocations in hot loops, no new error branches, and no changes to JSON-path timing or concurrency.
- Test suite is healthy: `cargo test --all` passes with 679 lib tests + integration/doc tests, 0 failures, no newly introduced skipped/ignored tests.
- CI gates are green: `cargo clippy --all -- -D warnings` clean, `cargo +nightly fmt --all --check` clean.

The four Suggestions are non-blocking reliability/UX polish items. The only one with material user impact is S-1 (intake remediation command semantics), but it does not represent a new hot-path regression, data-loss risk, or CI failure, so it is recorded as a Suggestion for PM disposition rather than a blocking Warning.
