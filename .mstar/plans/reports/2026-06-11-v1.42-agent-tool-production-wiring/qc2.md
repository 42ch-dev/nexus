---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-11-v1.42-agent-tool-production-wiring"
verdict: "Approve"
generated_at: "2026-06-12"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (DF-47 production caller wiring, one E2E schedule-initiated `nexus.*` tool)
- Report Timestamp: 2026-06-11

## Scope
- plan_id: 2026-06-11-v1.42-agent-tool-production-wiring
- Review range / Diff basis: merge-base: 11f8079ae6df8b861ed608ede57ee628f3f3b97e (iteration/v1.42 HEAD) | tip: 4798ff6417ac0ddc80f0886f7f68d931458010aa (feature/v1.42-agent-tool-wiring HEAD) | equivalent: git diff 11f8079a..4798ff64
- Working branch (verified): feature/v1.42-agent-tool-wiring
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-p3
- Files reviewed: 11 (4 implementation commits: b6e33d2a, eb56d7a2, c8a0f840, 4798ff64)
- Commit range: 11f8079a..4798ff64 (exactly matches Assignment)
- Tools run:
  - `cargo +nightly fmt --all -- --check` (clean)
  - `cargo clippy -p nexus-daemon-runtime -p nexus-orchestration -p nexus-agent-host -- -D warnings` (clean on changed crates; 1 pre-existing unused import warning in existing `agent_tool_api.rs` test)
  - `cargo test -p nexus-daemon-runtime --test agent_tool_production_wiring --test agent_tool_api` (31 tests passed: 5 new E2E + 26 existing)

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- **Cross-creator FORBIDDEN test assertion is weak (test hygiene / regression coverage)**: `agent_tool_e2e_cross_creator_forbidden_via_adapter` (in `crates/nexus-daemon-runtime/tests/agent_tool_production_wiring.rs:192`) asserts only that the error string contains `"daemon tool dispatch failed"`. It does not assert the underlying `NexusApiError` code is `FORBIDDEN` (the code actually returned by `execute_schedule_status` via `works::get_work(creator_id, work_id)`). The production path correctly enforces the V1.32 `SEC-V131-01` cross-creator boundary (same handler as HTTP), but the dedicated regression test for DF-47's "narrowed" claim does not surface the specific error code. This is a gap in test strength for the security invariant the plan claims to close.
  - Source: manual diff + test review + `execute_schedule_status` (host_tool_executor.rs:788)
  - Fix: strengthen the assertion to check for the FORBIDDEN code (or at minimum that the inner error from `dispatch_tool` carries the expected code before it is wrapped as `CapabilityError::Internal`).

- **No explicit completion-lock demonstration in the new production-wiring E2E suite**: The plan AC #2 states "read-only tool respects completion-lock; mutating tools follow runtime_lock (P0)". The test file header claims the same. However, none of the 5 new tests in `agent_tool_production_wiring.rs` set `completion_locked_at` on the seeded `WorkRecord` and assert that `nexus.orchestration.schedule_status` (via adapter or `HostToolCallTask`) still succeeds. The existing `agent_tool_api.rs` tests cover lock behavior for other tools, and `schedule_status` is intentionally allowed under completion-lock per spec §4 / §7.4, but the new production path (DF-47 P3) lacks a direct hermetic proof in its own test module.
  - Source: test file review + plan AC #2 + `seed_work` helper (no `completion_locked_at` set) + `execute_schedule_status` (no lock check, correct for read-only)
  - Fix: add one minimal test case (or extend an existing one) that seeds a completion-locked work and confirms the read-only schedule tool still succeeds through the adapter path. This would make the "respects completion-lock" claim directly verifiable from the DF-47 test module.

### 🟢 Suggestion
- **Adapter holds a full `WorkspaceState` clone (minor surface / future-proofing)**: `DaemonToolDispatchAdapter` stores `state: WorkspaceState` (cloned at construction in boot.rs:130). The comment correctly notes that current fields are `Arc`'d, so the clone is cheap and safe for the long-lived daemon state. However, this is a broad structural clone. If future non-`Arc` fields are added to `WorkspaceState`, the adapter could inadvertently hold stale or expensive copies. The trait surface (`dispatch_tool`) is narrow and correct; the storage choice is an implementation detail that could be tightened (e.g., hold only the Arcs it actually needs) in a later hygiene pass.
  - Source: `host_tool_executor.rs:427` (struct), `boot.rs:127` (wiring), `workspace/mod.rs:56` (field)
  - Recommendation: leave as-is for P3 (correct behavior, no risk today); consider a follow-up note in tech-debt or a small refactor when `WorkspaceState` next changes.

