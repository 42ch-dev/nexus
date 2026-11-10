---
module: apps/nexus-service
date: 2026-07-22
problem_type: architecture_pattern
category: architecture-patterns
severity: high
tags:
  - daemon
  - routing
  - 404
  - route-pattern
  - orchestration
applies_when:
  - adding or changing parametric HTTP routes under apps/nexus-service; diagnosing empty-body framework 404 vs handler envelope 404
last_updated: 2026-10-12
---

# Daemon API parametric route capture (RegExp capture groups)

**Track:** Knowledge (Bug -> durable routing invariant). Distilled from V1.132 P0 orch-load-404, the earlier setup-continue hotfix (`R-HOTFIX-404-PARAM-SYNTAX`), and V1.133 P0 full brace-param sweep. The original incident was on the retired Rust Axum/matchit daemon; the mechanism section below describes the current TypeScript service.

## Context

Path parameters must be a **real RegExp capture group** in the route pattern, not a framework template placeholder. A pattern that contains a literal `:id` or `{id}` matches those literal characters and never a real id, so the request matches no retained route and the service answers `HttpError(501, 'route_not_migrated', …)` — an unmatched path is refused without side effects (`apps/nexus-service/src/server.ts:371-375`). Historical (retired v1.193 P2) Axum/matchit instead fell through to an **empty-body framework 404** (`content-length: 0`); against the current service a client showing a generic `Request failed with status 404` is a client/proxy artifact, not a route registration signature.

The current owner is the standalone TypeScript service. `apps/nexus-service/src/routes.ts:96-113` defines one retained route family identity:

```ts
export interface DomainRoute {
  readonly method: 'GET' | 'POST' | 'PATCH' | 'PUT' | 'DELETE';
  /** Anchored pattern; capture groups are positional params. */
  readonly pattern: RegExp;
  readonly tier: RouteTier;
  readonly family: DomainFamily;
  /** Declared capture name: the first pattern group publishes under this key. */
  readonly capture?: 'sessionId' | 'operationId' | 'worldId';
  …
}
```

`matchRoute` (`apps/nexus-service/src/routes.ts:143-163`) tests `pathname.match(route.pattern)`, returns `params: match.slice(1)`, and publishes `match[1]` under the optional `capture` name. Parametric paths are therefore written as anchored RegExp literals with a `([^/]+)` segment, e.g. `/^\/v1\/daemon\/agent-host\/sessions\/([^/]+)$/` (`routes.ts:364`).

Historical (retired v1.193 P2): the Rust daemon used Axum 0.7 whose router is **matchit**, and required **colon capture** (`:id`) instead of brace syntax (`{id}`); the brace-form sweep that fixed ~34 routes across the daemon's `api/mod.rs` is preserved only as the origin of this 404 signature.

## Guidance

1. **Register a parametric route as an anchored `RegExp` with a capture group** in the owning family's `*_ROUTES` array (for example `EXECUTION_ROUTES` in `apps/nexus-service/src/execution.ts:127-146`). A single path segment param is `([^/]+)`; `matchRoute` hands the groups to the handler as positional `params`.
2. **Never write a template placeholder inside the pattern.** `/^\/v1\/…\/:id$/` and `/^\/v1\/…\/\{id\}$/` both match the literal characters and never a real id. Imagine the old Axum/matchit colon convention only as historical context - do not carry it into a TypeScript pattern.
3. **Distinguish failure signatures:**
   - Framework miss (no pattern matched) -> `501 route_not_migrated` JSON envelope (`apps/nexus-service/src/server.ts:371-375`) -> route pattern / registration defect. (Historical Axum/matchit miss produced an empty 404 body.)
   - Handler reached -> JSON envelope (`success:false`, `code:not_found` / `service_unavailable`) -> business/engine path.
4. **Regression:** cover each parametric surface with a `TestServer` integration check that curls the exact client URL against the daemon loopback.
5. **RCA before client changes:** curl the daemon loopback with the exact client URL. `501 route_not_migrated` is **not** proof of a matcher miss — a matched handler deliberately returns the same code when it refuses an unsupported operation or profile (e.g. the registered operations route `apps/nexus-service/src/routes.ts:420-428` → `executeProviderOperation` throws `routeNotMigrated` for domain-only execution (`apps/nexus-service/src/provider.ts:452`) and for unsupported Character capture (`:456-462`), via `routeNotMigrated` → `HttpError(501,'route_not_migrated',…)`, `apps/nexus-service/src/errors.ts:67-73`). Before changing registration, check whether a matched handler refused; only then fix the daemon route — do not rewrite client paths or invent wire changes.
6. **Edge case - verb suffixes:** the retired matcher could not express a param followed by a verb suffix (`:a:b` was rejected); the current RegExp pattern has no such limit. Encode the suffix directly, e.g. `/^\/v1\/daemon\/agent-host\/sessions\/([^/]+)\/operations$/` (`routes.ts:422`), and read the id from `params[0]`.

## What did not work

- Treating Network 404 as a web mapping bug when URLs already matched contracts.
- Assuming list routes (no params) being healthy proves detail routes are registered.
- Leaving residual brace/colon routes outside the repaired surfaces (V1.132 P0 fixed 4 routes but left ~30 more; V1.133 P0 completed the sweep).
- V1.130 P3a tried classification-only fix (categorizing the 404 as an error type) without fixing the route registration - this was a false-Done.

## Evidence

- Contract + matcher: `apps/nexus-service/src/routes.ts` (`DomainRoute`, `matchRoute`).
- Registration examples: `apps/nexus-service/src/execution.ts:127-146` (`EXECUTION_ROUTES`), `apps/nexus-service/src/routes.ts` (`agent-host` families).
- Historical (retired Rust Axum/matchit, deleted v1.193 P2): the V1.133 P0 full sweep of ~30 brace-form routes + `:cancel` edge case + 8 router-level regression tests in the former daemon crate; the `brace_param_route_registration_api.rs` test and `api/mod.rs` are gone with that crate.
- Residual: `R-V1132P0-QC2-S-001` resolved by V1.133 P0; `R-V1133P0-QC1-S-001` (nit: handler doc comments) remains open
