# Developer rail app rebuild pattern

Promoted to: `.mstar/knowledge/architecture-patterns/developer-rail-app-rebuild.md` (this file). Derived from iteration v1.221-developer-rails (plans v221-modules-rail, v221-caps-sessions-rail); precedents: v1.220 developer shell (Hub/Harness), v1.219 shell skeleton.

## When to use

Rebuilding an `apps/web` rail app to the post-2026-10-22 generation standard (single SPA, app-shell IA per `.mstar/specs/proposed/surfaces/web-rebuild-ia.md`, web-first). Verified three times in v1.221 (modules, capabilities, sessions) after v1.220 established it (Hub, Harness).

## The recipe (per rail app)

1. **Routing/ownership first.** A rail app must have exactly one entry point. If a legacy surface is Settings-modal-owned or a compatibility redirect, delete the modal machinery outright (section descriptor, canonicalization special case, entrance-registry row, modal-primary "last safe location" special case) — no redirect; the product has never shipped. Verify with a repo-wide grep for the dead path (tracked sources only; `dist/` is gitignored).
2. **Move the body into `components/developer/`**, split at the **pure-view boundary**: presentational files import only `react`, `lucide-react`, `@42ch/nexus-ui`, `@/components/ui/*`, `@/lib/utils` (plus `@/components/developer/hub-card` when composing cards). No hooks, router, i18n, transport, or query imports in chrome; copy arrives via props, data via view models, behavior via callbacks. App wiring (queries, search-param seed/write-back, copy assembly) stays in the page component.
3. **Registry wiring**: repoint the `AppDef.load` dynamic import in `lib/app-registry.ts`; AppId/route/icon unchanged. Entrance rules (`entrance-registry.ts`) keep the route row; delete modal-era rows.
4. **Design Studio fixture** (repo convention: every new visual surface, both themes, all variants). Wire through the established five points — reuse, never rebuild: `@web-developer` alias in design-studio `tsconfig.json` + `vite.config.ts` **and `vitest.config.ts`** (standalone config; forgetting it breaks fixture-test resolution), `EXTRACT_IMPORT_ROOTS` in `surface-source-badge.tsx`, `SURFACES_SHELL_ENTRIES` in `gallery-index.ts`, a sibling section in `surfaces.tsx` (`SurfacesShellPage`). No new Studio route.
5. **Locale parity**: the `locale-parity.test.ts` already registers all rail namespaces — verify, never modify. Chrome copy is assembled in the page and passed down; fixture copy is literal.
6. **Fixture tests must pin instance identity**: a chrome mounted N times in a gallery needs per-instance ids — derive ids with React `useId()` inside the chrome (self-consistent `htmlFor`/`id`), and assert pairwise-distinct ids with a negative check (reverting to the hardcoded id must turn the test red). Lesson: PR #444 Greptile round (validate-preset) and v1.221 QC (caps filter) both caught hardcoded ids.

## Traps learned

- Deep links are contracts: preserve every param form verbatim (`?module=&run=`, `?world=&branch=`, `?filter=`) with seed-on-mount/read-only semantics; canvas/timeline entries navigate with those params.
- Old `pages/<app>-page.tsx` bodies sometimes are `<Navigate>` redirects — replacing them means the route finally renders a body; consumers calling `openSettings('<app>')` are forced cascades.
- Status display reuses the v1.220 `statusVariant` mapping as-is (`running`/`completed` green, `failed` red); never invent a variant per app.
- List ordering contracts survive visual rebuilds: keep the documented sort (e.g. sessions F-F1) and assert DOM order equality against a deliberately non-alphabetical fixture list.
- Killed table layouts leave orphaned locale keys (`columns.*`) — sweep both locales.
