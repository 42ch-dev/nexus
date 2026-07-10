# Component Promotion Boundary (V1.99 Draft)

**Status**: Locked (V1.99 P-1)  
**Owner**: architect (P-1 Task 1 lock) — product-manager decisions resolved via V1.99 compass Grill-Me  
**Consumers**: V1.99 P0 component promotion plan, `packages/nexus-ui/AGENTS.md`, `apps/design-studio/AGENTS.md`, `apps/web/AGENTS.md`

## Problem

V1.98 deliberately kept `@42ch/nexus-ui` brand-only while Design Studio imported `apps/web/src/components/ui/*` through `@web-ui/*`. That was safe for the first gallery, but it leaves reusable UI primitives owned by an app package and makes Design Studio depend on `apps/web` internals.

V1.99 reopens the boundary for a focused first batch of reusable UI primitives.

## Product Outcome

The package boundary should make future UI work easier without hiding app behavior inside a shared package. A promoted primitive is successful when Design Studio and Web can share its visual contract, while Web still owns data, routing, daemon integration, and product-specific composition.

This is a correction to V1.98's intentionally conservative boundary, not a reversal of the underlying rule: `@42ch/nexus-ui` must remain a reusable presentational package rather than a second app layer.

## Promotion Rules

A component may move to `@42ch/nexus-ui` only if all are true:

- It is pure presentational React and can render without daemon state, routing, `NexusClient`, localStorage, Tauri IPC, or app providers.
- It consumes shared design tokens through classes/CSS variables, not raw one-off values.
- It does not import from `apps/web`, `apps/design-studio`, or app-local aliases.
- It keeps React as peer dependency and remains compatible with the package's tsup/esbuild build.
- It has package-level tests that cover variants and accessibility-relevant class output or behavior.
- Consumers can import it through the public package export map, not internal source paths.

## Package Architecture Rules

These constraints are the implementation contract for V1.99 P0:

- **Package role:** `@42ch/nexus-ui` is a presentational React package. It may own primitive view components and brand assets; it must not become a form framework, app shell, routing layer, or daemon adapter.
- **Export map:** first-batch primitives should be named exports from `@42ch/nexus-ui` via `src/index.ts`. Do not add per-component deep public exports in V1.99 unless P-1/P0 explicitly locks them; named root exports keep the public API small while the package is pre-1.0.
- **Build entries:** keep tsup entries boring: `src/index.ts` and `src/tokens.ts` unless a new documented public subpath is required. Component files stay internal implementation details.
- **Runtime dependencies:** React and React DOM stay peer dependencies. Non-singleton implementation helpers such as `class-variance-authority`, `@radix-ui/react-slot`, `clsx`, and `tailwind-merge` may be package runtime dependencies only when the promoted primitive actually imports them. Do not make those peers unless the consumer must share an instance.
- **Class composition:** package code must own its own `cn` helper or equivalent local composition utility. It must not import `@web-lib/utils` or `apps/web/src/lib/utils.ts`.
- **Token consumption:** component class strings must reference tokens exposed by `@nexus/design-tokens` / root DESIGN, but the component package must not duplicate the token tables or require consumers to import package-owned component CSS beyond the existing brand `theme.css`.
- **Asset rules:** no `.svg`, `.png`, or app asset imports from component source. Asset-using components must keep the existing bundler-agnostic `src` prop or inline JSX pattern.
- **Consumer wrappers:** `apps/web/src/components/ui/*` may temporarily become thin re-export/wrapper files to avoid large call-site churn. Wrappers may add app behavior, labels, or data wiring only in `apps/web`; those additions must not move back into the package.

## Exclusions

Do not promote:

- Product screens, layout shells, sidebar/app chrome, page routes, or data-aware components.
- Components that require `NexusClient`, TanStack Query, React Router, Tauri commands, or daemon URLs.
- Components whose only reuse is a single one-off surface fixture.
- Components that import SVG/PNG assets directly from package source; asset-consuming components must follow the existing bundler-agnostic `src`-prop pattern.

## Locked First-Batch List (V1.99)

Per V1.99 compass Grill-Me lock: **promote `Button`, `Badge`, and `Card`; defer `Input`, `Label`, and `Textarea`.**

### Promoted

