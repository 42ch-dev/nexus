# V1.122 — Timeline Canvas Architecture (iteration-scoped)

> **Status:** Draft (Phase 1 architect seat 2). Locks the architecture for plan `2026-07-18-v1.122-timeline-first-canvas` (P1). Implements the locked direction: Timeline-centric World building as the primary Canvas selling point.
>
> **Promoted to:** `knowledge/architecture-patterns/canvas-surface-extraction-pattern.md`, `knowledge/architecture-patterns/world-vs-work-canvas-scope.md`, `knowledge/conventions/wire-contracts-frozen-verification.md` (compound V1.122).
>
> **Depends on:** P0 Draft overlay in `.mstar/specs/canvas-strategy-surface.md` (`CanvasSurfaceKind = "timeline"`).
>
> **Implements in:** plan `2026-07-18-v1.122-timeline-first-canvas`.
>
> **Companion docs:**
> - [`timeline-hero-product-spec.md`](./timeline-hero-product-spec.md) — product IA + user stories + ACs.
> - [`../../../specs/canvas-strategy-surface.md`](../../specs/canvas-strategy-surface.md) §3.3.1 — V1.114 `CanvasSurfaceAdapter` recipe.
> - [`../../../specs/entity-scope-model.md`](../../specs/entity-scope-model.md) §5.1.1 — `BlockType` taxonomy (incl. `event`).

## 1. Purpose

Lock the V1.122 P1 Timeline hero-surface architecture so the implementer (and the writing-specialist seat 3, then PM lock) have one normative reference for:

- Data composition (which existing Daemon route(s) feed `projectGraph`).
- Adapter interface (TypeScript signature conforming to V1.114 `CanvasSurfaceAdapter`).
- Write boundary (which existing structured operation(s) the surface invokes — and which it must NOT).
- Conflict policy (which existing DTO(s) the surface reuses).
- Honest empty-state and temporal-ordering rules.
- `wire_contracts_changed: false` verification contract.

## 2. Data composition (LOCKED)

### 2.1 Single graph source

The Timeline hero surface is **World-scoped**. `projectGraph` accepts **`WorldKbGraphResponse`** (V1.73 shipped) as its single graph payload.

**Endpoint (existing, unchanged):**

```
GET /v1/daemon/worlds/{world_id}/kb/graph
```

**Schema (existing, unchanged):** `schemas/daemon-api/canvas/world-kb/world-kb-graph-response.schema.json` → `WorldKbGraphResponse { entities[], source_anchors[], relationships[] }`.

The adapter does **not** fetch from any other endpoint as a graph source. The orchestrator may fetch a separate `WorldState` sidecar (§2.3) but does **not** pass it to `projectGraph`.

### 2.2 Projection mapping

| `WorldKbGraphResponse` field | Projection on Timeline canvas | Node/edge kind |
|-------------------------------|-------------------------------|----------------|
| `entities[block_type=event]` | When-axis events | `TimelineEventNode` (`layoutHint: 'event'`) |
| `entities[block_type!=event]` (character / scene / organization / item / info_point / conflict / ability / species / faction / magic_system / technology / deity / level / economy_tier / dialogue / beat / act) | Context clusters off the when-axis; may be positioned near related events via relationship edges | `TimelineKeyBlockNode` (`layoutHint: 'context'`) |
| `relationships[]` | Typed relationship edges (read-only in V1.122) | `Edge<TimelineEdgeData>` where `TimelineEdgeData = WorldKbEdgeData` |
| `source_anchors[]` | Grounding badge data on referenced nodes (optional rendering) | Node metadata; not a separate node kind |

**`block_type=event`** entities ARE World-scoped narrative events per `entity-scope-model.md` §5.1.1 — they ARE the "when-axis" content the Timeline hero surface projects. This is the architect's resolution to the product-manager's flagged High-severity data-composition risk.

### 2.3 Optional sidecar for header chrome

The orchestrator MAY additionally fetch `WorldState` for a Fork badge in the canvas header:

