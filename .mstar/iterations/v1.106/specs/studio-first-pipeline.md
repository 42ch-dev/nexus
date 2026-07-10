# Studio-First Pipeline (V1.106 P0)

**Status:** Draft — writing-complete (§5.3); PM lock pending §5.4  
**Tier:** Must (P0) — iteration incomplete if missing  
**Plan:** `2026-07-10-v1.106-studio-first-pipeline`  
**Compass:** `../v1.106-delivery-compass.md`

## Product outcome

Authors and contributors see the same daemon-launch, banner, toast, and form chrome in Design Studio and the real App because every author-facing surface on the setup → settings → control-room path is designed in Studio against DESIGN.md **before** App wiring claims.

**User-visible win:** No more “Studio stub vs App truth” drift for launch splash, degraded daemon banner, toast feedback, or undocumented Tabs/States/form-field patterns.

## Done definition (locked)

Pipeline is **complete** when each author-facing chrome path below has all three:

1. **DESIGN.md contract** — component table row and/or `###` prose section with tokens, states, and Voice & Content examples where copy is normative.
2. **Design Studio fixture** — Surfaces route or `/components` matrix demonstrating variants in light + dark.
3. **Promotion decision** — explicit `promote` | `keep-web` | `keep-studio` | `defer` recorded in the V1.99 promotion-boundary doc or iteration notes.

**Not required for Done:** moving Dialog / Tabs / Table / States into `@42ch/nexus-ui`.

## Scope paths (author-facing chrome)

| Path | App owner | DESIGN + fixture target |
|------|-----------|-------------------------|
| Desktop launch wait / error | `daemon-ready-splash.tsx` | Launch & daemon status § |
| Control Room degraded daemon | `main-banner.tsx` | Launch & daemon status § |
| Inline feedback | Toast (`useToast` + renderer) | Toast matrix in Studio `/components` |
| Settings / wizard forms | Form Field composition (V1.100) | Form Field § cross-ref |
| Settings tabs / empty-error states | `tabs.tsx`, `states.tsx` (keep-web) | Tabs §, States § |

## SP-V1106-001 — Process + doc lock

**User-visible outcome:** Future UI Assignments cite one studio-first invariant; promotion-boundary doc matches reality (Input + Select promoted; Dialog/Tabs/Table/States keep-web).

### Acceptance

- [ ] `.mstar/iterations/v1.99/specs/component-promotion-boundary.md` non-promotion table lists Input and Select as **promoted** (post-V1.100/V1.101); Dialog, Tabs, Table, States remain **keep-web** with rationale unchanged.
- [ ] `apps/design-studio/AGENTS.md` no longer says “V1.99-approved primitives only” where Select/Input are already promoted.
- [ ] `packages/nexus-ui/src/index.ts` header comment reflects current promotion scope (not “V1.99-only”).
- [ ] `.mstar/iterations/v1.106/guides/studio-first-invariant.md` and compass **Scope** repeat the locked invariant verbatim.
- [ ] Residuals `R-V1101P2-001`, `R-V1101P2-002`, `R-V1101P2-005` closed or re-targeted to V1.106 with evidence in plan gate summary.

## SP-V1106-002 — DESIGN.md SSOT gaps

**User-visible outcome:** Contributors can look up Tabs, States, Form Field composition, and Launch/daemon chrome in DESIGN.md without reading App source.

### Required sections (root `DESIGN.md` + `DESIGN.dark.md` when dual-theme values differ)

#### `components.tabs` + `### Tabs`

- List/tab trigger states: default, hover, active, disabled, focus-visible.
- Keyboard: arrow navigation between triggers; roving tabindex or documented equivalent.
- Voice: Title Case tab labels in examples.

#### `components.states` + `### States`

- Document `Spinner`, `EmptyState`, `ErrorState` (keep-web) with token columns.
- ErrorState: Title Case heading example; sentence-case helper; **Try again** action label.
- EmptyState: accepts `action` slot — document host-owned CTA pattern.

#### `### Form Field (composition)`

- Cross-reference `.mstar/iterations/v1.100/specs/form-field-contract.md` (locked).
- Prose: package owns Input/Label/Textarea primitives; app owns helper/error/required copy and `aria-describedby` wiring.
- At least one composition diagram or bullet list showing label → control → helper → error order.

#### `### Launch & daemon status`

Cover these surfaces with token tables and copy examples:

