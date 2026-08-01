/**
 * Timeline adapter — Narrative merge of compute-result log events (V1.147 P2
 * T3). Verifies the plan Global Constraints merge discipline:
 *
 *   - Two event families coexist on the Narrative layer: author KB
 *     `block_type=event` entities (existing `entity:<kb_id>` path) and
 *     machine-written `event_type=compute_result` log events
 *     (`compute:<event_id>` path). Distinct families, distinct node ids —
 *     no double-render is possible.
 *   - Only `event_type=compute_result` + `status=canon` merge; provisional /
 *     rejected / other event types are excluded (the T1 route already filters
 *     server-side; the adapter enforces defensively).
 *   - Dated compute events (parseable `created_at`) land on the when-axis
 *     after the authored KB event block; undated events cluster in the
 *     temporal-unknown group. No fabricated chronology from `sequence_no`.
 *   - Brief layer untouched — compute events never leak into the era sweep.
 *   - The compute node payload carries provenance (module id/version, run id,
 *     source kind), the report digest, and affected KB entries resolved
 *     against the graph.
 */
import { describe, expect, it } from 'vitest';
import type { Node } from '@xyflow/react';

import type {
  TimelineEventInfo,
  WorldKbEntityProjection,
  WorldKbGraphResponse,
} from '@42ch/nexus-contracts';

