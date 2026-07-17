# P2 Spec — App Surfaces Elevation

> Iteration: V1.121 “The Literary Engine”. Primary spec for plan `2026-07-17-v1.121-app-surfaces-elevation`.
> Compass: S3. **Must** — daily author path (shell → setup → Control Room); first impression and content-voice split live here. Desktop inherits via web SPA wrap (no Tauri chrome work).

## Problem statement

Shell and Control Room surfaces are production-functional but visually generic: the sidebar is a plain nav list, page headers are uniform sans, empty states are utilitarian, and the first-impression surfaces (setup wizard, connection setup) carry no brand atmosphere. An author launching Nexus for the first time meets an “installer,” not an atelier. Pages also carry the top arbitrary-value heat in the app (`findings-page.tsx`, `strategy-canvas/canvas-layout.tsx`, `connect-daemon-form.tsx`, `path-context-menu.tsx`, etc.). This plan elevates every **non-canvas** app surface to v0.4.

## User value

| Who | Outcome when P2 ships |
| --- | --- |
| Authors | Calm premium atelier from first launch through daily triage — literary titles, ink-dark shell, tactile chrome |
| Maintainers | Voice rules greppable both directions; arbitrary-value heatmap reduced to documented exceptions |
| Product | First-impression surfaces finally express brand atmosphere without new routes or copy rewrites |

## Scope (all non-canvas surfaces of `apps/web`)

1. **Shell chrome:** `RootLayout`, sidebar (tabs, collapsible groups, work items, footer profiles, settings link), header (logo, health indicator, theme toggle), `MainBanner`, `DaemonStatusBar`, command palette (⌘K overlay), `WorkRail` (non-canvas parts), mobile nav.
2. **First-impression:** setup wizard (3 steps + step indicator), connection setup form, fingerprint gate, splash/launch screens.
3. **Control Room pages:** Works, Worlds, Chapters (list), Sessions, Schedule, Modules, Findings, Memory (SOUL viz containers), Strategies list, Settings (all four tabs), NotFound.
4. **Cross-cutting states:** empty/error/loading states on every page (editorial **typography** polish per voice rules — no marketing copy rewrite).
5. **Token consumption/verification only (for P3):** `canvas-node-width-*` component tokens — registered in P0; **no canvas visual change in this plan**.

## Design intent

- **Content voice arrives.** Work/world/chapter titles and creative-entity headings use the display serif (`display-24`/`display-20`); page chrome (nav, tabs, table headers, buttons, badges, labels) stays sans. The split must be greppable and documented in DESIGN.md body.
- **Shell gains quiet brand identity:** sidebar active state gets the two-layer treatment (active bar `blue-700` + tinted wash from tokens); header keeps 56px restraint; brand mark treatment per `shell-nav` tokens; dark shell sits on ink surfaces (P0 T2).
- **Setup wizard becomes the handshake:** serif welcome headline, refined step indicator (existing tokens), card on subtle ambient background, form rows keep V1.96 geometry. First impression should read “atelier”, not “installer”. Flows and step order unchanged.
- **Command palette:** `bg-scrim` + `elevation-4`, refined item hover/selected, keyboard affordance hints; keep V1.111 ARIA behavior.
- **Banners/status bars:** semantic tint tokens (P0/P1), no new hues; `DaemonStatusBar` stays desktop-only.
- **Empty/error states:** display-serif headline + sentence-case helper + first action (per `states` tokens); error surfaces use P0 tokenized tints; **locale strings unchanged**.

## Arbitrary-value sweep (pages + non-ui components)

Convert to tokens or document exceptions:

