---
iteration_id: V1.125
start_date: 2026-07-19
status: locked
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.125
plans:
  - 2026-07-19-v1.125-shell-daemon-agent
  - 2026-07-19-v1.125-orchestration-repair-memory
  - 2026-07-19-v1.125-creation-world-first-ia
---

# V1.125 Delivery Compass — Control Room dogfood + World-first IA

> **Direction lock mode: autonomous** (`/iteration-loop`, scale **M** — 3 business plans).
> Caller constraint: dogfood feedback F1–F6 (create-creator transport; agent Save/footer/version; daemon starting banner; Memory→Orchestration; orchestration “无法加载此视图”; Creation World-first IA + Settings+theme).
>
> **Phase 1 Review & Edit chain:** product-manager seat 1 → architect seat 2 → writing-specialist seat 3 → PM lock. Direction is **locked** — do not re-question the Control Room dogfood + Worlds-first Creator sidebar first slice.

## Autonomous direction lock record

**Scale budget:** M = 3 business plans (harness process not counted).

**Branch policy (autonomous resolve):**
- `iteration_base_branch: main` — from `status.json` metadata
- `target_branch: main` — from `status.json` metadata
- `spec_integration_branch: iteration/v1.125` — cut from `main`

### Candidates evaluated

| # | Candidate | Verdict |
|---|-----------|---------|
| A | Shell reliability only | Rejected — leaves Orchestration broken + IA wrong |
| B | Creation IA full rewrite alone | Rejected — skips dogfood blockers; too large for M |
| C | **Shell + Orchestration repair + World-first IA first slice** | **LOCKED** |
| D | Residual cleanup / Studio follow-up | Rejected — wrong direction |

### Locked direction

Make Control Room trustworthy again (daemon wait, agent apply/footer, orchestration views), then pivot Creation nav to World-first experience-driven IA — deferring selection-submenu shell, canvas Brief/Narrative axis, and Schedule→cron role UX to V1.126+.

## Scope

- **S1 (P0 — Must):** Fullscreen daemon gate only when not `running` (no MainBanner strip); agent selection instant-applies with live footer sync + Codex version; create-creator recovery via gate
- **S2 (P1 — Must):** Fix Strategies/Modules/Sessions load UX; Orchestrator sidebar groups **Memory → Strategies → Runtime → Compute**; route-derived Orchestrator tab; remove Memories from Creator
- **S3 (P2 — Must):** Worlds-first Creation sidebar; demote Timeline peer groups; card-sized empty CTAs (honest Work-create fallback when Create World API missing); Settings icon beside theme

## Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| `2026-07-19-v1.125-shell-daemon-agent` | P0 — Shell daemon gate + agent instant-apply | Todo | Must |
| `2026-07-19-v1.125-orchestration-repair-memory` | P1 — Orchestration repair + Memory move | Todo | Must |
| `2026-07-19-v1.125-creation-world-first-ia` | P2 — Creation World-first IA first slice | Todo | Must |

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Phase 1 compass locked | 2026-07-19 | done (PM seat 1 + architect seat 2 + writing-specialist seat 3; PM lock pending) |
| P0 shell Done | 2026-07-19 | pending |
| P1 orchestration Done | 2026-07-19 | pending |
| P2 Creation IA Done | 2026-07-19 | pending |
| Iteration close + PR | 2026-07-19 | pending |

## Acceptance Criteria

Author-facing, observable in desktop Control Room unless noted.

- **AC-V1125-1** (P0 — daemon gate): While daemon status ≠ `running`, the author sees **only** the fullscreen `DaemonLaunchGate` / `DaemonReadySplash` — **no** `MainBanner` “守护进程启动中…” strip or other partial shell chrome. Gate polls `client.health()` every 1–2s until `running` or timeout (~25s). Shell unlocks **only** on `running` (not `degraded` alone).
- **AC-V1125-2** (P0 — agent settings): In Settings → Agent, tapping an installed agent card **immediately** persists the profile — **no Save button** is shown. Within one UI refresh, `DaemonStatusBar` footer shows the selected agent name/key. Codex native entry displays a version string when `--version` probe succeeds.
- **AC-V1125-3** (P1 — orchestration load): On Strategies list/detail, Modules, and Sessions, the author never sees generic “无法加载此视图” when the correct UX is empty idle or engine-unavailable. HTTP 503 `engine not available` → honest unavailable/empty copy + Retry where appropriate (not `common.error.title` crash framing).
- **AC-V1125-4** (P1 — orchestration IA): Orchestrator tab sidebar groups appear in order: **Memory** (first) → **Strategies** → **Runtime** (Sessions, Schedule) → **Compute** (Modules). Creator tab has **no** Memories group. Direct navigation to `/memory`, `/sessions`, `/schedule`, `/strategies`, or `/modules` auto-selects the Orchestrator tab.
- **AC-V1125-5** (P2 — Creation IA): Creator tab lists **Worlds** then **Works** — no top-level Timeline or Work Timelines groups (`/timeline` deep links still work). Empty Worlds and Works pages show **card-sized** primary CTAs (same footprint as content cards). **Worlds empty:** if Create World API exists → “Create World”; if not → CTA routes to Work create with copy that Worlds are created from Works (no query dead-end). Settings gear icon sits beside the theme toggle in the header (sidebar Settings text row removed/demoted).

