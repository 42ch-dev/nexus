---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-26-v1.67-frontend-scope-gaps"
verdict: "Approve"
generated_at: "2026-06-27"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: volcengine-plan/ark-code-latest
- Review Perspective: Performance and reliability (React Query cache behavior, selector churn, client-method error propagation)
- Report Timestamp: 2026-06-27

## Scope
- plan_id: 2026-06-26-v1.67-frontend-scope-gaps
- Review range / Diff basis: P1 commits `e74321db`+`963fa1ed`+`aeaaf91a` merged at HEAD; diff basis vs `26e477ee`
- Working branch (verified): iteration/v1.67
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 12 (`apps/web/AGENTS.md`, `apps/web/DESIGN.md`, `apps/web/src/lib/nexus/browser-client.ts`, `browser-client.test.ts`, `types.ts`, `tauri-client.ts`, `query-keys.ts`, `adapter-contract.test.ts`, `apps/web/src/api/queries.ts`, `apps/web/src/main.tsx`, `apps/web/src/pages/dialogs/create-work-dialog.tsx`, `create-work-dialog.test.tsx`)
- Commit range: e74321db, 963fa1ed, aeaaf91a (P1 changes only; merged via fedf82e4 into HEAD 9ef8251a)
- Tools run: `git rev-parse --show-toplevel`, `git branch --show-current`, `git rev-parse HEAD`, `git status --short`, `git log --oneline -10`, `git diff --stat/name-only 26e477ee...HEAD`, `git diff 26e477ee...HEAD -- apps/web/...`, `git show --name-only e74321db 963fa1ed aeaaf91a`, `git diff --check 26e477ee...HEAD -- apps/web`, `pnpm --filter @42ch/nexus-contracts run build`, `pnpm --filter web typecheck`, `pnpm --filter web test -- src/lib/nexus/browser-client.test.ts src/pages/dialogs/create-work-dialog.test.tsx`, `pnpm --filter web build`, `pnpm --filter web test`, `bash tooling/check-wire-drift.sh`

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- None.

### 🟢 Suggestion
- **QC3-S1 — Refresh adapter-contract coverage wording/count for the 24-method surface.** `apps/web/src/lib/nexus/adapter-contract.test.ts` still says it exercises “all 21 NexusClient methods” and asserts `seen.size === 21`, so it does not currently include the newly promoted `getPreset`/`updatePreset`/`deletePreset` calls in that parity guard. This is non-blocking for P1 because `TauriClient` inherits `BrowserClient` unchanged, TypeScript typecheck passed with the 24-method interface, and `browser-client.test.ts` directly covers the three new BrowserClient methods. Still, updating this guard to call/count all 24 methods would keep the reliability test’s stated invariant true for future regressions.
- **QC3-S2 — Add preset detail/update/delete query keys when V1.68 adds the management UI.** The delivered V1.67 scope intentionally promotes transport only; no React Query hooks were added for the new preset detail/update/delete methods. Therefore no new stale-cache trap exists in this change. When the deferred UI lands, add `queryKeys.presets.detail(id)` plus update/delete mutation invalidation for both `presets.list()` and the affected detail key.

## Source Trace
- Finding ID: QC3-S1
- Source Type: manual review + test review
- Source Reference: `apps/web/src/lib/nexus/adapter-contract.test.ts:126-154`; `apps/web/src/lib/nexus/browser-client.test.ts:233-276`; `apps/web/src/lib/nexus/tauri-client.ts:67-70`
- Confidence: High

- Finding ID: QC3-S2
- Source Type: manual review + React Query cache-key inspection
- Source Reference: `apps/web/src/lib/nexus/query-keys.ts:35-38`; `apps/web/src/api/queries.ts:161-167,213-249`; `apps/web/src/lib/nexus/types.ts:124-129`
- Confidence: High

- Finding ID: QC3-OK-ERROR-HANDLING
- Source Type: manual review + test execution
- Source Reference: `apps/web/src/lib/nexus/browser-client.ts:159-170,248-292`; `apps/web/src/lib/nexus/errors.ts:47-63`; `apps/web/src/lib/nexus/browser-client.test.ts:51-84,233-276`; `apps/web/src/main.tsx:13-29`; validation commands listed in Scope
- Confidence: High

- Finding ID: QC3-OK-SELECTOR-PERF
- Source Type: manual review + UI test execution
- Source Reference: `apps/web/src/pages/dialogs/create-work-dialog.tsx:9-15,41-53,63-75,116-128`; `apps/web/src/pages/dialogs/create-work-dialog.test.tsx:31-79`; validation commands listed in Scope
- Confidence: High

## Reliability / Performance Notes
- **React Query cache keys / invalidation:** No new hooks or cached preset detail/update/delete reads were introduced in V1.67 P1. Existing preset mutations (`useScaffoldPreset`, `useReloadPreset`) continue to invalidate `queryKeys.presets.list()`. The new transport-only client methods cannot leave a React Query cache stale until a UI hook starts using them.
- **Selector render/refetch behavior:** `WORK_PROFILES` is module-level, the native `Select` only updates local component state, and the dialog has no read query. Changing Work profile causes a local re-render only; it does not invalidate queries or trigger network refetches.
- **Client error propagation:** The new methods all route through the shared `request()` core. HTTP failures unwrap the daemon `{ success: false, error: ErrorResponse }` envelope via `NexusClientError.fromBody`; transport failures become a `NexusClientError` with `code: transport_unreachable`, which the query/mutation toast bridges surface. No unhandled “throw to nowhere” path was introduced.
- **Perf regression:** The UI change is a single native select and constant options array; no measurable hot-path cost or bundle-significant dependency was added.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve
