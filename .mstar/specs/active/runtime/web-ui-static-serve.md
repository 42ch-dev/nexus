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
  const staticTarget = rawRequestPath(req.url);
  let decoded: string | null = null;
  try { decoded = decodeURIComponent(staticTarget); } catch { /* malformed escape → §5 404 */ }
  const normalized = decoded?.startsWith('/') ? collapseDotSegments(decoded) : null;
  const apiDomain =
    urlObj.pathname.startsWith('/v1/') ||
    normalized === '/v1' ||
    (normalized?.startsWith('/v1/') ?? false);
  if (staticHandler && !apiDomain) {
    await staticHandler(res, method, staticTarget);
    return;
  }
  sendError(res, new HttpError(501, 'route_not_migrated', `Route is not migrated: ${urlObj.pathname}`), …);
  return;
}
```

**Raw static path vs normalized guard:** the static branch receives `rawRequestPath(req.url)` — the raw request-target path with the query stripped and percent-escapes intact. WHATWG `new URL()` collapses `%2e%2e` dot segments during normalization, so passing `urlObj.pathname` would silently erase the encoded-traversal forms §4/§8 require to return 404 and hand the static boundary an already-normalized path. The single decoding boundary therefore lives inside the handler (§5 step 1), and `pathname` remains encoded at entry.

**`/v1/` exclusion over both path domains:** the exclusion test covers the normalized `pathname` and a read-only decode-and-collapse of the raw target. The `pathname` comparison uses the same view `matchRoute` uses, so a dot-segment form (`GET /foo/../v1/daemon/nope`) collapses to the API domain instead of slipping into the SPA fallback 200. The raw view is additionally required because `new URL()` preserves percent-escapes in `pathname` (`/v1/` ≠ `/%76%31/`): without it, `GET /%76%31/daemon/nope` enters the static branch, misses, and answers the SPA fallback 200 where the disabled mode answers the 501 envelope — a silent admission change. Decoding alone is insufficient for the mixed form: `GET /foo/../%76%31/daemon/nope` decodes to `/foo/../v1/daemon/nope`, which still starts with `/foo/..`, so the decoded value's dot segments are collapsed textually before the prefix test — split on `/` only; **empty and `.` segments are skipped before any fold, exactly as `path.resolve` ignores duplicate slashes**, so `..` consumes the real parent (`GET /a//../…` folds `a//..` to nothing, not to `/a`); `..` on an empty stack keeps the root; every other segment is verbatim. The result is single-`/`-separated with no trailing empty segment, so `/v1/` and `/v1/.` both collapse to the same bare `/v1` that `path.resolve` also produces. The admission test must therefore match that form exactly as well as by prefix — `normalized === '/v1' || normalized.startsWith('/v1/')` (with the same rule on `pathname` via its `/v1/` prefix test plus the collapsed bare form) — or equivalent paths land in different views and the `.`-collapsed `/v1` form falls into the SPA fallback 200 where BASE answers 501. This is the same `/`-segment collapse the handler's `path.resolve` performs (§5 step 3). A decoded `?` or `#` is therefore a literal filename character in this view too; re-parsing the decoded target as a URL would treat them as structure and let the admission view and the resolve view segment different strings (`GET /foo%3f/../%76%31/daemon/nope`). Decoded `\` and NUL need no admission special case — the handler refuses them (§5 step 2), so those requests stay in the rejection class. A decoded target that does not start with `/` is a no-match. The check decode-and-collapse feeds this prefix test only, never serving, so §5 step 1 stays the sole decode of the served path. A malformed escape fails the decode; the `pathname` half still governs, so a malformed target outside `/v1/` falls into the static branch and is refused 404 per §5, while a malformed `/v1/…` target keeps the API-domain 501.

