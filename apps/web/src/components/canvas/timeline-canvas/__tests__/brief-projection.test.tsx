/**
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

    expect(nodes).toHaveLength(2);
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
    expect(early.position.x).toBeLessThan(late.position.x);
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
    expect(undatedNode.position.y).toBeGreaterThan(0);
    expect(datedNode.position.y).toBe(0);
  });

  it('omits directedAxisSpine when all eras lack start_hint (R1 guard)', () => {
    const undatedA = eraEntity({
      key_block_id: 'kb-era-undated-a',
      canonical_name: 'The Forgotten Age',
      body: { attributes: { era_id: 'era-forgotten-a' } },
    });
    const undatedB = eraEntity({
      key_block_id: 'kb-era-undated-b',
      canonical_name: 'The Lost Age',
      body: { attributes: { era_id: 'era-forgotten-b' } },
    });
    const graph: WorldKbGraphResponse = {
      entities: [undatedA, undatedB],
      source_anchors: [],
      relationships: [],
    };

    const adapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'brief',
    );
    const { nodes } = adapter.projectGraph(graph);

    expect(nodes).toHaveLength(2);
    expect(nodes[0].data.startHint).toBeUndefined();
    expect(nodes[1].data.startHint).toBeUndefined();
    const spineNode = nodes.find((n) => n.id === 'directed-axis-spine');
    expect(spineNode).toBeUndefined();
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

    // Brief-era inspector (Task 4).
    expect(node.data.eraId).toBe('era-first');
    expect(node.data.startHint).toBe('1000-01-01T00:00:00Z');
    expect(node.data.endHint).toBe('1100-01-01T00:00:00Z');
    expect(node.data.worldSummary).toBe('A time of myth and legend.');
  });

  it('does not render relationship edges on the Brief layer (Brief = era sweep, not full graph)', () => {
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

    expect(nodes).toHaveLength(2);
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
    // `entities.filter(e => !['event','era'].includes(e.block_type))`. An
    // era entity MUST NOT appear as a Context cluster on the Narrative
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

    expect(nodes).toHaveLength(1);
    expect(nodes[0].id).toBe('entity:kb-char-1');
    expect(nodes[0].type).toBe('timeline-key-block');
  });
});


describe('TimelineCanvasAdapter.projectGraph — default layer delegation', () => {
  it("default active layer is 'narrative' when no layer is passed to the factory (V1.122 backward compat)", () => {
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

    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    const { nodes } = adapter.projectGraph(graph);

    expect(nodes).toHaveLength(2);
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

    expect(nodes).toHaveLength(2);
    expect(nodes[0].id).toBe('entity:kb-era-1');
    expect(nodes[0].type).toBe('timeline-brief-era');
  });
});


describe('TimelineCanvasAdapter — node-type registry (V1.123 P1 T2 + V1.126 P1)', () => {
  it("registers 'timeline-brief-era' alongside V1.122 'timeline-event' + 'timeline-key-block' + V1.126 'directedAxisSpine' + V1.147 'timeline-compute-result' + V1.156 'work-timeline-moment-*'", () => {
    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    const keys = Object.keys(adapter.nodeTypes).sort();
    expect(keys).toEqual(
      [
        'directedAxisSpine',
        'timeline-brief-era',
        'timeline-compute-result',
        'timeline-event',
        'timeline-key-block',
        // components (World-Moment feel ≡ Work-Moment feel; no new node
        // component family).
        'work-timeline-moment-beat',
        'work-timeline-moment-scene',
      ].sort(),
    );
  });

  it('does NOT register a fork-marker node type (V1.122 §8 prohibition preserved)', () => {
    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    expect(Object.keys(adapter.nodeTypes)).not.toContain('timeline-fork-marker');
    expect(Object.keys(adapter.nodeTypes)).not.toContain('timeline-forkmarker');
  });

  it('exposes TimelineLayer type at runtime via the factory signature (smoke test)', () => {
    const layers: TimelineLayer[] = ['brief', 'narrative', 'moment'];
    for (const layer of layers) {
      const adapter = createTimelineCanvasAdapter(
        { current: makeContext() },
        layer,
      );
      expect(adapter.surfaceKind).toBe('timeline');
    }
  });
});
