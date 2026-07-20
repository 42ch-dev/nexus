---
module: nexus-ui
date: 2026-07-08
last_updated: 2026-07-20
problem_type: architecture_decision
category: architecture-patterns
severity: medium
plan_id: 2026-07-08-v1.99-nexus-ui-component-promotion; V1.100 guardrails+form-fields; V1.101 Select + AgentPicker placement; V1.106 Toast promotion; V1.107 App Toast adoption
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
2. **`AgentPicker` is app-shared, not package (Must P0).** Reusable across wizard and Settings (**V1.102** thin host → **V1.103** `/settings/agent` section) at `apps/web/src/components/setup/agent-picker.tsx`, but it composes scan/profile/outbound-link product semantics — **do not** promote to `@42ch/nexus-ui` until the surface is presentational-only. Studio may import via a scoped alias. **V1.103** delivers S3 Settings shell (Agent/Connection/Setup; Stretch Workspace deferred → **V1.104**). **V1.104** delivers Workspace W2 Must — execution-mode matrix remains DF-70 deferred. See [settings-shell-ia.md](../../iterations/v1.103/specs/settings-shell-ia.md) and [v1.104 workspace section](../../iterations/v1.104/specs/settings-workspace-section.md).

**Studio-first + smoke separation (process):** UI-visual work = Studio fixtures → visual acceptance → App wiring on a separate track. Interactive macOS desktop smoke is a **human gate**, not an automated Done / CI blocker. Distilled from `.mstar/iterations/v1.101/guides/studio-first-visual-then-app.md` (workspace snapshot).

## V1.103 Extension — Settings shell modules + form extract

V1.103 deepened the thin Settings host into an S3 multi-section shell without promoting product forms to `@42ch/nexus-ui`:

1. **Shell + section modules under `apps/web/src/pages/settings/`.** `settings-shell-layout.tsx` owns secondary nav + `<Outlet />`; section bodies are siblings (`settings-agent-section`, `settings-connection-section`, `settings-setup-section`). Register `/settings/workspace` **only** when Stretch Workspace runs — Must plans must not ship a dead Workspace tab.
2. **Extract product forms beside the shell, not into the package.** Connection reused Connect UI via `apps/web/src/components/settings/connect-daemon-form.tsx` (extract from the legacy page); Settings mounts the form and owns post-save stay-on-section + `/connect` → `/settings/connection` redirect.
3. **Marker context races are directional.** Re-run Setup vs wizard Finish need asymmetric `setCompleted` timing — see [asymmetric-setup-completed-context.md](./asymmetric-setup-completed-context.md).

**Process note:** V1.103 reaffirmed studio-first per section (Studio chrome → App IPC) in `.mstar/iterations/v1.103/guides/studio-first-visual-then-app.md` (workspace snapshot; not promoted as a second process doc).

## V1.106 Extension — Toast package promotion + re-export hazard

V1.106 P0 promoted `ToastProvider` / `useToast` / `Toaster` to `@42ch/nexus-ui` so Studio `/components` could fixture variant matrices. The package landed, but App kept a near-verbatim duplicate at `apps/web/src/lib/use-toast.tsx` (~40+ call sites) — residual **`R-V1106P0-001`**.

**Lesson:** package promotion is **not** complete until the App adopts the **thin re-export** pattern from §Consumer wrapper strategy. Verbatim copy creates drift risk and false “pipeline complete” claims. Toast also introduced a **`lucide-react` package runtime dependency** for variant icons — documented exception in [`component-promotion-boundary.md`](../../iterations/v1.99/specs/component-promotion-boundary.md) (`R-V1106P0-002`).

## V1.107 Extension — App Toast adoption + presentational gallery aliases

V1.107 P0 closes the Toast loop and extends studio-first to shell/Settings chrome:

1. **FB-012:** Replace App `use-toast.tsx` body with re-export from `@42ch/nexus-ui`; preserve `@/lib/use-toast` import path; closes `R-V1106P0-001`.
2. **FB-013..015:** Presentational extracts under `layout/presentational/` and `settings/presentational/`; Studio imports `@web-layout/*` and `@web-settings/*` — not routing-heavy `sidebar.tsx` or IPC-backed `ConnectDaemonForm`.
3. **FB-000:** Studio Tailwind must scan setup + presentational + nexus-ui sources — without this, studio-first visual acceptance is blocked.

