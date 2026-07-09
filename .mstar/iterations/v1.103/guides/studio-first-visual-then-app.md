# Studio-First: Visual Then App (V1.103)

Reuse the V1.101/V1.102 discipline.

1. **Studio fixtures** — Settings shell chrome + each section’s visual states in Design Studio (props-driven; no daemon required where possible).
2. **Visual acceptance** — section nav, Agent, Connection, Setup (and Stretch Workspace) read intentional in light/dark.
3. **App wiring** — only after (1)+(2), connect routing/persistence/IPC in `apps/web` / desktop.

| Plan | Tier | Studio-first? |
|------|------|---------------|
| P0 Settings shell | Must | Yes |
| P1 Agent + preselect | Must | Yes (Agent section chrome; IPC in App) |
| P2 Connection | Must | Yes (host chrome; reuse Connect form) |
| P3 Setup re-run | Must | Yes (confirm dialog chrome) |
| P4 Workspace | Stretch | Yes if plan runs |

## Module layout (architect-locked)

```
apps/web/src/pages/settings/
  settings-shell-layout.tsx      # P0 — nav + Outlet
  settings-agent-section.tsx     # P0 refactor + P1 preselect
  settings-connection-section.tsx # P2
  settings-setup-section.tsx     # P3
  settings-workspace-section.tsx # P4 Stretch only

apps/web/src/components/settings/
  connect-daemon-form.tsx        # P2 extract from connect-daemon-page
```

P0 refactors V1.102 `settings-page.tsx` into shell + agent section; do not leave two competing Settings hosts.

## Hard preferences

- `wire_contracts_changed: false`
- lucide only
- AgentPicker stays app-shared
- Do not invent Settings IA beyond compass section allowlist
- **Workspace section nav** registers only when P4 Stretch runs — Must plans must not ship a dead Workspace tab
- Human desktop smoke is a **separate gate**; passing automated plan Done does not substitute for it

## DESIGN Voice (author-facing copy)

Section specs lock Title Case for titles, nav, and primary actions; sentence case for helpers, errors, and toasts. Implementers copy from:

- `specs/settings-shell-ia.md` — shell helper + nav labels
- `specs/settings-agent-section.md` — Agent helper + browser honesty
- `specs/settings-connection-section.md` — Connection helper + form field helpers (Settings context)
- `specs/settings-setup-section.md` — Re-run Setup confirm dialog
- `specs/settings-workspace-section.md` — post-change honesty copy (Stretch)
