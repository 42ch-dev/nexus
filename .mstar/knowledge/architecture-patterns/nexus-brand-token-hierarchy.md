---
module: packages/nexus-ui + apps/web + apps/design-studio + tooling/design-tokens + repo-root DESIGN
date: 2026-07-06
problem_type: architecture-pattern
category: architecture-patterns
severity: medium
plan_id: V1.83-P-last (compound of brand UI foundation iteration); V1.94-P-last (contrast rule correction); V1.98-P0 (DESIGN SSOT unification + shared token pipeline + design-studio)
tags: [brand, design-tokens, nexus-ui, design-md, git-lfs, svg, npm-package, button-contrast, dark-theme, design-studio, tailwind-preset, ssot-unification]
applies_when: adding or consuming cross-application Nexus brand/design tokens (new product surface, platform package, Web shell refresh, or a new app consuming the design system); also when defining any button background/text colour combination
last_updated: 2026-07-08 (V1.98 unification: root DESIGN pair is now the SOLE full token SSOT; apps/web/DESIGN*.md retired; shared @nexus/design-tokens pipeline extracted; apps/design-studio added as read-only gallery consumer)
---

# Nexus Brand & Design Token Hierarchy

**Track**: Knowledge (durable guidance distilled from V1.83 Brand UI Foundation; corrected V1.94; unified V1.98).

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