```
GET /v1/daemon/narrative/worlds/{world_id}   (V1.26 shipped)
```

- **Purpose:** read-only header badge ("Fork of `<parent_world_id>` at event `<forked_from_event_id>`") when `WorldState.is_fork === true`.
- **Not a timeline data source.** The sidecar MUST NOT be merged into `projectGraph`; it renders only in the canvas chrome.
- **Graceful degradation:** if the sidecar is not fetched (or returns an error), the badge is omitted. The Timeline surface remains fully functional.

### 2.4 Explicit non-composition (architect decision)

**Work-scoped outline timeline events (`timeline.patch_event` surface) are NOT composed onto the World Timeline surface in V1.122.** Rationale:

1. Work outline timeline events are **chapter-relative** (`realizes_chapter_id`, foreshadow edges between chapter-linked events) — they have no World-level merge key.
2. Composing them onto a World when-axis would require N+1 fetches per bound Work (one `/works/{work_id}/outline` per Work bound to the World), an unacceptable complexity + performance risk for MVP.
3. The product lock "honest empty-state" accepts sparse World timeline as valid MVP.
4. The Outline (Timeline-companion) surface keeps chapter-relative timeline affordances for Work entry (§5).

**World-scoped `TimelineEvent` HTTP route is deferred.** The domain `schemas/domain/timeline-event.schema.json` table (with `world_id`, `branch_id`, `sequence_no`, `caused_by_event_ids`, `affected_key_block_ids`) is currently reachable only via:

- `NarrativeGateway::get_timeline()` — internal Rust method.
- `nexus.timeline.recent.get` — host-tool capability (ACP/orchestration, not an HTTP route).

Promoting it to `GET /v1/daemon/worlds/{world_id}/timeline` is **out of V1.122 scope** (would require daemon Rust changes + a new external route — violates the Non-Goal "No new Daemon API routes"). Tracked under `DF-V1122-DEEPER-WB`.

## 3. Adapter contract (LOCKED)

### 3.1 TypeScript signature

```ts
// apps/web/src/components/canvas/timeline-canvas/timeline-canvas-adapter.tsx

import type { CanvasSurfaceAdapter } from '@/components/canvas/canvas-surface-adapter'; // V1.114 recipe
import type {
  WorldKbGraphResponse,
  WorldKbEntityProjection,
  WorldKbRelationshipProjection,
  WorldKbSourceAnchorProjection,
  WorldKbPatchEntityRequest,
  WorldKbConflictError,
  WorldKbValidationError,
} from '@42ch/nexus-contracts';

/** Single source — no wrapper, no join. */
type TimelineGraph = WorldKbGraphResponse;

/** Entity payload + adapter-owned layout hint + temporal signal. */
type TimelineNodeData = WorldKbEntityProjection & {
  /**
   * 'event' when block_type === 'event' (entity-scope-model §5.1.1).
   * 'context' for all other BlockType values.
   */
  layoutHint: 'event' | 'context';
  /**
   * Free-form temporal signal extracted from body.attributes.occurred_at.
   * Undefined when not declared by the KeyBlock body.
   */
  occurredAtHint?: string;
  /** Count of source anchors referencing this entity (from entity.source_anchor_count). */
  // (already present on WorldKbEntityProjection)
};

/** Verbatim reuse — no extension. */
type TimelineEdgeData = {
  relationshipId: string;
  relationType: WorldKbRelationshipProjection['relation_type'];
  customLabel?: string;
  confidence?: number;
  sourceAnchorIds: string[];
  symmetric: boolean;
  projectionDirection: 'stored' | 'symmetric_reverse';
};

interface TimelineCanvasAdapter
  extends CanvasSurfaceAdapter<TimelineGraph, TimelineNodeData, TimelineEdgeData> {
  surfaceKind: 'timeline';
  projectGraph(graph: TimelineGraph): { nodes: Node<TimelineNodeData>[]; edges: Edge<TimelineEdgeData>[] };
  nodeTypes: NodeTypes;    // registers TimelineEventNode, TimelineKeyBlockNode
  edgeTypes?: EdgeTypes;   // optional; reuses World KB relationship edge components where applicable
  layoutOptions: { direction: 'LR'; rankSep?: number; nodeSep?: number }; // opts into dagre left-to-right
  adaptConflict?(error: unknown): ConflictModalProps | null; // projects WorldKbConflictError / WorldKbValidationError
  renderInspector?(node: Node<TimelineNodeData>): ReactNode; // entity-inspector reusing World KB fields
  renderAltView?(): ReactNode; // sortable entity table
  summarizeGraph(graph: TimelineGraph): string; // includes ordering disclaimer when applicable
}

/** Stable factory per V1.114 §3.3.1 recipe. */
declare function createTimelineCanvasAdapter(
  ctxRef: React.RefObject<TimelineAdapterContext>,
): TimelineCanvasAdapter;
```

