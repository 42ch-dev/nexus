# Studio-First UI Workflow (V1.99 Draft)

**Status**: Draft (iteration-scoped guide)  
**Owner**: product-manager + architect during Phase 1 Review & Edit  
**Consumers**: V1.99 plans, `apps/design-studio`, `apps/web`, `packages/nexus-ui`, possible iteration-close knowledge

## Problem

UI work currently becomes expensive when agents must validate visual direction through the full `apps/web` / desktop path. `apps/design-studio` can shorten that loop because it runs as a daemon-independent Vite app and already consumes the same root DESIGN pair and `@nexus/design-tokens` pipeline.

The product risk is that Design Studio becomes either a pretty dead-end gallery or an accidental second app shell. V1.99 validates the middle path: use studio for fast View-level decisions, then promote or integrate only after the boundary is clear.

The workflow to validate in V1.99:

1. Compose pure View components and static fixtures in `apps/design-studio`.
2. Iterate visually and accessibly against light/dark themes without daemon/Tauri.
3. Promote stable presentational primitives into `@42ch/nexus-ui` when reusable.
4. Integrate accepted components into `apps/web` with real data, routing, and behavior.

## Workflow Contract

### Stage 1 — Studio Composition

- Build static, story-like fixtures in `apps/design-studio`.
- Use root `DESIGN.md` / `DESIGN.dark.md` tokens and `@nexus/design-tokens`.
- Use `@42ch/nexus-ui` for promoted primitives and brand pieces.
- Use `@web-ui/*` only for not-yet-promoted primitives, and track the dependency as transitional.
- Do not import `apps/web` pages, layout components, daemon clients, route definitions, app providers, product hooks, localStorage-backed product state, Tauri helpers, or `@42ch/nexus-contracts`.

### Stage 1A — Surface Fixture Boundary

`/surfaces` may look like setup or app shell UI, but its implementation remains a View fixture:

- Keep setup steppers, workspace rows, shell chrome, daemon status strips, and profile/footer mockups studio-local unless a smaller primitive beneath them is approved for package promotion.
- Use promoted package primitives for generic controls (`Button`, `Badge`, `Card` after P0) and keep unpromoted controls on `@web-ui/*` only while transitional.
- Use static product-shaped data; do not simulate daemon lifecycle, routing transitions, creator bootstrap, or persistence.
- For each major section, record the intended next home: `promoted primitive`, `studio-local fixture`, `web-only wrapper`, or `future web product component`.

### Stage 2 — Visual Acceptance

Acceptance before `apps/web` integration:

- Light and dark themes both look intentional.
- Keyboard focus and disabled states are visible.
- No unregistered Tailwind scale steps are introduced.
- Copy follows DESIGN.md Voice & Content rules.
- The surface communicates its job without live daemon data.
- A reviewer can name what would change, if anything, when the fixture enters `apps/web`.

### P1 Surfaces Visual Direction (Locked)

**Scope:** This section locks the visual target and acceptance bar for P1 (`2026-07-08-v1.99-surfaces-visual-direction`). P1 will redesign the two surfaces in `apps/design-studio/src/pages/surfaces.tsx` (Setup step card + App shell chrome) against these observable criteria. The surfaces remain studio-local View fixtures per the Stage 1A boundary.

#### 1. Target Visual Qualities

**Setup / Welcome surface (the integrated wizard card):**
- **Hierarchy:** Content panel opens with `heading-24` title ("Welcome to Nexus"), followed by `copy-16` body (two sentences max). Field label is `label-14` above the inline location row. Primary action uses a full-width (max 400px) primary Button. Step list on left uses `label-14` with active state in `gray-1000` medium weight.
- **Rhythm / spacing:** Vertical rhythm uses `space-6` (24px) between title/body, body/input group, and input group/CTA. Input row uses `space-4` (16px) horizontal padding, `space-3` (12px) icon-to-text gap. Left step panel uses `space-6` padding. Card internals follow `setup-wizard-surface.*` tokens.
- **Depth / elevation:** Single `Card` with `shadow-modal` and `rounded-popover`. Left step panel on `background-100` with thin `gray-alpha-200` right divider on desktop.
- **Affordance clarity:** Workspace row is one visual unit (folder icon in `blue-700`, path text in `gray-1000`, secondary "Browse…" Button). Active step shows filled blue circle with white checkmark; pending steps use gray-alpha circle. Connector is a thin vertical line (`gray-alpha-400`).
- **Theme parity:** All colors drawn from registered scales (`gray-*`, `background-*`, `blue-700`, `green-700`). No raw hex or one-off values.
- **Focus visibility:** Every Button and the Browse control shows the two-layer focus ring (outer `blue-700` / inner `background-100` in light; cyan outer in dark).
- **Calm / premium personality:** Generous but disciplined internal padding (no cramped elements); quiet borders; restrained weights (no bold beyond headings); the surface reads as a single calm card, not a stack of boxes.

