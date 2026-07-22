---
module: packages/nexus-ui + apps/web
date: 2026-07-03
problem_type: architecture-pattern
category: architecture-patterns
severity: medium
plan_id: 2026-07-03-v1.87-nexus-ui-component-library (compound of V1.87); 2026-07-22-vi-logo-upgrade (shell primary-only)
tags: [nexus-ui, react-component-library, tsup, esbuild, bundler-agnostic, svg, vite, peer-deps, presentational-component, logo-system]
applies_when: adding an asset-consuming React component to a publishable workspace package built with tsup/esbuild, or promoting an assets-only package to a component library
last_updated: 2026-07-22 (shell uses logo-primary only; drop theme-split logo-color wrapper)
---

# Bundler-Agnostic Component Library Assets

**Track**: Knowledge (durable guidance distilled from V1.87 `@42ch/nexus-ui` component-library promotion).

## Context

`@42ch/nexus-ui` shipped in V1.83 as an assets/tokens/theme-CSS-only package (deliberately "framework-neutral — no React components"). Its README deferred a React component library to "a later plan after P2 proves stable usage." V1.87 was that plan: it promoted the package to ship `<NexusLogo>` + `<NexusMark>`.

The V1.87 iteration-start draft assumed the package's React components could import the canonical `.svg` logo assets directly (e.g. `import logoPrimary from './assets/logos/logo-primary.svg'`), exactly as `apps/web` already does. **This assumption was wrong**, and the architect caught it during the Phase 1 Review & Edit chain.

## Guidance (the pattern)

A publishable React component library built with **tsup/esbuild** cannot import static asset files (`.svg`, `.png`, etc.) in its source the way an application bundler (Vite, Webpack) resolves them. tsup/esbuild do not emit URL strings for `.svg` imports — they either fail to resolve the import or inline the file as a base64 data URL, neither of which matches the `<img src="...">` contract the app expects. Building the library therefore breaks, or the built output is wrong.

The **bundler-agnostic** resolution is to invert the asset-resolution responsibility:

1. **The library component does NOT import the asset.** It accepts the resolved asset URL as a **prop** (`src: string`). It remains a pure presentational component — no bundler-specific loader, no asset coupling.
2. **The consumer resolves the asset** through its own application bundler. `apps/web` uses Vite, which natively resolves `.svg` imports to URL strings. The consumer imports the asset via the package's public `exports` map (e.g. `@42ch/nexus-ui/assets/logos/logo-primary.svg`) and passes the URL to the component.
3. **A thin app-local wrapper preserves call-site ergonomics.** The consumer keeps a tiny wrapper (e.g. `apps/web/src/components/brand/nexus-logo.tsx`) that imports the chosen SVG via Vite and passes `src` to the package component. Call sites stay zero-prop (`<NexusLogo />`). **Chronos shell policy:** import **`logo-primary.svg` only** (theme-stable deep-blue plate). Do not theme-split shell lockups (`primary` vs `color` / `whiteBg` by `useTheme()`).

For inline-SVG marks (no `<img>`), hand-author the path data as JSX inside the library component. This is bundler-agnostic (no asset import at all) and enables `currentColor` inheritance. Only do this when the SVG is small and stable (the Nexus mark is ~22 source lines); for large/changing artwork, prefer the `src`-prop pattern with the consumer resolving the asset.

### Companion: dependency posture

When promoting an assets-only package to a React component library:
- Add `react` + `react-dom` as **peerDependencies** (consumers already ship React; the library must not bundle its own copy).
- Add `@types/react` + `@types/react-dom` as **devDependencies** (for the library's own typecheck/build).
- Enable JSX in the library's `tsconfig.json` (`"jsx": "react-jsx"`). tsup's esbuild auto-detects JSX — no `--jsx` CLI flag; the existing `--format cjs,esm --dts` build command suffices.
- Add a test runner (vitest + jsdom) as dev deps if the package had none; component unit tests mock the `src` prop (no `.svg` import in tests either).

## Why This Matters

- **Bundler neutrality**: the library builds with tsup and is consumable by any bundler (Vite, Webpack, Rollup, Turbopack) without per-bundler loader configuration. Coupling the library to Vite's `.svg` resolution would lock every consumer to Vite.
- **Single asset source**: the canonical SVGs still live in the package (`assets/logos/*.svg`) and are exposed via the `exports` map; the consumer resolves them, but the package remains the single source of truth. No asset duplication.
- **Clean separation**: presentational components (library) vs. theme/wiring (app wrapper). The library never imports a theme context; the app never duplicates brand markup.

## When to Apply

- Adding a React component to `@42ch/nexus-ui` (or any tsup-built workspace package) that renders an image/SVG asset.
- Promoting an assets-only package to a component library.
- Deciding whether a mark should be `<img src>` (variant assets → `src`-prop pattern) vs. inline JSX (mono/`currentColor` mark → hand-authored JSX).

## What Did NOT Work

- **Importing `.svg` directly in the package component source.** tsup/esbuild cannot resolve `.svg` to a URL string the way Vite does. The V1.87 iteration-start draft assumed this would work (mirroring apps/web's existing imports); the architect's Review & Edit pass caught it before implementation. If it had reached the build, `pnpm --filter @42ch/nexus-ui run build` would have failed or produced a wrong (base64-inlined) output.
- **Shipping a theme context inside the library.** Considered and rejected: the app's theme infra (localStorage / `.dark` class / Tailwind strategy) is generic, not brand-specific, and putting it in a brand-named package is a naming mismatch. The `src`-prop design sidesteps this entirely — the library component is theme-agnostic; the app wrapper owns asset selection.
- **Theme-split shell logos.** Pre-Chronos wrappers switched `logo-primary` / `logo-color` by light/dark. Chronos primary is a **deep plate** lockup that reads on both shells; theme-split is unnecessary and drifts from DESIGN logo tables.

## Do NOT

- Import `.svg`/`.png`/other static assets in a tsup-built library's component source.
- Bundle `react`/`react-dom` as runtime deps of the library (use peer deps).
- Put the app's theme-resolution logic inside the library component (keep it in the thin app wrapper).
- Add per-bundler loader config to the library build (defeats bundler neutrality).
- Reintroduce `logo-color.svg` or theme-split shell imports without an explicit DESIGN + `logoVariants` change.

## Examples

- **`<NexusLogo variant src label? className? size?>`** (`packages/nexus-ui/src/components/nexus-logo.tsx`) — accepts the consumer-resolved SVG URL via `src`; renders `<img>`. `logoVariants`: `primary | whiteBg | white | mono | text` (no `color`).
- **`<NexusMark label? className? size?>`** (`packages/nexus-ui/src/components/nexus-mark.tsx`) — hand-authored wide timeline SVG JSX; `currentColor` inheritance; no asset import.
- **Thin wrapper** (`apps/web/src/components/brand/nexus-logo.tsx`) — imports `logo-primary.svg` via Vite, passes `src` + `variant="primary"` to the package `<NexusLogo>`. Studio shell does the same. Call sites stay zero-prop.
- **White plate exception:** import `logo-white-bg.svg` only when the surface must be a light/white plate (not default chrome).

## Related

- [`nexus-brand-token-hierarchy.md`](nexus-brand-token-hierarchy.md) — token layers, Chronos dual-role (ink vs cyan), and logo variant table. This doc covers *how* a library component consumes assets bundler-agnostically; that doc covers *where* brand tokens/assets live and *which* logo to pick.