| Surface | States to document | Example copy (Voice & Content) |
|---------|-------------------|--------------------------------|
| DaemonReadySplash — waiting | loading | Title: **Starting daemon…**; helper: *This takes a few seconds on first launch.* |
| DaemonReadySplash — error | error + retry | Title: **Daemon not ready**; primary: **Restart Nexus** |
| DaemonReadySplash — recovery | reset local DB | Tertiary: **Reset local database**; helper (sentence case): *This will clear the daemon's local state database (config, registry cache). Your creative files in the workspace are not affected.* |
| MainBanner | starting, degraded, stopped, error | Title examples: **Daemon reconnecting**, **Daemon stopped**, **Port unavailable**; CTA: **Restart Daemon** |
| Status bar / footer strip | running (icon-only) | Cross-ref `web-ui.md` — no duplicate normative copy unless tokens change |

### Acceptance

- [x] All four section groups above exist in DESIGN.md with component tables and/or prose as specified (§5.3 writing — normative prose landed in root `DESIGN.md` + dark frontmatter parity).
- [x] Dark theme parity where semantic colors differ (`DESIGN.dark.md`).

## SP-V1106-003 — Studio Surfaces fixtures

**User-visible outcome:** Authors preview launch splash, degraded banner, and toast variants in Design Studio before those patterns ship in the App.

### Fixture file layout (locked §5.2)

| File | Route | Import strategy | `data-testid` |
|------|-------|-----------------|---------------|
| `apps/design-studio/src/fixtures/launch-daemon-fixtures.tsx` | `/surfaces/launch` | `@web-setup/daemon-ready-splash` (presentational App module) | `surfaces-launch` (page), `daemon-ready-splash` (per variant mount) |
| `apps/design-studio/src/fixtures/main-banner-fixtures.tsx` | `/surfaces/banner` | **Composition-only** — props-driven chrome replicating `main-banner.tsx` visuals; **no** App import (daemon/desktop hooks forbidden in Studio) | `surfaces-banner`, `main-banner-fixture-{starting\|degraded\|stopped\|error}` |
| `apps/design-studio/src/fixtures/toast-fixtures.tsx` (or inline `/components` section) | `/components` Toast section | `@42ch/nexus-ui` Toast primitives + existing renderer pattern | `toast-matrix`, `toast-variant-{success\|error\|warning\|info}` |

Register `/surfaces/launch` and `/surfaces/banner` in `SURFACES_SECTIONS` (`surfaces.tsx`) and nested routes in `App.tsx`.

### Fixtures (props-driven; no daemon IPC in Studio)

| Route | Fixture | Minimum variant matrix |
|-------|---------|------------------------|
| `/surfaces/launch` | DaemonReadySplash | `waiting` · `error` (+ **Restart Nexus**) · `error` + `resetLocalDatabase` |
| `/surfaces/banner` | MainBanner chrome (composition-only) | `starting` · `degraded` · `stopped` · `error` (incl. port-conflict copy path) |
| `/components` (Toast section) | Toast renderer | `success` · `error` · `warning` · `info` with title + optional description |

### keep-web DESIGN cross-reference (Tabs / States)

Tabs and States DESIGN sections document `apps/web/src/components/ui/tabs.tsx` and `states.tsx` as **keep-web** owners. Future promotion slice may extract presentational layers only after Form Field + empty/error patterns are fixtured — **out of V1.106 scope**.

### Acceptance

- [ ] Each route renders all listed variants; light/dark toggle passes visual review checklist in studio-first guide.
- [ ] Studio Vitest smoke covers new fixture mounts (`surfaces-launch`, `surfaces-banner`, toast matrix testids).
- [ ] Fixtures import shared primitives per promotion boundary (`@42ch/nexus-ui` where promoted; `@web-ui/*` only for keep-web with transitional comment).
- [ ] MainBanner fixture remains composition-only — no `main-banner.tsx` extract in V1.106.

## Non-goals (locked)

- Package-promoting Dialog, Tabs, Table, or States this iteration
- Promoting AgentPicker or SettingsShell into `@42ch/nexus-ui`
- DF-70 execution-mode / BYOK matrix
- Wire / schema changes (`wire_contracts_changed: false` unless architect proves unavoidable)
- Radix Select rewrite

## Architecture locks (§5.2)

See compass **Architecture Locks** — fixture paths, composition-only MainBanner, keep-web Tabs/States, `wire_contracts_changed: false`.

## Verification hooks

- `pnpm --filter design-studio test` green for new Surfaces tests.
- Promotion-boundary doc + AGENTS cross-links consistent with `tooling/check-ui-guardrails.sh`.
- Compass acceptance checkboxes satisfied before P0 plan Done.
