# Spec — Shell daemon gate + agent instant-apply (V1.125 P0)

**Status:** product-reviewed, architect-reviewed (seat 2), writing-hygiene done (seat 3)  
**Document class:** Iteration package working spec (not `{SPECS_DIR}` Master)  
**Compass:** [`../delivery-compass.md`](../delivery-compass.md) — AC-V1125-1, AC-V1125-2  
**Plan:** [`2026-07-19-v1.125-shell-daemon-agent`](../../../plans/2026-07-19-v1.125-shell-daemon-agent.md)  
**Wire contracts:** `wire_contracts_changed: false` — native version populates existing optional `AgentScanEntry.version`; no `schemas/` change.

## Problem

Authors enter Control Room while daemon is not fully ready (MainBanner “守护进程启动中…” strip over partial shell), agent selection requires a Save tap and footer stays stale, Codex native has no version, create-creator surfaces opaque transport errors.

## Normative decisions

1. **Fullscreen gate only (no MainBanner)** — When desktop daemon status ≠ `running`, render **only** `DaemonLaunchGate` / `DaemonReadySplash`. **Remove `MainBanner`** from `root-layout` (do not mount a starting strip). Author must not interact with sidebar/header/canvas until gate clears. Post-unlock daemon degradation is **not** re-gated in P0 — `DaemonStatusBar` restart remains; do not reintroduce MainBanner for degraded mid-session.
2. **Gate lifecycle — events + poll** — While waiting (`daemonReady === false`):
   - **Primary:** Tauri `onDaemonStatusChanged` → `applyStatus`; unlock when `state === 'running'` only.
   - **Secondary:** `setInterval` health poll every **1–2s** calling `client.health()`; success does **not** unlock unless status is also `running` (health alone is insufficient when status lags).
   - **Opportunistic:** keep existing one-shot `probeForReady()` on non-ready status events for attach races.
   - Keep ~25s timeout. **Remove `degraded` from unlock paths** in `applyStatus`, `markReady`, and timeout handler (today `running || degraded` unlocks — incorrect for V1.125).
3. **Ready = `running` only** — Unlock shell on `running` only. `degraded` alone keeps splash (or equivalent blocked state).
4. **Agent instant-apply (Save removed)** — On installed card select or successful custom verify, call `setAgentProfile(name, launchCommand)` immediately. **Remove** the Settings Agent Save button and any “unsaved changes” affordance for agent pick.
5. **Footer sync** — After persist, invalidate `queryKeys.agentProfile.detail()` and `queryKeys.agentHost.scan({ filter: 'all' })` so `DaemonStatusBar` reflects the new selection on next render (same invalidation contract as V1.120 save path; no toast required on instant apply).
6. **Codex version** — In `map_native_catalog_entry` (`agent_host.rs`), set `version` by looking up `launch_command` in the existing `by_binary` map produced by `scan_local_installations_with_path` (reuses `probe_local_binary` / `<binary> --version` — same path as `build_scan_entry`). **Do not** add a second probe implementation.
7. **Match stability** — Preselect, persist, and footer badge resolution use `launchCommandMatches` + `resolveAgentKey` / catalog key — **not** exact display `name` (e.g. saved `codex` vs scan `name: "Codex"`). Update `applySavedProfile` to prefer command match over name equality.

## Architecture notes (implementer)

```
Tauri status event ──► applyStatus ──► running? ──► markReady → children
                              ▲
1–2s interval ──► client.health() ──┘ (attach race only; running still required)

MainBanner: unmounted from root-layout (Studio fixture may remain for degraded demo)
```

| Component | Change |
|-----------|--------|
| `daemon-launch-gate.tsx` | Drop `degraded` unlock; add interval poll; tighten tests |
| `root-layout.tsx` | Remove `<MainBanner />` |
| `settings-agent-section.tsx` | Instant persist on select; remove Save/dirty gate; fix `applySavedProfile` matching |
| `agent_host.rs` | `map_native_catalog_entry(entry, &by_binary)` version lookup |
| `daemon-status-bar.tsx` | Update header comment (no longer references MainBanner for non-running) |

## Acceptance (author-observable)

| ID | Author sees / does |
|----|-------------------|
| AC-V1125-1 | Non-`running` → fullscreen splash only; no banner strip; unlock on `running` |
| AC-V1125-2 | Tap agent card → immediate apply; no Save; footer updates; Codex version when probed |

## Out of scope

Selection preview without persist; Schedule/cron; Creation IA (P2); re-gating fullscreen on mid-session degradation.
