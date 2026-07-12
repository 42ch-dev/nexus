# i18n Completion Spec (V1.113 P0)

## User value

Authors who choose **zh-CN** should see Control Room product pages (Works,
Schedule, Sessions, Strategies, Capabilities, details, and two common dialogs)
fully localized — not a mix of Chinese chrome and English page bodies after
V1.112 foundation work.

## Problem

V1.112 shipped i18n foundation + migrated primary UI surfaces. ~110 hardcoded
user-facing strings remain across 9 page/dialog files, so zh-CN users still see
English on those flows.

## Scope

Migrate all remaining hardcoded user-facing strings in the target files to
`t()` calls:

| File | Approx strings | Namespace |
|------|---------------|-----------|
| works-page.tsx | ~15 | `works` |
| schedule-page.tsx | ~14 | `schedule` |
| sessions-page.tsx | ~14 | `sessions` |
| strategies-page.tsx | ~14 | `strategies` |
| capabilities-page.tsx | ~13 | `capabilities` |
| work-detail-page.tsx | ~18 | `works` (`workDetail.*`) |
| strategy-page.tsx | ~5 | `strategies` (`strategyDetail.*`) |
| patch-work-dialog.tsx | ~7 | `works` (`dialog.patchWork.*`) |
| validate-preset-dialog.tsx | ~10 | `strategies` (`dialog.validatePreset.*`) |

**Namespace lock:** Page copy lives in dedicated `<page>.json` catalogs. `shell.json`
keeps route/chrome/not-found only. `shell.json` currently holds page-specific keys
(`works.*`, `workDetail.*`, `schedule.*`, `sessions.*`) scaffolded in V1.112 P0
but not yet consumed by the pages. Migrate these into the dedicated page namespace
files; no dual-SSOT left behind.

## Catalog hygiene fixes

1. Consolidate 7 duplicate `route.*` / `mobileNav.*` key pairs in `shell.json`
   (mobile nav consumes `route.*`) — R-P0-002
2. Add `isLocalePreference()` runtime guard to `setPreference` — R-P0-004
3. Cache `Intl.DateTimeFormat` / `Intl.RelativeTimeFormat` per-locale in
   `format.ts` — R-P1-002
4. Close R-P0-005 (not-found keys are used — not a bug)

## Acceptance criteria

- [ ] Nine target files have no hardcoded user-facing strings
- [ ] en + zh-CN key-set parity for new/updated namespaces
- [ ] Grep audit of production `pages` + product `components` finds no remaining
  user-facing hardcodes in those modules (design-studio / CLI excluded)
- [ ] Runtime guard + formatter cache covered by tests
- [ ] Existing i18n tests pass; migrated pages covered by tests (both locales)

## Conventions

- Follow V1.112 i18n conventions: caller-owned presentational copy
- Namespace: `<page-name>.json` per page (locked table above)
- Key naming: dot-separated, camelCase leaf
- Both `en` and `zh-CN` updated in lockstep
- Catalog paths: `apps/web/src/locales/{en,zh-CN}/`
- i18n config: `apps/web/src/lib/i18n/config.ts`
- CLI / daemon messages remain English-only
- Design-studio string migration is NOT in scope (P2 may fix design-studio tests only)

## Non-goals

- Languages beyond en / zh-CN
- Dynamic locale bundle splitting (R-P0-003)
- Manuscript / author content translation
- Wire contract changes
