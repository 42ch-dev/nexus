# V1.122 — Timeline hero product spec (iteration-scoped)

> **Status:** Draft (Phase 1 product-manager). Implements the locked direction: Timeline-centric World building as the primary Canvas selling point.
>
> **Depends on:** P0 Draft overlay in `.mstar/specs/canvas-strategy-surface.md` (`CanvasSurfaceKind = "timeline"`).
>
> **Implements in:** plan `2026-07-18-v1.122-timeline-first-canvas`.

## Problem

1. Timeline is a **lane** inside Outline+Timeline (`CanvasSurfaceKind = "work-outline-timeline"`), not a peer surface — authors cannot treat world history as a first-class instrument.
2. **World entry** today opens **World KB** (`/worlds/:worldId/kb`) — entity graph first, not the "when" axis.
3. Domain model already says World+Timeline are the spine (`CONCEPTS.md`); UI inverts that for World entry.

## Target users

- **Worldbuilders / novelists** building history, events, and structured world state before or alongside chapter prose.
- **Authors returning to a World** who need orientation on the timeline, not a relationship graph dump.

## User stories

1. **As an author**, when I pick a World from the Worlds list, I land on a **Timeline canvas** so I immediately see the World's when-axis.
2. **As an author**, I can open **Outline / World KB / Strategy** as peer surfaces without losing my World context.
3. **As an author**, when I open a **Work**, I still land on **Outline** (chapter planning) — Timeline does not hijack writing entry.
4. **As an author**, if the World has little or no timeline data yet, I see an **honest empty-state** that explains Timeline as the spine (not a blank fail or silent redirect to World KB).
5. **As an author**, I can edit timeline/KeyBlock data only through **structured operations** (no raw file edits from the webview).

## Product locks (non-negotiable for P1)

