---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-04-v1.34-residual-convergence"
verdict: "Approve w/ residuals"
generated_at: "2026-06-05"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: openai/gpt-5.5
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-04T16:18:19Z

## Scope
- plan_id: 2026-06-04-v1.34-residual-convergence
- Review range / Diff basis: `merge-base: origin/main..HEAD` on `feature/v1.34-residual-convergence` (i.e. `git diff $(git merge-base HEAD origin/main)..HEAD`)
- Working branch (verified): feature/v1.34-residual-convergence
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-residual-convergence
- Files reviewed: 14 (7 changed files plus plan/status, crate AGENTS, validation/loader context)
- Commit range: 5b71318aa8cd2e91e3115820dec7eac71869f261..HEAD
- Tools run:
  - `git rev-parse --show-toplevel`
  - `git branch --show-current`
  - `git log --oneline -5`
  - `git diff --stat $(git merge-base HEAD origin/main)..HEAD`
  - `git diff --name-status $(git merge-base HEAD origin/main)..HEAD`
  - `cargo clippy -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -- -D warnings 2>&1 | tail -10`
  - `cargo test -p nexus-orchestration all_embedded_presets_pass_strict_validation_gate -- --nocapture`
  - `cargo test -p nexus-orchestration --test run_intents_validation`

## Findings

### 🔴 Critical

- **C-001 — Residual convergence decisions were not written to the harness SSOT or plan note.** The reviewed branch only changes 7 Rust source/test files; it does not update `.mstar/status.json` or `.mstar/plans/2026-06-04-v1.34-residual-convergence.md`. This leaves the 9 fix/defer decisions (including the 4 explicit defers: R-V133P1-03, R-V133P1-08, R-V133P1-09, R-P2-02) unrecorded in the root `residual_findings` lifecycle or a plan note, despite plan Acceptance #1 requiring "closed or deferred with `status.json` + plan note" and Task T3 requiring status residual decisions. This is a release/process blocker for a plan whose purpose is residual convergence: future agents cannot inherit which residuals were closed, still open, or intentionally deferred from the SSOT.
  - **Fix:** before QA/merge, update root `.mstar/status.json` residual entries and the plan note/index to record each of the 9 decisions with evidence/target, or explicitly have PM own that update before marking this plan out of review. Do not leave closure/defer evidence only in commit messages or chat.

### 🟡 Warning

- **W-001 — R-P2-01 `creator.inject_prompt` schema fix is partial: the validator still reports missing required `prompt` for prompt-file presets.** The schema adds `prompt_file` and `vars`, but keeps `required:["prompt"]`. Embedded `novel-writing` and `research` call `creator.inject_prompt` with `prompt_file`/`vars` and no `prompt`, so the A4 schema check still emits `CapabilityArgDrift` for each usage. Targeted evidence: `cargo test -p nexus-orchestration all_embedded_presets_pass_strict_validation_gate -- --nocapture` passes only because `preset::tests::all_embedded_presets_pass_strict_validation_gate` downgrades `creator.inject_prompt` drift to warnings; output still contains four warnings: `capability 'creator.inject_prompt' requires argument 'prompt' which is not provided` for novel-writing and research. Architecturally, this does not fully align the capability schema with orchestration-layer args and does not close R-P2-01 as stated by commit `a044f94`.
  - **Fix:** model the schema as either `prompt` or `prompt_file` (for example JSON Schema `anyOf`/`oneOf` if the lightweight checker is extended, or remove `prompt` from top-level `required` and add an explicit semantic check for execution-time requirements). Add a regression test that asserts embedded `creator.inject_prompt` prompt-file calls no longer produce `CapabilityArgDrift` warnings, not merely that the strict gate passes after downgrade.

- **W-002 — R-V133P1-11 true-total semantics silently degrade to page size on `COUNT(*)` failure.** `list_works` now calls `works::count_works`, but maps any count error to `records.len()` with no log or API error (`map_or(records.len(), |n| n as usize)`). That fallback reintroduces the exact misleading pagination semantics this residual is meant to fix whenever the count query fails, while returning a successful HTTP response. This weakens observability and makes the API contract ambiguous.
  - **Fix:** propagate the `count_works` error through the same handler error path as `list_works`, or at least log an explicit warning and document that `total` is best-effort. For this residual, propagating is cleaner because the contract now claims true total row count.

### 🟢 Suggestion

- **S-001 — Keep the duplicated `list_works`/`count_works` dynamic WHERE construction on the R-V133P1-09 migration path.** The new `count_works` mirrors the existing dynamic runtime SQL pattern and includes a SAFETY comment with bound parameters, so it is acceptable within the current deferred R-V133P1-09 context. However, it duplicates the same optional-filter WHERE assembly as `list_works`, which can drift as filters are added. When R-V133P1-09 is addressed, centralize the filter builder or convert both list and count paths to compile-time checked forms where possible.

## Source Trace

