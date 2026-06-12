---
report_kind: qc-review
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: 2026-06-12-v1.43-hygiene-and-residuals
verdict: Request Changes
generated_at: 2026-06-12T23:45:00+08:00
---

# Code Review Report — P-last (hygiene and residuals)

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-12T23:45:00+08:00

## Scope
- plan_id: 2026-06-12-v1.43-hygiene-and-residuals
- Review range / Diff basis: merge-base: a693752b + tip: 283d61e4
- Working branch (verified): feature/v1.43-hygiene-and-residuals
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.43-p-last
- Files reviewed: 11 (per plan Completion Report v2)
- Commit range: a693752b..283d61e4
- Tools run: git diff, git log, cargo +nightly fmt --all --check, cargo clippy -p nexus-orchestration -p nexus-local-db -- -D warnings, rg for TODO/unsafe/paths/secrets, manual audit of loader.rs (warn_unknown_top_level_keys + test) and work_chapters.rs (reconcile volume logic + test), status.json / archived residuals / specs / trackers cross-check.

## Findings
### 🔴 Critical
- None.

### 🟡 Warning
- **W-01 (correctness/hygiene gate)**: `cargo clippy -p nexus-orchestration -p nexus-local-db -- -D warnings` fails on code introduced in this plan. The final "lint cleanup" commit (283d61e4) added `#[allow(clippy::too_many_lines)]` on `reconcile_from_filesystem` but left a new `too_long_first_doc_paragraph` lint on the doc comment for the newly added `warn_unknown_top_level_keys` function (loader.rs:1056). The plan explicitly claims "lint cleanup" as part of the P-last hygiene scope; the tree does not pass the workspace CI lint gate for the changed crates. Source: `git diff` + clippy run on review HEAD.
- **W-02 (test coverage gap for side-effect behavior)**: The new `warn_unknown_top_level_keys_detects_misplaced_gates` test (loader.rs:2715) verifies that load succeeds and that the helper can surface the stray key via manual Value walk, but it does **not** assert that `tracing::warn!` was actually emitted. The security/correctness intent of R-V137P0-01 is to surface (via warn) the original bug class (misplaced `gates:` block). The logging side-effect is the observable signal for operators; it is untested. The function itself is pure on the Value and safe, but the "warn" contract lacks a test double or capture assertion.
- **W-03 (spec promotion timing/process)**: The "Shipped (V1.42)" stamps for `preset-conditional-routing.md` and `novel-workflow-profile.md` (V1.42 P2 items) were applied in the V1.43 P-last hygiene commit (2c13c2c6) rather than at the close of the V1.42 P2 plan. The underlying code was shipped earlier (git log confirms the conditional routing and multi-volume work landed in prior V1.42 branches). This is factually accurate but is a process deviation from the normal "stamp at ship time" discipline. Low process risk; recorded for PM visibility.

### 🟢 Suggestion
- **S-01**: Consider adding a small tracing-test or mock subscriber assertion (or at minimum a `tracing::debug!` + test that exercises the warn path) so that future changes to the unknown-key policy are covered by observable behavior, not just the helper's return value.
- **S-02**: The `KNOWN_TOP_LEVEL_KEYS` list is the single source of truth for the "what is a top-level preset section" contract. If the PresetManifest struct grows new top-level sections in the future, this array must be kept in sync (no derive or macro currently enforces it). A compile-time or test-time reflection check would be a low-cost hardening.
- **S-03 (minor)**: The reconcile test fixture for volume 2 uses `chapter: 1` in the same `my-novel` work. This is valid per the (work_id, volume, chapter) PK, but consider adding a comment that the same chapter number can legitimately exist in different volumes.

## Source Trace
- Finding ID: W-01
- Source Type: static-analysis
- Source Reference: `cargo clippy -p nexus-orchestration ...` on 283d61e4; diff shows the doc comment at crates/nexus-orchestration/src/preset/loader.rs:1056
- Confidence: High

- Finding ID: W-02
- Source Type: test-review
- Source Reference: crates/nexus-orchestration/src/preset/loader.rs:2713 (test) vs. 1064 (the warn! call)
- Confidence: High

- Finding ID: R-V137P0-01 / R-V142P1-F-003 code audit
- Source Type: git-diff + manual code review
- Source Reference: crates/nexus-orchestration/src/preset/loader.rs (new fn + test), crates/nexus-local-db/src/work_chapters.rs (reconcile + test_reconcile_volume_aware_from_frontmatter)
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

**Gate rationale**: The plan's own "lint cleanup" commit left the changed crates failing `cargo clippy ... -D warnings`. Per mstar-review-qc gate rule, any unresolved Warning blocks Approve. The two code changes under review (strict loader warn, volume-aware reconcile) are otherwise correct, safe, and well-tested for the stated residual fixes. The other hygiene items (residual closures, spec stamps, iteration closeout, system invariants) are accurate with only the minor process note on stamp timing (W-03). Fix the clippy failure (and consider strengthening the warn-side-effect test) and this becomes Approve on re-review.
