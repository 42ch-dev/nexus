---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-07-08-v1.98-design-studio-and-design-unification"
verdict: "Approve with residuals"
generated_at: "2026-07-08"
---

# Code Review Report — QC3 (Performance & Reliability)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: deepseek-v4-flash
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-07-08T13:30:00Z

## Scope
- plan_id: `2026-07-08-v1.98-design-studio-and-design-unification`
- Review range / Diff basis: `merge-base: 908de272 (iteration/v1.98 fork point) … tip: c35c3200 (HEAD)`. Equivalent to `git diff 908de272...c35c3200`.
- Working branch (verified): `feature/v1.98-design-studio-and-design-unification`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 44 (5113 insertions, 2639 deletions)
- Commit range: `55dd06cc..c35c3200` (7 commits)
- Tools run: `git diff`, `git log`, `git ls-files`, `pnpm --filter web run build`, `pnpm --filter design-studio build`, `pnpm --filter web exec vitest run`, `pnpm --filter design-studio exec vitest run`, `rg`, `read`
- Deep review: triggered (S1: 5113 lines / 44 files, S3: new domain design-studio, S6: ≥3 modules — apps/web, apps/design-studio, tooling/design-tokens, DESIGN.md pair)
- Lenses applied: Performance Lens, Reliability Lens, Testing Lens

## Findings

### 🟡 Warning

#### [W-001] requestAnimationFrame cleanup missing in ColorSwatch and ElevationCard

**Description:** Both `ColorSwatch` (tokens.tsx:235) and `ElevationCard` (tokens.tsx:311) call `requestAnimationFrame` inside `useEffect` without storing or cancelling the rAF id. If the component unmounts before the callback fires (e.g., rapid navigation, React 18 Strict Mode double-mount), `setComputed` will be called on an unmounted component. In React 18 this produces a no-op warning in development, but it is a reliability anti-pattern.

**Trigger condition:** Navigate away from `/tokens` while a theme toggle is in progress — the rAF callback fires after unmount.

**Impact:** Low. React 18 suppresses state updates on unmounted components. No crash, no data corruption. However, the pattern is fragile and could mask real issues during debugging.

**Fix suggestion:** Store the rAF id and cancel it in the cleanup function:

```tsx
useEffect(() => {
  if (typeof window !== 'undefined') {
    const id = requestAnimationFrame(() => setComputed(resolveSwatchColor(token.varName)));
    return () => cancelAnimationFrame(id);
  }
}, [resolvedTheme, token.varName]);
```

Apply the same fix to `ElevationCard` (line 311).

**Source Type:** deep-lens: Reliability Lens
**Source Reference:** `apps/design-studio/src/pages/tokens.tsx` lines 232-238, 309-314
**Confidence:** Medium

### 🟢 Suggestion

#### [S-001] resolveSwatchColor creates DOM nodes during render

**Description:** `resolveSwatchColor()` (tokens.tsx:201-208) creates a temporary `<div>`, appends it to `document.body`, reads `getComputedStyle`, then removes it. This function is called in the `useState` initializer (line 230), which runs during render. Calling `document.body.appendChild` during render is a side-effect in a pure computation context.

**Trigger condition:** Initial render of the Tokens page. In jsdom (test environment), `getComputedStyle` returns empty strings so the DOM manipulation is wasted work.

**Impact:** Low. Works correctly in browser. In jsdom, the DOM manipulation is harmless but unnecessary.

**Fix suggestion:** Replace with `readCSSVar()` (already defined at line 9-12) which uses `getComputedStyle(document.documentElement)` without creating temporary DOM nodes. Alternatively, move the computation entirely into `useEffect`:

```tsx
const [computed, setComputed] = useState<string>('');
useEffect(() => {
  requestAnimationFrame(() => setComputed(resolveSwatchColor(token.varName)));
}, [resolvedTheme, token.varName]);
```

**Source Type:** deep-lens: Performance Lens
**Source Reference:** `apps/design-studio/src/pages/tokens.tsx` lines 201-209, 230
**Confidence:** Low

#### [S-002] Theme toggle discards 'system' preference on first click

