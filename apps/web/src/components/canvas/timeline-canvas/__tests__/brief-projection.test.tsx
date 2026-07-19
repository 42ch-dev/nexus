/**
 * TimelineCanvasAdapter — V1.123 P1 T2 (Brief layer projection).
 *
 * Verifies the Brief layer projection contract locked by
 * `iterations/v1.123/specs/three-layer-architecture.md` §2 + §8:
 *
 *   - `projectGraphForLayer(graph, 'brief')` filters `entities[block_type=era]`
 *     onto the Brief when-axis as `timeline-brief-era` nodes (compact era
 *     marker cards). Non-era entities are EXCLUDED from the Brief layer.
 *   - `projectGraphForLayer(graph, 'narrative')` preserves the V1.122 event
 *     timeline projection (events on the when-axis, other entities as
 *     Context clusters). Era entities are EXCLUDED from the Narrative layer
 *     (architect §5.2: Context clusters = `entities.filter(e =>
 *     !['event','era'].includes(e.block_type))`).
 *   - `projectGraph(graph)` delegates to the adapter's active layer. Default
 *     active layer is `'narrative'` for V1.122 backward compatibility until
 *     Task 3 wires Brief default.
 *   - `createTimelineCanvasAdapter(ctxRef, 'brief')` makes `projectGraph`
 *     delegate to the Brief layer; `createTimelineCanvasAdapter(ctxRef)` (no
 *     layer) keeps V1.122 Narrative-default behavior.
 *   - The Brief-era node type carries era markers (`eraId`, `startHint`,
 *     `endHint`, `worldSummary`) extracted from `body.attributes`.
 *   - Era nodes position horizontally (LR direction) by `body.attributes.start_hint`
 *     when present; cluster below the when-axis otherwise.
 *
 * Architect lock (§2.4 + §8): `Brief-on-KeyBlock via new wire BlockType = "era"`.
 * Single graph source: `WorldKbGraphResponse` (V1.73 unchanged). The Brief
 * layer is a pure frontend filter over the same graph — no new endpoint, no
 * new DTO, no new daemon route (architect §5).
 */
import { describe, expect, it, vi } from 'vitest';
import type { Node } from '@xyflow/react';

import type {
  WorldKbEntityProjection,
  WorldKbGraphResponse,
} from '@42ch/nexus-contracts';

import type { NexusClient } from '@/lib/nexus';
import {
  createTimelineCanvasAdapter,
  type TimelineCanvasAdapterContext,
  type TimelineLayer,
  type TimelineNodeData,
} from '../timeline-canvas-adapter';

// ─── Fixture builders ──────────────────────────────────────────────────────

function entity(
  overrides: Partial<WorldKbEntityProjection> &
    Pick<WorldKbEntityProjection, 'key_block_id' | 'block_type' | 'canonical_name'>,
): WorldKbEntityProjection {
  return {
    world_id: 'world-7',
    status: 'confirmed',
    version: 1,
    ...overrides,
  } as WorldKbEntityProjection;
}

function eraEntity(
  overrides: Partial<WorldKbEntityProjection> &
    Pick<WorldKbEntityProjection, 'key_block_id' | 'canonical_name'>,
): WorldKbEntityProjection {
  const { key_block_id, canonical_name, body, ...rest } = overrides;
  return entity({
    key_block_id,
    block_type: 'era',
    canonical_name,
    // Default era body shape per architect §2.3: era markers ride in
    // `body.attributes` (`era_id`, `start_hint`, `end_hint`, `world_summary`).
    body: body ?? {
      attributes: {
        era_id: 'era-1',
        start_hint: '1000-01-01T00:00:00Z',
        end_hint: '1100-01-01T00:00:00Z',
        world_summary: 'The First Age',
      },
    },
    ...rest,
  });
}

function makeMockClient() {
  return {
    getWorldKbGraph: vi.fn(),
    worldKbPatchEntity: vi.fn(),
    worldKbPatchRelationship: vi.fn(),
    worldKbPromoteCandidate: vi.fn(),
    patchOutlineStructure: vi.fn(),
    patchOutlineChapter: vi.fn(),
    patchTimelineEvent: vi.fn(),
  } as unknown as NexusClient;
}

function makeContext(
  overrides: Partial<TimelineCanvasAdapterContext> = {},
): TimelineCanvasAdapterContext {
  return {
    worldId: 'world-7',
    client: makeMockClient(),
    ...overrides,
  };
}

