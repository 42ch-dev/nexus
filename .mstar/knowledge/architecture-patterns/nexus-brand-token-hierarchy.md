---
module: packages/nexus-ui + apps/web + apps/design-studio + tooling/design-tokens + repo-root DESIGN
date: 2026-07-06
problem_type: architecture-pattern
category: architecture-patterns
severity: medium
plan_id: V1.83-P-last (compound of brand UI foundation iteration); V1.94-P-last (contrast rule correction); V1.98-P0 (DESIGN SSOT unification + shared token pipeline + design-studio); V1.121-P0 (v0.4 Literary Engine: display typography, ink atmosphere, elevation scale, motion recipes, canvas chromatic hygiene, structural namespace)
tags: [brand, design-tokens, nexus-ui, design-md, git-lfs, svg, npm-package, button-contrast, dark-theme, design-studio, tailwind-preset, ssot-unification, literary-engine, ink-atmosphere, display-typography, motion-recipes, structural-namespace]
applies_when: adding or consuming cross-application Nexus brand/design tokens (new product surface, platform package, Web shell refresh, or a new app consuming the design system); also when defining any button background/text colour combination, adding a display typography tier, tuning surface atmosphere, extending elevation/motion, or registering a structural (non-color) CSS variable family
last_updated: 2026-07-18 (V1.121 v0.4 Literary Engine: display typography tier, ink atmosphere, elevation scale, motion recipes, canvas chromatic hygiene, structural vs color namespace distinction, real build gate, twMerge registry hardening, self-hosted font)
---

# Nexus Brand & Design Token Hierarchy

**Track**: Knowledge (durable guidance distilled from V1.83 Brand UI Foundation; corrected V1.94; unified V1.98; V1.121 v0.4 Literary Engine elevation).

## Context

Before V1.83, `apps/web/DESIGN.md` held both app-specific canvas/SOUL/findings tokens and de-facto brand colors. V1.83 introduced a publishable `@42ch/nexus-ui` package and root `DESIGN.md` / `DESIGN.dark.md` as the cross-application brand SSOT. **V1.98 unified the full token contract**: the root `DESIGN.md` / `DESIGN.dark.md` pair is now the **sole** normative token SSOT (not just brand), `apps/web/DESIGN*.md` were deleted, and a shared `@nexus/design-tokens` workspace package (Tailwind preset + `tokens.css`) was extracted at `tooling/design-tokens/` so every app consumes one pipeline with no per-app duplicate `theme.extend`. A read-only `apps/design-studio` gallery visualizes the contract. Future surfaces must reuse this unified hierarchy instead of redefining hex values locally.

## Guidance (the pattern)

Token consumption follows these layers, top to bottom (post-V1.98):

1. **Root token SSOT** — repo-root `DESIGN.md` / `DESIGN.dark.md` own canonical token names, VI palette values (`#1E3A5F`, `#25D1E0`, `#FFFFFF`), logo usage rules, all color/typography/spacing/rounded/elevation scales, and accessibility intent. These files are normative for **all** shared design semantics (brand + app tokens). *(Pre-V1.98, apps/web/DESIGN.md held a parallel app mapping layer — retired in V1.98.)*
2. **`@nexus/design-tokens` shared pipeline** — `tooling/design-tokens` workspace package exports a Tailwind **preset** (`tailwind.preset.ts`) + generated **`tokens.css`** (CSS custom properties) derived from the root SSOT. Every app imports `@nexus/design-tokens/tokens.css` and uses the preset; **no app defines its own `theme.extend` token block** that the preset already owns. This is the single CSS/Tailwind pipeline layer (introduced V1.98).
3. **`@42ch/nexus-ui` package** — owns reusable **brand** artifacts derived from the root contract: Git LFS–tracked PNG provenance, canonical SVG logo variants (regular-git text), token data (`tokens.ts`), CSS theme entry (`theme.css`), and React brand components (`<NexusLogo>`, `<NexusMark>` — V1.87). Brand-layer only; do NOT migrate shadcn primitives here.
4. **App implementation** — shells/base primitives consume `@nexus/design-tokens` (preset + tokens.css), public `@42ch/nexus-ui` brand exports, and (transitionally) the `@web-ui/*` alias to `apps/web/src/components/ui/*`. No deep imports into `packages/nexus-ui/src/**`; use declared `package.json` `exports` only.

### Asset policy

