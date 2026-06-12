---
report_kind: qc-review
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-12-v1.43-hygiene-and-residuals
verdict: Request Changes
generated_at: 2026-06-12T21:20:00+08:00
---

# Code Review Report — P-last (hygiene and residuals)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: k2p6
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-12T21:20:00+08:00

## Scope
- plan_id: 2026-06-12-v1.43-hygiene-and-residuals
- Review range / Diff basis: merge-base: a693752b + tip: 283d61e4
- Working branch (verified): feature/v1.43-hygiene-and-residuals
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.43-p-last
- Files reviewed: 11
- Commit range: a693752b..283d61e4
- Tools run: cargo +nightly fmt --all --check, cargo clippy -p nexus-orchestration -p nexus-local-db -- -D warnings, cargo test -p nexus-orchestration --lib, cargo test -p nexus-local-db --lib, cargo test --workspace --no-run, python3 -m json.tool .mstar/status.json, rg

## Findings
### 🔴 Critical
- None

### 🟡 Warning
- **W-001**: New `warn_unknown_top_level_keys` doc comment triggers `clippy::too_long_first_doc_paragraph` under `-D warnings`, blocking CI. The lint cleanup commit (283d61e4) did not catch this. -> Split the first paragraph before the blank line or add a briefly-justified `#[allow(clippy::too_long_first_doc_paragraph)]`.
- **W-002**: `reconcile_from_filesystem` now parses `volume` from frontmatter but accepts non-positive values (e.g. `-1`) because `v.parse::<i32>()` succeeds and the value is used directly. This silently corrupts the `(work_id, volume, chapter)` PK space. -> Validate `fm_volume > 0`; clamp or default to `1` for any non-positive or unparseable value, and add tests for `volume: -1` / `volume: 0`.

### 🟢 Suggestion
- **S-001**: `KNOWN_TOP_LEVEL_KEYS` is a 5-element slice with linear `.contains()`; while effectively O(1), consider a `HashSet<&'static str>` for explicit O(1) intent and to avoid accidental ordering assumptions.
- **S-002**: The new `test_reconcile_volume_aware_from_frontmatter` covers missing and positive volume well; extend it with malformed (`"abc"`, `"1.5"`) and negative boundary cases now that the parser surface accepts arbitrary strings.

## Source Trace
- Finding ID: W-001
- Source Type: static-analysis
- Source Reference: `cargo clippy -p nexus-orchestration -p nexus-local-db -- -D warnings` output → `crates/nexus-orchestration/src/preset/loader.rs:1056`
- Confidence: High

- Finding ID: W-002
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-local-db/src/work_chapters.rs:479` — `let fm_volume: i32 = fm.get("volume").and_then(|v| v.parse().ok()).unwrap_or(1);`
- Confidence: High

- Finding ID: S-001
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-orchestration/src/preset/loader.rs:1054`
- Confidence: Medium

- Finding ID: S-002
- Source Type: test-review
- Source Reference: `crates/nexus-local-db/src/work_chapters.rs:1329`
- Confidence: Medium

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes
