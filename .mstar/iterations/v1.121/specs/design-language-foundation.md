# P0 Spec — Design Language Foundation (DESIGN.md v0.4 + token pipeline)

> Iteration: V1.121 “The Literary Engine”. Primary spec for plan `2026-07-17-v1.121-design-language-foundation`.
> Compass: S1. **Must** — unblocks P1–P3; without a locked v0.4 contract the elevation cascade has no single truth source.

## Problem statement

The design system is mechanically complete (Level 3) but expressively flat — authors experience a capable tool that still *looks* like a generic developer dashboard:

1. **No literary voice.** Every glyph is system sans; the only serif is a hardcoded `Georgia` in `apps/web/src/index.css` reading chrome. A creative-writing product has no typographic identity.
2. **No atmosphere.** Dark theme is a neutral mechanical flip (`#0a0a0a`/`#111`/`#1a1a`) — generic, indistinguishable from any dev tool; the brand's ink-blue depth (`brand-deep-blue-1000 #0C1A2B`) is unused for surfaces.
3. **No depth.** Three flat shadows; interactive surfaces (cards, canvas nodes) have no hover/press elevation story.
4. **Chromatic drift on canvas.** Canvas token values reference Tailwind-palette hexes (`#94A3B8`, `#3B82F6`, `#10B981`, `#F59E0B`, `#A78BFA`, `#0EA5E9`, `#EF4444`, `#8B5CF6`, `#EDE9FE`) that do not belong to the brand semantic scales — the signature surface is off-palette.
5. **Pipeline fragility.** New token categories (font sizes, spacing) are silently stripped by `tailwind-merge` unless registered (V1.94 lesson); no regression guard exists.
6. **Studio blindness.** design-studio cannot render Typography, Spacing, Radius, Elevation, or Motion token galleries — the system is partially unverifiable (canvas galleries land in P3).

## User value

| Who | Outcome when P0 ships |
| --- | --- |
| Authors (downstream) | Every later surface can inherit one coherent literary-computational identity — no more surface-local invention |
| Maintainers | Normative v0.4 contract + hardened pipeline; studio token galleries make tokens inspectable in light + dark |
| QC/QA | Contrast tables + chromatic mapping appendix + greppable token SSOT for residual close-out |

## Design concept — “The Literary Engine”

Nexus is a **writer's atelier resting on a computational engine**. The language reconciles two registers:

- **Content voice (文学 / 创作):** editorial serif for creative-entity titles, reading surfaces, and brand moments. Serif = the author's material.
- **Interface voice (AI / 互联 / 画布 / 计算引擎):** precise system sans, ink-blue atmospheric darks, cyan signal accents, instrument-grade status language. Sans = the engine.

Rules that keep the concept premium rather than themed:

- Serif appears **only** in content voice positions (work/world/chapter titles, manuscript headings, empty-state headlines on authoring surfaces, brand page). Never in nav, buttons, tables, badges, labels.
- Atmosphere comes from **tint, not decoration**: ink-blue cast in dark surfaces; whisper of warm-paper cast in light surfaces. No gradients-as-decoration, no noise textures, no glassmorphism.
- Depth is **functional**: elevation communicates interactivity (rest → hover → pressed → dragging), not ornament.
- Motion is **short and standard-eased**: 120–220ms, reduced-motion honored.

## v0.4 token additions/changes (contract-level)

### T1. Display typography tier

- `typography.font-display` — self-hosted OFL serif + fallback stack. **Recommended family: Source Serif 4** (variable, optical sizing, OFL; shortlist Fraunces / Newsreader if build cost objects). Fallback: `Georgia, 'Times New Roman', ui-serif, serif`.
- `typography.display-32` / `display-24` / `display-20` — content-voice titles (sizes aligned to existing heading scale for layout stability; serif metrics tuned: line-height 1.2–1.3, letter-spacing `-0.01em`…`0`).
- **Font asset location (decision — concern 1):**
  - **Canonical provenance:** `packages/nexus-ui/assets/fonts/` (LFS-tracked; OFL license file alongside; matches the existing `packages/nexus-ui/assets/logos/` precedent).
  - **App consumption:** apps do **not** import font binaries from package source (`packages/nexus-ui/AGENTS.md` forbids binary asset imports from component source, and deep-imports of `@42ch/nexus-ui/src/*|assets/*` are forbidden). Each consuming app (`apps/web`, `apps/design-studio`) vendors the **subset** `.woff2` binaries into its own `public/fonts/` with a one-line provenance comment pointing at the package path. The subset is regenerated from the package canonical file by a documented copy step (no runtime cross-package binary import).
  - **`@font-face` declaration + `--font-display` CSS variable:** `tooling/design-tokens/src/tokens.css` (currently defines only `--font-sans` / `--font-mono`; `--font-display` is added here, `:root` only — single family, no dark override needed).
  - **Subset spec:** Latin + common punctuation + figures; variable axis pinned to a single weight ramp (400 regular + 600 semibold); `font-display: swap`.
  - **Preload:** primary weight (400) only — `<link rel="preload" as="font" type="font/woff2" crossorigin>` in each app's `index.html`.
  - **Bundle delta gate (measurable):** gzipped `.woff2` size per weight **≤ 80 KB** (Source Serif 4 Latin subset typically 30–45 KB/weight). If exceeded after subsetting, fall back to system-serif stack (`Georgia, 'Times New Roman', ui-serif, serif`) now and record self-host as a V1.122 roadmap item (Durable Roadmap Gate — must land on the P0 plan, not just this spec).
- Reading chrome's hardcoded Georgia is absorbed into `font-display` tokens.

### T2. Ink atmosphere

- Dark surfaces move from pure neutral to ink-blue family: `background-100/200/300` and `gray-100…300` receive a deep-blue cast derived from `brand-deep-blue-1000` (`#0C1A2B` light / `#3D6A94` dark) — target feel: “ink chamber”, not “dark gray app”. Values must keep all existing text pairings AA — recompute the full contrast table.
- Light surfaces keep near-white calm with a whisper of warm-paper cast in `background-200/300` (very subtle; measure against gray text AA).
- `scrim` values re-checked against new surfaces.

**Candidate ink values (concern 2 — normative candidates, subject to AA gate):**

These candidates shift hue toward blue at approximately matched lightness to the current neutrals. The AA contrast table (below) is the gate: any candidate that drops a currently-passing pairing below AA **blocks that candidate**, not the table. Implementer may tune ±1 lightness step to clear AA, but may not change hue family.

| Token | Current (dark) | Candidate (dark, ink cast) | Lightness delta |
|-------|----------------|----------------------------|-----------------|
| `background-100` | `#0a0a0a` | `#0A1320` | ~matched (L* ≈ 5→6) |
| `background-200` | `#111111` | `#0F1A2A` | ~matched |
| `background-300` | `#1a1a1a` | `#152438` | ~matched |
| `gray-100` | `#1f1f1f` | `#141F2E` | ~matched |
| `gray-200` | `#2a2a2a` | `#1E2A3D` | ~matched |
| `gray-300` | `#3a3a3a` | `#283749` | ~matched |

| Token | Current (light) | Candidate (light, warm-paper whisper) |
|-------|-----------------|----------------------------------------|
| `background-200` | `#fafafa` | `#FAF8F4` (very subtle warm) |
| `background-300` | `#f5f5f5` | `#F5F2EC` (very subtle warm) |

**Contrast table structure (lives in DESIGN.md body — AA blocks values, not the table):**

Rows = text/emphasis tokens (`gray-1000`, `gray-900`, `gray-700`, `gray-500`, `brand-cyan`, `brand-deep-blue`, `blue-700`, `red-700`, `green-700`, `amber-700`, `teal-700`, `purple-700`, `scrim` text). Columns = ink/warm surfaces (`background-100/200/300`, `gray-100/200/300`, `canvas-surface` dark, `scrim` overlay). Each cell = `ratio:1` + **Pass/Fail**. Any **Fail** on a pairing currently in use blocks the candidate value (implementer returns to the table, tunes lightness ±1, re-enters). The table is recomputed before token lock and recorded verbatim in DESIGN.md `## Contrast (AA, recomputed)` body section.

