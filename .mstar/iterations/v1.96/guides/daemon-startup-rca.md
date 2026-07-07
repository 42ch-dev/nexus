# Daemon Startup RCA Strategy — V1.96

**Document class**: iteration workspace guide (`.mstar/iterations/v1.96/guides/`).
**Promoted to**: `knowledge/architecture-patterns/daemon-ready-gate-pattern.md` § "V1.96 refinements" (Rules 5-8 + anti-patterns) at V1.96 iteration-close. This file is kept as an iteration-scoped snapshot with full RCA details (code sketches, line citations); durable pattern guidance lives in the knowledge doc.

**Authority**: this guide records confirmed root causes and the stderr-capture
implementation strategy. The implement plan's T3 and T4 tasks are the execution
authority; the compass §1 locked decisions are the binding product contract.

---

## 1. Three confirmed wizard-side root causes

### Bug 1: React subscription race — no `'starting'` branch (P0)

**Source**: `apps/web/src/pages/setup-step-daemon.tsx` lines 40–48.

The `onDaemonStatusChanged` callback only branches on `'running'`, `'error'`,
and `'stopped'`:

```ts
unsub = await desktop.onDaemonStatusChanged((status) => {
  if (cancelled) return;
  if (status.state === 'running') {
    setReady(true);
    setError(null);
  } else if (status.state === 'error' || status.state === 'stopped') {
    setError(status.detail ?? `Daemon is ${status.state}.`);
  }
  // No branch for 'starting' → callback is a no-op for the first event
});
```

**Why it matters**: The daemon auto-starts at Tauri boot (`lib.rs:529-538`):
```rust
tauri::async_runtime::spawn(async move {
    if let Err(e) = manager.start(&handle).await {
        eprintln!("nexus-desktop: sidecar failed to start: {e}");
    }
});
```

The `SidecarManager` enters `DaemonState::Starting` immediately after
`command.spawn()` succeeds (see `sidecar.rs` monitor-entry path around line 200).
This happens **before** the SPA's `SetupWizardPage` component mounts and
subscribes. When the SPA does subscribe, the first status event it receives is
`state: 'starting'`. The callback does nothing → `setReady(false)` + no
`setError(...)` → the component stays in the "Starting daemon…" transient UI
forever.

**Additional race**: on a clean `~/.nexus42/` first launch, the daemon exits
within milliseconds (see §2 below). `wait_for_first_health` returns `None`,
`start_with_budget` transitions to `Error` and calls `notify()`. However, if
the SPA has not yet subscribed, the Error event fires into the void. The SPA
then subscribes and receives… nothing (the event already fired; no current-state
probe on mount). This is the second half of the race: the SPA must call
`getDaemonStatus()` on mount to check the current state, not rely solely on the
event listener for initialization.

### Bug 2: Stderr discarded — `_rx` dropped (closes R-V195-ARCH-STRERR-GAP)

**Source**: `apps/desktop/src-tauri/src/sidecar.rs` line 246.

```rust
let (_rx, child) = command
    .spawn()
    .map_err(|e| format!("failed to spawn sidecar: {e}"))?;
```

The `_rx` prefix discards the `CommandEvent` receiver returned by
`tauri_plugin_shell::CommandChild::spawn()`. This receiver is the only path to
sidecar stderr/stdout data. Without it, the `DaemonStatus.detail` field on the
Error transition (line 286) can only carry generic SidecarManager strings:

```rust
// sidecar.rs lines 280-288 (Error transition)
let message = if conflict {
    format!("Nexus couldn't start… port {port} is already in use…")
} else {
    "Daemon did not start. Check the logs or try restarting.".to_string()
};
inner.state = DaemonState::Error;
inner.detail = Some(message.clone());
```

