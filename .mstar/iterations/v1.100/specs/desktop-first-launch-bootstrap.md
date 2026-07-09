# Desktop Clean-State First-Launch Bootstrap Contract

**Status:** Locked (V1.100 T1 — architect implementation-ready)
**Document class:** Iteration-scoped contract
**Coordinates with:** `.mstar/specs/desktop-shell.md` §13 (wizard), `.mstar/specs/web-ui.md`

## Problem

V1.97 made the desktop sidecar path reachable but exposed the next clean-state blocker: on a fresh local home, the Tauri `.setup()` hook starts the bundled daemon before the wizard has created creator/workspace state. The daemon then exits with `No active creator`, and the wizard has no creator-bootstrap step that can make the launch path complete.

Root cause (code trace):
1. `.setup()` in `apps/desktop/src-tauri/src/lib.rs` ~L529 unconditionally spawns the sidecar
2. Daemon boot calls `resolve_state_db_path` (`crates/nexus-daemon-runtime/src/config.rs:98`) which needs `active_creator_id` in `~/.nexus42/config.toml`
3. Clean-state has no `active_creator_id` → daemon exits "No active creator in ~/.nexus42/config.toml"
4. Wizard step 2 (Daemon) cannot start the daemon because bootstrap hasn't happened yet

## Locked Architecture

### Bootstrap mechanism: `ensure_setup_bootstrap` Tauri IPC command

**Name:** `ensure_setup_bootstrap`

**Transport:** Tauri IPC (custom command in `apps/desktop/src-tauri/src/lib.rs`), exposed via `DesktopCapabilities` (`apps/web/src/lib/nexus/desktop-capabilities.ts`).

**Implementation:** Direct Rust helper reuse is practical and preferred. The Tauri crate writes `active_creator_id` + `active_workspace_slug_by_creator` to `~/.nexus42/config.toml` using its existing `toml_edit` infrastructure (same pattern as `write_setup_completed_at`, `write_workspace_path_at`). Creator ID generation uses a new `uuid` dep (or replicates the 3-line hex-random routine from `nexus-creator/src/local_identity.rs:211-221`). No `nexus42` sidecar one-shot is needed for the config path.

The bundled `nexus42` one-shot (`nexus42 system identity create --persistent`) is the **documented fallback** only for the **identity DB** portion (`~/.nexus42/state.db` with `local_identities` table), which is **optional for daemon boot** but required for `system identity list` and the Settings identity panel. If T2 cannot pragmatically add `nexus-creator` / `nexus-local-db` as workspace path dependencies, the one-shot approach handles only the DB persistence portion; the config.toml writes always stay in Rust.

**Reuse targets (T2 reads these before implementation):**

| Reuse target | Location | Purpose |
|---|---|---|
| `nexus_config_path()` | `lib.rs:261` | Resolve `~/.nexus42/config.toml` |
| `write_setup_completed_at()` pattern | `lib.rs:287-305` | TOML round-trip edit preserving existing keys |
| `read_setup_completed_at()` pattern | `lib.rs:271-278` | Read a boolean key from config.toml |
| `set_workspace_path` command | `lib.rs:470-495` | Write `workspace_path` to config.toml (already called by wizard step 1) |
| `default_workspace_root()` | `lib.rs:33-47` | Cross-platform workspace default path |
| `SidecarManager::start()` | `sidecar.rs:205` | Auto-start (used by `.setup()` for existing-install) |
| `SidecarManager::start_daemon()` | `sidecar.rs:193` | Manual start (used by wizard `startDaemon` IPC) |
| `read_setup_completed()` | `lib.rs:266` | Read `setup_completed` marker for gating |
| `LocalIdentity::create_persistent()` | `crates/nexus-creator/src/local_identity.rs:100` | ID generation reference (replicate inline or add path dep) |

**Creator ID generation:** `ctr_local` + 12 random hex chars (matches `nexus-creator/src/local_identity.rs:214-221`). The Tauri crate adds `uuid = { version = "1", features = ["v4"] }` to `Cargo.toml` if the inline hex-random routine is preferred over a `nexus-creator` path dependency.

### Daemon-start timing matrix

