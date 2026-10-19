# Web UI Static Serve Contract

| Attribute | Value |
| --- | --- |
| **Status** | Normative — serving contract for the optional embedded web-UI static serve in `apps/nexus-service` (plan 20261019-web-static-serve, decisions D1–D6). |
| **Document class** | Contract |
| **Scope** | Optional static SPA serving on the existing `apps/nexus-service` HTTP(S) listener: enablement, pipeline placement, auth exemption boundary, path safety, response hygiene, and the Task-1 Step-4 test matrix. `/v1/*` behavior is out of scope except as the invariant this contract must not change. |
| **Coordinates with** | [daemon-api-surface-conventions.md](daemon-api-surface-conventions.md) (error envelope, §3), [header-key-csrf-defence.md](../../../knowledge/architecture-patterns/header-key-csrf-defence.md) (auth model rationale) |
| **Evidence** | `apps/nexus-service/src/{config,server,security,bind,routes,errors,index,main}.ts` @ `feat/web-static-serve` base |

---

## 1. Purpose

A browser pointed at the service port today receives only API responses. This contract defines the minimal, optional static serve of the built SPA (`apps/web/dist`) on the existing listener, with `/v1/*` admission bit-for-bit unchanged and no new auth scheme.

---

## 2. Enablement and dist resolution (D1)

`config.ts` gains `--web-dist <dir>`: `CliArgs.webDist?: string` (absolute via `resolve`, same treatment as `--tls-cert`, `config.ts:430`), `ServiceOptions.webDist?: string | null`, `ResolvedServiceConfig.webDist: string | null` (`config.ts:5`, `config.ts:23`).

`resolveServiceConfig` (`config.ts:369`) resolves the field through `resolveWebDist` (exported from `static-files.ts`):

```ts
function resolveWebDist(explicit: string | undefined | null): string | null;
```

- **Explicit string** — require `<dir>/index.html` to exist; otherwise `throw new Error("--web-dist requires <dir>/index.html to exist: <dir>")`. The throw propagates through `startService` and `main.ts` prints `[nexus-service] failed to start: …` with exit 1. Explicit config never silently no-ops.
- **`undefined` (auto-discovery)** — probe `fileURLToPath(new URL('../../../apps/web/dist/index.html', import.meta.url))`. Three levels up lands on the repo root from both `src/` (tsx) and `dist/` (compiled), the same module-relative pattern as `routes.ts:3-6`. Exists → root is its dirname; missing → `null` (disabled, no error).
- **`null`** — force-disabled hook for tests and embedders; the auto-discovery probe is repo-anchored, so without this hook the no-dist boot case would depend on whether the checkout happens to contain `apps/web/dist`.

When `webDist` is non-null, `startService` (`index.ts`) emits exactly one stderr line `console.error('[nexus-service] serving web UI from <root>')`, matching the `main.ts:55` stderr convention. Disabled-by-default-discovery is silent.

---

## 3. Pipeline placement and the auth exemption boundary (D2, D3)

The request pipeline in `createServiceServer` (`server.ts:288`) runs in this order:

1. `writeCors` + `OPTIONS` → 204 (`server.ts:292-298`)
2. `checkOrigin` → 403 `forbidden` on non-allowlisted `Origin` (`server.ts:300`)
3. Operator stop / discovery special routes with their own `checkApiKey` (`server.ts:335`, `server.ts:358`)
4. `matchRoute(method, pathname)`; no match → 501 `route_not_migrated` (`server.ts:371-375`)
5. `route.tier !== 'unguarded'` → `checkApiKey` (`server.ts:377-386`)

The static branch MUST be inserted inside the step-4 `if (!route)` block, and nowhere else:

```ts
if (!route) {
  if (staticHandler && !urlObj.pathname.startsWith('/v1/')) {
    await staticHandler(res, method, urlObj.pathname);
    return;
  }
  sendError(res, new HttpError(501, 'route_not_migrated', `Route is not migrated: ${urlObj.pathname}`), …);
  return;
}
```

