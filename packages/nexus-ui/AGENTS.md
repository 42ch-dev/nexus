# @42ch/nexus-ui — AGENTS.md

Publishable npm workspace package for Nexus brand assets, design tokens, theme CSS, **React brand components** (`<NexusLogo>`, `<NexusMark>`), and V1.99-approved pure presentational primitives. V1.83 shipped the assets/tokens/theme foundation; V1.87 promoted the package to a React component library (adds `react` / `react-dom` as peer deps); V1.99 may promote a small UI primitive batch under the component-promotion boundary.

## Purpose

- Canonical SVG logo variants and PNG source provenance (LFS)
- Machine-consumable brand token constants (`src/tokens.ts`)
- Optional CSS custom properties (`theme.css`)
- React brand primitive components (`<NexusLogo>`, `<NexusMark>`) — presentational only, no theme context
- Approved React UI primitives that are pure presentational, token-driven, and reusable across `apps/design-studio` and `apps/web`

## Boundaries

- **Consumer wrappers** under `apps/web/src/components/ui/` that re-export from this package (`button.tsx`, `badge.tsx`, `card.tsx`, `input.tsx`, `label.tsx`, `textarea.tsx`, `select.tsx`) **must not** import `clsx`, `class-variance-authority`, `tailwind-merge`, `@/lib/*`, or deep-import `@42ch/nexus-ui/src/*`. The package is the sole class-merge authority (`cn`). Enforced by `tooling/check-ui-guardrails.sh` (CI job `ui-guardrails`).

- **Must not** import from `apps/web`, `apps/design-studio`, `nexus-platform`, app aliases, daemon clients, app routing/state, Tauri IPC, or localStorage
- **Must not** export product screens, layout shells, Web-only primitives, data-aware controls, or app chrome
- **May** export plan-approved presentational primitives only when the active plan/spec records the component in the promotion list
- **Must not** duplicate the full DESIGN.md token contract — root `DESIGN.md` / `DESIGN.dark.md` own normative cross-app tokens; this package references token-backed class names and keeps only the existing brand primitive slice in `theme.css`
- **Must not** import `.svg` files in package source — tsup/esbuild cannot resolve `.svg` imports. Components that need SVG assets use consumer-provided `src` prop (NexusLogo) or hand-authored JSX (NexusMark) for bundler-agnostic portability.
- **Must not** import `.png` or other runtime assets from component source. Keep assets behind documented public asset exports or consumer-resolved `src` props.
- **Must** own its own `cn` helper (`clsx` + `tailwind-merge` with DESIGN.md token class-group extension) for class composition. It **must not** import from `apps/web/src/lib/utils.ts` or any app-local utility.

## Dependencies

- **Runtime/peer**: `react` (>=18), `react-dom` (>=18) — peer deps only (consumers ship React)
- **Runtime/package deps**: non-singleton implementation helpers are allowed only when imported by promoted primitives (for example `class-variance-authority`, `@radix-ui/react-slot`, `clsx`, `tailwind-merge`). Do not make these peer dependencies unless consumers must share a singleton instance.
- **Dev-only**: `typescript`, `tsup`, `@types/react`, `@types/react-dom`, package tests
- Consumers: `apps/web` and `apps/design-studio` (workspace); future external surfaces via public exports

## Asset policy

| Asset type | Storage | Role |
|------------|---------|------|
| `assets/logos/*.png` | Git LFS | Source/provenance references only |
| `assets/logos/*.svg` | Normal git text | App-consumable canonical marks |

## Public exports

Documented in `package.json` `exports` and `README.md`. Do not rely on undocumented internal paths.

### V1.99 primitive export strategy

- First-batch primitives use named exports from `@42ch/nexus-ui` through `src/index.ts`.
- Do not add per-component deep public subpaths unless the active plan explicitly locks that API.
- Keep `src/components/*` internal; consumers must not deep-import package source files.
- If apps need app-specific behavior, keep a thin wrapper in the app and import the presentational primitive from the package.

### Component export strategy

- **`<NexusLogo>`**: bundler-agnostic — accepts `src` prop (consumer resolves SVG URL through their bundler, e.g. Vite). Does not import `.svg` files.
- **`<NexusMark>`**: hand-authored inline SVG JSX (no asset import) — inherits color via `currentColor`.

## Pre-release

Package version and export paths may change before 1.0. Coordinate breaking export changes with the active iteration's plan owner(s) before shipping.