- **Error code information is lost at the adapter boundary (observability / downstream graph logic)**: `DaemonToolDispatchAdapter::dispatch_tool` maps every `NexusApiError` to `CapabilityError::Internal(format!("daemon tool dispatch failed for {tool_name}: {e}"))`. This erases the distinction between `FORBIDDEN`, `INVALID_INPUT`, `POLICY_BLOCKED`, etc. The worker upcall path (`dispatch_from_worker`) already surfaces a structured `WorkerToolError { code, message }`. For schedule-initiated calls this may be acceptable (the `HostToolCallTask` just fails the step), but it means graph logic, logging, or future conditional routing cannot differentiate authz failures from other failures without string matching. Not a correctness bug for the read-only tool shipped in P3, but a surface inconsistency.
  - Source: `host_tool_executor.rs:449` (map_err) + `tasks/mod.rs:1544` (TaskExecutionFailed)
  - Recommendation: consider a richer `CapabilityError` variant (or a structured payload) in a future increment if schedule-side tools need to drive conditional behavior based on specific denial reasons.

- **Pre-existing unused import surfaces in clippy run**: `cargo clippy ... --test agent_tool_api` emitted one warning: `unused import: HostToolCallerKind` at `agent_tool_api.rs:27`. This is not introduced by the 4 commits under review (it predates the plan) and does not affect the changed crates' production code. Recorded here for hygiene tracking; can be cleaned in any subsequent edit of that test file.
  - Source: clippy output during QC verification

## Source Trace
- Finding ID: W-01 (cross-creator test assertion)
- Source Type: manual-reasoning + test review
- Source Reference: `crates/nexus-daemon-runtime/tests/agent_tool_production_wiring.rs:208` (assertion) + `host_tool_executor.rs:794` (FORBIDDEN return)
- Confidence: High

- Finding ID: W-02 (completion-lock demonstration gap)
- Source Type: manual-reasoning + plan cross-check
- Source Reference: plan AC #2 + test file header + `agent_tool_production_wiring.rs:36` (seed_work) + spec §7.4
- Confidence: High

- Finding ID: S-01 (WorkspaceState clone in adapter)
- Source Type: manual-reasoning + code review
- Source Reference: `host_tool_executor.rs:427`, `boot.rs:130`, `workspace/mod.rs:56`
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

**Rationale**: No Critical findings. The two Warnings are test-strength / demonstration gaps rather than security or correctness defects in the shipped production path. The core wiring is sound:

- `dispatch_for_schedule` creates a `ToolExecuteRequest` with `caller_kind=Schedule` and delegates to the same `HostToolExecutor::execute` used by HTTP (single dispatch table invariant per spec §7.1).
- `DaemonToolDispatchAdapter` is wired at boot before any `GraphFlowEngine` or schedule tick can run; the field is behind `Arc<Option<...>>` with a one-time `set_` at startup.
- `HostToolCallTask` performs template rendering (fail-closed via shared `render_value_templates`) and propagates `dispatch_tool` errors as `TaskExecutionFailed` (no silent swallow).
- The 5 new hermetic E2E tests cover round-trip, stub mode, `HostToolCallTask` context integration, cross-creator rejection (behaviorally), and Schedule-vs-HTTP result equivalence.
- Cross-creator isolation is enforced in the handler (`execute_schedule_status` calls `works::get_work(creator_id, work_id)` and returns FORBIDDEN); the adapter does not bypass it.
- The chosen tool (`nexus.orchestration.schedule_status`) is read-only; per spec it is allowed under `completion_lock` while mutating tools would still require `runtime_lock` (the trait surface does not weaken the existing authz gates in `execute`).

