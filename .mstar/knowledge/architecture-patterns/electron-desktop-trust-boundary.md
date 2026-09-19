---
module: apps/desktop-electron (Electron main / preload / scheme / OS-action boundary), apps/web (desktop bridge seam)
date: 2026-09-20
problem_type: architecture_pattern
category: architecture-patterns
severity: high
plan_id: 2026-09-19-v1.192-p0-electron-desktop-cutover
applies_when:
  - building or reviewing the IPC / preload / custom-protocol trust boundary of an Electron (or any webview-host) desktop shell
  - adding a renderer→main bridge operation that must not become a generic capability channel
  - hardening a packaged renderer (sender validation, CSP, navigation lockdown, secret handling)
  - reviewing "validated" IPC code for vacuous checks — sender checks, window generations, byte bounds, mirrored constants
related_components:
  - apps/desktop-electron
  - apps/web
  - apps/nexus-service
tags:
  - electron
  - ipc
  - preload
  - trust-boundary
  - sender-validation
  - window-generation
  - csp
  - custom-scheme
---

# Electron desktop trust boundary (typed IPC, sender identity, protocol and CSP)

## Context

v1.192 replaced the Tauri desktop shell with an Electron host (`apps/desktop-electron`) while keeping the product — the same `apps/web` SPA, setup flow, workspace semantics and TS service — behind it. The host's security model is the classic Electron three-process shape: a privileged **main** process owning OS actions, config, credentials, windows and service lifecycle; a **sandboxed renderer** (`sandbox: true`, `contextIsolation: true`, `nodeIntegration: false`) that never loads `.node` or secrets; and a **preload** bridge as the only renderer→main primitive. Renderer↔service traffic stays ordinary HTTP (`BrowserClient` / thin `DesktopClient` subclass); it never rides the desktop IPC channel.

The trust boundary has five separable surfaces, each with its own failure mode and its own review checklist:

1. **Contract + envelope** — one typed module, versioned, closed shapes, byte bounds.
2. **Sender identity + registration lifetime** — which `WebContents`/frame/window generation may invoke, and who owns the single `ipcMain.handle` channel.
3. **Custom scheme + resource resolution** — `nexus://app` serving packaged files only, with traversal/symlink containment.
4. **CSP + navigation/external-URL policy** — frozen posture, per-response origin, deny-by-default navigation.
5. **Main-owned secrets and runtime metadata** — the renderer must never see the stored API key, and non-secret metadata must be validated before use.

## Guidance

### 1. One host contract module; the preload is a mechanical mirror with a machine-checked parity lock

`src/desktop-contract.ts` is the single contract: envelope types, the closed operation union, per-operation payload/result maps, bounds, validators and the `DesktopBridge`/`window.nexusDesktop` type. It must not import Electron or Node-only modules (use `TextEncoder`, not `Buffer`) so the SPA can import **types only** — the web seam (`apps/web/src/lib/nexus/desktop-bridge.ts`) imports the contract relatively and never touches the Electron runtime.

The preload cannot import the contract in this toolchain: both `tsconfig.json` and `tsconfig.preload.json` emit to `dist/`, so pulling the contract into the preload program re-emits it as CommonJS, overwriting the ESM artifact main loads — under `"type": "module"` that is a hard `SyntaxError` at boot. The preload therefore carries **mechanical mirrors** (channel names, envelope version, bounds, operation union) under a marked header, and the drift risk is closed structurally: the contract exports `DESKTOP_BRIDGE_MANIFEST`, and a test extracts every mirrored value **from the compiled `dist/preload.js`** (numeric constants via a whitelisted arithmetic evaluator, strings via anchored regex, the operation union via array-literal extraction) and asserts equality. Drift on either side fails the test; a code-review convention alone would not.

### 2. Version the envelope end-to-end, bound the complete frame, and keep the failure path total

- Envelope: `{version: 1, request_id, operation, payload}` → `{version: 1, request_id, ok, result | error:{code, message}}`. `version` is **required and literal on requests and both response variants**; missing/wrong versions reject before any effect. An unversioned envelope is a defect — without the field, a stale renderer and a newer main can silently disagree about the frame shape.
- `request_id` grammar `^[A-Za-z0-9._-]{1,128}$`; unknown envelope/payload keys rejected before effects (closed shapes); NUL/C0/DEL rejected in every string field; per-field caps (path ≤ 4096 B, URL ≤ 8192 B, creator id/agent name ≤ 256 B, launch command ≤ 8192 B, connection JSON ≤ 64 KiB).
- Bounds apply to the **complete serialized UTF-8 frame**, not the payload: the success constructor measures the whole envelope (overhead included) and throws a coded `internal` on overflow. The error constructor **never throws** — it collapses over-long codes to `internal` and trims over-long messages until the complete failure frame fits, with a minimal hard-guard fallback. The failure path is the one path that must always produce a frame.
- Admission: 32 active calls, ≤ 16 queued with ≤ 1 MiB aggregate, overflow → typed `busy`; 30 s default deadline. User-cancellable dialogs must not be reported as timeout-success.

