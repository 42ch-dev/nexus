# daemon ready gate pattern — Historical Notes

**Provenance**: frozen from [`.mstar/knowledge/architecture-patterns/daemon-ready-gate-pattern.md`](../../../knowledge/architecture-patterns/daemon-ready-gate-pattern.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed text spans multiple iteration markers with no single uncontested calendar date, so the freeze date is used and stated here).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live doc; their superseded wording appears here. Nothing here is current authority.

### at pre-freeze line 23

```markdown
**Track**: Knowledge (durable guidance distilled from V1.94 Desktop App Onboarding & IA Pass).
```

### at pre-freeze line 27

```markdown
V1.66 (the retired Desktop Shell) shipped a `SidecarManager` that auto-starts the bundled `nexus42` daemon and exposes a single lifecycle event stream `onDaemonStatusChanged` carrying `DaemonStatus { state, version, port, detail }`. States: `starting → running → degraded → stopped → error`.
```

### at pre-freeze line 29

```markdown
V1.94 introduced **two distinct consumers** of that single event stream (wizard step + per-launch splash). **V1.105 collapses wait ownership into one outer app-level gate:**
```

### at pre-freeze line 45

```markdown
| Surface failure paths explicitly (`error`, `stopped`, timeout) | Silent hangs are the worst UX; the wizard step 2 and the per-launch splash both need actionable CTAs (Restart, Open Logs, etc.). V1.96 changed the wizard timeout from 15s to 25s with a mount-time probe — see `desktop-shell.md` §13.10. |
```

### at pre-freeze line 63

```markdown
- **Outer launch gate (V1.105 current)** (`apps/web/src/components/setup/daemon-launch-gate.tsx` + `daemon-ready-splash.tsx`): mount-time `getDaemonStatus` + `onDaemonStatusChanged`; 25s timeout; retry = reload; reset = `resetLocalDatabase` + reload; **no** happy-path `startDaemon` (D2 always-starts in the host `.setup()`).
- **Marker routing (V1.105)** (`apps/web/src/components/setup/setup-gate.tsx`): after outer gate Ready — `!setup_completed` → `/setup`; else main shell. No splash/subscribe.
- **Historical — Setup Wizard Step 2** (`setup-step-daemon.tsx`, deleted in V1.105 P1): previously owned wait+`startDaemon` inside the wizard. Do not reintroduce.
```

### at pre-freeze line 74

```markdown
## V1.110 refinement — three-valued port-probe gate (FB-D1)
```

### at pre-freeze line 76

```markdown
V1.110 optimized the cold-start path. Previously `start_with_budget` ran the full HTTP `probe_health` (2s timeout) **before** spawning the sidecar — a tax paid on every cold start where no daemon was running. V1.110 inserts a **three-valued TCP gate** first:
```

### at pre-freeze line 90

```markdown
- ❌ **Subscribing without a mount-time state probe** (V1.96 regression root cause #1 — the daemon may have already transitioned before the SPA subscribed; the event is lost; the UI hangs forever). See "V1.96 refinements" below.
- ❌ **Leaving a state-enum branch implicit** (V1.96 regression root cause #2 — the `'starting'` branch was missing → callback was silent → no state update → UI stuck). Every state must have an explicit branch.
- ❌ **Discarding the supervised process's stderr** (V1.96 residual `R-V195-ARCH-STRERR-GAP` — the daemon's real crash reason was never captured; the wizard showed a generic "Daemon did not start" message).
```

### at pre-freeze line 94

```markdown
## V1.96 refinements: late-subscription race + diagnostic surfacing
```

### at pre-freeze line 96

```markdown
V1.96 () hit a P0 blocker: the setup wizard Step 2 hung indefinitely in "Starting daemon…" on a clean `~/.nexus42/` first launch. RCA revealed **three** consumer-side root causes (pre-V1.118, the daemon also crashed within milliseconds when `WorkspaceState::initialize()` found no `active_creator_id`; the wizard just never learned about it). The fixes distill into four durable rules that apply to **any** observer of a process lifecycle event stream, not just the daemon-ready gate.
```

### at pre-freeze line 98

```markdown
> **V1.118 supersession (daemon no-Profile boot):** After V1.118 P0 ships, clean home reaches T0 health without `active_creator_id`; the gate opens on `running` and Profile selection is post-gate business flow. Crash-on-no-creator RCA below is **pre-V1.118** only. See [daemon-runtime.md §17 (archived)](../runtime/2026-09-21-daemon-runtime.md) + [desktop-shell.md §13.11](../../active/surfaces/desktop-shell.md).
```

### at pre-freeze line 120

```markdown
The V1.96 bug had a callback shaped like `if (running) {...} else if (error || stopped) {...}` with **no** `'starting'` and **no** `'degraded'` branch. The first event after subscription was `'starting'` → no branch matched → no state update → `setReady(false)` stayed → UI stuck forever.
```

### at pre-freeze line 126

```markdown
A consumer that subscribes and waits can hang forever if no terminal event arrives (event lost, process stuck in initialization, subscription silently dropped). A hard timeout (V1.96 uses 25s; the SidecarManager's own health timeout is 15s — the consumer timeout should be LONGER to give the state machine room) prevents indefinite hangs.
```