### T3. Elevation scale

- Add `elevation-0`…`elevation-4` shadow tokens (ambient + key two-part shadows, soft large-blur): `0` flat, `1` resting card, `2` hover/raised, `3` popover/floating, `4` modal/dragging.
- Existing `shadow-card/popover/modal` become aliases onto the new scale (no consumer breakage).
- Component recipe: interactive cards lift to `elevation-2` + `translateY(-1px)` on hover (160ms `ease-standard`); canvas nodes rest `elevation-1`, hover `elevation-2`, selected keeps two-layer ring, dragging `elevation-4`.

**Two-part shadow recipe + alias chain (concern 3 — normative):**

The current `popover`/`modal` are already two-part (ambient `0 1px 1px` + key large-blur); `card` is single-layer and weak. v0.4 normalizes all five levels to the same two-part recipe (ambient tight + key soft), with shadow color taking a subtle ink-blue tint in light theme (matching the T2 cast) and pure-black in dark.

```
/* Light — :root */
--shadow-elevation-0: none;
--shadow-elevation-1: 0 1px 2px rgba(15, 23, 42, 0.04), 0 1px 3px rgba(15, 23, 42, 0.03);   /* resting card — replaces flat 1px */
--shadow-elevation-2: 0 2px 4px rgba(15, 23, 42, 0.06), 0 4px 12px -2px rgba(15, 23, 42, 0.05);  /* hover/raised — NEW */
--shadow-elevation-3: 0 1px 1px rgba(15, 23, 42, 0.03), 0 8px 24px -12px rgba(15, 23, 42, 0.18);  /* popover — preserves current */
--shadow-elevation-4: 0 1px 1px rgba(15, 23, 42, 0.04), 0 24px 48px -24px rgba(15, 23, 42, 0.30); /* modal/dragging — preserves current */

/* Dark — .dark (pure-black, stronger alphas per current) */
--shadow-elevation-0: none;
--shadow-elevation-1: 0 1px 2px rgba(0, 0, 0, 0.40), 0 1px 3px rgba(0, 0, 0, 0.30);
--shadow-elevation-2: 0 2px 4px rgba(0, 0, 0, 0.50), 0 4px 12px -2px rgba(0, 0, 0, 0.40);
--shadow-elevation-3: 0 1px 1px rgba(0, 0, 0, 0.60), 0 12px 28px -12px rgba(0, 0, 0, 0.70);
--shadow-elevation-4: 0 1px 1px rgba(0, 0, 0, 0.70), 0 28px 56px -24px rgba(0, 0, 0, 0.85);

/* Alias chain — zero consumer breakage (existing shadow-card/popover/modal classes unchanged) */
--shadow-card:     var(--shadow-elevation-1);
--shadow-popover:  var(--shadow-elevation-3);
--shadow-modal:    var(--shadow-elevation-4);
/* elevation-2 has no legacy alias — consumed directly as shadow-elevation-2 (hover recipe) */
```

**Pipeline projection:** `tailwind.preset.ts` keeps existing `boxShadow.{card, popover, modal}` keys (so `shadow-card`/`shadow-popover`/`shadow-modal` classes still resolve via the aliased CSS vars) and **adds** `boxShadow.elevation = { 0, 1, 2, 3, 4 }` so the new `shadow-elevation-N` utilities are available. No consumer class renames required; the alias chain is the non-breaking strategy.

### T4. Motion recipes

- Add `duration-enter` (200ms) / `duration-exit` (140ms); codify per-component recipes (card hover, popover enter, dialog enter, canvas node) in DESIGN.md body. `prefers-reduced-motion` rule unchanged and restated per recipe.

### T5. Canvas ambient + chromatic hygiene