// ─── Brief layer projection (architect §2 + §8) ─────────────────────────────

describe('TimelineCanvasAdapter.projectGraphForLayer — Brief projection (block_type=era)', () => {
  it("filters entities to block_type='era' for Brief layer", () => {
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
    });
    const event = entity({
      key_block_id: 'kb-event-1',
      block_type: 'event',
      canonical_name: 'Coronation',
    });
    const character = entity({
      key_block_id: 'kb-char-1',
      block_type: 'character',
      canonical_name: 'Aria',
    });
    const graph: WorldKbGraphResponse = {
      entities: [era, event, character],
      source_anchors: [],
      relationships: [],
    };

    const adapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'brief',
    );
    const { nodes } = adapter.projectGraph(graph);

    // ONLY the era entity renders on the Brief layer.
    expect(nodes).toHaveLength(1);
    const node = nodes[0] as Node<TimelineNodeData>;
    expect(node.id).toBe('entity:kb-era-1');
    expect(node.type).toBe('timeline-brief-era');
    expect(node.data.layoutHint).toBe('brief');
    expect(node.data.canonical_name).toBe('The First Age');
  });

  it('excludes event and non-event entities from Brief layer', () => {
    const event = entity({
      key_block_id: 'kb-event-1',
      block_type: 'event',
      canonical_name: 'Coronation',
    });
    const character = entity({
      key_block_id: 'kb-char-1',
      block_type: 'character',
      canonical_name: 'Aria',
    });
    const graph: WorldKbGraphResponse = {
      entities: [event, character],
      source_anchors: [],
      relationships: [],
    };

    const adapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'brief',
    );
    const { nodes } = adapter.projectGraph(graph);

    // No era entities → Brief layer is empty (Task 5 owns the honest
    // empty-state copy; Task 2 just guarantees no non-era leakage).
    expect(nodes).toEqual([]);
  });

  it('positions era nodes horizontally (LR) sorted by body.attributes.start_hint', () => {
    const earlier = eraEntity({
      key_block_id: 'kb-era-early',
      canonical_name: 'The First Age',
      body: {
        attributes: {
          era_id: 'era-first',
          start_hint: '1000-01-01T00:00:00Z',
          end_hint: '1100-01-01T00:00:00Z',
        },
      },
    });
    const later = eraEntity({
      key_block_id: 'kb-era-late',
      canonical_name: 'The Second Age',
      body: {
        attributes: {
          era_id: 'era-second',
          start_hint: '1100-01-01T00:00:00Z',
          end_hint: '1200-01-01T00:00:00Z',
        },
      },
    });
    const graph: WorldKbGraphResponse = {
      // intentionally unsorted input — projection owns the chronology
      entities: [later, earlier],
      source_anchors: [],
      relationships: [],
    };

    const adapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'brief',
    );
    const { nodes } = adapter.projectGraph(graph);

    const early = nodes.find((n) => n.id === 'entity:kb-era-early')!;
    const late = nodes.find((n) => n.id === 'entity:kb-era-late')!;
    // Earlier era MUST land to the left of the later era (LR axis = X).
    expect(early.position.x).toBeLessThan(late.position.x);
    // Both dated eras sit on the Brief when-axis baseline (Y = 0).
    expect(early.position.y).toBe(0);
    expect(late.position.y).toBe(0);
  });

  it('clusters eras without start_hint in a temporal-unknown group off the when-axis', () => {
    const undated = eraEntity({
      key_block_id: 'kb-era-undated',
      canonical_name: 'The Forgotten Age',
      body: { attributes: { era_id: 'era-forgotten' } },
    });
    const dated = eraEntity({
      key_block_id: 'kb-era-dated',
      canonical_name: 'The First Age',
      body: {
        attributes: {
          era_id: 'era-first',
          start_hint: '1000-01-01T00:00:00Z',
        },
      },
    });
    const graph: WorldKbGraphResponse = {
      entities: [undated, dated],
      source_anchors: [],
      relationships: [],
    };

    const adapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'brief',
    );
    const { nodes } = adapter.projectGraph(graph);

    const undatedNode = nodes.find((n) => n.id === 'entity:kb-era-undated')!;
    const datedNode = nodes.find((n) => n.id === 'entity:kb-era-dated')!;
    expect(undatedNode.data.startHint).toBeUndefined();
    expect(datedNode.data.startHint).toBe('1000-01-01T00:00:00Z');
    // The temporal-unknown cluster sits BELOW the when-axis (Y > 0).
    expect(undatedNode.position.y).toBeGreaterThan(0);
    expect(datedNode.position.y).toBe(0);
  });

  it('carries era markers (eraId, startHint, endHint, worldSummary) extracted from body.attributes', () => {
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
      body: {
        attributes: {
          era_id: 'era-first',
          start_hint: '1000-01-01T00:00:00Z',
          end_hint: '1100-01-01T00:00:00Z',
          world_summary: 'A time of myth and legend.',
        },
      },
    });
    const graph: WorldKbGraphResponse = {
      entities: [era],
      source_anchors: [],
      relationships: [],
    };

    const adapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'brief',
    );
    const { nodes } = adapter.projectGraph(graph);
    const node = nodes[0] as Node<TimelineNodeData>;

    // Architect §2.3 + §8: era markers ride in `body.attributes` and surface
    // on the Brief-era node data for the compact era marker card + the
    // Brief-era inspector (Task 4).
    expect(node.data.eraId).toBe('era-first');
    expect(node.data.startHint).toBe('1000-01-01T00:00:00Z');
    expect(node.data.endHint).toBe('1100-01-01T00:00:00Z');
    expect(node.data.worldSummary).toBe('A time of myth and legend.');
  });

  it('does not render relationship edges on the Brief layer (Brief = era sweep, not full graph)', () => {
    // Architect §8: Brief layer carries era markers only; full relationship
    // edges belong to the Narrative layer (V1.122 preserved). The Brief
    // layer is a minimal-density world-shape view per layer-feel-differentiation.md §2.2.
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
    });
    const graph: WorldKbGraphResponse = {
      entities: [era],
      source_anchors: [],
      relationships: [
        {
          relationship_id: 'rel-1',
          world_id: 'world-7',
          source_entity_id: 'kb-era-1',
          target_entity_id: 'kb-other',
          relation_type: 'references',
          symmetric: false,
          source_anchor_ids: [],
          needs_review: false,
          source: 'manual',
          version: 1,
          updated_at: '2026-01-01T00:00:00Z',
          projection_direction: 'stored',
        },
      ],
    };

    const adapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'brief',
    );
    const { edges } = adapter.projectGraph(graph);

    // No edges on Brief layer — would clutter the era sweep.
    expect(edges).toEqual([]);
  });

  it('does not crash on an empty graph and returns empty node + edge arrays', () => {
    const adapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'brief',
    );
    const result = adapter.projectGraph({
      entities: [],
      source_anchors: [],
      relationships: [],
    });
    expect(result.nodes).toEqual([]);
    expect(result.edges).toEqual([]);
  });
});