The test gaps (W-01, W-02) are real and should be addressed in a follow-up hygiene increment or the next time the test module is touched, but they do not block the narrow DF-47 P3 scope (one read-only tool, production caller wiring proven E2E). Lint and the required test binaries are clean/green. Report committed on the review branch per assignment.

## Revalidation

**Targeted re-review (qc2 lane only)**: Fix-wave delta addressing the two Warnings originally raised by this reviewer (W-01 weak FORBIDDEN assertion; W-02 no completion-lock test in the DF-47 E2E suite). Review range for revalidation: `merge-base: b122db77` (PM consolidated pre-fix) + `tip: HEAD` (`8cda43c9`) on the QC worktree `.worktrees/v1.42-p3-reqc` (detached). Equivalent to `git diff b122db77...HEAD`. The delta consists of exactly two commits:
- `aa0574cc` fix(v1.42 P3): QC fix wave — wire production path + hot-path + test strengthening
- `8cda43c9` merge(v1.42 P3 fix-wave): PM merge of fix-wave

qc1's original Approve stands; qc3 re-reviews in parallel (different lane). No code changes by this reviewer; only report update + evidence capture.

### W-01 (cross-creator FORBIDDEN test assertion is weak) — Resolved
- **Initial finding**: The test `agent_tool_e2e_cross_creator_forbidden_via_adapter` only asserted that the error string contained `"daemon tool dispatch failed"`. It did not assert that the underlying error was the `FORBIDDEN` code returned by the handler via `works::get_work`.
- **Fix evidence** (from `aa0574cc`):
  - Added `CapabilityError::Forbidden` variant in the orchestration capability layer.
  - `DaemonToolDispatchAdapter::dispatch_tool` (host_tool_executor.rs) now pattern-matches on `NexusApiError::Forbidden` and emits `CapabilityError::Forbidden(...)` (preserving the error code); all other errors remain `Internal`.
  - Strengthened the test assertion to match the variant directly:
    ```rust
    match &err {
        nexus_orchestration::capability::CapabilityError::Forbidden(msg) => {
            assert!(msg.contains("daemon tool dispatch failed"), ...);
        }
        other => panic!("expected CapabilityError::Forbidden, got: {:?}", other),
    }
    ```
- **Commands / artifacts**:
  - `git show aa0574cc -- crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs | head -100` (shows the match arm for Forbidden vs fallback Internal).
  - `cargo test -p nexus-daemon-runtime --test agent_tool_production_wiring` now runs 6 tests (the cross-creator test is still present and now variant-asserting).
- **Disposition**: Closed for this wave. The security invariant (cross-creator boundary returns a distinguishable FORBIDDEN code through the schedule/adapter path) is now directly asserted in the regression test.

### W-02 (no explicit completion-lock demonstration in the new production-wiring E2E suite) — Resolved
- **Initial finding**: Plan AC #2 and the test header claimed "read-only tool respects completion-lock", but none of the 5 new E2E tests seeded a `WorkRecord` with `completion_locked_at` and verified that the read-only `nexus.orchestration.schedule_status` still succeeds through the adapter / `HostToolCallTask` path.
- **Fix evidence** (from `aa0574cc`):
  - New helper: `seed_work_completion_locked(state)` — seeds a work then uses `works::patch_work` to set `completion_locked_at` (the INSERT path hardcodes NULL, so patch is required).
  - New test: `agent_tool_e2e_read_only_tool_succeeds_under_completion_lock`:
    - Seeds a completion-locked work.
    - Verifies `record.completion_locked_at.is_some()`.
    - Dispatches `nexus.orchestration.schedule_status` via `DaemonToolDispatchAdapter`.
    - Asserts success and correct output (`work_id`, `count: 1`) despite the lock.
  - This directly exercises the production adapter path for a read-only schedule tool under the lock condition required by spec §4 / §7.4.
- **Commands / artifacts**:
  - `git show aa0574cc -- crates/nexus-daemon-runtime/tests/agent_tool_production_wiring.rs | head -120` (shows the new helper + full new test).
  - `cargo test -p nexus-daemon-runtime --test agent_tool_production_wiring` (6 passed, including the new lock test; the prior 5 + this one).