| Asset type | Storage | Consumption |
|------------|---------|-------------|
| PNG logo sources (provenance) | Git LFS under `packages/nexus-ui/assets/logos/*.png` | Reference only; not exported for runtime |
| SVG logos (canonical) | Regular git text under `assets/logos/*.svg` | Exported via `@42ch/nexus-ui` |
| Token/theme (shared pipeline) | `tooling/design-tokens/src/tokens.css` + `tailwind.preset.ts` | Imported by apps via `@nexus/design-tokens` |
| Brand theme slice | `packages/nexus-ui/theme.css`, `tokens.ts` | Imported by apps via public `@42ch/nexus-ui` exports |

### Contrast rule (V1.94 clarification — background-driven)

**V1.83 locked rule (still normative)**: "Cyan `#25D1E0` is accent-only on white (~1.9:1 — fails AA as body text). Primary actions on light surfaces use deep blue `#1E3A5F`. Dark mode primary button: **cyan fill + deep blue text.**"

**V1.94 clarification**: The contrast rule is **mode-independent** — the background color decides the text color, not the theme. Dark/primary/saturated backgrounds → light/white text; light/bright backgrounds → dark text. Cyan `#25D1E0` is a light/bright background, so the dark-mode primary button correctly uses deep-blue text.

Practical application:
- Light mode primary: `bg-blue-700 text-white` (unchanged since V1.83).
- Dark mode primary: `dark:bg-brand-cyan dark:text-brand-deep-blue` (V1.94 correction; was `dark:text-white`).
- Secondary/tertiary/destructive: existing token mapping preserved.
- Cyan as accent-only-on-white rule for **body text** still holds (cyan ~1.9:1 on white fails AA for body copy). The V1.94 correction narrows the V1.83 rule to body text only; **button labels** follow the background-driven contrast rule.

### Registered-scale-only rule (V1.98 lesson — qc1 W001)

A Tailwind utility class only emits a CSS rule if its scale step is **registered** in the shared preset. `bg-gray-alpha-150` produced no production CSS because the `gray-alpha` scale registers only `{100,200,300,400,500,600}` — the active nav highlight was silently invisible in the production bundle (JIT purges unregistered steps; dev may tolerate the artifact). **Use only scale steps that exist in `tooling/design-tokens/tailwind.preset.ts`**; verify gallery chrome against the production build, not just dev. See also [tailwind-theme-key-routing-for-sizing-tokens.md](tailwind-theme-key-routing-for-sizing-tokens.md) (a token under the wrong `theme.*` key likewise emits nothing).

### Audit pattern (V1.94)

When introducing the rule or changing any button token: write a vitest snapshot test that captures the rendered `className` for every variant in both themes, plus explicit assertions that encode the background-driven rule. A regression that reverts `dark:text-brand-deep-blue` to `dark:text-white` on the cyan fill will fail the snapshot and the explicit assertions. Reference: `apps/web/src/components/ui/button.test.tsx`.

### Prebuild chain

Apps that consume `@42ch/nexus-ui` should run `pnpm --filter @42ch/nexus-ui run build` (or equivalent) in `prebuild` / `pretypecheck` hooks so workspace resolution and theme.css exist before Vite/Tailwind compile. (`@nexus/design-tokens` is consumed directly via workspace resolution; no separate build step required for the preset + tokens.css.)

## Why This Matters

- **Single token source** prevents drift when multiple product surfaces ship independently — V1.98 collapsed the brand SSOT + app mapping into one root pair + one shared pipeline.
- **Shared pipeline** (`@nexus/design-tokens`) means a token edit in root DESIGN → regenerate tokens.css → every app picks it up; no per-app `theme.extend` copies to keep in sync.
- **Package boundary** keeps brand artifacts publishable without coupling to app routing, state, or React components.
- **LFS vs SVG split** preserves designer PNG references while keeping runtime assets diff-friendly and CDN/npm friendly.

## When to Apply

- Adding a new product surface that needs Nexus design tokens/branding → consume `@nexus/design-tokens` + `@42ch/nexus-ui`; do NOT create a per-app DESIGN.md.
- Extending token scales or brand tokens — update root DESIGN first, regenerate `tooling/design-tokens` output, then package exports, then app consumers.
- Publishing `@42ch/nexus-ui` to npm (future) — export map must remain stable; breaking renames require coordinated semver.
- Adding a gallery/visualization surface for the design system → follow the `apps/design-studio` read-only-mirror pattern (consume SSOT, do not invent tokens).

