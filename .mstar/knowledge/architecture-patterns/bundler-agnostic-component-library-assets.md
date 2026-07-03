---
module: packages/nexus-ui + apps/web
date: 2026-07-03
problem_type: architecture-pattern
category: architecture-patterns
severity: medium
plan_id: 2026-07-03-v1.87-nexus-ui-component-library (compound of V1.87)
tags: [nexus-ui, react-component-library, tsup, esbuild, bundler-agnostic, svg, vite, peer-deps, presentational-component]
applies_when: adding an asset-consuming React component to a publishable workspace package built with tsup/esbuild, or promoting an assets-only package to a component library
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
3. **A thin app-local wrapper preserves call-site ergonomics.** The consumer keeps a tiny wrapper (e.g. `apps/web/src/components/brand/nexus-logo.tsx`) that imports the SVGs via Vite, derives the right variant (here: from the app's theme context), and passes `src` to the package component. Call sites stay zero-prop (`<NexusLogo />`); the theme→variant→URL mapping is centralized in one app-local file.

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
- **Shipping a theme context inside the library.** Considered and rejected: the app's theme infra (localStorage / `.dark` class / Tailwind strategy) is generic, not brand-specific, and putting it in a brand-named package is a naming mismatch. The `src`-prop design sidesteps this entirely — the library component is theme-agnostic; the app wrapper owns theme→variant.

## Do NOT

- Import `.svg`/`.png`/other static assets in a tsup-built library's component source.
- Bundle `react`/`react-dom` as runtime deps of the library (use peer deps).
- Put the app's theme-resolution logic inside the library component (keep it in the thin app wrapper).
- Add per-bundler loader config to the library build (defeats bundler neutrality).

## Examples (V1.87)

- **`<NexusLogo variant src label? className? size?>`** (`packages/nexus-ui/src/components/nexus-logo.tsx`) — accepts the consumer-resolved SVG URL via `src`; renders `<img>`. Exports `VARIANT_FILENAMES` for programmatic filename lookup.
- **`<NexusMark label? className? size?>`** (`packages/nexus-ui/src/components/nexus-mark.tsx`) — hand-authored inline mono SVG JSX; `currentColor` inheritance; no asset import.
- **Thin wrapper** (`apps/web/src/components/brand/nexus-logo.tsx`) — imports `logo-color.svg`/`logo-primary.svg` via Vite, derives variant from `useTheme()`, passes `src` to the package `<NexusLogo>`. Call sites (`sidebar.tsx`, `header.tsx`) stay zero-prop.

## Related

- [`nexus-brand-token-hierarchy.md`](nexus-brand-token-hierarchy.md) — the four-layer brand token/asset hierarchy (root DESIGN → package → app mapping → impl). This doc covers *how* a library component consumes assets bundler-agnostically; that doc covers *where* the brand tokens/assets live.