**App shell chrome surface:**
- **Hierarchy:** Top tab strip uses `label-14` medium with active state showing `blue-700` bottom bar. Nav group labels `label-14`; active group has `gray-alpha-100` background + `gray-1000`. Nested child item has left `blue-700` accent bar. Footer profile uses small avatar + `label-14` name / `copy-13` secondary.
- **Rhythm / spacing:** Sidebar nav items are 36px tall. Consistent `space-2` (8px) gaps in footer row. Tab strip items have `py-3` vertical. Content area uses `background-200`.
- **Depth / elevation:** Sidebar has right `gray-alpha-200` border. Main area is a quiet recessed `background-200`. No heavy shadows on chrome.
- **Affordance clarity:** Active tab and active nav group are immediately distinguishable by bar + background. Hover states on non-active items. Add-profile icon button is small, secondary, and labeled.
- **Theme parity, focus, calm:** Same rules as setup surface. Shell chrome must feel like a container, not compete with content.

#### 2. Token Gap Triage

Review of `DESIGN.md` + `DESIGN.dark.md` (v0.3.0) + current `surfaces.tsx` (before P1):

**Token gaps (missing scale steps or projections that would force raw values):**
- Shell sidebar width (`248px`) and nav item height (`36px`) are recorded in `components.sidebar-nav` but are **not projected** as consumable Tailwind utilities in the preset (no `w-sidebar` or equivalent spacing token). Current fixture hardcodes `w-[248px]`.
- Thin connector line thickness (`2px` in step list) has no dedicated token.
- Fixture-specific min-heights (e.g. `min-h-[440px]`) are intentionally not root tokens.

**Composition / copy issues (tokens exist in DESIGN + preset but current code does not use them):**
- Setup surface hardcodes values (`w-[208px]`, `p-8 sm:p-10`, `h-12`, `max-w-[400px]`, raw `bg-blue-700` on step circle) instead of the `setup-wizard-surface.*` and `setup-wizard-step.*` tokens + mapped utilities that were added to DESIGN + `@nexus/design-tokens` preset in V1.96.
- Step circle sizes, input row heights, card paddings, and CTA container gap have registered tokens but the fixture uses raw Tailwind classes and inline dimensions.
- Avatar, footer buttons, and some chrome use `w-8 h-8`, `h-9`, `w-2.5 h-2.5` instead of standardized size or component tokens where defined.
- Many `space-*`, `rounded-*`, and `shadow-*` opportunities are present but not taken.

**Conclusion for P1:** The dominant problems are composition and token consumption, not missing tokens. P1 must align the fixtures to existing registered scales and the V1.96 setup-wizard tokens. Add at most minimal shell chrome projections only if P1 proves they are required for future web parity; otherwise treat sidebar and thin-line dimensions as documented studio-local fixture values.

#### 3. Surface Element Classification

**Setup wizard step card:**
- Outer `Card`: `promoted primitive` (Card after P0)
- Step indicator list (circles + connectors + labels): `studio-local fixture` (setup-specific stepper flow)
- Welcome title + body copy: composition (no primitive)
- Workspace location affordance (folder icon + path text + Browse Button inline row): `studio-local fixture` (inline input pattern for first-launch setup)
- "Browse…" Button: `promoted primitive`
- "Continue" primary Button: `promoted primitive`
- Field `Label`: transitional `@web-ui/label` (explicitly deferred per P-1 component boundary; treat as `web-only wrapper` until a later Form Field slice)

