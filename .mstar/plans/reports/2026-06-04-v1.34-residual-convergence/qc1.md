---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-04-v1.34-residual-convergence"
verdict: "Request Changes"
generated_at: "2026-06-04T16:18:19Z"
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
