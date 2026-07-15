# V1.112 workspace

Iteration-scoped specs and guides for **Frontend i18n** (`apps/web`).

**Product outcome:** Authors can use the local Control Room in **English** or
**简体中文** with system-locale default, Settings → **Appearance** → Language,
and instant switch. Desktop inherits via the shared SPA. Product copy conventions:
**caller-owned** strings in shared UI primitives; **user-facing-only** locale
catalogs in `apps/web`.

| Path | Purpose |
|------|---------|
| [`specs/i18n-foundation.md`](specs/i18n-foundation.md) | P0 — runtime, Appearance product copy, conventions, sample migration |
| [`specs/i18n-ui-migration.md`](specs/i18n-ui-migration.md) | P1 — full user-facing UI migration + Done audit |

Compass: [`../v1.112/delivery-compass.md`](../v1.112/delivery-compass.md).

Plans: [`../../plans/2026-07-12-v1.112-i18n-foundation.md`](../../plans/2026-07-12-v1.112-i18n-foundation.md),
[`../../plans/2026-07-12-v1.112-i18n-ui-migration.md`](../../plans/2026-07-12-v1.112-i18n-ui-migration.md).