- Canvas surface/grid/minimap/controls tokens tuned per theme (grid dot size/gap/alpha; dark canvas = ink surface, not `#0a0a0a` flip).
- **Chromatic hygiene mapping (hue-preserving):** every Tailwind-palette leftover hex in `components.canvas.*` is remapped to the nearest brand semantic-scale value (e.g. `#3B82F6` → `blue-700` family, `#10B981` → `green-700` family, `#F59E0B` → `amber-700` family, `#A78BFA`/`#8B5CF6` → `purple-700` family, `#0EA5E9` → `teal-700` family, `#EF4444` → `red-700` family, `#94A3B8` → `gray-500/600`, `#EDE9FE` → purple alpha wash). Light + dark values both specified. The mapping table is normative in the spec appendix and applied verbatim to DESIGN.md/DESIGN.dark.md frontmatter.
- Per-surface accent spines stay semantic (strategy = purple-700, outline = amber-700, worldkb = teal-700) — recorded as tokens so P3 can apply consistently.

**Canvas node width family (concern 4 — contract + registration owned by P0):**

P0 *registers* the `canvas-node-width-*` family in DESIGN.md frontmatter under **`components.canvas.node-width.<role>`** (consistent with the existing `components.canvas.*` namespace — `canvas.surface`, `canvas.grid`, `canvas.node-fill`, etc., already projected through `tooling/design-tokens/tailwind.preset.ts`). The five confirmed source values (grep-verified) and their semantic roles:

| DESIGN.md key (`components.canvas.node-width.*`) | Value | Source (current hardcoded) |
|----------------------------------------------------|-------|----------------------------|
| `strategy-root` | `260px` | `apps/web/src/components/canvas/strategy-nodes.tsx:133` |
| `strategy-primary` | `140px` | `apps/web/src/components/canvas/strategy-nodes.tsx:174` |
| `strategy-secondary` | `150px` | `apps/web/src/components/canvas/strategy-nodes.tsx:190` |
| `outline-scene-beat` | `160px` | `apps/web/src/components/canvas/outline-canvas/scene-beat-nodes.tsx:72,120` |
| `default` | `176px` | `apps/web/src/components/canvas/presentational/node-chrome-shell.tsx:94` (NodeChromeShell default) |

The family is **registered** (DESIGN.md frontmatter + tokens.css `--canvas-node-width-*` structural vars + tailwind preset `minWidth` keys + twMerge `min-w` group) in **P0**. The tailwind utility prefix is fixed at registration as **`min-w-canvas-node-<role>`** so all downstream plans consume one name. **P2** *consumes/verifies* the registered utilities while sweeping arbitrary values (no re-registration, no canvas visual change). **P3** *applies* them (replaces the hardcoded `min-w-[Npx]` with `min-w-canvas-node-<role>`).

### T6. Reading-chrome tokenization contract

- All 40+ hardcoded values currently in `apps/web/src/index.css` reading-chrome block are **named and valued** as `reading-chrome-*` component tokens in DESIGN.md frontmatter (font families → `font-display`, sizes, weights, spacing, borders, the 4 raw rgba values → semantic tints).
- **Ownership split (product gate):** P0 **authors** the token contract + projects values into `tokens.css`. P3 **migrates** the CSS block to `var(--…)`-only consumption and wires the display serif on novel chapter titles. P0 must not leave orphan token names without documented P3 consumers; P3 must not invent reading tokens outside this contract.

### T7. Pipeline hardening

