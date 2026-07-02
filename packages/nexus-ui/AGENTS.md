# @42ch/nexus-ui — AGENTS.md

Publishable npm workspace package for Nexus brand assets, design tokens, and theme CSS. V1.83 scope is **assets/tokens/theme only** — no React components.

## Purpose

- Canonical SVG logo variants and PNG source provenance (LFS)
- Machine-consumable brand token constants (`src/tokens.ts`)
- Optional CSS custom properties (`theme.css`)

## Boundaries

- **Must not** import from `apps/web`, `nexus-platform`, or app routing/state
- **Must not** export React components, shadcn wrappers, or Web-only primitives in V1.83
- **Must not** duplicate the full DESIGN.md token contract — root `DESIGN.md` / `DESIGN.dark.md` (P1) own normative cross-app tokens; this package exposes a minimal brand primitive slice

## Dependencies

- Zero runtime dependencies; `typescript` + `tsup` dev-only for build/typecheck
- Consumers: `apps/web` (P2), future `nexus-platform` surfaces via public exports

## Asset policy

| Asset type | Storage | Role |
|------------|---------|------|
| `assets/logos/*.png` | Git LFS | Source/provenance references only |
| `assets/logos/*.svg` | Normal git text | App-consumable canonical marks |

## Public exports

Documented in `package.json` `exports` and `README.md`. Do not rely on undocumented internal paths.

## Pre-release

Package version and export paths may change before 1.0. Coordinate breaking export changes with P1/P2 plan owners.
