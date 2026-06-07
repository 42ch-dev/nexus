---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-07-v1.37-novel-foundation-first"
verdict: "Request Changes"
generated_at: "2026-06-07T18:13:04Z"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: openai/gpt-5.5
- Review Perspective: architecture coherence + maintainability risk
- Report Timestamp: 2026-06-07T17:29:05Z

## Scope
- plan_id: 2026-06-07-v1.37-novel-foundation-first
- Review range / Diff basis: merge-base(iteration/v1.37)..HEAD on feature/v1.37-novel-foundation-first
- Working branch (verified): feature/v1.37-novel-foundation-first
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 23 changed files, plus cited plan/spec inputs
- Commit range (if not identical to Review range line, explain): implementation diff reviewed at 73b9cb85479a14c70ae79d898f4859fda3a4e324 (`feat(v1.37-p0): novel foundation-first UX hardening`) against iteration/v1.37. Before this report commit, branch HEAD also contained peer QC report commit 35b9963e; product-code findings above are based on the implementation diff, not peer report content.
- Tools run: git rev-parse, git branch, git log, git status, git diff, cargo +nightly fmt --all -- --check, cargo clippy -p nexus-orchestration -p nexus42 -p nexus-daemon-runtime -p nexus-local-db -- -D warnings, glob/read spot checks for .sqlx metadata and preset manifests

## Findings
### 🔴 Critical
- None.

### 🟡 Warning
- **F-001 — Formatting gate currently fails.** `cargo +nightly fmt --all -- --check` reports a required reflow in `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs` around the `workspace_root` expression. This is a CI gate failure and must be fixed before approval. -> Run `cargo +nightly fmt --all`, verify with `cargo +nightly fmt --all -- --check`, and keep generated contracts protected by the repo's nightly-format policy.
- **F-002 — New static SQL bypasses the mandatory sqlx compile-time macro policy.** `schedules.rs` adds static DML/SELECT/COUNT queries through runtime `sqlx::query`, `sqlx::query_as`, and `sqlx::query_scalar` (lines 158-170, 207-225, 1141-1150). `force_gates_audit.rs` correctly uses `sqlx::query!` for insert, but `list_force_gates_audit` uses runtime `query_as` for a static SELECT (lines 57-67). The comments describe these as DML/dynamic mapping/lookups, but the crate AGENTS rules only allow runtime SQL for DDL, PRAGMAs, or truly dynamic SQL. -> Convert static queries to `sqlx::query!`, `sqlx::query_as!`, or `sqlx::query_scalar!`; if a bool mapping needs an override, express it in the macro row mapping rather than bypassing offline checking. Regenerate and commit `.sqlx/` metadata.
- **F-003 — Gate enforcement can silently no-op for gated presets without a bound Work context.** In `add_schedule`, if `body.input.work_id` is missing or the work row is absent, the code falls through and enqueues the schedule (lines 199-206 and 304-309). The normative contract says enqueue-time evaluation runs after input binding, all gates must pass, and a failed gate means the preset is not enqueued. This creates a non-force bypass path for any API caller that omits `input.work_id`, including backward-compatible callers. -> Treat missing/unknown Work context for a preset with `gates:` as a gate failure or explicit `4xx` admission error with `preset_gates_failed`-style remediation; do not enqueue unless gates were evaluated or `force_gates` was audited.
- **F-004 — The script-driven novel gate values do not match the cited novel profile gate set.** The evaluator reads gates from the preset YAML (good; no hard-coded novel list found), but `embedded-presets/novel-writing/preset.yaml` currently declares only `work_profile`, `work_ref`, and filesystem gates. `novel-workflow-profile.md` §5.3.2 additionally requires `intake_status == complete` and a `previous_preset: novel-project-init` gate for the same Work (with `world_id` conditional on optional/required world binding). Because the runtime is manifest-driven, these omitted YAML entries are not enforced. -> Update the manifest to match the active §5.3.2 gate set, or amend the spec/plan explicitly if V1.37 P0 intentionally narrows the delivered gate values.

### 🟢 Suggestion
- **S-001 — Extract schedule admission concerns from `schedules.rs`.** The handler now owns novel completion guard logic, force-gates reason/audit handling, preset resolution, Work snapshot loading, input var binding, filesystem root selection, previous-preset lookup, and schedule creation. Even after the blockers above are fixed, this is becoming a scheduler-admission service embedded in an HTTP handler. Consider extracting a small `schedule_admission` / `gate_admission` helper that returns either an audited bypass, a structured gate failure, or an admission-ok context.
- **S-002 — Reuse the local-db audit module from the daemon handler.** `force_gates_audit.rs` is well-placed in `nexus-local-db` as persistence, but `schedules.rs` currently writes directly to `force_gates_audit`, duplicating the insert SQL and bypassing the module boundary. Once the macro issue is fixed, call `nexus_local_db::insert_force_gates_audit` (or an API-layer wrapper) from the daemon handler.
- **S-003 — Preserve machine-readable failed-gate details for CLI UX.** The minimal `FailedGate` shape includes `kind`, `expected`, `actual`, and `remediation`, which satisfies the assignment's minimum check. However, §7.9.2 examples include machine-readable discriminators such as `field`, `op`, `path`, and `must_exist`; the current shape folds some of that into human strings. Consider a tagged failed-gate payload or optional detail fields so CLI renderers do not have to parse prose.
- **S-004 — Add a short justification to new lint suppressions.** `patch_work_tx` adds `#[allow(clippy::too_many_lines)]` without a nearby rationale. The function mirrors the existing dynamic partial-update binder, so this is understandable, but repo policy asks not to suppress without justification. Add a concise comment or split reusable patch-binding helpers if that stays readable.