- `tooling/design-tokens/tailwind.preset.ts`: register new token categories — `fontFamily.display` (`--font-display`), `fontSize.{display-32, display-24, display-20}`, `boxShadow.elevation.{0..4}` (additive alongside existing `boxShadow.{card,popover,modal}`), `transitionDuration.{enter, exit}` (additive alongside existing `state/popover/modal`).
- `tooling/design-tokens/src/tokens.css`: project all v0.4 values, `:root` + `.dark`.
- **`extendTailwindMerge` registration (concern 6 — location pinned):** the V1.100 SSOT is **`packages/nexus-ui/src/lib/cn.ts`** (re-exported via `apps/web/src/lib/utils.ts`; guarded by `tooling/check-ui-guardrails.sh` `check_cn_parity`). It is **not** "the V1.94 location" — V1.94 only introduced the lesson; V1.100 consolidated the authority. The current registry contains `font-size`, `opacity`, `max-h` class groups.

  **Exact new class groups / entries to add for v0.4:**

  | tailwind-merge class group | Entries to add | Reason |
  |----------------------------|----------------|--------|
  | `font-size` (extend existing) | `text-display-32`, `text-display-24`, `text-display-20` | Display tier classes must not be misparsed as text-color and strip `text-white` (V1.94 silent-strip class of bug) |
  | `font-family` (new entries in default `font-family` group) | `font-display` | Prevents `font-sans` / `font-display` from being merged away when both appear |
  | `shadow` (default `box-shadow` group — extends with named tokens) | `shadow-elevation-0`, `shadow-elevation-1`, `shadow-elevation-2`, `shadow-elevation-3`, `shadow-elevation-4` | Defensive — `shadow-*` is one class group in tailwind-merge by default, but registering named elevation tokens guards against future prefix drift |
  | `transition-duration` (default `duration` group) | `duration-enter`, `duration-exit` | Same defensive registration for new duration tokens |
  | `min-width` (default `min-w` group) | `min-w-canvas-node-*` family (keys fixed at P0 registration per concern 4) | P0 registers `canvas-node-width-*` tokens; the resulting `min-w-*` utilities must survive merge |

- **Regression test:** a vitest case in `packages/nexus-ui/src/lib/cn.test.ts` (or `apps/web/src/lib/utils.test.ts` mirroring the existing parity check) asserting representative classes from **each** new group survive `twMerge()` against a conflicting default — e.g. `cn('text-display-24', 'text-white')` keeps both; `cn('shadow-elevation-2', 'shadow-card')` collapses to one but never drops an unrelated class; `cn('font-display', 'font-sans')` resolves to the latter (correct merge semantics) without dropping e.g. `text-display-24`. The V1.94 silent-strip class of bug is the threat model.

### T8. Card.Title content-voice opt-in contract (concern 5 — implemented in P1)

P0 owns the **token + recipe contract** that P1 implements on `Card.Title`. The opt-in shape is **prop-based (additive, non-breaking)**:

- New optional prop on `CardTitle` (in `packages/nexus-ui/src/components/card.tsx`): `voice?: 'interface' | 'content'` — default `'interface'`, which preserves the current `text-heading-16 font-heading` (sans) treatment exactly. Existing call sites (no `voice` prop) compile and render identically — no breaking change, no CVA variant explosion on the `Card` root.
- When `voice="content"`: `CardTitle` swaps to `font-display text-display-20 tracking-tight` (serif) per the v0.4 typography tier. Sized to `display-20` to match the current `heading-16` px layout within serif metrics.
- Use is **greppable** (`voice="content"`) and reserved for cards presenting a creative entity (work/world card, brand-page card). Not used on interface cards (settings cards, dialog content cards, table-cell cards). The recipe is documented in DESIGN.md body under `components.card.title.voice` and verified by the P1 components gallery states matrix.
- Alternatives rejected: class-only opt-in (drift risk — every call site reinvents the recipe), CVA variant on the `Card` root (would force restructuring of the existing unsplit `Card` className), new component `CardTitleContent` (proliferates the API surface). Prop-on-`CardTitle` is the minimum additive change.

## Non-goals

- No consumer adoption of elevation/voice on real app surfaces beyond what pipeline + **token** galleries need (P1–P3 adopt). **No** reading-chrome CSS migration (P3); **no** canvas component visual adoption (P3).
- No Canvas token gallery in studio (P3); P0 ships Typography / Spacing / Radius / Elevation / Motion only.
- No wire/daemon/Rust changes (`wire_contracts_changed: false`).
- No new brand colors; VI palette frozen.
- No component API changes; no package promotion.
- No i18n/copy changes; no new motion libraries; no desktop native chrome.

## Acceptance criteria

Each AC is binary; evidence = file path + command log and/or grep and/or screenshot as noted.

