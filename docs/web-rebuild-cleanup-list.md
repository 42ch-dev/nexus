# Web Rebuild Cleanup List

Status: list only · 2026-10-22 · branch `feat/ui-teardown-docs`

## 1. Purpose & discipline

This is the input list for the web rebuild's first cleanup wave. **Nothing is deleted in this round** — every entry keeps its evidence and a disposition label. Labels: `delete-at-rebuild` (clear legacy/unwired evidence, delete when rebuild lands), `merge-at-rebuild` (current chrome that should be merged/replaced by the new shell rather than deleted outright), `verify-then-delete` (grep-level evidence only; re-verify routing/imports/tests before deleting). All evidence is grep/routing-level from DeadRoutesSurvey; **AST-level dead-code proof was NOT done** — hidden dynamic imports, external entry points, or retained test contracts may change a disposition.

## 2. Zombie / unreachable pages

Pages present in `apps/web/src/pages/` but not registered as elements in `App.tsx` routes; only test files import them.

| Path | Evidence | Label |
|---|---|---|
| `apps/web/src/pages/worlds-page.tsx` | Not in `App.tsx` routes; only importers found: `worlds-page.test.tsx`, `components/__tests__/worlds-list-activity.test.tsx`, `pages/__tests__/world-default-route.test.tsx`. `/worlds` currently mounts `CreatorHubPage`. | verify-then-delete |
| `apps/web/src/pages/works-page.tsx` | Not in `App.tsx` routes; only importers found: `works-page.test.tsx`, `components/__tests__/works-list-activity.test.tsx`. `/works` currently mounts `CreatorHubPage`. | verify-then-delete |
| `apps/web/src/pages/work-detail-page.tsx` | Not in `App.tsx` routes; only importer found: `work-detail-page.test.tsx`. Work detail routing is now `WorkShellLayout` + Outline/Timeline/Chapters/Inspector sub-routes. | verify-then-delete |
| `apps/web/src/pages/settings/settings-shell-layout.tsx` | Not a product-route element; file comment states full-page Settings is no longer the main flow (Settings runs via `SettingsModalHost` / `SettingsSectionFrame`) and the file is kept only for isolated section unit tests. | verify-then-delete |
| `apps/web/src/pages/settings/settings-modules-section.tsx` | `/modules` registers `ModulesPage` directly (compat route with a real element); `App.tsx` comment says the Settings modal owns Modules. Cannot rule out a runtime entry from current evidence. | verify-then-delete |

Deleting any row above requires deleting/migrating its test consumers in the same rebuild change.

## 3. Legacy shell candidates

| Path | Evidence | Label |
|---|---|---|
| `apps/web/src/components/layout/header.tsx` | Standalone Header; `RootLayout` uses `ChronosTitlebar` instead. Only usage found is its own test. | verify-then-delete |
| `apps/web/src/components/layout/main-banner.tsx` | One-off daemon status banner with browser/daemon branches and dedicated tests; no production mount point confirmed. | verify-then-delete |
| `apps/web/src/components/layout/chronos-titlebar.tsx` | **Current chrome** — active titlebar, hosts the Settings button opening the settings modal. Not a deletion candidate; fold into new chrome if the rebuild unifies the titlebar. | merge-at-rebuild |
| `apps/web/src/components/layout/settings-modal-host.tsx` | **Current chrome** — mounted by `App.tsx` inside `SettingsModalProvider`; carries the live Settings modal flow. | merge-at-rebuild |
| `apps/web/src/components/layout/work-shell-layout.tsx` | **Current chrome** — used by `App.tsx` for `/works/:workId` sub-routes; a work-scoped layout, not an old top-level shell. Re-check only if the rebuild unifies canvas shells. | merge-at-rebuild |

(`settings-shell-layout.tsx` appears in §2 as a zombie/verify-then-delete entry; its "kept for isolated tests" comment is the reason it is not `delete-at-rebuild`.)

## 4. Dead exports (zero non-test importers)

Grep-level evidence: no non-test importers found for these page-level exports. Not AST-proven; re-verify at rebuild.

| Path | Export | Evidence | Label |
|---|---|---|---|
| `apps/web/src/pages/worlds-page.tsx` | `WorldsPage` | Only test-file references; product route uses `CreatorHubPage`. Overlaps zombie-page row above. | verify-then-delete |
| `apps/web/src/pages/works-page.tsx` | `WorksPage` | Only test-file references; product route uses `CreatorHubPage`. Overlaps zombie-page row above. | verify-then-delete |
| `apps/web/src/pages/work-detail-page.tsx` | `WorkDetailPage` | Only its own test imports it; product routing is `WorkShellLayout` + sub-pages. Overlaps zombie-page row above. | verify-then-delete |

## 5. Redirect-only stubs kept on purpose (compat)

| Route | Target | Status |
|---|---|---|
| `/strategy` | `/strategies` (query string preserved, `StrategyRedirect` in `App.tsx`) | kept on purpose — compat |
| `/connect` | `/settings/advanced#connection` (`<Navigate replace />`) | kept on purpose — compat |
| `/modules` | — | **not a stub**: compat route with a real element (`ModulesPage`); Settings modal owns the Modules section per `App.tsx` comment |

Also structural, not dead: `/works/:workId/` and `/worlds/:worldId/` index redirects; `*` → `NotFoundPage`.

## 6. Out of scope notes

- `pages/rft-native-proof-page.tsx` is **dev-gated, not dead**: conditionally routed only when `import.meta.env.DEV && import.meta.env.VITE_RFT_NATIVE_PROOF === '1'`, outside the main `Routes` tree.
- `apps/desktop-electron` shows no web-shell references (RootLayout / SettingsModalHost paths) in a light grep; no cleanup implication from the desktop app.
- Route index redirects (`/works/:workId/` → outline, `/worlds/:worldId/` → timeline) are intentional navigation, not dead code.
