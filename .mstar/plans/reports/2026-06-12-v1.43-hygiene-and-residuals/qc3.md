---
report_kind: qc-review
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-12-v1.43-hygiene-and-residuals
verdict: Approve
generated_at: 2026-06-12T21:38:03+08:00
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

## Revalidation (post-fix wave, fix commit 016832f1)

**Re-review mode**: Targeted — qc-specialist-3 only (raised 2 blocking Warnings in initial wave)
**Fix range reviewed**: 283d61e4..016832f1
**Files in fix wave**: crates/nexus-orchestration/src/preset/loader.rs, Cargo.toml, crates/nexus-local-db/src/work_chapters.rs, Cargo.lock

### Previously raised blocking findings — re-check
| Finding ID | Summary | Status | Evidence |
|------------|---------|--------|----------|
| qc3-W-001 | clippy too_long_first_doc_paragraph | PASS | `rg -n 'allow\(clippy::too_long_first_doc_paragraph\)' crates/nexus-orchestration/src/preset/loader.rs` → line 1066; doc comment split and `#[allow(...)]` with justification comment present; `cargo clippy -p nexus-orchestration -p nexus-local-db -- -D warnings` finished clean |
| qc3-W-002 | negative volume silently accepted | PASS | `rg -n 'raw_volume|\>= 1'` → lines 480-481; `tracing::warn!` with path + bad volume emitted and defaults to 1; new `test_reconcile_volume_rejects_negative` exists at line 1877 and passes |

### Static checks (re-run on full P-last feature scope a693752b..016832f1)
- `cargo +nightly fmt --all --check`: PASS (no output)
- `cargo clippy -p nexus-orchestration -p nexus-local-db -- -D warnings`: PASS (`Finished dev profile` with no warnings)
- `cargo test -p nexus-orchestration --lib`: PASS — 560 passed; 0 failed; 1 ignored
- `cargo test -p nexus-local-db --lib`: PASS — 187 passed; 0 failed; 0 ignored
- Targeted volume tests (`cargo test -p nexus-local-db --lib test_reconcile_volume`): PASS — 2 passed; 0 failed

### Updated verdict
**Verdict**: Approve
**Rationale**: Both blocking Warnings raised in the initial review are resolved in fix commit `016832f1`. The clippy lint is suppressed with a justified `#[allow]` and the doc paragraph has been split; the negative/zero volume frontmatter input now defaults to `1` with an observable `tracing::warn!` and is covered by a new regression test. All scoped static checks and tests pass with no failures.
