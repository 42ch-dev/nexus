---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-14-v1.46-research-auto-chain-e2e"
verdict: "Request Changes"
generated_at: "2026-06-15"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: kimi-for-coding/k2p7
- Review Perspective: Performance and reliability risk — determinism, hermetic isolation, test runtime cost, supervisor-state robustness under CI.
- Report Timestamp: 2026-06-15T02:55:00+08:00

## Scope
- plan_id: `2026-06-14-v1.46-research-auto-chain-e2e`
- Review range / Diff basis: `merge-base: 1d776d23 (P2 Done commit, base of P3 work) → tip: 87f00619 (P3 merge) (1 commit + 1 --no-ff merge = 2 total)` — equivalent `git diff 1d776d23..87f00619` or `git show --stat 1d776d23..87f00619`
- Working branch (verified): `iteration/v1.46`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 1
- Commit range (if not identical to Review range line, explain): identical
- Tools run:
  - `cargo test -p nexus-orchestration --test research_supervisor_e2e`
  - `cargo clippy -p nexus-orchestration --test research_supervisor_e2e -- -D warnings`
  - `cargo +nightly clippy -p nexus-orchestration --test research_supervisor_e2e -- -D warnings`
  - `cargo test --all`
  - `cargo +nightly fmt --all --check`
  - Pre-existing claim verification: `cargo +nightly clippy -p nexus-orchestration --tests -- -D warnings` on `origin/main` HEAD `63b36a32`

## Findings

### 🔴 Critical
None.

### 🟡 Warning

#### W-1: Debug-output assertion couples preset-gate test to non-stable representation
- **Issue**: `research_preset_loads_and_structurally_valid` asserts the presence of `intake_status` and `work_ref` gates by formatting each gate with `format!("{g:?}")` and checking substrings (`crates/nexus-orchestration/tests/research_supervisor_e2e.rs:255-270`). `Debug` output is not a semantic contract; a rename of a private struct field, a change to `#[derive(Debug)]` formatting, or a newtype wrapper will cause this test to fail even though the runtime gate semantics are unchanged. Conversely, the test could pass while the actual gate operator/value changes, because the substring only checks field names.
- **Fix**: Replace the `format!("{g:?}")` substring checks with typed pattern matching on `nexus_contracts::local::orchestration::preset_gate::Gate` (its variants and public fields are stable generated wire types). Assert that one `Gate::WorkField { field: "intake_status", op: GateOp::Equals { value: "complete" } }` exists and that a `Gate::WorkField { field: "work_ref", op: GateOp::Required }` (or equivalent) exists.
- **Source**: `git-diff` + manual-reasoning; `crates/nexus-orchestration/tests/research_supervisor_e2e.rs:255-270`
- **Confidence**: High

### 🟢 Suggestion

#### S-1: Raw SQL fixture duplicates schema knowledge
- **Issue**: `insert_research_schedule` issues a raw `INSERT INTO creator_schedules ...` with hardcoded column order and the `preset_version = 2` constant (`crates/nexus-orchestration/tests/research_supervisor_e2e.rs:108-135`). This duplicates schema knowledge already owned by the crate's migrations/DAO layer and is brittle if columns are reordered or renamed.
- **Fix**: If a test-only helper does not already exist in `nexus-local-db` or `nexus-orchestration` tests, wrap the insert in a DAO-style helper that names columns explicitly, or reuse `auto_chain::insert_driver_schedule` (mirrored from `tests/auto_chain.rs`) with a `preset_id` parameter. At minimum, add a comment referencing the migration file that owns the column set.
- **Source**: `git-diff`; `crates/nexus-orchestration/tests/research_supervisor_e2e.rs:108-135`
- **Confidence**: Medium

#### S-2: `schedule_status` helper panics on missing row
- **Issue**: `schedule_status` uses `fetch_one(...).unwrap()` (`crates/nexus-orchestration/tests/research_supervisor_e2e.rs:138-145`). A future test that queries a row before inserting it will panic with an unhelpful message.
- **Fix**: Return `Option<String>` (or `Result`) and let the caller `expect("schedule row must exist")` with a descriptive message, or use `fetch_optional` and `unwrap_or_else`.
- **Source**: `git-diff`; `crates/nexus-orchestration/tests/research_supervisor_e2e.rs:138-145`
- **Confidence**: Low

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W-1 | manual-reasoning / git-diff | `crates/nexus-orchestration/tests/research_supervisor_e2e.rs:255-270` | High |
| S-1 | manual-reasoning / git-diff | `crates/nexus-orchestration/tests/research_supervisor_e2e.rs:108-135` | Medium |
| S-2 | manual-reasoning / git-diff | `crates/nexus-orchestration/tests/research_supervisor_e2e.rs:138-145` | Low |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes

## Notes

- **Pre-existing clippy claim independently verified**: `cargo +nightly clippy -p nexus-orchestration --tests -- -D warnings` fails on `origin/main` HEAD `63b36a32` with 60+ errors in `src/tasks/mod.rs`, `src/worker/registry.rs`, and existing test files — matching the PM-override recorded in `.mstar/status.json` under `residual_findings["pre-existing-v1.45-orchestration-clippy"]` (`R-V145-PRE-CLIPPY-001`). These failures are **not** raised as V1.46 P3 findings.
- **P3-introduced file is clean in isolation**: `cargo clippy -p nexus-orchestration --test research_supervisor_e2e -- -D warnings` and `cargo +nightly` variant both pass; `cargo +nightly fmt --all --check` passes; `cargo test -p nexus-orchestration --test research_supervisor_e2e` passes (5/5); `cargo test --all` passes.
- **Reliability positives**: one pool per test; no network/ACP/LLM; no wall-clock assertions; deterministic preset loading; `resume_running_as_paused` idempotency is explicitly asserted; test runtime is ~0.15s for the file, well under the 30s CI budget.