**Exemption boundary (D2):** the branch runs after `checkOrigin` and is exempt from `checkApiKey` only — the same exemption the `unguarded` tier grants `/v1/daemon/runtime/cert-fingerprint` (`routes.ts:296-302`), reached one step earlier because no route matches. Browser navigations carry no `Origin` header, so the origin gate does not block them; CORS response headers are unchanged.

**Invariant:** every `/v1/*` request (matched or not) is admitted exactly as today. The branch condition excludes `/v1/` paths, matched routes never enter `if (!route)`, and the 501 body for unknown `/v1/*` stays byte-identical to `server.ts:373`. The static branch fires only when UI serving is enabled (§2); disabled mode falls through to the unchanged 501.

---

## 4. Behavior by request class (normative)

| Request | UI enabled | UI disabled |
| --- | --- | --- |
| `GET /` | 200 `index.html`; `Content-Type: text/html; charset=utf-8`; `Cache-Control: no-store` | 501 `route_not_migrated` (unchanged) |
| `GET /assets/<hashed>.js` (file hit) | 200; MIME per §6; `Cache-Control: public, max-age=3600` | 501 `route_not_migrated` |
| `GET /<spa-route>` (no file) | 200 `index.html` fallback; `Cache-Control: no-store` | 501 `route_not_migrated` |
| `GET /v1/daemon/daemon/status` | unchanged (unguarded route) | unchanged |
| `GET`/`POST` unknown `/v1/*` | 501 `route_not_migrated` envelope (unchanged) | 501 `route_not_migrated` |
| Traversal attempt (`/…`, `%2e%2e`, `%2E%2E/`, `%5c`, `%00`, absolute, symlink escape) | 404 `not_found` envelope; never 500; no filesystem text | n/a (never reaches static branch) |
| `HEAD /` (or any served path) | same status/headers as `GET`, empty body | 501 `route_not_migrated` |
| `POST`/`PUT`/etc. on non-`/v1/*` | 405 `method_not_allowed` envelope | 501 `route_not_migrated` (unchanged) |
| Any static request with non-allowlisted `Origin` header | 403 `forbidden` (`checkOrigin`, `server.ts:300`) | 403 `forbidden` |
| Explicit `--web-dist` naming a dir without `index.html` | startup fails before bind (§2); no listener | — |

All error bodies use the canonical envelope via the existing `HttpError` → `mapNativeError` → `sendError` path (`errors.ts`, `server.ts:455-461`): the static handler writes success responses itself and throws `HttpError(404, 'not_found', 'not found')` / `HttpError(405, 'method_not_allowed', 'method not allowed')`, so `X-Request-Id`, CORS, and envelope shape stay uniform.

---

## 5. Path safety (D5, trust boundary)

`static-files.ts` resolves the request path before any filesystem access:

1. `decodeURIComponent(pathname)` exactly once; malformed escape → 404. The decoded value is never decoded again.
2. Reject (404) any decoded path containing NUL (`\0`) or backslash (`\`) — backslash is refused on all platforms, per D5.
3. `path.resolve(root, decoded)`; `path.relative(root, candidate)` must not be empty-for-escape, must not start with `..`, and must not be absolute — else 404.
4. File hit: `realpathSync(candidate)` must remain under `realpathSync(root)` (computed once at handler creation) — symlink escape → 404.
5. Miss or non-file (including directory and ENOENT): serve the `index.html` fallback (200). No `ENOENT`/filesystem message ever reaches the client.

---

## 6. Response hygiene (D6)

- MIME map (exact set): `html js mjs css map svg png jpg jpeg gif ico json txt wasm woff woff2`; `html` → `text/html; charset=utf-8`, `js`/`mjs` → `text/javascript`, `json` → `application/json`, `wasm` → `application/wasm`; unknown extension → `application/octet-stream`.
- `X-Content-Type-Options: nosniff` on every static response.
- `Cache-Control`: `no-store` for `index.html` and every fallback response; `public, max-age=3600` for files under `/assets/` (Vite's content-hashed output); `no-store` for any other served file.
- No Range, ETag, or compression this round (D6; add only on evidence).

---

## 7. Interfaces

```ts
// static-files.ts (new)
export function resolveWebDist(explicit: string | undefined | null): string | null;
export type StaticFileHandler = (res: ServerResponse, method: string, pathname: string) => Promise<void>;
export function createStaticFileHandler(options: { root: string; fallback: string }): StaticFileHandler;
```

`fallback` is the absolute `index.html` path; the handler owns the full method matrix (GET/HEAD serve, others 405) so the wiring in `server.ts` is the single branch shown in §3. Consumes: existing `HttpError` envelope, `ServerResponse`; touches neither the auth chain nor `bind.ts`.

---

## 8. Test contract (Task 1 Step 4)

New HTTP test file in the `actor-http.test.mjs` harness style (`node:test`; build `tsc` first, import `dist/index.js`, `startService` on a fixed 18xxx port; fixture dist under `mkdtempSync` with `index.html`, `assets/app-test.js`, and a symlink-escape fixture). Every case below is mandatory:

**Enabled mode** (explicit `--web-dist` → fixture): (1) `GET /` → 200, `text/html; charset=utf-8`, `no-store`, `nosniff`, body = index bytes; (2) `GET /assets/app-test.js` → 200, `text/javascript`, `public, max-age=3600`, `nosniff`; (3) `GET /spa/route` → 200 index fallback, `no-store`; (4) `HEAD /` → 200, headers of (1), empty body; (5) `GET /v1/daemon/daemon/status` → 200 unchanged; (6) `GET /v1/daemon/nope` → 501 `route_not_migrated`; (7) `POST /v1/daemon/nope` → 501; (8) `GET /%2e%2e/%2e%2e/etc/passwd`, `GET /%2E%2E/`, `GET /%2e%2e%5c`, `GET /%00`, `GET /%2e%2e/web/dist/index.html` escaping root → each 404 `not_found`, no fs text; (9) `POST /` → 405 `method_not_allowed`; (10) keyed mode (`NEXUS42_DAEMON_API_KEY` set): `GET /` → 200 without the key, `GET /v1/daemon/daemon/status` → 401 without the key.

**Disabled mode** (`webDist: null`): boots clean; `GET /` → 501 `route_not_migrated`; `POST /` → 501; API unchanged.

**Startup failure**: explicit `--web-dist` naming a missing dir → `startService` rejects with the §2 message; no listener.

**Unit (`static-files.ts` resolver)**: `../`, `%2e%2e`, `%2E%2E/`, backslash, NUL, absolute path, and the symlink-escape fixture each resolve to refusal; `resolveWebDist` explicit-missing throws, auto-discovery-miss returns `null`.

---

## 9. Plan amendments

1. **D1 probe depth corrected.** The plan's "`apps/nexus-service/dist/` → `../../`" resolves to `<repo>/apps`, not the repo root; the probe would never find `apps/web/dist` and the feature would silently never enable. The correct derivation is `new URL('../../../apps/web/dist/index.html', import.meta.url)` — three levels lands on the repo root from both `src/` and `dist/` (layout evidence: `apps/nexus-service/{src,dist}/`; one-level precedent: `routes.ts:3-6`).
2. **D2 precedent stated precisely.** `cert-fingerprint` is not answered "before the auth chain"; it is an ordinary route at tier `unguarded` (`routes.ts:296-302`) that passes `checkOrigin` (`server.ts:300`) and skips only `checkApiKey` (`server.ts:377-386`). The static branch is exempt from the API-key gate only; the origin gate and CORS still apply. The security posture is identical, the rationale text in the plan should read "exempt from `checkApiKey`, after `checkOrigin`".
3. **`resolveWebDist` signature narrowed** from `resolveWebDist(config)` to `resolveWebDist(explicit: string | undefined | null)`, and `ServiceOptions.webDist` admits `null` as a force-disabled test hook — required by the plan's own constraint that the no-dist boot be test-verified, since auto-discovery is repo-anchored and would otherwise make that test depend on checkout state.
4. **404/405 emission path fixed**: static refusals are thrown as `HttpError` and emitted by the existing `catch`/`sendError` envelope (`server.ts:455-461`), not by a parallel static error writer — one envelope, one CORS/`X-Request-Id` path.
