# Desktop Shell — Product and Host Contract

**Classification:** Master. **Status:** Electron target contract selected for implementation; **not a claim that the Electron product cutover or dual-architecture GUI qualification has shipped**. The existing Tauri implementation is the source baseline until its replacement passes the cutover gates. This in-place revision replaces Tauri-specific normative hosting, preserves the shipped setup/product behavior, and leaves historical evidence unchanged.

Architecture authority: [rust-core-service-boundary.md](rust-core-service-boundary.md) §§5,7–12,16. This Master owns desktop capability, window/setup, IPC and packaging contracts; service/domain HTTP schemas remain schema-owned. No second desktop UI or competing desktop spec is introduced.

## 1. Product and cohort

Nexus desktop reuses `apps/web`, `apps/design-studio` and `packages/nexus-ui`; no visual redesign, duplicate UI or new design system. Supported desktop cohort is macOS13+ on arm64 and x86_64. Windows/Linux GUI, auto-update, native product-menu expansion and global shortcut systems are not added.

Bundle id `io.nexus42.desktop`; product/app name `Nexus`. Product version comes from root `package.json.version`, not an iteration label or independent staging literal. `apps/desktop-electron/resources/product.json` owns id/name. Existing brand icon composition is retained in Electron resources without the Tauri CLI.

## 2. Host ownership and replacement boundary

- Electron main owns trusted raw home, config persistence, credentials, active workspace resolution, OS actions, service supervision and windows.
- Preload exposes only `window.nexusDesktop` version1. Renderer remains sandboxed, context-isolated, Node-disabled, web-security-enabled and never loads `.node`, native handles, Principal claims or persisted secrets. A path shown in UI is not path authority.
- An Electron utility process hosts `@42ch/nexus-service`; the service calls the Rust/native authority. No separate proof `openCore` owner exists beside the service. Service code never imports Electron.
- A utility process is app-lifetime-bound. Only independent TS-service composition may outlive the GUI; attach and keep/quit are defined in §7.
- Tauri product tree/toolchain/CI are retired only after accepted host parity and unsigned packaging. The still-consumed integrated legacy daemon + embedded SPA and callable dormant CLI rows are **not** retired merely because desktop changed. Public operator names survive.

## 3. Public entries and resources

`pnpm dev:desktop` launches the Electron host over built web dist; `pnpm dev:desktop:web` launches Vite HMR plus Electron. Neither stable-interface UI path invokes Cargo or fetches a Rust sidecar. Explicit native changes require the separate native rebuild/preparation command.

`pnpm build:desktop -- --arch arm64|x64` delegates to the package-scoped unsigned driver. `nexus42 desktop bundle` delegates to that same entry; the command name is preserved. No second Rust implementation of packaging.

Packaged `Contents/Resources` contains host/preload JS, web-dist, required native payloads outside ASAR where loaded, and an ordinary unpacked `service/` directory with compiled TS entry plus its production dependency closure. Standalone Node must not depend on reading Electron ASAR or workspace symlinks. Packaged resource lookup has no repository fallback.

## 4. IPC and sender boundary

Global `{version:1,runtime:{localEndpoint},invoke,onStatusChanged}` is typed through one desktop contract; immutable nonsecret endpoint metadata comes from main before preload so a nondefault port is available synchronously. Web imports types only, never Electron runtime. Invoke channel `nexus:desktop:invoke`; main event `nexus:desktop:status-changed`. Preload exposes no raw ipcRenderer, Node/process/env, arbitrary channel invocation, proof commands or Electron event objects.

Request `{request_id,operation,payload}`; response `{request_id,ok:true,result}` or `{request_id,ok:false,error:{code,message}}`. ASCII request IDs `[A-Za-z0-9._-]{1,128}`; operation and payload schemas closed. Unknown fields, wrong types and oversized values reject before effect. Invoke checks the selected live webContents, exact top mainFrame identity, exact approved origin and current window generation. Missing senderFrame never falls back to trusting the whole webContents URL. Stale window, subframe and foreign-origin messages deny.

Bounds: request/response1MiB UTF-8, status4KiB, diagnostic tail2KiB; path4096 bytes, URL8192, creator id256, agent name256, agent command8192, connection config64KiB. No path/id/URL NUL or control characters. At most32 active and16 queued, aggregate queued≤1MiB; excess=`busy`. Default command30s deadline, restart25s; user dialogs require cancellation rather than fake timeout-success. Reload/destruction removes pending renderer replies/listeners, not service ownership.

