# apps/design-studio — AGENTS.md

**Design Studio** — contributor and frontend-dev gallery for the Nexus DESIGN SSOT, brand VI, and UI primitives.  
Parent rules: [`../AGENTS.md`](../AGENTS.md) (apps placement), root [`AGENTS.md`](../../AGENTS.md).

## Placement

- **Product surface** under `apps/design-studio` (polyglot `apps/` rule — same as `apps/web`)
- **Consumer**, not producer — no daemon, no `nexus42` embed, no `@42ch/nexus-contracts`
- **Not author-facing** — never shipped in Control Room or desktop installer as a product route

## SSOT

- Design tokens: repo-root [`DESIGN.md`](../../DESIGN.md) + [`DESIGN.dark.md`](../../DESIGN.dark.md) only
- CSS projection: [`@nexus/design-tokens`](../../tooling/design-tokens) (`tokens.css` + Tailwind preset) — shared with `apps/web`
- Normative spec: [`.mstar/specs/design-studio.md`](../../.mstar/specs/design-studio.md)
- Merge rules: [`.mstar/iterations/v1.98/specs/design-unification.md`](../../.mstar/iterations/v1.98/specs/design-unification.md)

## Import boundaries (HARD)

### Allowed

| Alias | Resolves to | Use |
| --- | --- | --- |
| `@/*` | `./src/*` | Studio routes, fixtures, gallery layout |
| `@web-ui/*` | `../web/src/components/ui/*` | Transitional gallery source for not-yet-promoted primitives |
| `@web-lib/utils` | `../web/src/lib/utils.ts` | `cn()` only |
| `@42ch/nexus-ui` | workspace package | Brand VI plus V1.99-approved presentational primitives through public exports |
| `@nexus/design-tokens` | `tooling/design-tokens` | Shared CSS + Tailwind preset |

### Forbidden

- `apps/web/src/lib/nexus/**` — no `NexusClient`, no daemon transport
- `apps/web/src/pages/**` — no product screens
- `apps/web/src/components/layout/**` — use studio-local Surfaces fixtures instead
- `apps/web` route definitions, app providers, product hooks, Tauri helpers, and localStorage-backed product state
- `@42ch/nexus-contracts` — no wire DTOs
- Inventing design tokens not in root DESIGN pair

## Transitional `apps/web` UI import policy (V1.98 → V1.99)

Gallery **displays** shadcn primitives from `apps/web/src/components/ui/*` without migrating them to `@42ch/nexus-ui`. This coupling is **intentional and transitional**:

- Import only presentational primitives (`button`, `dialog`, `tabs`, …)
- `tabs` barrel export landed in P0 T1 (commit `55dd06cc`); use `@web-ui/<module>` direct imports or barrel as needed
- Declare matching Radix/CVA peer versions in `package.json` (same majors as `apps/web`)
- V1.99 decoupling rule: once a primitive is promoted into `@42ch/nexus-ui`, Studio must import it from `@42ch/nexus-ui`, not `@web-ui/*`
- Unpromoted primitives may remain on `@web-ui/*` until a later promotion or explicit keep-studio/keep-web decision

## Dev commands

| Action | Command |
| --- | --- |
| Dev server | `pnpm --filter design-studio dev` (port **5174**) |
| Build | `pnpm --filter design-studio build` |
| Test | `pnpm --filter design-studio test` |

No daemon or Tauri required.

## Conventions

- TypeScript strict; match `apps/web` toolchain (Vite 6, React 18, Tailwind 3, react-router-dom v6)
- Theme toggle: `class` strategy on `<html>` — mirrors web `theme-provider` behavior
- Read-only gallery — no YAML write-back, no localStorage token overrides
- App chrome shows **Read-only · edit `DESIGN.md`** (repo-root SSOT helper)
- Voice & Content and Surfaces fixture strings: [IA guide §4.4–§4.5](../../.mstar/iterations/v1.98/guides/design-studio-information-architecture.md) — sourced from DESIGN § Voice & Content and shipped product copy

## Audiences

| Audience | Role |
| --- | --- |
| Contributors (design-minded maintainers) | Tune colors, typography, spacing, and component tokens |
| Frontend developers | Pick correct variant/state when building screens; use component matrix as reference |
| Brand / VI reviewers | Confirm logo usage, clear space, and theme.css alignment |
| Authors (local Web UI users) | **Not in scope** — studio is not bundled in `nexus42` or desktop installer |

See [design-studio.md spec §2](../../.mstar/specs/design-studio.md#2-audiences) for audience job-to-be-done detail.

## Tests

- Runner: Vitest 3 with jsdom + @testing-library/react — mirrors `apps/web` conventions
- Config: `vitest.config.ts` (resolve aliases match `vite.config.ts`; setup in `src/test/setup.ts`)
- Scope: smoke tests for App shell render, theme toggle, and gallery section routing — see `src/App.test.tsx`
- Run: `pnpm --filter design-studio test` (CI-compatible; no daemon required)
