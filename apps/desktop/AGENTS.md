# apps/desktop — AGENTS.md

The Nexus **Tauri v2 desktop shell** (macOS-first, unsigned dev build in V1.66).
Parent rules: [`../../AGENTS.md`](../../AGENTS.md) (repo),
[`../../.mstar/AGENTS.md`](../../.mstar/AGENTS.md) (harness).

## Identity & placement

- `apps/desktop` is a **pnpm workspace sibling** of `apps/web`
  (`pnpm-workspace.yaml` admits `apps/*`). It shares the lockfile and the
  `@42ch/nexus-contracts` workspace dependency.
- `apps/desktop/src-tauri/` is a **standalone Tauri-managed Rust crate** — it is
  **NOT** a member of the root `Cargo.toml` workspace (compass §5 #9). Run
  `cargo check` / `cargo build` from `apps/desktop/src-tauri/`, never from the
  repo root, so Tauri's build script + generated context resolve.
- The desktop shell **wraps the bundled `apps/web/dist`** via
  `build.frontendDist` (compass §5 #4). It does **not** embed a second SPA or a
  handwritten UI; all screens come from `apps/web`.

## Development prerequisites

`apps/desktop/src-tauri/` is a standalone Tauri crate. Its `bundle.externalBin`
expects the sidecar binary for the target Cargo is building for to exist at
compile time, but the binaries are **gitignored** and not present on a fresh
clone. Run the workspace script before any `cargo` command in `src-tauri/`:

```bash
pnpm -w run sidecar
```

This builds `nexus42` for `aarch64-apple-darwin` (V1.66's single-arch CI target
and the native arch of current Apple Silicon Macs) via
`scripts/fetch-sidecar.sh` and copies the artifact into
`apps/desktop/src-tauri/binaries/`. To build a different target locally (e.g.
`x86_64-apple-darwin` on an Intel Mac), pass targets explicitly:

```bash
SIDECAR_TARGETS="x86_64-apple-darwin" pnpm -w run sidecar
```

Without this step, `cargo build`/`test`/`clippy` fails with the fail-fast guard
in `src-tauri/build.rs` if the sidecar binary for the current target is missing.

## SSOT & authority

- **Contract**: [`.mstar/knowledge/specs/desktop-shell.md`](../../.mstar/knowledge/specs/desktop-shell.md)
  (the feature-line spec — `NexusClient` desktop extensions, sidecar lifecycle,
  port discovery, capability detection, scope-whitelist path guard).
- **Iteration compass**: [`.mstar/iterations/v1.66-tauri-desktop-shell-delivery-compass-v1.md`](../../.mstar/iterations/v1.66-tauri-desktop-shell-delivery-compass-v1.md)
  §5 (LOCKED design items).
- **Design tokens**: [`apps/web/DESIGN.md`](../web/DESIGN.md) "Desktop Shell
  Supplement (V1.66 Standard+)" — single design-system SSOT (compass §5 #6).
- **Wire contracts unchanged** (`wire_contracts_changed: false`, compass §5 #5):
  desktop-native methods are Tauri IPC / app-side process control, **not** Local
  API HTTP. Do **not** add `schemas/` for desktop behavior.

## Transport boundary (inherited from `apps/web`)

Screen logic in `apps/web` depends only on the `NexusClient` interface
(`apps/web/src/lib/nexus/types.ts`) — never on `fetch`/`invoke` directly. The
desktop shell selects `TauriClient` (thin-over-`BrowserClient`, §5 #1) at the
client factory via capability detection (§5 #7). The 21 `NexusClient` data
methods reuse the **identical HTTP transport** to the localhost daemon; only the
desktop-only capability surface (`openWith`, `revealInFinder`, daemon lifecycle)
is new.

## Path guard (security-critical — compass §5 #8)

`open_with` and `reveal_in_finder` custom commands enforce an **authoritative**
runtime guard in `src-tauri/src/lib.rs` (`guard_path`): canonicalize the
requested path + the active workspace root, then prefix-check. The workspace root
is resolved from `~/.nexus42/config.toml` (`workspace_path`) — the same source the
daemon/CLI use. The Tauri opener `scope` in `capabilities/main.json` is
**defense-in-depth only** (static scopes cannot encode the dynamic workspace
root). On rejection the JS layer surfaces plain-language copy
(`Path not opened. The file is outside the active workspace.`).

**V1.66 limitation:** the workspace root is captured once at app startup. If the
active workspace is changed while the app is running, the context-menu path guard
continues to use the root from startup until the app is restarted. Live refresh
is V1.67+ scope.

## V1.66 scope

- **In**: `apps/desktop` scaffold; `TauriClient`; capability detection; Q5
  desktop actions (Open with…, Reveal in Finder, Copy Path); path guard;
  DESIGN.md supplement consumption; bundled sidecar lifecycle
  (`externalBin` + `Command.sidecar` + `SidecarManager`); tests.
- **Out (V1.67+)**: in-process `nexus-daemon-runtime` library link (replacing
  the bundled sidecar process), port discovery handshake, signing/notarization,
  multi-OS, auto-update, system tray, mobile, body editor.

## Conventions

- **macOS-only** in V1.66 (`aarch64-apple-darwin` + `x86_64-apple-darwin`).
- Tauri v2 config keys: `build.frontendDist` (NOT `build.dist`), `app.withGlobalTauri`
  (set `true` so `window.__TAURI__` is present), `bundle.externalBin`.
- Capability detection: `@tauri-apps/api/core` `isTauri()` checked once at the
  client factory (§5 #7).
- **No desktop-owned JS runtime dependencies**: the webview uses the shared
  `apps/web` bundle and invokes Rust custom commands through
  `window.__TAURI__.core.invoke`; transport primitives live in the Rust crate.
  Do not add `@tauri-apps/plugin-shell` or `@tauri-apps/api` to
  `apps/desktop/package.json` — the shell plugin is a Rust crate dependency
  (`Cargo.toml`).