Allowlist:

- `open_with({path})`, `reveal_in_finder({path})`, `open_external_url({url})`, `pick_directory({defaultPath})`.
- `get_workspace_root(null)`, `set_workspace_path({path})`, `switch_active_creator({creatorId})`, `ensure_setup_bootstrap(null)`.
- `get_entrance(null)`, `set_entrance({value})`, `get_setup_completed(null)`, `set_setup_completed({value})`, `get_agent_profile(null)`, `set_agent_profile({name,launchCommand?})`.
- `get_connection_config(null)`, `set_connection_config({config,credential})`, `delete_connection_config(null)`.
- `get_daemon_status(null)`, `start_daemon(null)`, `stop_daemon(null)`, `restart_daemon(null)`, `reset_local_database(null)`, `toggle_maximize_window(null)`.

Method success returns the existing capability result: void/null, picker/path strings, bootstrap `{creator_id,already_bootstrapped}`, entrance enum, setup boolean, optional agent profile, status or public connection config as appropriate. Domain requests/streaming remain HTTP, not a generic IPC SQL/native-operation bridge.

## 5. HTTP client and credential storage

`DesktopClient` replaces the Tauri-named thin subclass; inherited BrowserClient methods stay the one HTTP implementation. Runtime detection requires a valid versioned preload; no build-env-only desktop selection or retained `__TAURI_INTERNALS__` branch. Web/browser mode and same-origin `/v1/daemon/*` remain supported independently of desktop.

Main stores connection configuration with Electron safeStorage encryption, atomic replacement and owner-only file permissions at product userData. No new plaintext-write fallback. A one-time import can read the old `nexus42`/`connection_config` macOS keychain entry or old app-data JSON on main only; validate and encrypt before selecting the new store, retain original user data. Once selected, encrypted storage is sole authority; a durable cleared marker prevents re-import after deletion. Unavailable encryption or corrupt state errors rather than silently wiping or retaining a fake success.

Public config `{endpointUrl,label?,active?,pinnedFingerprint?,hasApiKey}` contains no stored API key. Credential update is explicit `{action:'keep'}` or `{action:'replace',value}`; empty replacement clears. Endpoint change cannot inherit the previous endpoint's secret. The user may type and submit a key, but after saving the UI reloads public metadata and discards it. Masked placeholders are never sent as credentials. Browser storage retains its existing real-key contract.

Main attaches `X-API-Key` only to the selected top-frame renderer's fetch/XHR for the exact active saved origin and `/v1/daemon/` path prefix. Strip renderer-provided auth first; no secret on inactive/foreign endpoints, images, external links, other frames, fingerprint probes or redirects. Authenticated redirects deny. Keep current fingerprint/TOFU mismatch gate; no TLS error bypass. This is request policy on the existing client, not a second HTTP server/proxy.

## 6. Scheme, CSP, navigation and external URLs

Production static origin `nexus://app`; scheme registered secure/standard/fetch/streaming before ready. Files are decoded once, realpathed beneath canonical web-dist, checked by path components and regular-file status. Traversal/symlink escape rejects. HTML SPA navigation may fall back to index; missing assets/APIs never do. No repository fallback in packaged mode.

CSP: `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob: https:; font-src 'self' data:; connect-src 'self' <selected-service-origin> <explicit-fingerprint-probe-origin>; object-src 'none'; base-uri 'none'; frame-src 'none'; frame-ancestors 'none'; form-action 'none'`. Main inserts validated exact origins, never untrusted strings/wildcards. HMR is restricted to the explicitly launched loopback Vite origin and exact websocket origin in development only; packaged code ignores dev-origin override. No unsafe-eval, remote scripts, webviews or service workers.

Deny new windows and navigation/redirect out of active app origin. External opening happens only via the explicit main operation. One main URL policy accepts valid http/https with host and without userinfo/control characters; rejects all other schemes. No proof-only github allowlist and no auto-open of a denied navigation.

