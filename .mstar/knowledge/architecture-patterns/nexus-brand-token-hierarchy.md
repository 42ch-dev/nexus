---
module: packages/nexus-ui + apps/web + apps/design-studio + tooling/design-tokens + repo-root DESIGN
date: 2026-07-06
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
tags:
  - design-tokens
  - design-md
  - nexus-ui
  - tailwind-preset
  - ssot-unification
  - cobalt-signal
  - chronos
  - logo-system
applies_when:
  - "adding or consuming cross-application Nexus brand/design tokens (new product surface, platform package, Web shell refresh, or a new app consuming the design system)"
  - "defining any button background/text colour combination"
  - "choosing between the ink pigment (brand-deep-blue) and the cobalt interaction signal"
  - "extending elevation/motion, or registering a structural (non-color) CSS variable family"
last_updated: 2026-09-10
---

# Nexus Brand & Design Token Hierarchy

**Refreshed 2026-09-10** — re-based on v1.187 v0.5 "Precision Creative Tool" (cobalt / silver-neutral / graphite, offline system typography, compiler-owned projection); the v0.4 warm-paper/serif/cyan values it previously described are retired.

**Track**: Knowledge (durable guidance distilled from V1.83 Brand UI Foundation; corrected V1.94; unified V1.98; recalibrated V1.121 v0.4 Literary Engine; re-based on v1.187 v0.5 "Precision Creative Tool").

## Context

Before V1.83, `apps/web/DESIGN.md` held both app-specific canvas/SOUL/findings tokens and de-facto brand colors. V1.83 introduced a publishable `@42ch/nexus-ui` package and root `DESIGN.md` / `DESIGN.dark.md` as the cross-application brand SSOT. **V1.98 unified the full token contract**: the root pair is the **sole** normative token SSOT (not just brand), `apps/web/DESIGN*.md` were deleted, and a shared `@nexus/design-tokens` workspace package (Tailwind preset + `tokens.css`) was extracted at `tooling/design-tokens/` so every app consumes one pipeline with no per-app duplicate `theme.extend`. A read-only `apps/design-studio` gallery visualizes the contract.

**v1.187 (`version: 0.5.0`)** recalibrated the language to a precision creative tool: silver-neutral light planes, graphite dark planes, cobalt interaction, offline system typography. The v0.4 warm-paper surfaces, global literary serif and cyan glow are retired values — they are not an alternative theme, and the public token *names* (`brand-deep-blue`, `brand-cyan`, `blue-*`, `brandColors.deepBlue/cyan/white`) are stable API identifiers that now carry the new pigments. Exact values live in the DESIGN pair; do not restate them elsewhere.

## Guidance (the pattern)

Token consumption follows these layers, top to bottom:

1. **Root token SSOT** — repo-root `DESIGN.md` / `DESIGN.dark.md` own canonical token names, the live palette, logo usage rules, all color/typography/spacing/rounded/elevation/motion scales, and accessibility intent. Normative for **all** shared design semantics (brand + app tokens).
2. **`@nexus/design-tokens` shared pipeline** — `tooling/design-tokens` exports a Tailwind **preset** (`tailwind.preset.ts`) plus generated **`tokens.css`**; both `tokens.css`, `packages/nexus-ui/theme.css` and the numeric brand snapshot are produced by one compiler from the DESIGN pair. **No app defines its own `theme.extend` token block** the preset already owns, and no handwriting exists between the pair and the CSS.
3. **`@42ch/nexus-ui` package** — owns reusable **brand** artifacts derived from the root contract: Git LFS–tracked PNG provenance, canonical SVG logo variants (regular-git text), the brand slice of CSS (`theme.css`, imported by `tokens.css`), the generated `brandColors` snapshot, and React brand components (`<NexusLogo>`, `<NexusMark>`).
4. **App implementation** — shells/base primitives consume `@nexus/design-tokens` (preset + tokens.css), public `@42ch/nexus-ui` brand exports, and (transitionally) the `@web-ui/*` alias to `apps/web/src/components/ui/*`. No deep imports into `packages/nexus-ui/src/**`; use declared `package.json` `exports` only.