- AC-P0-1 — DESIGN.md + DESIGN.dark.md at v0.4.0 with T1–T8 frontmatter + body sections (concept, voice rules, elevation/motion recipes, contrast tables, chromatic mapping appendix); version field bumped; completeness Level 3 re-audited.
- AC-P0-2 — Full light + dark WCAG 2.1 AA contrast table recomputed and recorded in DESIGN.md body for every changed pairing (ink backgrounds × gray text steps, scrim, canvas tokens). Any failing pairing blocks the candidate value, not the table. Table structure per T2 (rows = text tokens, columns = surfaces, cells = ratio + Pass/Fail).
- AC-P0-3 — `tokens.css` + `tailwind.preset.ts` project all v0.4 tokens (light + dark), including the `--shadow-elevation-*` primitives, the `--shadow-{card,popover,modal}` alias chain, `--font-display`, the `--text-display-*` metric tuples, and the `--space-*` / `--radius-*` / `--duration-*` / `--ease-*` scalar scales; `pnpm --filter @nexus/design-tokens build` green — the gate is real: `tsc --noEmit` type-checks the preset and `scripts/check-tokens.mjs` asserts the expected v0.4 projections exist.
- AC-P0-4 — `extendTailwindMerge` at `packages/nexus-ui/src/lib/cn.ts` registers the new groups listed in T7 (font-size, font-family, shadow, transition-duration entries) + vitest regression passes (representative display/elevation/duration classes survive merge). `tooling/check-ui-guardrails.sh` `check_cn_parity` still green.
- AC-P0-5 — Serif decision recorded with **measured bundle delta** (gzipped `.woff2` per weight ≤ 80 KB → self-hosted path; otherwise documented system-stack fallback + Durable Roadmap entry on the P0 plan for V1.122). Font files vendored to `apps/web/public/fonts/` + `apps/design-studio/public/fonts/` with provenance comment pointing to `packages/nexus-ui/assets/fonts/`.
- AC-P0-6 — design-studio gains Typography (incl. display tier), Spacing/Radius, Elevation/Motion galleries rendering real token values in both themes (typically extending the existing `/tokens` page — new top-level routes optional); studio build + tests green.
- AC-P0-7 — Canvas chromatic mapping table applied verbatim in both DESIGN files; zero Tailwind-palette leftover hexes remain in `components.canvas.*` frontmatter (grep-verified).
- AC-P0-8 — `apps/web` + `apps/design-studio` typecheck, vitest, and builds green after pipeline change (no intentional UI adoption required beyond gallery/font wiring).

## Interfaces

- `DESIGN.md` / `DESIGN.dark.md` frontmatter keys (additive; existing key names unchanged).
- `tooling/design-tokens/{src/tokens.css, tailwind.preset.ts}`.
- **tailwind-merge registration:** `packages/nexus-ui/src/lib/cn.ts` (V1.100 SSOT — **not** "V1.94 location"; re-exported via `apps/web/src/lib/utils.ts`; guarded by `tooling/check-ui-guardrails.sh`).
- **Font assets:** provenance `packages/nexus-ui/assets/fonts/` (LFS); vendored subsets consumed from each app's `public/fonts/`; `@font-face` + `--font-display` in `tooling/design-tokens/src/tokens.css`.
- design-studio gallery surfaces: extend the existing `/tokens` page (`apps/design-studio/src/pages/tokens.tsx`) with Typography / Spacing / Radius / Elevation / Motion sections — new top-level routes under `App.tsx` are optional, not required.

## Validation plan

- Token grep sweeps (leftover hexes: `rg "#94A3B8|#3B82F6|#10B981|#F59E0B|#A78BFA|#0EA5E9|#EF4444|#8B5CF6|#EDE9FE" DESIGN.md DESIGN.dark.md` must return zero hits in `components.canvas.*`); contrast table artifacts; vitest + builds; studio gallery screenshots (light/dark) as QA evidence; twMerge regression test (T7); bundle-delta measurement for self-hosted serif (T1 gate).