**App shell chrome:**
- Overall shell container, borders, and layout grid: `studio-local fixture`
- Creator / Orchestrator top tab strip: `studio-local fixture`
- Nav group labels + children (Works → All Works): `studio-local fixture`
- Nested child item with left accent bar: `studio-local fixture`
- Avatar stub + profile name / secondary text: `studio-local fixture`
- "+ Add profile" icon button: `studio-local fixture` (potential future small icon-button primitive, but not in first batch)
- Daemon status strip container (dot + text + Badge): `Badge` = `promoted primitive`; surrounding markup and layout = `studio-local fixture`
- Main content placeholder area: `studio-local fixture` (chrome demonstration only)

#### 4. Observable Acceptance Checklist for P1

A reviewer runs this list in the design-studio dev server (`pnpm --filter design-studio dev`, visit `/surfaces`) in both light and dark (`.dark` on `<html>`). No daemon or Tauri required. Each item must be verifiable without subjective debate.

- [ ] **Light theme:** All text, borders, fills, backgrounds, and accents use only registered tokens from DESIGN.md scales (`gray-*`, `background-*`, `blue-700`, `green-700`, `gray-alpha-*`, etc.). No raw `#hex`, no arbitrary `rgba()` outside DESIGN frontmatter, no one-off opacities.
- [ ] **Dark theme:** Primary actions use `brand-cyan` fill with `brand-deep-blue` text. All other colors come from the dark scales in DESIGN.dark.md. Toggling `.dark` does not shift layout, spacing, or overflow.
- [ ] **Keyboard focus visible:** Tab through every Button and the Browse control. Every focused element renders the two-layer focus ring (outer `blue-700` / inner `background-100` in light; outer `brand-cyan` in dark). No invisible focus, no focus trap, no missing ring on secondary actions.
- [ ] **Contrast passes:** `gray-1000` on `background-100` ≥ 18:1; `gray-700` secondary text ≥ 5.7:1 (light). Dark theme equivalents documented in DESIGN.dark.md pass AA. Badge "healthy" text on its background passes.
- [ ] **No unregistered token scale steps:** All spacing uses `space-*` or the mapped `setup-wizard-surface-*` / `setup-wizard-step-*` utilities from the preset. Widths, heights, radii, and shadows use registered tokens (`rounded-*`, `shadow-modal`, etc.). No `w-[248px]`, `min-h-[440px]`, `h-9`, `w-[2px]`, or other bracket/arbitrary values unless a corresponding DESIGN token + preset projection was added during this plan.
- [ ] **Hierarchy readable at a glance:** Setup title is `heading-24`; body is `copy-16`; labels are `label-14`. In shell, active tab/nav group is immediately locatable by bar + background change. No two levels use the same size/weight.
- [ ] **CTAs / affordances findable:** The "Continue" primary action is the strongest visual target inside the setup card. "Browse…" is clearly subordinate. In shell, the active Creator tab and the expanded "Works" group are the dominant navigation targets.
- [ ] **Daemon-free + boundary clean:** The page renders cleanly at the studio URL with zero console errors about missing providers, clients, or contracts. No imports from `apps/web/src/pages/**`, `apps/web/src/components/layout/**`, `apps/web/src/lib/nexus/**`, `@42ch/nexus-contracts`, or any Tauri module. All data is static fixture arrays defined in the file.
- [ ] **Copy alignment:** All headings, body, labels, and button text follow DESIGN.md Voice & Content rules (Title Case for titles and primary actions; sentence case for helpers; author-facing nouns; no trailing periods on status lines).

P1 is complete only when a reviewer can check every item above as passing in both themes on the updated surfaces, with the surfaces remaining within the Stage 1A studio-local fixture boundary.

### Stage 3 — Promotion Decision

For each stable component:

- If it is reusable and pure presentational, promote to `@42ch/nexus-ui`.
- If it is app-specific but reusable within web only, keep it in `apps/web`.
- If it is a one-off surface fixture, keep it in `apps/design-studio`.
- If missing tokens block consistency, update root DESIGN pair through the plan before implementing raw values.
- If the decision is deferred, record the owner, trigger, and missing evidence.