### Asset policy

| Asset type | Storage | Consumption |
|------------|---------|-------------|
| PNG logo sources (provenance) | Git LFS under `packages/nexus-ui/assets/logos/*.png` | Reference only; not exported for runtime |
| SVG logos (canonical) | Regular git text under `assets/logos/*.svg` | Exported via `@42ch/nexus-ui` |
| Token/theme (shared pipeline) | `tooling/design-tokens/src/tokens.css` (generated) + `tailwind.preset.ts` | Imported by apps via `@nexus/design-tokens` |
| Brand theme slice | `packages/nexus-ui/theme.css` (generated), generated `brandColors` | Imported by apps via public `@42ch/nexus-ui` exports |

### Background-driven contrast invariant (the load-bearing rule)

**Foreground follows the actual fill, not the page name or the theme label.**

- Primary action: **`blue-700` fill**; label is `brand-white` in the light theme and `brand-deep-blue` in the dark theme (whose `blue-700` is a bright cobalt). Rest/hover/active = `blue-700/800/900`. Never place white on a bright dark-theme accent.
- Destructive: white on the light red scale; deep-blue on the dark theme's brighter red scale.
- Light text links and structural elements: `brand-deep-blue` (the ink pigment), never "the default blue".
- Semantic status colors (success/running, warning/stale, error, queued, preset, info) keep their meanings and are never replaced by the interaction accent.
- Design-time AA arithmetic is recorded in DESIGN §Contrast; it is **not** rendered acceptance — Studio (or the consuming app) must still show real rest/hover/active/focus states in both themes.

### Interactive fill/label audit

