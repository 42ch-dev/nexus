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
| `@/components/ui/*` | `../web/src/components/ui/*` | Mirror alias for direct per-module imports (used by Vite/Vitest/tsconfig); transitional until promotion |
| `@web-ui/*` | `../web/src/components/ui/*` | Transitional gallery source for not-yet-promoted primitives |
| `@web-setup/*` | `../web/src/components/setup/*` | Gallery-only import of app-shared setup compositions (e.g. AgentPicker, WorkspacePathField) — props-driven; no contracts/daemon |
| `@web-layout/*` | `../web/src/components/layout/presentational/*` | Shell chrome extracts (sidebar, footer profiles, header/health) — props-driven; no routing or daemon hooks (V1.107) |
| `@web-settings/*` | `../web/src/components/settings/presentational/*` | Settings section chrome extracts (ConnectDaemon form, Setup section) — props-driven; no IPC (V1.107) |
| `@web-canvas/*` | `../web/src/components/canvas/presentational/*` | Canvas node-chrome extracts (`NodeChromeShell`) — props-driven; no `@xyflow/react`, no RF types (V1.115) |
| `@web-lib/utils` | `../web/src/lib/utils.ts` | `cn()` only |
| `@42ch/nexus-ui` | workspace package | Brand VI plus promoted presentational primitives (Button, Badge, Card, Input, Label, Textarea, Select, Toast) through public exports |
| `@nexus/design-tokens` | `tooling/design-tokens` | Shared CSS + Tailwind preset |

### Forbidden

- `apps/web/src/lib/nexus/**` — no `NexusClient`, no daemon transport
- `apps/web/src/pages/**` — no product screens
- `apps/web/src/components/layout/**` except via `@web-layout/*` presentational extracts — no direct import of routing-heavy `sidebar.tsx`, `root-layout.tsx`, or daemon-wired layout modules
- `apps/web/src/components/settings/**` except via `@web-settings/*` presentational extracts — no live ConnectDaemonForm with IPC in Studio
- `apps/web/src/hooks/**` — no product hooks
- `apps/web/src/(providers|contexts)/**` — no app providers
- `@42ch/nexus-contracts` — no wire DTOs
- `@42ch/nexus-ui/src/*` — deep import; use public package API only
- `@tauri-apps/*` — desktop-only; studio is a browser SPA
- `@web-ui/button`, `@web-ui/badge`, `@web-ui/card`, `@web-ui/input`, `@web-ui/label`, `@web-ui/textarea`, `@web-ui/select` — already-promoted; import from `@42ch/nexus-ui`
- Inventing design tokens not in root DESIGN pair
- **Any `@web-ui/*` import without a transitional annotation** (`// transitional — …` or `// @web-ui/<name> — transitional …`)

**Guardrails:** `tooling/check-ui-guardrails.sh` (CI job `ui-guardrails`) enforces these boundaries mechanically.

## Transitional `apps/web` UI import policy (post-V1.99 promotion waves)

Gallery **displays** shadcn primitives from `apps/web/src/components/ui/*` without migrating them to `@42ch/nexus-ui`. This coupling is **intentional and transitional**:

- Import only presentational primitives (`button`, `dialog`, `tabs`, …)
- `tabs` barrel export landed in P0 T1 (commit `55dd06cc`); use `@web-ui/<module>` direct imports or barrel as needed
- Declare matching Radix/CVA peer versions in `package.json` (same majors as `apps/web`)
- Decoupling rule: once a primitive is promoted into `@42ch/nexus-ui`, Studio must import it from `@42ch/nexus-ui`, not `@web-ui/*`
- Unpromoted primitives may remain on `@web-ui/*` until a later promotion or explicit keep-studio/keep-web decision
- **Transitional annotation required:** every unpromoted `@web-ui/*` import in Studio source files must carry an inline comment identifying the blocking criteria for promotion (e.g., `// @web-ui/label — transitional until Form Field slice locks label/control/helper/error composition`). This ensures the dependency's temporary status and promotion trigger are visible to future contributors.
- **Annotation placement:** the `// transitional` marker must appear on the **line containing the module path** (the `'@web-ui/<name>'` line). This is the most robust anchor — the quoted module specifier is a single lexical token that cannot be split across lines, so it covers single-line, multiline, and any `from`/path split. Placing the annotation on the line above the import is not valid under this convention. The guardrail in `tooling/check-ui-guardrails.sh` enforces this by checking the quoted module path line itself for the `transitional` keyword.

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
- Canvas surfaces fixture (`src/fixtures/canvas-surfaces-fixtures.tsx`) mirrors all three canvas surfaces — Outline, Strategy, and World KB — via the shared `canvas-*` / `canvas-worldkb-*` / `canvas-strategy-accent` tokens (light/dark driven by the `.dark` token block).

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
