# Nexus Design Studio

A standalone read-only visual gallery for the Nexus DESIGN SSOT, brand VI, and
UI primitives. Built as a Vite + React SPA — no daemon, no Tauri, no
`@42ch/nexus-contracts` required.

## Quick start

```bash
pnpm --filter design-studio dev
```

Opens at **http://localhost:5174** (port 5174; avoids collision with `apps/web`
on 5173).

## What it shows

| Section | Route | Content |
| --- | --- | --- |
| **Tokens** | `/tokens` | Full scalar inventory: brand core/extended/alpha + blue-1100, backgrounds, gray solid/alpha, semantic hues, scrim, state surface fills/borders, finding/memory/reading/badge status families, typography (incl. reading metrics + bilingual), spacing, radius, elevation + aliases, motion/easing/reduced-motion, disabled wash, canvas/soul/layer/pin/width families, and structural scalars (footer/setup/sidebar/dialog/sheet/reading-chrome) |
| **Brand VI** | `/brand` | All five `@42ch/nexus-ui` logo variants + square plates + `NexusMark` + theme.css swatches + clear-space guidance + four historical `NexusLogoVariant` specimens + VI acceptance fixtures. Frozen assets and historical palettes are explicitly labeled references, separate from live theme tokens. |
| **Components** | `/components` | All 11 `apps/web/src/components/ui/*` primitives with variant/state matrices — promoted primitives (Button, Badge, Card, Input, Label, Textarea, Select, Tabs, Toast, TransportErrorBlock, RunFormFields, EntityPickerField, ProposalSections, RunStatusBadge, RunsTable) imported from `@42ch/nexus-ui`; unpromoted remain on `@web-ui/*` (Dialog, States, Table) |
| **Voice & Content** | `/voice` | Labeled writing-pattern specimens from `DESIGN.md` §Voice & Content |
| **Surfaces** | `/surfaces` | §7.5 fixture families on nested routes: Setup wizard; Shell (Chronos titlebar, app shell, dual-pane IA, Creator/Orchestrator IA, Creator shell, Settings shell, Footer profiles, Header health); AgentPicker states; Canvas (mirrored chrome, Mental Surfacing, NLE/World/Work/Global timelines, Layer Breadcrumb, Conflict Modals); Daemon status strip; Launch splash; Selection Submenu (six variants) — studio-local composition, no daemon data |

Every value is driven by the repo-root `DESIGN.md` / `DESIGN.dark.md` SSOT.
Edit those files in your IDE, then refresh the studio to see the effect.

## Discovery and comparison (v1.187)

- **Section index** — each gallery exposes a labeled filter over route headings,
  keywords, and import provenance. Keyboard: Arrow keys move results, Enter
  selects, Escape clears.
- **Pair view** — on Tokens, Brand, Components, Voice, and every Surfaces nested
  page, toggle **Compare** to render two same-origin iframe documents
  (`?studio-embed=light|dark`) side by side (stacked below 1024px). Each frame
  owns its own theme, portals, and focus — not a nested `.dark` wrapper in the
  parent document.
- **Read-only SSOT** — edit repo-root `DESIGN.md` / `DESIGN.dark.md`, then
  refresh; the dev server re-projects tokens in memory.

## Import provenance (four categories)

| Category | Pattern | Meaning |
| --- | --- | --- |
| Promoted primitive | `@42ch/nexus-ui` | Package export after Studio acceptance |
| App presentational extract | Recognized `@web-*` roots only: `@web-layout`, `@web-canvas`, `@web-setup`, `@web-settings`, `@web-global-timeline`, `@web-shell` (exact root or `/` subpath) | Props-driven `apps/web` chrome — not every `@web-*` alias |
| Transitional primitive | `@web-ui/*` | Unpromoted `components/ui/*` mirror |
| Studio-local | `@/fixtures/*`, `@/pages/*`, `@/components/*`, `@/lib/*` | Gallery composition only |

Surfaces nested routes: Overview, Setup, Shell, AgentPicker, Canvas, Daemon,
Launch, Selection Submenu — see `.mstar/specs/design-studio.md` §6–§7.


## Visual review (surfaces)

The `/surfaces` page is the primary visual-review target for product-surface
decisions before they enter `apps/web`:

1. Open studio at `/surfaces` in both light and dark themes.
2. Verify that every color, border, background, and accent uses only registered
   DESIGN token scale steps — no raw hex, no arbitrary bracket values.
3. Verify keyboard focus is visible on all interactive elements (Tab through
   every button — each should show the two-layer focus ring).
