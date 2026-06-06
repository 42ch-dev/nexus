---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-06-v1.35-critical-residual-convergence"
verdict: "Approve"
generated_at: "2026-06-07T00:58:45+08:00"
revalidation: "targeted — C-QC1-001 (UTF-8 fix) + W-QC1-001 (lifecycle normalization)"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: openai/gpt-5.5
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-07T00:50:53+08:00

## Scope
- plan_id: 2026-06-06-v1.35-critical-residual-convergence
- Review range / Diff basis: merge-base: 30efd06 (iteration/v1.35 HEAD before P0) + tip: 2d9746a (current HEAD). Equivalent: git diff 30efd06..2d9746a.
- Working branch (verified): feature/v1.35-critical-residual-convergence
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.35-p0
- Files reviewed: 16 changed files in `git diff 30efd06..HEAD --stat`; detailed manual review focused on the 8 assignment scopes.
- Commit range: 30efd06..2d9746a
- Tools run: `git rev-parse --show-toplevel`; `git branch --show-current`; `git log -1 --oneline`; `git diff 30efd06..HEAD --stat`; scoped `git diff`; `grep` for dev artifacts; Python residual rollup check; `cargo test -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db`; `cargo clippy -p nexus-orchestration -p nexus-daemon-runtime -- -D warnings`; `cargo +nightly fmt --all -- --check`.

## Findings
### 🔴 Critical
- **C-QC1-001 — TD-V131-04 truncates `&str` by raw byte index and can panic on valid UTF-8 input.** The 256 KiB cap is architecturally sound: a defensive prompt-size limit with an explicit truncation marker fits the capability boundary and keeps the LLM IPC contract bounded. However, the implementation uses `&content[..DEFAULT_MAX_CONTENT_BYTES]`, which violates Rust string boundary invariants whenever byte 262144 falls inside a multi-byte scalar. This makes a defensive guard a panic surface in production `context.summarize`, and the current tests are ASCII-only, so they do not protect the intended invariant. Fix by truncating at a valid char boundary (for example, walk backward to `is_char_boundary`, or use `char_indices`/`unicode-segmentation` if grapheme-level behavior is desired) and add a multi-byte boundary regression test.

### 🟡 Warning
- **W-QC1-001 — Closed residual archive shape and open-list cleanup are not fully aligned with the harness lifecycle contract.** The rollup numbers are correct (`28` total open, `0` critical), and the closed IDs are present in archived files. But `.mstar/status.json` still retains an empty `residual_findings["2026-06-04-v1.34-cursor-pr42-stage-status"]` array, while `mstar-plan-artifacts` says empty plan keys should be deleted. The new archived entries also use top-level `archived_at` and per-entry `closed_reason`, but omit per-entry `archived_at`, `lifecycle`, and `closure_note` fields recommended by the lifecycle schema. This is not a product-code blocker, but it weakens cross-session residual inheritance and should be normalized before PM/QA closeout.

### 🟢 Suggestion
- **S-QC1-001 — Consider extracting status-only stage transition policy into the stage-gates module in a later FL-E cleanup.** `check_stage_status_transition()` is well named and surgical for R-CURSOR-PR42-03, and keeping it local avoids churn for this P0 fix. If additional status-only transition semantics grow, moving the policy next to `nexus_orchestration::stage_gates::check_stage_advance()` would keep FL-E state invariants centralized.

## Source Trace
- Finding ID: C-QC1-001
- Source Type: git-diff + manual-reasoning
- Source Reference: `crates/nexus-orchestration/src/capability/builtins/context_summarize.rs:169-181` in `git diff 30efd06..HEAD`; specifically `&content[..DEFAULT_MAX_CONTENT_BYTES]`.
- Confidence: High

- Finding ID: W-QC1-001
- Source Type: doc-rule + manual-reasoning
- Source Reference: `.mstar/status.json:273-597`, `.mstar/archived/residuals/{v1.30-post-qc-tech-debt.json,v1.31-post-qc-tech-debt.json,2026-06-04-v1.34-cursor-pr42-stage-status.json}`; `mstar-plan-artifacts/references/status-and-residuals.md` empty-key and archive lifecycle rules.
- Confidence: High

- Finding ID: S-QC1-001
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:336-342` and `:499-536`; existing `nexus_orchestration::stage_gates::check_stage_advance()` call at `works.rs:320-331`.
- Confidence: Medium

## Evidence
### Pre-review alignment
```text
$ git rev-parse --show-toplevel
/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.35-p0

$ git branch --show-current
feature/v1.35-critical-residual-convergence

$ git log -1 --oneline
2d9746a qc(v1.35-p0): qc3 performance+reliability review — Request Changes (1 Critical: UTF-8 truncation panic)