V1.99 draft recommendation:

- Promote first: `Button`, `Badge`, and `Card`.
- Defer by default: `Input`, `Label`, and `Textarea` until a Form Field slice locks label/control/helper/error composition across at least two Web consumers and one Studio fixture.
- Keep shells, steppers, setup rows, daemon status strips, nav groups, and page sections out of `@42ch/nexus-ui`.

### Stage 4 — Web Integration

After visual acceptance:

- Replace static fixture data with `apps/web` data and behavior.
- Keep app state, routing, and daemon transport in `apps/web`.
- Preserve the same primitive imports where promoted.
- Prefer thin app-local wrapper/re-export files when they reduce churn or attach app-specific behavior; wrappers must point inward to `@42ch/nexus-ui`, never the other way around.
- Add integration tests around behavior that Design Studio could not cover.
- Document any intentional divergence from the studio fixture so future contributors do not treat it as drift.

## Evidence Template

Use this template during P0/P1/P2 rather than relying on memory:

| Evidence | Required answer |
| --- | --- |
| Component or surface | What was composed in Design Studio? |
| Studio acceptance | What light/dark/focus/structure checks passed? |
| Promotion decision | `promote`, `keep-web`, `keep-studio`, or `defer` |
| Package path | If promoted, what public `@42ch/nexus-ui` import path is used? |
| Web integration | Which `apps/web` consumer adopted it, which wrapper/direct import was used, or why not? |
| Boundary check | Which forbidden imports were checked absent? |
| Remaining caveat | What must be revisited before treating this as a durable pattern? |

## V1.99 Validation Evidence (Captured)

This section records actual outcomes from P0 (Component Promotion) and P1 (Surfaces Visual Direction) to ground P2 codification in evidence. Data sourced from plan gate summaries, QA acceptance reports, and post-edit source inspection (2026-07-08).

### 1. Component Promotion Evidence (P0)

**For Button, Badge, Card** (first-batch primitives per P-1 boundary):

| Evidence | Value |
| --- | --- |
| Component or surface | Button, Badge, Card composed/validated in Design Studio via `components.tsx`, `surfaces.tsx`, `voice.tsx` (all import directly from `@42ch/nexus-ui`). |
| Studio acceptance | Build: 5/5 green (package + web + design-studio). Tests: 43/43 (package), 548/548 (web), 11/11 (studio). Visual/behavioral parity: identical class output and `cn` semantics to pre-promotion; contrast invariants verified in web re-export tests. |
| Promotion decision | `promote` (all three). |
| Package path | Named root exports from `@42ch/nexus-ui`: `export { Button, type ButtonProps } from './components/button';` (similar for Badge, Card + sub-primitives). See `packages/nexus-ui/src/index.ts`. |
| Web integration | Thin re-export wrappers in `apps/web/src/components/ui/` (e.g., `button.tsx`: 9-line `export { Button, type ButtonProps } from '@42ch/nexus-ui';`). App-specific behavior/tests remain in web. |
| Boundary check | Zero forbidden imports verified: `grep -r "@web-ui/button|@web-ui/badge|@web-ui/card" apps/design-studio/src/` → NO MATCHES. No variant helpers (`buttonVariants`) leaked to web. |
| Remaining caveat | `Input`, `Label`, `Textarea` deferred to Form Field slice (explicitly not promoted). |

### 2. Surfaces Visual Iteration Evidence (P1)

**For /surfaces redesign** (SetupWizardFixture + AppShellFixture + DaemonStatusStrip):