| Scenario | `setup_completed` | `.setup()` auto-start sidecar? | Wizard step 2 (Daemon) calls `startDaemon`? | Bootstrap trigger |
|---|---|---|---|---|
| **Clean-state first launch** | `false` (absent) | **SKIP** — `.setup()` reads `setup_completed`, sees `false`, does NOT spawn sidecar | **YES** — wizard step 2 calls `desktop.startDaemon()` after bootstrap completes | After workspace persistence (step 1 Continue→step 2 transition) |
| **Existing install** | `true` | **AUTO-START** (preserved behavior) — `.setup()` spawns sidecar, attaches if already healthy | **N/A** — wizard is skipped entirely (per-launch daemon-ready gate instead) | N/A |
| **Re-run setup (first time after reset)** | `false` (cleared from settings) | **SKIP** — same as clean-state | **YES** — wizard step 2 calls `startDaemon()` | `ensure_setup_bootstrap` is **idempotent**: if `active_creator_id` already set, it returns `{ already_bootstrapped: true }` and skips generation |
| **Re-run setup (partial state: creator exists but workspace missing)** | `false` (cleared) | **SKIP** | **YES** | Bootstrap detects `active_creator_id` is set → skips ID generation, but still writes `workspace_path` |
| **`setup_completed=true` but daemon not running** | `true` | **AUTO-START** — `.setup()` spawns; if port conflict, surfaces error in per-launch gate | N/A | N/A |

### Threading contract: `.setup()` timing

```
.setup() hook (lib.rs):
  1. set_app_handle on SidecarManager (existing, unchanged)
  2. Read setup_completed from config.toml
  3. IF setup_completed = true:
       spawn async task: manager.start(app_handle)  ← existing behavior
     ELSE:
       no-op  ← clean-state; wizard owns daemon start
  4. Register all invoke handlers (existing, unchanged)
```

**Invariant:** `.setup()` must complete synchronously (per Tauri contract). The `setup_completed` read and conditional branch happen inline; the daemon spawn is already delegated to `tauri::async_runtime::spawn`. Clean-state adds no latency to `.setup()` — the no-op branch is instant.

### Bootstrap contract: minimum state for daemon to not exit `No active creator`

The daemon (`crates/nexus-daemon-runtime/src/workspace/mod.rs:150-166`) requires these keys in `~/.nexus42/config.toml`:

```toml
# Required for daemon boot (resolve_state_db_path)
active_creator_id = "ctr_localXXXXXXXXXXXX"

# Already set by wizard step 1 (set_workspace_path)
workspace_path = "/Users/.../Documents/nexus/default"

# Required for daemon boot (workspace_slug_for_creator — falls back to "default" if absent,
# but explicitly written for consistency with all test configs)
[active_workspace_slug_by_creator]
"ctr_localXXXXXXXXXXXX" = "default"
```

**What the daemon does NOT need at boot:**
- `~/.nexus42/state.db` (global identity DB) — daemon uses per-workspace `state.db` under `creators/<id>/workspaces/<slug>/`
- `~/.nexus42/creators/<id>/workspaces/default/state.db` — daemon auto-creates and migrates this on first boot
- `~/.nexus42/agent-host/config.toml` — wizard step 3 (agent) handles this

**What `ensure_setup_bootstrap` does (must not create):**
- Work/Intake artifacts — out of scope (the minimum bootstrap is identity + workspace, not a full Work)
- Cloud/platform accounts, ACP agent config, preset config
- Full creator profile (SOUL.md, KB, etc.) — daemon boot doesn't need them

### Wizard integration: bootstrap flow

**Step 1 (Welcome) → Step 2 (Daemon) transition:**

```
Wizard step 1 "Continue" handler (suite-step-welcome.tsx):
  1. If desktop: call desktop.setWorkspacePath(selectedPath)  ← existing, unchanged
  2. If desktop: call desktop.ensureSetupBootstrap()
     - Returns { creator_id, already_bootstrapped }
     - On failure: toast the error, do NOT advance to step 2
  3. Advance to step 2 (Daemon)
```

**Bootstrap idempotency contract:**
- If `active_creator_id` is already set in config.toml: return `{ creator_id, already_bootstrapped: true }` — no generation, no overwrite
- If not set: generate ID, write to config.toml, return `{ creator_id, already_bootstrapped: false }`
- Never overwrite an existing `active_creator_id` (re-run-safety)
- If `active_workspace_slug_by_creator` already has an entry for `creator_id`: preserve it; otherwise set to `"default"`

**Browser build:** `ensureSetupBootstrap` is a no-op on `DesktopCapabilities` — returns `undefined` / skips silently. Screens must handle `desktop === null` without error.

### No wire contracts