| Component | Source | Variant API | Actual Dependencies (verified) | Package-Runtime Deps Needed |
| --- | --- | --- | --- | --- |
| Button | `button.tsx:52` | `cva` with `variant` (`primary`, `secondary`, `tertiary`, `destructive`) + `size` (`small`, `default`, `large`); `asChild` via `@radix-ui/react-slot` | `@radix-ui/react-slot`, `class-variance-authority` (cva, VariantProps), `react` (forwardRef, ButtonHTMLAttributes), `@/lib/utils` (cn → clsx + tailwind-merge) | `@radix-ui/react-slot`, `class-variance-authority`, `clsx`, `tailwind-merge` |
| Badge | `badge.tsx:46` | `cva` with `variant` (`neutral`, `running`, `queued`, `warning`, `error`, `preset`); no `size` axis | `class-variance-authority` (cva, VariantProps), `react` (forwardRef, HTMLAttributes), `@/lib/utils` (cn → clsx + tailwind-merge) | `class-variance-authority`, `clsx`, `tailwind-merge` |
| Card | `card.tsx:12` | No `cva`; exports five related primitives (`Card`, `CardHeader`, `CardTitle`, `CardDescription`, `CardContent`) | `react` (forwardRef, HTMLAttributes), `@/lib/utils` (cn → clsx + tailwind-merge) | `clsx`, `tailwind-merge` |

**Combined runtime deps to add to `packages/nexus-ui/package.json`:** `class-variance-authority`, `@radix-ui/react-slot`, `clsx`, `tailwind-merge`. All are runtime deps (not peers); React and React DOM remain peer deps.

**Reminder for P0:** the package must own its own `cn` helper with the same custom `tailwind-merge` class-group extension as `apps/web/src/lib/utils.ts` (registering DESIGN.md custom font-size tokens). The package must NOT import `@/lib/utils` or any `apps/web` source.

### Deferred (per Grill-Me lock)

| Component | Status | Revisit Trigger |
| --- | --- | --- |
| Input | **defer** | Future Form Field slice; needs helper text, error text, required/optional copy, and label/control association patterns reviewed together before promotion |
| Label | **defer** | Same Form Field slice as Input; label-htmlFor contract must be reviewed alongside control association |
| Textarea | **defer** | Same Form Field slice as Input; shares the invalid-prop pattern; needs holistic form-field accessibility contract |

**Deferred-owner:** P-1 does not assign a specific plan ID. The revisit trigger is the Form Field slice, which must demonstrate at least two Web consumers and one Studio fixture before reopening these three for promotion.

> **Status update (V1.106 P0 T1 — 2026-07-10):** Input, Label, and Textarea were promoted in V1.100 P2; Select was promoted in V1.101 P2. See §Non-Promotion Record for the current classification.

> **Update (V1.100 T1 — 2026-07-08):** The Form Field contract is now locked — see `.mstar/iterations/v1.100/specs/form-field-contract.md`. Implementation proceeds in plan `2026-07-08-v1.100-form-field-component-promotion` (P2). The contract defines precise label/control association, `aria-invalid`/`aria-describedby` ownership, helper/error/required semantics, wrapper strategy, and confirms no stateful `FormField` in this iteration.

### Out of Scope (never candidates)

Any component whose main value comes from app copy, daemon status, route state, setup progression, or shell layout. These are not listed in the non-promotion record — the exclusion rules in §Exclusions are the SSOT.

## Consumer Pattern

- `packages/nexus-ui` owns promoted primitive implementation and exports it publicly.
- `apps/web` imports promoted primitives from `@42ch/nexus-ui`; app-specific wrappers stay in `apps/web` only when behavior/state is needed.
- `apps/design-studio` imports promoted primitives from `@42ch/nexus-ui`; remaining transitional primitives may still use `@web-ui/*` until later iterations.
- `tooling/design-tokens` remains the shared Tailwind/CSS pipeline; `@42ch/nexus-ui` must not duplicate the full root DESIGN contract.
- New Studio `/surfaces` compositions may use promoted primitives, but shell/setup fixture structure remains studio-local until Web integration proves reusable behavior.

## Locked Decisions (resolved by V1.99 Grill-Me)

| Question | Decision | Rationale |
| --- | --- | --- |
| Defer Input / Label / Textarea? | **Deferred** | Grill-Me lock: promote only Button, Badge, Card first. Input/Label/Textarea are technically pure enough, but promoting them alone locks an incomplete form-field contract. Revisit trigger: Form Field slice with ≥2 Web consumers + one Studio fixture. |
| Expose variant helpers (`buttonVariants`, `badgeVariants`)? | **Keep internal for V1.99** | Grill-Me lock: variant helpers stay internal unless implementation proves a consumer needs one. If any consumer already imports `buttonVariants` from `@web-ui/*`, P0 must either (a) keep the app-local wrapper importing the old `buttonVariants` from `apps/web/src/components/ui/button.tsx` or (b) document the public API need in the P0 plan and reopen this decision with architect sign-off. Current `buttonVariants` and `badgeVariants` are exported from their source files in `apps/web` — P0 must verify whether `apps/web` callers import those exports before deciding the promotion strategy for those symbols. |
| Package naming? | **Remain `@42ch/nexus-ui`** | Grill-Me lock: no split between brand assets and component primitives at this stage. |
| AGENTS.md amendments needed? | **Minimal — add `cn` helper rule** | The existing `packages/nexus-ui/AGENTS.md` V1.99 section already covers: named root exports, `src/index.ts` entry, no deep subpaths, wrapper strategy, runtime-deps allowlist. One gap: the spec's "Class composition" rule (§Package Architecture Rules line 3) is not explicitly reflected. P-1 adds it: "Package must own its own `cn` helper (clsx + tailwind-merge with DESIGN.md token class-group extension); must not import from `apps/web`." |