Even when the daemon produces a real error on stderr (e.g. "No active creator
in ~/.nexus42/config.toml…"), the SPA never sees it — only the generic message.

### Bug 3: No wizard-side timeout (closes R-V195QC3-S001)

**Source**: `apps/web/src/pages/setup-step-daemon.tsx` lines 19–61 (`useEffect`).

The effect subscribes via `desktop.onDaemonStatusChanged` and cleans up on
unmount (line 58-60). There is **no `setTimeout`** that fires if no terminal
status (`'running'` / `'error'` / `'stopped'`) arrives. If the sidecar never
emits a terminal event (e.g. daemon process hangs, or the only `notify()` call
happened before SPA subscription), the wizard waits indefinitely.

The `useEffect` cleanup only sets `cancelled = true` and calls `unsub?.()` —
there is no hard deadline.

---

## 2. First-launch daemon boot path: crash, not hang

### Analysis

**Daemon initialization path** (clean `~/.nexus42/`):

1. `WorkspaceState::initialize()` is called at daemon boot.
   — `crates/nexus-daemon-runtime/src/workspace/mod.rs` line 166.
2. It calls `crate::config::resolve_state_db_path(&user_home, &nexus_home)?`.
   — `crates/nexus-daemon-runtime/src/config.rs` lines 96–107.
3. `resolve_state_db_path` loads `CliConfigSnapshot` from
   `~/.nexus42/config.toml`. On a clean wipe, the file does not exist →
   `CliConfigSnapshot::load()` returns `CliConfigSnapshot::default()` —
   an empty struct with `active_creator_id: None`.
4. Line 98 checks `cfg.active_creator_id.as_deref().ok_or_else(|| …)` →
   returns `Err("No active creator in ~/.nexus42/config.toml. …")`.
5. The `?` at `workspace/mod.rs:166` propagates this error →
   `WorkspaceState::initialize()` returns `Err`.
6. The daemon's `main()` or equivalent startup code receives `Err` and exits
   the process (daemon binary terminates).

**Result**: the daemon **crashes immediately** (within milliseconds), NOT hangs.
The process exits, `wait_for_first_health` detects the exit via
`process_alive(pid)` polling, returns `None`, and `start_with_budget` transitions
to `Error`.

### Conclusion: no separate daemon-side fix needed

The daemon correctly crashes on a clean `~/.nexus42/` first launch — the error is
expected (no setup wizard has run yet to create a config and creator). The root
cause of the user-visible "hangs forever" symptom is **purely wizard-side**:

1. The SPA misses the Error event (Bug 1 — race).
2. Even if the Error event arrived, the message is generic (Bug 2 — stderr
   discarded).
3. Even if neither, there is no timeout escape (Bug 3 — no timeout).

**Do NOT add a daemon-side "skip WorkspaceState::initialize() if no active
creator" branch.** The daemon correctly refuses to start without a fully
configured environment. The wizard's job is to guide the user through creating
that environment (workspace selection, daemon start → creator init), and then
the daemon will start successfully. A daemon that starts silently without a
creator would have undefined behavior for every API route that calls
`read_active_creator_id()`.

**Verification**: after the wizard completes (creator created + config.toml
written via `setWorkspacePath`), the daemon's `start_daemon` retry will find
`active_creator_id` present, `resolve_state_db_path` will succeed, and the
daemon will start normally.

---

## 3. Stderr-capture implementation strategy

### 3.1 Bounded async task draining `_rx`

Replace `sidecar.rs:246`:

```rust
// BEFORE (bug):
let (_rx, child) = command
    .spawn()
    .map_err(|e| format!("failed to spawn sidecar: {e}"))?;

// AFTER (fix):
let (rx, child) = command
    .spawn()
    .map_err(|e| format!("failed to spawn sidecar: {e}"))?;
```

Add a new field to `SidecarInner` (the struct behind `Arc<Mutex<…>>`):

```rust
last_stderr_tail: Option<String>,
```

Spawn a bounded async task that drains the `rx` event receiver concurrently
with `wait_for_first_health`:

```rust
let stderr_tail = Arc::new(Mutex::new(String::new()));
let stderr_tail_clone = stderr_tail.clone();
tauri::async_runtime::spawn(async move {
    let mut buf = String::new();
    while let Some(event) = rx.recv().await {
        if let tauri_plugin_shell::process::CommandEvent::Stderr(line) = event {
            buf.push_str(&line);
            buf.push('\n');
            // Cap at 2 KiB tail: keep the last N bytes.
            if buf.len() > 2048 {
                let keep_start = buf.len().saturating_sub(2048);
                // Find the nearest newline boundary to avoid truncating mid-line.
                if let Some(nl) = buf[keep_start..].find('\n') {
                    buf = buf[keep_start + nl + 1..].to_string();
                } else {
                    buf = buf[keep_start..].to_string();
                }
            }
        }
    }
    let mut lock = stderr_tail_clone.lock().await;
    *lock = if buf.is_empty() { String::new() } else { buf };
});
```

On the Error transition (line 273-293), append stderr to `detail`:

```rust
// After the existing message construction, before inner.detail = Some(…)
let stderr = stderr_tail.lock().await;
let message = if !stderr.is_empty() {
    format!("{message}\n\nDaemon output:\n{}", stderr.trim())
} else {
    message
};
inner.detail = Some(message);
```

### 3.2 Design invariants

- **Non-blocking spawn**: the stderr-reading task runs concurrently with
  `wait_for_first_health`. It does not block the spawn path.
- **Bounded buffering**: cap at 2 kiB tail (kept at nearest newline boundary to
  avoid mid-line truncation). No unbounded `String::push_str` accumulation.
- **Fallback preservation**: when stderr is empty (daemon didn't print anything),
  the existing generic SidecarManager messages are used verbatim.
- **No schema change**: `DaemonStatus.detail: Option<String>` carries the
  combined message unchanged. No new fields on the DTO.
- **Concurrent safety**: `Arc<Mutex<String>>` for stderr tail. The drain task
  writes, the Error path reads (after `wait_for_first_health` has returned, so
  the drain task may still be running — the read races with the final write;
  this is acceptable: we'll either get the full tail or a slightly truncated
  one, and on next retry the full tail will be available).

### 3.3 Test plan (T8)

Per the implement plan, sidecar.rs tests should cover:
- Stderr tail cap at 2 kiB (inject lines exceeding the cap, verify only last
  ~2 kiB survive).
- Append formatting includes `"Daemon output:"` header when stderr is non-empty.
- Generic fallback message preserved when stderr is empty.
- Use the existing test pattern in `sidecar.rs:526+`.

---

## 4. Whether a separate daemon-side fix is needed

**Answer: No.** The daemon-side first-launch behavior is correct: the daemon
refuses to start without an active creator, as it should. The root cause of all
three user-visible symptoms is wizard-side (subscription race, stderr discarded,
no timeout). Fixing the wizard-side issues fully resolves the P0 blocker.

**No T9 addition needed** — the implement plan's existing 8 tasks (T1–T8) cover
all required changes. The "Architecture residual" section in the plan (line
277-283) anticipated this outcome: if the daemon crashes and the wizard just
misses the event, no separate daemon-side fix is needed.

---

## 5. Verification plan (manual smoke for the implementer)

After implementing T3 + T4:

```bash
# 1. Wipe the Nexus home directory completely.
rm -rf ~/.nexus42/

# 2. Build the desktop app.
cd apps/desktop
pnpm tauri dev

# 3. Wizard Step 1 appears (centered in viewport, inline Browse row).
#    Select a workspace → click Continue. No [object Object] on errors.

# 4. Wizard Step 2 shows "Starting daemon…".
#    Expected: within ≤30s, one of:
#    a) "Daemon is running." → Continue enabled (if daemon started)
#    b) Error message containing "Daemon output:" section with verbatim
#       stderr (e.g. "No active creator in ~/.nexus42/config.toml.…")
#    c) "Taking longer than expected" with Retry/Reset buttons
```

**Verification criterion**: the wizard NEVER stays in "Starting daemon…" for
more than 30s without surfacing one of the three terminal states above.

---

## References

- `apps/desktop/src-tauri/src/sidecar.rs` lines 234–293 — sidecar spawn + Error
  transition.
- `apps/desktop/src-tauri/src/lib.rs` lines 529–538 — Tauri boot auto-start.
- `apps/web/src/pages/setup-step-daemon.tsx` lines 19–61 — subscription
  effect (missing 'starting' + timeout).
- `apps/web/src/pages/setup-step-daemon.tsx` lines 85–90 — inline
  `errorMessage()` helper (to be extracted to shared module T1).
- `crates/nexus-daemon-runtime/src/config.rs` lines 96–107 —
  `resolve_state_db_path` with `active_creator_id` precondition.
- `crates/nexus-daemon-runtime/src/workspace/mod.rs` line 166 — `?` on
  `resolve_state_db_path` in `WorkspaceState::initialize()`.
- `crates/nexus-home-layout/src/lib.rs` lines 34–36 — `workspace_state_db_path`.
- V1.96 compass §1 Decision "Daemon diagnostic chain" + "Stderr capture approach".
- V1.96 implement plan T3 + T4.
