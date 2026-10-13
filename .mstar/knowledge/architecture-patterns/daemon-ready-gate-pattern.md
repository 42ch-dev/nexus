---
module: apps/desktop-electron + apps/web (setup-gate, daemon-status-bar, main-banner)
date: 2026-07-06
last_updated: 2026-10-12
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
tags:
  - service-host
  - sidecar
  - health-probe
  - desktop-shell
  - setup-wizard
  - daemon-status-bar
  - gate
  - two-consumer-pattern
applies_when:
  - 'gating main-UI entry on daemon readiness; designing any "wait for service X before entering app" UX; wiring observers to a process lifecycle event stream that may fire before subscription; surfacing supervised-process crash reasons to the user'
---

# Daemon-Ready Gate Pattern (App-Level Launch Gate)

**Track**: Knowledge (durable guidance).

## Context

The Electron desktop host owns the app-managed TypeScript service (`apps/nexus-service`) lifecycle through its main-process `DesktopServiceController` (`apps/desktop-electron/src/service-controller.ts`), and exposes a single status event stream `onDaemonStatusChanged` carrying `DaemonStatus { state, version, port, detail }`. States: `starting → running → degraded → stopped → error`.

Wait ownership collapses into one outer app-level gate:

1. **`DaemonLaunchGate`** (every launch) — fullscreen splash until Ready; wraps **all** routes including `/setup`.
2. **`SetupGate`** (marker only) — after Ready, routes incomplete setup to `/setup` vs main UI. **No splash.**
3. **Setup wizard** — Agent → Workspace → Done only; **no** Daemon wizard step (`setup-step-daemon.tsx` deleted).

The naive implementation would create two independent health-probe polls or two host commands ("is the daemon ready yet?"). Both are wrong: they compete with the existing `DesktopServiceController` state machine, can deadlock on the 15s `HEALTH_START_TIMEOUT` boundary, and drift from the source of truth.

## Guidance (the pattern)

**Single source of truth, multiple observers.** The `DesktopServiceController` owns daemon lifecycle state. Consumers observe the existing `onDaemonStatusChanged` event; they do NOT spawn their own probes, do NOT poll a health endpoint independently, and do NOT ask for a new host command when the event already carries the signal.

| Rule | Reason |
|------|--------|
| Subscribe to `onDaemonStatusChanged`; do not add `is_daemon_ready()` commands | The event already carries `state`; a synchronous command would race with the state machine and lie during transitions |
| Treat `state: "running"` as the only "gate open" signal | `starting` is not ready (health probe pending); `degraded`/`error`/`stopped` are explicit failures |
| Surface failure paths explicitly (`error`, `stopped`, timeout) | Silent hangs are the worst UX; the wizard step 2 and the per-launch splash both need actionable CTAs (Restart, Open Logs, etc.). The wizard timeout is 25s with a mount-time probe — see `desktop-shell.md` §13.10. |
| Two consumers can coexist without coordination | The event is fan-out; each consumer maintains its own UI state but reads the same lifecycle signal |
| Do not call `start_daemon()` from a consumer unless you intend to reset the crash budget | `start_daemon()` resets `restart_count`; `start()` does not. The wizard may reset (user-initiated); the per-launch splash must not (automatic) |

## Why This Matters

- **Avoids deadlock.** Independent probes racing the state machine can hang the UI past the 15s `HEALTH_START_TIMEOUT`.
- **Keeps the gate truthful.** The event stream reflects what `DesktopServiceController` actually believes; a command would snapshot a moment that may already be stale.
- **Supports two distinct UX flows from one signal.** First-launch wizard and returning-user splash are different visuals driven by the same state machine — no duplicated logic.

## When to Apply

- Adding any new "wait for daemon" surface (loading splash, error banner, reconnect attempt).
- Designing a similar gate for any other local service the app supervises (sidecar, embedded runtime, attached external process).
- Whenever the temptation arises to add an `is_X_ready()` command — first check whether an event stream already carries the signal.

## Examples

