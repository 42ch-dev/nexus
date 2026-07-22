# Spec: Orchestration load repair

**plan_id:** `2026-07-22-v1.130-p3-orch-load-repair`  
**Status:** specify+clarify+plan locked (architect Seat 2)  
**Wave:** 3b · blocked_by: P1 (hard for implementation)

## Problem

Dogfood: Strategy detail「无法加载策略」; Sessions「无法加载会话」; Modules detail unloadable. V1.125 fixed the framing, but load success is still broken — the author sees load errors on a daemon that is running fine. The UI conflates "engine not running" with "load failed", so the author cannot tell a real failure from an idle engine.

## Goals

- RCA + fix Strategy canvas / Sessions / Modules detail on healthy daemon
- **Healthy daemon contract (Seat 1 concern #6):** daemon reachable + auth OK + engine running. If engine not running → honest empty / engine-unavailable state (NOT a load error)
- Must complete **before** Settings rehome moves Modules (P3b)

## Non-Goals

- IA moves (P3b); new orchestration features; engine-autostart (only fix load path — autostart is a separate decision)

## Architecture decision (locked)

- “Healthy” is a three-part request contract: daemon health succeeds, protected requests pass auth, and the endpoint-required engine/registry is present. The target endpoint is authoritative for engine/registry readiness; no aggregate health endpoint is added.
- Canonical `503 service_unavailable` caused by missing engine/registry maps to an honest unavailable/empty state. Network/TLS/timeout, 401/403, 404, malformed responses, and non-engine 5xx remain distinct load errors.
- Pages/query hooks own loading/empty/unavailable/error presentation; `BrowserClient` owns canonical envelope parsing; daemon handlers own status/envelope; `WorkspaceState` owns engine/registry availability.
- RCA covers Strategy list+detail, Sessions list, and Modules list+detail. Each row records request path, runtime preconditions, actual and expected status/body, and the narrowest root owner before edits.
- P3a may repair the shared client classifier or existing handlers, but it does not auto-start the engine, synthesize data, or move routes.
- Implementation is hard-blocked by P1 to preserve Wave 3 P2 ∥ P3a. Read-only RCA may be prepared earlier.

## Wire

- Locked expectation: `wire_contracts_changed: false`.
- If RCA proves a response shape is missing or incompatible, stop and amend the plan/spec before schema/codegen work.

## Acceptance

Per compass AC **编排 load (P3a)** section. Plan-level DoD maps T1–T5 → AC 编排 load. **Gate: must be green before P3b dispatch.**

## Risks

Daemon engine not started; locate_preset regressions; auth/tier gates.
