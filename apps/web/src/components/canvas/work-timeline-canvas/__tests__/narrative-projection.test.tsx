/**
 * WorkTimelineCanvasAdapter — V1.123 P2 Task 2 (Narrative layer projection).
 *
 * Verifies the Narrative projection contract locked by
 *   - `iterations/v1.123/specs/three-layer-architecture.md` §7 + §8 (Work
 *     Timeline adapter TypeScript contract + Narrative data composition
 *     from `WorkOutline.timeline_events[]`).
 *   - Plan `2026-07-18-v1.123-work-timeline-narrative-moment.md` Task 2.
 *
 * Coverage:
 *   - `projectGraphForLayer(graph, 'narrative')` returns Work-Narrative
 *     event nodes from `WorkOutline.timeline_events[]` on a left-to-right
 *     when-axis.
 *   - Events positioned horizontally (LR) sorted by `realizes_chapter_id`
 *     (numeric ascending, undefined last) then by `event_id` (lexicographic
 *     tiebreaker). Mirrors the V1.122 Narrative ordering discipline but
 *     uses chapter anchor (Work-scoped) instead of `occurred_at`.
 *   - Events with no `realizes_chapter_id` cluster at the temporal-unknown
 *     tail of the when-axis (honest ordering per architect §7 — do not
 *     fabricate chronology).
 *   - The Narrative event node carries Work-scoped data (`workId`,
 *     `nodeKind: 'event'`, `eventId`, `label`, optional `description`,
 *     optional `realizesChapterId`).
 *   - `foreshadows` edges derived from `outline.foreshadows[]`
 *     (`source_event_id` → `target_event_id`) on the Narrative layer.
 *   - `surfaceKind === 'work-timeline'` + `defaultLayer === 'narrative'`
 *     (architect §7.3 UX-risk override).
 *   - `projectGraph(graph)` delegates to the active layer (Narrative by
 *     default for V1.122 callers — V1.123 P2 keeps Narrative-default).
 *   - Honest empty projection: zero events → zero nodes (Task 7 owns the
 *     empty-state copy).
 *
 * Architect lock: Moment-on-Outline (frontend-only projection; backend
 * stays V1.72 `WorkOutline`). `wire_contracts_changed: true` is
 * attributable entirely to the Brief carrier (`BlockType = "era"`, owned
 * by P1) — P2 adds ZERO wire diff. This test file imports only V1.72
 * `WorkOutline` reuses; no new DTOs.
 */
import { describe, expect, it, vi } from 'vitest';
import type { Node } from '@xyflow/react';

import type { WorkOutline } from '@42ch/nexus-contracts';

import type { NexusClient } from '@/lib/nexus';
import {
  createWorkTimelineCanvasAdapter,
  projectWorkTimelineGraph,
  type WorkTimelineCanvasAdapterContext,
  type WorkTimelineNodeData,
} from '../work-timeline-canvas-adapter';

// ─── Fixture builders ──────────────────────────────────────────────────────

function outline(overrides: Partial<WorkOutline> = {}): WorkOutline {
  return {
    work_id: 'work-1',
    outline_revision: 1,
    volumes: [],
    timeline_events: [],
    foreshadows: [],
    chapter_titles: {},
    updated_at: '2026-07-18T00:00:00Z',
    ...overrides,
  } as WorkOutline;
}

function event(partial: {
  event_id: string;
  title?: string;
  description?: string | null;
  realizes_chapter_id?: number | null;
}): WorkOutline['timeline_events'][number] {
  const { event_id, title, description, realizes_chapter_id } = partial;
  const base: WorkOutline['timeline_events'][number] = {
    event_id,
    title: title ?? `Event ${event_id}`,
  };
  if (description !== undefined && description !== null) {
    base.description = description;
  }
  if (realizes_chapter_id !== undefined && realizes_chapter_id !== null) {
    base.realizes_chapter_id = realizes_chapter_id;
  }
  return base;
}

function makeMockClient(): NexusClient {
  return {
    getWorkOutline: vi.fn(),
    patchOutlineStructure: vi.fn(),
    patchOutlineChapter: vi.fn(),
    patchTimelineEvent: vi.fn(),
    health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
  } as unknown as NexusClient;
}

function makeContext(
  overrides: Partial<WorkTimelineCanvasAdapterContext> = {},
): WorkTimelineCanvasAdapterContext {
  return {
    workId: 'work-1',
    client: makeMockClient(),
    ...overrides,
  };
}

// ─── Narrative projection (architect §7 + §8) ─────────────────────────────