These are **final** for V1.99 P0 — do not reopen unless a P0 implementation discovery proves one must change, in which case escalate to PM with specific evidence.

## Non-Promotion Record

Every component in `apps/web/src/components/ui/index.ts` barrel has a classification. Promoted components are listed in §Locked First-Batch List and not repeated here.

| Component | Classification | Rationale |
| --- | --- | --- |
| button | `promote` | — see §Locked First-Batch List |
| badge | `promote` | — see §Locked First-Batch List |
| card | `promote` | — see §Locked First-Batch List |
| input | `promote` | Promoted in V1.100 P2 — see `.mstar/iterations/v1.100/specs/form-field-contract.md` |
| label | `promote` | Promoted alongside Input in V1.100 P2 Form Field slice |
| textarea | `promote` | Promoted alongside Input in V1.100 P2 Form Field slice |
| select | `promote` | Promoted in V1.101 P2 — native `<select>` presentational primitive |
| toast | `promote` | Promoted in V1.106 P0 (`packages/nexus-ui/src/components/toast.tsx`) for Studio Surfaces fixtures; **App thin re-export** at `apps/web/src/lib/use-toast.tsx` completes in V1.107 FB-012 (`R-V1106P0-001`). Variant icons use `lucide-react` — **package runtime dependency exception** (`R-V1106P0-002`); apps retain separate lucide usage elsewhere. |
| dialog | `keep-web` | Imports `@radix-ui/react-dialog` + `lucide-react`; uses `DialogPrimitive.Portal` (fixed positioning, focus trap, scroll lock) — behavior layer beyond presentational scope; title/description wired to Radix accessibility primitives |
| tabs | `keep-web` | Compound component with internal React context + `useState` state management (`controlled`/`uncontrolled` pattern); not purely presentational — owns selection state |
| table | `keep-web` | Pure presentational table primitives, but wraps output in `<div className="w-full overflow-x-auto">` (responsive layout concern); not in V1.99 first batch |
| states | `keep-web` | `Spinner` imports `lucide-react` (`Loader2` icon — asset boundary); `ErrorState` has `onRetry` callback + product copy ("Could not load this view", "Try again"); `EmptyState` accepts `action` ReactNode (app-composition pattern) |

### Revisit history

The V1.99 deferred primitives below have since been promoted:

| Component(s) | Promotion slice | Owner |
| --- | --- | --- |
| input, label, textarea | V1.100 P2 Form Field slice — ≥2 Web consumers + 1 Studio fixture | `2026-07-08-v1.100-form-field-component-promotion` |
| select | V1.101 P2 native Select primitive | `2026-07-09-v1.101-select-component-promotion` |
| toast | V1.106 P0 Studio-first pipeline (package); V1.107 P0 App re-export shim | `2026-07-10-v1.106-studio-first-pipeline`, `2026-07-10-v1.107-studio-ui-tune` |

No deferred primitives remain from V1.99. Future keep-web candidates (Dialog, Tabs, Table, States) should be reconsidered only when a cross-app reuse case, dependency-footprint shrink, or behavior-layer extraction is demonstrated.

### Toast runtime dependency footnote (V1.106 / V1.107)

`Toast` is the first promoted primitive whose variant icons import `lucide-react` inside `@42ch/nexus-ui`. This is a **documented exception** to the general “no lucide in package” pattern used by `keep-web` `states.tsx`. Add `lucide-react` to `packages/nexus-ui/package.json` `dependencies` when promoting; do **not** make it a peer — consumers are not required to share a lucide instance for Toast icons. App call sites may keep `@/lib/use-toast` as a thin re-export after V1.107 FB-012.

### keep-web Intent

Components classified `keep-web` are not expected to move to `@42ch/nexus-ui` in V1.99. They may be reconsidered in a future iteration if:
- A cross-app reuse case is demonstrated (Dialog used in Studio onboarding flows; Select used in Studio preference panels).
- Their dependency footprint shrinks (e.g. `states.tsx` shed its `lucide-react` import and product copy).
- Their behavior layer is extracted (e.g. `tabs.tsx` separated into a pure presentational layer + a state-management hook).

## P0 Implementation Contract

This section is the unambiguous contract for the V1.99 P0 implementer. Every item is locked per the V1.99 compass Grill-Me decisions and verified against the actual `apps/web/src/components/ui/*` source.

