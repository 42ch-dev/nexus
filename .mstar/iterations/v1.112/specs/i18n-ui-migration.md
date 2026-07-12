# Spec — Full user-facing UI migration (V1.112 P1)

**Status:** draft (Phase 1 — architect plan-lock 2026-07-12)  
**plan_id:** `2026-07-12-v1.112-i18n-ui-migration`  
**blocked_by:** `2026-07-12-v1.112-i18n-foundation`  
**Surface:** `apps/web`

## Intent

Complete migration of **user-facing** product copy in `apps/web` onto the P0
i18n foundation so authors can use the **entire** Control Room in `en` or
`zh-CN` — not only shell chrome. P1 is the iteration **Done** gate for
localized product UI.

## User scenarios

1. **Full product path in 简体中文:** Author sets Language to 简体中文, then
   completes setup (if shown), navigates canvas, opens reading/findings/memory,
   runs command-palette actions, and sees toasts/errors in Chinese.
2. **Instant switch mid-session:** Author switches to English while on canvas;
   visible labels, command palette, and formatted dates update without reload.
3. **Formatted values:** Dates, numbers, and relative times match the active UI
   locale (e.g. Chinese month/day presentation when UI is `zh-CN`).

## Normative requirements

### Scope definition

1. **In scope — user-facing product chrome** in `apps/web/src` production UI:
   setup wizard and first-launch flows; canvas surfaces, inspectors, and tool
   chrome; command palette action labels; reading views; findings lists and
   detail chrome; memory / soul / chapters UI; toasts; empty states; user-facing
   error messages and validation helpers shown to authors.
2. **Out of scope (must not inflate catalogs):**
   - Manuscript / author-authored content (not product chrome)
   - Developer-auxiliary surfaces (including `apps/design-studio`)
   - Test fixtures, Storybook-only copy, `console.*` / dev-only strings
   - Default slot text inside `@42ch/nexus-ui` primitives (callers pass `t()`)
   - CLI and daemon messages (English-only per compass)

### Migration rules

3. All in-scope strings resolve through i18n (`t()` / catalogs). Hardcoded
   English (or Chinese) literals for user-facing chrome are **not allowed** at
   P1 Done.
4. Use the **nine-namespace map** from P0 (`common`, `shell`, `settings`,
   `setup`, `canvas`, `reading`, `findings`, `memory`, `commands`) — add keys
   within the assigned namespace; do not create new namespaces or bake copy into
   `@42ch/nexus-ui`.
5. **Command registry (locked):** store `labelKey` / `groupKey` / `keywordKeys`
   on `Command`; resolve with `t()` in `command-palette.tsx` at render time so
   labels update on locale change without re-registering commands. Remove
   resolved `label` / `group` / `keywords` string fields from the interface.
6. **`format.ts` / Intl (locked):** all production format helpers use
   `getActiveLocale()` from `lib/i18n/active-locale.ts` (not `undefined` /
   stale `navigator.language`). `formatUtcAndLocal.utc` stays `en-US` + UTC;
   `local` uses active locale. Migrate `formatRelative` to
   `Intl.RelativeTimeFormat` where practical.

### Done audit

7. P1 Done requires explicit audit evidence (see **Audit procedure** below).

## Architecture locks (implementer SSOT — architect 2026-07-12)

### Surface → namespace routing

| Area | Primary namespace | Key modules |
|------|-------------------|-------------|
| Setup | `setup` | `pages/setup-wizard-page.tsx`, `components/setup/**` |
| Canvas chrome | `canvas` | `components/canvas/**` (not registry) |
| Command palette | `commands` | `canvas-nav-commands.tsx`, canvas `useRegisterCommand` sites |
| Reading | `reading` | `components/reading/**` |
| Findings | `findings` | `pages/findings-page.tsx`, `components/findings/**` |
| Memory / soul / chapters | `memory` | `pages/memory-page.tsx`, `components/memory/**`, `components/soul/**`, chapter pages |
| Global toasts/errors | `common` | `main.tsx` query toast, mutation error surfaces |
| Shell leftovers | `shell` | anything not migrated in P0 |

### Command palette instant switch

```text
Registrant                  Registry store              Palette render
──────────                  ──────────────              ──────────────
labelKey: 'go.strategy.label'  (stable keys)    →    t(labelKey, { ns: 'commands' })
groupKey: 'group.navigate'                              t(groupKey, …)
```

- `useRegisterCommand` mount semantics unchanged (keyed by `id`).
- `filterCommands` receives **resolved** labels for search ranking.
- QC: switch locale while palette is open; labels update without remount.

### format.ts wiring

| Export | Locale source |
|--------|---------------|
| `formatDateTime`, `formatDate`, `formatRelative`, `formatUtcAndLocal.local` | `getActiveLocale()` |
| `formatUtcAndLocal.utc` | `'en-US'` (explicit UTC presentation) |
| `humanizeStatus` | replace with `t('common.status.<value>')` at call sites or localized map |

### Audit procedure (FB-MIG-005)

**Automated** (attach to plan QC):

```bash
rg -n '>[A-Z][A-Za-z]{2,}[^<{]*</' apps/web/src \
  --glob '!**/*.test.*' --glob '!**/test/**'
rg -n '(label|title|placeholder|aria-label)=\{?["'"'"'][A-Za-z]' apps/web/src \
  --glob '!**/*.test.*'
rg -n 'label:\s*["'"'"'][A-Z]' apps/web/src/lib/canvas apps/web/src/components/canvas
```

**Exclude:** `*.test.*`, `src/test/**`, design-studio, manuscript content,
`console.*`, raw daemon codes.

**Manual minimum:** Appearance switch → setup (if shown) → canvas inspector →
palette navigate commands → reading/findings/memory page → formatted date on Works
list → one toast/error path.

**Pass:** triaged zero in-scope hardcoded product copy; false positives documented.

## Surface inventory (checklist for implementers / QC)

| Area | Examples of in-scope copy |
|------|---------------------------|
| Setup | Wizard steps, buttons, validation messages |
| Canvas | Panel titles, inspector labels, empty states |
| Command palette | Action names, group headings, shortcuts help |
| Reading | Toolbar, section headers, loading/empty states |
| Findings | List headers, filters, detail chrome |
| Memory / soul / chapters | Nav labels, section titles, actions |
| Global | Toasts, dialogs, user-facing API error surfacing |

## Acceptance criteria (P1)

- [ ] FB-MIG-000 through FB-MIG-005 satisfied per plan Must list
- [ ] User scenarios 1–3 pass on desktop (shared SPA) and browser
- [ ] Audit evidence attached for plan QC (grep output + spot-check list)
- [ ] No regression to P0 Appearance, persistence, or instant-switch behavior

## Non-goals

New languages; CLI/daemon localization; developer-auxiliary surfaces
(including `apps/design-studio`); theme into Appearance; changing P0 library,
persistence key, or resolve order.

## References

- P0 spec: [`i18n-foundation.md`](i18n-foundation.md)
- Compass: `.mstar/iterations/v1.112-delivery-compass.md`
- Plan: `.mstar/plans/2026-07-12-v1.112-i18n-ui-migration.md`
