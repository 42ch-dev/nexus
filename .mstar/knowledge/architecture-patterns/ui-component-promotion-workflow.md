---
module: nexus-ui
date: 2026-07-08
problem_type: architecture_decision
category: architecture-patterns
severity: medium
plan_id: 2026-07-08-v1.99-nexus-ui-component-promotion
tags: [nexus-ui, component-promotion, design-studio, presentational-primitives, package-boundary]
applies_when: promoting a UI primitive from app ownership into @42ch/nexus-ui, or deciding where a new UI component should live
---

# UI Component Promotion Workflow

## Context

Nexus has three surfaces for UI work: `apps/design-studio` (daemon-independent Vite gallery), `packages/nexus-ui` (publishable presentational React package), and `apps/web` (daemon-served SPA). V1.99 established and validated a **studio-first promotion workflow** that moves reusable visual primitives through these surfaces in a controlled way.

Related: [`nexus-brand-token-hierarchy.md`](nexus-brand-token-hierarchy.md) covers the token flow; this doc covers the **component placement** decision and workflow.

## Guidance

### The workflow (4 stages)

1. **Studio composition** — Build pure View fixtures in `apps/design-studio` using root DESIGN tokens and `@nexus/design-tokens`. Iterate visually (light/dark, focus, a11y) without daemon/Tauri cost. Use `@42ch/nexus-ui` for already-promoted primitives; use `@web-ui/*` (transitional alias to `apps/web/src/components/ui/*`) for not-yet-promoted primitives.

2. **Visual acceptance** — Before any Web integration: both themes look intentional, keyboard focus visible, no unregistered Tailwind scale steps, copy follows DESIGN.md Voice & Content, fixture communicates its job without daemon data.

3. **Promotion decision** — For each stable component, classify:
   - **`promote`**: pure presentational, token-driven, reusable across Studio + Web → move to `@42ch/nexus-ui`.
   - **`keep-web`**: app-specific behavior, route/data coupling, or reuse only inside Web.
   - **`keep-studio`**: one-off fixture or visual exploration with no reusable contract.
   - **`defer`**: likely reusable but API/accessibility contract not stable enough yet.

4. **Web integration** — Replace static fixture data with real data/behavior. Keep app state, routing, daemon transport in `apps/web`. Prefer thin app-local re-export wrappers to reduce call-site churn.

### Promotion rules (a component may move to `@42ch/nexus-ui` only if ALL are true)

- Pure presentational React — renders without daemon state, routing, `NexusClient`, localStorage, Tauri IPC, or app providers.
- Consumes shared design tokens through classes/CSS variables, not raw one-off values.
- Does not import from `apps/web`, `apps/design-studio`, or app-local aliases.
- React stays peer dependency; implementation helpers (cva, radix-slot, clsx, tailwind-merge) are package runtime deps only when imported.
- Package owns its own `cn` helper (replicating `extendTailwindMerge` config from `apps/web/src/lib/utils.ts`); no `@web-lib/utils` import.
- Has package-level tests covering variants and accessibility-relevant class output.
- Consumers import through public named root exports (`@42ch/nexus-ui`), not internal source paths.
- No `.svg`/`.png` imports from component source (see [`bundler-agnostic-component-library-assets.md`](bundler-agnostic-component-library-assets.md)).

### Consumer wrapper strategy

`apps/web/src/components/ui/<component>.tsx` becomes a thin re-export:
```typescript
export { Button, type ButtonProps } from '@42ch/nexus-ui';
```
The barrel (`index.ts`) propagates via `export * from './button'`. This eliminates call-site churn (all `@/components/ui` imports keep working). App-specific behavior stays in the app, never moves back to the package.

### `@web-ui/*` transitional policy

Unpromoted primitives stay on `@web-ui/*` (the design-studio tsconfig alias to `apps/web/src/components/ui/*`). Each transitional import should carry an inline comment with its classification and blocking criteria for promotion. Once a primitive is promoted, both Studio and Web switch to `@42ch/nexus-ui` imports.

## Why This Matters

Without this workflow, UI primitives either (a) stay app-owned and can't be shared, or (b) get dumped into the package without boundary discipline, turning `@42ch/nexus-ui` into a second app shell. The promotion rules keep the package presentational-only while making visual iteration fast (Studio) and integration safe (thin wrappers in Web).

## When to Apply

- Adding a new shared UI primitive
- Deciding whether a Studio fixture should become a package component
- Evaluating whether an `apps/web` component is ready for promotion
- Reviewing the `@42ch/nexus-ui` package boundary

## V1.99 Validation

V1.99 validated this workflow with a focused first batch: **Button, Badge, Card** promoted; **Input, Label, Textarea** deferred to a future Form Field slice; **Dialog, Tabs, Select, Table, States** classified `keep-web`. All three promoted components followed the full Studio → Package → Web path with 602+ tests passing across all packages.

## Known Limitations

- `cn` helper is duplicated between `packages/nexus-ui/src/lib/cn.ts` and `apps/web/src/lib/utils.ts` (byte-identical `extendTailwindMerge` config). Future consolidation to `@nexus/design-tokens` is tracked as a post-V1.99 residual.
- Variant helpers (`buttonVariants`, `badgeVariants`) stay internal to the package. If a consumer needs them, that's a signal the promotion boundary needs review.