Standalone service origin admission replaces obsolete Tauri-only origins with exactly `nexus://app`, preserving loopback/Vite and supported browser origins. No wildcard/null-origin relaxation or stripping Origin to bypass policy. Remote service must explicitly support the selected app origin; incompatible configuration surfaces an error.

## 7. Service lifecycle, identity and quit

### 7.1 Readiness and ownership

App launch always starts or attaches irrespective of `setup_completed`. Desktop local port precedence: explicit launch value → valid `NEXUS_DAEMON_PORT` →8420. Standalone service's own default8421 is unchanged. Invalid explicit port errors. Selected remote HTTP connection does not grant local controls authority over remote processes.

Start reads the private `CoreServiceDiscovery` record and matches it against guarded `GET /v1/daemon/runtime/discovery`, including instance/home/endpoint/epoch, then health. This guarded read is a **required implementation prerequisite**: current unauthenticated runtime/status does not provide those identity fields. Reuse the existing closed DTO, auth rules and service instance; do not invent a new schema. PID is diagnostic, never stop/attach authority. Legacy listener without matching identity is a conflict, not an auto-kill target.

Utility calls existing `startService(ServiceOptions)`, awaits published ready/uninitialized discovery plus HTTP health≤15s (probe2s; 100ms first second then250ms). Clean home/no profile is healthy uninitialized state, not startup failure. Messages between main and utility carry generation/request id and only start/close/reset-local-state operations; stale generation replies are ignored.

### 7.2 Stop, restart, recovery, events

Owned stop awaits cooperative `CoreCloseReport` within the5s budget. Unconfirmed close/lost owner/timeout is Interrupted/error with retained diagnostic state, not SIGKILL-and-success. Ordinary stop on attached independent service is a no-op. Explicit Restart/Stop-and-Quit may send the authenticated expected-instance/epoch stop, verify shutdown, wait≤3s for port release, then start. A `stopping` acknowledgment alone is not completion; stale identity conflicts without touching replacement.

Restart is single-flight. Intentional stop/handoff suppresses auto-restart. Owned unexpected exits retry500/1000/2000/4000/8000ms (five attempts), degraded during backoff, **stopped** at exhaustion, manual recovery thereafter. Stop cancels backoff. Renderer crash only recreates renderer, not native owner. Status `{state,version?,port,detail?}` uses current starting/running/degraded/stopped/error states; retain concrete diagnostics, bounded event size and unsubscribe behavior. Register event listener before snapshot fetch.

### 7.3 Three-option quit

Stop Daemon & Quit / Keep Daemon & Quit / Cancel remain available. Cancel leaves state alone. Stop waits real owned close or verified attached stop; failure keeps app open. Keep on independent attach detaches only GUI. Keep on owned utility preflights locally installed Node≥22.22 plus packaged service entry, confirms utility close, starts independent detached Node with same home/port, waits identity+health≤15s, then exits. No two native owners; no utility falsely described as detached. Missing Node, unconfirmed close or failed handoff gives an actionable error and keeps the app open. In-flight work is Interrupted, not promised continuation. For uninterrupted after-exit work attach to independently started service from the outset. No auto-install of Node.

Closing a window is not automatically quitting/stopping work. Actual quit, including native quit role, shares the serialized decision path.

## 8. Config, bootstrap, profile and local-state recovery

Main reads existing home config: per-active-creator `workspace_path_by_creator` → legacy `workspace_path` → documents `nexus/default`. Workspace/creator changes preserve the per-creator mapping, legacy mirror and default slug; serialize mutations, preserve unrelated TOML values, atomically replace. Corrupt config errors without overwriting it. Bootstrap is idempotent, creates `ctr_local` +12hex only when no active creator exists, never replaces an existing creator, and is not a boot prerequisite.

`entrance` is developer/content-creator with established missing/stale behavior; absent `setup_completed` is false. `getAgentProfile` returns first valid native_cli profile or null; upsert retains other provider kinds/unknown values. Merely persisting a launch command never executes it. Directory picker is directory-only, honors defaultPath and returns null on cancel.