### at pre-freeze line 132

```markdown
When the daemon fails to start, `DaemonStatus.detail` originally carried only generic SidecarManager messages ("Daemon did not start. Check the logs or try restarting.") — the **real** crash reason (SQLite migration mismatch, missing config, port bind failure, binary missing) was written to the daemon's stderr but never captured. The `_rx` event receiver from `command.spawn()` was discarded.
```

### at pre-freeze line 140

```markdown
### Source
```

### at pre-freeze line 142

```markdown
Distilled from V1.96 . Iteration-scoped RCA with code sketches: `daemon-startup-rca.md` (snapshot; promoted here).

## V1.97 refinements: initial-state correctness + v2 host spawn-name resolution

V1.97 () ran the first real clean-state desktop smoke against the bundled sidecar and found **two latent first-launch blockers** that had been present since V1.66 but never surfaced (no prior iteration had actually spawned the sidecar end-to-end). Both distil into durable rules that apply to the supervisor state machine and the host shell-plugin contract.
```

### at pre-freeze line 166

```markdown
### Rule 11: the v2 host's `app.shell().sidecar(name)` takes the FILENAME, not the `externalBin` path

> **Historical (host retired v1.192):** this rule documents the retired v2 desktop host, which was replaced by the Electron composition (`apps/desktop-electron`). The general lesson — a sidecar/spawn argument must match the registered runtime name byte-for-byte, not the build-time source path — still applies to any host's child-process launch; the concrete bundle-config / shell-plugin details below are a historical record.
```

### at pre-freeze line 170

```markdown
This was the more severe latent blocker. `bundle.externalBin` in the bundle config lists **build-time source paths** (e.g. `["binaries/nexus42"]`); the bundler appends the target-triple suffix (`nexus42-aarch64-apple-darwin`) at bundle time. But the **runtime** Rust API `ShellExt::sidecar()` "expects only the filename of the sidecar, not its full path" (host sidecar docs). The desktop shell had called `app.shell().sidecar("binaries/nexus42")` since V1.66 — which **never resolves**, so the bundled daemon was never spawnable from the desktop app. The error (`failed to spawn sidecar: No such file or directory (os error 2)`) only appeared once V1.97 actually ran a clean-state smoke.

The matching shell-plugin capability scope (`shell:allow-execute`, `sidecar: true`) registers a `name` that must be **byte-identical** to the `sidecar()` argument — so both must be the bare filename (`"nexus42"`), while `bundle.externalBin` keeps the source-relative path (`"binaries/nexus42"`).
```

### at pre-freeze line 178

```markdown
// sidecar.rs (runtime — filename only)
```

### at pre-freeze line 184

```markdown
> **JS vs Rust asymmetry (footgun):** the JavaScript `Command.sidecar('binaries/my-sidecar', ...)` API *does* accept the path-form string matching the `externalBin` entry. The Rust `app.shell().sidecar()` does NOT. When porting a sidecar between JS-triggered and Rust-triggered spawn sites, re-derive the argument from the docs — do not copy it across the language boundary.
```

### at pre-freeze line 192

```markdown
Rules 9 + 10 fix the supervisor state machine so a real spawn is actually attempted. Rule 11 fixes the framework-contract misuse so the spawn can actually resolve the binary. Rule 12 keeps attach honest. Together they make the desktop clean-state first-launch path **reachable** — before V1.97 it was silently broken at two layers (state machine never spawned, and even if it had, the sidecar name would not resolve). The deeper remaining gap (the daemon requires an active creator to boot, and the desktop wizard does not bootstrap one before the `.setup()` auto-start — see residual `R-V197-SMOKE-CLEAN-STATE`) is a product-architecture deferral tracked for V1.98, not a state-machine or shell-contract rule.

### Rule 13 (V1.100, historical): gate `.setup()` auto-start behind `setup_completed`
```

### at pre-freeze line 196

```markdown
> **Superseded by Rule 15 (V1.105 D2).** Kept for archaeology only.

### Rule 15 (V1.105): always auto-start sidecar; outer `DaemonLaunchGate`; bootstrap on Workspace Continue
```

### at pre-freeze line 208

```markdown
**Why:** Authors must pick an ACP Agent first, but scan needs a Ready daemon. Making wait app-level (not a wizard preference) keeps the product narrative honest while preserving the V1.100 bootstrap IPC contract at a later step.

**Source:** V1.105  + `wizard-ia-reorder` + `portrait-shell-steps`; masters `desktop-shell.md` §13.10 / `web-ui.md` §29.13.
```

### at pre-freeze line 212

```markdown
> Residual `R-V197-SMOKE-CLEAN-STATE` was closed by V1.100 P0. V1.105 moves bootstrap **after** Ready (Workspace Continue) while keeping always-start. **V1.118 P0 supersedes the no-creator boot failure:** clean home reaches T0 health without `active_creator_id`; bootstrap remains optional wizard convenience (§13.11 / daemon-runtime §17, archived).
```

### at pre-freeze line 214

