# P0 orch-load-404 — RCA matrix (T1)

**plan_id:** `2026-07-22-v1.132-p0-orch-load-404`  
**base_sha:** `4c875e8670a4ecac8daea3fe6f0ed7a83371cf65`  
**captured:** 2026-07-22 (local dogfood host)  
**daemon:** `nexus42 daemon start --foreground` on `127.0.0.1:8420` (worktree build from `~/.cache/nexus-target/debug/nexus42`)

## Executive summary

| Rank | Owner | Confidence | Evidence |
|------|-------|------------|----------|
| **1** | **`crates/nexus-daemon-runtime` route registration** | High | Parametric routes registered with `{param}` never match at runtime → Axum **framework 404** (empty body). Same defect class as hotfix `2026-07-16-hotfix-setup-continue-404` / residual `R-HOTFIX-404-PARAM-SYNTAX`. |
| 2 | `apps/web` client | Ruled out | Client paths match locked contracts (`browser-client.ts`). Empty framework 404 surfaces as generic `Request failed with status 404` via `NexusClientError.fromBody`. |
| 3 | `apps/desktop` sidecar | Ruled out | Direct `curl` to loopback `:8420` reproduces without Tauri/desktop; sidecar defaults to port 8420. |

**Fix boundary (T2–T4):** Change daemon route patterns from `{id}` / `{session_id}` / `{module_id}` to `:id` / `:session_id` / `:module_id` (matchit colon capture). Add TestServer regressions mirroring `creators_route_api.rs`. Client and desktop changes not required unless RCA disproves after route fix.

## Fixture definitions

| Fixture | Meaning | How reproduced |
|---------|---------|----------------|
| **Healthy daemon** | HTTP up, orchestration engine wired | `nexus42 daemon start --foreground` → `GET /v1/daemon/runtime/health` → 200 |
| **Engine absent** | Daemon HTTP up, no orchestration engine | `WorkspaceState::new_for_testing(..., None)` in `error_envelope.rs` integration test |

## RCA matrix — healthy daemon (`127.0.0.1:8420`)

### List surfaces (no path params)

| Surface | Client call | Method + URL | Status | Body (truncated) |
|---------|-------------|--------------|--------|------------------|
| Health | — | `GET /v1/daemon/runtime/health` | **200** | `{"status":"ok","version":"0.1.0"}` |
| Strategy list | `client.listPresets()` | `GET /v1/daemon/presets` | **200** | `{"embedded":[...],"system":[...],"user":[{"id":"test",...}]}` |
| Sessions list | `client.listSessions()` | `GET /v1/daemon/orchestration/sessions` | **200** | `{"items":[...],"pagination":{"limit":100,"has_more":false}}` |
| Sessions list (filtered) | `client.listSessions({creator_id})` | `GET /v1/daemon/orchestration/sessions?creator_id=ctr_local18344183b9f9` | **200** | `{"items":[],"pagination":{...}}` |
| Modules list | `client.getComputeModules()` | `GET /v1/daemon/compute/modules` | **200** | `{"items":[{"module_id":"basic-combat",...}],"has_more":false}` |

### Detail / param surfaces (dogfood failure mode)

| Surface | Client call | Method + URL | Status | Body | Classification |
|---------|-------------|--------------|--------|------|----------------|
| Strategy canvas | `client.getPreset(id)` | `GET /v1/daemon/presets/test` | **404** | *(empty, `content-length: 0`)* | **Framework route miss** |
| Strategy canvas | `client.getPreset(id)` | `GET /v1/daemon/presets/user%2Ftest` | **404** | *(empty)* | **Framework route miss** |
| Strategy reload | `client.reloadPreset(id)` | `POST /v1/daemon/presets/test:reload` | **404** | *(empty)* | **Framework route miss** |
| Session detail | `client.getSession(id)` | `GET /v1/daemon/orchestration/sessions/_system.maintenance%3A1784697415520` | **404** | *(empty)* | **Framework route miss** |
| Module detail | `client.getComputeModule(id)` | `GET /v1/daemon/compute/modules/basic-combat` | **404** | *(empty)* | **Framework route miss** |
| Strategy patch | `client.strategyPatchState(...)` | `POST /v1/daemon/strategies/test/states/foo/patch` | **404** | *(empty)* | **Framework route miss** |

### Control — handler 404 vs framework 404