**Guard divergence is not a serving hole:** the guard clamps `..` at the request root, so `GET /../dist/v1/y` folds to `/dist/v1/y` — non-API in both views — and enters the static branch. `path.resolve(root, …)` folds the same input back to `<root>/v1/y`, so the handler refuses any candidate it resolves under `<root>/v1` (§5) instead of relying on the guard to classify the path.

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
| `GET /%76%31/…` (percent-encoded `/v1/` prefix) | 501 `route_not_migrated` (check-decode, §3) | 501 `route_not_migrated` |
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
3. Remove exactly one leading request-root slash from the decoded value, reject (404) an absolute remainder (e.g. `//…` from `%2F…`), then `path.resolve(root, remainder)`; `path.relative(root, candidate)` must not be empty-for-escape, must not start with `..`, and must not be absolute — else 404. (Literal `path.resolve(root, decoded)` with an ordinary `/assets/x` discards `root`, so the root slash is stripped first; stripping every leading slash would lose the absolute-path refusal.)
3b. **Resolve-view `/v1/` refusal (serving authority).** After the containment-checked root-relative path is computed, a candidate whose normalized root-relative path — folded to lowercase (an ASCII fold suffices for the `v1` literal) with trailing dots/spaces trimmed per segment (Win32 name normalization) — is exactly `v1` or starts with `v1/` is refused (404 `not_found`, same class as NUL/backslash): it is neither served as a file nor allowed the SPA fallback. The handler's resolve view — not the admission guard — decides what is served, so this refusal survives guard rewrites and dist layouts that contain a `v1/` directory. It exists because the guard clamps `..` at the request root (`GET /../dist/v1/y` → `/dist/v1/y`, non-API in both guard views) while `path.resolve(root, '../dist/v1/y')` folds back to `<root>/v1/y`; without it, such a `v1/` file would bypass the `/v1/` API-key gate as static content (or answer a fallback 200 where BASE answers 501). Candidates whose resolve view still lands outside `v1` (e.g. `<root>/dist/v1/y`) keep normal file-or-fallback rules. The fold is a pure string decision made before any `stat`, so `GET /V1/y` and `GET /v1./y` are refused on every host regardless of filesystem case sensitivity or name normalization; refusing a literal `v1./x` file on a case-sensitive host is accepted over-refusal, as static asset names never carry trailing dots.

4. File hit: `realpath(candidate)` must remain under `realpathSync(root)` (computed once at handler creation) — symlink escape → 404. **The bytes are read from that verified realpath** (opened with `fs.promises.open` before any success header is committed), not from the pre-realpath candidate, shrinking the symlink-swap window between the containment check and the read. The fallback is served from its containment-checked realpath too.
5. Miss or non-file (including directory and ENOENT): serve the `index.html` fallback (200). The fallback is itself containment-checked once at handler creation — a symlinked `index.html` escaping the root is refused (404) on every fallback path, exactly like a direct hit, never served. No `ENOENT`/filesystem message ever reaches the client.

**Async I/O and absolute-form targets:** every per-request filesystem call is asynchronous (`node:fs/promises`); only the two creation-time containment checks stay synchronous. An absolute-form request target (`GET http://host/assets/x.js`) has its scheme and authority stripped in `server.ts` before the handler decodes the path (a one-clause alternative to an absolute-URI special case), so it resolves to the same file as its origin-form equivalent instead of missing into the fallback.

---

## 6. Response hygiene (D6)

