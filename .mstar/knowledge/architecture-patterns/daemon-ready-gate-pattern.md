---
module: apps/desktop/src-tauri + apps/web (setup-gate, daemon-status-bar, main-banner)
date: 2026-07-06
problem_type: architecture-pattern
category: architecture-patterns
severity: medium
plan_id: V1.94-P-last (compound of desktop onboarding & IA pass)
tags: [daemon-runtime, sidecar, health-probe, desktop-shell, setup-wizard, daemon-status-bar, gate, two-consumer-pattern]
applies_when: gating main-UI entry on daemon readiness; designing any "wait for service X before entering app" UX; or wiring two consumers to a single lifecycle event stream
---

# Daemon-Ready Gate Pattern (Per-Launch + Setup Wizard Step 2)

**Track**: Knowledge (durable guidance distilled from V1.94 Desktop App Onboarding & IA Pass).

## Context

V1.66 (Tauri Desktop Shell) shipped a `SidecarManager` that auto-starts the bundled `nexus42` daemon and exposes a single lifecycle event stream `onDaemonStatusChanged` carrying `DaemonStatus { state, version, port, detail }`. States: `starting → running → degraded → stopped → error`.

V1.94 introduced **two distinct consumers** of that single event stream:

1. **Setup Wizard Step 2** (first-launch only) — the wizard's daemon-ready step observes the event until `state: "running"`, then advances to step 3 (ACP Agent Detection). Failure paths (`error`, timeout) surface an actionable CTA inside the wizard.
2. **Per-launch daemon-ready splash** (every launch, including returning users) — a brief splash that gates main-UI entry until `state: "running"`. Returning users (`setup_completed === true`) see this splash instead of the wizard.

The naive implementation would create two independent health-probe polls or two Tauri commands ("is the daemon ready yet?"). Both are wrong: they compete with the existing `SidecarManager` state machine, can deadlock on the 15s `HEALTH_START_TIMEOUT` boundary, and drift from the source of truth.

## Guidance (the pattern)

**Single source of truth, multiple observers.** The `SidecarManager` owns daemon lifecycle state. Consumers observe the existing `onDaemonStatusChanged` event; they do NOT spawn their own probes, do NOT poll a health endpoint independently, and do NOT ask for a new Tauri command when the event already carries the signal.

| Rule | Reason |
|------|--------|
| Subscribe to `onDaemonStatusChanged`; do not add `is_daemon_ready()` commands | The event already carries `state`; a synchronous command would race with the state machine and lie during transitions |
| Treat `state: "running"` as the only "gate open" signal | `starting` is not ready (health probe pending); `degraded`/`error`/`stopped` are explicit failures |
| Surface failure paths explicitly (`error`, `stopped`, 15s timeout) | Silent hangs are the worst UX; the wizard step 2 and the per-launch splash both need actionable CTAs (Restart, Open Logs, etc.) |
| Two consumers can coexist without coordination | The event is fan-out; each consumer maintains its own UI state but reads the same lifecycle signal |
| Do not call `start_daemon()` from a consumer unless you intend to reset the crash budget | `start_daemon()` resets `restart_count`; `start()` does not. The wizard may reset (user-initiated); the per-launch splash must not (automatic) |

## Why This Matters

- **Avoids deadlock.** Independent probes racing the state machine can hang the UI past the 15s `HEALTH_START_TIMEOUT`.
- **Keeps the gate truthful.** The event stream reflects what `SidecarManager` actually believes; a command would snapshot a moment that may already be stale.
- **Supports two distinct UX flows from one signal.** First-launch wizard and returning-user splash are different visuals driven by the same state machine — no duplicated logic.

## When to Apply

- Adding any new "wait for daemon" surface (loading splash, error banner, reconnect attempt).
- Designing a similar gate for any other local service the app supervises (sidecar, embedded runtime, attached external process).
- Whenever the temptation arises to add an `is_X_ready()` command — first check whether an event stream already carries the signal.

## Examples

- **Setup Wizard Step 2** (`apps/web/src/pages/setup-step-daemon.tsx`): subscribes via `useDaemonStatus()`; renders "Starting daemon…" while `state === "starting"`; advances on `"running"`; surfaces error CTA on `"error"` or after 15s.
- **Per-launch splash** (`apps/web/src/components/setup/setup-gate.tsx`): returning-user path; same subscription; brief splash → main UI on `"running"`.
- **Crash banner** (`apps/web/src/components/layout/main-banner.tsx`): long-running-session consumer; appears when state degrades from `"running"` to `"degraded"`/`"error"`/`"stopped"`.

## Anti-patterns

- ❌ Adding `is_daemon_ready() -> bool` Tauri command (race-prone; the event already carries it).
- ❌ Polling `GET /v1/daemon/runtime/health` from the SPA (duplicates `SidecarManager`'s own probe).
- ❌ Two consumers calling `start_daemon()` independently (budget-reset semantics differ; can mask crash loops).
- ❌ Treating `starting` as "ready" (health probe may still fail; main UI would render against an unreachable daemon).