**Confirmed: `wire_contracts_changed: false`.** The bootstrap is Tauri IPC only:
- No new daemon HTTP endpoint
- No `schemas/` changes
- No `@42ch/nexus-contracts` version bump
- No `wire_contracts_changed` flag toggled

The bootstrap writes to the local filesystem through the Tauri Rust layer. The daemon reads the same `~/.nexus42/config.toml` it already reads at boot — no new API surface is introduced.

### Existing-install invariant

**`setup_completed=true` preserves current per-launch auto-start/attach behavior** (no regression). The `.setup()` hook's conditional gate only changes the `false` branch (which doesn't exist in existing installs). The `true` branch is byte-for-byte the existing `manager.start(&handle)` call. Existing tests for sidecar auto-start/attach must continue to pass.

## Wizard Flow Diagram

```
Clean-state first launch:
  .setup() → setup_completed=false → SKIP daemon auto-start
  ↓
  Wizard step 1 (Welcome) → select workspace → setWorkspacePath()
  ↓
  ensureSetupBootstrap() → generate ctr_local* → write config.toml
  ↓
  Wizard step 2 (Daemon) → startDaemon() → health probe → ready
  ↓
  Wizard step 3 (Agent) → scan → select agent → setAgentProfile()
  ↓
  Wizard step 4 (Done) → setSetupCompleted(true) → main UI

Existing install:
  .setup() → setup_completed=true → AUTO-START daemon (preserved)
  ↓
  Per-launch daemon-ready gate (not wizard) → health probe → main UI
```

## `DesktopCapabilities` TypeScript surface (additive)

```typescript
// apps/web/src/lib/nexus/desktop-capabilities.ts — new method on DesktopCapabilities

/** Bootstrap local creator/workspace state before daemon start.
 *  Idempotent: if a creator ID already exists, returns it without overwriting.
 *  Browser build: no-op (desktop === null, wizard skips this step). */
ensureSetupBootstrap(): Promise<{ creator_id: string; already_bootstrapped: boolean }>;
```

## Implementation Boundaries

In scope (same as draft):

- Tauri setup lifecycle gating (`lib.rs` `.setup()` hook).
- `ensure_setup_bootstrap` Tauri IPC command (`lib.rs`).
- `DesktopCapabilities` TypeScript surface addition.
- Setup wizard sequencing: step 1 Continue triggers bootstrap before advancing to step 2.
- Rust tests: three lifecycle branches (clean-state skip, existing auto-start, bootstrap idempotency).
- Web tests: wizard order (bootstrap before daemon step), browser-mode safety.

Out of scope (same as draft):

- Full Work/intake creation beyond minimum creator/workspace bootstrap.
- Cloud/platform account creation.
- New daemon API schemas.
- Signing, notarization, auto-update, tray, Windows/Linux release hardening.
- Broad config atomicity or default-path consolidation.

## Smoke Gate (unchanged from draft)

The plan Done gate requires interactive macOS evidence. Automated Rust/web tests and a desktop bundle build are necessary evidence, but they cannot substitute for the native `.app` smoke.

- Clean-state smoke: isolated or cleared local home, `.app` launch, wizard completion, daemon reaches running, main UI visible.
- Existing-install smoke: pre-seeded setup-completed config, relaunch skips wizard, daemon auto-start/attach still reaches the main UI.
- Native folder picker and recovery copy are observed or explicitly not exercised with rationale.
- Evidence must record the local-state setup used, the observed wizard stages, daemon status, and the final UI state for both clean-state and existing-install paths.

`R-V197-SMOKE-CLEAN-STATE` must remain open until clean-state smoke passes. `R-V197-SMOKE-UI` must remain open unless both smoke paths are observed or PM/user explicitly re-scope the residual with rationale.

## Verification Strategy

- **Rust desktop tests (T2):** Pin three lifecycle branches:
  1. `setup_completed=false` does NOT auto-start the sidecar from `.setup()`
  2. `setup_completed=true` preserves current auto-start/attach behavior (regression-proof)
  3. `ensure_setup_bootstrap` is idempotent (second call returns `already_bootstrapped: true`)
- **Web setup tests (T3):** Pin the wizard order: workspace persistence succeeds, bootstrap runs before the daemon step, browser mode remains safe when desktop capabilities are unavailable.
- **Contract verification:** Local to Tauri IPC and React capability typing. `wire_contracts_changed` remains `false`; any proposal to touch `schemas/` blocks implementation and returns to architecture review.
- **Smoke (T4):** Interactive macOS evidence for clean-state and existing-install paths (required before plan Done).