### 3. Sender validation binds the live window, its main frame, and the current window generation

`assertDesktopEventSender` checks, in order: live selected window → `event.sender === window.webContents` → a **real `senderFrame` object** (never fall back to `sender.getURL()` — a frameless sender must reject, not degrade) → `senderFrame === webContents.mainFrame` (subframes reject) → bound generation equals the **live** generation from `getCurrentGeneration()` evaluated at call time → exact app origin (`nexus://app`, no credentials/port). Error codes: `invalid_sender`, `stale_sender`, `invalid_origin`.

The live-generation source is a **required** option, validated before registration; registration without it fails closed (`invalid_input`) and is observable in plain Node. An optional generation source defaulted to "whatever was captured at registration time" makes the check vacuous — a stale registration passes its own check. Pin the negative: bound generation 7 against live 8 rejects `stale_sender`, while a captured `{generation: 7}` alone must reject `invalid_input`.

`registerDesktopIpc` is called per window and the generation bumps on every window replacement. Because the invoke channel is a single singleton (`ipcMain.handle` throws on duplicate registration), a renderer-crash replacement must serialize registration attempts on the previous in-flight attempt and dispose the superseded registration **exactly once** — whether it won and was stored or resolved late and was not. A late stale registration that is merely dropped still occupies the channel and leaves the visible window without its bridge.

### 4. Custom scheme: register privileges pre-ready, install the handler before the first window, and resolve on raw components

- `registerDesktopSchemes()` (privileges: standard, secure, fetch, streaming; CORS off) runs **before `app.whenReady()`** — a post-ready registration is invalid.
- `protocol.handle('nexus', …)` completes **before the first `createMainWindow()`/`loadURL`**; the composition test pins the adapter order `['protocol', 'window']`.
- Resolution chain: decode once → traversal check on the **raw decoded components** → realpath containment under the canonical dist root → regular files only → 404 for unknown assets; SPA fallback to `index.html` **only for explicit `text/html` navigations**, never for `*/*` or asset/API paths.
- Two failure modes to remember: `path.normalize()` lexically resolves `..` and would *hide* a traversal attempt (the check must inspect raw decoded components); and treating `Accept: */*` as a navigation signal turns every missing asset into a 200 — the fallback requires an explicit `text/html` media type.

### 5. CSP is a frozen posture rebuilt per response from the *active* origin

`buildDesktopCsp` emits one frozen policy: `default-src 'self'` with `connect-src 'self' <service-origin> <explicit fingerprint-probe origin>`, `object-src/frame-src/frame-ancestors/form-action 'none'`, `base-uri 'none'`, no `unsafe-eval`, no `*`. The origin is **never a renderer string**: main rebuilds the CSP per response in `session.webRequest.onHeadersReceived` from the current connection config (local endpoint by default, saved active remote endpoint after a connection change). Dev HMR adds only the two explicit Vite http origins and their exact websocket origins; packaged builds ignore dev URL overrides.

### 6. Navigation is deny-by-default; external URLs go through exactly one shared predicate

- `will-navigate`, `will-redirect` and `window.open`/webview creation deny everything outside the app origin — installed on the app-level `web-contents-created` handler **and** the selected window's `hardenWindow`, so non-primary `WebContents` cannot redirect out either.
- An outbound link never auto-opens through the navigation path; it must explicitly call `open_external_url`, whose predicate (`isAllowedDesktopExternalUrl`, shared by main and the OS-action handler) accepts parsed `http`/`https` with a nonempty host and **rejects raw C0/DEL bytes, padding, userinfo and unsafe schemes before parsing**. There is deliberately no host allowlist: parity means arbitrary `http(s)` destinations, not the proof shell's narrow test hosts.

### 7. Secrets never cross to the renderer; main injects auth for the exact origin and path prefix

- The connection store (`src/connection-store.ts`) keeps `credential` as safeStorage ciphertext in `userData/connection-config.enc` (dir 0700, file 0600, atomic temp+rename). `get()` returns a **public projection with no key**; only `getAuth()` (main-side) returns the active credential for network injection.
- `attachDesktopNetworkHooks` strips renderer-supplied `X-API-Key` (case-insensitively) and injects the stored key only for fetch/XHR whose URL's **exact origin** equals the pinned endpoint and whose path starts with `/v1/daemon/`; redirect targets leaving the origin get nothing; inactive/keyless config injects nothing.
- Encryption unavailable → explicit `secure_storage_unavailable`, never a plaintext fallback; a failed encryption is non-destructive (encrypt before atomic write). An endpoint change cannot retain the previous endpoint's credential.
- **Clear must be durable.** `delete()` writes a non-secret tombstone (`connection-config.cleared`) after the store file is confirmed gone, and `open()` checks the tombstone before the one-time legacy import. Without it, "clear never reimports" holds only for the current call: the next launch re-imports the untouched legacy key. (Deviation D-18 in `.mstar/specs/desktop-shell.md`: no persisted-secret readback, no plaintext-write fallback, and a deleted credential must not reappear.)
- The one-time legacy import reads the old macOS keychain entry via `/usr/bin/security find-generic-password` (no shell; the secret is never logged or returned) and validates/encrypts before switching stores; originals stay untouched.