describe('WorkTimelineCanvasAdapter.projectGraphForLayer — Narrative projection', () => {
  it("projects outline.timeline_events onto the Narrative when-axis as 'work-timeline-narrative-event' nodes", () => {
    const g = outline({
      timeline_events: [
        event({ event_id: 'evt-1', title: 'Inciting Incident', realizes_chapter_id: 3 }),
        event({ event_id: 'evt-2', title: 'Midpoint Reversal', realizes_chapter_id: 7 }),
      ],
    });

    const adapter = createWorkTimelineCanvasAdapter(
      { current: makeContext() },
      'narrative',
    );
    const { nodes } = adapter.projectGraph(g);

    expect(nodes).toHaveLength(2);
    expect(nodes.every((n) => n.type === 'work-timeline-narrative-event')).toBe(true);
    const ids = nodes.map((n) => n.id).sort();
    expect(ids).toEqual(['wt-event:evt-1', 'wt-event:evt-2']);
  });

  it('carries Work-scoped data on each Narrative event node (workId, nodeKind, eventId, label)', () => {
    const g = outline({
      work_id: 'work-42',
      timeline_events: [
        event({
          event_id: 'evt-1',
          title: 'Coronation',
          description: 'The heir claims the throne.',
          realizes_chapter_id: 5,
        }),
      ],
    });

    const { nodes } = projectWorkTimelineGraph(g, 'narrative');
    const node = nodes[0] as Node<WorkTimelineNodeData>;

    expect(node.data.workId).toBe('work-42');
    expect(node.data.nodeKind).toBe('event');
    expect(node.data.eventId).toBe('evt-1');
    expect(node.data.label).toBe('Coronation');
    expect(node.data.description).toBe('The heir claims the throne.');
    expect(node.data.realizesChapterId).toBe(5);
  });

  it('sorts Narrative events by realizes_chapter_id (ascending) with undefined chapters tailing', () => {
    // Architect §8: sort by realizes_chapter_id then event_id.
    const g = outline({
      timeline_events: [
        event({ event_id: 'evt-late', realizes_chapter_id: 12 }),
        event({ event_id: 'evt-none' }), // no chapter anchor → tails
        event({ event_id: 'evt-early', realizes_chapter_id: 1 }),
        event({ event_id: 'evt-mid', realizes_chapter_id: 6 }),
      ],
    });

    const { nodes } = projectWorkTimelineGraph(g, 'narrative');

    // Ordering: evt-early (ch1) → evt-mid (ch6) → evt-late (ch12) → evt-none.
    const ids = nodes.map((n) => n.id);
    expect(ids).toEqual([
      'wt-event:evt-early',
      'wt-event:evt-mid',
      'wt-event:evt-late',
      'wt-event:evt-none',
    ]);
  });

  it('breaks realizes_chapter_id ties by event_id (lexicographic)', () => {
    const g = outline({
      timeline_events: [
        event({ event_id: 'evt-zeta', realizes_chapter_id: 5 }),
        event({ event_id: 'evt-alpha', realizes_chapter_id: 5 }),
        event({ event_id: 'evt-mid', realizes_chapter_id: 5 }),
      ],
    });

    const { nodes } = projectWorkTimelineGraph(g, 'narrative');
    const ids = nodes.map((n) => n.id);
    expect(ids).toEqual(['wt-event:evt-alpha', 'wt-event:evt-mid', 'wt-event:evt-zeta']);
  });

  it('positions dated events along the LR when-axis (Y = 0); undated cluster tails on the same axis', () => {
    const g = outline({
      timeline_events: [
        event({ event_id: 'evt-2', realizes_chapter_id: 7 }),
        event({ event_id: 'evt-none' }),
        event({ event_id: 'evt-1', realizes_chapter_id: 3 }),
      ],
    });

    const { nodes } = projectWorkTimelineGraph(g, 'narrative');
    const e1 = nodes.find((n) => n.id === 'wt-event:evt-1')!;
    const e2 = nodes.find((n) => n.id === 'wt-event:evt-2')!;
    const eNone = nodes.find((n) => n.id === 'wt-event:evt-none')!;

    // LR ordering: evt-1 → evt-2 → evt-none (X strictly increasing).
    expect(e1.position.x).toBeLessThan(e2.position.x);
    expect(e2.position.x).toBeLessThan(eNone.position.x);
    // All events sit on the Narrative when-axis baseline (Y = 0). The
    // undated tail stays on the axis per architect §7 — Work Timeline
    // has no Context-cluster lane; chapter-unbound events tail right
    // rather than cluster below.
    expect(e1.position.y).toBe(0);
    expect(e2.position.y).toBe(0);
    expect(eNone.position.y).toBe(0);
  });

  it('derives foreshadows edges from outline.foreshadows[] on the Narrative layer', () => {
    const g = outline({
      timeline_events: [
        event({ event_id: 'src' }),
        event({ event_id: 'tgt' }),
      ],
      foreshadows: [{ source_event_id: 'src', target_event_id: 'tgt' }],
    });

    const { edges } = projectWorkTimelineGraph(g, 'narrative');

    expect(edges).toHaveLength(1);
    const edge = edges[0];
    expect(edge.source).toBe('wt-event:src');
    expect(edge.target).toBe('wt-event:tgt');
    expect(edge.data?.relation).toBe('foreshadows');
  });

  it('drops foreshadows edges whose source or target event is absent (honest projection)', () => {
    // Mirrors the V1.108 rf-projection dangling-edge guard.
    const g = outline({
      timeline_events: [event({ event_id: 'src' })],
      foreshadows: [
        { source_event_id: 'src', target_event_id: 'missing' },
        { source_event_id: 'also-missing', target_event_id: 'src' },
      ],
    });

    const { edges } = projectWorkTimelineGraph(g, 'narrative');
    expect(edges).toEqual([]);
  });

  it('returns empty projection when outline.timeline_events is empty (honest empty)', () => {
    const g = outline({ timeline_events: [] });

    const { nodes, edges } = projectWorkTimelineGraph(g, 'narrative');
    expect(nodes).toEqual([]);
    expect(edges).toEqual([]);
  });
});

