# P0 Spec — Profile/Creator create reliability + dialog error surface

> **Iteration:** `.mstar/iterations/v1.129/delivery-compass.md`
> **Status:** product-reviewed, architect-locked, writing-hygiene done (2026-07-21)
> **Plan:** `.mstar/plans/2026-07-21-v1.129-p0-profile-create-reliability.md`
> **SSOT:** `.mstar/status.json`

## Problem statement (user value)

**Symptom a manual tester recognizes:** Footer → **Add creator** → type a name → **Create** → long red error about URL/port/daemon/self-signed cert — and **no new profile** appears. This happens even when the tester just started the daemon and the rest of the app is talking to it fine.

That experience fails the author on two user-visible levels:

1. **Create never succeeds when it should.** The happy path is broken today: the UI claims to create a creator, but the backend never accepted that request. The author cannot start multi-profile work from the footer.
2. **When create truly cannot reach the daemon, the message still does not help.** The same multi-cause paragraph appears for "daemon not running," "wrong URL," "cert rejected," and other cases. There is no Retry that feels intentional, no path to Connection settings, and no honest "use the desktop app for this cert" path.

**Engineering RCA (for implementers; not the user promise):** web `createCreator` posts `POST /v1/daemon/creators` (`browser-client.ts:168-170`); daemon router has no matching route (`api/mod.rs:140-157, 569`) → SPA HTML fallback → parse/throw → generic `transport_unreachable` blob (`browser-client.ts:614-623`). Prior pattern: V1.119 `patch_creator` pool-attach (`e320e62d`).

## Root-cause hypothesis (to confirm in T1)

- **Daemon gap:** `POST /v1/daemon/creators` is unrouted. Mirror the V1.119 `fix: Setup Continue 404 on creator PATCH` pattern — the existing `patch_creator` handler attaches the workspace pool inside the handler; the new `create_creator` handler must do the same.
- **Web classification gap:** `BrowserClient.request()` collapses every `fetch` throw into one `transport_unreachable` `NexusClientError`. We need a small `TransportErrorKind` enum and per-kind copy + CTA.
- **Dialog UX gap:** `CreateCreatorDialog` calls `useCreateCreator().mutate()` and relies on the global toast from `useErrorToast`. There is no inline error state and no recovery affordance.

## Scope (in)

- **Happy-path create:** author can create a creator from the footer dialog; it persists and shows in the avatar row (daemon route + web client call path must agree).
- **Dialog failure surface (create dialog only):** when create fails for transport reasons, show classified headline + ≤2-line body + primary CTA (Retry / Open Connection Settings / Use Desktop App). Toast may remain secondary but must not be the only surface and must not dump the generic blob.
- **Classification primitive for P0:** introduce `TransportErrorKind` on `NexusClientError` in the web client (and Tauri client if it has a separate throw path) so the dialog can branch honestly. App-wide adoption is P1.
- **Tests the author path depends on:** daemon create integration test; classification matrix unit tests; dialog recovery tests per kind.

## Scope (out)

- **Connection settings page redesign** — deep-link only (`/settings/advanced#connection`).
- **OS-keychain / fingerprint re-pin** — deferred under `R-V192SEC-001`; desktop app is the cert recovery story.
- **App-wide transport UX** — **P1** (`transport-error-ux.md`); P0 stops at create-dialog + classification API.
- **Creator Controller / business widgets** — V1.128 P2 stub remains a stub.
- **"Switch to local daemon" as a separate product flow** — not a new wizard; local-mode recovery is Retry after `nexus42 daemon start`, or Open Connection Settings if the saved address is wrong.

## Interfaces

### Wire contract — `POST /v1/daemon/creators`

```http
POST /v1/daemon/creators
Content-Type: application/json
X-API-Key: <key>

{ "display_name": "<string>" }
```

**Response 201** (locked by Seat 2; reuse existing `CreatorDetail` shape):

```json
{ "creator_id": "<string>", "handle": null, "display_name": "<string>", "has_api_key": false, "has_cached_token": false, "is_active": false }
```

(`CreatorDetail` is already defined at `crates/nexus-daemon-runtime/src/api/handlers/creators.rs:50-57` — no new struct needed.)

**Errors:** canonical `NexusApiError` envelope (`BadRequest` for empty `display_name`; `Internal` for pool/DB failures). Reuse `nexus_daemon_runtime::api::errors::NexusApiError` — do **not** hand-roll a new envelope (per `crates/nexus-daemon-runtime/AGENTS.md` Error envelope single-source rule).

> **`wire_contracts_changed` verdict: `true` (locked Seat 2).** New daemon route `POST /v1/daemon/creators`. No new JSON Schema file is needed — the handler reuses the existing `CreatorDetail` struct already present in the generated contracts (`crates/nexus-contracts/src/generated/daemon_api/creators/`). The schema diff is the route entry only; no codegen change.

### Web `NexusClientError` extension (locked Seat 2)

`TransportErrorKind` is an **instance field** (`kind?: TransportErrorKind`) on `NexusClientError` — not a discriminated union of error subclasses. Rationale: smaller diff, stays compatible with existing `instanceof NexusClientError` checks, and the existing constructor in `errors.ts:34-45` needs only one new optional parameter.

**Classification algorithm (locked):** inspect the `fetch` throw + response in `BrowserClient.request()` (`browser-client.ts:612-623`). The catch block already constructs a `NexusClientError(0, 'transport_unreachable', message, { cause })`. Add classification before throw:

1. `baseUrl === ''` (local mode, no remote URL configured) → `daemon_down`
2. `cause instanceof TypeError` and message contains `fetch` or `Failed to fetch` → `network`
3. `cause instanceof DOMException` and `cause.name === 'AbortError'` → `timeout`
4. **Fallthrough to response inspection (for `http_fallback`):** when fetch does NOT throw but returns a 200 response with `Content-Type` containing `text/html`, re-classify as `http_fallback`. This must happen **before** the `if (!response.ok)` check — inject between lines 613 and 625.
5. If none of the above match, fall back to `unknown`.

```ts
export type TransportErrorKind =
  | 'network'        // TCP refuse, DNS, offline
  | 'tls'            // certificate failure (best-effort; browser hides precise reason → fallback to 'network')
  | 'timeout'        // explicit abort/timeout
  | 'http_fallback'  // daemon returned HTML (release-mode SPA fallback) — endpoint not routed
  | 'daemon_down'    // local-mode daemon not running (baseUrl === '')
  | 'unknown';

export class NexusClientError extends Error {
  readonly status: number;
  readonly code: string;
  readonly kind?: TransportErrorKind; // present iff status === 0 (transport unreachable)
  readonly cause?: string;
  // …
}
```

> **TLS fail-open note (locked):** Browsers hide the precise certificate rejection reason from JS. `tls` classification is **best-effort** via error-message substring matching (e.g., `ERR_CERT_AUTHORITY_INVALID`, `SSL`, `certificate`). When the throw does not carry detectable TLS signals, **fall back to `network`** — do not over-claim certificate rejection. Copy for `tls` (dialog table above) is informational ("This browser rejected…") and the recovery CTA is "Use Desktop App."

> **Tauri client parity:** `TauriClient extends BrowserClient` (`tauri-client.ts:88`) — shares the same `request()` method. No separate classification path needed.

> **`http_fallback` edge case (locked):** when the daemon returns `text/html` with a non-200 status (e.g., 404 HTML from a misconfigured reverse proxy), the existing `!response.ok` branch (line 625) already catches it as an HTTP error — not a transport error. Only status 200 + `text/html` is re-classified as transport-class. This is correct: 200 HTML means the fallback served a page successfully instead of JSON.

### Dialog UX contract (locked Seat 2)

Copy rules (product): **honest** (do not invent causes), **no unfair blame** (prefer "we could not reach…" over "you misconfigured…"), **one real next action** per kind. Do not restructure columns; wording only. The copy table below is a **shared source** — P1 consumes these same strings from the same locale keys.

| Failure kind | Headline | Body (≤ 2 lines) | Primary CTA | Secondary CTA |
|--------------|----------|------------------|-------------|---------------|
| `daemon_down` | Local daemon is not running | Start it with `nexus42 daemon start`, then try again. | **Retry** | — |
| `network` | Could not connect to the daemon at this address | Check the URL and port in Connection settings, or confirm the network can reach that host. | **Open Connection Settings** | Retry |
| `tls` | This browser rejected the daemon certificate | The web app cannot trust a remote self-signed certificate. The Nexus desktop app can store trust in the OS keychain. | **Use Desktop App** | Open Connection Settings |
| `http_fallback` | The app could not complete this request | The daemon answered with a page instead of an API response. Retry once; if it keeps happening, check the daemon status. | **Retry** | — |
| `timeout` | The daemon took too long to respond | The connection stalled or the daemon is busy. Retry in a moment. | **Retry** | Open Connection Settings |
| `unknown` | Could not reach the daemon | Something went wrong before a response arrived. Retry, or check Connection settings if it continues. | **Retry** | Open Connection Settings |

Notes for implementers / Seat 2:
- **Use Desktop App** is informational (web cannot launch the binary); body must say what to do next without a dead "Launch" promise.
- **http_fallback** secondary "Report issue" was dropped — no in-app report target yet; Retry-only avoids a fake CTA. Architect may restore if a real report URL exists.
- Localized via `shell` / `common` namespaces per `apps/web/AGENTS.md` i18n rules; EN + zh-CN day one.

