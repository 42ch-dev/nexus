---
report_kind: qa
reviewer: qa-engineer
plan_id: "2026-06-11-v1.42-agent-tool-production-wiring"
verdict: "Approve"
generated_at: "2026-06-11T16:12:37Z"
qa_mode: "full verification"
---

# QA Verification Report — V1.42 P3 Agent Tool Production Wiring (DF-47)

## Reviewer Metadata
- Reviewer: @qa-engineer
- Runtime Agent ID: qa-engineer
- Runtime Model: volcengine-plan/ark-code-latest
- QA Mode: full verification
- Report Timestamp: 2026-06-11T16:12:37Z

## Scope
- plan_id: `2026-06-11-v1.42-agent-tool-production-wiring`
- Review range / Diff basis: `merge-base: 11f8079a` (P3 status commit) + `tip: HEAD` of `iteration/v1.42` (`c1dcfdef`) — equivalent to `git diff 11f8079a...HEAD` on `.worktrees/v1.42-p3-reqc`. Covers 13 commits (4 implementation + 3 QC + 1 fix-wave + 1 PM status + 1 PM fix-wave merge + 1 qc2 re-review merge + 1 qc3 re-review merge + 1 PM consolidation).
- Working branch (verified): `HEAD` (detached)
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-p3-reqc`
- Integrated HEAD verified: `c1dcfdef qc(v1.42 P3): PM consolidated revalidation Approve (qc1+qc2+qc3 all Approve)`
- Files reviewed: plan, primary spec, compass, consolidated QC reports, archived residuals, production wiring code, production E2E tests, status/tracker artifacts.
- Tools run: git checkout/range checks, scoped cargo tests, clippy, nightly fmt, source reads, tracker/spec/archive checks.

## Acceptance Criteria Mapping

| AC | Plan criterion | QA result | Evidence |
| --- | --- | --- | --- |
| AC1 | One tool callable from a running schedule without manual CLI invocation. | ✓ Verified | Production chain exists: `EnterAction::HostTool` → `StateCompositeTask` → `HostToolCallTask::from_dispatch` → `DaemonToolDispatchAdapter::dispatch_tool` → `HostToolExecutor::dispatch_for_schedule` → `HostToolExecutor::execute` handler. `agent_tool_production_wiring` 6/6 passed. |
| AC2 | Authz/locks: read-only tool respects completion-lock; mutating tools follow runtime_lock (P0). | ✓ Verified for P3 slice | Cross-creator FORBIDDEN variant assertion is present and passes; completion-lock test seeds `completion_locked_at` and confirms read-only `nexus.orchestration.schedule_status` succeeds. Mutating tools remain out of P3 scope and continue under P0 runtime_lock policy. |
| AC3 | Test proves E2E path in CI hermetic mode. | ✓ Verified | `cargo test -p nexus-daemon-runtime --test agent_tool_production_wiring` passed 6 tests; `cargo test -p nexus-daemon-runtime --test agent_tool_api` passed 26 tests; aggregate `agent_tool` filter command exited successfully. |

## Findings

### Critical
- None.

### Warning
- None.

### Suggestion
- None raised by QA. The six non-blocking QC Suggestions remain deferred/tracked by PM process; QA did not re-open them.

### Evidence Note (non-finding)
- The assignment-listed regression command `cargo test -p nexus-orchestration --lib gonogo judge_next judge_llm` is not valid Cargo syntax and fails with `unexpected argument 'judge_next' found`. To preserve intent, QA also ran the three filters separately (`gonogo`, `judge_next`, `judge_llm`), and all passed. This is recorded as command evidence, not an implementation finding.

## Source Trace

- Trace ID: QA-AC1-PRODUCTION-PATH
  - Source Type: source-read + test evidence
  - Source Reference: `crates/nexus-contracts/src/local/orchestration/preset.rs:196-208`; `crates/nexus-orchestration/src/tasks/mod.rs:751-764`; `crates/nexus-orchestration/src/tasks/mod.rs:1540-1662`; `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:379-394, 440-459`; `crates/nexus-daemon-runtime/src/boot.rs:126-143`.
  - Confidence: High

- Trace ID: QA-AC2-AUTHZ-LOCKS
  - Source Type: source-read + test evidence
  - Source Reference: `crates/nexus-daemon-runtime/tests/agent_tool_production_wiring.rs:209-244` (FORBIDDEN variant assertion) and `:286-316` (completion-lock read-only test).
  - Confidence: High

- Trace ID: QA-AC3-HERMETIC-CI
  - Source Type: command-output
  - Source Reference: `cargo test -p nexus-daemon-runtime --test agent_tool_production_wiring`; `cargo test -p nexus-daemon-runtime --test agent_tool_api`; scoped clippy and fmt commands below.
  - Confidence: High

- Trace ID: QA-QC-FIX-WAVE
  - Source Type: artifact review
  - Source Reference: `.mstar/archived/residuals/2026-06-11-v1.42-agent-tool-production-wiring.json` has 4 closed entries; `qc2.md` and `qc3.md` targeted revalidation verdicts are Approve; root `.mstar/status.json` open residual list for this plan is empty.
  - Confidence: High

## Evidence

### Checkout and review range

```bash
git rev-parse --show-toplevel
# /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-p3-reqc

