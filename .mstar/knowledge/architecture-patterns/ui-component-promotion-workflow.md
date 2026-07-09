---
module: nexus-ui
date: 2026-07-08
last_updated: 2026-07-09
problem_type: architecture_decision
category: architecture-patterns
severity: medium
plan_id: 2026-07-08-v1.99-nexus-ui-component-promotion; V1.100 guardrails+form-fields; V1.101 Select + AgentPicker placement
tags: [nexus-ui, component-promotion, design-studio, presentational-primitives, package-boundary, agent-picker, select, studio-first]
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

- ~~`cn` helper is duplicated between `packages/nexus-ui/src/lib/cn.ts` and `apps/web/src/lib/utils.ts` (byte-identical `extendTailwindMerge` config). Future consolidation to `@nexus/design-tokens` is tracked as a post-V1.99 residual.~~ **RESOLVED V1.100 (P1):** `cn` is now a public `@42ch/nexus-ui` export; `apps/web/src/lib/utils.ts` is a thin re-export. `@nexus/design-tokens` was rejected as the authority because it already depends on `@42ch/nexus-ui` (would cycle). A behavioral SSOT check (`tooling/check-ui-guardrails.sh`) verifies single-source authority.
- Variant helpers (`buttonVariants`, `badgeVariants`) stay internal to the package. If a consumer needs them, that's a signal the promotion boundary needs review.

## V1.100 Extension — Mechanically-Enforced Guardrails + Semantics-First Form Fields

V1.100 (P1 + P2) hardened the promotion workflow from reviewer-instruction to **mechanically-enforced** and proved it on a **semantics-first** second slice:

1. **Guardrails are now a shell script + CI job** (`tooling/check-ui-guardrails.sh`), not manual grep. It catches: promoted-wrapper forbidden imports (`clsx`/`cva`/`tailwind-merge`/`@/lib/*`/deep `@42ch/nexus-ui/src/*`), Design Studio boundary violations (web pages/daemon/Tauri), missing/invalid `@web-ui/*` transitional annotations, and cn-SSOT drift. A new primitive auto-enters the guard set upon promotion (wrapper auto-detected by re-export content). **Lesson:** a `set -euo pipefail` grep script modeled on `check-schema-drift.sh` is the right ladder rung — scoped ESLint was overkill; Vitest-only can't catch unintended additions on unmodified files.
2. **cn class-merge config has one authority** (`@42ch/nexus-ui/src/lib/cn.ts`). Do NOT move it to `@nexus/design-tokens` while design-tokens depends on the UI package (cycle). Public `cn` export via the barrel; deep imports forbidden.
3. **Form-field promotion is semantics-first, not lift-and-shift.** V1.99 deferred Input/Label/Textarea because moving code without locking helper/error/required semantics "would only move code." V1.100 P2 locked an explicit contract first: label/control association (`htmlFor`+`id`, **app-owned** id generation), `invalid`→`aria-invalid="true"` (`invalid || undefined` coercion so false/omitted omits the attribute), `aria-describedby` **app-wired**, helper/error/required copy **app-owned**, **no stateful `FormField` package export**. The package owns only the presentational surface + native attribute passthrough. Promote the contract BEFORE the code.

**Promotion checklist (locked, reusable):** move source+tests → `packages/nexus-ui/src/components/`; add to barrel; Web wrapper = thin re-export; Studio switches `@web-ui/<name>` → `@42ch/nexus-ui`; update the guardrail promoted set + transitional table. See `.mstar/iterations/v1.100/specs/ui-guardrails-cn-ssot.md` § "Promotion checklist" (iteration snapshot).

## V1.101 Extension — Select promotion + app-shared vs package placement

V1.101 proved two complementary placement rules on the same studio-first track:

1. **`Select` is a package promotion (Stretch P2).** Same semantics-first ladder as V1.100 form fields: lock a11y/composition contract → Studio fixtures → `@42ch/nexus-ui` presentational export + tests → Studio-direct import → Web thin re-export → update `tooling/check-ui-guardrails.sh` + Studio README import surface. Transitional `@web-ui/*` remains only for unpromoted keep-web primitives (Dialog, States, Table, Tabs). **Docs must match guardrails** — listing a promoted primitive under `@web-ui/*` in README is a QC blocker even when code is correct.
2. **`AgentPicker` is app-shared, not package (Must P0).** Reusable across wizard and Settings (**V1.102** thin host → **V1.103** `/settings/agent` section) at `apps/web/src/components/setup/agent-picker.tsx`, but it composes scan/profile/outbound-link product semantics — **do not** promote to `@42ch/nexus-ui` until the surface is presentational-only. Studio may import via a scoped alias. **V1.103** delivers S3 Settings shell (Agent/Connection/Setup; Stretch Workspace deferred → V1.104+) — execution-mode matrix remains DF-70 deferred. See [settings-shell-ia.md](../../iterations/v1.103/specs/settings-shell-ia.md).

**Studio-first + smoke separation (process):** UI-visual work = Studio fixtures → visual acceptance → App wiring on a separate track. Interactive macOS desktop smoke is a **human gate**, not an automated Done / CI blocker. Distilled from `.mstar/iterations/v1.101/guides/studio-first-visual-then-app.md` (workspace snapshot).

## V1.103 Extension — Settings shell modules + form extract

V1.103 deepened the thin Settings host into an S3 multi-section shell without promoting product forms to `@42ch/nexus-ui`:

1. **Shell + section modules under `apps/web/src/pages/settings/`.** `settings-shell-layout.tsx` owns secondary nav + `<Outlet />`; section bodies are siblings (`settings-agent-section`, `settings-connection-section`, `settings-setup-section`). Register `/settings/workspace` **only** when Stretch Workspace runs — Must plans must not ship a dead Workspace tab.
2. **Extract product forms beside the shell, not into the package.** Connection reused Connect UI via `apps/web/src/components/settings/connect-daemon-form.tsx` (extract from the legacy page); Settings mounts the form and owns post-save stay-on-section + `/connect` → `/settings/connection` redirect.
3. **Marker context races are directional.** Re-run Setup vs wizard Finish need asymmetric `setCompleted` timing — see [asymmetric-setup-completed-context.md](./asymmetric-setup-completed-context.md).

**Process note:** V1.103 reaffirmed studio-first per section (Studio chrome → App IPC) in `.mstar/iterations/v1.103/guides/studio-first-visual-then-app.md` (workspace snapshot; not promoted as a second process doc).