### zh-CN tone guidance

> Applies to the dialog table above and all transport-error copy in P0/P1. The implementer (T4) adds zh-CN locale keys; do not translate now — the note below is a one-line directive for that task.

Calm, technical, user-respecting tone. Avoid blaming the user or implying negligence ("you misconfigured" → prefer "the daemon address could not be reached"). Mark any machine-translated string with a `// MT: needs review` comment in the locale file for human follow-up. The EN source strings above are already tone-checked; the zh-CN keys should match the same author-respecting register.

## Acceptance criteria

Each AC is pass/fail for a manual tester or automated test that mirrors the same observation.

- **AC-V1129-P0-1 (happy path):** Daemon running and reachable → **Add creator** → non-empty name → **Create**. **Pass:** new avatar in footer within one refresh cycle; still present after full reload. **Fail:** generic blob, error dialog, or profile missing after reload.
- **AC-V1129-P0-2 (failure classification):** Force `daemon_down`, `network` (wrong URL/port), and `http_fallback` (HTML instead of JSON). **Pass:** dialog shows the matching headline/body/CTA from the table above; **none** of the three shows the generic multi-cause blob. **Fail:** shared blob or mismatched kind copy.
- **AC-V1129-P0-3 (recovery):** **Pass:** Retry re-submits the same create payload after the environment is fixed; Open Connection Settings lands on `/settings/advanced#connection`; Use Desktop App does not navigate to a dead route (informational only). **Fail:** CTA no-ops or wrong destination.
- **AC-V1129-P0-4 (no regression):** Existing `client-context.test.tsx`, `browser-client.test.ts`, `use-toast.test.tsx`, and `footer-profiles.test.tsx` pass with no skips; new tests cover the kinds matrix. *Needs architect lock* only if Tauri throw-path divergence forces a different classification API shape.

## Test strategy

- **Daemon:** axum integration test in `crates/nexus-daemon-runtime` mirroring existing creator handler tests; assert 201 + JSON body + persisted row.
- **Web unit (classification):** feed each `fetchImpl` failure shape (TypeError for network, AbortError for timeout, response with `Content-Type: text/html` and 200 status for SPA fallback) → assert kind.
- **Web integration (dialog):** MSW or `fetchImpl` injection to drive each kind through the dialog → assert copy + CTA visible + click leads to right action.

## Risks / open questions (architect Seat 2 — locked)

1. ~~Should `POST /v1/daemon/creators` require an active workspace first, or bootstrap one if missing?~~ **Locked:** The handler must lazily attach the pool via `state.ensure_creator_pool()` on entry (mirroring `patch_creator` at `creators.rs:533-541`). If `active_creator_id` is set in config but the pool is not yet open (clean first run), attach before inserting. If no creator is active in config, the pool may be absent — still attempt the insert; if the pool is absent and the creator DB schema doesn't exist, return `NexusApiError::Internal` with a descriptive message.
2. ~~Does the new route need Tier-1 (API key) only, or also Tier-2 (active creator)?~~ **Locked: Tier-1 only.** You cannot require an active creator to create the first creator. Add the `.post(handlers::creators::create_creator)` to the existing `tier1_routes` `.route("/v1/daemon/creators", ...)` call — do NOT create a separate route registration that could conflict.
3. ~~SPA-fallback classification relies on `Content-Type` sniffing — confirm stable across Axum versions.~~ **Locked: stable across Axum 0.7.** The SPA fallback is served by `static_assets::serve_embedded_app` which sets standard MIME types via `tower-http`. `Content-Type: text/html` is the canonical signal. Tested by the classification unit test matrix (T3).
4. ~~Should `TransportErrorKind` live on `NexusClientError` (instance field) or as a discriminated union of error subclasses?~~ **Locked: instance field** (`kind?: TransportErrorKind`). See § Interfaces above for rationale and full classification algorithm.

## References

- Concrete bug evidence: `apps/web/src/lib/nexus/browser-client.ts:168-170, 614-623`
- Daemon router: `crates/nexus-daemon-runtime/src/api/mod.rs:140-157, 569`
- Prior similar fix: commit `e320e62d` — `fix: Setup Continue 404 on creator PATCH`; plan `.mstar/plans/2026-07-15-v1.119-setup-continue-unblock.md`
- Toast bridge: `apps/web/src/api/queries.ts:249-261`
- i18n rules: `apps/web/AGENTS.md` § i18n
- Error envelope rule: `crates/nexus-daemon-runtime/AGENTS.md` § Error envelope single-source rule