// ─── Narrative layer projection (V1.122 behavior preserved) ─────────────────

describe('TimelineCanvasAdapter.projectGraphForLayer — Narrative projection (V1.122 preserved)', () => {
  it("filters entities to block_type='event' on the when-axis (V1.122 unchanged)", () => {
    const event = entity({
      key_block_id: 'kb-event-1',
      block_type: 'event',
      canonical_name: 'Coronation',
      body: { attributes: { occurred_at: '1042-03-01T00:00:00Z' } },
    });
    const graph: WorldKbGraphResponse = {
      entities: [event],
      source_anchors: [],
      relationships: [],
    };

    const adapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'narrative',
    );
    const { nodes } = adapter.projectGraph(graph);

    expect(nodes).toHaveLength(1);
    const node = nodes[0] as Node<TimelineNodeData>;
    expect(node.type).toBe('timeline-event');
    expect(node.data.layoutHint).toBe('event');
    expect(node.data.occurredAtHint).toBe('1042-03-01T00:00:00Z');
  });

  it('projects non-event, non-era entities as timeline-key-block context nodes', () => {
    const character = entity({
      key_block_id: 'kb-char-1',
      block_type: 'character',
      canonical_name: 'Aria',
    });
    const graph: WorldKbGraphResponse = {
      entities: [character],
      source_anchors: [],
      relationships: [],
    };

    const adapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'narrative',
    );
    const { nodes } = adapter.projectGraph(graph);

    expect(nodes).toHaveLength(1);
    const node = nodes[0] as Node<TimelineNodeData>;
    expect(node.type).toBe('timeline-key-block');
    expect(node.data.layoutHint).toBe('context');
  });

  it('excludes era entities from the Narrative context clusters (architect §5.2)', () => {
    // V1.123 architect lock §5.2: Context clusters =
    // `entities.filter(e => !['event','era'].includes(e.block_type))`. An
    // era entity MUST NOT appear as a Context cluster on the Narrative
    // layer — it is a Brief-layer-only marker.
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
    });
    const character = entity({
      key_block_id: 'kb-char-1',
      block_type: 'character',
      canonical_name: 'Aria',
    });
    const graph: WorldKbGraphResponse = {
      entities: [era, character],
      source_anchors: [],
      relationships: [],
    };

    const adapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'narrative',
    );
    const { nodes } = adapter.projectGraph(graph);

    // Only the character renders as a Context cluster; the era is excluded.
    expect(nodes).toHaveLength(1);
    expect(nodes[0].id).toBe('entity:kb-char-1');
    expect(nodes[0].type).toBe('timeline-key-block');
  });
});

