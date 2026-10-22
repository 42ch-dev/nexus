---
module: packages/nexus-ui + apps/design-studio
date: 2026-10-22
last_updated: 2026-10-22
problem_type: testing_pattern
category: testing-patterns
severity: medium
applies_when:
  - Testing Radix Dialog/Select primitives under jsdom (open/close, dismissal paths, focus management)
  - Writing a source-scanning coverage/assertion guard over TypeScript export syntax
  - Deciding how strictly a static guard must treat unrecognized source shapes
tags:
  - testing
  - radix
  - jsdom
  - nexus-ui

# Radix dialog jsdom testing quirks and fail-closed export guards

## Context

The v1.218 UI teardown shipped dialog-based patterns (ConfirmDialog, FormDialog) and a
source-scanning fixture-coverage guard over `packages/nexus-ui/src/index.ts`. Both surfaced
non-obvious testing constraints that cost review rounds to converge.

## Guidance

### Radix + jsdom dismissal tests

- Radix attaches its document-level `pointerdown` listener **after a tick**: `await new Promise(r => setTimeout(r, 0))` before simulating outside-click, or the event never reaches the handler and the test passes vacuously.
- Outside-click dismissal requires the **paired `pointerdown` + `click`** (Dialog's `deferPointerDownOutside`); firing only `click` does nothing.
- Stacked open dialogs collapse in the a11y tree (`data-state="closed"` on the first) — unmount between renders instead of rendering Dialog and Sheet simultaneously.
- Dismissal paths (Escape, outside click, header Close) all funnel through `onOpenChange`; a pending-guard belongs there (one guard closes all three paths), and tests must include a non-pending positive proving the path actually reaches `onOpenChange` — otherwise the blocked assertions are vacuous.

### Fail-closed source-scanning guards

- Never line-anchor export regexes: `export { Button } from './x'; export function Y() {}` (same-line continuation) and inter-keyword comments (`export /* public */ function`) both bypass `^export\s+function` scans.
- Strip comments by replacing with a **single space**, not an empty string — `export/* c */function` must not collapse into `exportfunction`.
- Accept the bounded guarantee honestly: document the remaining shortcut (a string literal containing `//` or `/*` confuses naive comment stripping) and the escalation condition (switch to the TypeScript compiler API when the index outgrows value re-exports).
- Red-check the guard itself (temporarily remove a registry entry, watch it fail, restore) — a red-check of a *recognized* export does not prove unknown *forms* are caught; add synthetic-input tests for each bypass shape.

## Why This Matters

Each of these produced a full review round (Needs fixes → fix → re-review) before converging;
encoding them removes ~3 review rounds from the next dialog-pattern or coverage-guard task.

## When to Apply

Any `@42ch/nexus-ui` dialog-family work, any Radix-based pattern promotion, and any new
source-scanning guard over TypeScript sources in this repo.

## Examples

- `packages/nexus-ui/src/components/confirm-dialog.test.tsx` (pending-guarded dismissal paths)
- `apps/design-studio/src/fixtures/__tests__/fixture-coverage.test.ts` (`unsupportedExportForms`)