### 8. Non-secret runtime metadata is validated and fail-closed

`window.nexusDesktop.runtime` (e.g. `localEndpoint`) is populated by main before the preload runs, fetched synchronously via `sendSync` and **validated** (non-secret `http(s)` URL); the bridge construction throws if main has not supplied valid metadata, and the SPA consumes the endpoint synchronously rather than racing an async status read. `onStatusChanged` returns an unsubscribe function (the SPA adapts it to its async contract), drops invalid/oversized frames, and never passes Electron event objects to the renderer.

### 9. The product preload exposes no generic capability

Only `{version: 1, runtime, invoke, onStatusChanged}` with the closed operation union — no raw `ipcRenderer`, no `require`, no env/Node handles, no proof/step/openCore IPC, and no channel/name/path execution primitive. Every added operation is a named, typed capability with its own payload map and bounds; “generic invoke” is the anti-pattern this shape exists to prevent.

## Why This Matters

- **Every check here has a weaker version that looks correct in review.** An unversioned envelope, an optional generation source, payload-only bounds, an `Accept: */*` fallback, substring parity checks, a dropped-but-not-disposed registration — each shipped plausible. The reusable lesson is procedural: for every validation, ask *what makes it vacuous* (defaulted input, optional source, bounds on the wrong artifact, drift checks that only compare substrings) and pin the negative case with a test that fails pre-fix.
- **The renderer is replaceable; the boundary is not.** The SPA is shared with browser mode; nothing about it should be able to name a native path or secret. A typed, closed bridge keeps the shared code honest.
- **Deny-by-default composes.** Navigation lockdown + one external predicate + CSP from active origin means an injected link cannot exfiltrate, auto-open, or reach a non-pinned endpoint.

## When to Apply

- Building or reviewing any Electron/webview host whose renderer ships a shared web app.
- Adding preload operations: check the closed union, payload map, bounds, sender path and failure-path totality before merge.
- Designing CSP/navigation for a packaged app that also supports a dev server.
- Any "parity" port of an old shell: keep the old shell's *observable* behavior (e.g. arbitrary http/https external links) while dropping its mechanisms (Tauri globals, PID-based process replacement).

## Examples

### Sender validation — the vacuous version and the bound version

```ts
// Before (vacuous): generation defaults to the registration-time capture,
// so a stale registration still passes its own check.
function assertDesktopEventSender(view, options = { getCurrentGeneration: () => view.generation }) { … }

// After: the live source is required; registration fails closed without it.
type RegisterDesktopIpcOptions = { getCurrentGeneration: () => number; … };
// bound=7, live=8 → stale_sender; { generation: 7 } alone → invalid_input.
```

### Response bounds — payload-only vs complete frame

```ts
// Before: measured `result` only — envelope overhead could push the frame past 1 MiB.
assert(Buffer.byteLength(JSON.stringify(result)) <= MAX_REQUEST_BYTES);

// After: build the complete envelope, measure it, and make the error path total.
const frame = { version: 1, request_id, ok: true, result };
if (utf8Bytes(frame) > MAX_REQUEST_BYTES) throw codedDesktopError('internal', …);
```

### SPA fallback — any Accept vs explicit navigation

```ts
// Before: `*/*` (fetch/XHR) also fell back to index.html → 200 for missing assets.
// After: only an explicit text/html media type is a navigation signal.
const isDesktopHtmlNavigation = (accept: string | undefined) =>
  !!accept && /\btext\/html\b/.test(accept); // `*/*` alone is false
```

## Evidence

- Contract/envelope/validators — `apps/desktop-electron/src/desktop-contract.ts`; IPC registration, sender check, admission, status frames — `src/desktop-ipc.ts`; scheme/CSP/navigation/external predicate — `src/protocol.ts`; product bridge — `src/preload.ts`; composition + wiring — `src/main.ts`.
- Credential store + exact-origin network hook — `src/connection-store.ts`, `src/desktop-network.ts`.
- Behaviour pins — `apps/desktop-electron/tests/desktop-security.test.mjs` (39 cases: versioning, generation, traversal incl. double-encoded and symlink escapes, admission caps, preload parity extraction), `desktop-host.test.mjs` (composition order, late-registration race, global lockdown), `connection-store.test.mjs` + `desktop-network.test.mjs` (redaction, durable clear, header isolation), `tests/lifecycle-harness.mjs`.
- Web seam — `apps/web/src/lib/nexus/desktop-bridge.ts`, `detect.ts` (bridge-only selection, `version === 1`), `desktop-capabilities.ts`; pins in `apps/web/src/lib/nexus/*.test.ts` and the setup-gate/wizard component tests.
- Companion docs — [daemon-ready-gate-pattern.md](daemon-ready-gate-pattern.md) (status/event-stream single source of truth), [gui-process-path-enrichment.md](gui-process-path-enrichment.md) (trusted PATH resolution for the detached service), [vite-daemon-proxy-boot-window.md](vite-daemon-proxy-boot-window.md) (dev proxy readiness).