4. Verify text hierarchy is readable at a glance: heading-24 titles, copy-16
   body, label-14 UI labels.
5. Verify CTAs are findable — the primary action should be the strongest visual
   target in its area.
6. Verify type contrast passes in both themes (see DESIGN.md and
   DESIGN.dark.md §Contrast review).

## Light / dark toggle

The theme toggle in the header switches between `DESIGN.md` (light) and
`DESIGN.dark.md` (dark) values. The active theme is reflected in the `.dark`
class on `<html>` (Tailwind `class` strategy). The last-selected theme persists
in `localStorage` under `nexus-studio-theme` (`light` / `dark` / `system`),
defaulting to the OS `prefers-color-scheme`.

## Frozen assets vs live theme tokens

On `/brand`, install-identity logo assets (`logo-primary.svg`,
`logo-white-bg.svg`, square plates) are **frozen baked-gradient references** —
they are labeled as installed/historical and are never presented as the active
cobalt palette. The four `NexusLogoVariant` palettes are explicitly marked
**historical geometry/pigment references**. Live theme tokens (`--nexus-brand-*`,
`--color-brand-*`, `NexusMark` `currentColor`) render under the current document
theme (light or dark) and re-resolve on theme flip. Frozen cyan pixels are not
claimed to equal `brandColors.cyan`, and assets are never recolored with CSS
filters.

## Token-tuning workflow

`DESIGN.md` / `DESIGN.dark.md` are the sole token SSOT. Editing them drives the
gallery and the in-memory dev CSS; checked-in generated artifacts are written
only by the explicit generate command below.

1. **Open studio** — `pnpm --filter design-studio dev` (port 5174; `apps/web`
   stays on 5173)
2. **Baseline** — toggle light/dark; scan token tables, Brand, Components,
   Voice, and Surfaces
3. **Edit SSOT** — change a value in root `DESIGN.md` or `DESIGN.dark.md`
4. **Refresh (dev/projection only)** — the Studio dev plugin re-reads both
   DESIGN files in memory and re-projects tokens.css into the running app:
   editing either triggers a full reload so computed-value labels and CSS both
   reflect the change. A malformed YAML surfaces a Vite error overlay instead
   of silently keeping the last-good tokens. **This does not touch the
   checked-in generated files.**
5. **Regenerate checked-in outputs** — only the compiler writes the shared
   artifacts. After DESIGN edits, run exactly:
   ```bash
   pnpm --filter @nexus/design-tokens generate   # writes tokens.css + theme.css + generated-brand.ts
   pnpm --filter @nexus/design-tokens check      # verifies no drift / parity
   ```
   Checked-in generated outputs are committed separately from the in-memory
   dev projection.
6. **Validate** — confirm Brand VI, Components, Voice, and Surfaces still look
   correct in both themes
7. **Verify product** — run `pnpm --filter @42ch/nexus-ui build`, then
   `pnpm --filter web typecheck` and `pnpm --filter web build` to ensure
   `apps/web` consumers still resolve tokens (no web source edits)

## Commands

| Action | Command |
| --- | --- |
| Dev server | `pnpm --filter design-studio dev` |
| Build | `pnpm --filter design-studio build` |
| Test | `pnpm --filter design-studio test` |

No daemon or Tauri required for any command.

## Architecture

- **CSS pipeline**: shared `@nexus/design-tokens` workspace package (`tooling/design-tokens`) — Tailwind preset + `tokens.css`
- **Import surface**: `@42ch/nexus-ui` (promoted primitives: Button, Badge, Card, Input, Label, Textarea, Select, Tabs, Toast, TransportErrorBlock, RunFormFields, EntityPickerField, ProposalSections, RunStatusBadge, RunsTable + brand layer + `cn`), `@web-ui/*` (transitional unpromoted: Dialog, States, Table), `@nexus/design-tokens` (CSS + preset)
- **Toolchain**: Vite 6 + React 19 + TypeScript strict + Tailwind 3 + Vitest 3 — mirrors `apps/web`
- **Boundaries**: no daemon transport, no `NexusClient`, no `@42ch/nexus-contracts`, no product-page imports

For detailed rules see [`AGENTS.md`](./AGENTS.md).

## Docs

- [Studio spec](../../.mstar/specs/design-studio.md) — normative target contract, audiences, boundaries, gallery inventory
- [DESIGN.md SSOT](../../DESIGN.md) — sole normative design token source (light/default)
- [DESIGN.dark.md](../../DESIGN.dark.md) — dark companion with identical token paths
- [Import boundaries and conventions](./AGENTS.md) — Studio-specific agent rules
