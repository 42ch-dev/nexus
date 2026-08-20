---
module: apps/web
date: 2026-07-12
problem_type: knowledge
category: architecture-patterns
severity: low
tags: [i18n, i18next, react-i18next, locale, LocaleProvider, command-registry, format, intl, design-studio, test-setup]
applies_when: Adding or extending i18n in apps/web; adding user-facing strings; wiring Intl formatters; setting up i18n in test environments that import shared web components
---

# Web i18n Architecture Pattern

## Context

V1.112 introduced standard i18n (`i18next` + `react-i18next`) to `apps/web` so
authors can use the Control Room in English or 简体中文. This document captures
the non-obvious architecture decisions that future implementers must follow.

## Guidance

### 1. LocaleProvider mirrors ThemeProvider

`apps/web/src/components/locale-provider.tsx` is a structural mirror of
`theme-provider.tsx`:
- `LocalePreference = 'system' | 'en' | 'zh-CN'` (parallel to `Theme`)
- localStorage key `nexus-web-locale`
- Resolve order: stored -> system (`navigator.language`; `zh*` -> `zh-CN`, else `en`) -> fallback `en`
- Side effects on change: `i18n.changeLanguage(resolvedLocale)` + `document.documentElement.lang = resolvedLocale`
- System-follow: `languagechange` event listener (parallel to ThemeProvider's `matchMedia`)

**Mount order in `main.tsx`:** `ThemeProvider -> LocaleProvider -> ToastProvider -> AppProviders -> App`

### 2. Nine namespaces - do not create new ones

All user-facing strings go into one of nine namespace files under
`src/locales/{en,zh-CN}/`:

| Namespace | Scope |
|-----------|-------|
| `common` | Shared actions, generic loading/error fragments, toast titles |
| `shell` | Header, sidebar, mobile nav, route titles, not-found |
| `settings` | Settings shell, section nav, Appearance page |
| `setup` | Setup wizard steps, validation |
| `canvas` | Canvas inspectors, panels, empty states |
| `reading` | Reading toolbar, inspectors |
| `findings` | Findings list/detail chrome |
| `memory` | Memory, soul, chapters |
| `commands` | Command-palette action labels + group headings |

All nine are registered in `config.ts` even when stubbed as `{}`.

### 3. Command registry: labelKey + render-time resolution

**Problem:** `useRegisterCommand` captures the `Command` object once per mount
(keyed by `id`). Storing resolved English `label` strings prevents instant
locale switch without re-registering every command.

**Solution:** Store translation keys on `Command`; resolve in `command-palette.tsx` at render time.

```typescript
// Command interface (labelKey, not label)
readonly labelKey: string;
readonly labelNs?: string;        // default 'commands'
readonly groupKey: string;
readonly groupNs?: string;        // default 'commands'
readonly keywordKeys?: readonly string[];
readonly keywordNs?: string;      // default 'commands'
```

In `command-palette.tsx`:
- `useTranslation('commands')` subscribes to language changes -> palette re-renders
- `useMemo([commands, t])` resolves keys to strings
- Pass resolved commands to `filterCommands` (ranking uses localized labels)

### 4. format.ts: active locale for all Intl calls

`getActiveLocale()` from `lib/i18n/active-locale.ts` returns the current resolved
locale (`'en'` or `'zh-CN'`). All `Intl.*` constructors in `format.ts` use it:

- `formatDateTime`, `formatDate`: `Intl.DateTimeFormat(getActiveLocale(), …)`
- `formatRelative`: `Intl.RelativeTimeFormat(getActiveLocale(), …)`
- `formatUtcAndLocal.local`: uses `getActiveLocale()`
- `formatUtcAndLocal.utc`: **stays `'en-US'`** (explicit UTC presentation)
- `humanizeStatus`: uses `i18n.t('common.status.<value>')` with `titleCaseStatus` fallback

### 5. Key convention

- Dot-separated within namespace file: `settings.appearance.language.label`
- English source strings: Title Case for nav/titles/buttons; sentence case for helpers
- Interpolation: i18next `{{name}}` syntax (no string concatenation in components)
- `useTranslation('<namespace>')` at component top level

### 6. Caller-owned copy

`@42ch/nexus-ui` and shared primitives receive `t()` output from the caller.
Do not bake product copy into shared packages.

## Why This Matters

- **Instant switch without remount:** The labelKey + render-time resolution
  pattern allows locale switching while the command palette is open. Adding
  `i18n.language` to `useRegisterCommand` effect deps would cause store churn
  (every command re-registers on locale change). This was explicitly avoided.
- **No undefined locale leakage:** Before V1.112, `format.ts` used
  `Intl.DateTimeFormat(undefined, …)` which defaults to the browser locale,
  not the user's UI preference. This caused dates to show in the browser
  language even when the UI was set to English.
- **Pattern reuse:** The LocaleProvider pattern can be reused for other
  user preferences (e.g., density, timezone display) by mirroring the same
  structure.

## When to Apply

- Adding any new user-facing string in `apps/web` -> use `t()` from day one
- Adding a new command -> use `labelKey`/`groupKey`, not resolved strings
- Adding a new Intl formatter -> use `getActiveLocale()`, never `undefined`
- Adding a new namespace -> **don't**; extend an existing one

## Examples

```typescript
// Component using i18n
const { t } = useTranslation('shell');
return <nav aria-label={t('aria.primary')}>{/* ... */}</nav>;

// Command registration with keys
useRegisterCommand({
  id: 'go.strategy',
  labelKey: 'go.strategy.label',
  groupKey: 'group.navigate',
  // ...
});

// Format with active locale
import { getActiveLocale } from './i18n/active-locale';
new Intl.DateTimeFormat(getActiveLocale(), { dateStyle: 'medium' });
```

## What Didn't Work

- **Adding `i18n.language` to `useRegisterCommand` effect deps:** Caused
  store churn - every registered command would re-register on locale change.
  Solution: store keys, resolve at render time in the palette.
- **Using `undefined` locale in `Intl.DateTimeFormat`:** Defaults to browser
  locale, not UI preference. Must use `getActiveLocale()`.
- **Hardcoded `aria-label` on mobile nav:** P0 missed migrating the mobile nav
  `aria-label="Primary"` while the sidebar used `t('aria.primary')`. QC caught
  this as an inconsistency.

## V1.113 Additions

### IntlFormatterCache (V1.113 P0)

`format.ts` caches `Intl.DateTimeFormat` and `Intl.RelativeTimeFormat` instances
per locale + options combo. The cache key includes `JSON.stringify` of sorted
option keys to handle different `dateStyle`/`timeStyle` combinations. The cache
is unbounded but negligible (~6 entries max for 2 locales x ~4 option combos).
`formatUtcAndLocal` uses hardcoded `'en-US'` for the UTC branch (locale-independent).

### Catalog namespace migration (V1.113 P0)

Page-specific catalogs live in `src/locales/{en,zh-CN}/<page-name>.json`. Do not
dual-SSOT page copy under both `shell.json` and a page namespace. When migrating,
remove page-specific keys from `shell.json` and place them in the dedicated
namespace file. `shell.json` should contain only route/chrome/not-found/daemon/
profile/health keys.

### Design-studio test setup (V1.113 P2)

When `apps/design-studio` imports shared components from `apps/web` (via
`@web-setup/*` aliases) that use `useTranslation()`, the design-studio test
environment must initialize i18next. Fix: import `@/lib/i18n/config` in
`apps/design-studio/src/test/setup.ts` and reset locale to `en` in `beforeEach`.
Also ensure fixture agent IDs match `COMMON_AGENT_PRIORITY` so agent cards
render in the visible grid. Do NOT migrate design-studio strings to i18n
(developer-auxiliary surface).
