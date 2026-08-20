---
module: crates/nexus-daemon-runtime
date: 2026-07-22
problem_type: architecture-pattern
category: architecture-patterns
severity: high
tags: [daemon, axum, matchit, routing, 404, orchestration]
applies_when: adding or changing parametric HTTP routes under crates/nexus-daemon-runtime; diagnosing empty-body framework 404 vs handler envelope 404
last_updated: 2026-07-23
---

# Daemon API matchit colon capture

**Track:** Knowledge (Bug -> durable routing invariant). Distilled from V1.132 P0 orch-load-404, the earlier setup-continue hotfix (`R-HOTFIX-404-PARAM-SYNTAX`), and V1.133 P0 full brace-param sweep.

## Context

Axum's router uses **matchit**. Path parameters must use **colon capture** (`:id`), not brace syntax (`{id}`). Brace patterns never match at runtime, so requests fall through to Axum's **framework 404** (empty body, `content-length: 0`). The web client then surfaces a generic `Request failed with status 404` - which looks like a client bug but is a daemon registration defect.

Creator routes were fixed earlier; orchestration presets/sessions/modules were fixed in V1.132 P0. V1.133 P0 completed the full sweep: ~34 routes total across agent-host, orchestration schedules, KB, narrative worlds, reading annotations, memory, works, worlds+KB, and findings.

## Guidance

1. **Register parametric routes with `:param` only** in `crates/nexus-daemon-runtime/src/api/mod.rs` (and any new route modules).
2. **Never use `{param}`** in Axum/matchit path templates in this crate.
3. **Distinguish failure signatures:**
   - Framework miss -> empty 404 body -> route pattern / registration defect.
   - Handler reached -> JSON envelope (`success:false`, `code:not_found` / `service_unavailable`) -> business/engine path.
4. **Regression:** add `TestServer` integration tests for each parametric surface (mirror `brace_param_route_registration_api.rs` which covers all 9 route groups).
5. **RCA before client changes:** curl the daemon loopback with the exact client URL. If empty 404, fix daemon routes first - do not rewrite client paths or invent wire changes.
6. **Edge case - verb suffixes:** matchit 0.7 rejects consecutive captures like `:operation_id:cancel` (`:a:b` pattern). When a route needs a param + verb suffix (e.g., `:cancel`, `:logout`), register the route with just `:param` and strip the verb in the handler (mirror `logout_creator` / `cancel_operation` patterns).

## What did not work

- Treating Network 404 as a web mapping bug when URLs already matched contracts.
- Assuming list routes (no params) being healthy proves detail routes are registered.
- Leaving residual brace routes outside the repaired surfaces (V1.132 P0 fixed 4 routes but left ~30 more; V1.133 P0 completed the sweep).
- V1.130 P3a tried classification-only fix (categorizing the 404 as an error type) without fixing the route registration - this was a false-Done.

## Evidence

- Guide: `p0-orch-load-404-rca.md`
- V1.132 P0 fix: colon capture for presets/sessions/modules/strategy reload in `api/mod.rs`
- V1.133 P0 fix: full sweep of all remaining ~30 brace-form routes + `:cancel` edge case + 8 router-level regression tests
- Residual: `R-V1132P0-QC2-S-001` resolved by V1.133 P0; `R-V1133P0-QC1-S-001` (nit: handler doc comments) remains open
