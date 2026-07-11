# V1.110 P0 — Daemon startup latency (FB-D1) — spec draft

> Iteration-scoped spec draft. Architect refines in §5.2 Review & Edit chain.
> Long-term spec promotion to `{SPECS_DIR}/` only if it proves durable at
> iteration-close (via `mstar-compound`).

## 1. Problem

`apps/desktop/src-tauri/src/sidecar.rs` `start_with_budget` runs a sequential
`probe_health(port)` (HTTP GET, `HEALTH_PROBE_TIMEOUT = 2s`) **before**
spawning the bundled `nexus42` binary. On a true cold start this probe cannot
succeed, yet it gates the spawn. The felt "Daemon starting" slowness traces to
this probe-then-spawn-then-poll structure.

## 2. Current flow (evidence)

```
start_with_budget(app, reset_budget)
  ├─ probe_health(port)           # 2s timeout HTTP probe — gates spawn
  │    └─ Some → attach (no ownership)  ← preserve this path
  │    └─ None → continue
  ├─ app.shell().sidecar("nexus42").args([...]).spawn()
  ├─ wait_for_first_health(port, pid)   # poll 250ms up to 15s
  │    └─ Some → Running
  │    └─ None → kill child, Failed
```

Constants: `HEALTH_PROBE_TIMEOUT=2s`, `HEALTH_POLL_INTERVAL=250ms`,
`HEALTH_START_TIMEOUT=15s` (sidecar.rs:25-31).

## 3. Target state

The cold-start **auto** path spawns optimistically as soon as the port is
known free; a concurrent best-effort attach probe catches the "external daemon
already running" case without blocking spawn. The manual `start_daemon` path
keeps the attach probe (user explicitly asked to (re)start).

## 4. Locked decisions (architect §5.2 review)

### Q1 — Cold-start strategy: fast `port_free` TCP gate (Option a) ✅ LOCKED

**Verdict:** Introduce a **three-valued** port probe `probe_port_state(port) ->
PortState { Free, Occupied, Unknown }` as the cold-start gate; **skip the HTTP
`probe_health` only when the port is conclusively `Free`**.

- `TcpStream::connect(("127.0.0.1", port))` **succeeds** → `Occupied`
  (something is listening).
- connect errors with **connection-refused** → `Free` (loopback refuse is
  near-instant; the 2 s `HEALTH_PROBE_TIMEOUT` was an upper bound, never the
  steady cost on a free port, but the HTTP machinery round-trip is still
  redundant when we already know the port is free).
- **timeout or other connect error** → `Unknown`.

Decision matrix (applied in shared `start_with_budget`, both auto and manual
paths benefit; the `reset_budget` budget semantics are untouched):

| Port state | Action |
|------------|--------|
| `Free` | Spawn the bundled `nexus42` immediately — **skip** the HTTP probe. |
| `Occupied` | Run HTTP `probe_health`: success → **attach** (owned=false); fail → **port-conflict error** (existing diagnostic path at sidecar.rs:320). |
| `Unknown` | Run HTTP `probe_health`: success → **attach**; fail → **spawn** (treat as free-ish). |

**Why not Option b (parallelize probe + spawn):** racing the probe against spawn
means if the probe attaches to an *external* daemon we have already spawned a
second `nexus42` that will lose the port race and have to be killed — messy,
racy, and risks a transient double-daemon. Option a is simpler, unit-testable
("port free → spawn without HTTP probe"), and leverages the existing
`tcp_reachable` primitive (sidecar.rs:573) — generalized to three values and
given a tight gate timeout (≈150 ms; loopback connect resolves in <10 ms
normally).

**Attach invariant (critical):** `Occupied` **and** `Unknown` **always** run the
HTTP `probe_health`, so a running external daemon (user ran
`nexus42 daemon start` first) is always detected and attached without
ownership. The `Free` short-circuit is the only path that skips the HTTP probe,
and only when the OS has conclusively told us nothing is listening.

### Q2 — Inconclusive port: fall back to HTTP probe ✅ LOCKED

`Unknown` (timeout / non-refuse error) → HTTP `probe_health`; healthy → attach,
unhealthy → spawn. Never treat `Unknown` as `Free` (could spawn onto an occupied
port) nor as `Occupied` (could block spawn unnecessarily).

### Q3 — Poll-interval tuning: two-phase first-second burst ✅ LOCKED

In `wait_for_first_health` (sidecar.rs:582), use a **fast interval for the first
~1 s, then the steady interval**:

- Add `HEALTH_POLL_INTERVAL_FAST = 100 ms` and `FAST_POLL_WINDOW = 1 s`.
- In the poll loop, track `elapsed` since deadline start; use
  `HEALTH_POLL_INTERVAL_FAST` while `elapsed < FAST_POLL_WINDOW`, else the
  existing `HEALTH_POLL_INTERVAL` (250 ms).
- Do **not** lower the `HEALTH_POLL_INTERVAL` constant globally — it documents
  steady-state crash-restart polling intent; compute the fast phase locally.

Rationale: the daemon typically reaches `/health` within the first second; a
100 ms burst catches the ready transition ~3 polls sooner than 250 ms, trimming
felt latency without busy-looping for the full 15 s budget.

### Q4 — Startup-phase signal: tracing-only `phase` field ✅ LOCKED

Emit `tracing` logs distinguishing four phases (diagnostics-only; **no** wire
change, **no** new `DaemonState` enum value — the SPA state machine stays
`Starting → Running`):

| Phase | When | Fields |
|-------|------|--------|
| `port_probe` | before spawn, the `probe_port_state` gate | `phase`, `port`, `port_state` (`free`/`occupied`/`unknown`), `elapsed_ms` |
| `attach_probe` | HTTP `probe_health` when port `Occupied`/`Unknown` | `phase`, `port`, `attached` (bool), `elapsed_ms` |
| `spawn` | sidecar `spawn()` | `phase`, `port`, `pid` |
| `wait_for_ready` | `wait_for_first_health` poll loop (one summary log at transition) | `phase`, `port`, `ready` (bool), `polls`, `elapsed_ms` |

Field names are stable for log-grep. No `DaemonStatus.detail` change required
for DoD (UI progress surfacing is a future enhancement, not V1.110 scope).

## 5. Constraints (Global)

- **Preserve attach-without-ownership** for the external-daemon case
  (`start_daemon` manual path + "user ran `nexus42 daemon start` first"). The
  `Occupied`/`Unknown` → HTTP probe fallback guarantees this.
- **No wire/schema change** (`wire_contracts_changed: false`). The phase signal
  is `tracing`-only; `DaemonState` enum is unchanged.
- **Desktop crate is standalone Tauri** (`apps/desktop/src-tauri/`), not a root
  workspace member — `cargo` runs from that dir, never from repo root.
- **No new dependencies** — reuse `tokio::net::TcpStream` (already imported)
  and the shared `HEALTH_CLIENT` reqwest client.
- `probe_port_state` must be unit-testable without a real daemon (bind a
  listener for `Occupied`; rely on a high unused port for `Free`).
