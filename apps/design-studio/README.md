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
| **Tokens** | `/tokens` | Colors (brand, gray, blue, red, amber, green, teal), typography scale, spacing, radius, elevation |
| **Brand VI** | `/brand` | All four `@42ch/nexus-ui` logo variants + `NexusMark` + theme.css swatches + clear-space guidance |
| **Components** | `/components` | All 11 `apps/web/src/components/ui/*` primitives with variant/state matrices — promoted primitives (Button, Badge, Card, Input, Label, Textarea, Select) imported from `@42ch/nexus-ui`; unpromoted remain on `@web-ui/*` (Dialog, States, Table, Tabs) |
| **Voice & Content** | `/voice` | Labeled writing-pattern specimens from `DESIGN.md` §Voice & Content |
| **Surfaces** | `/surfaces` | Setup wizard, App shell, AgentPicker, Settings shell chrome fixtures (studio-local; no daemon data) |

Every value is driven by the repo-root `DESIGN.md` / `DESIGN.dark.md` SSOT.
Edit those files in your IDE, then refresh the studio to see the effect.

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
class on `<html>` (Tailwind `class` strategy). No localStorage persistence in
V1.98 — the toggle is session-scoped.

## Token-tuning workflow

1. **Open studio** — `pnpm --filter design-studio dev`
2. **Baseline** — toggle light/dark; scan token tables and component matrix
3. **Edit SSOT** — change values in root `DESIGN.md` / `DESIGN.dark.md`
4. **Refresh** — reload studio (HMR picks up CSS variable changes automatically)
5. **Validate** — confirm Brand VI, Components, Voice, and Surfaces still look correct in both themes
6. **Verify product** — run `pnpm --filter web test` and `pnpm --filter web run build` to ensure `apps/web` consumers still resolve tokens

## Commands

| Action | Command |
| --- | --- |
| Dev server | `pnpm --filter design-studio dev` |
| Build | `pnpm --filter design-studio build` |
| Test | `pnpm --filter design-studio test` |

No daemon or Tauri required for any command.

## Architecture

- **CSS pipeline**: shared `@nexus/design-tokens` workspace package (`tooling/design-tokens`) — Tailwind preset + `tokens.css`
- **Import surface**: `@42ch/nexus-ui` (promoted primitives: Button, Badge, Card, Input, Label, Textarea, Select + brand layer + `cn`), `@web-ui/*` (transitional unpromoted: Dialog, States, Table, Tabs), `@nexus/design-tokens` (CSS + preset)
- **Toolchain**: Vite 6 + React 18 + TypeScript strict + Tailwind 3 + Vitest 3 — mirrors `apps/web`
- **Boundaries**: no daemon transport, no `NexusClient`, no `@42ch/nexus-contracts`, no product-page imports

For detailed rules see [`AGENTS.md`](./AGENTS.md).

## Docs

- [Studio spec](../../.mstar/specs/design-studio.md) — product contract, audiences, boundaries
- [IA guide](../../.mstar/iterations/v1.98/guides/design-studio-information-architecture.md) — gallery section design
- [Design unification spec](../../.mstar/iterations/v1.98/specs/design-unification.md) — merge rules (architect-owned)
- [DESIGN.md SSOT](../../DESIGN.md) — sole normative design token source
