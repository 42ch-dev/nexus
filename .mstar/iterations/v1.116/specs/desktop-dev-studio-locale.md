# Desktop Dev Script + Design-studio Locale (V1.116 P1)

> Iteration-scoped product/tech brief for V1.116 P1. Not a normative
> `{SPECS_DIR}` Master. Architect (seat 2) refines Tauri config strategy and
> locale import path.

| Attribute | Value |
| --- | --- |
| **plan_id** | `2026-07-13-v1.116-desktop-dev-design-studio-locale` |
| **Tier** | Must |
| **Audience** | Maintainers (daily desktop dev) + design reviewers (Studio preview honesty) |
| **primary plan** | `.mstar/plans/2026-07-13-v1.116-desktop-dev-design-studio-locale.md` |

## Problem framing

V1.116 is a **stabilize-before-extending** iteration. Two maintainer-facing
surfaces currently **misrepresent reality** and slow confident work:

### Track A — Desktop dev is not production-like

`pnpm dev:desktop` starts Vite via Tauri `beforeDevCommand` while Rust is still
compiling. Vite polls the daemon API before the sidecar is ready → console spam
("cannot connect to daemon"). Maintainers avoid the script or misread noise as
product bugs.

**Author expectation for the default desktop script:** behave like launching
the installed app — load pre-built web assets, start the daemon sidecar, render.
No live web server required.

HMR remains available via an explicit variant (`pnpm dev:desktop:web`).

### Track B — Design-studio preview lies about copy

`apps/design-studio` reuses `@web-setup/*` components that call `t()`, but the
Studio entry has no locale provider/catalogs. Live previews show raw keys
(e.g. `setup.agent.verify`) instead of real text ("Verify"). Design decisions
made against raw keys are unreliable.