## Non-Goals

- Selection → submenu shell (World/Work selected mode + agent dialog)
- Canvas Brief/Narrative directed center axis
- Schedule → cron role creation UX
- Harness rename, composite timeline API, Studio follow-ups, status.json compaction

## Roadmap Position

- **Current (V1.125):** Control Room dogfood + World-first IA first slice
- **Next (V1.126+):** selection→submenu shell; canvas axis; Schedule/cron; Fork UI / Computable per prior roadmap

## Delivery Branch Policy

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.125` |
| `target_branch` | `main` |

## Architecture locks (architect seat 2)

| Area | Lock |
|------|------|
| **Daemon gate lifecycle** | Dual-source wait: Tauri `onDaemonStatusChanged` (fast path) **plus** periodic `client.health()` poll every **1–2s** while `daemonReady === false`. **Unlock only on `status.state === 'running'`** — remove `degraded` from `markReady`/`applyStatus` (today both unlock). Remove `MainBanner` mount from `root-layout` entirely (no starting strip over partial chrome). Post-unlock runtime degradation is **not** re-gated in P0 — `DaemonStatusBar` restart path remains; do not reintroduce MainBanner for degraded. |
| **Agent profile persist** | Card select / verify-success → immediate `desktop.setAgentProfile(name, launchCommand)`; remove Save + dirty gate. After persist: invalidate `queryKeys.agentProfile.detail()` + `queryKeys.agentHost.scan({ filter: 'all' })` (same keys as V1.120 save path). Match saved profile to scan rows via `launchCommandMatches` first, then `resolveAgentKey` / catalog key — **not** display `name` alone (`codex` vs “Codex”). |
| **Native version probe** | In `map_native_catalog_entry`, populate `version` by reusing the existing `by_binary` map from `scan_local_installations_with_path` (same `probe_local_binary` / `<binary> --version` path ACP entries use). **Do not** add a parallel probe helper. |
| **Sessions 503** | **Client-side classification preferred** — shared `isOrchestrationEngineUnavailable(NexusClientError)` at page/query boundary; render honest unavailable/empty copy + Retry. **No daemon handler change** in P1 unless classification proves insufficient. |
| **Orchestrator tab sync** | Derive active tab from `pathname` prefix set (`/memory`, `/sessions`, `/schedule`, `/strategies`, `/modules` → Orchestrator); manual tab clicks still work for Creator routes. |
| **Worlds empty CTA** | Feature-detect Create World via `'createWorld' in client` on `NexusClient` (absent today). Fallback: open existing Work-create flow with honest copy — no dead-end query param. |
| **`wire_contracts_changed`** | **`false`** for all three plans — no `schemas/` edits; native version uses existing optional `AgentScanEntry.version`; 503 UX is client classification only. |

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Create World API missing | Med | Med | P2 Worlds empty CTA routes to Work-create with honest copy (“Worlds are created from your Works”); feature-detect via `NexusClient` method presence |
| Orchestration engine 503 | Med | Med | Client-side `service_unavailable` / 503 classification → honest unavailable UX; daemon 503 envelope unchanged |
| Agent name mismatch (codex vs native CLI) | Med | Low | Persist `launch_command`; match via `launchCommandMatches` + `resolveAgentKey`; invalidate agent query keys after apply |
| Gate `degraded` vs `running` drift | Low | Med | Architect lock: only `running` unlocks; update `daemon-launch-gate.test.tsx` matrix |
| MainBanner removal leaves no mid-session banner | Low | Low | Acceptable — `DaemonStatusBar` covers post-unlock recovery; MainBanner was startup dogfood blocker only |

## Iteration package

| Path | Kind | Status |
|------|------|--------|
| [`README.md`](README.md) | index | active |
| [`specs/shell-daemon-agent.md`](specs/shell-daemon-agent.md) | spec (P0) | product-reviewed, architect-reviewed, writing-hygiene done |
| [`specs/orchestration-repair-memory.md`](specs/orchestration-repair-memory.md) | spec (P1) | product-reviewed, architect-reviewed, writing-hygiene done |
| [`specs/creation-world-first-ia.md`](specs/creation-world-first-ia.md) | spec (P2) | product-reviewed, architect-reviewed, writing-hygiene done |

Plans: [`.mstar/plans/2026-07-19-v1.125-shell-daemon-agent.md`](../../plans/2026-07-19-v1.125-shell-daemon-agent.md) · [`.mstar/plans/2026-07-19-v1.125-orchestration-repair-memory.md`](../../plans/2026-07-19-v1.125-orchestration-repair-memory.md) · [`.mstar/plans/2026-07-19-v1.125-creation-world-first-ia.md`](../../plans/2026-07-19-v1.125-creation-world-first-ia.md)

## Quality Gate Summary

> Filled at iteration-close.

## Compound Round Summary

> Filled at iteration-close.