When a light-surface interactive element uses an interaction-step fill (`blue-700`…`blue-1000`), verify its paired label token (white on the light theme's steps; the dark theme follows its own brighter scale).

**Audit trap:** grepping literals for the fill class **misses** semantic tokens that alias those fills (e.g. `setup-wizard-step-circle-active-*`, `footer-profile-avatar-*-active`). Scan `tokens.css` for light `blue-*` / `brand-cyan-*` active-bg pairs and verify the paired `*-text` token; prefer token SSOT fixes over component class overrides. Both pairs belong in the projection/audit evidence when adding theme-split active chrome.

### Registered-scale-only rule (V1.98 lesson — qc1 W001)

A Tailwind utility class only emits a CSS rule if its scale step is **registered** in the shared preset. `bg-gray-alpha-150` produced no production CSS because the `gray-alpha` scale registers only `{100,200,300,400,500,600}` — the active nav highlight was silently invisible in the production bundle (JIT purges unregistered steps; dev may tolerate the artifact). **Use only scale steps that exist in `tooling/design-tokens/tailwind.preset.ts`**; verify gallery chrome against the production build, not just dev. See also [tailwind-theme-key-routing-for-sizing-tokens.md](tailwind-theme-key-routing-for-sizing-tokens.md) (a token under the wrong `theme.*` key likewise emits nothing).

### Audit pattern (V1.94, still the shape to copy)

When introducing the rule or changing any button token: write a vitest test that captures the rendered `className` for every variant in both themes, plus explicit assertions encoding the background-driven rule. A regression that flips the dark primary label back to white will fail the assertions. Existing references: `packages/nexus-ui/src/components/button.test.tsx` and the consumer-side `apps/web/src/components/ui/button.test.tsx`. Assert the observable recipe contract, not incidental neighbouring classes — see [behavior-first-assertions-shared-ui.md](../testing-patterns/behavior-first-assertions-shared-ui.md).

### Prebuild chain

Apps that consume `@42ch/nexus-ui` should run `pnpm --filter @42ch/nexus-ui run build` (or equivalent) in `prebuild` / `pretypecheck` hooks so workspace resolution and `theme.css` exist before Vite/Tailwind compile. `@nexus/design-tokens` is consumed directly via workspace resolution; its `tokens.css` must be current — regenerate with `pnpm --filter @nexus/design-tokens generate` and prove it with `pnpm --filter @nexus/design-tokens check`.

## Why This Matters

- **Single token source** prevents drift when multiple product surfaces ship independently — one root pair + one shared pipeline, one compiler.
- **The compiler owns the projection** so a DESIGN edit cannot silently fail to reach CSS or the brand snapshot ([design-pair-token-compiler.md](design-pair-token-compiler.md)).
- **Package boundary** keeps brand artifacts publishable without coupling to app routing, state, or React components.
- **LFS vs SVG split** preserves designer PNG references while keeping runtime assets diff-friendly and CDN/npm friendly.

## When to Apply

- Adding a new product surface that needs Nexus design tokens/branding → consume `@nexus/design-tokens` + `@42ch/nexus-ui`; do NOT create a per-app DESIGN.md.
- Extending token scales or brand tokens — update root DESIGN first, regenerate the derived artifacts, then package exports, then app consumers.
- Publishing `@42ch/nexus-ui` to npm (future) — the export map must remain stable; breaking renames require coordinated semver.
- Adding a gallery/visualization surface for the design system → follow the `apps/design-studio` read-only-mirror pattern (consume SSOT, do not invent tokens).

## Do NOT

- Resurrect a per-app `DESIGN.md` mapping layer (V1.98 retired `apps/web/DESIGN*.md`); the root pair + `@nexus/design-tokens` is the SSOT.
- Hand-edit `tokens.css`, `packages/nexus-ui/theme.css` or the generated brand snapshot — they are derived outputs.
- Define a per-app `theme.extend` token block that duplicates the shared preset.
- Use a Tailwind scale step that is not registered in `tooling/design-tokens/tailwind.preset.ts` (it silently emits nothing in production — V1.98 qc1 W001).
- Put canonical brand hex values only in an app without root DESIGN + package alignment.
- Export React components from `@42ch/nexus-ui` without following the bundler-agnostic asset convention (consumer resolves the SVG URL via its own bundler and passes it as a `src` prop — do NOT import `.svg` in package source; see [bundler-agnostic-component-library-assets.md](bundler-agnostic-component-library-assets.md)).
- Commit runtime SVG logos through Git LFS (breaks text diffs and bundler inlining).
- Treat the public name `brand-cyan` as a colour promise — it carries the theme's cobalt signal now, and the frozen logo assets intentionally keep their historical pigments.
- Reintroduce the retired v0.4 language (warm-paper surfaces, serif display tier, neon cyan glow) as an alternative palette or a "light reset" scope.
- Swap plain wide marks (`logoVariants`) for square plate lockups (`logoSquareVariants`) or vice versa.

## Examples

- Root SSOT: `DESIGN.md`, `DESIGN.dark.md` (sole full-token pair).
- Shared pipeline: `tooling/design-tokens` — `@nexus/design-tokens` exports `tailwind.preset.ts` + generated `src/tokens.css`; consumer command `pnpm --filter @nexus/design-tokens generate|check`.
- Brand package: `packages/nexus-ui` — `@42ch/nexus-ui` exports generated `theme.css`, `tokens` (`logoVariants`, `logoSquareVariants`, `logoCompactMarkHeightPx`), generated `brandColors`, logo SVGs, `<NexusLogo>` / `<NexusMark>` / Studio `<NexusLogoVariant>`.
- Web implementation: `apps/web` consumes `@nexus/design-tokens` + `@42ch/nexus-ui`; shell `NexusLogo` wrapper imports `logo-primary-square.svg` at `logoShellHeightPx`; the deep-ink titlebar uses `logo-white.svg` at `logoCompactMarkHeightPx`.
- Gallery consumer: `apps/design-studio` — read-only Vite SPA visualizing every token scale + brand VI + primitives (promoted, `@web-*` extracts, transitional `@web-ui/*`) + Voice/Surface fixtures; runs without the daemon; not embedded in `nexus42`.

---

## Chronos dual-role + timeline logo (`2026-07-22-vi-logo-upgrade`, values re-based v1.187)

Chronos Light/Dark shells share one brand language via an explicit **dual-role** model. Normative tables live in root `DESIGN.md` §Brand Colors; this section is the agent-facing operational summary.

### Ink structure vs cobalt signal

| Role | Token(s) | v0.5 role | Use for | Do **not** use for |
| --- | --- | --- | --- | --- |
| **Ink structure** | `brand-deep-blue` (`#202936`) | dark structural pigment in **both** themes | titlebar fill, light **text links**, labels on bright fills, security-note washes, logo plate structure | dark-theme paragraph text; the interaction accent |
| **Cobalt signal** | `brand-cyan` / `blue-700` and their 800/900/1000 steps (deepen in light, brighten in dark) | interactive accent | primary CTA fills, active nav bar, focus band, selection, timeline activity, checked chrome | body/paragraph text on light surfaces without checking DESIGN §Contrast; deep-structure washes |

**Token strategy (unchanged in shape, new values):** the light and dark `blue-*` scales both carry the interactive accent, so component tokens that reference `{colors.blue-700}` follow the accent in both themes without renaming CSS keys. Ink structure must use **`brand-deep-blue` explicitly** — the generic scale is the action scale.

### Failure mode that cost a QC fix-wave (W-001) — the durable lesson

After a scale reinterpretation, surfaces that still meant **deep structure** but used the generic interaction scale (`border-blue-700/20 bg-blue-700/10 text-blue-700`) rendered as accent washes — fingerprint trust/match cards on connection-setup violated `DESIGN.md` `connection-setup.security-note`. Fix: deep-ink classes (`border-brand-deep-blue/20`, `bg-brand-deep-blue/10`, `text-brand-deep-blue`) plus regression assertions that **exclude** the generic scale on those nodes.

**Heuristic:** if DESIGN or a component token names ink/structure/security-note/titlebar/link-on-light, use `brand-deep-blue` — never assume the generic scale means "deep".

### Timeline logo system (V1.132 plain vs square split — assets unchanged by v0.5)

| Contract | Package key | Asset | Role |
| --- | --- | --- | --- |
| Plain primary mark | `logoVariants.primary` | `logo-primary.svg` | Wide timeline mark (no plate) |
| Plain white-bg mark | `logoVariants.whiteBg` | `logo-white-bg.svg` | Wide mark for light surfaces (no plate) |
| Square primary plate | `logoSquareVariants.primary` | `logo-primary-square.svg` | Sidebar/header plate; desktop `icons:compose` source |
| Square white-bg plate | `logoSquareVariants.whiteBg` | `logo-white-bg-square.svg` | White plate lockup only |
| White mark | `logoVariants.white` | `logo-white.svg` | Ink titlebar + dark heroes |
| Mono mark | `logoVariants.mono` | `logo-mono.svg` | Static grayscale; tintable UI uses `<NexusMark>` |
| Text | `logoVariants.text` | `logo-text.svg` | Wordmark (`currentColor`) |

**Compact scale:** `logoCompactMarkHeightPx` = 14px (−30% from `logoShellHeightPx` 20px) — titlebar, Brand hero mini.

**Asset boundary (v0.5):** the SVG/PNG assets keep their historical baked pigments. That is deliberate: installed-identity assets are references, not live token swatches. `NexusMark` is the inline `currentColor` mark driven by shared tokens; Brand separates installed asset references from live token/CSS swatches. Do not recolor assets with CSS filters to simulate a new logo, and do not regenerate desktop icons from this change.

**macOS app icon (V1.135 P1 / V1.136 — desktop-domain, unaffected by v0.5):**

1. **H1 (retain):** Canvas must stay **fully opaque RGB** (`hasAlpha: false`) — never transparent margins/`INSET_RATIO` alpha that defeat the macOS squircle mask.
2. **H6:** Bake a **visible squircle plate** in `compose-app-icon.mjs` — opaque plate-colour canvas + ~6% opaque inset + ~22% corner radius clip (margin pixels remain plate colour, not alpha).
3. **H6 contrast (V1.136):** Margin/plate **must not share the same hex** — same-colour bakes yield 0 non-plate border pixels (geometry exists but is invisible). Use a contrasting margin and verify with a border-pixel scan before claiming compose success.
4. **H7:** `pnpm dev:desktop` must run `icons:generate` before `tauri dev`.
5. **Done gate:** a live Dock squircle confirm — Studio VI-004 / PNG opacity alone are **not** Dock-done.

**Removed:** `logo-color.svg` / `logoVariants.color` — redundant; do not resurrect.

**Shell rule:** sidebar plate uses `logo-primary-square.svg` at `logoShellHeightPx`; ink titlebar uses plain `logo-white.svg` at `logoCompactMarkHeightPx`. Do not theme-split shell lockups by light/dark.

**Geometry:** square plate lockups (`*-square.svg`) are square; plain marks and `<NexusMark>` are wide (~10:1) — prefer `height` + `width: auto`.

### Audit pattern additions (logo/brand)

- Button: assert the light primary recipe (`blue-700` fill + `brand-white` label) and the dark primary recipe (`blue-700` fill + `brand-deep-blue` label); assert absence of the retired cyan-fill primary.
- Focus band: `components.focus-ring` — 2px background gap + 2px `blue-700` outer band, in both themes.
- Links: light retry/list links assert `brand-deep-blue`; a light link using the generic interaction scale is a smell.
- Security / structure washes: assert `brand-deep-blue` alpha classes and **no** interaction-scale class on those nodes.
- Logos: `logoVariants` + `logoSquareVariants` keys match DESIGN; shell plate imports `*-square.svg`; titlebar imports plain `logo-white.svg` at compact height; desktop compose = opaque RGB + baked squircle plate.

---

## v0.5 design-language additions (v1.187)

These replaced the v0.4 values wholesale; the DESIGN pair is the value authority, the rules below are the durable part.

### Surfaces and signal

- Light planes are **silver-neutral** (`background-100/200/300`), dark planes are **graphite**; gray 100–300 are neutral fills, 400–500 borders/subdued graphical detail, 600–700 secondary/helper copy (Studio metadata uses gray-700), 800–1000 text emphasis. Do not set active small text in gray-500.
- Gray-alpha holds hover washes and decorative separators; a real control boundary uses an opaque gray step, never a low-alpha divider as the only affordance.
- `scrim` is a backdrop only — text lives on an opaque plane above it.
- Semantic hues keep their assignments: `red` failed/destructive/conflict, `amber` warning/stale/review, `green` success/healthy/running, `teal` queued/starting, `blue` info/interaction/timeline, `purple` preset/strategy/research, `pink` annotation/relationship. Colour is never the only status cue.

### Typography (offline system stacks — no self-hosted font)

- Interface and content display both use the OS sans stack with named simplified-Chinese fallbacks (`system-ui` → Apple/Segoe UI → PingFang SC / Hiragino Sans GB / Microsoft YaHei UI / Noto Sans CJK SC). No network font request; do not claim identical glyph metrics across operating systems.
- The display tier (`display-32/24/20`) remains a **larger title hierarchy**, not a different typeface; `font-display`, `display-*` utilities and `CardTitle.voice="content"` stay valid. Mono uses the OS monospace stack with CJK sans fallback.
- Reading measure 66ch, line-height 1.75, paragraph gap 1.25em in both themes; containers constrain the measure rather than shrinking the type.
- The V1.121 Source Serif 4 font-face declarations and Studio preloads are **removed from the active pipeline**; the pattern that wired them (and what a future reintroduction must do differently, given `tokens.css` is now generated) is recorded in [self-hosted-ofl-font-wiring.md](self-hosted-ofl-font-wiring.md).

### Shape, elevation, motion, focus

- Radius: control 4px, card/popover 8px, fullscreen 12px, pill 9999px; logo/asset corner radii are frozen and independent of UI radius tokens.
- Elevation 0–4 are the separation scale (rest card/node = 1, interactive hover = 2, popover/menu/tooltip = 3, modal/dragging = 4; flat grouping = 0). Legacy `shadow-card`/`popover`/`modal` names remain aliases; interactive card hover changes border/shadow only, without moving text.
- Motion values live in DESIGN §Motion; menus/dialogs/toasts fade (no translation required), reduced motion removes decorative transitions/transforms and makes state visibility immediate.
- Focus: every keyboard target has the visible ring (`components.focus-ring`); active controls never use the disabled opacity wash.

### Structural vs colour namespace distinction

Layout metrics (canvas node widths, dialog/sheet sizing) live in **structural** CSS variables (`--canvas-node-width-*`, `--dialog-width`, `--sheet-width`, `--dialog-max-height`), **not** `--color-*`. The `sv()` helper (structural var) in `tailwind.preset.ts` resolves them under `minWidth`, `width`, `maxWidth`, `maxHeight` keys — not `colors`. The compiler projects these names explicitly; the projection is the guard (a structural token has no `--color-` twin to drift into).

### twMerge registry hardening (V1.121, retained)

Token-derived class groups are registered in `packages/nexus-ui/src/lib/cn.ts` so they merge within their own group instead of being stripped:

| Group | Entries |
|-------|---------|
| `font-size` | `text-display-32`, `text-display-24`, `text-display-20` |
| `font-family` | `font-display` |
| `shadow` | `shadow-elevation-0`…`4` (plus legacy `shadow-card`/`popover`/`modal` aliases) |
| `duration` | `duration-enter`, `duration-exit`, `duration-state`, `duration-popover` |
| `min-w` | `min-w-canvas-node-*` (5 entries) |
| `w` | `w-dialog`, `w-sheet` |
| `max-w` | `max-w-dialog` |
| `max-h` | `max-h-dialog` |

**Threat model**: the V1.94 silent-strip class of bug — an unregistered display-size class misparsed as a text-colour class and dropped by `twMerge`. `packages/nexus-ui/src/lib/cn.test.ts` asserts representative classes from each new group survive `twMerge()` against conflicting defaults.

### Projection gate

`pnpm --filter @nexus/design-tokens check` regenerates the three derived artifacts from the DESIGN pair and compares them byte-for-byte, plus asserts leaf/var parity and rejects stale v0.4 pins. It **replaced** the older hand-written needle list: the compiler's fail-closed rules and the byte comparison are what make a landing provable now — see [design-pair-token-compiler.md](design-pair-token-compiler.md) and [component-variant-token-projection.md](component-variant-token-projection.md).

### Canvas colour discipline

Every canvas/outline/worldkb/timeline/SOUL/annotation colour resolves from the brand semantic scales (no Tailwind-palette leftovers). Per-surface accent spines stay distinct (strategy / outline / worldkb / timeline / layer accents) and canvas stays neutral with a subtle decorative grid; hierarchy comes from the node plane, border, selection ring/state text and semantic spine. The v0.4 hue-mapping appendix is retired — the rule is now simply "these families project from the same scales as everything else".

### Reading-chrome tokenization

The reading-chrome CSS block consumes named component tokens (`reading-chrome-novel-*`, `reading-chrome-essay-*`, `reading-chrome-screenplay-*`, …) projected from DESIGN frontmatter; the novel-profile chapter title absorbs its former hardcoded serif into the shared display family (now sans). Shared primitives own their focus-visible treatment; app surfaces must not rely on global CSS for it.