Web remains **locale SSOT**. Studio loads **en only** (locked grill-me #7).

## User value

| Who | Why they care |
| --- | --- |
| **Maintainers** | `pnpm dev:desktop` is quiet and production-like; HMR is one intentional script away. |
| **Design reviewers / frontend authors** | Studio shows real en copy for shared setup/chrome components — previews match product text. |
| **Authors (indirect)** | Fewer false "daemon broken" alarms during desktop dev; fewer copy mistakes from Studio mis-previews. |

## Goals

1. Default `pnpm dev:desktop` loads pre-built `web/dist/` (production-like;
   no Vite server spam / no pre-sidecar daemon connection errors).
2. Preserve `pnpm dev:desktop:web` as the Vite + Tauri HMR workflow.
3. Studio live preview shows real localized en text for components that use
   `t()`, not raw i18n keys.
4. Keep locale SSOT in web; Studio imports compiled catalogs only; no multi-
   language Studio this iteration.

## Non-goals

- Auto-rebuild web on every file change in dist-load mode (use `:web` for HMR)
- Tauri auto-reload when dist changes (nice-to-have)
- Multi-language Studio (zh-CN etc.) — en only
- Studio editing of locale content
- Moving locale SSOT out of web
- Closing unrelated residual slate items

## Target state

| Surface | Before | After |
| --- | --- | --- |
| `pnpm dev:desktop` | Vite + spam during compile | dist-load, quiet, production-like |
| `pnpm dev:desktop:web` | (current default behavior) | Explicit HMR mode (same as today) |
| design-studio preview | raw keys | real en strings |

## Acceptance criteria (author/maintainer-observable)

| ID | Criterion | How to verify |
| --- | --- | --- |
| **AC-P1-1** | `pnpm dev:desktop` starts without Vite server spam or daemon connection errors during startup; loads pre-built web assets | Run script after `web/dist` is built (or script builds it); console is free of the current pre-sidecar connection spam pattern; UI loads from dist |
| **AC-P1-2** | `pnpm dev:desktop:web` still provides Vite + Tauri HMR | Run variant; edit a web file; HMR updates as today |
| **AC-P1-3** | design-studio live preview shows real en text for shared setup/chrome components that use `t()` | Open Studio surfaces that import `@web-setup/*` (or other t()-using web components); visible strings are human language, not `namespace.key` keys |

## Product decisions (locked)

| Decision | Choice | Rationale |
| --- | --- | --- |
| Default desktop script | dist-load production-like | Grill-me #5; matches "launch installed app" mental model |
| HMR mode | separate `:web` script | Keep power-user workflow without poisoning the default |
| Studio locale | import web en catalogs only | Grill-me #7; web SSOT; no multi-locale Studio scope |
| Plan bundling | one Must plan, two tracks | Same iteration story: honest maintainer surfaces; independent of P0 |

## Why Must (not Should)

This plan is **Must** for a stabilization iteration because:

1. Desktop noise actively trains maintainers to ignore real errors.
2. Studio raw keys actively mislead design/copy review of the same Setup
   surfaces P0 is fixing (Verify, agent list chrome).

Neither is an author-facing first-launch blocker like P0, but both are
**honesty bugs on maintainer surfaces** that block confident daily work. They
are not Stretch.

## Architect decisions (seat 2 — resolved)

### AD-1: Tauri v2 dev config split — `--config` override

**Current state** (`apps/desktop/src-tauri/tauri.conf.json`):
```json
"build": {
  "frontendDist": "../../web/dist",
  "devUrl": "http://localhost:5173",
  "beforeDevCommand": "pnpm --filter web dev",
  "beforeBuildCommand": "..."
}
```

In Tauri v2 dev mode, `devUrl` (when non-empty) takes precedence — the webview
loads from the Vite dev server. `beforeDevCommand` launches Vite before the
Rust compile. This is the source of the pre-sidecar connection spam.

**Dist-load strategy:** Tauri v2 supports `--config <file>` which deep-merges
a JSON patch over the base config. Create
`apps/desktop/src-tauri/tauri.dev.dist.conf.json`:

```json
{
  "build": {
    "devUrl": "",
    "beforeDevCommand": ""
  }
}
```

When `devUrl` is empty string, Tauri v2 dev mode loads from `frontendDist`
(`../../web/dist`) instead of a dev server URL. When `beforeDevCommand` is
empty, no pre-dev hook runs. The webview loads pre-built assets — production-
like, no Vite, no pre-sidecar daemon polling.

**Scripts (root `package.json`):**
```json
"dev:desktop": "pnpm --filter web build && tauri dev --config apps/desktop/src-tauri/tauri.dev.dist.conf.json",
"dev:desktop:web": "tauri dev"
```

- `dev:desktop`: builds web first (`pnpm --filter web build`), then launches
  Tauri with the dist-load override. No Vite server, no spam.
- `dev:desktop:web`: uses the **base** config (no `--config` override) —
  preserves `devUrl: http://localhost:5173` + `beforeDevCommand: pnpm --filter
  web dev`. HMR works as today.

**Fallback (Risk Register mitigation):** if Tauri v2 does not honor empty
`devUrl` as "load from frontendDist" in some version, the fallback is to serve
`web/dist/` via `vite preview --port 5173` and set `devUrl` to that URL in the
override config. This still eliminates pre-sidecar spam (vite preview starts
instantly, no compile wait) but reintroduces a local server. Document this
fallback in `apps/desktop/AGENTS.md` if the primary approach fails at
implementation time.

### AD-2: Studio locale — dedicated i18next instance, direct JSON import

**Current state:** `apps/design-studio/src/test/setup.ts` imports
`{ i18n } from '@/lib/i18n/config'` — but `apps/design-studio/src/lib/` does
NOT exist yet. This file must be created by this plan. The `@` alias resolves
to `design-studio/src`, so the import target is
`design-studio/src/lib/i18n/config.ts`.

**Web locale files are source JSON** (not compiled output). Web's own i18n
config (`apps/web/src/lib/i18n/config.ts`) imports JSON directly:
`import enSetup from '../../locales/en/setup.json'`. There is no build step
that produces "compiled" locale output — the JSON files ARE the source.

**Import approach:**

1. Add a Vite alias in `apps/design-studio/vite.config.ts`:
   ```typescript
   '@web-locales/en': path.resolve(__dirname, '../web/src/locales/en'),
   ```
   This alias must be registered BEFORE the general `@` → `./src` alias
   (same pattern as existing `@/lib/utils` → web override).

2. Create `apps/design-studio/src/lib/i18n/config.ts`:
   ```typescript
   import i18next from 'i18next';
   import { initReactI18next } from 'react-i18next';

   // Import only the namespaces Studio surfaces use (setup, common, shell,
   // settings, canvas). En only — no zh-CN in Studio (locked grill-me #7).
   import enCommon from '@web-locales/en/common.json';
   import enShell from '@web-locales/en/shell.json';
   import enSettings from '@web-locales/en/settings.json';
   import enSetup from '@web-locales/en/setup.json';
   import enCanvas from '@web-locales/en/canvas.json';

   i18next.use(initReactI18next).init({
     lng: 'en',
     fallbackLng: 'en',
     supportedLngs: ['en'],
     defaultNS: 'common',
     ns: ['common', 'shell', 'settings', 'setup', 'canvas'],
     resources: { en: { common: enCommon, shell: enShell, settings: enSettings, setup: enSetup, canvas: enCanvas } },
     interpolation: { escapeValue: false },
     react: { useSuspense: false },
   });

   export const i18n = i18next;
   ```

3. Wrap Studio entry (`main.tsx`) — the i18next init runs as a side effect of
   importing `config.ts`. No `I18nextProvider` wrapper needed (react-i18next
   uses the default instance). Import the config before rendering the app.

**No circular dependency risk:** JSON imports are data-only (no module graph).
Studio imports web's static JSON files, not web's i18n config module (which
would pull in web providers, hooks, etc.). The two SPAs keep separate i18next
instances.

**Namespaces to import:** only the namespaces Studio surfaces actually consume
via `@web-setup/*`, `@web-layout/*`, `@web-settings/*`, `@web-canvas/*`
components. At minimum: `setup`, `common`, `shell`, `settings`, `canvas`. The
implementer should audit which namespaces the imported components' `t()` calls
reference and include all of them. Missing namespaces will show raw keys for
those specific strings (graceful degradation, not a crash).

### AD-3: `apps/desktop/AGENTS.md` doc touch

Add a "Dev scripts" section documenting:
- `pnpm dev:desktop` — dist-load mode (production-like, no HMR). Use for
  desktop-only work where web changes are not expected. Run `pnpm build:web`
  or `pnpm dev:desktop:web` after web changes.
- `pnpm dev:desktop:web` — Vite + Tauri HMR mode. Use when actively editing
  web source. Expect daemon connection messages during Rust compile (these
  are expected in this mode, not errors).

## Mapping to plan tasks

| AC | Plan tasks |
| --- | --- |
| AC-P1-1 | T1 dist-load mode |
| AC-P1-2 | T2 HMR variant + docs |
| AC-P1-3 | T3 Studio I18nextProvider + en catalogs |