**Description:** The `toggleTheme` function (theme-provider.tsx:64-69) always switches to an explicit `light` or `dark` value, discarding the `system` preference. A user who has `prefers-color-scheme: dark` at night and `light` during the day will lose automatic switching after one manual toggle. This is documented as intentional in the `ThemeToggle` component comment ("system mode is set on first paint only and is replaced by explicit choice on toggle"), but it degrades the UX for users who rely on OS-level auto-switching.

**Trigger condition:** User with `system` mode clicks the theme toggle once.

**Impact:** Low. The toggle still works correctly for light/dark switching. The `system` mode is only used on first paint.

**Fix suggestion:** Consider cycling `light → dark → system → light` instead of `light ↔ dark`. This would preserve the `system` preference for users who want it. However, this is a product decision, not a correctness fix.

**Source Type:** manual-reasoning
**Source Reference:** `apps/design-studio/src/components/theme-provider.tsx` lines 64-69
**Confidence:** Low

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|---|---|---|---|
| W-001 | deep-lens: Reliability Lens | tokens.tsx:232-238, 309-314 | Medium |
| S-001 | deep-lens: Performance Lens | tokens.tsx:201-209, 230 | Low |
| S-002 | manual-reasoning | theme-provider.tsx:64-69 | Low |

## Summary

| Severity | Count |
|---|---|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 2 |

## Verdict: Approve with residuals

**Rationale:** No critical or blocking issues found. The single Warning (W-001) is low-impact and has a straightforward fix (add rAF cleanup). The two Suggestions are minor quality improvements.

### Verified OK (no findings)

| Check | Result |
|---|---|
| **apps/web build** | ✅ `pnpm --filter web run build` passes (2516 modules, 3.11s) |
| **apps/web tests** | ✅ 75 files, 548 tests, all pass |
| **design-studio build** | ✅ `pnpm --filter design-studio build` passes (1660 modules, 1.21s) |
| **design-studio tests** | ✅ 1 file, 11 tests, all pass |
| **DESIGN merge completeness** | ✅ `git ls-files '**/DESIGN*.md'` returns only root pair; all consumers updated |
| **Token pipeline** | ✅ `@nexus/design-tokens` builds/exports correctly; both apps consume shared preset |
| **Workspace registration** | ✅ `pnpm-workspace.yaml` globs `apps/*` and `tooling/*` cover both packages |
| **Import alias reliability** | ✅ `@/lib/utils` resolves correctly (Vite config orders it before `@`); no `src/lib/` in studio |
| **Theme reactivity** | ✅ ColorSwatch/ElevationCard re-read CSS vars on theme change; no orphan hardcoded colors |
| **No circular dependency** | ✅ `@nexus/design-tokens` depends on `@42ch/nexus-ui` only; no circular workspace deps |
| **Process Done** | ✅ 7 commits present; all SDD tasks (T1-T7) have commit evidence |
| **No daemon dependency** | ✅ Studio runs standalone on port 5174 |

### Residuals to register

1. **W-001**: `requestAnimationFrame` cleanup missing in `ColorSwatch` and `ElevationCard` — fix in follow-up or next iteration.

---

## Completion Report v2

**Agent**: qc-specialist-3
**Task**: Plan-level QC tri-review (L3), seat 3 of 3 — Performance and reliability risk
**Status**: Done
**Scope Delivered**: Full review of 44 files across 7 commits; build/test verification for both apps; deep review (Performance Lens + Reliability Lens + Testing Lens)
**Artifacts**: `.mstar/plans/reports/2026-07-08-v1.98-design-studio-and-design-unification/qc3.md`
**Validation**: `pnpm --filter web run build` ✅, `pnpm --filter design-studio build` ✅, `pnpm --filter web exec vitest run` (548 tests) ✅, `pnpm --filter design-studio exec vitest run` (11 tests) ✅
**Issues/Risks**: 1 Warning (rAF cleanup), 2 Suggestions (DOM during render, theme toggle system mode)
**Plan Update**: N/A — QC report only
**Handoff**: PM to register W-001 as residual; fix is small (add `cancelAnimationFrame` to useEffect cleanup)
**Git**: Report committed to `feature/v1.98-design-studio-and-design-unification` at HEAD `c35c3200`
