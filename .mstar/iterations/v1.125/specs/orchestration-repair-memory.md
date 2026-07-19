# Spec — Orchestration repair + Memory move (V1.125 P1)

**Status:** product-reviewed, architect-reviewed (seat 2), writing-hygiene done (seat 3)  
**Document class:** Iteration package working spec (not `{SPECS_DIR}` Master)  
**Compass:** [`../delivery-compass.md`](../delivery-compass.md) — AC-V1125-3, AC-V1125-4  
**Plan:** [`2026-07-19-v1.125-orchestration-repair-memory`](../../../plans/2026-07-19-v1.125-orchestration-repair-memory.md)  
**Wire contracts:** `wire_contracts_changed: false` — 503 classification is client-side over existing `ErrorResponse` envelope (`code: service_unavailable`).

## Problem

Strategies / Modules / Sessions show “无法加载此视图” on load failures; Sessions 503 `engine not available` looks like a crash; Memory lives under Creator; Orchestrator tab does not auto-select on deep links.

## Normative decisions

1. **Honest load UX** — Distinguish transport failure, HTTP 503 engine-unavailable, and empty idle lists. Never use generic `common.error.title` / “无法加载此视图” when the correct UX is empty or engine-unavailable. Author-facing copy must state what happened and what to do (Retry, start daemon, etc.).
2. **503 classification — client preferred** — Add a shared helper (e.g. `isOrchestrationEngineUnavailable` in `apps/web/src/lib/nexus/errors.ts` or query boundary):

   ```ts
   error instanceof NexusClientError
     && error.status === 503
     && (error.code === 'service_unavailable'
         || error.message.toLowerCase().includes('engine not available'))
   ```

   Sessions / Modules / Strategies list pages map this to **Unavailable** or **Empty** state with honest copy + Retry — not `ErrorState` with crash framing. **Do not change** `sessions.rs` / daemon orchestration handlers in P1 unless client classification fails in dogfood (daemon already returns correct 503 envelope).
3. **Strategies detail** — Keep Back on not-found/load-error; fix remaining `getPreset` / graph load failures; apply same 503 classification on canvas load where applicable.
4. **Modules** — Actionable error + Retry on real transport failures; empty list is EmptyState; 503 → unavailable copy.
5. **IA — group labels and order** — Orchestrator sidebar groups, top to bottom: **Memory** (first) → **Strategies** → **Runtime** (Sessions, Schedule) → **Compute** (Modules). Remove Memories group from Creator tab entirely.
6. **Tab sync — route-derived** — Derive active Orchestrator tab from `pathname` via prefix set:

   | Prefix | Tab |
   |--------|-----|
   | `/memory` | Orchestrator |
   | `/strategies` | Orchestrator |
   | `/sessions` | Orchestrator |
   | `/schedule` | Orchestrator |
   | `/modules` | Orchestrator |

   Implement with `useEffect`/`useMemo` on `pathname` in `sidebar.tsx` (do not rely on manual tab state alone). Manual Creator-tab clicks remain valid for Creator routes. Initial `useState` default may also derive from current path.

## Architecture notes (implementer)

| Surface | Error → UX mapping |
|---------|-------------------|
| Sessions list 503 | Unavailable / empty idle + Retry |
| Modules list 503 | Unavailable + Retry |
| Strategies list 503 | Unavailable + Retry |
| Strategies detail transport | ErrorState + Back (unchanged) |
| Empty 200 `[]` | EmptyState (all surfaces) |

## Acceptance (author-observable)

| ID | Author sees / does |
|----|-------------------|
| AC-V1125-3 | No false “无法加载此视图” on idle/503; honest empty/unavailable + Retry where needed |
| AC-V1125-4 | Orchestrator groups: Memory → Strategies → Runtime → Compute; no Creator Memories; deep links select Orchestrator tab |

## Out of scope

Harness rename; Capabilities UI; Schedule→cron role UX; daemon orchestration engine wiring changes.