## Do NOT

- Resurrect a per-app `DESIGN.md` mapping layer (V1.98 retired `apps/web/DESIGN*.md`); the root pair + `@nexus/design-tokens` is the SSOT.
- Define a per-app `theme.extend` token block that duplicates the shared preset.
- Use a Tailwind scale step that is not registered in `tooling/design-tokens/tailwind.preset.ts` (it silently emits nothing in production — V1.98 qc1 W001).
- Put canonical brand hex values only in an app without root DESIGN + package alignment.
- Export React components from `@42ch/nexus-ui` without following the bundler-agnostic asset convention (consumer resolves the SVG URL via its own bundler and passes it as a `src` prop — do NOT import `.svg` in package source; see [bundler-agnostic-component-library-assets.md](bundler-agnostic-component-library-assets.md)). *(V1.83's "no React components without a dedicated component-library plan" guard was satisfied by V1.87.)*
- Commit runtime SVG logos through Git LFS (breaks text diffs and bundler inlining).
- Use cyan `#25D1E0` as primary body text on white backgrounds.

## Examples

- Root SSOT: `DESIGN.md`, `DESIGN.dark.md` (sole full-token pair, post-V1.98)
- Shared pipeline: `tooling/design-tokens` — `@nexus/design-tokens` exports `tailwind.preset.ts` + `src/tokens.css`; both `apps/web` and `apps/design-studio` import `@nexus/design-tokens/tokens.css` + use the preset.
- Brand package: `packages/nexus-ui` — `@42ch/nexus-ui` exports `theme.css`, `tokens`, logo SVGs, `<NexusLogo>`/`<NexusMark>` (V1.87).
- Web implementation: `apps/web` consumes `@nexus/design-tokens` + `@42ch/nexus-ui`; `NexusLogo` thin wrapper imports `@42ch/nexus-ui/assets/logos/logo-color.svg` and passes the resolved URL to the package's `<NexusLogo variant src>`.
- Gallery consumer: `apps/design-studio` — read-only Vite SPA visualizing every token scale + brand VI + all `apps/web` ui primitives (via `@web-ui/*` transitional alias) + Voice/Surface fixtures; runs without the daemon; not embedded in `nexus42`.

---

## V1.121 v0.4 "Literary Engine" Additions

V1.121 elevated the design system from Level 3 (mechanically complete) to an expressive literary-computational identity. The following additions refine the token hierarchy established in V1.83/V1.98:

### Display typography tier (content voice)

- `typography.font-display` — self-hosted Source Serif 4 + `Georgia, 'Times New Roman', ui-serif, serif` fallback. Content voice only: creative-entity titles, brand moments, empty-state headlines on authoring surfaces. Never on chrome (nav, buttons, tables, badges, labels).
- `typography.display-32` / `display-24` / `display-20` — semibold serif metric tuples, consumed via `text-display-*` utilities.
- **Voice split discipline**: `Card.Title` gains additive `voice?: 'interface' | 'content'` prop (default `interface`). The voice-split rule is greppable, test-pinned, and documented in DESIGN.md body. See [editorial-typography-voice-split.md](editorial-typography-voice-split.md).
- **Self-hosted OFL font wiring**: canonical provenance in `packages/nexus-ui/assets/fonts/` (LFS), app-vendored subsets in `public/fonts/`, `@font-face` in `tokens.css`, preload in each `index.html`, bundle gate ≤ 80 KB gz/weight. See [self-hosted-ofl-font-wiring.md](self-hosted-ofl-font-wiring.md).

### Ink atmosphere (dark surfaces)

- Dark backgrounds shifted from pure neutral (`#0a0a0a`/`#111`/`#1a1a`) to ink-blue-derived values (`#0A1320`/`#0F1A2A`/`#152438`), with gray tints similarly shifted (`#141F2E`/`#1E2A3D`/`#283749`). Lightness matched to pre-v0.4 values; AA contrast table recomputed.
- Light surfaces: `background-200`/`300` gained a whisper of warm-paper cast (`#FAF8F4`/`#F5F2EC`).
- **AA-gated value selection**: any candidate value that drops a currently-passing pairing below AA blocks that candidate (not the table). The full contrast table is recorded in DESIGN.md body.

### Elevation scale (two-part shadows)

- `elevation-0`…`elevation-4` replaces the previous 3-flat-shadow system. Each level is a two-part shadow (ambient tight + key soft).
- Legacy alias chain: `shadow-card` → `elevation-1`, `shadow-popover` → `elevation-3`, `shadow-modal` → `elevation-4`. Zero consumer breakage.
- Light theme shadows tinted toward ink blue (`rgba(15, 23, 42, …)`); dark theme uses pure black with stronger alphas.
- `tailwind.preset.ts` keeps existing `boxShadow.{card, popover, modal}` and adds `boxShadow.elevation.{0..4}`.

### Motion recipes

- `duration-enter` (200ms) / `duration-exit` (140ms) added alongside existing `duration-state` (120ms), `duration-popover` (160ms), `duration-modal` (220ms).
- `ease-standard` / `ease-emphasized` — standard `cubic-bezier(0.16, 1, 0.3, 1)` and emphasized `cubic-bezier(0.2, 0.8, 0.2, 1)`.
- `prefers-reduced-motion` honored per recipe — unchanged.

### Structural vs color namespace distinction

Layout metrics (canvas node widths, dialog/sheet sizing) live in **structural** CSS vars (`--canvas-node-width-*`, `--dialog-width`, `--sheet-width`, `--dialog-max-height`), **not** `--color-*`. The `sv()` helper (structural var) in `tailwind.preset.ts` resolves them under `minWidth`, `width`, `maxWidth`, `maxHeight` keys — not `colors`. The `check-tokens.mjs` build gate asserts 8 namespace guards forbidding the `--color-` prefix on these tokens.

**V1.94 flakiness**: in V1.94, structural tokens were briefly registered under `--color-*` (the only CSS var namespace the preset had at the time). V1.121 corrected this: `--canvas-node-width-*` are structural, `--dialog-width`/`--sheet-width`/`--dialog-max-height` are structural. The namespace guard prevents regression.

### twMerge registry hardening

Every new token class group added in V1.121 is registered in `packages/nexus-ui/src/lib/cn.ts`:

| Group | New entries |
|-------|-------------|
| `font-size` | `text-display-32`, `text-display-24`, `text-display-20` |
| `font-family` | `font-display` |
| `shadow` | `shadow-elevation-0`…`4` (plus legacy `shadow-card`/`popover`/`modal` aliases) |
| `duration` | `duration-enter`, `duration-exit`, `duration-state`, `duration-popover` |
| `min-w` | `min-w-canvas-node-*` (5 entries) |
| `w` | `w-dialog`, `w-sheet` |
| `max-w` | `max-w-dialog` |
| `max-h` | `max-h-dialog` |

**Threat model**: the V1.94 silent-strip class of bug — an unregistered display-size class was misparsed as a text-color class and dropped by `twMerge`. The regression test in `packages/nexus-ui/src/lib/cn.test.ts` asserts representative classes from each new group survive `twMerge()` against conflicting defaults.

### Real design-tokens build gate

`tooling/design-tokens/scripts/check-tokens.mjs` validates 58 projections (font-display vars, display-32/24/20 metric tuples, spacing/radius steps, motion tokens, elevation scale + alias chain, canvas node width family, dialog/sheet layout metrics, reading-chrome projection, badge family tints) + 8 namespace guards (structural tokens must not use `--color-*`). The `package.json` `build` script runs `tsc --noEmit && node scripts/check-tokens.mjs` — no longer a no-op.

### Canvas chromatic hygiene

Every Tailwind-palette leftover hex in `components.canvas.*` was remapped hue-preserving onto the brand semantic scales (e.g. `#3B82F6` → `blue-700` family, `#10B981` → `green-700` family, `#F59E0B` → `amber-700` family, `#A78BFA`/`#8B5CF6` → `purple-700` family, `#0EA5E9` → `teal-700` family, `#EF4444` → `red-700` family, `#94A3B8` → `gray-500/600`, `#EDE9FE` → purple alpha wash). Per-surface accent spines tokenized: strategy = `purple-700`, outline = `amber-700`, worldkb = `teal-700`. The mapping table is recorded in DESIGN.md `§Appendix: Canvas Chromatic Hygiene Mapping`.

### Reading-chrome tokenization

All 40+ hardcoded values in the reading-chrome CSS block were named as component tokens in DESIGN.md frontmatter (`reading-chrome-novel-*`, `reading-chrome-essay-*`, `reading-chrome-screenplay-*`). The novel-profile chapter title absorbs the hardcoded `Georgia` into `font-display`. P0 authored the token contract; P3 migrated the CSS block to `var(--…)`-only consumption.
