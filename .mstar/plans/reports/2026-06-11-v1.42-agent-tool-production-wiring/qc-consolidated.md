---
reportkind: qc-consolidated
consolidated_by: project-manager
plan_id: "2026-06-11-v1.42-agent-tool-production-wiring"
verdict: "Approve"
generated_at: "2026-06-11"
---

# QC Consolidated Decision — V1.42 P3 Agent Tool Production Wiring (DF-47)

## Scope
- plan_id: `2026-06-11-v1.42-agent-tool-production-wiring`
- Review range / Diff basis: `merge-base: 11f8079a` (P3 status commit) + `tip: HEAD` of `feature/v1.42-agent-tool-wiring` (`01c9f4c8`) — équivalent to `git diff 11f8079a...HEAD` on `.worktrees/v1.42-p3`. Covers 7 commits: 4 implementation + 3 QC.
- Working branch: `feature/v1.42-agent-tool-wiring` → merged to `iteration/v1.42` at `7887c837`
- Review cwd: `.worktrees/v1.42-p3` (detached HEAD; QC work was done on the topic branch)
- All 3 reviewers' individual scope lines copy-pasted the same `plan_id` and `Review range / Diff basis` — alignment verified character-level.

## Reviewer Matrix

| Reviewer | Index | Focus | Verdict | Commit | Critical | Warning | Suggestion |
|----------|-------|-------|---------|--------|----------|---------|------------|
| @qc-specialist | 1 | Architecture coherence and maintainability risk | **Approve** | `238fb1e6` | 0 | 0 | 4 (S-001..S-004) |
| @qc-specialist-2 | 2 | Security and correctness risk | **Approve** | `146eae00` | 0 | 2 (W-01, W-02) | 2 |
| @qc-specialist-3 | 3 | Performance and reliability risk | **Request Changes** | `01c9f4c8` | 0 | **2 (W-01, W-02)** | (see report) |
| **Totals** | — | — | **2 Approve, 1 Request Changes** | — | **0** | **4** | **6+** |

Per `mstar-review-qc` rule: "存在未解决的 `Critical` 或 `Warning` → `Request Changes`". qc3 raised 2 Warnings; per the strict rule, the consolidated verdict must be **Request Changes**. qc3 W-01 is the most consequential (production path is dead code; AC1 not met).

## Blocking Items (must fix before Approve)

| F# | Source | Severity | Title |
|----|--------|----------|-------|
| W-01 | qc3 | Warning | **Production path incomplete — `HostToolCallTask` is dead code in production.** The task, adapter, and boot wiring are built, but no production code actually instantiates or invokes `HostToolCallTask` from a schedule tick. AC1 ("One tool callable from a running schedule without manual CLI invocation") is NOT met. |
| W-02 | qc3 | Warning | Hot-path overhead — `HostToolCallTask::run` serializes full Context to JSON + runs handlebars on every invocation. Non-blocking per se, but worth tightening. |
| W-01 | qc2 | Warning | Cross-creator FORBIDDEN test assertion is weak — only checks error string, not the `NexusApiError::FORBIDDEN` code. Test strengthening required. |
| W-02 | qc2 | Warning | No explicit completion-lock demonstration in the new E2E suite — no test sets `completion_locked_at` and verifies the read-only tool still works. |

## Non-Blocking (track as residuals; defer)

| F# | Source | Severity | Title |
|----|--------|----------|-------|
| S-001 | qc1 | nit (Suggestion) | `DaemonDispatchSlot` type alias — triple-wrapping (Arc → Mutex → Option → Arc) adds conceptual weight |
| S-002 | qc1 | nit (Suggestion) | `CapabilityRuntimeDeps.daemon_tool_dispatch` field is added but not consumed in `with_runtime_deps` — dead field |
| S-003 | qc1 | nit (Suggestion) | `WorkspaceState` clone in boot.rs — snapshot semantics should be documented |
| S-004 | qc1 | nit (Suggestion) | Pre-existing unused import `HostToolCallerKind` in `agent_tool_api.rs:27` |
| (qc2) | qc2 | nit (Suggestion) | Adapter holds a full `WorkspaceState` clone |
| (qc2) | qc2 | nit (Suggestion) | Error code information is lost at the adapter boundary |

## Process Gap (Documented, Risk-Accepted)

- **R-V142P0-PROC** (severity: high, decision: risk-accepted, owner: @project-manager): Cursor (`Auto.Wood`) direct-committed during P0 closeout + migration. Same pattern applied to P1 + P2 + P3. For P3, the Cursor agent did the full implement + QC on the topic branch; my dispatched `fullstack-dev` was cancelled (likely due to work overlap with the Cursor agent's parallel work, or possibly due to a recursion attempt that the system caught). User has accepted this and PM continues.
- **qc3 W-01 production path gap** is the real blocker for AC1.

## Consolidated Decision

**Decision**: **Request Changes** (per `mstar-review-qc` rule; qc3 unresolved Warnings)

**Blocking Items**: qc3 W-01 (production path wiring) + qc3 W-02 (hot-path overhead) + qc2 W-01 (test strengthening) + qc2 W-02 (completion-lock test)

**Residual Findings** (new for P3; open list, severity enum canonical):
- R-V142P3-QC3-W-01 (medium, defer) — production path incomplete (HostToolCallTask is dead code)
- R-V142P3-QC3-W-02 (low, defer) — hot-path overhead (full Context serialization + handlebars on every invocation)
- R-V142P3-QC2-W-01 (medium, defer) — cross-creator FORBIDDEN test assertion is weak
- R-V142P3-QC2-W-02 (medium, defer) — no explicit completion-lock demonstration in E2E suite
- R-V142P3-QC1-S-001..S-004 (nit, defer) — 4 qc1 suggestions
- 2 qc2 suggestions (nit, defer)

**Assigned Fix Owners**:
- R-V142P3-QC3-W-01, W-02: @fullstack-dev (fix wave, blocking)
- R-V142P3-QC2-W-01, W-02: @fullstack-dev (fix wave, blocking)
- *-S-*: @fullstack-dev (P-last or future)

**Next Step**: **Fix wave dispatch to @fullstack-dev** (N=1) to address W-01 (production path wiring) + W-02 (hot-path) + qc2 W-01 (test strengthening) + qc2 W-02 (completion-lock test). After fix: **targeted QC re-review of qc2 + qc3** (N=2, same turn); qc1's Approve stands. Then **QA verification** (N=1). Then PM closure.