## Source Trace
- Finding ID: F-001
- Source Type: linter
- Source Reference: `cargo +nightly fmt --all -- --check` output: diff required in `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs` around `workspace_root`
- Confidence: High

- Finding ID: F-002
- Source Type: doc-rule | git-diff | manual-reasoning
- Source Reference: `crates/nexus-daemon-runtime/AGENTS.md` lines 17-35; `crates/nexus-local-db/AGENTS.md` lines 9-12; `schedules.rs` lines 158-170, 207-225, 1141-1150; `force_gates_audit.rs` lines 57-67
- Confidence: High

- Finding ID: F-003
- Source Type: doc-rule | manual-reasoning
- Source Reference: `orchestration-engine.md` §7.9.2 lines 763-785; `schedules.rs` lines 199-206 and 304-309
- Confidence: High

- Finding ID: F-004
- Source Type: doc-rule | git-diff | manual-reasoning
- Source Reference: `novel-workflow-profile.md` §5.3.2 lines 256-289; `crates/nexus-orchestration/embedded-presets/novel-writing/preset.yaml` lines 27-44
- Confidence: High

- Finding ID: S-001
- Source Type: manual-reasoning
- Source Reference: `schedules.rs` lines 133-314 plus existing novel-completion guard lines 96-130
- Confidence: Medium

- Finding ID: S-002
- Source Type: manual-reasoning | git-diff
- Source Reference: `force_gates_audit.rs` lines 29-45 and `schedules.rs` lines 145-176
- Confidence: High

- Finding ID: S-003
- Source Type: doc-rule | manual-reasoning
- Source Reference: `orchestration-engine.md` §7.9.2 lines 770-780; `preset_gate.rs` `FailedGate` fields
- Confidence: Medium

- Finding ID: S-004
- Source Type: doc-rule | git-diff
- Source Reference: root `AGENTS.md` clippy policy; `crates/nexus-local-db/src/works.rs` line 716
- Confidence: Medium

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 4 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

## Revalidation (2026-06-08)

Re-review after fix commit `7d7f3d0b`. Targeted scope: F-001..F-004 + S-001..S-004.

### Status by prior finding

- **F-001 (fmt)**: RESOLVED — `cargo +nightly fmt --all -- --check` completed cleanly (no formatter diff emitted).
- **F-002 (sqlx macro)**: UNRESOLVED — the required scan still finds static runtime SQL in the cited paths, including `schedules.rs` lines 165/238/332/430/1325 and `force_gates_audit.rs` lines 36/64/90; these are not DDL/PRAGMA/dynamic-SQL exceptions under `crates/nexus-daemon-runtime/AGENTS.md`.
- **F-003 (gated preset w/o work_id)**: RESOLVED — `add_schedule` now returns `422` with `preset_gates_failed` when a gated preset lacks `body.input.work_id`, and `gate_failure_returns_422_with_structured_body` passes for structured gate failure behavior.
- **F-004 (novel-writing YAML gates)**: RESOLVED — `novel-writing/preset.yaml` now places `gates:` under `preset:` and includes the §5.3.2 gate set (`work_profile`, `work_ref`, `intake_status`, required directories, and `previous_preset: novel-project-init`); `world_id` is explicitly omitted while world binding remains optional. The new `R-V137P0-01` residual tracks strict validation for misplaced YAML keys.
- **S-001 (extract schedule_admission)**: DEFERRED — no dedicated admission module was extracted; acceptable as follow-up maintainability work for this targeted fix round.
- **S-002 (dedup audit INSERT)**: ADDRESSED — the handler now calls `nexus_local_db::insert_force_gates_audit`, removing the duplicate audit INSERT from `schedules.rs` (the helper's runtime SQL is covered by F-002).
- **S-003 (machine-readable failed-gate)**: DEFERRED — no broader tagged failed-gate detail shape was added; acceptable future work.
- **S-004 (lint suppression justification)**: ADDRESSED — new `too_many_lines` suppressions now include rationale comments near `add_schedule` and `patch_work_tx`.

### New findings (if any)

- None beyond the still-unresolved F-002 blocker. Note: the required aggregate `cargo test` command also failed during `nexus-daemon-runtime` doc-tests because sqlx query macros could not find `DATABASE_URL`/prepared cache entries, reinforcing that the sqlx/static-query gate is not ready for approval.

### New evidence

- `cargo +nightly fmt --all -- --check` output: passed; command emitted only `Finished dev profile ...` before the next gate.
- `cargo clippy -p nexus-orchestration -p nexus42 -p nexus-daemon-runtime -p nexus-local-db -- -D warnings` output: passed; the chained gate proceeded to `cargo test`.
- `cargo test -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db -p nexus42` summary: failed in `nexus-daemon-runtime` doc-tests with sqlx macro errors (`set DATABASE_URL to use query macros online, or run cargo sqlx prepare`) and `can't find crate for nexus_orchestration`; unit/integration test execution before doc-tests included `608 passed` for `nexus42` plus the other shown suites. Targeted confirmation `cargo test -p nexus-daemon-runtime --test fl_e_schedule_api gate_failure_returns_422_with_structured_body` passed (`1 passed; 0 failed`).
- `rg -n "sqlx::query[^!]" crates/nexus-daemon-runtime/src/ crates/nexus-orchestration/src/ crates/nexus-local-db/src/` evidence: remaining targeted runtime static SQL includes `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs:165`, `:238`, `:332`, `:430`, `:1325`; `crates/nexus-local-db/src/force_gates_audit.rs:36`, `:64`, `:90`; and `crates/nexus-orchestration/src/capability/builtins/novel_scaffold.rs:260`. Dynamic builders/DDL/PRAGMA occurrences remain elsewhere and were not treated as this finding's blocker.

**Updated Verdict**: Request Changes