### (a) First-Batch List (lock)

| # | Component | Source File (current) | Exported Symbols | Must Have in Package |
| --- | --- | --- | --- | --- |
| 1 | Button | `apps/web/src/components/ui/button.tsx` | `Button`, `ButtonProps`, `buttonVariants` (see note) | `Button` component + `ButtonProps` type; `buttonVariants` internal unless P0 proves a consumer needs it public |
| 2 | Badge | `apps/web/src/components/ui/badge.tsx` | `Badge`, `BadgeProps`, `badgeVariants` (see note) | `Badge` component + `BadgeProps` type; `badgeVariants` internal unless P0 proves a consumer needs it public |
| 3 | Card | `apps/web/src/components/ui/card.tsx` | `Card`, `CardHeader`, `CardTitle`, `CardDescription`, `CardContent` | All five sub-primitives |

**Variant-helper note:** P0 must `grep` for `buttonVariants` and `badgeVariants` imports across `apps/web` before deciding whether to export or internalize. If callers exist, document them in the P0 plan; if none exist, keep them internal.

### (b) Named Root Exports

All first-batch primitives are named exports from `@42ch/nexus-ui` via `src/index.ts`:

```ts
// packages/nexus-ui/src/index.ts additions (P0 adds after existing brand exports)
export { Button, type ButtonProps } from './components/button';
export { Badge, type BadgeProps } from './components/badge';
export { Card, CardHeader, CardTitle, CardDescription, CardContent } from './components/card';
```

No per-component deep subpath exports (e.g., `@42ch/nexus-ui/button`). `src/components/*` remains internal implementation.

### (c) Runtime Dependencies to Add

These must be added to `packages/nexus-ui/package.json` `dependencies` (NOT `peerDependencies`):

```json
{
  "dependencies": {
    "class-variance-authority": "^0.7.0",
    "@radix-ui/react-slot": "^1.0.0",
    "clsx": "^2.0.0",
    "tailwind-merge": "^2.0.0"
  }
}
```

React (`>=18`) and React DOM (`>=18`) remain `peerDependencies` — already declared.

### (d) Package-Local `cn` Helper

The package must own its own class-composition helper. P0 must:

1. Create `packages/nexus-ui/src/lib/cn.ts` containing the same `extendTailwindMerge` config as `apps/web/src/lib/utils.ts` (registering all DESIGN.md custom font-size tokens).
2. All promoted components must import `cn` from the package-local helper, not from `@/lib/utils` or `apps/web`.
3. The package build (tsup) already bundles `clsx` and `tailwind-merge` when imported — no extra build config needed.

### (e) Variant Helper Visibility

- `buttonVariants` and `badgeVariants`: **internal by default.** P0 checks for external callers; if none exist, do not export them from `src/index.ts`.
- If a consumer needs a variant helper: P0 must file a note in the P0 plan and update this spec's locked decisions — not unilaterally export.

### (f) Consumer Wrapper Strategy for `apps/web`

After promotion, `apps/web/src/components/ui/button.tsx`, `badge.tsx`, and `card.tsx` may become thin re-export/wrapper files:

```ts
// apps/web/src/components/ui/button.tsx (example wrapper)
export { Button, type ButtonProps } from '@42ch/nexus-ui';
// Re-export buttonVariants only if callers exist in apps/web
export { buttonVariants } from '@42ch/nexus-ui';  // only if needed
```

Or, if the component needs no app-specific behavior, P0 may delete the file and update all `@/components/ui` imports to `@42ch/nexus-ui` directly. Either approach is acceptable as long as:

- `apps/web` screens remain visually identical before and after promotion.
- No daemon state, routing, or product copy leaks into the package.
- All existing tests pass.

### (g) Package Test Requirements

Each promoted component must have a `packages/nexus-ui/src/components/<name>.test.tsx` that:

- Renders each variant combination at least once.
- Verifies accessibility-relevant class output (e.g., `aria-invalid` binding for future form components, `role` attributes for compound components).
- Tests the `cn` merge path (pass `className` prop and verify it appears in output).

### (h) Consumer Updates

After P0 promotion:

- `apps/design-studio` must no longer import Button, Badge, or Card through `@web-ui/*` — use `@42ch/nexus-ui` directly.
- `apps/web` may consume through wrappers or direct imports per §(f).
- `pnpm run build` and `pnpm run test` must remain green in the package and both consumer apps.

## Acceptance Hooks

- Package boundary update is reflected in `packages/nexus-ui/AGENTS.md` and README.
- `apps/design-studio` no longer imports promoted first-batch components through `@web-ui/*`.
- `apps/web` uses the promoted first-batch primitives where parity is low risk.
- Package build/typecheck/test and both consumer tests/builds remain green.
