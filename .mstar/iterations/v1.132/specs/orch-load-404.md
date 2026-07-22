# Spec: Orchestration load 404 repair

**plan_id:** `2026-07-22-v1.132-p0-orch-load-404`  
**Status:** plan locked (architect, 2026-07-22)  
**Wave:** 1 · Must-first

**Related documents**

- **Compass:** [delivery-compass.md](../delivery-compass.md) (AC-0, AC-0b)
- **Plan:** [2026-07-22-v1.132-p0-orch-load-404.md](../../../plans/2026-07-22-v1.132-p0-orch-load-404.md)
- **Prior context:** V1.130 P3a false-Done — [v1.130 delivery compass](../../v1.130/delivery-compass.md)

## Problem

Dogfood: Strategy detail, Sessions, and Compute Modules fail with generic `Request failed with status 404`. V1.130 P3a marked Done after **503 classification only**; load happy-path remains broken across iterations.

## Goals

- RCA with curl/Network evidence on dogfood host before code changes
- Fix Strategy list+detail, Sessions list, Modules list+detail on healthy daemon
- Engine-absent → UnavailableState (503); do not conflate with 404

## User Value

Authors and operators can actually use the 编排 (Orchestration) surfaces in Control Room — Strategy / Sessions / Compute Modules load on a healthy daemon instead of presenting a generic 404. This unblocks daily dogfood and restores trust in orchestration surfaces after the V1.130 P3a false-Done (classification-only).

## Non-Goals

- Engine auto-start
- IA moves
- Wire contract changes unless RCA proves shape gap (then amend plan)

## Architecture decision (locked 2026-07-22)

### Boundaries and ownership

- The web client owns request construction, exact route selection, response classification, and the visible UnavailableState. The locked candidate paths are `/v1/daemon/presets`, `/v1/daemon/orchestration/sessions`, and `/v1/daemon/compute/modules`; any deviation must be proven by RCA before changing code.
- `crates/nexus-daemon-runtime` owns route registration, host/port serving, and healthy-versus-engine-absent semantics. A healthy daemon must not return a generic 404 for the locked list/detail paths; engine absence is a distinct 503/UnavailableState outcome.
- `apps/desktop` sidecar/Overlay owns daemon process selection/start context and endpoint injection. A stale process, wrong host/port, or sidecar mismatch is a desktop-boundary failure, not evidence that the client route is wrong.
- RCA evidence is the authority: curl and browser Network records must include method, exact URL, status, and body for client, daemon, and desktop contexts. Fix only the proven boundary.

### Failure modes and closure rule

- 404 with a framework/router body is treated as a route/runtime miss until proven otherwise; do not mask it as an unavailable engine.
- 503 or an explicit engine-absent response maps to UnavailableState and is not a 404 regression.
- Classification-only tests, mocked happy paths, or a 503-only observation cannot close this plan. Done requires a regression for the proven failure mode plus dogfood curl/Network evidence for Strategy, Sessions, and Modules.

## Wire

- Locked verdict: `wire_contracts_changed: false`. Amend only if RCA proves a wire-shape gap; route, runtime, sidecar, and state-classification fixes do not change contracts.

## Acceptance

Maps to compass AC-0, AC-0b.

### Success criteria (dogfood)

- Strategy list renders; selecting a Strategy opens canvas detail (no 404).
- Sessions list renders with live status (no 404).
- Compute Modules list renders; selecting a Module opens detail (no 404).
- Engine absent → UnavailableState (503) shown honestly, not a 404.
- RCA matrix (curl + Network) attached as evidence; regression covers the proven failure mode.
- **Done gate:** no classification-only; dogfood Network/curl evidence required.