$ git diff 30efd06..HEAD --stat
16 files changed, 898 insertions(+), 179 deletions(-)
```

### Residual rollup / SSOT spot check
```text
$ python3 - <<'PY'
... compute residual_findings counts and severity totals ...
PY
residual keys/counts: {'2026-06-04-v1.33-llm-judge-runtime-fix': 2, '2026-06-04-v1.33-memory-review-closed-loop': 3, '2026-06-04-v1.33-work-model-and-creator-run': 3, 'v1.31-post-qc-tech-debt': 5, 'v1.30-post-qc-tech-debt': 9, '2026-06-04-v1.34-fl-e-run-intents-and-stages': 5, '2026-06-04-v1.34-agent-tool-implementation': 1, '2026-06-04-v1.34-cursor-pr42-stage-status': 0}
computed total: 28
computed by_severity: {'critical': 0, 'high': 0, 'medium': 8, 'low': 15, 'nit': 5}
stored summary: 28 {'critical': 0, 'high': 0, 'medium': 8, 'low': 15, 'nit': 5}
empty keys: ['2026-06-04-v1.34-cursor-pr42-stage-status']
```

### Dev artifact scan
```text
$ grep-equivalent scan for \b(dbg!|TODO|FIXME|unimplemented!\() under crates/*.rs
Found 4 pre-existing TODO comments outside the touched production paths; no `dbg!`, `FIXME`, or `unimplemented!()` introduced in the reviewed P0 production changes.
```

### Required verification commands
```text
$ cargo test -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db
Result: PASS. Key result lines included:
- nexus-local-db: test result: ok. 129 passed; 0 failed; finished in 1.64s
- nexus-orchestration: test result: ok. 429 passed; 0 failed; 1 ignored; finished in 0.68s
- integration/doc tests shown in command output all passed.

$ cargo clippy -p nexus-orchestration -p nexus-daemon-runtime -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.23s

$ cargo +nightly fmt --all -- --check
Result: PASS (no output).
```

## Architecture Checklist Notes
- Naming clarity: acceptable for the surgical fixes. `check_stage_status_transition()` clearly names the new status-only policy.
- Single responsibility: code changes are localized to the residual targets; no broad refactor detected.
- Error handling: `cleanup_row` now logs cleanup failure; stage-status invalid transitions return an explicit `BadRequest`; queue claims use existing `Result` paths.
- Comments explain intent: residual IDs are present near the changed logic.
- No leftover dev artifacts: no new reviewed-path `dbg!`, `TODO`, `FIXME`, or `unimplemented!()` found.
- Public API impact: no Rust public signature changes observed in the reviewed scope; API behavior intentionally tightens status-only terminal transitions unless `force=true`.
- Consistency with existing patterns: `mark_running` mirrors the `claim_job` queued guard; prompt-injection batching uses bound params; `cleanup_row` observability matches surrounding tracing use.
- Test placement: new handler and capability tests are colocated with existing module/integration tests and reuse existing hermetic setup.
- Residual lifecycle SSOT: rollup counts are correct, but empty open-list key and archive entry shape should be normalized (W-QC1-001).
- `metadata.tech_debt_summary`: derived totals match the open residual list (`28` total, `0` critical).
- TD-V131-04 design vs implementation: design is separable and directionally correct, but the current implementation panic is unresolved and merge-blocking.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes

## Revalidation

### What was re-reviewed
- Targeted re-review type: `targeted — reviewers: qc-specialist, qc-specialist-3`.
- Prior QC1 findings rechecked: `C-QC1-001` (TD-V131-04 multi-byte UTF-8 truncation panic) and `W-QC1-001` (residual lifecycle normalization).
- Review range / Diff basis: `merge-base: 30efd06` + `tip: 8bc7071`; equivalent to `git diff 30efd06..8bc7071`.
- Working branch (verified): `feature/v1.35-critical-residual-convergence`.
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.35-p0`.
- Revalidation timestamp: `2026-06-07T00:58:45+08:00`.

### Prior finding dispositions

- **C-QC1-001 — RESOLVED.** `truncate_to_char_boundary()` now exists in `crates/nexus-orchestration/src/capability/builtins/context_summarize.rs` and walks backward with `is_char_boundary` before slicing. `build_summary_prompt()` now calls this helper instead of using `&content[..DEFAULT_MAX_CONTENT_BYTES]` directly. Regression coverage was added for multi-byte UTF-8 truncation, including `build_summary_prompt_truncates_multibyte_utf8_without_panic()` and `build_summary_prompt_truncates_mid_cjk_char()`.
  - Evidence: required command `cargo test -p nexus-orchestration --lib context_summarize 2>&1 | tail -25` passed with `18 passed; 0 failed; 0 ignored; 415 filtered out`.

- **W-QC1-001 — RESOLVED.** Root `.mstar/status.json` no longer retains the empty `residual_findings["2026-06-04-v1.34-cursor-pr42-stage-status"]` key, and the empty residual-key count is `0`. `.metadata.tech_debt_summary.by_plan` no longer contains `cursor-pr42` or any empty plan key. Each entry in the five new archived residual files now carries `archived_at`, `lifecycle`, and `closure_note`.
  - Evidence: `jq '.residual_findings | keys' .mstar/status.json` lists seven non-empty open residual plans and excludes `2026-06-04-v1.34-cursor-pr42-stage-status`; `jq '.residual_findings | to_entries | map(select((.value|length)==0)) | length' .mstar/status.json` returned `0`; `jq '.metadata.tech_debt_summary.by_plan | keys' .mstar/status.json` excludes the empty key; per-file archive checks returned `missing_required_fields=0` for all five target archive files.

### Revalidation summary
| Prior Finding | Status |
|---------------|--------|
| C-QC1-001 | RESOLVED |
| W-QC1-001 | RESOLVED |

**New findings introduced by the fix:** 0

**Revalidation Verdict**: Approve
