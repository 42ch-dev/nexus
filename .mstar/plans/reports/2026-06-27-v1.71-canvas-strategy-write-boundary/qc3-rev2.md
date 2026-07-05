---
report_kind: qc_re_review
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-27-v1.71-canvas-strategy-write-boundary"
verdict: "Request Changes"
generated_at: "2026-06-28T00:43:54Z"
---

# Code Review Report — Targeted Re-review

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: volcengine-plan/ark-code-latest
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-28T00:43:54Z

## Scope
- plan_id: 2026-06-27-v1.71-canvas-strategy-write-boundary
- Review range / Diff basis: `git log --oneline 1afdd592..5ed2ee6c` and targeted inspection of the P0 fix-wave merge `5ed2ee6c`
- Working branch (verified): iteration/v1.71
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: fix-wave diff paths plus targeted runtime/web/build/status files for `R-V171P0-QC3-C1`, `C2`, `C3`, `W3`, and `S3`
- Commit range: 1afdd592..5ed2ee6c (with current HEAD e462bc08 including prior qc1/qc2/qc3 follow-up commits)
- Tools run:
  - `git rev-parse --show-toplevel && git branch --show-current && git rev-parse HEAD && git log --oneline -5 && git status --short`
  - `git log --oneline 1afdd592..5ed2ee6c && git diff --name-status 1afdd592..5ed2ee6c`
  - `cargo test -p nexus-daemon-runtime --test strategy_patch` — passed, 5/5 tests
  - `cargo +nightly-2026-06-26 fmt --all --check` — passed
  - `cargo clippy --workspace -- -D warnings` — passed
  - `cargo test --workspace` — passed; warnings only in test targets
  - `pnpm --filter @42ch/nexus-contracts run build` — passed
  - `pnpm --filter web typecheck` — passed
  - `pnpm --filter web test` — passed, 18 files / 147 tests
  - `pnpm --filter web build` — passed; prebuild ran contracts build first; Vite chunk-size warning remains on bootstrap

## Original Finding Revalidation

| Finding | Status | Evidence |
|---|---|---|
| `R-V171P0-QC3-C1` — revision precondition not atomic / concurrent writers | **Resolved** | All three patch paths now run in `spawn_blocking`, acquire `.strategy-lock` with exclusive `flock`, then load/re-check `base_revision` while holding the guard before validation and persistence (`crates/nexus-daemon-runtime/src/api/handlers/strategy.rs:544`, `563`, `567`, `570`; `891`, `910`, `912`, `915`). Regression coverage: `concurrent_patch_state_serializes_and_one_writer_gets_conflict` passed in both the targeted integration test and workspace test. |
| `R-V171P0-QC3-C2` — prompt-template non-atomic relative to revision persistence | **Partially resolved** | Validation-failure rollback is fixed: the template is staged, renamed, validated, and restored on validation errors (`strategy.rs:977-1011`), and `patch_prompt_template_rolls_back_on_validation_failure` passed. However, the template rename still happens before `write_preset_yaml`, and there is no rollback if `write_preset_yaml` fails after validation (`strategy.rs:997-1015`). A YAML temp write/fsync/rename failure can still leave prompt content changed while `revision:` remains unchanged. |
| `R-V171P0-QC3-C3` — multi-field Save uses stale `base_revision` after its own first patch | **Resolved** | `StrategyCanvas` now tracks `workingRevision`, updates it from each `StrategyPatchResponse.new_revision`, and passes the updated value to subsequent state/transition/prompt mutations in the same Save (`apps/web/src/components/canvas/strategy-canvas.tsx:114-121`, `170-205`, `208`). The broader UI-level non-transactional multi-patch save risk remains separately tracked as `R-V171P0-QC1-004`, but the stale-base self-conflict from qc3 C3 is fixed. |
| `R-V171P0-QC3-W3` — web build depends on unstated contracts dist prebuild | **Resolved** | `apps/web/package.json:11` adds `prebuild: pnpm --filter @42ch/nexus-contracts run build`; `pnpm --filter web build` now executes contracts build first and succeeds in a clean command sequence. |
| `R-V171P0-QC3-S3` — track Strategy route chunk size baseline | **Recorded** | Current build records `strategy-page-B-YSqRaf.js` at 320.26 kB minified / 101.07 kB gzip and `strategy-page-C5ap-Sga.css` at 15.87 kB / 2.67 kB gzip. Bootstrap `index-JJoVHwH5.js` remains 955.61 kB / 304.61 kB gzip and triggers the existing Vite >500 kB warning. |

## Findings

### 🔴 Critical

- **C2 remains partially open — prompt-template commits are still not rolled back if YAML revision persistence fails after validation** -> Extend the prompt-template write path so every post-template failure rolls back the template bytes, not only validation failures. The fix wave correctly added validation rollback, but `patch_prompt_template_inner` still commits the template rename before `write_preset_yaml`; if the subsequent YAML temp write, fsync, or rename returns an error, the prompt file is already changed and `preset.yaml` still has the old `revision:`. That preserves the original split-brain failure mode for persistence failures.
  - Evidence: `crates/nexus-daemon-runtime/src/api/handlers/strategy.rs:997-1015` renames the template, validates, then writes `preset.yaml`; rollback is only in the `!errors.is_empty()` branch at `1007-1011`.
  - Required fix: either roll back `canonical_template` on any `write_preset_yaml` error, or redesign the operation so the revision commit and template commit have a recoverable commit/rollback order. Add a failure-injection test around YAML write/rename failure to prove prompt content and `revision:` cannot diverge.

### 🟡 Warning

- None new from this fix wave. The advisory flock serializes per-strategy writes and runs inside `spawn_blocking`, so it avoids async-runtime blocking; the lock scope is coarse but acceptable for local single-author Strategy patch traffic.

### 🟢 Suggestion

- Keep monitoring Strategy route size. The Strategy route chunk increased from the V1.70 baseline (~305.40 kB / 97.28 kB gzip) to 320.26 kB / 101.07 kB gzip. Route splitting still holds; no immediate blocker, but manual chunking should be reconsidered if the Strategy surface grows again.

## Source Trace
- Finding C2: `crates/nexus-daemon-runtime/src/api/handlers/strategy.rs:997-1015`; `patch_prompt_template_rolls_back_on_validation_failure` only covers validation rollback, not YAML persistence failure after template commit.
- Finding C1 closure: `crates/nexus-daemon-runtime/src/api/handlers/strategy.rs:63-92`, `563-570`, `792-797`, `910-915`; `crates/nexus-daemon-runtime/tests/strategy_patch.rs:222-279`.
- Finding C3 closure: `apps/web/src/components/canvas/strategy-canvas.tsx:114-121`, `170-205`, `208`.
- Finding W3 closure: `apps/web/package.json:11`; `pnpm --filter web build` output shows contracts prebuild followed by successful Vite build.
- S3 baseline: `pnpm --filter web build` output: `strategy-page-B-YSqRaf.js` 320.26 kB / gzip 101.07 kB; `strategy-page-C5ap-Sga.css` 15.87 kB / gzip 2.67 kB.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes

**Rationale**: The fix wave resolves the concurrent same-base writer race, the client stale-base self-conflict, and the clean-workspace web build ordering issue. It also records the Strategy route chunk baseline. One reliability blocker remains: prompt-template writes can still diverge from `revision:` if YAML revision persistence fails after the template rename succeeds.

## Residual Findings
- `R-V171P0-QC3-C2` — remains open as `critical`: prompt-template body write is still non-atomic relative to Strategy revision persistence for post-validation YAML persistence failures.