git rev-parse --abbrev-ref HEAD
# HEAD

git log -1 --oneline
# c1dcfdef qc(v1.42 P3): PM consolidated revalidation Approve (qc1+qc2+qc3 all Approve)

git log 11f8079a..HEAD --oneline
# c1dcfdef qc(v1.42 P3): PM consolidated revalidation Approve (qc1+qc2+qc3 all Approve)
# fc5bff7d qc(v1.42 P3): 4 blocking residuals archived after targeted re-review Approve
# 501f1758 merge(v1.42 P3 qc3 re-review): bring targeted re-review (W-01/W-02 verified) onto integration
# 7a3e3737 merge(v1.42 P3 qc2 re-review): bring targeted re-review (W-01/W-02 verified) onto integration
# 615f1e93 qc(v1.42-p3): revalidate W-01/W-02 fix wave in qc3 report
# 9edb1658 docs(qc): qc2 revalidation for 2026-06-11-v1.42-agent-tool-production-wiring (W-01/W-02 fixes verified; Approve)
# 8cda43c9 merge(v1.42 P3 fix-wave): production path wiring + hot-path + test strengthening
# aa0574cc fix(v1.42 P3): QC fix wave — wire production path + hot-path + test strengthening
# b122db77 qc(v1.42 P3): PM consolidated Request Changes + 4 QC findings as residuals
# 7887c837 merge(v1.42 P3): DF-47 one E2E agent tool upcall (partial — production path gap)
# 01c9f4c8 qc(qc3): V1.42 P3 DF-47 agent tool production wiring — performance/reliability review
# 238fb1e6 review(qc1): approve V1.42 P3 agent tool production wiring (DF-47)
# 146eae00 qc(qc-specialist-2): V1.42 P3 DF-47 production wiring review (security/correctness)
# 4798ff64 docs(spec,tracker): T5 update DF-47 disposition and spec §7.4 production wiring (DF-47 P3)
# c8a0f840 test(daemon-runtime): T4 hermetic E2E test for agent tool production wiring (DF-47 P3)
# eb56d7a2 feat(daemon-runtime): T3 wire daemon tool dispatch into WorkspaceState and boot (DF-47 P3)
# b6e33d2a feat(orchestration,daemon-runtime): T1+T2 host tool dispatch trait and HostToolCallTask (DF-47 P3)

git diff 11f8079a..HEAD --stat
# 21 files changed, 1444 insertions(+), 27 deletions(-)
```

Note: PM assignment expected 13 commits, but the executable range contains 17 commits, including the 4 implementation commits listed above in addition to QC/fix/consolidation commits. The diff/stat scope still matches the assigned `11f8079a..HEAD` verification range.

### Required test, lint, and format commands

```bash
cargo test -p nexus-daemon-runtime -p nexus-agent-host -- agent_tool 2>&1 | tail -40
# exit 0; tail ended with nexus_agent_host/nexus_daemon_runtime doc-tests all ok after filtered package/test binaries.

cargo test -p nexus-daemon-runtime --test agent_tool_production_wiring 2>&1 | tail -40
# running 6 tests
# test agent_tool_e2e_stub_mode_without_adapter ... ok
# test agent_tool_e2e_cross_creator_forbidden_via_adapter ... ok
# test agent_tool_e2e_read_only_tool_succeeds_under_completion_lock ... ok
# test agent_tool_e2e_schedule_dispatch_matches_execute ... ok
# test agent_tool_e2e_schedule_status_through_adapter ... ok
# test agent_tool_e2e_host_tool_call_task_round_trip ... ok
# test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.34s

