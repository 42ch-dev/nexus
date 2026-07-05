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
- [schemas-external-consumer-boundary.md](../schemas-external-consumer-boundary.md) — `wire_contracts_changed: false` (V1.66); desktop-native methods are Tauri IPC, not Local API wire

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

*Desktop shell feature-line spec. V1.66 Draft (Phase 2b `@architect`); flips Shipped (V1.66) at P-last. The compass is authoritative for scope/batching/residual tracking; this spec is the durable contract.*