- MIME map (exact set): `html js mjs css map svg png jpg jpeg gif ico json txt wasm woff woff2`; `html` → `text/html; charset=utf-8`, `js`/`mjs` → `text/javascript`, `json` → `application/json`, `wasm` → `application/wasm`; unknown extension → `application/octet-stream`.
- `X-Content-Type-Options: nosniff` on every static response — successes and the 404/405 refusal envelopes alike (set at static-handler entry so it survives the shared `sendError` path).
- `Cache-Control`: `no-store` for `index.html` and every fallback response; `public, max-age=3600` for files under `/assets/` (Vite's content-hashed output); `no-store` for any other served file. The tier derives from the **normalized** root-relative location of the served file (the containment-checked `relative(root, candidate)`), never from the raw first request segment, so a dot-segment alias like `GET /assets/%2e%2e%2findex.html` resolves to the root `index.html` and ships `no-store`.
- File bodies are **streamed** (`FileHandle.createReadStream()` piped to the response) with `Content-Length` from an `fstat` on that same opened handle, so the advertised length and the streamed bytes describe one file object; a large asset is never buffered whole on the shared API/SSE event loop.
- Success framing and caching headers are set **only after the opened file's first read returns**. Every pre-header static failure — including a read failure on the already-open file — therefore reaches the single `sendError` envelope without a stale `Content-Length`; a read failure once that first chunk is out destroys the response rather than appending an envelope to a short body.
- No Range, ETag, or compression this round (D6; add only on evidence).

---

## 7. Interfaces

```ts
// static-files.ts (new)
export function resolveWebDist(explicit: string | undefined | null): string | null;
export type StaticFileHandler = (res: ServerResponse, method: string, pathname: string) => Promise<void>;
export function createStaticFileHandler(options: { root: string; fallback: string }): StaticFileHandler;
```

`fallback` is the absolute `index.html` path; the handler owns the full method matrix (GET/HEAD serve, others 405) so the wiring in `server.ts` is the single branch shown in §3. `pathname` is the raw, still-encoded request-target path (query stripped); the handler performs the sole decode, then reads and streams the file with asynchronous filesystem calls. Consumes: existing `HttpError` envelope, `ServerResponse`; touches neither the auth chain nor `bind.ts`.

---

## 8. Test contract (Task 1 Step 4)

New HTTP test file in the `actor-http.test.mjs` harness style (`node:test`; build `tsc` first, import `dist/index.js`, `startService` on a fixed 18xxx port; fixture dist under `mkdtempSync` with `index.html`, `assets/app-test.js`, and a symlink-escape fixture). Every case below is mandatory:

**Enabled mode** (explicit `--web-dist` → fixture): (1) `GET /` → 200, `text/html; charset=utf-8`, `no-store`, `nosniff`, body = index bytes; (2) `GET /assets/app-test.js` → 200, `text/javascript`, `public, max-age=3600`, `nosniff`; (3) `GET /spa/route` → 200 index fallback, `no-store`; (4) `HEAD /` → 200, headers of (1), empty body; (5) `GET /v1/daemon/daemon/status` → 200 unchanged; (6) `GET /v1/daemon/nope` → 501 `route_not_migrated`; (7) `POST /v1/daemon/nope` → 501; (8) `GET /%2e%2e/%2e%2e/etc/passwd`, `GET /%2E%2E/`, `GET /%2e%2e%5c`, `GET /%00`, `GET /%2e%2e/web/dist/index.html` escaping root → each 404 `not_found`, no fs text; (9) `POST /` → 405 `method_not_allowed`; (10) keyed mode (`NEXUS42_DAEMON_API_KEY` set): `GET /` → 200 without the key, `GET /v1/daemon/daemon/status` → 401 without the key; (11) `GET /%76%31/daemon/nope` → 501 `route_not_migrated` (check-decode, §3) while the (8) encoded-traversal cases stay 404; (12) `GET http://127.0.0.1:<port>/assets/app-test.js` (absolute-form target) → 200 asset bytes; (13) fixture file with mode `000` → `GET /<file>` → 500 `internal` envelope whose `Content-Length`, when present, equals the body length (skipped when running as root).

**Disabled mode** (`webDist: null`): boots clean; `GET /` → 501 `route_not_migrated`; `POST /` → 501; API unchanged.

**Startup failure**: explicit `--web-dist` naming a missing dir → `startService` rejects with the §2 message; no listener.

**Unit (`static-files.ts` resolver)**: `../`, `%2e%2e`, `%2E%2E/`, backslash, NUL, absolute path, and the symlink-escape fixture each resolve to refusal; `resolveWebDist` explicit-missing throws, auto-discovery-miss returns `null`.

---

## 9. Plan amendments

1. **D1 probe depth corrected.** The plan's "`apps/nexus-service/dist/` → `../../`" resolves to `<repo>/apps`, not the repo root; the probe would never find `apps/web/dist` and the feature would silently never enable. The correct derivation is `new URL('../../../apps/web/dist/index.html', import.meta.url)` — three levels lands on the repo root from both `src/` and `dist/` (layout evidence: `apps/nexus-service/{src,dist}/`; one-level precedent: `routes.ts:3-6`).
2. **D2 precedent stated precisely.** `cert-fingerprint` is not answered "before the auth chain"; it is an ordinary route at tier `unguarded` (`routes.ts:296-302`) that passes `checkOrigin` (`server.ts:300`) and skips only `checkApiKey` (`server.ts:377-386`). The static branch is exempt from the API-key gate only; the origin gate and CORS still apply. The security posture is identical, the rationale text in the plan should read "exempt from `checkApiKey`, after `checkOrigin`".
3. **`resolveWebDist` signature narrowed** from `resolveWebDist(config)` to `resolveWebDist(explicit: string | undefined | null)`, and `ServiceOptions.webDist` admits `null` as a force-disabled test hook — required by the plan's own constraint that the no-dist boot be test-verified, since auto-discovery is repo-anchored and would otherwise make that test depend on checkout state.
4. **404/405 emission path fixed**: static refusals are thrown as `HttpError` and emitted by the existing `catch`/`sendError` envelope (`server.ts:455-461`), not by a parallel static error writer — one envelope, one CORS/`X-Request-Id` path.
