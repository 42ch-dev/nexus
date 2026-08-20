---
module: apps/desktop/src-tauri + apps/web (setup-gate, daemon-status-bar, main-banner)
date: 2026-07-06
last_updated: 2026-07-15
problem_type: architecture-pattern
category: architecture-patterns
severity: medium
tags: [daemon-runtime, sidecar, health-probe, desktop-shell, setup-wizard, daemon-status-bar, gate, two-consumer-pattern, late-subscription-race, stderr-capture, bounded-timeout, tauri-v2-sidecar-resolution, stopped-initial-state, attach-without-ownership, path-enrichment, agent-scan, daemon-launch-gate, d2-always-start]
applies_when: gating main-UI entry on daemon readiness; designing any "wait for service X before entering app" UX; wiring observers to a process lifecycle event stream that may fire before subscription; surfacing supervised-process crash reasons to the user
---

# Daemon-Ready Gate Pattern (App-Level Launch Gate)

**Track**: Knowledge (durable guidance distilled from V1.94 Desktop App Onboarding & IA Pass).

## Context

V1.66 (Tauri Desktop Shell) shipped a `SidecarManager` that auto-starts the bundled `nexus42` daemon and exposes a single lifecycle event stream `onDaemonStatusChanged` carrying `DaemonStatus { state, version, port, detail }`. States: `starting → running → degraded → stopped → error`.

V1.94 introduced **two distinct consumers** of that single event stream (wizard step + per-launch splash). **V1.105 collapses wait ownership into one outer app-level gate:**

1. **`DaemonLaunchGate`** (every launch) — fullscreen splash until Ready; wraps **all** routes including `/setup`.
2. **`SetupGate`** (marker only) — after Ready, routes incomplete setup to `/setup` vs main UI. **No splash.**
3. **Setup wizard** — Agent → Workspace → Done only; **no** Daemon wizard step (`setup-step-daemon.tsx` deleted).

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

- **Outer launch gate (V1.105 current)** (`apps/web/src/components/setup/daemon-launch-gate.tsx` + `daemon-ready-splash.tsx`): mount-time `getDaemonStatus` + `onDaemonStatusChanged`; 25s timeout; retry = reload; reset = `resetLocalDatabase` + reload; **no** happy-path `startDaemon` (D2 always-starts in Tauri `.setup()`).
- **Marker routing (V1.105)** (`apps/web/src/components/setup/setup-gate.tsx`): after outer gate Ready — `!setup_completed` → `/setup`; else main shell. No splash/subscribe.
- **Historical — Setup Wizard Step 2** (`setup-step-daemon.tsx`, deleted in V1.105 P1): previously owned wait+`startDaemon` inside the wizard. Do not reintroduce.
- **Crash banner** (`apps/web/src/components/layout/main-banner.tsx`): long-running-session consumer; appears when state degrades from `"running"` to `"degraded"`/`"error"`/`"stopped"`.

## Anti-patterns