- Finding ID: C-001
  - Source Type: git-diff / doc-rule / manual-reasoning
  - Source Reference: `git diff --name-status $(git merge-base HEAD origin/main)..HEAD` lists only 7 Rust files; plan lines 49-58 require T3 + status/plan decision updates; `.mstar/status.json` residual entries remain under prior plan keys.
  - Confidence: High

- Finding ID: W-001
  - Source Type: targeted-test / git-diff / manual-reasoning
  - Source Reference: `crates/nexus-orchestration/src/capability/builtins/creator.rs:453-457`; `crates/nexus-orchestration/src/preset/validation.rs:710-724`; targeted test output from `all_embedded_presets_pass_strict_validation_gate` still reports four `creator.inject_prompt requires argument 'prompt'` drift warnings.
  - Confidence: High

- Finding ID: W-002
  - Source Type: git-diff / manual-reasoning
  - Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:233-238`; `crates/nexus-local-db/src/works.rs:438-475`.
  - Confidence: High

- Finding ID: S-001
  - Source Type: git-diff / doc-rule
  - Source Reference: `crates/nexus-local-db/AGENTS.md:9-12`; `crates/nexus-local-db/src/works.rs:365-403` and `:444-469`.
  - Confidence: Medium

## Evidence

```text
$ git rev-parse --show-toplevel
/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-residual-convergence

$ git branch --show-current
feature/v1.34-residual-convergence

$ git log --oneline -5
a044f94 fix(residual): R-P2-01 add prompt_file + vars to creator.inject_prompt input_schema
27df8cb fix(residual): R-V133P1-11 list_works total returns true row count via separate COUNT(*) query
cbe5e78 fix(residual): R-V133P1-05 promote cross-claim to Error + R-V133P1-12 migrate inline tests to standalone binary
29aa9bf fix(residual): R-V133P1-07 use url::Url to encode status filter in creator run list
5b71318 docs(harness): add V1.34 FL-E workflow and agent tool bridge planning

$ git diff --stat $(git merge-base HEAD origin/main)..HEAD
 .../nexus-daemon-runtime/src/api/handlers/works.rs |   6 +-
 crates/nexus-local-db/src/lib.rs                   |   5 +-
 crates/nexus-local-db/src/works.rs                 |  47 +++++
 .../src/capability/builtins/creator.rs             |   5 +-
 .../nexus-orchestration/src/preset/validation.rs   | 153 ++++-----------
 .../tests/run_intents_validation.rs                | 206 +++++++++++++++++++++
 crates/nexus42/src/commands/creator/run.rs         |  18 +-
 7 files changed, 316 insertions(+), 124 deletions(-)

$ cargo clippy -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -- -D warnings 2>&1 | tail -10
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.21s
```

Additional targeted validation evidence:

```text
$ cargo test -p nexus-orchestration --test run_intents_validation
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

$ cargo test -p nexus-orchestration all_embedded_presets_pass_strict_validation_gate -- --nocapture
embedded preset validation warnings (non-blocking):
preset 'novel-writing' capability arg drift at states[0].enter[0].args: capability 'creator.inject_prompt' requires argument 'prompt' which is not provided
preset 'novel-writing' capability arg drift at states[2].enter[0].args: capability 'creator.inject_prompt' requires argument 'prompt' which is not provided
preset 'research' capability arg drift at states[0].enter[0].args: capability 'creator.inject_prompt' requires argument 'prompt' which is not provided
preset 'research' capability arg drift at states[2].enter[0].args: capability 'creator.inject_prompt' requires argument 'prompt' which is not provided
test preset::tests::all_embedded_presets_pass_strict_validation_gate ... ok
```

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes

## Revalidation

### Scope and checkout revalidated

- Targeted re-review scope: fix wave 2 `71c10cc..2a84e68` plus harness archive commit `21e4deb`; overall P0 context remains `merge-base: origin/main..HEAD` on `feature/v1.34-residual-convergence`.
- Checkout evidence:

```text
$ git rev-parse --show-toplevel
/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-residual-convergence

$ git branch --show-current
feature/v1.34-residual-convergence