// ─── Adapter shape (architect §7.1 + §7.3) ────────────────────────────────

describe('WorkTimelineCanvasAdapter — adapter shape + default layer', () => {
  it("declares surfaceKind: 'work-timeline' and defaultLayer: 'narrative'", () => {
    const adapter = createWorkTimelineCanvasAdapter({ current: makeContext() });

    // Architect §7.1: surfaceKind is 'work-timeline' (Task 5 promotes the
    // string literal to a real CanvasSurfaceKind enum value; until then
    // the adapter casts so the contract stays type-compatible).
    expect(adapter.surfaceKind).toBe('work-timeline');
    // Architect §7.3: defaultLayer = 'narrative' (UX-risk override).
    expect(adapter.defaultLayer).toBe('narrative');
  });

  it("projectGraph delegates to the active layer (default 'narrative')", () => {
    const g = outline({
      timeline_events: [event({ event_id: 'evt-1', realizes_chapter_id: 1 })],
    });

    const adapter = createWorkTimelineCanvasAdapter({ current: makeContext() });
    const { nodes } = adapter.projectGraph(g);

    expect(nodes).toHaveLength(1);
    expect(nodes[0].type).toBe('work-timeline-narrative-event');
  });

  it("switching the adapter's active layer to 'moment' routes projectGraph through the Moment projection", () => {
    const g = outline({
      timeline_events: [event({ event_id: 'evt-1' })],
    });

    // No scene/beat fixture → Moment layer emits honest empty projection
    // (Task 3 covers the populated Moment path; here we verify the
    // delegation switch alone).
    const adapter = createWorkTimelineCanvasAdapter(
      { current: makeContext() },
      'moment',
    );
    const { nodes } = adapter.projectGraph(g);

    expect(nodes).toEqual([]);
  });

  it('registers the Narrative event node type and NO fork marker (Task 2 minimum; Task 3 adds scene/beat)', () => {
    const adapter = createWorkTimelineCanvasAdapter({ current: makeContext() });

    // Task 2 ships the Narrative event node; Task 3 ADDS the Moment scene
    // + beat nodes (verified in moment-projection.test.tsx). The Task 2
    // invariant under test here is: (1) the Narrative event node is
    // registered; (2) NO fork marker exists (V1.122 §8 + V1.123 §9
    // forbid fork markers on either Timeline surface). The full registry
    // shape (3 entries) is verified at Task 3; here we enforce the
    // Narrative invariant without freezing Task 3's additive extension.
    expect(adapter.nodeTypes['work-timeline-narrative-event']).toBeDefined();
    expect(Object.keys(adapter.nodeTypes).some((k) => k.includes('fork'))).toBe(false);
  });
});

// ─── Honest summary (architect §7) ─────────────────────────────────────────

describe('WorkTimelineCanvasAdapter.summarizeGraph — honest a11y summary', () => {
  it('counts events + foreshadows for the screen-reader live region', () => {
    const g = outline({
      timeline_events: [
        event({ event_id: 'evt-1' }),
        event({ event_id: 'evt-2' }),
        event({ event_id: 'evt-3' }),
      ],
      foreshadows: [{ source_event_id: 'evt-1', target_event_id: 'evt-2' }],
    });

    const adapter = createWorkTimelineCanvasAdapter({ current: makeContext() });
    const summary = adapter.summarizeGraph(g);

    expect(summary).toContain('3');
    expect(summary.toLowerCase()).toContain('event');
    expect(summary.toLowerCase()).toContain('foreshadow');
  });

  it('includes the ordering disclaimer when events are rendered (Work Timeline has no canonical chronology)', () => {
    const g = outline({
      timeline_events: [event({ event_id: 'evt-1' })],
    });

    const adapter = createWorkTimelineCanvasAdapter({ current: makeContext() });
    const summary = adapter.summarizeGraph(g);

    // Architect §7: do not fabricate chronology. The disclaimer surfaces
    // honestly even when events carry realizes_chapter_id (chapter order
    // is a structural hint, not a temporal chronology).
    expect(summary.toLowerCase()).toMatch(/order|chronology|inferred/);
  });

  it('omits the ordering disclaimer for zero-event outlines', () => {
    const g = outline({ timeline_events: [] });

    const adapter = createWorkTimelineCanvasAdapter({ current: makeContext() });
    const summary = adapter.summarizeGraph(g);

    expect(summary.toLowerCase()).not.toMatch(/chronology/);
    expect(summary.toLowerCase()).toContain('0');
  });
});
