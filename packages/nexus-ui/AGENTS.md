# @42ch/nexus-ui — AGENTS.md

Publishable npm workspace package for Nexus brand assets, design tokens, theme CSS, and **React brand components** (`<NexusLogo>`, `<NexusMark>`). V1.83 shipped the assets/tokens/theme foundation; V1.87 promoted the package to a React component library (adds `react` / `react-dom` as peer deps).

## Purpose

- Canonical SVG logo variants and PNG source provenance (LFS)
- Machine-consumable brand token constants (`src/tokens.ts`)
- Optional CSS custom properties (`theme.css`)
- React brand primitive components (`<NexusLogo>`, `<NexusMark>`) — presentational only, no theme context

## Boundaries

- **Must not** import from `apps/web`, `nexus-platform`, or app routing/state
- **Must not** export shadcn wrappers, layout primitives, or Web-only primitives
- **Must not** duplicate the full DESIGN.md token contract — root `DESIGN.md` / `DESIGN.dark.md` (P1) own normative cross-app tokens; this package exposes a minimal brand primitive slice
- **Must not** import `.svg` files in package source — tsup/esbuild cannot resolve `.svg` imports. Components that need SVG assets use consumer-provided `src` prop (NexusLogo) or hand-authored JSX (NexusMark) for bundler-agnostic portability.

## Dependencies

- **Runtime/peer**: `react` (>=18), `react-dom` (>=18) — peer deps only (consumers ship React)
- **Dev-only**: `typescript`, `tsup`, `@types/react`, `@types/react-dom`
- Consumers: `apps/web` (workspace); future `nexus-platform` surfaces via public exports

## Asset policy

| Asset type | Storage | Role |
|------------|---------|------|
| `assets/logos/*.png` | Git LFS | Source/provenance references only |
| `assets/logos/*.svg` | Normal git text | App-consumable canonical marks |

## Public exports

Documented in `package.json` `exports` and `README.md`. Do not rely on undocumented internal paths.

### Component export strategy

- **`<NexusLogo>`**: bundler-agnostic — accepts `src` prop (consumer resolves SVG URL through their bundler, e.g. Vite). Does not import `.svg` files.
- **`<NexusMark>`**: hand-authored inline SVG JSX (no asset import) — inherits color via `currentColor`.

## Pre-release

Package version and export paths may change before 1.0. Coordinate breaking export changes with the active iteration's plan owner(s) before shipping.
