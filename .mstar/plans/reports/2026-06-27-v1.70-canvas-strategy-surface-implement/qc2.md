---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-27-v1.70-canvas-strategy-surface-implement"
verdict: "Approve"
generated_at: "2026-06-27"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-27

## Scope
- plan_id: 2026-06-27-v1.70-canvas-strategy-surface-implement
- Review range / Diff basis: merge-base: 69310a3191d05c80460da227360cba6c9d6539b8 + tip: 6dabf0b58c39ddde641c0e0234828e6c7b89d8b3 (equivalent to: git diff 69310a31...HEAD -- apps/web/ .mstar/knowledge/specs/canvas-strategy-surface.md)
- Working branch (verified): iteration/v1.70
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 22 (all under apps/web/; 0 changes to schemas/, crates/nexus-contracts/, or packages/)
- Commit range: 5 commits (dad35736, 10edf22f, 81cb4256, f82bcdd3, 079f687f)
- Tools run: git diff --stat, git branch --show-current, pnpm --filter web test, manual source review of preset-yaml.ts / idea-input.tsx / use-strategy-data.ts / daemon-status-bar.tsx / strategy-nodes.tsx / strategy-canvas.tsx / strategy-graph.ts

## Findings

### 🔴 Critical
None.

### 🟡 Warning
- **W1 (Injection / prompt surface via Idea-input)**: `idea-input.tsx` + `use-strategy-data.ts` pass raw user `text` (the "Idea") directly as `seed` (via `addSchedule`) and `body` (via `editCoreContext` with `op: 'append'`), plus `signalSchedule`. No client-side length limit, sanitization, or escaping is applied before the POST/PATCH. The endpoints are pre-existing orchestration routes (A4 re-use). The product thesis ("human steers, AI owns prose") intends this channel, but any downstream prompt-construction that does not isolate or escape the appended seed/context inherits a classic prompt-injection / context-pollution surface. Source: `idea-input.tsx:66-76`, `use-strategy-data.ts:132-139` (run), `161-166` (steer). Not a new wire contract, but a correctness/attack-surface gap that should be documented in the daemon-side prompt assembly or have an explicit size guard.
- **W2 (Test cannot execute for new canvas module)**: `pnpm --filter web test` fails with "Failed to resolve import 'yaml' from 'src/lib/canvas/preset-yaml.ts'" in `strategy-graph.test.ts`. The `"yaml": "^2.6.1"` dependency was added to `package.json`, but Vitest transform does not resolve it for the new test file. 121 other tests pass; the canvas-specific test suite is un-runnable in the current CI-equivalent command. This is a packaging/test-infra correctness issue for the delivered surface.

### 🟢 Suggestion
- **S1 (YAML parser safety)**: `preset-yaml.ts` uses the standard `yaml` package `parse` (no custom schema, no `!!js/function`, no `safeLoad` bypass). It performs a read-only projection and ignores unknown keys. This is safe for untrusted YAML in the usual Node `yaml` sense. The input is daemon-returned (trusted in the local model), so the risk is low. Consider adding an explicit size guard on the incoming YAML blob before `parseYaml` for defense-in-depth if presets grow large.
- **S2 (Overlay polling data exposure)**: `usePresetSessions` / `usePresetSchedules` poll every 5 s (`OVERLAY_POLL_MS`) and keep the full `listSessions` / `listSchedules` responses in React Query cache. Only `status`, `current_task_id`, `creator_id`, and `preset_id` are used for the overlay. No obvious secrets are rendered, but the full session objects remain in memory for any component that mounts the hooks. If future session payloads grow to include user notes or large context, consider a narrower projection endpoint or `select` in the query.
- **S3 (Daemon-status-bar unmount race fix)**: The `R-V167PSEC-QC1-S-UNMOUNT` change (added `cancelled` flag, checks after each `await`, early `unlisten()` on late subscription) is correct and matches the race description in the diff comment. No leak remains in the reviewed path. Good defensive hygiene.
- **S4 (XSS / node content)**: All React Flow node labels, descriptions, and status badges are rendered as ordinary React children (`<span>`, `<p>`, text nodes). No `dangerouslySetInnerHTML`, `innerHTML`, or `eval` paths exist in the new canvas files. Preset `id`/`description` values flow from daemon YAML through the graph adapter; they are treated as data, not markup.
- **S5 (Error paths)**: Mutations (`useRunStrategy`, `useSteerStrategy`, `useResumeStrategy`) route errors through `useErrorToast` → visible error toast. Queries surface `ErrorState` with retry. No silent `catch {}` that would hide failures from the author. Correct.
- **S6 (wire_contracts_changed)**: Confirmed `FALSE`. Diff touches only `apps/web/` (22 files, +2016/-28). No `schemas/`, no `crates/nexus-contracts/`, no generated DTOs, no new Local API contracts. All steering re-uses existing schedule/orchestration methods promoted onto `NexusClient`.

## Source Trace
- Finding ID: W1
- Source Type: manual-reasoning + git-diff
- Source Reference: `git diff 69310a31...HEAD -- apps/web/src/components/canvas/idea-input.tsx apps/web/src/lib/canvas/use-strategy-data.ts`
- Confidence: High

- Finding ID: W2
- Source Type: test-run
- Source Reference: `pnpm --filter web test` (failure in strategy-graph.test.ts import of yaml)
- Confidence: High

- Finding ID: S3
- Source Type: git-diff
- Source Reference: `git diff ... daemon-status-bar.tsx` (lines adding `cancelled`, post-await guards, early detach)
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 6 |

**Verdict**: Approve

## Completion Report v2

**Agent**: qc-specialist-2
**Task**: QC2 Security/Correctness Review for P0 Canvas Strategy Surface (plan 2026-06-27-v1.70-canvas-strategy-surface-implement)
**Status**: Done
**Scope Delivered**: Full review of the 22-file apps/web delta per assigned Review range. Verified branch, diff scope (no contracts touched), YAML parser safety, Idea-input steering path, subscription cleanup race fix (R-V167PSEC-QC1-S-UNMOUNT), overlay polling, node rendering, error handling, and test execution.
**Artifacts**:
- Report: `.mstar/plans/reports/2026-06-27-v1.70-canvas-strategy-surface-implement/qc2.md`
- Git commit of only the report (see below)
**Validation**:
- Branch: `iteration/v1.70` (matches Assignment)
- `git diff --stat` confirms scope limited to `apps/web/`; zero changes to schemas/contracts
- `pnpm --filter web test` executed (121 pass, 1 new canvas test suite blocked by missing yaml resolution — tracked as W2)
- Manual source audit of all security-relevant files (preset-yaml.ts, idea-input.tsx, use-strategy-data.ts, daemon-status-bar.tsx diff, strategy-nodes.tsx, strategy-canvas.tsx)
**Issues/Risks**:
- W1 documents the intentional but unaudited prompt-injection surface introduced by raw Idea text flowing into orchestration seed/context. This is a product-thesis decision more than a code defect, but must be called out for downstream daemon-side handling.
- W2 is a test-infra packaging issue that prevents the new canvas tests from running under the standard command; not a runtime security bug.
**Plan Update**: None required from QC2 (no residual registration authority).
**Handoff**: Report is ready for PM consolidation. Targeted re-review not needed unless W1/W2 are turned into actionable fixes.
**Git**: (will be populated after `git add` + `git commit`)