- **Outer launch gate** (`apps/web/src/components/setup/daemon-launch-gate.tsx` + `daemon-ready-splash.tsx`): mount-time `getDaemonStatus` + `onDaemonStatusChanged`; 25s timeout; retry = reload; reset = `resetLocalDatabase` + reload; **no** happy-path `startDaemon` (D2 always-starts in the host `.setup()`).
- **Marker routing** (`apps/web/src/components/setup/setup-gate.tsx`): after outer gate Ready — `!setup_completed` → `/setup`; else main shell. No splash/subscribe.
- **Do not reintroduce a Setup Wizard Step 2** that owns wait + `startDaemon` inside the wizard (`setup-step-daemon.tsx` is gone).
- **Crash banner** (`apps/web/src/components/layout/main-banner.tsx`): long-running-session consumer; appears when state degrades from `"running"` to `"degraded"`/`"error"`/`"stopped"`.

## Anti-patterns

- ❌ Adding an `is_daemon_ready() -> bool` host command (race-prone; the event already carries it).
- ❌ Polling `GET /v1/daemon/runtime/health` from the SPA (duplicates `DesktopServiceController`'s own probe).
- ❌ Two consumers calling `start_daemon()` independently (budget-reset semantics differ; can mask crash loops).

## Owner probe: attach by identity, else spawn

`DesktopServiceController.startLocked` never probes the port's TCP state. Before spawning it calls `probeOwner()`, which decides from the service's own published identity, and forks the utility only when no service can be proven:

| `probeOwner()` outcome | Evidence | Action |
|------------------------|----------|--------|
| `attached` | `~/.nexus42/run/service.json` names this session's home and loopback origin, and that endpoint answers the guarded `GET /v1/daemon/runtime/discovery` with a matching identity plus `GET /v1/daemon/runtime/health` `status: "ok"` | `state: running`, held as `attached` (no owned child), `detail: 'attached to an independent service'` |
| `conflict` | a record from another home or naming another endpoint, or a listener that answers HTTP without proving matching discovery | `state: error` with the conflict `detail`; the start throws — a foreign listener is never adopted, and is never killed by pid |
| `none` | no record and no HTTP answer on this session's origin | `startOwned()` forks the utility and waits for its readiness report |

**Spawn path:** `startOwned` forks the utility (`utilityProcess.fork(..., { stdio: 'ignore' })`), posts the `start` request with the trusted home/host/port, and waits up to `READINESS_DEADLINE_MS` (15s) for the owner's `service-ready` discovery frame. The reply, the published record and the ready instance must agree on instance, home and endpoint; the controller then re-proves authenticated discovery + health (`confirmIdentity`) before it sets `owned` and reports `running`. A cold start forks and waits — there is no TCP gate that skips a probe.

**Probe cadence:** `confirmIdentity` re-probes at `PROBE_FAST_INTERVAL_MS` (100ms) for the first `PROBE_FAST_WINDOW_MS` (1s), then `PROBE_SLOW_INTERVAL_MS` (250ms); every request is bounded by `PROBE_REQUEST_TIMEOUT_MS` (2s) inside the 15s deadline.

**Test boundary:** drive the controller through its injected seams — `spawnUtility` (a fake `UtilityChildProcess`) and `ControllerClock` — rather than starting a real service; the real fork path is integration territory.
- ❌ Treating `starting` as "ready" (health probe may still fail; main UI would render against an unreachable daemon).
- ❌ **Subscribing without a mount-time state probe** (the daemon may have already transitioned before the SPA subscribed; the event is lost; the UI hangs forever). See "Late-subscription race + diagnostic surfacing" below.
- ❌ **Leaving a state-enum branch implicit** (a missing `'starting'` branch → callback silent → no state update → UI stuck). Every state must have an explicit branch.
- ❌ **Discarding the owner's crash diagnostic** (the real failure reason never reaches `detail`; the wizard shows a generic "Daemon did not start" message).

## Late-subscription race + diagnostic surfacing

The setup wizard Step 2 could hang indefinitely in "Starting daemon…" on a clean `~/.nexus42/` first launch. RCA revealed **three** consumer-side root causes. The fixes distill into four durable rules that apply to **any** observer of a process lifecycle event stream, not just the daemon-ready gate.

> **No-Profile boot:** a clean home reaches T0 health without `active_creator_id`; the gate opens on `running` and Profile selection is post-gate business flow. See [desktop-shell.md §13.11](../../specs/active/surfaces/desktop-shell.md).

### Rule 5: probe current state on mount, BEFORE subscribing

The daemon auto-starts at host boot (`lib.rs` setup hook), which runs **before** the SPA's React tree mounts. By the time the SPA subscribes to `onDaemonStatusChanged`, the daemon may have already transitioned to `running` (normal) or `error` (crashed on boot). The first event the SPA would otherwise receive is `state: 'starting'` — but if the daemon already exited, even that event was emitted before subscription and is lost.

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

A callback shaped like `if (running) {...} else if (error || stopped) {...}` with **no** `'starting'` and **no** `'degraded'` branch leaves the first event after subscription (`'starting'`) unmatched → no state update → `setReady(false)` stays → UI stuck forever.

**Pattern**: factor status handling into a single `applyStatus(status)` function that covers **every** state in the enum. No-op branches (`'starting'` → keep the spinner) are explicit, not implicit fall-through silence. This also lets the mount-probe and the subscription share one code path (no drift).

### Rule 7: bound the wait with a hard timeout that RE-PROBES

A consumer that subscribes and waits can hang forever if no terminal event arrives (event lost, process stuck in initialization, subscription silently dropped). A hard timeout (25s; the `DesktopServiceController`'s own readiness deadline is 15s — the consumer timeout should be LONGER to give the state machine room) prevents indefinite hangs.

**Pattern**: `setTimeout(() => { re-probe getDaemonStatus(); if still non-terminal → surface "taking longer than expected" }, 25_000)`. The re-probe before declaring timeout avoids misreporting when the process silently transitioned between mount-probe and timeout fire. Clear the timeout in the effect cleanup.

### Rule 8: carry the owner's own crash diagnostic into `detail`

When the service fails, `DaemonStatus.detail` must carry more than a generic `DesktopServiceController` message ("Daemon did not start. Check the logs or try restarting."). The owner reports its own pre-exit diagnostic as the typed `utility-crashed` frame (`utility-host.ts` `reportCrash`, hooked to `uncaughtException` / `unhandledRejection`); `onChildMessage` retains it as `crashDetail` (capped at 512 chars) and `scheduleRecovery` folds it into the recovery `detail` as `<reason>: <crashDetail>`. The controller does not read the child's stderr — `utilityProcess.fork` is called with `stdio: 'ignore'`.

**Pattern**: surface `detail` verbatim on the `degraded`/`stopped` transitions these diagnostics ride on, and keep the generic message as the fallback when the owner reported nothing. The consumer renders `detail` with `whitespace-pre-wrap` so a multi-line owner message survives the browser's default `white-space: normal` collapse.

### Why these four rules compound

Rules 5+6+7 together make the consumer resilient to **any** timing of the event stream: late subscription (5), partial coverage (6), or total event loss (7). Rule 8 makes the failure **actionable**: the user reads the real crash reason and can act (reset the DB, free the port, install the binary) instead of staring at "Daemon did not start." Without rule 8, even a perfectly-timed subscription surfaces an unhelpful generic message.

## Initial-state correctness + spawn-name resolution

A clean-state desktop smoke against the bundled sidecar found **two latent first-launch blockers** that only surface when the sidecar is actually spawned end-to-end. Both distil into durable rules that apply to the supervisor state machine and the host shell-plugin contract.

### Rule 9: a freshly constructed `DesktopServiceController` starts in `Stopped`, never `Starting`

The `DesktopServiceController` constructor has no owned child process. Defaulting its initial state to `Starting` is an **invalid compound state** (`Starting` + no owned child): it implies a spawn is in progress when none is, and — critically — it causes the start path to short-circuit and never attempt a real spawn. The desktop app would report "Starting daemon…" forever on a clean launch while never actually spawning anything.

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

### Rule 11: a spawn argument must match the registered runtime name byte-for-byte

A bundled binary is registered under a **runtime name** distinct from its build-time source path: the bundle config lists source-relative paths (e.g. `["binaries/nexus42"]`) and the bundler appends the target-triple suffix (`nexus42-aarch64-apple-darwin`) at bundle time, while the runtime spawn API takes the bare registered name. The spawn argument and the host's capability/scope registration (`name`, `sidecar: true`) must both use that same bare name — passing the source-relative path silently fails to resolve (`failed to spawn sidecar: No such file or directory (os error 2)`), so the bundled binary is never spawnable, and the failure surfaces only when a clean-state smoke actually spawns it.

**Pattern**:
```rust
// bundle config  (build-time source path — UNCHANGED)
//   "bundle": { "externalBin": ["binaries/nexus42"] }
// sidecar.rs (runtime — registered name only)
let command = app.shell().sidecar("nexus42")?;   // NOT "binaries/nexus42"
// capabilities/main.json (scope name must match the sidecar() arg)
//   { "name": "nexus42", "sidecar": true, "args": [...] }
```

> **JS vs Rust asymmetry (footgun):** a JavaScript `Command.sidecar('binaries/my-sidecar', ...)` API may accept the path-form string matching the `externalBin` entry while the Rust `app.shell().sidecar()` does not. When porting a sidecar between JS-triggered and Rust-triggered spawn sites, re-derive the argument from the docs — do not copy it across the language boundary.

### Rule 12: attaching to a healthy daemon must not fabricate an owned child handle

The attach path (service already running on the resolved port, e.g. an independently started service, or a prior desktop session left it running) may report `state: Running` — but it must set `owned: false` and must NOT insert a child handle. Stop/quit cleanup terminates **only** processes the desktop app actually spawned. This preserves existing-install attach behavior without risking killing an unrelated user-started service.

### Why these compound

Rules 9 + 10 fix the supervisor state machine so a real spawn is actually attempted. Rule 11 fixes the framework-contract misuse so the spawn can actually resolve the binary. Rule 12 keeps attach honest. Together they make the desktop clean-state first-launch path **reachable** — without them it is silently broken at two layers (the state machine never spawns, and even if it did, the sidecar name would not resolve).

### Rule 15: always auto-start sidecar; outer `DaemonLaunchGate`; bootstrap on Workspace Continue

| Lock | Value |
|------|-------|
| Auto-start | Electron main **always** starts or attaches the app-managed service — ignore `setup_completed` |
| Wait UX | Outer `DaemonLaunchGate` before `/setup` **and** main UI |
| Marker | Inner `SetupGate` routes by `setup_completed` only |
| Bootstrap | `ensureSetupBootstrap` on Workspace **Continue** (after Ready), not before daemon start |
| Wizard | Agent → Workspace → Done; no Daemon step; no happy-path `startDaemon` |

**Why:** Authors must pick an ACP Agent first, but scan needs a Ready daemon. Making wait app-level (not a wizard preference) keeps the product narrative honest while preserving the bootstrap IPC contract at a later step.

> Bootstrap moves **after** Ready (Workspace Continue) while keeping always-start. A clean home reaches T0 health without `active_creator_id`; bootstrap remains optional wizard convenience (§13.11).

### Rule 14: enrich process PATH at startup for agent CLI discovery (Class B) — no schemas/

macOS GUI-launched processes often inherit a minimal PATH (`/usr/bin:/bin:/usr/sbin:/sbin`). Homebrew and user-local ACP agent CLIs under `/opt/homebrew/bin`, `~/.local/bin`, `~/.cargo/bin`, asdf/mise shims, etc. are then invisible to `which::which` during `POST /v1/daemon/agent-host/scan`, so the setup AgentPicker shows empty even when agents are installed in a login shell.

**Pattern:** merge a **login-shell-equivalent** set of common user bin dirs into the **process** `PATH` **once at startup** (before any scan probe). Implementation: `crates/nexus-agent-host/src/discovery/path_enrichment.rs` (`apply_process_path_enrichment`, `:333`, built on `login_equivalent_bin_dirs`, `:165`), called from the CLI's synchronous `main` **before Tokio starts** — `apps/nexus42/src/main.rs:27`. No shell-out; only existing directories are appended; idempotent merge.

| Class | Meaning | Fix locus |
|-------|---------|-----------|
| **A** | Scan API / UI wiring broken | App / handler |
| **B** | Daemon PATH/env incomplete vs login shell | **Process PATH enrichment at daemon/sidecar start** (this rule) |
| **C** | Product gap (custom launch only) | Document escape hatch; do not invent schema |

**Hard stop:** Class B must **not** add wire fields, query params, or `schemas/` changes. Prefer process env enrichment over teaching the SPA about PATH. Exotic layouts still use custom-launch.

**Anti-patterns:** ❌ Proposing a scan schema change for “extra PATH”; ❌ shelling out to `echo $PATH` / login shell on every scan; ❌ duplicating enrichment only in the desktop sidecar without covering the CLI start path.
