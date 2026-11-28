/**
 * TimelineCanvasAdapter — V1.122 P1 T2.
 *
 * The architect-locked DTO reuse (WorldKbGraphResponse single source,
 * wire_contracts_changed: false) is implicit in every fixture below: every
 * input is a plain `WorldKbGraphResponse` shape; every node data is a plain
 * `TimelineNodeData` shape; every edge data is a plain `WorldKbEdgeData` shape.
 */
import { describe, expect, it, vi } from 'vitest';
import type { Edge, Node } from '@xyflow/react';

import type {
  WorldKbEntityProjection,
  WorldKbGraphResponse,
  WorldKbRelationshipProjection,
  WorldKbSourceAnchorProjection,
} from '@42ch/nexus-contracts';

import type { NexusClient } from '@/lib/nexus';
import type { WorldKbEdgeData } from '../../world-kb/types';
import {
  createTimelineCanvasAdapter,
  type TimelineCanvasAdapterContext,
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

function relationship(
  overrides: Partial<WorldKbRelationshipProjection>,
): WorldKbRelationshipProjection {
  return {
    relationship_id: 'rel-1',
    world_id: 'world-7',
    source_entity_id: 'kb-event-1',
    target_entity_id: 'kb-char-1',
    relation_type: 'references',
    symmetric: false,
    source_anchor_ids: [],
    needs_review: false,
    source: 'manual',
    version: 1,
    updated_at: '2026-01-01T00:00:00Z',
    projection_direction: 'stored',
    ...overrides,
  } as WorldKbRelationshipProjection;
}

function anchor(
  overrides: Partial<WorldKbSourceAnchorProjection>,
): WorldKbSourceAnchorProjection {
  return {
    source_anchor_id: 'anchor-1',
    key_block_id: 'kb-event-1',
    source_type: 'manuscript',
    reference: 'ch1:p2',
    ...overrides,
  } as WorldKbSourceAnchorProjection;
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

function emptyGraph(): WorldKbGraphResponse {
  return { entities: [], source_anchors: [], relationships: [] };
}

const ORDERING_DISCLAIMER =
  'Ordering inferred from available event data; not a canonical chronology.';

describe('TimelineCanvasAdapter — surface kind + layout options', () => {
  it('declares surfaceKind: "timeline"', () => {
    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    expect(adapter.surfaceKind).toBe('timeline');
  });

  it('opts into dagre left-to-right (direction: "LR")', () => {
    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    expect(adapter.layoutOptions?.direction).toBe('LR');
  });
});

describe('TimelineCanvasAdapter.projectGraph — entity projection', () => {
  it('projects block_type=event entities as timeline-event nodes (layoutHint "event")', () => {
    const eventWithTimestamp = entity({
      key_block_id: 'kb-event-1',
      block_type: 'event',
      canonical_name: 'Coronation',
      body: { attributes: { occurred_at: '1042-03-01T00:00:00Z' } },
    });
    const graph: WorldKbGraphResponse = {
      entities: [eventWithTimestamp],
      source_anchors: [],
      relationships: [],
    };

    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    const { nodes } = adapter.projectGraph(graph);

    expect(nodes).toHaveLength(2);
    const node = nodes[0] as Node<TimelineNodeData>;
    expect(node.type).toBe('timeline-event');
    expect(node.data.layoutHint).toBe('event');
    expect(node.data.occurredAtHint).toBe('1042-03-01T00:00:00Z');
    expect(node.data.key_block_id).toBe('kb-event-1');
    expect(node.data.canonical_name).toBe('Coronation');
  });

  it('projects non-event entities as timeline-key-block nodes (layoutHint "context")', () => {
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

    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    const { nodes } = adapter.projectGraph(graph);

    expect(nodes).toHaveLength(1);
    const node = nodes[0] as Node<TimelineNodeData>;
    expect(node.type).toBe('timeline-key-block');
    expect(node.data.layoutHint).toBe('context');
    expect(node.data.occurredAtHint).toBeUndefined();
  });

  it('positions dated events along the when-axis sorted by occurred_at (left → right)', () => {
    const earlier = entity({
      key_block_id: 'kb-event-early',
      block_type: 'event',
      canonical_name: 'Founding',
      body: { attributes: { occurred_at: '1000-01-01T00:00:00Z' } },
    });
    const later = entity({
      key_block_id: 'kb-event-late',
      block_type: 'event',
      canonical_name: 'Coronation',
      body: { attributes: { occurred_at: '1042-03-01T00:00:00Z' } },
    });
    const graph: WorldKbGraphResponse = {
      entities: [later, earlier],
      source_anchors: [],
      relationships: [],
    };

    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    const { nodes } = adapter.projectGraph(graph);

    const early = nodes.find((n) => n.id === 'entity:kb-event-early')!;
    const late = nodes.find((n) => n.id === 'entity:kb-event-late')!;
    expect(early.position.x).toBeLessThan(late.position.x);
    expect(early.position.y).toBe(0);
    expect(late.position.y).toBe(0);
  });

  it('clusters events without occurred_at in a temporal-unknown group off the when-axis', () => {
    const undated = entity({
      key_block_id: 'kb-event-undated',
      block_type: 'event',
      canonical_name: 'Forgotten Battle',
    });
    const dated = entity({
      key_block_id: 'kb-event-dated',
      block_type: 'event',
      canonical_name: 'Coronation',
      body: { attributes: { occurred_at: '1042-03-01T00:00:00Z' } },
    });
    const graph: WorldKbGraphResponse = {
      entities: [undated, dated],
      source_anchors: [],
      relationships: [],
    };

    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    const { nodes } = adapter.projectGraph(graph);

    const undatedNode = nodes.find((n) => n.id === 'entity:kb-event-undated')!;
    const datedNode = nodes.find((n) => n.id === 'entity:kb-event-dated')!;
    expect(undatedNode.data.occurredAtHint).toBeUndefined();
    expect(datedNode.data.occurredAtHint).toBe('1042-03-01T00:00:00Z');
    expect(undatedNode.position.y).toBeGreaterThan(0);
    expect(datedNode.position.y).toBe(0);
  });

  it('positions non-event context entities off the when-axis (Context cluster)', () => {
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

    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    const { nodes } = adapter.projectGraph(graph);

    const ctx = nodes.find((n) => n.id === 'entity:kb-char-1')!;
    expect(ctx.position.y).toBeLessThan(0);
  });

  it('does not crash on an empty graph and returns empty node + edge arrays', () => {
    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    const result = adapter.projectGraph(emptyGraph());
    expect(result.nodes).toEqual([]);
    expect(result.edges).toEqual([]);
  });

  it('registers the V1.122 timeline-event + timeline-key-block node types and NO fork marker', () => {
    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    const keys = Object.keys(adapter.nodeTypes);
    expect(keys).toContain('timeline-event');
    expect(keys).toContain('timeline-key-block');
    expect(keys).toContain('timeline-brief-era');
    expect(keys).toContain('timeline-compute-result');
    for (const forbidden of ['fork-marker', 'forkmarker', 'fork']) {
      expect(keys.map((k) => k.toLowerCase())).not.toContain(forbidden);
    }
  });

  it('attaches source anchor count to entity node data (grounding badge metadata)', () => {
    const event = entity({
      key_block_id: 'kb-event-1',
      block_type: 'event',
      canonical_name: 'Coronation',
      source_anchor_count: 2,
    });
    const graph: WorldKbGraphResponse = {
      entities: [event],
      source_anchors: [anchor({ key_block_id: 'kb-event-1' }), anchor({
        source_anchor_id: 'anchor-2',
        key_block_id: 'kb-event-1',
      })],
      relationships: [],
    };

    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    const { nodes } = adapter.projectGraph(graph);

    const node = nodes[0] as Node<TimelineNodeData>;
    expect(node.data.source_anchor_count).toBe(2);
  });
});


describe('TimelineCanvasAdapter.projectGraph — relationship edges (verbatim WorldKbEdgeData reuse)', () => {
  it('projects relationships[] into edges carrying WorldKbEdgeData verbatim', () => {
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
    const rel = relationship({
      relationship_id: 'rel-1',
      source_entity_id: 'kb-char-1',
      target_entity_id: 'kb-event-1',
      relation_type: 'references',
      symmetric: false,
      confidence: 0.7,
      source_anchor_ids: ['anchor-1'],
    });
    const graph: WorldKbGraphResponse = {
      entities: [event, character],
      source_anchors: [],
      relationships: [rel],
    };

    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    const { edges } = adapter.projectGraph(graph);

    expect(edges).toHaveLength(1);
    const edge = edges[0] as Edge<WorldKbEdgeData>;
    expect(edge.source).toBe('entity:kb-char-1');
    expect(edge.target).toBe('entity:kb-event-1');
    const data = edge.data as WorldKbEdgeData;
    expect(data.relationType).toBe('relationship');
    expect(data.confidence).toBe(0.7);
    expect(data.sourceAnchorIds).toEqual(['anchor-1']);
  });

  it('does NOT introduce Foreshadow / Realizes / ForkFrom edge types', () => {
    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    const registered = adapter.edgeTypes ? Object.keys(adapter.edgeTypes) : [];
    for (const forbidden of ['foreshadow', 'realizes', 'fork-from', 'forkFrom']) {
      expect(registered).not.toContain(forbidden);
    }
  });

  it('projects every symmetric relationship row in both stored + reverse directions', () => {
    const a = entity({
      key_block_id: 'kb-char-1',
      block_type: 'character',
      canonical_name: 'Aria',
    });
    const b = entity({
      key_block_id: 'kb-char-2',
      block_type: 'character',
      canonical_name: 'Bran',
    });
    const stored = relationship({
      relationship_id: 'rel-1',
      source_entity_id: 'kb-char-1',
      target_entity_id: 'kb-char-2',
      relation_type: 'allied_with',
      symmetric: true,
      projection_direction: 'stored',
    });
    const reverse = relationship({
      relationship_id: 'rel-1',
      source_entity_id: 'kb-char-2',
      target_entity_id: 'kb-char-1',
      relation_type: 'allied_with',
      symmetric: true,
      projection_direction: 'symmetric_reverse',
    });
    const graph: WorldKbGraphResponse = {
      entities: [a, b],
      source_anchors: [],
      relationships: [stored, reverse],
    };

    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    const { edges } = adapter.projectGraph(graph);

    expect(edges).toHaveLength(2);
    const sourceIds = edges.map((e) => e.source).sort();
    const targetIds = edges.map((e) => e.target).sort();
    expect(sourceIds).toEqual(['entity:kb-char-1', 'entity:kb-char-2']);
    expect(targetIds).toEqual(['entity:kb-char-1', 'entity:kb-char-2']);
  });
});

describe('TimelineCanvasAdapter.summarizeGraph — honest temporal disclaimer', () => {
  it('includes the ordering disclaimer when any event lacks occurred_at', () => {
    const undated = entity({
      key_block_id: 'kb-event-undated',
      block_type: 'event',
      canonical_name: 'Forgotten Battle',
    });
    const graph: WorldKbGraphResponse = {
      entities: [undated],
      source_anchors: [],
      relationships: [],
    };

    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    const summary = adapter.summarizeGraph(graph);
    expect(summary).toContain(ORDERING_DISCLAIMER);
    expect(summary).toMatch(/1 event/i);
  });

  it('omits the disclaimer for an empty graph (zero events)', () => {
    // PR #156 fix: the disclaimer is tied to event entities being rendered.
    // A zero-event graph surfaces its own honest empty-state copy via
    // <EmptyState> (§7); the a11y summary therefore does NOT carry the
    // ordering disclaimer.
    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    const summary = adapter.summarizeGraph(emptyGraph());
    expect(summary).not.toContain(ORDERING_DISCLAIMER);
    expect(summary.length).toBeGreaterThan(0);
  });

  it('omits the disclaimer when only non-event (context) entities exist', () => {
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

    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    const summary = adapter.summarizeGraph(graph);
    expect(summary).not.toContain(ORDERING_DISCLAIMER);
    expect(summary).toMatch(/1 context entity/i);
  });

  it('includes the disclaimer even when every event carries an ISO occurred_at (lexical sort is not canonical)', () => {
    // PR #156 fix: even when every event has a parseable ISO timestamp, the
    // adapter performs no date parsing in MVP — the when-axis is sorted by
    // lexical string comparison. The disclaimer must still surface because
    // "ISO-looking" is not the same as "canonical chronology parsed".
    const dated = entity({
      key_block_id: 'kb-event-dated',
      block_type: 'event',
      canonical_name: 'Coronation',
      body: { attributes: { occurred_at: '1042-03-01T00:00:00Z' } },
    });
    const character = entity({
      key_block_id: 'kb-char-1',
      block_type: 'character',
      canonical_name: 'Aria',
    });
    const graph: WorldKbGraphResponse = {
      entities: [dated, character],
      source_anchors: [],
      relationships: [],
    };

    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    const summary = adapter.summarizeGraph(graph);
    expect(summary).toContain(ORDERING_DISCLAIMER);
    expect(summary).toMatch(/1 event/i);
    expect(summary).toMatch(/1 context entity/i);
  });

  it('includes the disclaimer for freeform (non-date) occurred_at strings like "Spring 1042"', () => {
    const spring = entity({
      key_block_id: 'kb-event-spring',
      block_type: 'event',
      canonical_name: 'Spring Court',
      body: { attributes: { occurred_at: 'Spring 1042' } },
    });
    const ten = entity({
      key_block_id: 'kb-event-ten',
      block_type: 'event',
      canonical_name: 'Tenth Year',
      body: { attributes: { occurred_at: '10' } },
    });
    const two = entity({
      key_block_id: 'kb-event-two',
      block_type: 'event',
      canonical_name: 'Second Year',
      body: { attributes: { occurred_at: '2' } },
    });
    const graph: WorldKbGraphResponse = {
      entities: [spring, ten, two],
      source_anchors: [],
      relationships: [],
    };

    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    const summary = adapter.summarizeGraph(graph);
    expect(summary).toContain(ORDERING_DISCLAIMER);
    expect(summary).toMatch(/3 events/i);
  });

  it('includes the disclaimer when events mix dated, freeform, and undated signals', () => {
    const iso = entity({
      key_block_id: 'kb-event-iso',
      block_type: 'event',
      canonical_name: 'Coronation',
      body: { attributes: { occurred_at: '1042-03-01T00:00:00Z' } },
    });
    const freeform = entity({
      key_block_id: 'kb-event-freeform',
      block_type: 'event',
      canonical_name: 'Spring Court',
      body: { attributes: { occurred_at: 'Spring 1042' } },
    });
    const undated = entity({
      key_block_id: 'kb-event-undated',
      block_type: 'event',
      canonical_name: 'Forgotten Battle',
    });
    const graph: WorldKbGraphResponse = {
      entities: [iso, freeform, undated],
      source_anchors: [],
      relationships: [],
    };

    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    const summary = adapter.summarizeGraph(graph);
    expect(summary).toContain(ORDERING_DISCLAIMER);
    expect(summary).toMatch(/3 events/i);
  });

  it('never returns an empty string', () => {
    const adapter = createTimelineCanvasAdapter({ current: makeContext() });
    expect(adapter.summarizeGraph(emptyGraph()).length).toBeGreaterThan(0);
    const dense: WorldKbGraphResponse = {
      entities: [
        entity({
          key_block_id: 'kb-e-1',
          block_type: 'event',
          canonical_name: 'A',
          body: { attributes: { occurred_at: '1000-01-01T00:00:00Z' } },
        }),
        entity({
          key_block_id: 'kb-c-1',
          block_type: 'character',
          canonical_name: 'B',
        }),
      ],
      source_anchors: [],
      relationships: [],
    };
    expect(adapter.summarizeGraph(dense).length).toBeGreaterThan(0);
  });
});


describe('TimelineCanvasAdapter — T2 write-boundary isolation', () => {
  it('projectGraph + summarizeGraph do NOT invoke any client method', () => {
    const ctx = makeContext();
    const ctxRef = { current: ctx };
    const adapter = createTimelineCanvasAdapter(ctxRef);

    const graph: WorldKbGraphResponse = {
      entities: [
        entity({
          key_block_id: 'kb-event-1',
          block_type: 'event',
          canonical_name: 'Coronation',
        }),
      ],
      source_anchors: [],
      relationships: [],
    };

    adapter.projectGraph(graph);
    adapter.summarizeGraph(graph);

    const client = ctx.client as unknown as {
      getWorldKbGraph: ReturnType<typeof vi.fn>;
      worldKbPatchEntity: ReturnType<typeof vi.fn>;
      worldKbPatchRelationship: ReturnType<typeof vi.fn>;
      worldKbPromoteCandidate: ReturnType<typeof vi.fn>;
      patchOutlineStructure: ReturnType<typeof vi.fn>;
      patchOutlineChapter: ReturnType<typeof vi.fn>;
      patchTimelineEvent: ReturnType<typeof vi.fn>;
    };
    expect(client.getWorldKbGraph).not.toHaveBeenCalled();
    expect(client.worldKbPatchEntity).not.toHaveBeenCalled();
    expect(client.worldKbPatchRelationship).not.toHaveBeenCalled();
    expect(client.worldKbPromoteCandidate).not.toHaveBeenCalled();
    expect(client.patchOutlineStructure).not.toHaveBeenCalled();
    expect(client.patchOutlineChapter).not.toHaveBeenCalled();
    expect(client.patchTimelineEvent).not.toHaveBeenCalled();
  });

});