- `pages/findings-page.tsx` (6: `max-w-[180px]`, `w-[150px]`, `w-[130px]`, `grid-cols-[minmax(0,1fr)_360px]`, …)
- `components/canvas/strategy-nodes.tsx` (3: `min-w-[260px]` root, `min-w-[140px]` primary, `min-w-[150px]` secondary), `components/canvas/outline-canvas/scene-beat-nodes.tsx` (`min-w-[160px]`), `components/canvas/presentational/node-chrome-shell.tsx` (`min-w-[176px]` default) — the `canvas-node-width-*` family under DESIGN.md **`components.canvas.node-width.<role>`** per the contract authored in P0 spec T5 (5 values: strategy-root 260px / strategy-primary 140px / strategy-secondary 150px / outline-scene-beat 160px / default 176px) is **registered in P0** (frontmatter + tokens.css `--canvas-node-width-*` structural vars + tailwind preset `minWidth` + twMerge `min-w` group); the tailwind utility prefix (`min-w-canvas-node-<role>`) is **fixed at P0 registration** so P3 consumes one name. P2 **verifies** the registered utilities resolve (grep/build evidence) while sweeping arbitrary values — **no re-registration, no canvas visual adoption** (P3 applies).
- `components/settings/connect-daemon-form.tsx` (3 × `text-[13px]` → `text-copy-13`)
- `components/canvas/outline-canvas/inspectors/chapter-outline-content-editor.tsx`, `scene-beat-nodes.tsx`, `strategy-canvas/canvas-layout.tsx`, `components/path-context-menu.tsx`, `pages/dialogs/validate-preset-dialog.tsx`

## Acceptance criteria

- AC-P2-1 — Shell chrome (sidebar/header/banner/status bar/footer/palette/work rail) consumes v0.4 tokens; active/hover/elevation states match DESIGN.md recipes in light + dark (screenshot evidence both themes).
- AC-P2-2 — Setup wizard + connection setup render the v0.4 first-impression treatment (serif welcome, ambient surfaces, refined step indicator) with V1.96 geometry preserved; wizard **flows, step order, and success criteria unchanged** (existing tests green).
- AC-P2-3 — Every Control Room page adopts content-voice typography where contracted (creative-entity titles serif; chrome sans) — grep evidence both directions (no serif in nav/tables/badges/buttons).
- AC-P2-4 — Empty/error/loading states on all pages use tokenized tints + editorial **typography** shape (serif headline allowed); no raw color-mix arbitrary classes remain in pages/non-ui components; i18n strings not rewritten for marketing.
- AC-P2-5 — Arbitrary-value sweep complete: listed files converted to tokens or documented exceptions; `text-[13px]` eliminated; the P0-registered `canvas-node-width-*` family (DESIGN.md `components.canvas.node-width.*` 5 keys per P0 spec T5 + tokens.css `--canvas-node-width-*` + tailwind preset `minWidth` + twMerge `min-w` group) is **consumed/verified** — the `min-w-canvas-node-*` utilities resolve and nothing is re-registered (no canvas visual adoption required here).
- AC-P2-6 — All existing page tests pass; new visual states covered by updated/added vitest cases only where behavior changed; `apps/web` typecheck + build green.
- AC-P2-7 — AA re-verified on ink dark surfaces for shell + page text pairings changed by P0 background shifts (spot-check table on plan).

## Non-goals

- No canvas surface visual elevation (P3); no reading-surface prose chrome migration (P3). Canvas work here is **token verification only** (registration shipped in P0).
- No new routes/features/IA; no settings IA changes (V1.103/V1.106 settled).
- No i18n/copy marketing rewrites; no desktop native Tauri chrome; no wire/daemon changes.

## Interfaces

- `apps/web/src/components/{layout,shell}/…` (sidebar, header, banner, status bar), `components/command-palette.tsx`, `components/work-rail*`.
- `apps/web/src/pages/**`, `apps/web/src/components/setup/**`, `components/settings/**`.
- `canvas-node-width-*` family: DESIGN.md `components.canvas.node-width.*` (keys + values authored in P0 spec T5; **registered in P0**; verified/consumed here; applied by P3).
- DESIGN.md v0.4 tokens (P0) + elevated components (P1).

## Validation plan

- Vitest suites, typecheck, build; light/dark screenshot evidence of shell + wizard + one dense page (Findings) + one empty state; grep sweeps for serif discipline and arbitrary values.