| Evidence | Value |
| --- | --- |
| Component or surface | Setup wizard step card (SetupWizardFixture) and App shell chrome (AppShellFixture) composed as studio-local View fixtures in `apps/design-studio/src/pages/surfaces.tsx`. |
| Studio acceptance | 9/9 observable checklist items PASS in both themes (verified by QA): light/dark intentional, keyboard focus visible, no unregistered tokens (except documented fixture-local `min-h-[420px]`/`min-h-[440px]`), hierarchy readable, CTAs findable, daemon-free, no forbidden imports, copy aligned with DESIGN.md Voice & Content. Build green; 23/23 tests pass. |
| Promotion decision | N/A (surfaces are fixtures, not primitives). Setup card elements classified per guide: `Card`/`Button` = promoted; stepper/workspace row = studio-local fixture; Label = transitional `@web-ui/label`. |
| Package path | N/A for fixtures. Promoted primitives inside use `@42ch/nexus-ui` root exports. |
| Web integration | None (surfaces remain studio-only per Stage 1A boundary). Future web integration would replace static data with real behavior while preserving primitive imports. |
| Boundary check | Zero forbidden imports: actual imports limited to `Badge, Button, Card` from `@42ch/nexus-ui` and `Label` from `@web-ui/label` (transitional). Grep confirmed no `apps/web`, `@42ch/nexus-contracts`, tauri, `invoke`, `NexusClient` (except explanatory comment string). |
| Remaining caveat | `@web-ui/label` usage is transitional for deferred Label (Form Field slice); fixture-local min-heights documented as intentional exceptions. |

### 3. Workflow Failures or Caveats (P0/P1)

- **Scope exceedance (acceptable):** P0 T1 implementer created full component implementations in `packages/nexus-ui` rather than minimal stubs. This was functionally correct and accepted (APIs identical), but deviated from "move or reimplement" minimal intent. Noted for future slice discipline.
- **QC residual (R-V199P1QC1-S001):** `@web-ui/label` import in `surfaces.tsx` lacks explicit transitional annotation comment (QC1). Matches plan expectation of transitional use but annotation missing.
- **Duplication (post-V1.99):** `cn` helper is byte-identical between `packages/nexus-ui/src/lib/cn.ts` and `apps/web/src/lib/utils.ts` (with duplicated `extendTailwindMerge` config). QC residuals R-V199QC1-S001 / R-V199QC3-F001 defer consolidation to `@nexus/design-tokens` or similar.
- **No other failures:** All builds/tests green, boundaries clean, visual parity confirmed, QA gate PASS for both plans.

This evidence directly informs T2–T4 codification (AGENTS.md updates, transitional policy enforcement, etc.).

### 4. Post-Validation Decisions

#### Knowledge Promotion Decision

Confirmed: knowledge promotion is **deferred to iteration-close** (`mstar-compound`). The studio-first workflow pattern is a candidate for compound promotion, conditional on proving reusable across future Nexus UI iterations (not just V1.99). No `.mstar/knowledge/**` content is created during this iteration's execution phase.

#### Skill Decision

Confirmed: **No skill creation.** The `mstar-skill-authoring` purpose test returns negative — the workflow is repo-specific (Nexus Design Studio → `@42ch/nexus-ui` → `apps/web`) and has not proven reusable beyond this repo. Better captured in AGENTS.md guidance + iteration-close knowledge. The locked default stands; neither override condition (external reuse evidence, PM approval) is met.

## Durable Landing Options

At iteration-close, choose the lightest durable surface that matches what V1.99 proves:

| Output | Use when |
| --- | --- |
| `apps/design-studio/AGENTS.md` / `apps/web/AGENTS.md` / `packages/nexus-ui/AGENTS.md` | The rule is repo/package-specific |
| `.mstar/knowledge/architecture-patterns/*.md` | The pattern is reusable across future Nexus UI iterations and is promoted during iteration-close |
| Possible skill proposal | The workflow has been validated, is reusable across projects/roles, and passes the `mstar-skill-authoring` purpose test |

Do not create a new skill just to memorialize this iteration. Skill work is a later conditional option, not an automatic output: the workflow must first prove reusable beyond this repo and not duplicate existing Morning Star behavior.

## V1.99 Validation Targets

- At least one promoted component follows the full Studio → Package → Web path.
- `/surfaces` demonstrates the studio-first visual iteration loop with setup and shell compositions.
- The final guidance identifies which rules belong in `AGENTS.md`, which become iteration-close knowledge input, and whether any skill proposal is justified.
- A negative skill decision is acceptable and preferred unless the workflow proves reusable beyond Nexus repo ownership boundaries.

