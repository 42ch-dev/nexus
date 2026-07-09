# V1.103 Iteration Workspace

Iteration-scoped contracts and guides for **V1.103 — Settings Shell Deepening (DF-70 Remainder)**.

**Compass:** [`v1.103-delivery-compass.md`](../v1.103-delivery-compass.md)

## Story

V1.103 turns the V1.102 thin Settings host into a Settings shell and moves Connect + Re-run setup (and optionally Workspace) into it — closing the V1.94 deferred Settings bodies.

**Iteration complete when:** P0–P3 (shell + Agent + Connection + Setup) pass automated gates. P4 Workspace is Stretch only.

| Tier | Plans | Iteration incomplete if missing? |
|------|-------|----------------------------------|
| **Must** | P0 shell; P1 Agent+G1; P2 Connection+C1; P3 Setup R1 | **Yes** |
| **Stretch** | P4 Workspace W2 | **No** |

### V1.94 promises closed in V1.103

| Promise | Spec / plan |
|---------|-------------|
| Connect reachable only from Settings | `specs/settings-connection-section.md` / P2 |
| Re-run setup from Settings | `specs/settings-setup-section.md` / P3 |
| Agent change after setup in Settings | `specs/settings-agent-section.md` / P1 |
| Settings shell (not single page) | `specs/settings-shell-ia.md` / P0 |

## Specs

| Path | Tier | Purpose |
|------|------|---------|
| `specs/settings-shell-ia.md` | Must / P0 | S3 shell + section nav + routes |
| `specs/settings-agent-section.md` | Must / P1 | AgentPicker + getAgentProfile |
| `specs/settings-connection-section.md` | Must / P2 | Connection + `/connect` redirect |
| `specs/settings-setup-section.md` | Must / P3 | Re-run setup R1 |
| `specs/settings-workspace-section.md` | Stretch / P4 | Workspace path W2 |

## Guides

| Path | Purpose |
|------|---------|
| `guides/studio-first-visual-then-app.md` | Studio → visual accept → App wiring |

## Locks (summary)

- Nested `/settings/*` allowed for accepted sections only (supersedes V1.102 thin-host nested-route ban).
- Route tree: `SettingsShellLayout` + section modules under `pages/settings/`.
- `get_agent_profile` / `getAgentProfile` — Tauri only; no schemas.
- Connect: extract `ConnectDaemonForm`; `/connect` redirect in `App.tsx`.
- Re-run: marker only + `SetupCompletedContext` sync; **Re-run Setup** copy per `settings-setup-section.md`.
- Workspace W2: existing IPC trio + honest restart copy (Stretch).
- AgentPicker stays app-shared.
- Prefer `wire_contracts_changed: false`.
- Execution-mode / BYOK / multi-workspace remain Out.