| Lock | Value |
|------|--------|
| Hero context | **World entry** only |
| World default route | `/worlds/:worldId` → Timeline |
| Worlds list pick-target | Timeline (replace today's `/kb` default) |
| Work default route | `/works/:workId` → **Outline** (unchanged, V1.118) |
| Peer surfaces | Strategy, Outline (Timeline-companion), Timeline, World KB |
| Wire contracts | `wire_contracts_changed: false` |
| Fork authoring | Markers only if data exists; **no** create/merge UI |
| Compute | No compute-on-timeline |

## Information architecture

### Today

```
Worlds list → /worlds/:id/kb     (World KB canvas)
Works list  → /works/:id/outline (Outline canvas)
Outline surface bundles timeline lane (chapter-relative events)
```

### After V1.122

```
Worlds list → /worlds/:id/timeline  (Timeline canvas — HERO)
              ├ peer → /worlds/:id/kb
              └ peer → Strategy / Outline (when work context available)

Works list  → /works/:id/outline    (UNCHANGED)
```

Exact path segment (`timeline` vs index redirect) is implementer choice; **product acceptance** is: World pick and World index show Timeline, not World KB.

### Spine vs projection (author-facing copy guidance)

| Term in UI | Means |
|------------|--------|
| Timeline | When-axis of the **World** — events, KeyBlocks realized in time, Fork markers |
| Outline | Structure of a **Work** — volumes/chapters/scenes; may still show chapter-linked events as companion |
| World KB | Entity graph of the World — characters, places, relationships |

Do not label Outline as "the timeline product." Do not remove World KB.

## Projection content (product intent)

Minimum viable hero content (read projection):

| Node / edge | Source intent | MVP bar |
|-------------|---------------|---------|
| Timeline event | Narrative events on the when-axis | Show if available from existing APIs |
| KeyBlock-on-timeline | KeyBlocks with temporal realization | Show if graph/timeline data supports positioning |
| Fork marker | Existing Fork points | Read-only markers if data exists |
| foreshadow / realizes / fork-from edges | Existing relationship types | Render when present; omit when absent |

**Empty World:** show empty-state with short explanation + CTA to peer World KB or existing create flows (no new Fork create UI). Empty-state is **success**, not failure.

**Architect maps** Work-scoped `timeline.*` routes vs World-scoped KB routes; product does not require new Daemon routes to pass MVP.

## Acceptance criteria (product → maps to compass)

| ID | Criterion | Evidence |
|----|-----------|----------|
| AC-V1122-5 | Peer `"timeline"` surface + World default = Timeline | Route/nav tests + type union |
| AC-V1122-6 | Structured writes only; no wire churn | Write tests + schemas diff empty |
| AC-V1122-7 | Builds/tests green | pnpm logs |
| AC-V1122-8 | Peers reachable; Work → Outline preserved | Nav + work-route tests |
| AC-V1122-9 | Demo path + empty-state + light/dark screenshots | Screenshot pack on P1 plan |

## Non-goals (product)

See delivery compass Non-Goals. Local emphasis:

- No Work-entry flip to Timeline
- No Outline removal
- No Fork create/merge
- No compute-on-timeline
- No new Daemon routes for hero completeness

## Demo script (PMF)

1. Launch app (daemon healthy, Profile active).
2. Creation → **Worlds**.
3. Pick a World that has some KB/timeline data (or an empty World to verify empty-state).
4. **Expect:** Timeline canvas is primary view; when-axis visible or honest empty-state.
5. Switch to **World KB** peer → entity graph still works.
6. Open a **Work** → still lands on **Outline**.
7. Capture light + dark screenshots for AC-V1122-9.

## Open for architect (not product re-decisions)

1. Exact composition of `projectGraph` inputs (which GET routes, how Work-scoped events join World).
2. Whether Outline surface keeps an internal timeline lane after extraction (product allows companion; architect decides complexity).
3. Conflict modal flavor (outline-flavored vs world-kb-flavored vs new timeline-flavored copy) — prefer reuse.

## Architecture (architect seat 2 — LOCKED)

> Full contract: [`timeline-canvas-architecture.md`](./timeline-canvas-architecture.md). This section reproduces the product-relevant invariants for cross-reference.

### Data flow

```
Worlds list pick / /worlds/:worldId route
  └─ Timeline page orchestrator
      ├─ NexusClient.worldKb.getGraph(worldId)  →  GET /v1/daemon/worlds/{world_id}/kb/graph  (V1.73, sole graph source)
      │     → WorldKbGraphResponse { entities[], relationships[], source_anchors[] }
      └─ NexusClient.narrative.getWorld(worldId)  →  GET /v1/daemon/narrative/worlds/{world_id}  (V1.26, OPTIONAL sidecar)
            → WorldState { is_fork?, parent_world_id?, forked_from_event_id?, ... }  // header badge only
  └─ TimelineCanvasAdapter.projectGraph(graph)  →  React Flow { nodes, edges }
        - entities[block_type=event] → TimelineEventNode (when-axis)
        - entities[block_type!=event] → TimelineKeyBlockNode (Context cluster)
        - relationships[] → edges (WorldKbEdgeData, read-only V1.122)
        - source_anchors[] → grounding badges on referenced nodes
```

**Single source of truth:** `WorldKbGraphResponse` only. No Work-scoped join, no new HTTP route.

### Adapter contract

`TimelineCanvasAdapter implements CanvasSurfaceAdapter<WorldKbGraphResponse, TimelineNodeData, WorldKbEdgeData>` (V1.114 recipe). See `timeline-canvas-architecture.md` §Adapter contract for the TypeScript signature.

### Write boundary

- **World-scoped entity edits only**, through `POST /v1/daemon/worlds/{world_id}/kb/patch-entity` (V1.73) → `NexusClient.worldKb.patchEntity(...)`.
- **NOT invoked from Timeline surface:**
  - `timeline.patch_event` (Work-scoped; not applicable to World entities).
  - `world_kb.patch_relationship` (V1.122 MVP renders relationships read-only on Timeline; relationship edits remain on the World KB surface).
  - `kb.promote_candidate` (candidate workflow belongs to World KB surface).
- **No raw-file writes** from the webview (`canvas-strategy-surface.md` §2 invariant).

### Conflict policy

- **Reuses** `WorldKbConflictError` (HTTP 409, stale `expected_version`) + `WorldKbValidationError` (HTTP 422, domain-rule failure) from V1.73.
- **No Timeline-specific conflict DTO.**
- Conflict-modal copy is **world-kb-flavored** (reuses V1.73/V1.74 copy tokens).
- Conflict resolution flow: keep draft patch → refetch canonical graph → offer Use-current / Reapply-my-edit / Review-side-by-side (side-by-side enabled only when draft and canonical touch non-overlapping fields).

### Outline companion (architect decision on complexity call)

- The Outline (`work-outline-timeline`) surface **keeps its chapter-relative timeline lane unchanged**. Product allowed removing it; architect decides — decision: **keep**. Rationale: Outline is the Work-entry hero; its chapter-relative timeline affordances are the Work-projection counterpart to the World Timeline hero; removing them would force authors to switch surfaces to inspect chapter-linked events, regressing the V1.72 β slice. The Outline adapter source file is therefore **untouched** on the P1 branch (P1 Global Constraints → Regression gate verifies).
- The two surfaces remain peers and serve different scopes (World when-axis vs Work chapter-relative events); no data is duplicated.

### Honest empty-state (architect-pinned copy requirements)

- Empty World (zero `entities[]`) → "This World has no entities yet. Add characters, events, and places through World KB to populate the timeline." + CTA to World KB peer.
- Non-empty World but zero `block_type=event` entities → "This World's timeline is empty. Events you add through World KB or chapter extraction will appear here." + CTA to World KB.
- Partial temporal signal (some events lack `body.attributes.occurred_at`) → `summarizeGraph` MUST include: "Ordering inferred from available event data; not a canonical chronology."
- Adapter MUST NOT fabricate event ordering from `updated_at`, `canonical_name`, or any non-temporal field.

### Fork markers (architect decision)

- **No Fork marker nodes on the timeline in V1.122.** Fork create/merge UI is forbidden (Non-Goal). Fork data is reserved for an optional canvas-header badge from `WorldState` (`is_fork`, `parent_world_id`, `forked_from_event_id`) — read-only chrome, not a graph node. If the sidecar is not fetched, the badge is omitted (graceful degradation).

### `wire_contracts_changed: false` (architect-verified feasible)

- V1.122 P1 adds **only** a frontend `CanvasSurfaceKind = "timeline"` enum value + a new adapter module.
- Reuses: `WorldKbGraphResponse`, `WorldKbEntityProjection`, `WorldKbRelationshipProjection` / `WorldKbRelationshipKind`, `WorldKbSourceAnchorProjection`, `WorldKbPatchEntityRequest` / `WorldKbPatchEntityResponse`, `WorldKbConflictError`, `WorldKbValidationError`, `WorldState` — all shipped.
- Forbids: new `schemas/` entries, new daemon HTTP routes, new daemon Rust changes, `@42ch/nexus-contracts` version bump, codegen output drift.
- P1 Task 6 step 4 records an 8-point verification gate (schemas/ diff empty, contracts/ diff empty, daemon api/ diff empty, codegen clean, version unchanged, etc.).