// ─── projectGraph default delegation (V1.122 backward compat) ───────────────

describe('TimelineCanvasAdapter.projectGraph — default layer delegation', () => {
  it("default active layer is 'narrative' when no layer is passed to the factory (V1.122 backward compat)", () => {
    // V1.122 callers invoked `createTimelineCanvasAdapter(ctxRef)` without a
    // layer argument. Their existing assertions (event timeline projection,
    // event+context node types, ordering disclaimer) MUST stay green.
    const event = entity({
      key_block_id: 'kb-event-1',
      block_type: 'event',
      canonical_name: 'Coronation',
      body: { attributes: { occurred_at: '1042-03-01T00:00:00Z' } },
    });
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
    });
    const graph: WorldKbGraphResponse = {
      entities: [event, era],
      source_anchors: [],
      relationships: [],
    };

    // No layer argument — defaults to 'narrative'.
    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    const { nodes } = adapter.projectGraph(graph);

    // Narrative projection: the event renders; the era is excluded.
    expect(nodes).toHaveLength(1);
    expect(nodes[0].id).toBe('entity:kb-event-1');
    expect(nodes[0].type).toBe('timeline-event');
  });

  it("passing 'brief' to the factory makes projectGraph delegate to the Brief layer", () => {
    const event = entity({
      key_block_id: 'kb-event-1',
      block_type: 'event',
      canonical_name: 'Coronation',
    });
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
    });
    const graph: WorldKbGraphResponse = {
      entities: [event, era],
      source_anchors: [],
      relationships: [],
    };

    const adapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'brief',
    );
    const { nodes } = adapter.projectGraph(graph);

    // Brief projection: only the era renders.
    expect(nodes).toHaveLength(1);
    expect(nodes[0].id).toBe('entity:kb-era-1');
    expect(nodes[0].type).toBe('timeline-brief-era');
  });
});

// ─── Node-type registry (additive over V1.122) ──────────────────────────────

describe('TimelineCanvasAdapter — node-type registry (V1.123 P1 T2)', () => {
  it("registers 'timeline-brief-era' alongside V1.122 'timeline-event' + 'timeline-key-block'", () => {
    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    const keys = Object.keys(adapter.nodeTypes).sort();
    expect(keys).toEqual(
      ['timeline-brief-era', 'timeline-event', 'timeline-key-block'].sort(),
    );
  });

  it('does NOT register a fork-marker node type (V1.122 §8 prohibition preserved)', () => {
    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    expect(Object.keys(adapter.nodeTypes)).not.toContain('timeline-fork-marker');
    expect(Object.keys(adapter.nodeTypes)).not.toContain('timeline-forkmarker');
  });

  it('exposes TimelineLayer type at runtime via the factory signature (smoke test)', () => {
    // Compile-time assurance: the factory accepts both layer values. This
    // test exists so a future rename of `TimelineLayer` that breaks the
    // factory signature surfaces in the test suite, not in production.
    const layers: TimelineLayer[] = ['brief', 'narrative'];
    for (const layer of layers) {
      const adapter = createTimelineCanvasAdapter(
        { current: makeContext() },
        layer,
      );
      expect(adapter.surfaceKind).toBe('timeline');
    }
  });
});