$ git log --oneline -10
2a84e68 fix(residual): R-V133P1-11 v3 log warn on count_works failure
a724e99 fix(residual): R-V133P1-11 v2 list_works+count_works in shared transaction
71c10cc fix(residual): R-P2-01 v2 make prompt optional + oneOf for prompt_file
21e4deb harness(v1.34-p0): archive 4 resolved residuals from v1.33-p1 + mark plan InReview
fe19376 qc(v1.34-residual-convergence): qc3.md — performance & reliability review
72bb2c3 docs(qc): add V1.34 residual convergence qc1 report
a044f94 fix(residual): R-P2-01 add prompt_file + vars to creator.inject_prompt input_schema
27df8cb fix(residual): R-V133P1-11 list_works total returns true row count via separate COUNT(*) query
cbe5e78 fix(residual): R-V133P1-05 promote cross-claim to Error + R-V133P1-12 migrate inline tests to standalone binary
29aa9bf fix(residual): R-V133P1-07 use url::Url to encode status filter in creator run list
```

### Original findings disposition

- **C-001 — Resolved.** Commit `21e4deb` writes the harness lifecycle evidence that was missing in wave 1:
  - `.mstar/archived/residuals/2026-06-04-v1.33-work-model-and-creator-run.json` now has 4 archived entries: `R-V133P1-05`, `R-V133P1-07`, `R-V133P1-11`, `R-V133P1-12`.
  - `.mstar/status.json` root `residual_findings["2026-06-04-v1.33-work-model-and-creator-run"]` now has only 3 open entries: `R-V133P1-03`, `R-V133P1-08`, `R-V133P1-09`.
  - The affected v1.33 plan row is marked `InReview` in `.mstar/status.json` for this P0 convergence wave.

```text
$ python3 - <<'PY'
archive_entries 4 ['R-V133P1-05', 'R-V133P1-07', 'R-V133P1-11', 'R-V133P1-12']
open_entries 3 ['R-V133P1-03', 'R-V133P1-08', 'R-V133P1-09']
v1.33_plan_status InReview
PY
```

- **W-001 — Resolved.** Commit `71c10cc` changes `creator.inject_prompt` from top-level `required:["prompt"]` to an `anyOf` contract (`prompt` or `prompt_file`) and extends the lightweight validator with `check_any_of_semantics`. The fix is scoped to the existing top-level schema checker and does not introduce a full JSON Schema dependency or broaden the previous drift downgrade. Targeted validation confirms the `creator.inject_prompt` drift warnings went from 4 to 0.

```text
$ cargo test -p nexus-orchestration --lib all_embedded_presets_pass_strict_validation_gate -- --nocapture 2>&1 | tail -10
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.14s
     Running unittests src/lib.rs (target/debug/deps/nexus_orchestration-e2f322a1935237d7)

running 1 test
embedded preset validation warnings (non-blocking):
preset 'memory-augmented' warning at states[2].enter[0].args: schema check skipped for capability 'creator.write_memory': input_schema is not valid JSON
test preset::tests::all_embedded_presets_pass_strict_validation_gate ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 387 filtered out; finished in 0.01s

$ cargo test ... full-output count check
creator.inject_prompt occurrences: 0
CapabilityArgDrift prompt required occurrences: 0
```

- **W-002 — Resolved.** Commits `a724e99` and `2a84e68` replace the prior best-effort fallback with `works::list_and_count_works(...)` and map any list/count failure to the handler's database error path after `tracing::warn!`. This removes the silent `records.len()` fallback and preserves true-total semantics. `crates/nexus-daemon-runtime/src/api/handlers/works.rs:235-245` contains the warning before returning `NexusApiError::Internal`. `crates/nexus-local-db/src/works.rs:394-404` keeps list and count in one transaction while preserving the existing public `list_works` and `count_works` APIs.

```text
$ cargo test -p nexus-local-db --lib test_list_works -- --nocapture 2>&1 | tail -10
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.12s
     Running unittests src/lib.rs (target/debug/deps/nexus_local_db-fc0303a6f1638ae5)

running 2 tests
test works::tests::test_list_works_with_status_filter ... ok
test works::tests::test_list_works_by_creator ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 115 filtered out; finished in 0.02s

$ cargo test -p nexus-daemon-runtime --test works_api list_works_returns_200 -- --nocapture 2>&1 | tail -10
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.25s
     Running tests/works_api.rs (target/debug/deps/works_api-b2731f65dbdea4f1)

running 1 test
test list_works_returns_200 ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 17 filtered out; finished in 0.05s
```

- **S-001 — Not revalidated as requested.** The new transaction helper still uses the existing dynamic-SQL filter-builder path with SAFETY comments. The duplication remains on the deferred R-V133P1-09 migration path and is not a blocker for this targeted re-review.

### Architecture and maintainability re-check

- `anyOf` validation is a narrow extension of the existing lightweight schema architecture: it only interprets per-alternative `required` arrays, continues to use existing `DiagnosticCategory::CapabilityArgDrift`, and avoids introducing a new schema-validation library for one residual.
- `list_and_count_works` is backward compatible for existing callers: `list_works` and `count_works` remain exported and keep their signatures, while the API handler opts into the atomic list+count path. The only behavior change is intentional: count failures now become observable database errors instead of silent page-size totals.
- The fix wave is surgical: code changes are limited to the residual touchpoints plus harness report/lifecycle files; no unrelated feature work or broad refactor was introduced.

### Verification

```text
$ cargo clippy -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -p nexus-creator-memory -- -D warnings 2>&1 | tail -10
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.37s
```

### Revalidation verdict

The original blocking finding and both warnings are resolved. No new Critical findings were found in the targeted architecture/maintainability re-review. Remaining non-blocking residual context is the already-deferred R-V133P1-09 dynamic SQL migration path, so the targeted re-review verdict is **Approve w/ residuals**.
