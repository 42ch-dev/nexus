# P0 orch-load-404 — Regression + dogfood evidence (T5)

**plan_id:** `2026-07-22-v1.132-p0-orch-load-404`  
**head_sha:** `3028319a5897ccc6e4ee87a22353e9b93c65459a`  
**captured:** 2026-07-22 (local dogfood host)  
**daemon:** rebuilt `~/.cache/nexus-target/debug/nexus42` → `nexus42 daemon start --foreground` on `127.0.0.1:8420`  
**prior RCA:** [p0-orch-load-404-rca.md](./p0-orch-load-404-rca.md) (T1 — pre-fix framework 404 on detail routes)

## Executive summary

| Check | Result |
|-------|--------|
| TestServer regressions (presets / sessions / modules) | **13/13 pass** — detail routes hit handlers, not framework empty-body 404 |
| Engine-absent 503 smoke (`error_envelope`) | **2/2 pass** — `service_unavailable` canonical envelope |
| Healthy daemon curl (list + detail) | **All 200** with `application/json` bodies |
| Handler 404 (unknown ids) | **JSON envelope** (`not_found`), not empty framework 404 |
| Browser Network tab | **Not captured** — Vite dev server not running in session |

**Verdict:** T2–T4 colon-param fix verified. Detail routes no longer return empty-body framework 404 on healthy daemon. `DONE_WITH_CONCERNS` for missing browser Network evidence (curl matrix + TestServer complete).

## TestServer regression suites

Run from worktree at `3028319a`:

```bash
cargo test -p nexus-daemon-runtime --test presets_route_api   # 5 passed
cargo test -p nexus-daemon-runtime --test sessions_route_api  # 5 passed
cargo test -p nexus-daemon-runtime --test modules_route_api   # 3 passed
cargo test -p nexus-daemon-runtime --test error_envelope      # 2 passed
```

Key assertions (proven failure mode from T1):

| Suite | Test | Proves |
|-------|------|--------|
| `presets_route_api` | `get_preset_by_id_hits_handler_not_framework_404` | `GET /v1/daemon/presets/:id` reaches handler |
| `presets_route_api` | `get_preset_unknown_returns_handler_json_404_not_empty_body` | Unknown preset → JSON `not_found`, not empty body |
| `sessions_route_api` | `get_session_by_id_hits_handler_not_framework_404` | `GET /v1/daemon/orchestration/sessions/:session_id` reaches handler |
| `sessions_route_api` | `sessions_without_engine_returns_503_not_404` | Engine absent → 503, not 404 |
| `modules_route_api` | `get_module_by_id_hits_handler_not_framework_404` | `GET /v1/daemon/compute/modules/:module_id` reaches handler |
| `error_envelope` | `service_unavailable_returns_canonical_envelope` | 503 `service_unavailable` envelope |

## Healthy daemon — curl matrix (`127.0.0.1:8420`)

### List surfaces

| Surface | Method + URL | Status | Content-Type | Notes |
|---------|--------------|--------|--------------|-------|
| Strategy list | `GET /v1/daemon/presets` | **200** | `application/json` | `embedded` + `system` + `user` arrays |
| Sessions list | `GET /v1/daemon/orchestration/sessions` | **200** | `application/json` | `items` + `pagination` |
| Modules list | `GET /v1/daemon/compute/modules` | **200** | `application/json` | `items` includes `basic-combat` |

### Detail surfaces (post-fix — was framework 404 in T1)

| Surface | Method + URL | Status | Content-Type | Body (truncated) |
|---------|--------------|--------|--------------|------------------|
| Strategy detail | `GET /v1/daemon/presets/novel-writing` | **200** | `application/json` | `{"id":"novel-writing","source":"embedded","yaml":"preset:\n  id: novel-writing...` |
| Session detail | `GET /v1/daemon/orchestration/sessions/_system.maintenance%3A1784697415520` | **200** | `application/json` | `{"session":{"sessionId":"_system.maintenance:1784697415520",...}}` |
| Module detail | `GET /v1/daemon/compute/modules/basic-combat` | **200** | `application/json` | `{"module_id":"basic-combat","name":"Basic Combat",...}` |

### Handler 404 controls (route matched, resource missing)

| Surface | Method + URL | Status | Body |
|---------|--------------|--------|------|
| Unknown preset | `GET /v1/daemon/presets/definitely-missing-preset-id` | **404** | `{"error":{"code":"not_found","message":"Not found: Preset 'definitely-missing-preset-id' not found"},...}` |
| Unknown session | `GET /v1/daemon/orchestration/sessions/definitely-missing-session-id` | **404** | `{"error":{"code":"not_found","message":"Not found: session definitely-missing-session-id"},...}` |
| Unknown module | `GET /v1/daemon/compute/modules/definitely-missing-module-id` | **404** | `{"error":{"code":"not_found","message":"Not found: module 'definitely-missing-module-id' not found"},...}` |

**T1 → T5 delta:** Detail routes that returned `HTTP/1.1 404` with `content-length: 0` (framework miss) now return **200** with JSON on healthy daemon, or **404** with JSON `not_found` when the handler is reached but the resource is absent.

## Engine absent — TestServer (no live daemon)

| Surface | Method + URL | Status | `error.code` |
|---------|--------------|--------|--------------|
| Sessions list (no engine) | `GET /v1/daemon/orchestration/sessions?creator_id=ctr_test` | **503** | `service_unavailable` |

Source: `crates/nexus-daemon-runtime/tests/error_envelope.rs::service_unavailable_returns_canonical_envelope`

## Residual

- **Browser Network:** not captured — no Vite + Control Room session in T5. QA may attach Network HAR/screenshots during manual dogfood; curl + TestServer satisfy plan DoD with explicit gap noted.
