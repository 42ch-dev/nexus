# Spec: 创作 dual-state entry

**plan_id:** `2026-07-22-v1.130-p2-creator-dual-state`  
**Status:** specify+clarify+plan locked (architect Seat 2)  
**Wave:** 3a · blocked_by: P1

## Problem

Creator entry is nav-list driven, not the Open Design hub (create+list) / entity (chat+canvas) the author wants. Author cannot quickly "create a World" or "continue this World into a Work" from a single hub, and opening an entity does not give a thin Chat + action buttons alongside the canvas.

## Goals

- **Hub:** Left — Create World + 延续 World 开 Work; Right — Worlds/Works lists only (no templates)
- **Entity open:** Left — small Chat slot (honest/thin, grill B) + action buttons (Agent, Outline/Timeline, Findings, back-to-hub); Right — canvas/content
- **Chat floor:** newest 12 already-available local messages in chronological order + one disabled input row with honest V1.131 copy. If no typed history source exists, render honest empty history. No streaming / send / optimistic echo / tool-call render / multi-turn agent loop (→ V1.131).
- **Create World:** typed wire path via `POST /v1/daemon/worlds` (no honesty-degrade)
- Studio-first fixtures

## Non-Goals

- Full Agent Chat product (streaming, tool-call render, multi-turn ACP loop → V1.131); templates gallery

## Architecture decision (locked)

- UI data access stays behind `NexusClient`; App pages own query/mutation/navigation while Studio consumes only props-driven creator chrome through `@web-layout/*`.
- `POST /v1/daemon/worlds` accepts generated `CreateWorldRequest { title }`. The daemon trims/validates 1–200 characters, resolves the active creator, derives an ASCII kebab slug (`world` when normalization yields empty) with deterministic `-2`, `-3`, … collision suffixes, and defaults to private/manual/active.
- The handler delegates to `nexus_local_db::narrative_write::create_world`; neither React nor the HTTP client supplies creator ownership or writes persistence directly.
- Success is `201 CreateWorldResponse { world_id, status }`. The client invalidates World/Timeline list keys and navigates to `/worlds/{world_id}/timeline`.
- Continue World → Work reuses the existing generated `CreateWorkRequest.world_id`; no parallel Work DTO is introduced.

## Wire

- `wire_contracts_changed: true`
- Add request/response JSON Schemas under `schemas/daemon-api/worlds/`, generated Rust/TypeScript output, drift registration, daemon/client adapter coverage, and UI mutation coverage in the same implementation slice.

## Acceptance

Per compass AC **创作 hub / entity (P2)** section. Plan-level DoD maps T1–T4 → AC hub/entity.

## Risks

Create World domain rules; Work binding to World field availability.