### 3.2 Conformance rules

- The adapter **MUST** conform to `CanvasSurfaceAdapter<TimelineGraph, TimelineNodeData, TimelineEdgeData>` from `specs/canvas-strategy-surface.md` §3.3.1.
- `TimelineCanvasAdapter` is the **only** adapter permitted to register `surfaceKind: "timeline"` in `CanvasShell` / `canvas-nav`.
- The adapter object **MUST** stay stable across renders (V1.114 §3.3.1 "stable factory that reads from a mutable `React.RefObject` context").
- The adapter **MUST NOT** introduce new edge types (`ForeshadowEdge`, `RealizesEdge`, `ForkFromEdge`) — those concepts belong to the Work outline timeline surface and are not part of `WorldKbEdgeData`. Timeline edge rendering reuses the V1.74 World KB relationship edge components.

### 3.3 Temporal positioning rule (honest)

- The adapter **MAY** position `TimelineEventNode` entities along the when-axis **only** when `body.attributes.occurred_at` is present.
- The adapter **MUST NOT** fabricate chronology from `updated_at`, `canonical_name`, `version`, `sequence_no`, or any non-temporal field.
- Entities without `occurredAtHint` cluster in a **temporal-unknown** group with honest copy ("Ordering inferred from available event data; not a canonical chronology.").
- `summarizeGraph` **MUST** include the disclaimer string when any event lacks `occurredAtHint`.

## 4. Write boundary (LOCKED)

### 4.1 Permitted operation

The Timeline surface edits World-scoped KeyBlock entities through:

```
POST /v1/daemon/worlds/{world_id}/kb/patch-entity    (V1.73 shipped)
```

→ `NexusClient.worldKb.patchEntity({ worldId, keyBlockId, expected_version, patch: WorldKbEntityPatch })`.

The `WorldKbEntityPatch` schema (`schemas/daemon-api/canvas/world-kb/world-kb-entity-patch.schema.json`, V1.73 shipped) allows: `title`, `body`, `aliases`, `block_type`. At least one property required.

### 4.2 Forbidden operations (V1.122)

The Timeline adapter **MUST NOT** invoke:

| Forbidden operation | Reason |
|---------------------|--------|
| `POST /v1/daemon/works/{work_id}/timeline/patch` (`timeline.patch_event`) | Work-scoped; operates on outline markdown, not World entities. Test must assert non-invocation. |
| `POST /v1/daemon/worlds/{world_id}/kb/patch-relationship` (`world_kb.patch_relationship`) | V1.122 MVP renders relationships read-only on Timeline; relationship edits remain on World KB surface. Test must assert non-invocation. |
| `POST /v1/daemon/worlds/{world_id}/kb/promote-candidate` (`kb.promote_candidate`) | Candidate workflow belongs to World KB surface. |
| Any raw-file write (`PUT` to a file route, Tauri `invoke` writing to disk) | `canvas-strategy-surface.md` §2 invariant. |

### 4.3 Flow