- **Disposition**: Closed for this wave. The "respects completion-lock for read-only tools" claim is now hermetically proven from the DF-47 production-wiring test module itself.

### Suggestions remain non-blocking (defer)
The two Suggestions noted in the initial wave (S-01: broad `WorkspaceState` clone held by the adapter; S-02: error code information lost at adapter boundary for future graph conditional logic) were explicitly non-blocking and outside the fix-wave scope (T8/T9 targeted only the two Warnings). They are tracked as residuals (see `status.json` and consolidated report) and deferred to a later hygiene increment or the next touch of the relevant modules. No new Suggestions or findings were raised from the security/correctness review of the fix delta. qc1's four Suggestions and any qc3-lane items are also out of this reviewer's scope.

### Static checks on the revalidation snapshot (detached HEAD @ 8cda43c9)
- `cargo +nightly fmt --all --check` → clean (no output)
- `cargo clippy -p nexus-daemon-runtime -p nexus-orchestration -p nexus-agent-host -- -D warnings` → clean (no warnings emitted for the changed crates)
- `cargo test -p nexus-daemon-runtime --test agent_tool_production_wiring --test agent_tool_api` → 32 total passed (6 + 26); the new completion-lock test is present and green.

**Revalidation verdict**: Approve

**Rationale (revalidation)**: Both Warnings originally raised by qc2 have been addressed with targeted, minimal, testable changes in the fix wave. The production adapter path now surfaces `CapabilityError::Forbidden` distinctly, and the cross-creator regression test asserts the variant. A direct completion-lock test for the read-only schedule tool (via the adapter) has been added and passes. No Critical or new Warning findings under the security/correctness lens on the delta. The two prior Suggestions remain deferred (non-blocking). Lint, fmt, and the required test binaries are clean/green on the post-fix snapshot. This satisfies the acceptance criteria for the targeted re-review of qc2's items.

## Evidence Appendix (QC verification commands) (revalidation run)
```bash
# QC worktree / branch / range alignment (revalidation snapshot)
cd /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-p3-reqc
git rev-parse --show-toplevel         # /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-p3-reqc
git rev-parse --abbrev-ref HEAD       # HEAD (detached)
git log -1 --oneline                  # 8cda43c9 merge(v1.42 P3 fix-wave)...
git log b122db77..HEAD --oneline      # exactly 2 commits (aa0574cc + 8cda43c9)
git diff b122db77..HEAD --stat        # 10 files, +248/-21 (matches fix wave)

# Lint / tests (required evidence for revalidation)
cargo test -p nexus-daemon-runtime --test agent_tool_production_wiring 2>&1 | tail -40
#   6 passed (the new completion-lock test + prior 5)
cargo test -p nexus-daemon-runtime --test agent_tool_api 2>&1 | tail -40
#   26 passed (plus 1 pre-existing unused-import warning in test; not from this delta)
cargo clippy -p nexus-daemon-runtime -p nexus-orchestration -p nexus-agent-host -- -D warnings 2>&1 | tail -40
#   clean (no warnings from changed crates)
cargo +nightly fmt --all --check 2>&1 | tail -20
#   (no output = clean)

# W-01 evidence (FORBIDDEN code preservation + strengthened test)
git show aa0574cc -- crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs | head -100
#   shows match arm: NexusApiError::Forbidden → CapabilityError::Forbidden; fallback Internal

# W-02 evidence (completion-lock test + seed helper)
git show aa0574cc -- crates/nexus-daemon-runtime/tests/agent_tool_production_wiring.rs | head -120
#   shows seed_work_completion_locked + agent_tool_e2e_read_only_tool_succeeds_under_completion_lock

# Report commit (after edit + git add of only this path)
git log -1 --oneline .mstar/plans/reports/2026-06-11-v1.42-agent-tool-production-wiring/qc2.md
```
All commands executed from the Assignment-specified QC worktree (detached HEAD at the post-merge fix-wave tip) using the exact `Review range / Diff basis` (`b122db77..HEAD`). Working tree clean after the report-only commit.

## Prior Evidence Appendix (initial wave, retained for traceability)
(Original commands from the initial qc2 review on the topic branch are preserved above for audit continuity; the revalidation run supersedes the runtime evidence for the fix delta.)
