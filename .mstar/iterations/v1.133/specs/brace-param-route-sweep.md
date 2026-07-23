# Spec: Brace-param route sweep

**Status:** product-reviewed, architect-locked, writing-hygiene done (2026-07-23)

## Product outcome

Creators can again open and mutate **ID-scoped** daemon resources from the web client (works, worlds, KB, findings, memory pending-review, agent sessions, schedules, reading annotations, narrative worlds). Those calls must not fail with a **framework path-match 404** when the ID is real. Unknown IDs may still return handler 404/4xx.

## Problem

Axum 0.7 + matchit 0.7 use `:param` for path captures. The brace form `{param}` is a **literal** path segment - it never matches real IDs and returns framework 404. V1.132 P0 fixed 4 route groups (presets, sessions, modules, strategy) but ~31 other daemon routes still use the broken brace form. The web client already sends real IDs; the product surface is false-Done.

## Scope

**In scope:** All `{param}` -> `:param` conversions in `crates/nexus-daemon-runtime/src/api/mod.rs` route path strings. Regression tests for each affected route group that prove real-ID requests are not framework-404.

**Out of scope:** Handler signature changes, wire contract changes, new routes, route refactoring, domain “entity missing” behavior redesign, residual burn-down (P1).

## Affected routes

See plan `2026-07-23-v1.133-p0-brace-param-route-sweep.md` § "Affected route groups" for the full table.

## Acceptance criteria

| ID | User / product check | Engineering check |
|----|----------------------|-------------------|
| **AC-0** | Known-good ID on each affected surface is not framework path-match 404 | Automated or scripted request with real ID |
| **AC-1** | — | Zero `{param}` brace-form route paths in `api/mod.rs` |
| **AC-2** | — | Regression test per affected route group (real ID → non-framework-404) |
| **AC-3** | — | `cargo test -p nexus-daemon-runtime` passes |
| **AC-4** | — | `cargo clippy -p nexus-daemon-runtime -- -D warnings` passes |

## Architecture decisions

- **No wire contract change:** Clients already send real IDs in URL paths. The route path syntax (`{param}` vs `:param`) is a framework-layer concern invisible to the wire contract.
- **No handler changes (33/34 routes):** `axum::extract::Path` works identically with `:param` captures.
- **Exception — `{operation_id}:cancel` (line 70):** This is the only route where a brace param immediately precedes a `:suffix` without `/` separator. Simple `{param}` → `:param` would produce `:operation_id:cancel`, which matchit 0.7 rejects (consecutive `:a:b` captures are not allowed). The fix mirrors the existing `logout_creator` pattern: change the route to `/:operation_id` and have the handler strip the trailing `:cancel` at the top of its body before UUID parsing (2-3 lines). The handler signature does not change.
- **Chapter `{n}` parameter:** Convert to `/:n` — handler already validates numeric input. In the nested chapter router (lines 448-456), `/:n` captures only until the next `/`; routes `/:n/outline` and `/:n/body` are unambiguous regardless of registration order.
- **`:cancel` handler scope:** The `cancel_operation` handler change is internal request-path parsing — not a wire contract or signature change. Clients already send `POST .../operations/<uuid>:cancel`.