**Voice lock (FB-008):** workspace field label **Workspace folder**; change action **Change Folder…** on wizard and Settings (no wizard **Browse…**).

**Anti-pattern lesson (QC F-CRIT-001 + F-WARN-001):** Presentational chrome extracts must be **purely** props-driven — no internal state, no `keep-web` Radix Dialog, no Studio-only aliases (`@web-ui/*`) in App-source files. The `SettingsSetupSectionChrome` initially imported `Dialog` from `@web-ui/dialog` (a Studio-only alias undefined in `apps/web/tsconfig.json`) and owned `useState(confirmOpen)`, causing: (a) TS2307 typecheck failure (CI blocker), (b) architectural boundary violation (Radix `keep-web` inside presentational chrome). **Fix pattern:** hoist Dialog state + JSX into the host (App wrapper or Studio fixture); chrome exposes `onReRunSetup?: () => void` callback only. The chrome is markup + classes + `data-testid` SSOT — nothing more.

**Studio Tailwind content (FB-000 root cause):** Many V1.106 "visual regressions" (Badge solid fills, Button destructive, TopStepIndicator circles invisible) were **not** component bugs — they were Studio Tailwind `content` glob gaps. Adding `../web/src/components/setup/**`, `../web/src/components/layout/presentational/**`, `../../packages/nexus-ui/src/**` fixed 5 FBs (001, 002, 005, 010, 011) without touching the components themselves. **Lesson:** before debugging a Studio visual regression, verify the Tailwind `content` array first.

## V1.128 Extension — Two-tier Studio import model (`@web-*` vs `@42ch/nexus-ui`)

V1.128 P3 made the import tiers **visible** in Surfaces badges and durable docs — without mass promotion.

### Three import tiers (not two packages)

| Tier | Pattern | npm? | Promotion path |
| --- | --- | --- | --- |
| **Promoted primitive** | `@42ch/nexus-ui` | Yes (workspace) | Studio visual acceptance → package export → Web thin re-export |
| **App presentational extract** | `@web-layout/*`, `@web-canvas/*`, `@web-setup/*`, `@web-settings/*`, `@web-global-timeline/*`, `@web-shell/*`, … | **No** — monorepo Vite/tsconfig alias only | Studio fixture → `apps/web/**/presentational/*` extract → App host wiring; stays on `@web-*` until a plan promotion list entry |
| **Transitional primitive** | `@web-ui/*` | **No** | Unpromoted shadcn mirror; inline `// transitional` comment required |

**Lesson:** `@web-*` paths look like package names but are **not** npm exports. Copying a Surfaces fixture import does not mean the symbol ships from `@42ch/nexus-ui` — check the section source badge or `apps/design-studio/AGENTS.md` two-tier table first.

### Labeling is the Must; promotion is optional

- P3 success = gallery badges (`surface-source-badge-*` test ids) + AGENTS/spec two-tier tables — **zero** mass migration required.
- Optional single-primitive promotion still follows §Promotion rules and an explicit plan promotion list entry; alias clarity must not be used as a excuse for bulk package moves.

### Surfaces badge convention

`classifySurfaceImport()` maps:

- `@42ch/nexus-ui` → **Promoted primitive**
- `@web-*` → **App presentational extract** (shows full alias path in badge copy)
- `@web-ui/*` → **Transitional primitive**

Daemon Surfaces sections that only compose promoted primitives must badge `@42ch/nexus-ui`, not a fictional `@web-daemon/*` alias (V1.128 P3 fix wave).

### RF-free canvas extracts stay on `@web-canvas/*`

NLE Timeline chrome (`nle-timeline-chrome`, V1.128 P1) is an `@web-canvas/*` presentational extract — **must not** import `@xyflow/react`. App RF hosts (`timeline-canvas.tsx`, `work-timeline-canvas.tsx`) mount a thin overlay (`nle-timeline-band-overlay.tsx`) that projects RF node data into extract props. Pull-off demo stays Studio fixture local state only; App adopt is chrome swap, not new RF DnD scope.

**Normative iteration detail:** [web-alias-clarity.md](../../iterations/v1.128/specs/web-alias-clarity.md) · [nle-timeline-canvas.md](../../iterations/v1.128/specs/nle-timeline-canvas.md).
