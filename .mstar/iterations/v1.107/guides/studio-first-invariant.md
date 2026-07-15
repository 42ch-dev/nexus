# Studio-First Invariant (V1.107)

**Status:** Carry-forward from V1.106; reaffirmed at V1.107 iteration-start; §5.1 PM product lock; writing-complete (§5.3)

## Invariant

All subsequent UI work for author-facing chrome follows:

> **需求 →【design-studio ↔ DESIGN.md】组件打磨 → Real App 采用**

**Plain language:** Polish UI in Design Studio against DESIGN.md first; wire the real App only after visual acceptance.

## Rules

1. **Studio composition first** — fixtures in `apps/design-studio` using DESIGN tokens / `@42ch/nexus-ui` / transitional `@web-ui/*` as classified.
2. **Visual acceptance** — light/dark, focus-visible, Voice & Content — before App wiring claims.
3. **DESIGN.md** — when tokens or primitive contracts change, update root DESIGN pair in the same change set.
4. **Promotion decision** — `promote` | `keep-web` | `keep-studio` | `defer` per promotion workflow knowledge.
5. **App integration last** — daemon data, routing, Tauri IPC stay in `apps/web` / `apps/desktop`.
6. **Studio Tailwind content** — must scan sources that own class strings (`setup/**`, `packages/nexus-ui/src/**`) so fixtures paint (V1.107 FB-000). **Without this, studio-first is unenforceable.**

## V1.107 application

| Concern | Studio-first requirement |
|---------|--------------------------|
| Primitives (Badge/Button/Select) | Studio `/components` matrix accept → App inherits package |
| Wizard / AgentPicker | Studio wizard chrome fixtures accept → App shared `agent-picker.tsx` |
| MainBanner / TopStepIndicator | Studio Surfaces accept → App layout modules |
| Toast | Studio matrix already exists; App must adopt package (FB-012) |
| Shell / Settings | Studio Surfaces import App presentational SSOT via `@web-layout/*`, `@web-settings/*`, `@web-setup/*` — not duplicate stubs (FB-013..015) |

## Pointers

- Prior guide: [`.mstar/iterations/v1.106/guides/studio-first-invariant.md`](../../v1.106/guides/studio-first-invariant.md)
- Primary spec: [`../specs/studio-ui-tune.md`](../specs/studio-ui-tune.md)
- Compass: [`../../v1.107/delivery-compass.md`](../../v1.107/delivery-compass.md)
- Deferred Surfaces: [`../specs/author-surfaces-backlog.md`](../specs/author-surfaces-backlog.md)
