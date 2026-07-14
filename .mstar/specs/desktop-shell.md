# Desktop Shell (Tauri) — Specification v1

**Status**: Shipped (V1.66) — Tauri Desktop Shell delivered (QC tri-review Approve after fix-wave-1 + QA Pass)
**Document class**: Feature line
**Created**: 2026-06-25 (Phase 2b, `@architect`)
**Scope**: Nexus desktop shell contract — `apps/desktop` Tauri v2 wrapper, SPA adapter selection (`TauriClient`), desktop-only `NexusClient` extensions, native file actions + path guard, bundled `nexus42` sidecar lifecycle, port discovery, capability detection, macOS-first unsigned dev build. V1.67+ deferrals (signing, multi-OS, auto-update, in-process lib link, body editor) recorded in §2.
**Iteration compass**: [v1.66-tauri-desktop-shell-delivery-compass-v1.md](../iterations/v1.66-tauri-desktop-shell-delivery-compass-v1.md) (scope/roadmap SSOT — §0 grill decisions, §1.1 Track A, §5 locked design items)

**Coordinates with**:

- [web-ui.md](web-ui.md) §14 (Desktop Shell stage — product UX + user stories + capability table delta)
- [web-ui-design-requirements.md](web-ui-design-requirements.md) §6 (desktop shell surface design requirements)
- [daemon-runtime.md](daemon-runtime.md) §12 (Tauri sidecar mode — daemon-side launch/readiness/lifecycle)
- [daemon-api-surface-conventions.md](daemon-api-surface-conventions.md) §9 (local daemon port discovery; `local-api-surface-conventions.md` is a V1.90 redirect stub)
- [agent-nexus-tool-bridge.md](agent-nexus-tool-bridge.md) / `host_tool_handlers.rs` (W-002 path-guard reference for `openWith`/`revealInFinder` scope)
- [repo-root `DESIGN.md`](../../DESIGN.md) + [`DESIGN.dark.md`](../../DESIGN.dark.md) — Desktop Shell Supplement (window/menu/dialog/context-menu/status tokens) *(V1.98: sole SSOT; former `apps/web/DESIGN*.md` retired)*
- [schemas-external-consumer-boundary.md](../knowledge/schemas-external-consumer-boundary.md) — `wire_contracts_changed: false` (V1.66); desktop-native methods are Tauri IPC, not Daemon API wire. **V1.94:** `wire_contracts_changed: true` (additive `POST /v1/daemon/agent-host/scan` schemas; `@42ch/nexus-contracts` 0.20.0 → 0.21.0).
- [daemon-runtime.md](daemon-runtime.md) — health-probe plumbing reused for per-launch daemon-ready gate; `setup_completed` field additive to `~/.nexus42/config.toml`
- [web-ui.md](web-ui.md) — sidebar IA (two-tab + nested nav + footer), daemon status bar simplification, Strategies unification, button contrast invariant

---

## 1. Purpose

Defines the V1.66 desktop shell boundary: a Tauri v2 wrapper (`apps/desktop`) around the unchanged-transport `apps/web` SPA, the `TauriClient` impl of `NexusClient`, desktop-only capability extensions, native file actions with workspace-root path guard, and the bundled `nexus42` sidecar lifecycle. The shell is a **packaging/delivery layer** — it reuses the V1.64/V1.65 HTTP transport and wire contracts unchanged; it adds only what the browser sandbox cannot do.

## 2. Non-goals (durable V1.67+ roadmap)

Recorded so deferrals are tracked, not lost:

- Body full-text editor + per-chapter edit lock (V1.67 lead authoring slice).
- UI productivity wave (drag-reorder, bulk ops, reconcile trigger, outline templates).
- Windows + Linux desktop builds; code signing + Apple notarization + Windows Authenticode; GitHub Releases + auto-update; in-process `nexus-daemon-runtime` lib link. (Signing/distribution v2 may split to its own iteration if the V1.67 body editor consumes V1.67 capacity.)
- System tray / menu-bar app / global hotkeys / native notifications; custom title bar / animated transitions (Production polish). **Menu-bar daemon status + stop/start control** tracked as [DF-71](../knowledge/deferred-features-cross-version-tracker.md) (interim quit dialog shipped on the agent-detection hotfix; tray remains opportunistic polish).
- Mobile (Tauri v2 mobile targets).

## 3. Application structure

