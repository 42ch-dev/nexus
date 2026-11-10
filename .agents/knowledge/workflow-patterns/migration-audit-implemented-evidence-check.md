---
title: Migration-audit issues need an implemented-evidence check before planning
category: workflow-patterns
track: knowledge
last_updated: 2026-10-18
created: 2026-10-18
status: active
date: 2026-10-18
problem_type: workflow_issue
module: harness-planning
severity: low
symptoms:
  - "An audit/migration-sourced issue is planned as business work but the demanded behavior already exists in code"
  - "A plan slot is consumed by a no-op implementation that cannot failing-first"
tags:
  - issue-store
  - audit
  - planning
  - false-premise
---

# Migration-audit issues need an implemented-evidence check before planning

## Context

Issues captured by migration/audit passes describe a gap as it existed at capture time. If a later iteration silently fixed the underlying behavior without closing the issue, the issue's acceptance text becomes a stale premise. Planning it as business work wastes a scale-budget slot and produces a no-op plan.

Nexus has now hit this twice in migration-audit catch-ups: I-000045 (post-codegen workspace check — already implemented by V1.182 `2f24d408a`, closed in v1.217) and I-000003 (refused/corrupt connection store indistinguishable from fresh install — already distinguished via `secure_store_unreadable`/`secure_store_corrupt` in `apps/desktop-electron/src/connection-store.ts`, closed in v1.218).

## Guidance

Before registering an audit-sourced issue into an iteration's plan set, run a code-evidence scout against the issue's acceptance text:

1. Extract the observable condition the acceptance demands (error codes, diagnostics, gates, tests).
2. Grep/read the current code for that condition — not the issue's description of past behavior.
3. If implemented: close as already-implemented with file:line + test-path evidence in the closure note (do not burn a plan slot). If partially implemented: restate the issue to the true residual before planning.

## Why This Matters

Turns a recurring planning failure mode (stale-premise plans) into a cheap pre-flight check; keeps the issue store's open set honest (open 27→22 in v1.217, further via v1.218 D3) and makes "close as already-implemented" auditable instead of improvised mid-flight.

## When to Apply

- Registering any issue whose `registeredAt` predates the last touching iteration, especially from migration/audit/residual-catch-up passes.
- Re-scoping deferred issues at direction lock (the autonomous ranking step).

## Examples

- v1.218 compass D3: I-000003 closed with `connection-store.ts:203-220` + `main.ts:479-525` + two test-file ranges as evidence, scout-verified before plan authoring.
- v1.217: I-000045 closed with `tooling/check-codegen-callsite-coverage.sh` + CI `verify-codegen` wiring (ci.yml:170) as evidence.
