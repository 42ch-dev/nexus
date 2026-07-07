# Desktop Shell (Tauri) — Specification v1

**Status**: Shipped (V1.66) — Tauri Desktop Shell delivered (QC tri-review Approve after fix-wave-1 + QA Pass)
**Document class**: Feature line
**Created**: 2026-06-25 (Phase 2b, `@architect`)
**Scope**: Nexus desktop shell contract — `apps/desktop` Tauri v2 wrapper, SPA adapter selection (`TauriClient`), desktop-only `NexusClient` extensions, native file actions + path guard, bundled `nexus42` sidecar lifecycle, port discovery, capability detection, macOS-first unsigned dev build. V1.67+ deferrals (signing, multi-OS, auto-update, in-process lib link, body editor) recorded in §2.
**Iteration compass**: [v1.66-tauri-desktop-shell-delivery-compass-v1.md](../../iterations/v1.66-tauri-desktop-shell-delivery-compass-v1.md) (scope/roadmap SSOT — §0 grill decisions, §1.1 Track A, §5 locked design items)

**Coordinates with**:

- [web-ui.md](web-ui.md) §14 (Desktop Shell stage — product UX + user stories + capability table delta)
- [web-ui-design-requirements.md](web-ui-design-requirements.md) §6 (desktop shell surface design requirements)
- [daemon-runtime.md](daemon-runtime.md) §12 (Tauri sidecar mode — daemon-side launch/readiness/lifecycle)
- [local-api-surface-conventions.md](local-api-surface-conventions.md) §9 (local daemon port discovery)
- [web-ui-design-requirements.md](web-ui-design-requirements.md) §6 (desktop shell surface design requirements)
- [agent-nexus-tool-bridge.md](agent-nexus-tool-bridge.md) / `host_tool_handlers.rs` (W-002 path-guard reference for `openWith`/`revealInFinder` scope)
- `apps/web/DESIGN.md` (Desktop Shell Supplement — window/menu/dialog/context-menu/status tokens)
- [schemas-external-consumer-boundary.md](../schemas-external-consumer-boundary.md) — `wire_contracts_changed: false` (V1.66); desktop-native methods are Tauri IPC, not Daemon API wire. **V1.94:** `wire_contracts_changed: true` (additive `POST /v1/daemon/agent-host/scan` schemas; `@42ch/nexus-contracts` 0.20.0 → 0.21.0).
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
- System tray / menu-bar app / global hotkeys / native notifications; custom title bar / animated transitions (Production polish).
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

Window chrome / app menu / native dialogs / desktop context menu / daemon-status indicator tokens in `apps/web/DESIGN.md` **Desktop Shell Supplement (V1.66 Standard+)**. Product intent + constraints in [web-ui-design-requirements.md](web-ui-design-requirements.md) §6. System tray: none in V1.66.

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
**Iteration compass**: [v1.94-desktop-onboarding-ia-pass-delivery-compass-v1.md](../../iterations/v1.94-desktop-onboarding-ia-pass-delivery-compass-v1.md) §1 (locked decisions A2+B1, C1, H1) + §5 (acceptance criteria).

### 13.1 Purpose

The desktop app's first-launch flow walks a new author through workspace creation, daemon startup, and ACP agent selection before entering the main UI. The wizard is gated by a `setup_completed` marker — absent or `false` → wizard; `true` → skip.

### 13.2 Four-step flow

| Step | Title | Action | UX states |
|------|-------|--------|-----------|
| 1 | Welcome + Workspace | Resolve default workspace (`~/Documents/nexus/default/` via `dirs::document_dir()`); create directory if absent. Existing `~/.nexus42/config.toml` values are preserved — no forced migration unless stale pattern. | Path display + native directory picker ("Browse…") + "Use default" affordance. |
| 2 | Daemon Ready | Start the bundled `nexus42` sidecar; poll `GET /v1/daemon/runtime/health` until healthy. Reuses the existing `HEALTH_START_TIMEOUT` (15s) + `SidecarManager` lifecycle from §7. | "Starting daemon…" transient → "Daemon ready" (success) OR error state distinguishing timeout vs port conflict vs crash. Never a silent hang. |
| 3 | ACP Agent Detection | Call `POST /v1/daemon/agent-host/scan`; display registry entries annotated with PATH-install status. Default recommendation = first `installed: true` entry with "Recommended" badge. | Scanning transient → agent list with selectable cards (name, version, installed badge, "Recommended" badge) → "No agents found" state with custom `launch_command` input + "Continue with custom" CTA. |
| 4 | Done | Persist the selected agent + `setup_completed = true` in `~/.nexus42/config.toml`; transition to main UI. | Confirmation screen; "Finish" CTA launches main UI. |

### 13.3 `setup_completed` marker

- **Location**: `~/.nexus42/config.toml` field `setup_completed: bool`.
- **Semantics**: absent or `false` = first-launch (wizard); `true` = skip wizard, enter per-launch daemon-ready gate.
- **Additive**: the field is optional; existing config files without it are treated as absent (= first-launch). TOML deserialiser must use `#[serde(default)]` or equivalent — the field must tolerate unknown config shapes.
- **Persistence**: the Tauri shell writes `setup_completed = true` on wizard completion via the existing `set_setup_completed` command (P0). The CLI config path (`apps/nexus42/src/config.rs`) accepts the field additively.
- **Reset**: Settings exposes a "Re-run setup" action that clears the marker. Missing marker = fail-safe to wizard.

### 13.4 Per-launch daemon-ready gate

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

#### 13.6.1 Setup wizard layout redesign

The setup wizard moves from a centered card with horizontal steps at the top to a left‑sidebar vertical step indicator with content on the right:

- Steps: Welcome (workspace selection), Daemon (status/error/reset), Agent (detection/selection), Done.
- The wizard fills the entire window (no `min-h-screen items-center justify-center`).
- Step indicators are a vertical list in a fixed left panel (`w-52`), with the current step highlighted.
- Content area keeps the card chrome (border, shadow, background).

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

---

## 14. ACP Agent Detection (V1.94)

**Status**: Draft (V1.94) — normative contract frozen by P-1; implement authority P0.

### 14.1 Endpoint

**`POST /v1/daemon/agent-host/scan`** — additive, no breaking change to existing agent-host routes.

**Handler**: `crates/nexus-daemon-runtime/src/api/handlers/agent_host.rs` — new `scan` function, wired into the existing agent-host router (same route group as `health`, `sessions`).

**Consumers**: P1 setup wizard step 3 (selectable agent cards); future Settings→Agent detection UI (deferred).

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

- Agent re-detection on non-first launch (deferred — scan is used by the wizard only; power-user re-scan is a Settings action).
- Agent installation / download / update (registry-only detection; the user manages their own ACP agent binaries).
- Full `AgentProfile` CRUD API (the wizard picks a default; CRUD is a separate future iteration).

---

*Desktop shell feature-line spec. V1.66 Draft (Phase 2b `@architect`); flips Shipped (V1.66) at P-last. **V1.94 amendment** (§13–14) adds Setup Wizard + ACP Agent Detection contracts; frozen by P-1. The compass is authoritative for scope/batching/residual tracking; this spec is the durable contract.*
