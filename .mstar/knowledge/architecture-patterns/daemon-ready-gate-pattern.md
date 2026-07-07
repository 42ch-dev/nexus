---
module: apps/desktop/src-tauri + apps/web (setup-gate, daemon-status-bar, main-banner)
date: 2026-07-06
last_updated: 2026-07-07
problem_type: architecture-pattern
category: architecture-patterns
severity: medium
plan_id: V1.94-P-last (compound of desktop onboarding & IA pass); V1.96 refinements from 2026-07-07-v1.96-implement-rework
tags: [daemon-runtime, sidecar, health-probe, desktop-shell, setup-wizard, daemon-status-bar, gate, two-consumer-pattern, late-subscription-race, stderr-capture, bounded-timeout]
applies_when: gating main-UI entry on daemon readiness; designing any "wait for service X before entering app" UX; wiring observers to a process lifecycle event stream that may fire before subscription; surfacing supervised-process crash reasons to the user
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
| Surface failure paths explicitly (`error`, `stopped`, timeout) | Silent hangs are the worst UX; the wizard step 2 and the per-launch splash both need actionable CTAs (Restart, Open Logs, etc.). V1.96 changed the wizard timeout from 15s to 25s with a mount-time probe — see `desktop-shell.md` §13.7.5. |
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

  > **V1.96 update**: the wizard step 2 now uses a mount-time state probe (`getDaemonStatus()` before subscribing), an explicit `'starting'` branch, and a 25s hard timeout (not 15s). The 15s timeout is historical (V1.94 original). See `desktop-shell.md` §13.7.5 for current behavior.
- **Per-launch splash** (`apps/web/src/components/setup/setup-gate.tsx`): returning-user path; same subscription; brief splash → main UI on `"running"`.
- **Crash banner** (`apps/web/src/components/layout/main-banner.tsx`): long-running-session consumer; appears when state degrades from `"running"` to `"degraded"`/`"error"`/`"stopped"`.

## Anti-patterns

- ❌ Adding `is_daemon_ready() -> bool` Tauri command (race-prone; the event already carries it).
- ❌ Polling `GET /v1/daemon/runtime/health` from the SPA (duplicates `SidecarManager`'s own probe).
- ❌ Two consumers calling `start_daemon()` independently (budget-reset semantics differ; can mask crash loops).
- ❌ Treating `starting` as "ready" (health probe may still fail; main UI would render against an unreachable daemon).
- ❌ **Subscribing without a mount-time state probe** (V1.96 regression root cause #1 — the daemon may have already transitioned before the SPA subscribed; the event is lost; the UI hangs forever). See "V1.96 refinements" below.
- ❌ **Leaving a state-enum branch implicit** (V1.96 regression root cause #2 — the `'starting'` branch was missing → callback was silent → no state update → UI stuck). Every state must have an explicit branch.
- ❌ **Discarding the supervised process's stderr** (V1.96 residual `R-V195-ARCH-STRERR-GAP` — the daemon's real crash reason was never captured; the wizard showed a generic "Daemon did not start" message).

## V1.96 refinements: late-subscription race + diagnostic surfacing

V1.96 (plan `2026-07-07-v1.96-implement-rework`) hit a P0 blocker: the setup wizard Step 2 hung indefinitely in "Starting daemon…" on a clean `~/.nexus42/` first launch. RCA revealed **three** consumer-side root causes (the daemon does NOT hang — it crashes within milliseconds when `WorkspaceState::initialize()` finds no `active_creator_id`; the wizard just never learns about it). The fixes distill into four durable rules that apply to **any** observer of a process lifecycle event stream, not just the daemon-ready gate.

### Rule 5: probe current state on mount, BEFORE subscribing

The daemon auto-starts at Tauri boot (`lib.rs` setup hook), which runs **before** the SPA's React tree mounts. By the time the SPA subscribes to `onDaemonStatusChanged`, the daemon may have already transitioned to `running` (normal) or `error` (crashed on boot). The first event the SPA would otherwise receive is `state: 'starting'` — but if the daemon already exited, even that event was emitted before subscription and is lost.

**Pattern**: call `getDaemonStatus()` (or the equivalent one-shot current-state command) on mount, apply the result, THEN subscribe for future transitions. If the mount-probe already returns a terminal state, the UI is immediately correct; the subscription is still attached for later restart/recovery events.

```tsx
useEffect(() => {
  let cancelled = false;
  let unsub;
  // 1. Probe current state first
  desktop.getDaemonStatus().then(s => { if (!cancelled) applyStatus(s); });
  // 2. Then subscribe for future transitions
  desktop.onDaemonStatusChanged(s => { if (!cancelled) applyStatus(s); }).then(u => unsub = u);
  return () => { cancelled = true; unsub?.(); };
}, []);
```

### Rule 6: every state-enum branch must be explicit (even no-ops)

The V1.96 bug had a callback shaped like `if (running) {...} else if (error || stopped) {...}` with **no** `'starting'` and **no** `'degraded'` branch. The first event after subscription was `'starting'` → no branch matched → no state update → `setReady(false)` stayed → UI stuck forever.

**Pattern**: factor status handling into a single `applyStatus(status)` function that covers **every** state in the enum. No-op branches (`'starting'` → keep the spinner) are explicit, not implicit fall-through silence. This also lets the mount-probe and the subscription share one code path (no drift).

### Rule 7: bound the wait with a hard timeout that RE-PROBES

A consumer that subscribes and waits can hang forever if no terminal event arrives (event lost, process stuck in initialization, subscription silently dropped). A hard timeout (V1.96 uses 25s; the SidecarManager's own health timeout is 15s — the consumer timeout should be LONGER to give the state machine room) prevents indefinite hangs.

**Pattern**: `setTimeout(() => { re-probe getDaemonStatus(); if still non-terminal → surface "taking longer than expected" }, 25_000)`. The re-probe before declaring timeout avoids misreporting when the process silently transitioned between mount-probe and timeout fire. Clear the timeout in the effect cleanup.

### Rule 8: capture the supervised process's stderr (bounded tail)

When the daemon fails to start, `DaemonStatus.detail` originally carried only generic SidecarManager messages ("Daemon did not start. Check the logs or try restarting.") — the **real** crash reason (SQLite migration mismatch, missing config, port bind failure, binary missing) was written to the daemon's stderr but never captured. The `_rx` event receiver from `command.spawn()` was discarded.

**Pattern**: spawn a bounded async task that drains the `_rx` receiver, accumulates a tail capped at ~2 KiB (nearest newline boundary — keep the tail, drop the head), and appends it verbatim to `DaemonStatus.detail` on the Error transition. Keep the generic message as fallback when stderr is empty. Run the drain concurrently with `wait_for_first_health` — do NOT block the spawn path. The consumer renders `detail` with `whitespace-pre-wrap` so the multi-line stderr survives the browser's default `white-space: normal` collapse.

### Why these four rules compound

Rules 5+6+7 together make the consumer resilient to **any** timing of the event stream: late subscription (5), partial coverage (6), or total event loss (7). Rule 8 makes the failure **actionable**: the user reads the real crash reason and can act (reset the DB, free the port, install the binary) instead of staring at "Daemon did not start." Without rule 8, even a perfectly-timed subscription surfaces an unhelpful generic message.

### Source

Distilled from V1.96 plan `2026-07-07-v1.96-implement-rework` (T3 sidecar stderr capture + T4 mount-probe/starting-branch/timeout/detail-render). Iteration-scoped RCA with code sketches: `.mstar/iterations/v1.96/guides/daemon-startup-rca.md` (snapshot; promoted here).
