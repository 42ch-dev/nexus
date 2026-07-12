# Spec — i18n foundation + Appearance (V1.112 P0)

**Status:** draft (Phase 1 — architect plan-lock 2026-07-12)  
**plan_id:** `2026-07-12-v1.112-i18n-foundation`  
**Surface:** `apps/web` (desktop via shared SPA)

## Intent

Introduce standard i18n for the Control Room so authors can read and operate
product chrome in **English** or **简体中文**, with system-locale default,
English fallback, and a discoverable Settings control. P0 lands runtime +
persistence + **Appearance** Language UI and proves the pattern on
shell/nav/settings before P1 migrates remaining surfaces.

## User scenarios

1. **First visit:** UI language follows system locale when supported (`zh*` →
   `zh-CN`; otherwise `en`); unsupported system locales fall back to `en`.
2. **Explicit choice:** Author opens Settings → Appearance, selects
   **简体中文**, and sees shell/nav/settings update immediately without reload.
3. **Persistence:** After reload or desktop restart, the same language applies.
4. **Return to system:** Author selects **System**; UI follows OS/browser again.

## Normative requirements

### Runtime & persistence

1. **Library:** `i18next` + `react-i18next`.
2. **Supported locales:** `en`, `zh-CN`. Preference values: `system` | `en` | `zh-CN`.
3. **Resolve order:** stored preference → system (`navigator.language`; `zh*` →
   `zh-CN`, otherwise `en` when not an exact supported match) → fallback `en`.
4. **Persistence:** `localStorage` key `nexus-web-locale`. No daemon /
   `config.toml` / wire changes. CLI remains English-only.
5. **UX:** Changing Language applies immediately (no full page reload); update
   `document.documentElement.lang` to the active locale code (`en` or `zh-CN`).

### Settings — Appearance (product framing)

6. **New section:** Settings gains an **Appearance** section (alongside Agent /
   Workspace / Advanced). Theme control **stays in the header** this iteration.
7. **Nav + title:** Section nav label and page title use localized product copy:
   English **Appearance**; Chinese **外观**.
8. **Language control:**
   - Field label: **Language** / **语言**
   - Options (display labels localized; stored values as below):

   | Stored value | English label | Chinese label |
   |--------------|---------------|---------------|
   | `system` | System | 跟随系统 |
   | `en` | English | English |
   | `zh-CN` | 简体中文 | 简体中文 |

   - Optional helper: explain that **System** follows the OS/browser language
     when supported, otherwise English.
9. **Accessibility:** Language control is keyboard-operable with visible labels
   (no icon-only control); selected option is programmatically associated with
   the field.

### Conventions (product rules)

10. **Caller-owned copy:** Presentational / shared UI primitives (including
    `@42ch/nexus-ui`) receive user-visible strings from the **caller** — do not
    bake product copy into shared packages.
11. **User-facing-only catalogs:** Only author-visible product chrome enters
    locale catalogs under `apps/web`. Exclude developer-auxiliary surfaces
    (including `apps/design-studio`), test harness copy, and manuscript body
    text.
12. **Future UI:** New user-facing strings in `apps/web` must use i18n from day
    one (documented in `apps/web/AGENTS.md`).

### Sample migration (pattern for P1)

13. Shell, primary navigation, and settings chrome (including the new Appearance
    section) use `t()` / catalogs as the reference implementation for P1.

## Architecture locks (implementer SSOT — architect 2026-07-12)

### LocaleProvider & init

| Artifact | Path | Pattern source |
|----------|------|----------------|
| i18n singleton | `apps/web/src/lib/i18n/config.ts` | new |
| Active locale helper | `apps/web/src/lib/i18n/active-locale.ts` | `getActiveLocale()` for P1 `format.ts` |
| Provider | `apps/web/src/components/locale-provider.tsx` | mirrors `theme-provider.tsx` |
| Mount | `apps/web/src/main.tsx` | `ThemeProvider` → **`LocaleProvider`** → `ToastProvider` → … |
| Test wrapper | `apps/web/src/test/test-providers.tsx` | add `LocaleProvider` |

**Resolve:** `readStoredLocale()` from `localStorage` key `nexus-web-locale` →
`resolveSystemLocale(navigator.language)` (`zh*` → `zh-CN`, else `en`) → `en`.
**Side effects on change:** `i18n.changeLanguage(resolvedLocale)`;
`document.documentElement.lang = resolvedLocale`.

### Catalog layout & namespaces

```text
apps/web/src/locales/{en,zh-CN}/
  common.json | shell.json | settings.json | setup.json | canvas.json
  reading.json | findings.json | memory.json | commands.json
```

P0 **must populate** `common`, `shell`, `settings`; register all nine namespaces
in `config.ts` (stub empty `{}` for the rest).

| Namespace | P0 responsibility |
|-----------|-------------------|
| `common` | shared actions, generic loading/error fragments |
| `shell` | `root-layout.tsx` route titles + mobile nav; sidebar/header nav labels |
| `settings` | settings shell helper, section nav labels, Appearance page + Language control |

**Key convention:** `section.element` within namespace — e.g.
`settings.appearance.language.label`, `shell.nav.works`. English source strings
follow DESIGN.md §Voice & Content (Title Case nav/titles).

### Settings Appearance integration

| Item | Detail |
|------|--------|
| Route | `/settings/appearance` → `SettingsAppearanceSection` in `App.tsx` |
| Nav | **Appearance** entry in `settings-shell-layout.tsx` `SETTINGS_SECTIONS` — four-tab order: Agent, Workspace, **Appearance**, Advanced |
| Control | Three stored values: `system`, `en`, `zh-CN`; labels per compass table |
| Theme | stays in `header.tsx` — **not** in Appearance this iteration |

### Command registry & format.ts (P0 boundary)

- P0 does **not** change `Command` interface or `format.ts` behavior.
- P1 spec locks instant-switch (`labelKey` + palette render-time `t()`) and
  `format.ts` active-locale wiring.

## Acceptance criteria (P0)

- [ ] FB-I18N-000 through FB-I18N-005 satisfied per plan Must list
- [ ] Author can complete scenarios 1–4 above on a clean profile
- [ ] `apps/web/AGENTS.md` documents i18n conventions for implementers
- [ ] No full user-facing surface migration beyond shell/nav/settings (P1 scope)

## Non-goals

Full surface migration (P1); developer-auxiliary surfaces (including
`apps/design-studio`); CLI/daemon localization;
moving theme into Appearance; languages beyond `en`/`zh-CN`; wire/schema changes.

## References

- Grill-me decisions: `.mstar/iterations/v1.112-delivery-compass.md`
- Theme persistence pattern: `apps/web/src/components/theme-provider.tsx`
- Plan: `.mstar/plans/2026-07-12-v1.112-i18n-foundation.md`
