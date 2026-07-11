# Studio-First Invariant (V1.108)

**Status:** Carry-forward from V1.106/V1.107; reaffirmed at V1.108 iteration-start

## Invariant

All subsequent UI work for author-facing chrome follows:

> **需求 →【design-studio ↔ DESIGN.md】组件打磨 → Real App 采用**

**Plain language:** Polish UI in Design Studio against DESIGN.md first; wire the real App only after visual acceptance.

## Rules

1. **Studio composition first** — fixtures in `apps/design-studio` using DESIGN tokens / `@42ch/nexus-ui` / approved `@web-*` aliases.
2. **Visual acceptance** — light/dark, focus-visible, Voice & Content — before App wiring claims.
3. **DESIGN.md** — token/primitive contract changes update root DESIGN pair in the same change set.
4. **Promotion decision** — `promote` | `keep-web` | `keep-studio` | `defer` per promotion workflow knowledge.
5. **App integration last** — daemon data, routing, Tauri IPC stay in `apps/web` / `apps/desktop`.
6. **Shell SSOT** — Settings/Studio shell fixtures consume `ShellSidebarChrome` / `@web-layout/*`; no second underline/plain-nav track (V1.108 P1 FB-UI-001..005).
7. **Canvas Surfaces** — Studio `/surfaces/canvas` fixtures land with Canvas IA (V1.108 P1 FB-UI-004; presentational preview while P0 ships App graph).
8. **Voice & Content** — Title Case CTAs and toolbar actions; sentence case helpers; no fake UI promises. Locked strings: P0 **Show list view** / **Show graph**, **Link Foreshadow** / **Unlink Foreshadow**; P1 **Open Outline** / **Open Strategy**, **Verify Agent** (FB-UI-010: no command-palette promise).

## V1.108 application

| Concern | Studio-first requirement |
|---------|--------------------------|
| Shell / Settings | Fixtures import shell SSOT (FB-UI-001..005) |
| AgentPicker | Studio matrices accept StatusDot/hover/Verify before App claims (FB-UI-006..008) |
| Work IA | App copy/CTAs — **Open Outline**, **Open Strategy** (FB-UI-009); Studio N/A |
| World KB copy | Honest empty helper — no command palette (FB-UI-010) |
| Canvas chrome | `/surfaces/canvas` fixtures for shell + context menu (FB-UI-004) |
| Outline spatial | App P0 owns graph behavior (FB-C1-000..006); toolbar **Show list view** / **Show graph** matches Strategy/World KB; Studio fixtures cover shared chrome (P1) |