- `apps/desktop` is a **pnpm workspace sibling** of `apps/web` (`pnpm-workspace.yaml` already admits `apps/*`); shares the lockfile + `@42ch/nexus-contracts` via workspace dep.
- `apps/desktop/src-tauri/` is a **standalone Tauri-managed Rust crate**, NOT a root Cargo workspace member (Tauri convention; avoids coupling the daemon workspace to Tauri's build).
- `tauri.conf.json`: `productName`, macOS bundle id, window config, `build.frontendDist` (= bundled `apps/web/dist`), `bundle.externalBin` (the sidecar), capability permissions (`shell:allow-execute` with `sidecar: true`; opener scope).

## 4. Web asset loading

**`build.frontendDist` serves the bundled `apps/web/dist` directly** (Tauri v2 key, under `build`). The daemon's rust-embed static-asset route (V1.64) remains normative for the browser-tab flow and standalone `nexus42 daemon ui`; it is **not** the desktop shell's asset-serving path. No static fallback to the daemon inside desktop mode.

## 5. NexusClient desktop contract

**`TauriClient`** (replaces the V1.65 stub at `apps/web/src/lib/nexus/tauri-client.ts`) implements the **21-method `NexusClient` interface** (health + 20 data methods) as **thin desktop-augmentation over `BrowserClient`**: the 21 data methods reuse the identical HTTP transport to `http://127.0.0.1:<resolvedPort>/v1/local/*`. **Not** a full Tauri-plugin IPC rewrite.

Desktop-only capability extensions (browser sandbox cannot perform these) are added as a separate `DesktopNexusClient extends NexusClient` (or equivalent capability object), exposed **only in desktop mode**:

| Method | Transport | Notes |
| --- | --- | --- |
| `openWith(path)` | Tauri custom command → `plugin-opener.openPath()` | Runtime path-guarded (§9). |
| `revealInFinder(path)` | Tauri custom command → `plugin-opener.revealItemInDir()` | Runtime path-guarded (§9). |
| `getDaemonStatus()` | Tauri `plugin-shell` / sidecar IPC | Returns health + port; drives status indicator. |
| `startDaemon()` / `stopDaemon()` | Tauri `plugin-shell` Sidecar | Lifecycle control; autostart on app launch is default. |

`copyPath(path)` is unchanged from V1.65 (clipboard write; browser + desktop).

## 6. Capability detection

**Primary signal**: injected build/runtime flag (`NEXUS_DESKTOP`). **Sanity check**: Tauri API presence — if relying on `window.__TAURI__`, set `app.withGlobalTauri: true` explicitly; otherwise prefer `@tauri-apps/api/core`'s `isTauri`. **Checked once at the `NexusClient` factory** (not scattered across screens). Browser build selects `BrowserClient`; desktop build selects `TauriClient` (+ desktop capability object).

## 7. Sidecar lifecycle

Owned by the Tauri app while the desktop session is alive. **Daemon-side detail in [daemon-runtime.md](daemon-runtime.md) §4.6.** Summary:

- **Launch**: `nexus42 daemon start --foreground --port <resolved>` via `Command.sidecar(...)` from `@tauri-apps/plugin-shell` on app start (unless a healthy daemon already responds on the resolved port — then attach).
- **Readiness**: `GET /v1/local/runtime/health` returns healthy (NOT stdout parsing). Bounded retry/backoff; render `Daemon starting…` until healthy.
- **Crash after healthy**: restart with bounded exponential backoff; on repeated crash, stop retrying + show `Daemon stopped` + diagnostics.
- **App quit**: request graceful termination of the owned sidecar; escalate after bounded timeout. Do NOT kill an unrelated user-started daemon without confirming ownership (track the process handle from the Sidecar API; PID-file/port stop is a CLI-compat mechanism only).
- **Manual restart**: from the daemon-status indicator, stop owned sidecar → spawn fresh → wait for health.

## 8. Port discovery

**Default `8420` + `NEXUS_DAEMON_PORT` override + health probe.** Resolution: explicit configured port → `NEXUS_DAEMON_PORT` (if valid) → `8420`. App passes `--port <resolved>` so CLI args + env cannot diverge. Readiness = health probe (§7). No dynamic port handshake in V1.66. Conventions codified in [local-api-surface-conventions.md](local-api-surface-conventions.md) §9.

## 9. Native file actions + path guard

`Open With…` (system MD-editor picker) + `Reveal in Finder` on chapter body/outline paths. **Path guard (security-critical):**

- **Runtime canonicalize + prefix-check against the active workspace root is AUTHORITATIVE** — mirrors the W-002 guard intent from `host_tool_handlers.rs`.
- **Tauri capability/opener scope is defense-in-depth ONLY** — Tauri permissions are *static* capability scopes and **cannot** encode a *dynamic* active workspace root.
- **Prefer custom Tauri commands** (`open_with`, `reveal_in_finder`) that validate the path (canonicalize + prefix-check) **before** calling opener functionality — over relying solely on the static opener scope.
- On rejection: plain-language disabled state (`Path not opened. The file is outside the active workspace.`), not a silent no-op.
- **Coordinate with P-sec `R-V165-QC-SUGG-DEFENSE`** — `host_tool_handlers.rs` body-write path gets parity hardening in V1.66; the openWith guard shares the canonicalize+prefix-check pattern.

Browser build: "Copy Path" only (no greyed-out teasing of unavailable actions).

## 10. Design requirements

Window chrome / app menu / native dialogs / desktop context menu / daemon-status indicator tokens in repo-root [`DESIGN.md`](../../DESIGN.md) **Desktop Shell Supplement (V1.66 Standard+)**. Product intent + constraints in [web-ui-design-requirements.md](web-ui-design-requirements.md) §6. System tray: none in V1.66.

## 11. Build + CI

- **macOS-only** in V1.66 (`aarch64-apple-darwin` + `x86_64-apple-darwin`).
- **Unsigned** `.app` + `.dmg` (T1 DoD). No signing, no notarization, no auto-update, no GitHub Releases.
- CI `desktop-build` job: `macos-14` aarch64 runner, both Rust targets installed, `--target universal-apple-darwin` if stable (else separate arch artifacts; no hand-rolled `lipo` first), 90-day retention, path filter (`apps/web/**`, `apps/desktop/**`, `apps/nexus42/**`, `packages/nexus-contracts/**`, `crates/**`, lockfiles, workflows).

## 12. Verification matrix

| Check | Scope |
| --- | --- |
| `pnpm --filter desktop tauri build` | Unsigned `.app`/`.dmg` produces on clean macOS checkout |
| `cargo check` in `apps/desktop/src-tauri` | Tauri Rust crate compiles (standalone, not workspace) |
| `TauriClient` transport parity | 21 data methods mirror `BrowserClient` HTTP paths (test mocks `__TAURI__`) |
| Capability detection | Factory selects correct client in browser vs desktop mode |
| Path guard | Rejection of paths outside workspace root (test coverage) |
| Sidecar lifecycle | Autostart on launch; health probe; restart-on-crash; stop-on-quit |
| Q5 actions | Open With / Reveal in Finder / Copy Path work in desktop mode; browser = Copy Path only |
| Daemon-status indicator | States surface (starting/healthy/degraded/stopped/error) with text + recovery |

---

---

## 13. Setup Wizard (V1.94)

**Status**: Draft (V1.94) — normative contract frozen by P-1; implement authority P0 + P1.
**Iteration compass**: [v1.94-desktop-onboarding-ia-pass-delivery-compass-v1.md](../iterations/v1.94-desktop-onboarding-ia-pass-delivery-compass-v1.md) §1 (locked decisions A2+B1, C1, H1) + §5 (acceptance criteria).

### 13.1 Purpose

Desktop first-launch is a **two-phase entry** (V1.105):

1. **Launch ritual** — fullscreen `DaemonLaunchGate` until the bundled sidecar is Ready (every launch; not a wizard step).
2. **Setup wizard** — after Ready, three author-facing steps only: **Agent → Workspace → Done** (see §13.10.3).

The `setup_completed` marker still gates main UI vs `/setup` **after** Ready — absent or `false` → wizard; `true` → main UI.

> **Current product authority:** §13.10. Sections §13.2–§13.9 record historical V1.94–V1.100 behavior for traceability.

### 13.2 Four-step flow (historical — superseded by §13.10)

| Step | Title | Action | UX states |
|------|-------|--------|-----------|
| 1 | Welcome + Workspace | Resolve default workspace (`~/Documents/nexus/default/` via `dirs::document_dir()`); create directory if absent. Existing `~/.nexus42/config.toml` values are preserved — no forced migration unless stale pattern. | Path display + native directory picker ("Browse…") + "Use default" affordance. |
| 2 | Daemon Ready | Start the bundled `nexus42` sidecar; poll `GET /v1/daemon/runtime/health` until healthy. Reuses the existing `HEALTH_START_TIMEOUT` (15s) + `SidecarManager` lifecycle from §7. | "Starting daemon…" transient → "Daemon ready" (success) OR error state distinguishing timeout vs port conflict vs crash. Never a silent hang. |

> **V1.96 update**: the wizard-side daemon-wait logic is now subscription-based (not polling) with a mount-time state probe, explicit `'starting'` branch, and a 25s hard timeout. See §13.7.5 for the current behavior. The polling/15s description above is historical (V1.94 original).
| 3 | ACP Agent Detection | Call `POST /v1/daemon/agent-host/scan`; display registry entries annotated with PATH-install status. Default recommendation = first `installed: true` entry with "Recommended" badge. | Scanning transient → agent list with selectable cards (name, version, installed badge, "Recommended" badge) → "No agents found" state with custom `launch_command` input + "Continue with custom" CTA. |
| 4 | Done | Persist the selected agent + `setup_completed = true` in `~/.nexus42/config.toml`; transition to main UI. | Confirmation screen; "Finish" CTA launches main UI. |

### 13.3 `setup_completed` marker

- **Location**: `~/.nexus42/config.toml` field `setup_completed: bool`.
- **Semantics**: absent or `false` = first-launch (wizard at `/setup` after `DaemonLaunchGate` Ready); `true` = skip wizard, enter main UI after Ready (§13.10.2).
- **Additive**: the field is optional; existing config files without it are treated as absent (= first-launch). TOML deserialiser must use `#[serde(default)]` or equivalent — the field must tolerate unknown config shapes.
- **Persistence**: the Tauri shell writes `setup_completed = true` on wizard completion via the existing `set_setup_completed` command (P0). The CLI config path (`apps/nexus42/src/config.rs`) accepts the field additively.
- **Reset**: Settings → **Setup** exposes **Re-run Setup**, which clears the `setup_completed` marker (R1). Missing marker = fail-safe to wizard. **V1.103 implement authority:** [settings-setup-section.md](../iterations/v1.103/specs/settings-setup-section.md).

### 13.4 Per-launch daemon-ready gate (historical pre-V1.105 — see §13.10.2)

> **V1.105 supersedes this section.** Current product: outer **`DaemonLaunchGate`** — fullscreen splash on **every** desktop launch (first-launch and return visits), always preceded by unconditional sidecar auto-start (D2). The wizard Daemon step is retired. **Normative:** §13.10.2.

The text below describes pre-V1.105 behavior retained for traceability.

Every app launch — not only first launch — gates entry to the main UI on a healthy daemon probe:

- `setup_completed === true` → show a brief "Starting daemon…" splash (full-screen, minimal, not the main UI shell).
- Poll `GET /v1/daemon/runtime/health` (reuses `wait_for_first_health` in `sidecar.rs`).
- On first successful probe → transition to main UI.
- On failure after `HEALTH_START_TIMEOUT` (15s) → error surface that distinguishes timeout, port conflict (port 8420 already in use), and daemon crash. The surface must:
  - (i) Show clear copy distinguishing the failure mode.
  - (ii) Offer an actionable next step (Restart CTA or "Kill conflicting process" hint).
  - (iii) Never silently hang or show an enabled-while-broken Start button.
- The wizard step 2 and the per-launch gate are two consumers of the same health-probe state machine; failure paths are plumbed by P0 (gate state + signals); visual copy is P1.

### 13.5 Default workspace

- **Path**: `~/Documents/nexus/default/` (cross-platform via `dirs::document_dir()`).
- **Fallback**: if `dirs::document_dir()` returns `None`, fall back to `dirs::home_dir().join("Documents").join("nexus").join("default")` and log a warning.
- **Resolution contract**: both `apps/nexus42/src/config.rs` (CLI/daemon-side) and `apps/desktop/src-tauri/src/lib.rs` (Tauri shell) MUST agree — the workspace-default resolver is shared by both.
- **Existing installs**: `workspace_path` already set in `~/.nexus42/config.toml` is preserved verbatim unless it matches known stale patterns (`nexus42/default` or `nexus/local/default`), in which case it's overwritten with the new default. The default applies **only when `workspace_path` is unset** or matches a stale pattern.

### 13.6 V1.95 Amendments

#### 13.6.1 Setup wizard layout redesign (V1.95 shipped behavior)

The setup wizard moves from a centered card with horizontal steps at the top to a left‑sidebar vertical step indicator with content on the right (V1.95 delivery):

- Steps: Welcome (workspace selection), Daemon (status/error/reset), Agent (detection/selection), Done.
- The wizard fills the entire window (no `min-h-screen items-center justify-center`).
- Step indicators are a vertical list in a fixed left panel (`w-52`), with the current step highlighted.
- Content area keeps the card chrome (border, shadow, background).

**Note**: V1.96 reworks this to a centered, integrated single-card IA (see §13.7). The V1.95 description is retained for historical traceability only.

#### 13.6.2 Setup wizard workspace selection with native directory picker

Step 1 (Welcome) now includes a native directory picker (Tauri `@tauri-apps/plugin-dialog` `open({ directory: true })`) to let the user select a custom workspace path:

- Default workspace path: `~/Documents/nexus/default` (brand `nexus/`, not `nexus42/`; system home remains `~/.nexus42/`).
- Stale path overwrite: if the existing `workspace_path` matches `~/Documents/nexus42/default` or `~/Documents/nexus/local/default`, it is overwritten with the new default; custom user‑set paths are preserved.
- Browser build hides the directory picker button (no native dialogs).

#### 13.6.3 FingerprintGate setup route bypass

The `FingerprintGate` adds `/setup` to its bypass routes (alongside `/connect`), so the wizard can render before any remote config exists without timing risks.

#### 13.6.4 ClientProvider immediate TauriClient for desktop

On desktop builds, `ClientProvider` returns `TauriClient` + `TauriDesktopCapabilities` immediately in the `!loaded` branch (no temporary `BrowserClient`), avoiding the "Request failed: The string did not match the expected pattern" error from same‑origin `/v1/daemon/runtime/health` calls in the Tauri webview.

#### 13.6.5 Daemon error surfacing + migration‑mismatch recovery

- Wizard step 2 (Daemon) surfaces the real error detail from `SidecarManager` (not a generic message).
- When the daemon fails to start (e.g., migration checksum mismatch), the wizard offers an **opt‑in "Reset local database" button** that clears the daemon state in `~/.nexus42/` (no user creative files touched) and retries daemon start.
- The button copy clearly states: "This will clear the daemon's local state database (config, registry cache). Your creative files in the workspace are not affected."

### 13.7 V1.96 Amendments — Setup Wizard Surface rework & daemon diagnostic chain

> **Supersession (V1.105):** §13.10.3 three-step IA; portrait top Steps (§13.10.5 / `portrait-wizard-shell.md`); daemon diagnostics on `DaemonLaunchGate` splash — not wizard step 2. Toast/CTA/Browse patterns below remain valid where §13.10 references them.

**Product behavior target (author-visible).** These describe what the user sees and does after the V1.96 changes. Technical token names, React implementation, and Rust sidecar mechanics are out of scope for this spec (see DESIGN.md and the implement plan).

#### 13.7.1 Centered, integrated card layout

- The entire wizard is centered in the viewport (both horizontally and vertically) rather than left-aligned or window-filling without centering.
- The step indicator list and the current step's content area live inside **one shared card chrome** (single container element with border, shadow, and background). The step list and content are not two disconnected panels.
- In the step indicator, the circle (number or completion marker) and the step label text align on the same horizontal baseline within each row (no vertical offset between circle and label).

#### 13.7.2 Inline workspace location row (Step 1)

- The workspace location is presented as a single inline affordance:
  - Folder icon + "Workspace location" label + current resolved path + "Browse…" button appear grouped on one row (or two tightly coupled rows inside the same visual block).
- The Browse button is visually adjacent to the path text (strong association between location display and the action that changes it).
- Browser builds continue to hide the native picker button.

#### 13.7.3 Global unified toast + shared error helper (all steps)

- Actionable errors originating from Tauri invokes (`pickDirectory`, `setWorkspacePath`, daemon status, finish, etc.) are **never** shown as inline `<p role="alert">` text inside a step.
- All such errors route through the page-level `useToast()` (variant "error").
- A shared `errorMessage(err: unknown)` helper (used by every wizard step) correctly turns Tauri error objects (`{ message: "..." }`), native `Error` instances, and plain strings into a human string. The literal text `[object Object]` never appears for these failures.
- The daemon step's prior inline error logic is updated for consistency with the global toast pattern.

#### 13.7.4 Primary bottom CTA pattern (all steps)

- Navigation controls sit at the bottom of each step's content area.
- The primary action ("Continue", "Finish", or equivalent) is a wide, prominent button that spans most or all of the available width inside the card (or a constrained max-width per the surface rules).
- The secondary "Back" action is a smaller tertiary/secondary button placed adjacent to the primary (typically left of it or in a compact pair).
- The pattern is applied consistently to Steps 1–4.

#### 13.7.5 Daemon diagnostic UX (Step 2)

- The wizard **never** hangs indefinitely in the "Starting daemon…" transient after the SPA has subscribed.
- The subscription callback explicitly branches on `state === 'starting'` (treated as progress; the default transient UI remains appropriate).
- A hard bounded timeout applies (≤30 s from step entry or from the moment subscription is established). If no terminal state (`running` / `error` / `stopped`) has arrived by then, the UI surfaces a "Taking longer than expected" state that exposes visible Retry and Reset actions.
- When the daemon reaches `error`, the surfaced `detail` contains the **verbatim stderr** captured from the sidecar (clearly prefixed or appended so the real output — e.g. migration failure, missing config, port conflict — is visible to the author). Generic SidecarManager strings are only a fallback when no stderr was captured.
- V1.95 fixes (ClientProvider immediate TauriClient, opt-in Reset local database, workspace-path stale-pattern handling) remain in effect and are not regressed.

##### 13.7.5.1 Technical invariant — mount-time state probe (React lifecycle contract)

The `SetupStepDaemon` `useEffect` MUST call `desktop.getDaemonStatus()` on mount **before** subscribing via `desktop.onDaemonStatusChanged`. On a clean first launch the daemon exits within milliseconds; the SidecarManager transitions to Error and fires `notify()` before the SPA subscribes. Without an initial probe, the SPA never learns about the Error and the timeout is the only escape. The mount-time probe catches the "event already fired" scenario without waiting 25s.

The `useEffect` cleanup MUST:
- Set a `cancelled` flag to prevent state updates after unmount.
- Call `unsub?.()` to tear down the daemon status listener.
- Call `clearTimeout(timer)` to cancel any in-flight hard timeout.

These are React lifecycle invariants, not product-behavior requirements; they are recorded here so the implement plan and code review are aligned.

#### 13.7.6 Preservation of prior fixes

V1.95 amendments (ClientProvider, migration-reset button, workspace default + stale overwrite, FingerprintGate bypass) continue to ship. V1.96 adds the surface and diagnostic improvements on top of them.

### 13.8 V1.97 Amendments — First-launch reliability hardening

> **Daemon step references below are historical** (V1.105 §13.10.2). Workspace/Browse rules apply to Workspace step 2 in the current IA.

**Product behavior target (author-visible).** A clean desktop install must not strand the author in an unbounded starting state. The wizard may succeed or surface a daemon failure, but the outcome must be observable, bounded, and actionable.

- Step 1 remains contained at desktop window sizes: the step list does not crowd the content area, card bounds hold, and long workspace paths truncate instead of expanding the layout.
- The Browse action calls the native directory picker with the desktop command's expected `defaultPath` argument. A casing mismatch such as `default_path` is a product-blocking failure because it prevents workspace selection.
- On clean first launch, the sidecar lifecycle cannot rely on a stale or synthetic `Starting` state. If no owned child process exists and no spawn attempt is in progress, desktop launch must either attempt a real daemon start or surface a bounded error state with recovery copy.
- Existing-install launches preserve V1.95/V1.96 setup behavior: `setup_completed`, workspace path preservation/stale overwrite rules, reset-local-database recovery, and daemon diagnostics.
- V1.97 does not expand desktop distribution scope: signing, notarization, auto-update, GitHub Releases, multi-OS release hardening, tray/menu-bar, and native notifications remain out of scope.

#### 13.8.1 Technical invariant — sidecar startup state machine

The desktop sidecar manager state machine uses process ownership as the boundary between attach and spawn:

- A newly constructed `SidecarManager` starts in `Stopped` with no owned child. It MUST NOT initialize to `Starting` as a synthetic "maybe starting" placeholder.
- `Starting` means a spawn attempt is in progress or the desktop app already owns and monitors a child during health probing. `start_with_budget` may return early for `Starting` only when the manager still owns and monitors a child (`child.is_some()`).
- `Starting` with no owned child is invalid. It must not suppress a new spawn attempt, retry path, or bounded error transition.
- Attaching to an already healthy daemon on the resolved port is allowed, but attach does not imply child ownership. App quit/stop may terminate only a child spawned and tracked by this desktop session.
- `Stopped` and `Error` remain retryable states. Retry/Reset actions must be able to re-enter the attach/spawn flow and then either reach health or surface a bounded diagnostic failure.

#### 13.8.2 Contract boundary

V1.97 does not change daemon routes, JSON schemas, generated TypeScript/Rust contracts, or `@42ch/nexus-contracts`. The wizard and desktop shell continue to use existing desktop status/detail capabilities and daemon health probes.

### 13.9 V1.100 Amendments — Clean-state first-launch bootstrap (P0)

> **Partial supersession (V1.105):** §13.9.1 bootstrap **timing** moves to Workspace **Continue** (§13.10.3). §13.9.2 Rule 13 gating is **rewritten** by §13.10.1 (D2 — always auto-start). Bootstrap IPC contract itself remains valid.

**Product behavior target.** A clean desktop install must complete the full wizard path without a pre-daemon `No active creator` failure. A new bootstrap substep between workspace selection and daemon start creates the minimum creator/workspace state the daemon requires to boot.

**Contract location:** The authoritative implementation-ready contract is [`.mstar/iterations/v1.100/specs/desktop-first-launch-bootstrap.md`](../iterations/v1.100/specs/desktop-first-launch-bootstrap.md). This section records the product behavior; the iteration contract is SSOT for implementation details (bootstrap mechanism, daemon-start timing matrix, minimum state, idempotency contract, reuse targets).

#### 13.9.1 Wizard flow change

A new bootstrap substep is inserted between step 1 (Welcome + Workspace) and step 2 (Daemon):

| Step | Title | What changed |
|------|-------|-------------|
| 1 | Welcome + Workspace | **Unchanged.** Workspace selection and `setWorkspacePath()` persist as before. |
| **1→2** | **Bootstrap (new)** | **New.** Wizard calls `ensureSetupBootstrap()` via desktop IPC. On clean state, generates a persistent creator ID and writes minimum config (`active_creator_id`, `active_workspace_slug_by_creator`) to `~/.nexus42/config.toml`. On re-run or partial state, detects existing creator and skips generation (idempotent). Failure blocks advance to step 2. |
| 2 | Daemon Ready | **Unchanged in behavior.** The daemon now boots successfully because creator state exists. Start, probe, error surfacing, and reset work as before. |
| 3 | ACP Agent Detection | **Unchanged.** |
| 4 | Done | **Unchanged.** |

#### 13.9.2 `.setup()` daemon auto-start gating (historical — superseded by §13.10.1)

The Tauri `.setup()` hook in `apps/desktop/src-tauri/src/lib.rs` now reads `setup_completed` before spawning the sidecar:

- `setup_completed = true` (existing install): **preserved** — auto-start as before (no regression).
- `setup_completed = false` or absent (clean state): **skip** — wizard owns daemon start via the step 2 `startDaemon` IPC.

#### 13.9.3 Contract boundary

V1.100 does not change daemon routes, JSON schemas, generated TypeScript/Rust contracts, or `@42ch/nexus-contracts` (`wire_contracts_changed: false`). The bootstrap is Tauri IPC only — it writes to `~/.nexus42/config.toml` through the existing Tauri Rust layer; the daemon reads the same config file it already reads at boot. No daemon boot-without-creator mode is introduced.

### 13.10 V1.105 Amendments — First-launch wizard reshape (Agent-first + app-level Daemon gate)

**Product behavior target.** V1.105 makes daemon readiness a **launch ritual** (fullscreen gate) and reduces the setup wizard to three author choices. **Iteration SSOT:** [`.mstar/iterations/v1.105-delivery-compass.md`](../iterations/v1.105-delivery-compass.md) + [`v1.105/specs/`](../iterations/v1.105/specs/).

#### 13.10.1 Rule 13 rewrite — always auto-start sidecar (D2)

The Tauri `.setup()` hook **always** spawns/attaches the sidecar on app launch — **regardless of `setup_completed`**. This **supersedes** §13.9.2 (V1.100 "wizard owns daemon start on clean state").

| `setup_completed` | Sidecar at `.setup()` | Author-visible entry after Ready |
|-------------------|----------------------|----------------------------------|
| false / absent | Auto-start (always) | `/setup` wizard |
| true | Auto-start (always) | Main UI |

The wizard **no longer** owns daemon start via a Daemon step or `startDaemon` IPC as the primary clean-state path.

#### 13.10.2 Fullscreen Daemon gate (every launch)

- First-launch **and** return visits wait on a fullscreen splash until daemon Ready (or bounded timeout/retry/recovery).
- **Gate layering (architect §5.2):** outer `DaemonLaunchGate` (`apps/web/src/components/setup/daemon-launch-gate.tsx`) wraps all routes in `App.tsx`; inner `SetupGate` routes by `setup_completed` only. `/setup` is under the outer gate, not inside `SetupGate`.
- Sidecar start is **exclusively** Tauri `.setup()` (`apps/desktop/src-tauri/src/lib.rs`) — gate subscribes/health-probes; happy path does **not** call wizard `startDaemon`.
- `setup_completed` marker still gates main UI vs `/setup` **after** Ready — unchanged semantics from §13.3.
- Wizard step 2 (Daemon Ready) from §13.2 is **retired** as a numbered step; diagnostic UX (timeout, retry, `resetLocalDatabase`) lives on `daemon-ready-splash.tsx` / outer gate.

#### 13.10.3 Three-step wizard flow (supersedes §13.2 four-step table for current product)

| Step | Title | Action |
|------|-------|--------|
| 1 | Agent | `POST /v1/daemon/agent-host/scan`; AgentPicker + custom command |
| 2 | Workspace | Default `~/Documents/nexus/default` + Browse; `ensureSetupBootstrap` on Continue |
| 3 | Done | `setAgentProfile` + `setup_completed=true` → main UI |

**Removed:** Welcome + Workspace as step 1; Daemon as step 2.

Bootstrap timing moves to Workspace Continue (not between Welcome and Daemon as in V1.100 §13.9.1).

#### 13.10.4 Settings Re-run Setup (V1.103 R1)

- Re-run still clears `setup_completed` marker only ([settings-setup-section.md](../iterations/v1.103/specs/settings-setup-section.md)).
- After confirm: fullscreen gate → `/setup` on Agent step (new IA).
- Workspace path and agent profile files are **not** deleted.

#### 13.10.5 Contract boundary

Prefer `wire_contracts_changed: false`. Portrait shell: `wizard-max-width` **480px**, `wizard-max-height` **720px**, viewport cap **85vh** (P2 — see `portrait-wizard-shell.md`). React structure: `TopStepIndicator` horizontal; retire left `step-panel-width` (208px) in wizard.

#### 13.10.6 V1.106 Amendments — Studio fixtures + shared chrome SSOT

**Iteration SSOT:** [`.mstar/iterations/v1.106-delivery-compass.md`](../iterations/v1.106-delivery-compass.md).

- **DaemonReadySplash fixtures:** Studio `/surfaces/launch` imports presentational `@web-setup/daemon-ready-splash` — same module as App outer gate.
- **MainBanner fixtures:** composition-only props-driven chrome in Studio — App `main-banner.tsx` stays daemon-hook-owned; no extract in V1.106.
- **TopStepIndicator:** single `apps/web/src/components/setup/top-step-indicator.tsx`; Studio `@web-setup/top-step-indicator` (closes dual-source residual).
- **Contract boundary:** `wire_contracts_changed: false`.

#### 13.10.7 V1.107 Amendments — Studio paint + presentational SSOT

**Iteration SSOT:** [`.mstar/iterations/v1.107-delivery-compass.md`](../iterations/v1.107-delivery-compass.md) + [`studio-ui-tune.md`](../iterations/v1.107/specs/studio-ui-tune.md).

- **Studio Tailwind content:** Design Studio must scan `apps/web/src/components/setup/**`, `layout/presentational/**`, and `packages/nexus-ui/src/**` so wizard and matrix utilities paint (FB-000).
- **Shell chrome SSOT:** Extract props-driven modules under `apps/web/src/components/layout/presentational/`; App wrappers (`sidebar.tsx`, `footer-profiles.tsx`, `daemon-health-indicator.tsx`) delegate markup; Studio imports via `@web-layout/*` (FB-013..014).
- **Settings chrome SSOT:** Presentational extracts under `apps/web/src/components/settings/presentational/`; Studio imports via `@web-settings/*` (FB-015).
- **Workspace path field:** Shared `workspace-path-field.tsx` — label **Workspace folder**, CTA **Change Folder…** on wizard and Settings (FB-008); wizard uses `layout="wizard-stack"`.
- **Toast:** App `apps/web/src/lib/use-toast.tsx` becomes thin re-export from `@42ch/nexus-ui` (FB-012) — package promotion alone (V1.106) does not close App duplication (`R-V1106P0-001`).
- **Contract boundary:** `wire_contracts_changed: false`.

---

## 14. ACP Agent Detection (V1.94)

**Status**: Draft (V1.94) — normative contract frozen by P-1; implement authority P0.

### 14.1 Endpoint

**`POST /v1/daemon/agent-host/scan`** — additive, no breaking change to existing agent-host routes.

**Handler**: `crates/nexus-daemon-runtime/src/api/handlers/agent_host.rs` — new `scan` function, wired into the existing agent-host router (same route group as `health`, `sessions`).

**Consumers**: Setup wizard agent step (V1.101 Must / P0 — app-shared `AgentPicker` at `apps/web/src/components/setup/agent-picker.tsx`); **V1.102** thin Settings host (`/settings`) remounts the same picker for post-setup agent change; **V1.103** deepens into S3 Settings shell with `/settings/agent` + `getAgentProfile` preselect (G1). **Current IA authority:** [settings-shell-ia.md](../iterations/v1.103/specs/settings-shell-ia.md) + [settings-agent-section.md](../iterations/v1.103/specs/settings-agent-section.md). Execution-mode matrix remains deferred post-V1.103 (DF-70).

### 14.2 Contract shapes

Frozen in `schemas/daemon-api/agent-host/scan-request.schema.json` and `schemas/daemon-api/agent-host/scan-response.schema.json`:

- **Request** (`AgentScanRequest`): optional `filter` (string enum: `"installed"` | `"all"`; default `"all"`); optional `registry_refresh` (bool; default `false` — uses cached registry data unless explicitly refreshed).
- **Response** (`AgentScanResponse`): `agents: AgentScanEntry[]`. Each entry:
  - `name` (string, required) — agent display name from registry.
  - `registry_agent_id` (string | null) — matching ACP registry agent ID; null for custom entries.
  - `launch_command` (string | null) — known launch command (from registry binary `cmd` or user-supplied); null when neither is available.
  - `installed` (bool, required) — `true` when the binary referenced by `launch_command` is found on PATH.
  - `version` (string | null) — best-effort `--version` probe result; null when probing fails or times out.
  - `description` (string | null) — agent description from registry.
  - `icon_url` (string | null) — agent icon URL from registry.

### 14.3 PATH-probe safety boundary

The scan is a read-only local operation executed by the daemon (not the frontend). It MUST observe the following safety constraints:

1. **Registry-known binary names only**: the probe extracts binary names from the ACP registry cache (`crates/nexus-acp-host/src/registry.rs` → `AgentEntry.distribution.binary.<platform>.cmd`). No user-supplied commands are executed during scan.
2. **Bounded concurrency**: probe at most N agents concurrently (recommended N=4); the probe is a `which`-equivalent PATH lookup followed by a `--version` subprocess call with a 2-second timeout per binary.
3. **Short `--version` timeout**: each `--version` subprocess is spawned with a ≤2s timeout. A timeout or non-zero exit is treated as "version unknown" — the agent is still reported as `installed: true` if the PATH lookup succeeded.
4. **No shell expansion**: binary names are executed directly (not through a shell); arguments are fixed (`--version` only); no `$PATH`, `~`, or other expansion.
5. **No user-supplied commands during scan**: the `launch_command` field in the response is populated from registry data or supplied separately outside the scan; the scan's subprocess boundary never runs a user-provided string.

**QC2 review note**: the PATH-probe execution boundary is reviewed by qc2 (security lens) at P-last. The constraints above are the architectural safety contract; implementers must not loosen them.

### 14.4 Integration with registry cache

The scan handler composes two existing subsystems:

1. **`RegistryClient::get_registry()`** (`crates/nexus-acp-host/src/registry.rs`) — provides the cached agent list (stale-while-revalidate). The `registry_refresh: true` flag on the request forces `RegistryClient::refresh()` before scanning.
2. **`scan_local_installations()`** (new helper in `crates/nexus-acp-host/src/registry.rs`) — PATH probe of registry-known binary names. Returns `Vec<LocalInstallation { binary, version: Option<String> }>`.

The handler joins the registry list with the scan results to produce the annotated `AgentScanEntry[]`.

### 14.5 Non-goals

- Agent installation / download / update (registry-only detection; the user manages their own ACP agent binaries).
- Full `AgentProfile` CRUD API (wizard + Settings Agent section write the default profile via desktop `setAgentProfile`; broader CRUD remains a separate future iteration).
- Execution-mode matrix / BYOK / AgentPicker package promotion (out of V1.103 scope; see [V1.103 compass](../iterations/v1.103-delivery-compass.md) Non-Goals). Multi-section Settings shell for Agent/Connection/Setup is **in scope V1.103** — not a non-goal here.

---

*Desktop shell feature-line spec. V1.66 Draft (Phase 2b `@architect`); flips Shipped (V1.66) at P-last. **V1.94 amendment** (§13–14) adds Setup Wizard + ACP Agent Detection contracts; frozen by P-1. The compass is authoritative for scope/batching/residual tracking; this spec is the durable contract.*