**Reset is retained product behavior, not a migration shortcut.** Explicit native confirmation is mandatory. Main serializes reset with lifecycle, confirms service close, then a narrow utility-native reset acquires the existing storage migration fences exclusively for all target stores before deletion. Target only non-symlink `.nexus42/creators/<id>/workspaces/<slug>/{state.db,state.db-wal,state.db-shm}`; preserve stable lock files and all other data. Live writer, unknown owner, symlink or unconfirmed close errors before destructive action. The native binding reuses Rust storage fencing; no JS SQL or unrelated filesystem wipe. Reset then restores service/readiness through main; renderer reload is not falsely treated as re-running main startup. Cancellation/failure must not look like completed recovery. Any safety-driven substep difference requires an explicit accepted deviation and durable tracked residual; the whole capability cannot be replaced by blanket refusal.

The no-wipe policy forbids reset as migration/implementation escape hatch, not an explicitly confirmed shipped local-state recovery function. User documents, harness, knowledge, specs and creative workspace directories are never reset targets.

## 9. Authoritative path guard and file actions

Open/reveal resolve the active root on **each** main-side call, realpath both root and candidate, accept root itself or root+separator prefix, and use the canonical result in the OS action. Relative paths resolve beneath root. Symlink escape, sibling-prefix collision, invalid/unreadable paths or missing root deny before OS effects with existing structured errors. Static capability scope is only defense-in-depth.

Changing the active creator/workspace invalidates prior root authority. A renderer cannot freeze a root or supply its own root claim. Directory selection intentionally may choose a new root outside the old root; it is not an unguarded open/reveal exception. Copy path remains browser clipboard behavior.

## 10. Window, menu and shared UI

Window1280×800, minimum960×640, overlay titlebar, traffic lights at12,14; maximize toggle remains main-owned. Keep Dock brand and single-instance focus behavior. Existing SPA menus/shortcuts remain product authority; standard native edit/quit roles support ordinary macOS interaction, not a duplicate native product menu. No auto-updater.

Studio uses the same presentational setup/layout/settings components and shared UI tokens. Do not change markup/design merely to fit the Electron host. Existing DESIGN/Studio contracts remain in force.

## 11. Unsigned packaging and distribution

Required artifacts for **each** macOS architecture: `Nexus.app` and `Nexus-<version>-darwin-<arch>-unsigned.dmg`; zip of app for CI transport, checksum manifest and provenance receipt. Default root `artifacts/desktop/<version>/darwin-<arch>/`. Build/layout entry and application identity are single-sourced. Native matrix labels follow existing repository precedent: arm64 `macos-15`, x64 `macos-15-intel`; assert actual native architecture, no emulation substitute.

**No Apple credentials are required.** Ordinary unsigned packaging must succeed with none. Signing/notarization/stapling/release requests fail explicitly before staging. Remove existing proof identity/env/entitlement signing wiring, no dormant future lane. Packager20.3 must use `asarIntegrityDigest:false`, no `osxSign`/`osxNotarize`, no binary/fuse modifications: its default integrity patch can invoke ad-hoc codesign. No path in this lane may invoke codesign/notarytool/stapler. Inherited vendor/ad-hoc signatures in downloaded Electron binaries are recorded honestly; the pipeline does not strip or apply them and does not claim Developer ID/notarized trust.

The explicit native build prerequisite likewise removes the old native helper's unconditional signing mutation, leaving no optional/dormant signing branch. It must load the produced unsigned `.node` and read real compatibility, never reuse stale compatibility JSON after a load failure. This native rebuild is separate from ordinary TS/UI loops; normal packaging consumes the verified prebuilt payload. Unsigned load failure blocks acceptance rather than triggering signing.

Fail before publication on missing/invalid web/host/service/native/tool/icon prerequisites. Stage temporarily; publish final arch directory only after both app+DMG and receipt succeed. Preserve previous completed artifacts on failure. Pin/record inputs, revision, lock/web/service/native hashes, runtime/tool versions, native contract/target and artifact checksums. Repeatable provenance/layout does not claim bit-identical timestamped DMGs. CI uploads both architectures' complete sets, not an automatic public signed release.

Do not relabel macOS15 package inspection as macOS13 launch proof, unsigned as trusted Gatekeeper distribution, or native package CI as historical x64 GUI qualification.

## 12. Acceptance and explicit host differences

All29 capability rows in the implementation acceptance inventory require evidence; no “same web bundle implies native parity.” Security/lifecycle negative cases, public entries, both artifact types/CPUs, config/bootstrap/profile, guarded paths and user-confirmed reset are load-bearing.

