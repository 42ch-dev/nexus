---
module: apps/web
date: 2026-07-18
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
tags: [canvas, surface-extraction, additive-enum, wire-free, canvas-surface-adapter, timeline, projection]
applies_when:
  - "Extracting a new Canvas surface from an existing bundled surface (e.g., splitting Timeline from Outline+Timeline)"
  - "Adding a new CanvasSurfaceKind value that reuses shipped DTOs and routes without wire-contract changes"
  - "Implementing an additive-frontend iteration that must produce zero schema/codegen/daemon diff"
---

# Canvas Surface Extraction Pattern (from Bundled Surface)

**Track**: Knowledge (durable guidance, distilled from V1.122 P1 — extracting Timeline from the shipped `work-outline-timeline` surface).

## Context

The Nexus Canvas (`CanvasSurfaceKind`) ships surfaces as coupled types. Initially, Timeline was a **lane** inside the `work-outline-timeline` (Outline+Timeline) surface — bundled with chapter-relative events. To make Timeline the World-building hero, it needed to be extracted as a **peer surface** (`"timeline"`) without:

- Breaking the existing Outline surface (which keeps its chapter-relative timeline lane).
- Adding new schemas, daemon HTTP routes, daemon Rust code, or codegen output.
- Bumping the `@42ch/nexus-contracts` npm version.
- Composing Work-scoped data sources onto the World-scoped surface.

The extraction pattern documented here generalizes to any future surface extraction from a bundled surface (e.g., extracting a "Fork Graph" from World KB, or a "Compute Canvas" from Strategy).

## Guidance

### 1. Additive Enum Value

The extraction starts with a single additive change to `CanvasSurfaceKind`:

```ts
// Before: bundled
type CanvasSurfaceKind = 'strategy' | 'work-outline-timeline' | 'world-kb';

// After: extracted
type CanvasSurfaceKind = 'strategy' | 'work-outline-timeline' | 'timeline' | 'world-kb';
```

**Rules:**
- **Never remove** existing enum values. The bundled surface (`work-outline-timeline`) remains a valid kind — it keeps the chapter-relative timeline lane for Work entry.
- **Never rename** existing enum values. The extraction is additive only.
- Register the new kind in `CanvasShell` surface registry and `canvas-nav` resolve helpers.

### 2. Single Graph Source (No Join)

The extracted surface consumes **one** existing DTO as its sole graph payload. Do not join multiple DTOs — the extraction is a projection of existing data, not a new aggregation endpoint.

```ts
// The extracted surface's graph type is a single existing DTO
type TimelineGraph = WorldKbGraphResponse;   // V1.73 shipped
```

**Rationale:** Joining multiple DTOs in the adapter creates a dependency on the orchestrator fetching multiple endpoints, introduces N+1 risk, and couples the adapter to the orchestrator's fetch graph. One source keeps the adapter self-contained and testable.

### 3. Stable Factory + Mutable Context

Follow the V1.114 `CanvasSurfaceAdapter` recipe: create the adapter once with `useMemo([])` and a mutable `ctxRef` that updates every render without invalidating the projection memo.

```ts
declare function createTimelineCanvasAdapter(
  ctxRef: React.RefObject<TimelineAdapterContext>,
): TimelineCanvasAdapter;
```

### 4. Projection Mapping (Entities → Node Kinds)

Map the existing entities from the single DTO onto node kinds, using a **`layoutHint`** discriminator that is derived from shipped data (not a new field):

```ts
type TimelineNodeData = WorldKbEntityProjection & {
  layoutHint: 'event' | 'context';   // 'event' when block_type === 'event'
  occurredAtHint?: string;            // from body.attributes.occurred_at if present
};
```

**Rules:**
- The `layoutHint` is a client-side derivation, not a new DTO field.
- Each entity type maps to exactly one `layoutHint` value.
- Entities without the temporal signal cluster in a "temporal-unknown" group — do not fabricate chronology from non-temporal fields.

### 5. Write-Boundary Reuse

The extracted surface reuses the **existing write boundary** of the bundled surface's data source. It does not create new write routes:

| Write path | Reuse | Rationale |
|------------|-------|-----------|
| Entity edits | Existing `kb.patch_entity` (V1.73) | Same entity type (`WorldKbEntityProjection`) as World KB surface |
| Work-scoped writes | **NOT invoked** | The extracted surface is World-scoped; `timeline.patch_event` is Work-scoped |
| Relationship writes | **NOT invoked in MVP** | Read-only on extracted surface in initial extraction; can be enabled later |