## Workflow Landing Strategy (Locked)

**Purpose:** This section locks the pre-validation landing strategy so P2 (Studio-First UI Workflow Codification) has explicit decisions to execute against. P2 must follow these defaults; overrides require explicit evidence + PM/user approval.

### 1. AGENTS.md Guidance Scope

During the iteration (after P0/P1 validation), workflow rules land in the following `AGENTS.md` files. Rules added must be **repo/package-specific ownership-boundary rules** — not restatements of Morning Star harness process.

| Target AGENTS.md | Rule types |
|---|---|
| `apps/design-studio/AGENTS.md` | Studio composition rules (what belongs in studio vs. web), import boundary rules (which aliases are allowed/forbidden for studio-local fixtures), transitional `@web-ui/*` import policy, and the surface-element classification pattern (promoted primitive / studio-local fixture / web-only wrapper / future web product component) |
| `packages/nexus-ui/AGENTS.md` | Promotion criteria (what qualifies a component for package promotion), consumer wrapper rules (thin wrappers in apps must point inward to `@42ch/nexus-ui`), and the `cn` helper ownership rule |
| `apps/web/AGENTS.md` | Web integration rules (how accepted studio fixtures enter web with real data), thin-wrapper strategy for app-specific behavior, and the daemon-free boundary for presentational imports |

P2 must update these three files after validation. No other `AGENTS.md` files receive workflow rules from this iteration.

### 2. `@web-ui/*` Transitional Policy

- **Promoted primitives** (Button, Badge, Card after P0): both Studio and Web must import from `@42ch/nexus-ui`, not `@web-ui/*`. The `@web-ui/*` alias remains available for unpromoted primitives only.
- **Unpromoted primitives** (Input, Label, Textarea, and any future deferred component): remain on `@web-ui/*` and are documented as **transitional** — the dependency is temporary until a later slice promotes or explicitly removes them.
- **Tracking:** each unpromoted `@web-ui/*` import in Studio must be annotated with a comment identifying the blocking criteria for promotion (e.g., `// @web-ui/label — transitional until Form Field slice locks label/control/helper/error composition`).

### 3. Knowledge Promotion Timing

- **No `.mstar/knowledge/**` content** is created during iteration-start or execution. All workflow guidance lives in the iteration workspace (`{ITERATION_DIR}/v1.99/guides/studio-first-ui-workflow.md`).
- **Knowledge promotion at iteration-close** (`mstar-compound`) is **conditional** on the pattern proving reusable across future Nexus UI iterations (not just V1.99). If the pattern is V1.99-specific, it stays in the iteration archive.
- P2's validation evidence is the primary input for the compound decision at close.

### 4. Skill Decision (Locked Default)

Apply the `mstar-skill-authoring` purpose test pre-validation:

> Does this workflow describe a reusable behavior that applies across projects and roles, and does it add behavior not already covered by existing Morning Star skills?

**Default: do NOT create a new skill.** The workflow is Nexus-repo-specific and describes a project-level development path, not a cross-project agent behavior.

P2 may only override this default with:
1. Explicit evidence that the workflow has been validated and reused outside Nexus (at minimum one external project or contributor).
2. PM/user approval documented in the plan.

Until both conditions are met, the skill decision is **negative**.

### 5. Evidence Capture Hooks

P0/P1 evidence must be recorded in the guide's **Evidence Template** (see above) so P2 codifies from evidence, not aspiration. Specifically:

- **P0 (Component Promotion):** for each promoted primitive, fill the Evidence Template row (component, studio acceptance, promotion decision, package path, web integration, boundary check, remaining caveat).
- **P1 (Surfaces Visual Direction):** for each surface element classification, record the evidence row showing the studio → web path (or explicit non-promotion).
- **Evidence location:** evidence rows live in the iteration workspace guide (`studio-first-ui-workflow.md`). At iteration-close, the PM/mstar-compound evaluates whether evidence warrants knowledge promotion.

P2 must not invent workflow rules from first principles — it must read the evidence captured here and codify the rules that the evidence supports.