```
React Flow draft edit (e.g., inline title edit on a TimelineEventNode)
  → TimelineAdapter.patchEntity(node, patch)
    → NexusClient.worldKb.patchEntity({ worldId, keyBlockId, expected_version: node.version, patch })
      → daemon validates (entity exists, version matches, body conforms) → atomic persistence
        → on 200: refetch canonical WorldKbGraphResponse; merge into local React Flow state (V1.114 §3.3.1)
        → on 409 WorldKbConflictError: adaptConflict → conflict modal → keep draft → refetch → reapply/merge
        → on 422 WorldKbValidationError: adaptConflict → render validation_summary.errors[] in modal
```

## 5. Conflict policy (LOCKED)

- **Reuses** `WorldKbConflictError` (HTTP 409 — stale `expected_version`) and `WorldKbValidationError` (HTTP 422 — domain-rule failure) from V1.73.
- **No Timeline-specific conflict DTO.**
- Conflict-modal copy is **world-kb-flavored** — reuses the V1.73 entity-patch / V1.74 relationship-patch copy tokens.
- Conflict resolution flow (matches V1.73/V1.74):
  1. Keep the user's draft patch.
  2. Refetch the canonical `WorldKbGraphResponse`.
  3. Offer **Use current** (discard draft), **Reapply my edit** (re-submit against fresh `expected_version`), **Review side-by-side** (enabled only when draft and canonical touch non-overlapping fields).
- The `adaptConflict(error)` adapter method parses the canonical `ErrorResponse` envelope (`{ success: false, error: { code, message, details } }`) and projects `details` (when it matches `WorldKbConflictError` or `WorldKbValidationError` shape) to the existing conflict-modal props.

## 6. Outline companion (LOCKED architect decision)

The Outline (`work-outline-timeline`) surface **keeps its chapter-relative timeline lane unchanged** in V1.122.

- **Product allowed:** removing the Outline timeline lane after Timeline extraction.
- **Architect decision:** **keep**. Rationale:
  1. Outline is the **Work-entry** hero (V1.118, unchanged this iteration). Its chapter-relative timeline lane is the Work-projection counterpart to the World Timeline hero.
  2. Removing it would force authors writing chapters to switch surfaces to inspect chapter-linked events, regressing the V1.72 β slice.
  3. The two surfaces serve different scopes (World when-axis vs Work chapter-relative events); no data is duplicated.
- **Implementation rule:** the Outline adapter source file is **untouched** on the P1 branch. P1 Global Constraints → Regression gate asserts this.

## 7. Honest empty-state (architect-pinned copy requirements)

| World data state | Empty-state copy |
|------------------|------------------|
| Empty World (zero `entities[]`) | "This World has no entities yet. Add characters, events, and places through World KB to populate the timeline." + CTA button → World KB peer surface |
| Non-empty World, zero `block_type=event` entities | "This World's timeline is empty. Events you add through World KB or chapter extraction will appear here." + CTA → World KB peer |
| Partial temporal signal (some events lack `body.attributes.occurred_at`) | `summarizeGraph` MUST include: "Ordering inferred from available event data; not a canonical chronology." |

The adapter MUST NOT fabricate event ordering from `updated_at`, `canonical_name`, `version`, or any non-temporal field.

## 8. Fork markers (architect decision)

**No Fork marker nodes on the timeline in V1.122.** Fork create/merge UI is forbidden (Non-Goal).

- Fork data is reserved for an **optional canvas-header badge** from the `WorldState` sidecar (§2.3):
  - When `WorldState.is_fork === true`, render: "Fork of `<parent_world_id>` at event `<forked_from_event_id>`".
  - Read-only chrome — not a graph node.
- If the sidecar is not fetched or returns an error, the badge is omitted (graceful degradation). The Timeline surface remains fully functional.
- Fork create/merge UI is explicitly Non-Goal (`DF-V1122-FORK-UI`).

## 9. `wire_contracts_changed: false` verification contract

V1.122 P1 adds **only** a frontend `CanvasSurfaceKind = "timeline"` enum value + a new adapter module under `apps/web/src/components/canvas/timeline-canvas/`.

### 9.1 Permitted reuses (no diff)

