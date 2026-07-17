# P3 Spec — Canvas & Reading Elevation

> Iteration: V1.121 “The Literary Engine”. Primary spec for plan `2026-07-17-v1.121-canvas-reading-elevation`.
> Compass: S4. **Must** — signature surfaces (画布 + 阅读); iteration is incomplete if only shell/components elevate. Depends P0 (tokens/mapping) + P1 (elevated chrome) + P2 (`canvas-node-width-*`).

## Problem statement

The canvas is Nexus's signature surface（画布 / 互联 / 计算引擎）, yet it is the most chromatically drifted and least tactile: node chrome is flat, edges are hairline gray, canvas token values carry Tailwind-palette leftovers, and per-surface identity is a single purple spine on Strategy only. Reading surfaces hold 40+ untokenized values and the product's only serif — hardcoded, theme-fragile. Authors spend their creative hours here; generic chrome undermines the Literary Engine story more than any Control Room list page.

This plan makes the canvas feel like an **instrument** and the reading surface like **literature**, completes studio canvas galleries, and runs the iteration-wide light/dark **parity + AA close-out** so “确保没问题” is evidence-backed — not vibes.

## User value

| Who | Outcome when P3 ships |
| --- | --- |
| Authors | Canvas feels like the product’s signature workspace; novel reading feels literary, not CSS-hacked |
| Maintainers | Chromatic drift eliminated; studio galleries verify every canvas token in both themes |
| QC/QA / product | Parity pack + AA table close AC-V1121-4/5/7; residual contrast/hygiene claims are greppable |

## Scope

1. **Three canvas surfaces** (`apps/web/src/components/canvas/**`): Strategy, Outline (+Scene/Beat), World KB.
   - **Ambient:** `canvas-shell` surface/grid/minimap/controls consume P0 ambient tokens; dot grid tuned per theme (size/gap/alpha); dark canvas sits on ink.
   - **Node chrome v2:** `NodeChromeShell` + per-surface nodes adopt the v0.4 elevation recipe (rest 1 → hover 2 → dragging 4), two-layer selection ring, refined border contrast, per-surface accent spines (strategy=purple-700, outline=amber-700, worldkb=teal-700) as tokens; node widths consume the tokens registered in P2. **Note:** `NodeChromeShell` (`apps/web/src/components/canvas/presentational/node-chrome-shell.tsx`) already exposes `accent?: boolean` (currently drives `border-l-canvas-strategy-accent`); P3 extends this single prop to a per-surface accent spine — chosen shape (e.g. `accent?: boolean | 'strategy' | 'outline' | 'worldkb'`) is a P3 design decision; the extension is **additive** (existing `accent={true}` call sites continue to render the strategy accent for backward compatibility). Studio fixtures consume `NodeChromeShell` directly with static props, so the gallery is the verification surface.
   - **Edges:** edge/edge-hover/port colors per v0.4; relationship-edge typing colors remapped per P0 hygiene table (hue-preserving); arrowheads/hover affordance refined.
   - **Inspectors/panels/conflict modals on canvas:** consume elevated popover/dialog tokens.
   - **Command palette on canvas:** inherits P2 palette work; verify on canvas context (no second palette implementation).
2. **Reading surfaces** (`apps/web/src/components/reading/**`, chapter page, annotations):
   - `index.css` reading-chrome block → **token-only** consumption per P0 T6 contract; novel chapter title → display serif token (replaces hardcoded Georgia).
   - Essay/game-bible/script profiles keep their identity but consume tokens; annotation highlights/inspector/selection toolbar consume v0.4 tints + elevation.
   - Chapter nav + progress indicator consume elevated tokens.
3. **Design-studio canvas galleries:** update `canvas-surfaces-fixtures` to v0.4 node/edge/ambient states; add canvas token gallery (surface/grid/node/edge/minimap values, light + dark). Completes compass AC-V1121-5 with P0 token galleries.
4. **Iteration-wide parity + AA close-out (“确保没问题”):**
   - Screenshot pack: Strategy + Outline + WorldKB + Reading × light + dark.
   - AA spot-check table for remapped canvas chromatics + ink-surface text pairings.
   - Grep: zero Tailwind-palette leftover hexes in canvas tokens **and** canvas component source; reading-chrome CSS has no hardcoded font/size/color.
   - No open BLOCKING residuals on contrast or chromatic hygiene at plan Done (compass AC-V1121-7).

## Acceptance criteria

- AC-P3-1 — All three canvases render ambient tokens (surface/grid/minimap/controls) with per-theme tuning; dark canvas is ink, not neutral flip (screenshot evidence).
- AC-P3-2 — Node chrome v2 shipped on all node types (state/group/join/terminal/inner, volume/chapter/scene/beat/timeline, entity/relationship): elevation states, two-layer selection ring, per-surface accent spines from tokens; zero hardcoded hex in canvas components (grep).
- AC-P3-3 — All `components.canvas.*` token values conform to brand semantic scales per the P0 mapping table — zero Tailwind-palette leftover hexes in canvas tokens and canvas component source (grep evidence).
- AC-P3-4 — Reading chrome consumes tokens exclusively (no hardcoded font/size/color in the CSS block); novel chapter titles render the display serif in light + dark (screenshot + computed-style evidence).
- AC-P3-5 — Annotation/inspector/toolbar/progress surfaces consume v0.4 tokens; highlight tints AA-checked for text legibility in both themes.
- AC-P3-6 — Studio canvas galleries updated (node states, edges, ambient) + new canvas token gallery renders values in both themes; studio build + tests green.
- AC-P3-7 — **Parity + AA close-out:** light + dark evidence pack for Strategy, Outline, WorldKB, Reading; AA spot-check table for remapped chromatics + ink text pairings recorded on plan; satisfies compass AC-V1121-7.
- AC-P3-8 — `apps/web` typecheck + vitest + build green; canvas interaction tests (existing) pass with class-only changes expected (no behavior deltas).

## Non-goals

- No canvas interaction/model changes (no new node types, no layout engine work, no React Flow upgrade).
- No performance work beyond avoiding regressions (no virtualization rewrite).
- No manuscript editing features; reading surfaces stay read-only per V1.79 boundary.
- No i18n/copy rewrites; no shell/Control Room rework (P2); no wire/daemon/desktop-native changes.

## Interfaces

- `apps/web/src/components/canvas/**` (shell, node-chrome-shell, strategy/outline/worldkb nodes + edges + inspectors), `components/reading/**`, `index.css` reading-chrome block.
- `apps/design-studio/src/fixtures/canvas-surfaces-fixtures.tsx`, studio pages.
- DESIGN.md v0.4 `components.canvas.*`, `reading-chrome-*`, elevation/motion tokens (P0); elevated dialog/popover components (P1); width tokens (P2).

## Validation plan

- Existing canvas vitest + interaction suites; grep sweeps (hex leftovers, hardcoded reading values); screenshot parity pack (3 canvases + reading, light + dark); AA spot-checks; studio gallery evidence.