| Request | Status | Body | Meaning |
|---------|--------|------|---------|
| `GET /v1/daemon/presets/test` | 404 | empty | Framework (route never registered for real id) |
| `PATCH /v1/daemon/creators/ctr_local18344183b9f9` | 404 | `{"success":false,"error":{"code":"not_found","message":"Not found: Creator route ..."}}` | Handler reached (colon param routes work) |

## RCA matrix — engine absent

| Surface | Method + URL | Status | Body (truncated) | UI expectation |
|---------|--------------|--------|------------------|----------------|
| Sessions list | `GET /v1/daemon/orchestration/sessions?creator_id=ctr_test` | **503** | `{"success":false,"error":{"code":"service_unavailable","message":"...engine..."}}` | `UnavailableState` (`isOrchestrationEngineUnavailable`) |
| Presets list | `GET /v1/daemon/presets` | **200** | *(preset listing does not require engine)* | Normal list |
| Modules list | `GET /v1/daemon/compute/modules` | **200** | *(registry read does not require engine)* | Normal list |

**Evidence source:** `crates/nexus-daemon-runtime/tests/error_envelope.rs::service_unavailable_returns_canonical_envelope` (TestServer, no live daemon needed).

## Code-path correlation

### Daemon registration (defect)

`crates/nexus-daemon-runtime/src/api/mod.rs` documents the matchit rule and prior hotfix, but orchestration surfaces still use brace params:

```rust
// creator_routes — FIXED (colon capture)
"/v1/daemon/creators/:creator_id"

// preset_routes — BROKEN
"/v1/daemon/presets/{id}"
"/v1/daemon/presets/{id}:reload"

// orchestration_routes — BROKEN
"/v1/daemon/orchestration/sessions/{session_id}"

// compute_routes — BROKEN
"/v1/daemon/compute/modules/{module_id}"
```

Comment at `creator_routes()` (same file): *"Brace form (`{param}`) is a literal path segment and never matches real ids (framework 404)."*

### Client request construction (correct)

| Hook | File | Wire path |
|------|------|-----------|
| `usePresets` | `apps/web/src/api/queries.ts` | `GET /v1/daemon/presets` |
| `usePresetGraph` → `getPreset` | `apps/web/src/lib/canvas/use-strategy-data.ts` → `browser-client.ts` | `GET /v1/daemon/presets/${encodeURIComponent(id)}` |
| `useSessions` | `queries.ts` | `GET /v1/daemon/orchestration/sessions` |
| `useComputeModules` / `useComputeModule` | `queries.ts` → `browser-client.ts` | `GET /v1/daemon/compute/modules` / `.../modules/${id}` |

### Error surfacing

Empty framework 404 → `NexusClientError.fromBody` → message `Request failed with status 404` (`apps/web/src/lib/nexus/errors.ts`). Matches dogfood screenshot class from hotfix RCA.

## Network evidence limitation

Browser DevTools Network capture was **not** available in this implementer session (no Vite dev server on `:5173`). Evidence is **curl + code-path correlation** only. List/detail status codes above are authoritative for daemon behavior; UI would issue the same relative `/v1/daemon/*` paths per `browser-client.ts` / `vite.config.ts` proxy.

## Recommended fix boundaries

| Task | Proven failure | Fix owner | Scope |
|------|----------------|-----------|-------|
| **T2 Strategy** | `getPreset`, reload, strategy patch routes | daemon-runtime | `:id` / `:strategy_id` / `:state_id` route patterns + TestServer tests |
| **T3 Sessions** | List works on healthy daemon; engine-absent → 503 | daemon-runtime (verify) | Confirm list happy-path; fix `:session_id` if detail needed |
| **T4 Modules** | `getComputeModule` framework 404 | daemon-runtime | `:module_id` route pattern + TestServer test |
| **T5** | Regression | daemon-runtime + web | Router tests + optional MSW/dogfood Network attach |

## Commands (repro)

```bash
# Healthy daemon
nexus42 daemon start --foreground   # port 8420

curl -sS -w "\nHTTP:%{http_code}\n" http://127.0.0.1:8420/v1/daemon/presets
curl -sS -D - -o /dev/null http://127.0.0.1:8420/v1/daemon/presets/test   # empty 404
curl -sS -w "\nHTTP:%{http_code}\n" http://127.0.0.1:8420/v1/daemon/compute/modules/basic-combat

# Engine absent (integration test)
cargo test -p nexus-daemon-runtime service_unavailable_returns_canonical_envelope
```