cargo test -p nexus-daemon-runtime --test agent_tool_api 2>&1 | tail -40
# running 26 tests
# ... all 26 listed tests ok ...
# test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.68s
# Note: emitted one pre-existing unused import warning for `HostToolCallerKind` in this test file; clippy gate below is clean.

cargo test -p nexus-orchestration --lib gonogo judge_next judge_llm 2>&1 | tail -20
# error: unexpected argument 'judge_next' found
# Usage: cargo test [OPTIONS] [TESTNAME] [-- [ARGS]...]

# Intent-preserving split runs:
cargo test -p nexus-orchestration --lib gonogo 2>&1 | tail -20
# test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 549 filtered out; finished in 0.01s

cargo test -p nexus-orchestration --lib judge_next 2>&1 | tail -20
# test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 550 filtered out; finished in 0.00s

cargo test -p nexus-orchestration --lib judge_llm 2>&1 | tail -20
# test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 541 filtered out; finished in 0.00s

cargo clippy -p nexus-daemon-runtime -p nexus-orchestration -p nexus-agent-host -- -D warnings 2>&1 | tail -40
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.25s

cargo +nightly fmt --all --check 2>&1 | tail -20
# no output
```

### Spec, tracker, archive, and residual evidence

```bash
grep -A 5 'production' .mstar/knowledge/specs/agent-nexus-tool-bridge.md | head -15
# **Status: NARROWED** — production caller wiring shipped in V1.42 P3.
# V1.42 P3 completes the production caller: `DaemonToolDispatchAdapter` bridges the orchestration engine's `HostToolCallTask` to `HostToolExecutor::dispatch_for_schedule` ...
# ### 7.4 Production caller wiring (V1.42 P3)

grep -A 5 'DF-47' .mstar/knowledge/deferred-features-cross-version-tracker.md | head -20
# DF-47 row shows **V1.42 P3 Narrowed** and **V1.42 P3 shipped** with `DaemonToolDispatchAdapter` + `HostToolCallTask` + `nexus.orchestration.schedule_status` proven E2E.

cat .mstar/archived/residuals/2026-06-11-v1.42-agent-tool-production-wiring.json | jq '.entries | length'
# 4

python3 - <<'PY'
import json
from pathlib import Path
s=json.loads(Path('.mstar/status.json').read_text())
pid='2026-06-11-v1.42-agent-tool-production-wiring'
print(len(s.get('residual_findings',{}).get(pid, [])))
PY
# 0
```

## Source Review Summary

- `EnterAction::HostTool` is deserializable as `kind: host_tool`, with `tool_name` and optional `args`, enabling production preset manifests to request daemon-side tool dispatch.
- `StateCompositeTask` handles `EnterAction::HostTool` by requiring injected `daemon_tool_dispatch`, constructing `HostToolCallTask::from_dispatch`, and running it; absence of the dispatch fails closed with `TaskExecutionFailed`.
- `HostToolCallTask::run` fast-paths non-template args, generates a request id, calls `DaemonToolDispatch`, stores the result in graph-flow context, and propagates dispatch errors.
- `DaemonToolDispatchAdapter` maps orchestration calls to `HostToolExecutor::dispatch_for_schedule`; FORBIDDEN is preserved as `CapabilityError::Forbidden` for the strengthened cross-creator assertion.
- Daemon boot wires the adapter into `WorkspaceState` and `GraphFlowEngine` before schedule execution.
- Fix-wave artifacts close all four blocking QC residuals; archived residual entries count is 4; open residual count for this plan is 0.

## Summary

| Severity | Count |
| --- | ---: |
| Critical | 0 |
| Warning | 0 |
| Suggestion | 0 |

**Verdict**: Approve

P3 satisfies AC1–AC3 for the minimal DF-47 slice. The production path is wired, the read-only schedule tool path is authz/lock-tested, the hermetic tests pass, and scoped clippy/nightly fmt gates pass. Optional status transition to `Done` was not performed in this QA commit; PM may close the plan row after consuming this report.