import {
  createTimelineCanvasAdapter,
  projectTimelineGraph,
  summarizeTimelineGraph,
  type ComputeNodePayload,
  type TimelineCanvasAdapterContext,
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

function graphWith(
  entities: WorldKbEntityProjection[],
): WorldKbGraphResponse {
  return { entities, source_anchors: [], relationships: [] };
}

function computeEvent(
  overrides: Partial<TimelineEventInfo> = {},
): TimelineEventInfo {
  return {
    id: 'evt_compute_1',
    branch_id: 'fbk_root',
    event_type: 'compute_result',
    status: 'canon',
    sequence_no: 3,
    title: 'Aria strikes Brann',
    summary: 'Brann takes 6 damage and staggers back.',
    affected_key_block_ids: ['char-aria', 'char-brann'],
    caused_by_event_ids: [],
    metadata: {},
    extensions: {
      compute: {
        module_id: 'basic-combat',
        module_version: '1.0.0',
        run_id: 'run_9f3a2c',
        source_kind: 'direct_invoke',
      },
    },
    created_at: '2026-08-01T00:00:00Z',
    ...overrides,
  };
}

function makeContext(
  overrides: Partial<TimelineCanvasAdapterContext> = {},
): TimelineCanvasAdapterContext {
  return { worldId: 'world-7', ...overrides };
}

function computeNodes(nodes: Node<TimelineNodeData>[]): Node<TimelineNodeData>[] {
  return nodes.filter((n) => n.data.layoutHint === 'compute');
}

// ─── Tests ──────────────────────────────────────────────────────────────────

describe('Narrative merge of compute_result events', () => {
  it('projects KB events and compute_result log events as distinct node families (no double-render)', () => {
    const graph = graphWith([
      entity({
        key_block_id: 'kb-event-1',
        block_type: 'event',
        canonical_name: 'The coronation',
        body: { attributes: { occurred_at: '2026-07-01T00:00:00Z' } },
      }),
      entity({ key_block_id: 'char-aria', block_type: 'character', canonical_name: 'Aria' }),
    ]);

    const { nodes } = projectTimelineGraph(graph, 'narrative', [computeEvent()]);

    const kbNodes = nodes.filter((n) => n.type === 'timeline-event');
    const merged = computeNodes(nodes);

    expect(kbNodes).toHaveLength(1);
    expect(kbNodes[0]?.id).toBe('entity:kb-event-1');

    expect(merged).toHaveLength(1);
    expect(merged[0]?.id).toBe('compute:evt_compute_1');
    expect(merged[0]?.type).toBe('timeline-compute-result');

    // Distinct families → no id collision, no duplicate rendering of the
    // same story beat.
    const ids = nodes.map((n) => n.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('merges only canon compute_result events (provisional/rejected/other types excluded)', () => {
    const graph = graphWith([]);
    const events = [
      computeEvent(),
      computeEvent({ id: 'evt_provisional', status: 'provisional' }),
      computeEvent({ id: 'evt_rejected', status: 'rejected' }),
      computeEvent({
        id: 'evt_other_type',
        event_type: 'story_advance',
        title: 'A handshake',
      }),
    ];

    const { nodes } = projectTimelineGraph(graph, 'narrative', events);

    const merged = computeNodes(nodes);
    expect(merged.map((n) => n.id)).toEqual(['compute:evt_compute_1']);
  });

  it('positions dated compute events on the when-axis after the KB block; undated in the temporal-unknown cluster', () => {
    const graph = graphWith([
      entity({
        key_block_id: 'kb-event-1',
        block_type: 'event',
        canonical_name: 'The coronation',
        body: { attributes: { occurred_at: '2026-07-01T00:00:00Z' } },
      }),
    ]);
    const events = [
      computeEvent({ id: 'evt_dated', created_at: '2026-08-02T00:00:00Z' }),
      computeEvent({ id: 'evt_dated_2', created_at: '2026-08-01T00:00:00Z' }),
      computeEvent({ id: 'evt_undated', created_at: 'not-a-date' }),
    ];

    const { nodes } = projectTimelineGraph(graph, 'narrative', events);

    const kb = nodes.find((n) => n.id === 'entity:kb-event-1');
    expect(kb?.position.y).toBe(0);

    const dated = computeNodes(nodes).filter((n) => ['compute:evt_dated', 'compute:evt_dated_2'].includes(n.id));
    // Dated compute events sit on the when-axis (Y=0), sorted among
    // themselves by created_at (evt_dated_2 first), appended AFTER the KB
    // dated block (X beyond the KB event).
    expect(dated.map((n) => n.id)).toEqual(['compute:evt_dated_2', 'compute:evt_dated']);
    for (const n of dated) {
      expect(n.position.y).toBe(0);
      expect(n.position.x).toBeGreaterThan(kb?.position.x ?? 0);
    }

    // Undated compute events cluster in the temporal-unknown group.
    const undated = computeNodes(nodes).find((n) => n.id === 'compute:evt_undated');
    expect(undated?.position.y).toBe(220);
  });

  it('never stacks the compute undated cluster on the KB undated cluster (review F2 — mixed family)', () => {
    const graph = graphWith([
      entity({
        key_block_id: 'kb-dated',
        block_type: 'event',
        canonical_name: 'The coronation',
        body: { attributes: { occurred_at: '2026-07-01T00:00:00Z' } },
      }),
      entity({ key_block_id: 'kb-undated-1', block_type: 'event', canonical_name: 'Mystery one' }),
      entity({ key_block_id: 'kb-undated-2', block_type: 'event', canonical_name: 'Mystery two' }),
    ]);
    const events = [
      // All-compute-undated case: dated compute count (0) < KB undated count
      // (2) — the exact overlap scenario the review flagged for the y=220
      // temporal-unknown lane.
      computeEvent({ id: 'evt_undated_compute', created_at: 'not-a-date' }),
    ];

    const { nodes } = projectTimelineGraph(graph, 'narrative', events);

    // Both event families on the Narrative layer share no identical
    // coordinates (KB events + compute result nodes).
    const family = nodes.filter(
      (n) => n.type === 'timeline-event' || n.type === 'timeline-compute-result',
    );
    const coords = family.map((n) => `${n.position.x},${n.position.y}`);
    expect(new Set(coords).size).toBe(coords.length);

    // The compute undated row sits PAST the KB undated cluster, not on it.
    const kbUndated = nodes.find((n) => n.id === 'entity:kb-undated-2');
    const computeUndated = nodes.find((n) => n.id === 'compute:evt_undated_compute');
    expect(computeUndated?.position.y).toBe(220);
    expect(computeUndated!.position.x).toBeGreaterThan(kbUndated!.position.x);
  });

  it('carries the compute payload (provenance + digest + affected entries resolved from the graph)', () => {
    const graph = graphWith([
      entity({ key_block_id: 'char-aria', block_type: 'character', canonical_name: 'Aria' }),
      entity({ key_block_id: 'char-brann', block_type: 'character', canonical_name: 'Brann' }),
    ]);
    const event = computeEvent({
      affected_key_block_ids: ['char-aria', 'char-brann', 'char-ghost'],
    });

    const { nodes } = projectTimelineGraph(graph, 'narrative', [event]);

    const merged = computeNodes(nodes);
    const payload = merged[0]?.data.compute as ComputeNodePayload;
    expect(payload.eventId).toBe('evt_compute_1');
    expect(payload.moduleId).toBe('basic-combat');
    expect(payload.moduleVersion).toBe('1.0.0');
    expect(payload.runId).toBe('run_9f3a2c');
    expect(payload.sourceKind).toBe('direct_invoke');
    expect(payload.reportDigest).toBe('Brann takes 6 damage and staggers back.');
    // Affected entries resolve canonical names from the KB graph; unknown ids
    // fall back to the id itself (honest).
    expect(payload.affectedEntries).toEqual([
      { id: 'char-aria', title: 'Aria' },
      { id: 'char-brann', title: 'Brann' },
      { id: 'char-ghost', title: 'char-ghost' },
    ]);
  });

  it('renders preset-path provenance when source_kind=preset and hides the run id', () => {
    const graph = graphWith([]);
    const event = computeEvent({
      extensions: {
        compute: {
          module_id: 'combat-engine',
          module_version: '3.0.0',
          source_kind: 'preset',
        },
      },
    });

    const { nodes } = projectTimelineGraph(graph, 'narrative', [event]);

    const payload = computeNodes(nodes)[0]?.data.compute as ComputeNodePayload;
    expect(payload.sourceKind).toBe('preset');
    expect(payload.runId).toBeUndefined();
  });

  it('leaves the Brief layer untouched — compute events never leak into the era sweep', () => {
    const graph = graphWith([
      entity({
        key_block_id: 'era-1',
        block_type: 'era',
        canonical_name: 'First Age',
        body: { attributes: { start_hint: '1000-01-01T00:00:00Z' } },
      }),
    ]);

    const { nodes } = projectTimelineGraph(graph, 'brief', [computeEvent()]);

    expect(computeNodes(nodes)).toHaveLength(0);
    expect(nodes.every((n) => n.type !== 'timeline-compute-result')).toBe(true);
  });

  it('no events input → V1.122 Narrative projection unchanged', () => {
    const graph = graphWith([
      entity({
        key_block_id: 'kb-event-1',
        block_type: 'event',
        canonical_name: 'The coronation',
        body: { attributes: { occurred_at: '2026-07-01T00:00:00Z' } },
      }),
    ]);

    const { nodes } = projectTimelineGraph(graph, 'narrative');

    expect(computeNodes(nodes)).toHaveLength(0);
    expect(nodes.filter((n) => n.type === 'timeline-event')).toHaveLength(1);
  });

  it('summarizeTimelineGraph includes the compute event count when events merge', () => {
    const graph = graphWith([]);
    const summary = summarizeTimelineGraph(graph, [computeEvent()]);
    expect(summary).toContain('1 compute event');
  });

  it('adapter factory with captured events projects them through projectGraph', () => {
    const graph = graphWith([]);
    const adapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'narrative',
      [computeEvent()],
    );

    const { nodes } = adapter.projectGraph(graph);
    expect(computeNodes(nodes)).toHaveLength(1);
  });
});
