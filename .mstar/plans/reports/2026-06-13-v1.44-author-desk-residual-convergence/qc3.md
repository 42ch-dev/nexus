---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-13-v1.44-author-desk-residual-convergence"
verdict: "Approve"
generated_at: "2026-06-13"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: kimi-for-coding/k2p6
- Review Perspective: Performance and reliability risk (Reviewer #3)
- Report Timestamp: 2026-06-13

## Scope
- plan_id: 2026-06-13-v1.44-author-desk-residual-convergence
- Review range / Diff basis: cbb18e25..ca2ac052
- Working branch (verified): iteration/v1.44
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 4
- Commit range: cbb18e25..ca2ac052 (5 commits: 4 fix + merge)
- Tools run:
  - git log --oneline cbb18e25..ca2ac052
  - git diff cbb18e25..ca2ac052 --stat
  - git diff cbb18e25..ca2ac052 -- crates/nexus42/src/commands/creator/run.rs crates/nexus42/tests/creator_works.rs crates/nexus-orchestration/embedded-presets/novel-writing/prompts/draft-chapter.md .mstar/status.json
  - cargo +nightly fmt --all --check (clean)
  - cargo clippy --all -- -D warnings (clean)
  - cargo test -p nexus42 --test creator_works (7/7 passed)
  - cargo test -p nexus42 --test integration (50/50 passed)

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
- **S-01 (test runtime amortization)**: `crates/nexus42/tests/creator_works.rs` spawns the `nexus42` binary once per test (7 invocations, ~2.65s total). This is within the current per-file budget, but if the CLI surface suite expands significantly, process-spawn overhead will dominate. Consider parameterizing the help/validation assertions or adding a shared binary-location fixture so future additions do not linearly increase wall-clock time.
- **S-02 (prompt template token budget)**: The restored frontmatter field docs add ~350 bytes (~50–80 tokens) to every chapter draft prompt. This is negligible against the declared `max_tokens: 8000`, but it sets a precedent for incremental template growth. If future P-last/P1 waves continue expanding prompts, consider adding a template-size regression guard to prevent silent token-budget erosion.
- **S-03 (debug span log-sensitivity)**: The `tracing::debug!` event in `stage_advance` emits local filesystem paths (`outline_path`, `body_path`) at debug level. In the current local-only deployment this is acceptable, but if support workflows ever request debug logs or cloud sync is re-enabled, these paths may expose user directory structure. Document the sensitivity model for `fl_e.stage` debug output in a runbook or support note.

## Source Trace
- Finding ID: (no blocking findings)
- Source Type: manual-diff + test-run + lint + runtime measurement
- Source Reference: git diff cbb18e25..ca2ac052 + cargo test + cargo clippy + cargo +nightly fmt --check
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

## Evidence (Commands Executed)

### Checkout & Range Verification
```bash
$ git rev-parse --show-toplevel && git branch --show-current && git rev-parse HEAD
/Users/bibi/workspace/organizations/42ch/nexus
iteration/v1.44
2e9f2331d3e4cb54930477f2c6017197f9aadafd

$ git log --oneline cbb18e25..ca2ac052
ca2ac052 merge(v1.44 P3): author-desk UX residual convergence
19497b45 chore(v1.44): T5 — update status.json residual closures (R-V141P0-04, R-V138P1-02, R-V138P1-07 resolved; R-V141P1-15 defer note)
93db2288 fix(v1.44): T4 — add tracing::debug span for stage_advance chapter context (R-V138P1-07)
6b834ae8 fix(v1.44): T3 — restore compact frontmatter field docs in draft-chapter template (R-V138P1-02)
d5ebbe6c feat(v1.44): T2 — integration test for creator works use / completion-lock (R-V141P0-04)

$ git merge-base --is-ancestor cbb18e25 ca2ac052 && echo "Range valid"
Range valid
```

### Static Checks
```bash
$ cargo +nightly fmt --all --check
(no output — clean)

$ cargo clippy --all -- -D warnings
... Finished `dev` profile ... (clean, 0 warnings treated as errors)
```

### Test Evidence (Performance/Reliability Scope)
```bash
$ cargo test -p nexus42 --test creator_works
running 7 tests
test works_completion_lock_help_shows_subcommands ... ok
test works_help_lists_use_subcommand ... ok
test works_completion_lock_release_help_shows_expected_text ... ok
test works_help_lists_all_expected_subcommands ... ok
test works_use_help_shows_expected_text ... ok
test works_use_requires_work_id ... ok
test works_completion_lock_release_requires_work_id ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.65s

$ cargo test -p nexus42 --test integration
running 50 tests
... (all 50 passed)

test result: ok. 50 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.43s
```

### Diff Scope (Performance/Reliability Relevant)
- `crates/nexus42/src/commands/creator/run.rs`: +13 LOC — added a single `tracing::debug!` event (target: `fl_e.stage`) after chapter context extraction in `stage_advance`. Uses `%`/`-debug` field capture; values are formatted lazily when the callsite is enabled. The function is invoked once per `creator run stage advance` CLI command, so the event is not on a tight inner loop.
- `crates/nexus42/tests/creator_works.rs` (new, 161 LOC): 7 hermetic CLI surface tests using `assert_cmd` + `predicates`. No daemon, no DB, no filesystem mutation. Each test spawns the `nexus42` binary independently.
- `crates/nexus-orchestration/embedded-presets/novel-writing/prompts/draft-chapter.md`: +7 LOC — restored compact frontmatter field docs. Template size grows from ~1.7 KiB to ~2.0 KiB.
- `.mstar/status.json`: residual lifecycle updates for R-V141P0-04, R-V138P1-02, R-V138P1-07 (resolved), and R-V141P1-15 (open + defer note). No performance/reliability impact.

## Performance & Reliability Assessment (qc-specialist-3 focus)

### `stage_advance` tracing overhead (R-V138P1-07)
The added `tracing::debug!` is an **event**, not a span, which is actually lighter-weight than the span alternative mentioned in the residual. Because:
- the callsite is `debug` level (disabled by default in release builds),
- field values use `%` and `?` capture and are formatted lazily only when the event is enabled,
- `stage_advance` runs once per user-facing CLI invocation (not in a loop or per-chapter batch),
the runtime overhead is negligible. The reliability upside is clear: chapter selection failures now have structured context (work_id, next_chapter, paths, slug) for production debugging.

### Integration test runtime budget (R-V141P0-04)
The new `creator_works.rs` file adds ~2.65s of wall-clock test time, all of it process-spawn overhead from invoking `nexus42 --help` / missing-arg paths. The tests are hermetic and deterministic. The per-test average (~0.38s) is reasonable for `assert_cmd` CLI surface tests. The suite currently fits comfortably within the existing `nexus42` integration-test budget (creator_works 2.65s + integration 6.43s ≈ 9.1s). No action required for this plan, but future scaling should watch the spawn-per-test pattern.

### Draft-chapter template size (R-V138P1-02)
Restoring the frontmatter docs increases the template by ~350 bytes (~50–80 tokens at typical LLM tokenizers). Against the declared `max_tokens: 8000` and the 3000–5000 word body target, this is well under 1% of the prompt budget. The reliability gain (fewer malformed frontmatter blocks) outweighs the token cost.

### Residual disposition correctness
All four P3 whitelist residuals are dispositioned correctly in `status.json`: three resolved with commit/plan references, one (R-V141P1-15) explicitly deferred with an open lifecycle and explanatory note. No new critical/high residual is introduced.

## Conclusion
The P3 wave is narrowly scoped, matches the plan whitelist, and introduces no material performance or reliability regressions. All required evidence (lint, fmt, tests, range verification) passes. Verdict is **Approve**.