| DTO / route | Source | Shipped |
|-------------|--------|---------|
| `WorldKbGraphResponse` | `schemas/daemon-api/canvas/world-kb/world-kb-graph-response.schema.json` | V1.73 |
| `WorldKbEntityProjection` | `schemas/daemon-api/canvas/world-kb/world-kb-entity-projection.schema.json` | V1.73 |
| `WorldKbRelationshipProjection` | `schemas/daemon-api/canvas/world-kb/world-kb-relationship-projection.schema.json` | V1.74 |
| `WorldKbRelationshipKind` | `schemas/daemon-api/canvas/world-kb/world-kb-relationship-kind.schema.json` | V1.74 |
| `WorldKbSourceAnchorProjection` | `schemas/daemon-api/canvas/world-kb/world-kb-source-anchor-projection.schema.json` | V1.73 |
| `WorldKbPatchEntityRequest` / `WorldKbPatchEntityResponse` / `WorldKbEntityPatch` | `schemas/daemon-api/canvas/world-kb/world-kb-*.schema.json` | V1.73 |
| `WorldKbConflictError` | `schemas/daemon-api/canvas/world-kb/world-kb-conflict-error.schema.json` (or wrapper) | V1.73 |
| `WorldKbValidationError` | `schemas/daemon-api/canvas/world-kb/world-kb-validation-error.schema.json` | V1.73 |
| `WorldState` | `crates/nexus-narrative/src/narrative_context.rs` (narrative read model) | V1.26 |
| `GET /v1/daemon/worlds/{world_id}/kb/graph` | `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs::get_graph` | V1.73 |
| `GET /v1/daemon/narrative/worlds/{world_id}` | `crates/nexus-daemon-runtime/src/api/handlers/narrative.rs::get_world` | V1.26 |
| `POST /v1/daemon/worlds/{world_id}/kb/patch-entity` | `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs::patch_entity` | V1.73 |

### 9.2 P1 Task 6 Step 4 — eight-point verification gate (architect-locked)

| # | Command | Expected |
|---|---------|----------|
| 1 | `git diff --stat schemas/` against `iteration/v1.122` base | **empty** |
| 2 | `git diff --stat crates/nexus-contracts/` | **empty** (codegen regenerates from unchanged schemas) |
| 3 | `git diff --stat packages/nexus-contracts/` | **empty** (codegen regenerates from unchanged schemas) |
| 4 | `git diff --stat crates/nexus-daemon-runtime/src/api/` | **empty** (no new routes / handlers) |
| 5 | `pnpm run codegen` on P1 branch → `git status` under `**/generated/` | **no untracked, no modified** |
| 6 | `jq '.version' packages/nexus-contracts/package.json` | matches pre-iteration version (no bump) |
| 7 | `rg -n '"timeline"' schemas/` | only existing `timeline-patch-event-request.schema.json` (Work-scoped) + `domain/timeline-event.schema.json` (untouched) |
| 8 | `rg -n 'CanvasSurfaceKind' schemas/` | **empty** (frontend-only enum; no schema drift) |

If any sub-step fails, the implementer **MUST STOP and escalate to architect** before marking the plan Done.

## 10. Open items for PM / writing-specialist seat

- **Writing-specialist (seat 3):** verify the empty-state copy strings (§7) align with the product voice in `timeline-hero-product-spec.md` and the pillar-framing docs; refine phrasing without changing the semantic triggers.
- **PM lock (after seat 3):** confirm that the architect-locked constraints in this doc are consistent with the final compass wording (no contradictions); record final approval in `delivery-compass.md` Iteration package section.

## 11. Non-goals (architecture)

- No new `schemas/` entries.
- No new daemon HTTP routes.
- No daemon Rust changes.
- No codegen drift.
- No `@42ch/nexus-contracts` version bump.
- No Work-scoped timeline event composition onto the World surface.
- No Fork marker nodes on the timeline.
- No new conflict DTO.
- No Timeline-specific edge types (no `ForeshadowEdge`, `RealizesEdge`, `ForkFromEdge`).
- No `world_kb.patch_relationship` write from the Timeline surface (deferred to post-MVP).