**Writes are additive-invocation, not additive-routes:** the surface routes through an existing daemon endpoint. No new route registration, no new handler, no daemon Rust change.

### 6. Conflict Reuse

Reuse the existing conflict DTO from the original data source. Do not create a Timeline-specific conflict DTO:

```ts
// Reuse, not create
adapter.adaptConflict(error) => projects WorldKbConflictError (409) | WorldKbValidationError (422)
```

Conflict-modal copy is **source-flavored** (e.g., world-kb-flavored), not new-surface-flavored. The conflict resolution flow (keep draft → refetch canonical → offer Use-current / Reapply / Review-side-by-side) is reused verbatim.

### 7. Honest Empty-State

The extracted surface must handle sparse data honestly. If the single DTO returns zero entities of the relevant type, render an empty-state that:

- Explains what the surface is (the spine).
- Links to the peer surface where data is created (e.g., World KB).
- Does **not** fabricate data or silently redirect to another surface.

### 8. Wire-Contract-Free Verification Gate

Because the extraction is additive-frontend-only, verify `wire_contracts_changed: false` via an 8-point gate (see `wire-contracts-frozen-verification.md` for the full checklist):

1. `git diff --stat schemas/` → empty
2. `git diff --stat crates/nexus-contracts/` → empty (or only codegen-regenerated)
3. `git diff --stat packages/nexus-contracts/` → empty (or only codegen-regenerated)
4. `git diff --stat crates/nexus-daemon-runtime/src/api/` → empty (no new routes)
5. `pnpm run codegen` → zero diff under `**/generated/`
6. `jq '.version' packages/nexus-contracts/package.json` → unchanged
7. Search `schemas/` for the new surface kind name → no new schema entries
8. Search `schemas/` for the frontend-only enum → empty (no schema drift)

## Why This Matters

- **Preserves wire contract stability** — the extraction is additive-frontend-only, so Platform and existing consumers do not need to upgrade `@42ch/nexus-contracts` or update their daemon deployments.
- **Preserves the existing surface** — the bundled surface keeps its original behavior. Outline chapter-relative timeline events are not lost.
- **Reduces implementation risk** — reusing existing DTOs, routes, and conflict models means the extraction is a projection exercise, not a full-stack surface build.
- **Keeps the adapter testable** — a single DTO source with no join logic means unit tests can mock one payload.

## When to Apply

- Extracting a new Canvas surface from a bundled surface (e.g., Fork Graph from World KB, Compute Canvas from Strategy).
- Adding a new `CanvasSurfaceKind` value that must be wire-contract-free.
- Any additive-frontend iteration where the constraint is "no new schemas, no new daemon routes, no daemon Rust changes."

## Examples

### V1.122: Timeline extraction from Outline+Timeline

| Concern | Pattern instance |
|---------|-----------------|
| Additive enum | `"timeline"` added to `CanvasSurfaceKind` |
| Single graph source | `WorldKbGraphResponse` (V1.73) |
| Projection mapping | `block_type=event` → `TimelineEventNode` (layoutHint='event'); other entities → `TimelineKnowledgeEntryNode` (layoutHint='context') |
| Write-boundary reuse | `kb.patch_entity` only (V1.73 shipped) |
| Conflict reuse | `WorldKbConflictError` (409) + `WorldKbValidationError` (422) |
| Temporal honesty | `body.attributes.occurred_at` only; no fabricated chronology |
| Empty-state | "This World has no entities yet. Add characters, events, and places through World KB to populate the timeline." |
| Wire-free gate | 8-point verification: all pass |

### What Didn't Work (V1.122)

- **Composing Work-scoped timeline events:** `timeline.patch_event` events are chapter-relative (no World-level merge key) and would require N+1 fetches per bound Work. Explicitly excluded from the extraction.
- **Fork marker nodes:** attempted as timeline nodes but deferred — Fork data is reserved for an optional canvas-header badge. Fork create/merge UI is out of scope.
- **New edge types:** `ForeshadowEdge`, `RealizesEdge`, `ForkFromEdge` were considered but rejected — they belong to the Work outline timeline surface, not the World-scoped surface. Reuse existing `WorldKbEdgeData`.
- **New conflict DTO:** considered but rejected — reuse `WorldKbConflictError` / `WorldKbValidationError` from V1.73.