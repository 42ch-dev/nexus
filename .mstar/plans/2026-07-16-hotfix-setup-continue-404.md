# Hotfix — Setup Continue HTTP 404 on `updateCreator`

| Field | Value |
|-------|-------|
| plan_id | 2026-07-16-hotfix-setup-continue-404 |
| tier | Hotfix |
| working_branch | fix/setup-continue-creator-patch-404 |
| merge_target | main |
| wire_contracts_changed | false |
| Execution mode | inline |

## Specify (min)

**Problem:** On desktop Setup Workspace step, Continue fails with inline error `Request failed with status 404` and soft helper copy. Author cannot advance to Done.

**Success:** Clean first-run / already-bootstrapped Continue persists profile display name and advances. Regression: router-level GET+PATCH `/v1/daemon/creators/{creator_id}` covered by TestServer.

**Non-goals:** Iteration V1.120; redesign of Setup IA; expanding Reset taxonomy.

## RCA (PM probe 2026-07-16)

Continue path (`setup-step-workspace.tsx`):

1. `ensureSetupBootstrap()` (Tauri) — writes `active_creator_id` — **succeeds**
2. `client.updateCreator(creator_id, { display_name })` → `PATCH /v1/daemon/creators/{id}` — **fails**

Live daemon (`nexus42` on `:8420`, config `ctr_local17da06edaffb`):

| Request | Result |
|---------|--------|
| `GET /v1/daemon/creators` | 200 |
| `GET /v1/daemon/creators/active` | 200 |
| `GET /v1/daemon/creators/{id}` | **404 empty body** (framework) |
| `PATCH /v1/daemon/creators/{id}` | **404 empty body** (framework) |
| `PUT /v1/daemon/creators/active` | 404 **JSON** `not_found` (handler ran) |

Empty body → web `NexusClientError.fromBody` fallback message exactly matches the screenshot.

**Not** V1.119 P0 lazy-attach (that surfaced as HTTP **409** `uninitialized`). Handler unit tests call `patch_creator` directly and **do not** exercise `create_router` path matching.

## Plan (min)

1. Confirm with `TestServer` / `create_router` whether `{creator_id}` routes match in-tree (suspect merge / `{id}:logout` / registration gap).
2. Fix daemon route registration or handler wiring so GET+PATCH `/v1/daemon/creators/{creator_id}` return handler responses (not framework 404).
3. Add regression TestServer tests for GET + PATCH by id (happy path + empty display_name validation still 400).
4. Smoke: Continue path after bootstrap can PATCH display name without 404.

## Tasks

- [x] T1: RCA confirm + fix creator `{id}` routing / wiring
- [x] T2: Router-level regression tests + scoped verify

## Review Gate Summary

**RCA:** `creator_routes()` registered GET/PATCH on `/v1/daemon/creators/{creator_id}` (curly-brace param). Matchit never matched those paths at runtime (Axum framework 404, empty body). Colon param `/v1/daemon/creators/:creator_id` matches; logout keeps `{creator_id}:logout` custom verb.

**Fix:** `crates/nexus-daemon-runtime/src/api/mod.rs` — register GET/PATCH on `:creator_id`; keep `{creator_id}:logout` for POST logout.

**Tests:** `tests/creators_route_api.rs` (TestServer GET/PATCH/400); `middleware.rs` tier1_get/patch_creator_by_id_hits_handler.

_(pending)_

## QA Gate Summary

- **Verdict:** Pass-with-manual-residual — `.mstar/sdd/2026-07-16-hotfix-setup-continue-404/review/qa.md`
- **Automated:** 5/5 cargo tests green (creators_route_api ×3 + middleware GET/PATCH ×2)
- **Manual residual:** Desktop Setup Workspace → Continue once after rebuilding/restarting daemon with `fix/setup-continue-creator-patch-404`