Explicit target differences from Tauri/proof: no killing PID-only/legacy listener (identity-safe attach/restart); stored secrets not returned to renderer or written plaintext; utility-owned Keep uses confirmed independent-service handoff with possible Interrupted work and installed-Node precondition; proof CSP/sandbox security is retained rather than Tauri's null CSP. No native product-menu/auto-update expansion is implied. Reset remains functional, not an approved omission.

Historical Electron decision JSON missing/unobserved rows remain unverified. New scoped evidence binds to the new implementation revision; it does not rewrite past qualification. Runtime checks, accepted package receipts and actual-surface evidence must be described at their exercised scope; no fabricated GUI/E2E success.

## 13. Setup Wizard — preserved product behavior

### 13.1 Marker and state

After service readiness, `setup_completed=false`/absent routes to setup; true routes to main UI. Marker is set only after successful completion persistence. Failed config writes leave the wizard recoverable. Workspace path and agent profile are durable across re-run setup.

### 13.2 Current flow

Entrance → Agent → Workspace → Done. Entrance chooses Content creator/Developer, default content-creator; agent uses shared AgentPicker/custom command; workspace defaults to documents `nexus/default` with Browse/Change Folder; Workspace Continue calls idempotent bootstrap; Done persists agent/profile and entrance **before** setup_completed=true.

### 13.3–13.9 Superseded bootstrap assumptions

Earlier Welcome/Daemon-numbered-step and bootstrap-before-daemon designs are no longer normative. Keep their product outcomes through the current flow, not their Tauri startup mechanism. No implicit data deletion when settings re-runs setup.

### 13.10 First-launch reshape — Entrance-first and app-level daemon gate

1. Electron main always starts/attaches service on app launch, independent of marker. Wizard does not own the clean-state start.
2. Outer `DaemonLaunchGate` wraps all routes; inner `SetupGate` routes by marker only. `/setup` is under the outer gate. First and returning launches wait for readiness or bounded error/retry/recovery. Gate subscribes/probes; no duplicate happy-path start.
3. Re-run Setup clears marker only after confirmation, then gate→Entrance with stored entrance pre-highlighted. No deletion of workspace/profile files. Missing/stale entrance resolves current content-creator default without read-time writes.
4. Portrait wizard width480px, height720px, viewport cap85vh; TopStepIndicator horizontal. Reuse presentational DaemonReadySplash, shared workspace-path field, layout/settings chrome and shared toast. Studio consumes the same presentational modules; App retains daemon hooks. No host-driven redesign.
5. Recovery retry/reset explicitly reaches main controller; reload alone does not rerun main's startup hook. Keep failed-reset error visible and never mark setup complete as a side effect of recovery.

### 13.11 No-profile boot

A clean raw home without active_creator_id must reach healthy service readiness. Profile/creator creation is post-gate business flow in setup/footer Profiles. Bootstrap remains optional idempotent Workspace Continue convenience. Readiness may truthfully be `uninitialized`; it is not failure and not permission to execute uninitialized domain operations.

## 14. ACP Agent Detection — unchanged service contract

`POST /v1/daemon/agent-host/scan` remains schema-owned by `schemas/daemon-api/agent-host/{scan-request,scan-response}.schema.json`, shared by Setup AgentPicker and Settings Agent section. Request optional filter installed/all (default all), registry_refresh boolean (default false). Response agents retain name, registry_agent_id, launch_command, installed, version, description and icon_url with existing nullability. Do not redefine these DTOs in Electron.

Scan is read-only service-side registry-known binary discovery, not frontend/native-shell command execution. At most bounded concurrency (recommended4); PATH lookup then fixed `--version` with≤2s timeout; nonzero/timeout means version unknown, not uninstalled if PATH succeeded. No shell expansion or user-provided launch string execution. Registry cache/explicit refresh semantics remain unchanged.

No automatic agent installation/download/update, broader profile CRUD, execution-mode matrix or new provider capability is introduced by desktop replacement. Settings IA authority remains [settings-shell-ia.md](../iterations/v1.103/specs/settings-shell-ia.md), [settings-agent-section.md](../iterations/v1.103/specs/settings-agent-section.md) and [settings-setup-section.md](../iterations/v1.103/specs/settings-setup-section.md).