```markdown
### Rule 14 (V1.101): enrich process PATH at startup for agent CLI discovery (Class B) — no schemas/
```

### at pre-freeze line 216

```markdown
V1.101 P0 closed `R-V1100P0SMOKE-AGENT-SCAN`: macOS GUI-launched daemons often inherit a minimal PATH (`/usr/bin:/bin:/usr/sbin:/sbin`). Homebrew and user-local ACP agent CLIs under `/opt/homebrew/bin`, `~/.local/bin`, `~/.cargo/bin`, asdf/mise shims, etc. are then invisible to `which::which` during `POST /v1/daemon/agent-host/scan`, so the setup AgentPicker shows empty even when agents are installed in a login shell.
```

### at pre-freeze line 218

```markdown
**Pattern:** merge a **login-shell-equivalent** set of common user bin dirs into the **process** `PATH` **once at startup** (before any scan probe). Implementation: `crates/nexus-agent-host/src/discovery/path_enrichment.rs` (`apply_process_path_enrichment`, `:333`, built on `login_equivalent_bin_dirs`, `:165`), called from the CLI's synchronous `main` **before Tokio starts** — `apps/nexus42/src/main.rs:27`. No shell-out; only existing directories are appended; idempotent merge. (Historical: the former caller was the deleted Rust daemon's `run_daemon`; it is gone with the daemon host in v1.193 P2.)
```

### at pre-freeze line 229

```markdown

### Source

Distilled from V1.97 (the v2 host; retired in v1.192). The v2 sidecar resolution rule confirmed against the host's sidecar docs ("expects only the filename of the sidecar, not its full path"). Iteration-scoped invariants + prototype intake rule: `sidecar-startup-state-machine.md` (snapshot; durable rules promoted here). **Rule 13 distilled from V1.100 P0** . **Rule 14 distilled from V1.101 P0** . **Rule 15 distilled from V1.105** P0–P2 (`DaemonLaunchGate`, D2 always-start, Agent→Workspace→Done, portrait shell).
```

---

## Post-freeze increment — Greptile review round 1 (PR #412, 2026-10-13)

Removed from the live document after the freeze above, in the same plan's Greptile fix wave. These units describe the retired Rust supervisor (`start_with_budget`, `probe_port_state`, the `_rx` / `command.spawn()` stderr drain); the live document now describes the shipped `DesktopServiceController` owner probe and its `utility-crashed` diagnostic. Nothing here is current authority.

### Removed block 1 — three-valued port-probe gate (verbatim)

```markdown
## Three-valued port-probe gate (FB-D1)

`start_with_budget` runs a **three-valued TCP gate** before the HTTP probe, so a cold start where no daemon is running skips the HTTP round-trip:

| `probe_port_state(port)` | Meaning | Action |
|--------------------------|---------|--------|
| `Free` | TCP connect refused fast (port unused) | **Skip** HTTP probe; spawn immediately |
| `Occupied` | TCP connect succeeded (something listens) | HTTP `probe_health` → attach (`owned=false`) or port-conflict error |
| `Unknown` | Timeout / inconclusive (≤150ms gate) | HTTP `probe_health` → attach or spawn (safe fallback) |

**Critical invariant preserved:** `Occupied`/`Unknown` always run the HTTP probe, so the attach-without-ownership path for an independently started service is intact. Only the `Free` cold-start case skips the HTTP round-trip — dropping the felt latency from "up to 2s probe" to "<50ms TCP gate".

**Two-phase poll:** `wait_for_first_health` now polls at 100ms for the first 1s (fast ready-detection), then 250ms (steady). Global `HEALTH_POLL_INTERVAL` unchanged.

**Unit-test boundary:** do NOT spawn the real `nexus42` binary in `#[cfg(test)]` — it hangs (the daemon needs DB/runtime resources unavailable in the test env). Test `probe_port_state` + timing directly; the full `start_with_budget` spawn path is integration territory.
```

### Removed block 2 — anti-pattern bullet (verbatim, from the `## Anti-patterns` list)

```markdown
- ❌ **Discarding the supervised process's stderr** (the daemon's real crash reason is never captured; the wizard shows a generic "Daemon did not start" message).
```

### Removed block 3 — stderr-drain rule (verbatim)

```markdown
### Rule 8: capture the supervised process's stderr (bounded tail)

When the daemon fails to start, `DaemonStatus.detail` must carry more than generic `DesktopServiceController` messages ("Daemon did not start. Check the logs or try restarting.") — the **real** crash reason (SQLite migration mismatch, missing config, port bind failure, binary missing) is written to the service's stderr, so the `_rx` event receiver from `command.spawn()` must not be discarded.

**Pattern**: spawn a bounded async task that drains the `_rx` receiver, accumulates a tail capped at ~2 KiB (nearest newline boundary — keep the tail, drop the head), and appends it verbatim to `DaemonStatus.detail` on the Error transition. Keep the generic message as fallback when stderr is empty. Run the drain concurrently with `wait_for_first_health` — do NOT block the spawn path. The consumer renders `detail` with `whitespace-pre-wrap` so the multi-line stderr survives the browser's default `white-space: normal` collapse.
```
