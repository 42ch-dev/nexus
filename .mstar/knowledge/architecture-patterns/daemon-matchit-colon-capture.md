---
module: crates/nexus-daemon-runtime
date: 2026-07-22
problem_type: architecture-pattern
category: architecture-patterns
severity: high
plan_id: 2026-07-22-v1.132-p0-orch-load-404
tags: [daemon, axum, matchit, routing, 404, orchestration]
applies_when: adding or changing parametric HTTP routes under crates/nexus-daemon-runtime; diagnosing empty-body framework 404 vs handler envelope 404
last_updated: 2026-07-22
---

# Daemon API matchit colon capture

**Track:** Knowledge (Bug → durable routing invariant). Distilled from V1.132 P0 orch-load-404 and the earlier setup-continue hotfix (`R-HOTFIX-404-PARAM-SYNTAX`).

## Context

Axum's router uses **matchit**. Path parameters must use **colon capture** (`:id`), not brace syntax (`{id}`). Brace patterns never match at runtime, so requests fall through to Axum's **framework 404** (empty body, `content-length: 0`). The web client then surfaces a generic `Request failed with status 404` — which looks like a client bug but is a daemon registration defect.

Creator routes were fixed earlier; orchestration presets/sessions/modules still used braces until V1.132 P0.

## Guidance

1. **Register parametric routes with `:param` only** in `crates/nexus-daemon-runtime/src/api/mod.rs` (and any new route modules).
2. **Never use `{param}`** in Axum/matchit path templates in this crate.
3. **Distinguish failure signatures:**
   - Framework miss → empty 404 body → route pattern / registration defect.
   - Handler reached → JSON envelope (`success:false`, `code:not_found` / `service_unavailable`) → business/engine path.
4. **Regression:** add `TestServer` integration tests for each new parametric surface (mirror `presets_route_api`, `sessions_route_api`, `modules_route_api`, `creators_route_api`).
5. **RCA before client changes:** curl the daemon loopback with the exact client URL. If empty 404, fix daemon routes first — do not rewrite client paths or invent wire changes.

## What did not work

- Treating Network 404 as a web mapping bug when URLs already matched contracts.
- Assuming list routes (no params) being healthy proves detail routes are registered.
- Leaving residual brace routes outside the repaired surfaces (track as open residual until swept).

## Evidence

- Guide: `.mstar/iterations/v1.132/guides/p0-orch-load-404-rca.md`
- Fix: colon capture for presets/sessions/modules/strategy reload in `api/mod.rs`
- Residual class: `R-HOTFIX-404-PARAM-SYNTAX` / `R-V1132P0-QC2-S-001` (remaining brace routes)