- ❌ Adding `is_daemon_ready() -> bool` Tauri command (race-prone; the event already carries it).
- ❌ Polling `GET /v1/daemon/runtime/health` from the SPA (duplicates `SidecarManager`'s own probe).
- ❌ Two consumers calling `start_daemon()` independently (budget-reset semantics differ; can mask crash loops).

## V1.110 refinement — three-valued port-probe gate (FB-D1)

V1.110 optimized the cold-start path. Previously `start_with_budget` ran the full HTTP `probe_health` (2s timeout) **before** spawning the sidecar — a tax paid on every cold start where no daemon was running. V1.110 inserts a **three-valued TCP gate** first:

| `probe_port_state(port)` | Meaning | Action |
|--------------------------|---------|--------|
| `Free` | TCP connect refused fast (port unused) | **Skip** HTTP probe; spawn immediately |
| `Occupied` | TCP connect succeeded (something listens) | HTTP `probe_health` → attach (`owned=false`) or port-conflict error |
| `Unknown` | Timeout / inconclusive (≤150ms gate) | HTTP `probe_health` → attach or spawn (safe fallback) |

**Critical invariant preserved:** `Occupied`/`Unknown` always run the HTTP probe, so the attach-without-ownership path for an external daemon (user ran `nexus42 daemon start` first) is intact. Only the `Free` cold-start case skips the HTTP round-trip — dropping the felt latency from "up to 2s probe" to "<50ms TCP gate".

**Two-phase poll:** `wait_for_first_health` now polls at 100ms for the first 1s (fast ready-detection), then 250ms (steady). Global `HEALTH_POLL_INTERVAL` unchanged.

**Unit-test boundary:** do NOT spawn the real `nexus42` binary in `#[cfg(test)]` — it hangs (the daemon needs DB/runtime resources unavailable in the test env). Test `probe_port_state` + timing directly; the full `start_with_budget` spawn path is integration territory.
- ❌ Treating `starting` as "ready" (health probe may still fail; main UI would render against an unreachable daemon).
- ❌ **Subscribing without a mount-time state probe** (V1.96 regression root cause #1 — the daemon may have already transitioned before the SPA subscribed; the event is lost; the UI hangs forever). See "V1.96 refinements" below.
- ❌ **Leaving a state-enum branch implicit** (V1.96 regression root cause #2 — the `'starting'` branch was missing → callback was silent → no state update → UI stuck). Every state must have an explicit branch.
- ❌ **Discarding the supervised process's stderr** (V1.96 residual `R-V195-ARCH-STRERR-GAP` — the daemon's real crash reason was never captured; the wizard showed a generic "Daemon did not start" message).

## V1.96 refinements: late-subscription race + diagnostic surfacing

V1.96 () hit a P0 blocker: the setup wizard Step 2 hung indefinitely in "Starting daemon…" on a clean `~/.nexus42/` first launch. RCA revealed **three** consumer-side root causes (pre-V1.118, the daemon also crashed within milliseconds when `WorkspaceState::initialize()` found no `active_creator_id`; the wizard just never learned about it). The fixes distill into four durable rules that apply to **any** observer of a process lifecycle event stream, not just the daemon-ready gate.

> **V1.118 supersession (daemon no-Profile boot):** After V1.118 P0 ships, clean home reaches T0 health without `active_creator_id`; the gate opens on `running` and Profile selection is post-gate business flow. Crash-on-no-creator RCA below is **pre-V1.118** only. See [daemon-runtime.md §17](../../specs/daemon-runtime.md) + [desktop-shell.md §13.11](../../specs/desktop-shell.md).

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

Distilled from V1.96 . Iteration-scoped RCA with code sketches: `daemon-startup-rca.md` (snapshot; promoted here).

## V1.97 refinements: initial-state correctness + Tauri v2 spawn-name resolution

V1.97 () ran the first real clean-state desktop smoke against the bundled sidecar and found **two latent first-launch blockers** that had been present since V1.66 but never surfaced (no prior iteration had actually spawned the sidecar end-to-end). Both distil into durable rules that apply to the supervisor state machine and the Tauri shell-plugin contract.

### Rule 9: a freshly constructed SidecarManager starts in `Stopped`, never `Starting`

`SidecarManager::new` has no owned child process. Defaulting its initial state to `Starting` is an **invalid compound state** (`Starting` + `child.is_none()`): it implies a spawn is in progress when none is, and — critically — it causes `start_with_budget` to short-circuit and never attempt a real spawn. The desktop app would report "Starting daemon…" forever on a clean launch while never actually spawning anything.

**Pattern**: the initial state of any supervisor that does not yet own the supervised process is the inactive terminal (`Stopped`), not a transient "in progress" state. Transient states (`Starting`) are only valid while a spawn attempt is actually in progress or an owned child is being health-probed.

### Rule 10: the `Starting` short-circuit must be gated on `child.is_some()`

Even with a correct initial state, `start_with_budget` typically has a "don't double-spawn" short-circuit: `if state == Starting { return }`. That guard must additionally require `inner.child.is_some()` — otherwise a stale `Starting` (e.g. left over from a failed spawn that never cleared) silently suppresses every subsequent spawn/retry. `Starting` + `child.is_none()` is invalid and must NOT block a real spawn attempt.

**Pattern**:
```rust
// WRONG: a stale Starting suppresses all real spawns
if state == Starting { return Ok(()); }
// RIGHT: only short-circuit when an owned child is actually being supervised
if state == Starting && inner.child.is_some() { return Ok(()); }
```

### Rule 11: Tauri v2 `app.shell().sidecar(name)` takes the FILENAME, not the `externalBin` path

This was the more severe latent blocker. `bundle.externalBin` in `tauri.conf.json` lists **build-time source paths** (e.g. `["binaries/nexus42"]`); Tauri appends the target-triple suffix (`nexus42-aarch64-apple-darwin`) at bundle time. But the **runtime** Rust API `tauri_plugin_shell::ShellExt::sidecar()` "expects only the filename of the sidecar, not its full path" (Tauri v2 docs, `https://v2.tauri.app/develop/sidecar`). The desktop shell had called `app.shell().sidecar("binaries/nexus42")` since V1.66 — which **never resolves**, so the bundled daemon was never spawnable from the desktop app. The error (`failed to spawn sidecar: No such file or directory (os error 2)`) only appeared once V1.97 actually ran a clean-state smoke.

The matching `tauri-plugin-shell` capability scope (`shell:allow-execute`, `sidecar: true`) registers a `name` that must be **byte-identical** to the `sidecar()` argument — so both must be the bare filename (`"nexus42"`), while `bundle.externalBin` keeps the source-relative path (`"binaries/nexus42"`).

**Pattern**:
```rust
// tauri.conf.json  (build-time source path — UNCHANGED)
//   "bundle": { "externalBin": ["binaries/nexus42"] }
// sidecar.rs (runtime — filename only)
let command = app.shell().sidecar("nexus42")?;   // NOT "binaries/nexus42"
// capabilities/main.json (scope name must match the sidecar() arg)
//   { "name": "nexus42", "sidecar": true, "args": [...] }
```

> **JS vs Rust asymmetry (footgun):** the JavaScript `Command.sidecar('binaries/my-sidecar', ...)` API *does* accept the path-form string matching the `externalBin` entry. The Rust `app.shell().sidecar()` does NOT. When porting a sidecar between JS-triggered and Rust-triggered spawn sites, re-derive the argument from the docs — do not copy it across the language boundary.

### Rule 12: attaching to a healthy daemon must not fabricate an owned child handle

The attach path (daemon already running on the resolved port, e.g. user ran `nexus42 daemon start` first, or a prior desktop session left it running) may report `state: Running` — but it must set `owned: false` and must NOT insert a child handle. Stop/quit cleanup terminates **only** processes the desktop app actually spawned. This preserves existing-install attach behavior without risking killing an unrelated user-started daemon.

### Why these compound

Rules 9 + 10 fix the supervisor state machine so a real spawn is actually attempted. Rule 11 fixes the framework-contract misuse so the spawn can actually resolve the binary. Rule 12 keeps attach honest. Together they make the desktop clean-state first-launch path **reachable** — before V1.97 it was silently broken at two layers (state machine never spawned, and even if it had, the sidecar name would not resolve). The deeper remaining gap (the daemon requires an active creator to boot, and the desktop wizard does not bootstrap one before the `.setup()` auto-start — see residual `R-V197-SMOKE-CLEAN-STATE`) is a product-architecture deferral tracked for V1.98, not a state-machine or shell-contract rule.

### Rule 13 (V1.100, historical): gate `.setup()` auto-start behind `setup_completed`

> **Superseded by Rule 15 (V1.105 D2).** Kept for archaeology only.

### Rule 15 (V1.105): always auto-start sidecar; outer `DaemonLaunchGate`; bootstrap on Workspace Continue

| Lock | Value |
|------|-------|
| Auto-start | Tauri `.setup()` **always** `SidecarManager::start` — ignore `setup_completed` |
| Wait UX | Outer `DaemonLaunchGate` before `/setup` **and** main UI |
| Marker | Inner `SetupGate` routes by `setup_completed` only |
| Bootstrap | `ensureSetupBootstrap` on Workspace **Continue** (after Ready), not before daemon start |
| Wizard | Agent → Workspace → Done; no Daemon step; no happy-path `startDaemon` |

**Why:** Authors must pick an ACP Agent first, but scan needs a Ready daemon. Making wait app-level (not a wizard preference) keeps the product narrative honest while preserving the V1.100 bootstrap IPC contract at a later step.

**Source:** V1.105  + `wizard-ia-reorder` + `portrait-shell-steps`; masters `desktop-shell.md` §13.10 / `web-ui.md` §29.13.

> Residual `R-V197-SMOKE-CLEAN-STATE` was closed by V1.100 P0. V1.105 moves bootstrap **after** Ready (Workspace Continue) while keeping always-start. **V1.118 P0 supersedes the no-creator boot failure:** clean home reaches T0 health without `active_creator_id`; bootstrap remains optional wizard convenience (§13.11 / daemon-runtime §17).

### Rule 14 (V1.101): enrich process PATH at daemon boot for agent CLI discovery (Class B) — no schemas/

V1.101 P0 closed `R-V1100P0SMOKE-AGENT-SCAN`: macOS GUI / Tauri-launched daemons often inherit a minimal PATH (`/usr/bin:/bin:/usr/sbin:/sbin`). Homebrew and user-local ACP agent CLIs under `/opt/homebrew/bin`, `~/.local/bin`, `~/.cargo/bin`, asdf/mise shims, etc. are then invisible to `which::which` during `POST /v1/daemon/agent-host/scan`, so the setup AgentPicker shows empty even when agents are installed in a login shell.

**Pattern:** merge a **login-shell-equivalent** set of common user bin dirs into the **process** `PATH` **once at daemon boot** (before any scan probe). Implementation: `crates/nexus-daemon-runtime/src/path_enrichment.rs`, invoked from `run_daemon`. No shell-out; only existing directories are appended; idempotent merge.

| Class | Meaning | Fix locus |
|-------|---------|-----------|
| **A** | Scan API / UI wiring broken | App / handler |
| **B** | Daemon PATH/env incomplete vs login shell | **Process PATH enrichment at daemon/sidecar start** (this rule) |
| **C** | Product gap (custom launch only) | Document escape hatch; do not invent schema |

**Hard stop:** Class B must **not** add wire fields, query params, or `schemas/` changes. Prefer process env enrichment over teaching the SPA about PATH. Exotic layouts still use custom-launch.

**Anti-patterns:** ❌ Proposing a scan schema change for “extra PATH”; ❌ shelling out to `echo $PATH` / login shell on every scan; ❌ duplicating enrichment only in the Tauri sidecar without covering `nexus42 daemon start`.

### Source

Distilled from V1.97 . Tauri v2 sidecar resolution rule confirmed against `https://v2.tauri.app/develop/sidecar` ("expects only the filename of the sidecar, not its full path"). Iteration-scoped invariants + prototype intake rule: `sidecar-startup-state-machine.md` (snapshot; durable rules promoted here). **Rule 13 distilled from V1.100 P0** . **Rule 14 distilled from V1.101 P0** . **Rule 15 distilled from V1.105** P0–P2 (`DaemonLaunchGate`, D2 always-start, Agent→Workspace→Done, portrait shell).
