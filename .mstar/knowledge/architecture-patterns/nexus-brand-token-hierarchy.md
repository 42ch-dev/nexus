---
module: packages/nexus-ui + apps/web + repo-root DESIGN
date: 2026-07-02
problem_type: architecture-pattern
category: architecture-patterns
severity: medium
plan_id: V1.83-P-last (compound of brand UI foundation iteration)
tags: [brand, design-tokens, nexus-ui, design-md, git-lfs, svg, npm-package]
applies_when: adding or consuming cross-application Nexus brand assets/tokens (new product surface, platform package, or Web shell refresh)
---

# Nexus Brand Token Hierarchy

**Track**: Knowledge (durable guidance distilled from V1.83 Brand UI Foundation).

## Context

Before V1.83, `apps/web/DESIGN.md` held both app-specific canvas/SOUL/findings tokens and de-facto brand colors. V1.83 introduces a publishable `@42ch/nexus-ui` package and root `DESIGN.md` / `DESIGN.dark.md` as the cross-application brand SSOT. Future surfaces (`nexus-platform`, desktop, additional SPAs) must reuse this hierarchy instead of redefining hex values locally.

## Guidance (the pattern)

Brand consumption follows **four layers**, top to bottom:

1. **Root brand SSOT** — repo-root `DESIGN.md` / `DESIGN.dark.md` own canonical brand token names, VI palette values (`#1E3A5F`, `#25D1E0`, `#FFFFFF`), logo usage rules, and accessibility intent. These files are normative for *shared* brand semantics.
2. **`@42ch/nexus-ui` package** — owns reusable artifacts derived from the root contract: Git LFS–tracked PNG provenance, canonical SVG logo variants (regular-git text), token data (`tokens.ts`), CSS theme entry (`theme.css`), and package metadata/exports. **V1.83 scope was assets/tokens/theme only; V1.87 promoted the package to also ship React brand components (`<NexusLogo>`, `<NexusMark>`) — see [bundler-agnostic-component-library-assets.md](bundler-agnostic-component-library-assets.md) for the component asset-handling convention.**
3. **App consumption mapping** — e.g. `apps/web/DESIGN.md` / `DESIGN.dark.md` map root brand tokens into app-specific Tailwind keys, CSS custom properties, and shadcn-style primitive mappings. Web files **derive** brand values; they must not become a second brand source (explicit disclaimers in frontmatter/body).
4. **App implementation** — shell/base primitives consume public package exports and mapped CSS variables. No deep imports into `packages/nexus-ui/src/**`; use declared `package.json` `exports` only.

### Asset policy

| Asset type | Storage | Consumption |
|------------|---------|-------------|
| PNG logo sources (provenance) | Git LFS under `packages/nexus-ui/assets/logos/*.png` | Reference only; not exported for runtime |
| SVG logos (canonical) | Regular git text under `assets/logos/*.svg` | Exported via `@42ch/nexus-ui` |
| Token/theme | `tokens.ts`, `theme.css` | Imported by apps via public exports |

### Contrast rule (V1.83 locked)

Cyan `#25D1E0` is **accent-only on white** (~1.9:1 — fails AA as body text). Primary actions on light surfaces use deep blue `#1E3A5F`. Dark mode primary button: cyan fill + deep blue text.

### Prebuild chain

Apps that consume `@42ch/nexus-ui` should run `pnpm --filter @42ch/nexus-ui run build` (or equivalent) in `prebuild` / `pretypecheck` hooks so workspace resolution and theme.css exist before Vite/Tailwind compile.

## Why This Matters

- **Single brand source** prevents drift when multiple product surfaces ship independently.
- **Package boundary** keeps brand artifacts publishable without coupling to `apps/web` routing, state, or React components.
- **LFS vs SVG split** preserves designer PNG references while keeping runtime assets diff-friendly and CDN/npm friendly.

## When to Apply

- Adding a new product surface that needs Nexus branding.
- Extending logo variants or brand tokens — update root DESIGN first, then package exports, then app mappings.
- Publishing `@42ch/nexus-ui` to npm (future) — export map must remain stable; breaking renames require coordinated semver.

## Do NOT

- Put canonical brand hex values only in `apps/web` without root DESIGN + package alignment.
- Export React components from `@42ch/nexus-ui` without following the bundler-agnostic asset convention (consumer resolves the SVG URL via its own bundler and passes it as a `src` prop — do NOT import `.svg` in package source; see [bundler-agnostic-component-library-assets.md](bundler-agnostic-component-library-assets.md)). *(V1.83's "no React components without a dedicated component-library plan" guard was satisfied by V1.87.)*
- Commit runtime SVG logos through Git LFS (breaks text diffs and bundler inlining).
- Use cyan `#25D1E0` as primary body text on white backgrounds.

## Examples (V1.83)

- Root SSOT: `DESIGN.md`, `DESIGN.dark.md`
- Package: `packages/nexus-ui` — `@42ch/nexus-ui` exports `theme.css`, `tokens`, logo SVGs
- Web mapping: `apps/web/DESIGN.md` — `blue-*` aliases → `--nexus-brand-*`
- Web implementation: `NexusLogo` (apps/web thin wrapper) imports `@42ch/nexus-ui/assets/logos/logo-color.svg` / `logo-primary.svg` (public export) and passes the resolved URL to the package's `<NexusLogo variant src>` component (V1.87).
