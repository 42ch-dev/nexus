/**
 * TimelineCanvasAdapter — V1.122 P1 T5 (a11y + alt-view companion).
 *
 * Verifies the architect-locked accessibility + non-spatial alt-view contract:
 *   - `renderAltView()` produces a sortable Timeline event/KeyBlock table that
 *     reads from the adapter context (mirrors the V1.114 World KB alt-view
 *     pattern). Selection routes through the orchestrator's `onSelectNode`
 *     callback — the write-equivalent for `kb.patch_entity` lives in the
 *     inspector that the selection opens, NOT in the alt-view itself.
 *   - `summarizeGraph(graph)` returns a non-empty screen-reader live-region
 *     string for every graph state (empty, dense, partial temporal signal).
 *   - The architect-locked ordering disclaimer
 *     ("Ordering inferred from available event data; not a canonical
 *     chronology.") is present whenever temporal signals are partial OR absent
 *     (zero events OR any event lacking `body.attributes.occurred_at`).
 *
 * Architect lock (§4.2 write boundary): the alt-view MUST NOT wire
 * `timeline.patch_event` (Work-scoped). The negative assertion future-proofs
 * the surface against an accidental Work-scoped write leaking in via the
 * alt-view's selection path.
 *
 * @see `iterations/v1.122/specs/timeline-canvas-architecture.md` §7 + §9
 * @see `specs/canvas-strategy-surface.md` §4.4 a11y requirements
 */
import { describe, expect, it, vi } from 'vitest';
import { fireEvent, screen } from '@testing-library/react';
import type { Node } from '@xyflow/react';

import type {
  WorldKbEntityProjection,
  WorldKbGraphResponse,
} from '@42ch/nexus-contracts';

import type { NexusClient } from '@/lib/nexus';
import { renderInApp } from '@/test/test-providers';

import {
  createTimelineCanvasAdapter,
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
    updated_at: '2026-01-01T00:00:00Z',
    ...overrides,
  } as WorldKbEntityProjection;
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

const ORDERING_DISCLAIMER =
  'Ordering inferred from available event data; not a canonical chronology.';

// ─── renderAltView — sortable entity table companion ────────────────────────

describe('TimelineCanvasAdapter.renderAltView — non-spatial sortable table', () => {
  it('renders a sortable table of Timeline events + KeyBlocks from the adapter context', () => {
    const eventNode: Node<TimelineNodeData> = {
      id: 'entity:kb-event-1',
      type: 'timeline-event',
      position: { x: 0, y: 0 },
      data: {
        ...entity({
          key_block_id: 'kb-event-1',
          block_type: 'event',
          canonical_name: 'Coronation',
          body: { attributes: { occurred_at: '1042-03-01T00:00:00Z' } },
          source_anchor_count: 2,
        }),
        layoutHint: 'event',
        occurredAtHint: '1042-03-01T00:00:00Z',
      },
    };
    const contextNode: Node<TimelineNodeData> = {
      id: 'entity:kb-char-1',
      type: 'timeline-key-block',
      position: { x: 0, y: -220 },
      data: {
        ...entity({
          key_block_id: 'kb-char-1',
          block_type: 'character',
          canonical_name: 'Aria',
          source_anchor_count: 0,
        }),
        layoutHint: 'context',
      },
    };

    const ctxRef = {
      current: {
        worldId: 'world-7',
        client: makeMockClient(),
        nodes: [eventNode, contextNode],
        selectedNodeId: null,
        onSelectNode: vi.fn(),
      } as TimelineCanvasAdapterContext,
    };
    const adapter = createTimelineCanvasAdapter(ctxRef);

    const { container } = renderInApp(<>{adapter.renderAltView!()}</>);

    // Section landmark with an accessible label — screen readers can navigate
    // to the alt-view by name (canvas-strategy-surface.md §4.4 a11y).
    const section = container.querySelector('section[aria-label]');
    expect(section).not.toBeNull();

    // Both entities render by their canonical name (event + context entity).
    expect(screen.getByText('Coronation')).toBeInTheDocument();
    expect(screen.getByText('Aria')).toBeInTheDocument();

    // The block-type column surfaces the kind (mirrors the World KB entity
    // table `blockType` column — canvas-strategy-surface.md §4.4 parity).
    expect(screen.getByText('Event')).toBeInTheDocument();
    expect(screen.getByText('Character')).toBeInTheDocument();

    // Sortable column headers are present (at least the Title column).
    const columnHeaders = container.querySelectorAll('th[scope="col"]');
    expect(columnHeaders.length).toBeGreaterThanOrEqual(3);
  });

  it('sorts rows by the Title column asc/desc on header click', () => {
    const a: Node<TimelineNodeData> = {
      id: 'entity:kb-a',
      type: 'timeline-event',
      position: { x: 0, y: 0 },
      data: {
        ...entity({
          key_block_id: 'kb-a',
          block_type: 'event',
          canonical_name: 'Zeta Event',
        }),
        layoutHint: 'event',
      },
    };
    const b: Node<TimelineNodeData> = {
      id: 'entity:kb-b',
      type: 'timeline-event',
      position: { x: 0, y: 0 },
      data: {
        ...entity({
          key_block_id: 'kb-b',
          block_type: 'event',
          canonical_name: 'Alpha Event',
        }),
        layoutHint: 'event',
      },
    };

    const onSelectNode = vi.fn();
    const ctxRef = {
      current: {
        worldId: 'world-7',
        client: makeMockClient(),
        // intentionally pre-sorted Z → A; the table default sort is
        // `title asc`, so the initial render re-orders to A → Z.
        nodes: [a, b],
        selectedNodeId: null,
        onSelectNode,
      } as TimelineCanvasAdapterContext,
    };
    const adapter = createTimelineCanvasAdapter(ctxRef);

    const { container, getByRole } = renderInApp(<>{adapter.renderAltView!()}</>);

    const rows = () => container.querySelectorAll('tbody tr[tabindex]');
    expect(rows().length).toBe(2);

    // Default sort is Title ascending — Alpha Event sorts before Zeta Event
    // regardless of the input order.
    expect(rows()[0]).toHaveTextContent('Alpha Event');
    expect(rows()[1]).toHaveTextContent('Zeta Event');

    // Click the Title column header button — toggles to descending sort.
    const titleHeader = getByRole('columnheader', { name: /Title/i });
    const titleButton = titleHeader.querySelector('button')!;
    fireEvent.click(titleButton);
    expect(rows()[0]).toHaveTextContent('Zeta Event');
    expect(rows()[1]).toHaveTextContent('Alpha Event');

    // Click again — toggles back to ascending.
    fireEvent.click(titleButton);
    expect(rows()[0]).toHaveTextContent('Alpha Event');
    expect(rows()[1]).toHaveTextContent('Zeta Event');
  });

  it('invokes onSelectNode when a row is clicked (selection opens the inspector — the kb.patch_entity write path)', () => {
    const node: Node<TimelineNodeData> = {
      id: 'entity:kb-event-1',
      type: 'timeline-event',
      position: { x: 0, y: 0 },
      data: {
        ...entity({
          key_block_id: 'kb-event-1',
          block_type: 'event',
          canonical_name: 'Coronation',
        }),
        layoutHint: 'event',
      },
    };
    const onSelectNode = vi.fn();
    const ctxRef = {
      current: {
        worldId: 'world-7',
        client: makeMockClient(),
        nodes: [node],
        selectedNodeId: null,
        onSelectNode,
      } as TimelineCanvasAdapterContext,
    };
    const adapter = createTimelineCanvasAdapter(ctxRef);

    const { container } = renderInApp(<>{adapter.renderAltView!()}</>);

    const row = container.querySelector('tbody tr[tabindex]')!;
    fireEvent.click(row);

    expect(onSelectNode).toHaveBeenCalledTimes(1);
    // The selection callback receives the node id (the orchestrator selects
    // the matching React Flow node → opens the inspector that routes the
    // patch through `worldKbPatchEntity`).
    expect(onSelectNode).toHaveBeenCalledWith('entity:kb-event-1');
  });

  it('does NOT wire timeline.patch_event from the alt-view (Work-scoped write boundary)', () => {
    const client = makeMockClient();
    const node: Node<TimelineNodeData> = {
      id: 'entity:kb-event-1',
      type: 'timeline-event',
      position: { x: 0, y: 0 },
      data: {
        ...entity({
          key_block_id: 'kb-event-1',
          block_type: 'event',
          canonical_name: 'Coronation',
        }),
        layoutHint: 'event',
      },
    };
    const ctxRef = {
      current: {
        worldId: 'world-7',
        client,
        nodes: [node],
        selectedNodeId: null,
        onSelectNode: vi.fn(),
      } as TimelineCanvasAdapterContext,
    };
    const adapter = createTimelineCanvasAdapter(ctxRef);

    renderInApp(<>{adapter.renderAltView!()}</>);

    // The alt-view MUST NOT invoke any write method — it is a read-only
    // companion that selects a node. The inspector (opened by selection)
    // owns the kb.patch_entity write path.
    expect(client.patchTimelineEvent).not.toHaveBeenCalled();
    expect(client.worldKbPatchEntity).not.toHaveBeenCalled();
    expect(client.worldKbPatchRelationship).not.toHaveBeenCalled();
    expect(client.worldKbPromoteCandidate).not.toHaveBeenCalled();
  });

  it('renders an honest empty state when the Timeline has zero nodes', () => {
    const ctxRef = {
      current: {
        worldId: 'world-7',
        client: makeMockClient(),
        nodes: [],
        selectedNodeId: null,
        onSelectNode: vi.fn(),
      } as TimelineCanvasAdapterContext,
    };
    const adapter = createTimelineCanvasAdapter(ctxRef);

    renderInApp(<>{adapter.renderAltView!()}</>);

    // The honest empty-state copy is surfaced (canvas-strategy-surface.md
    // §4.4 + timeline-canvas-architecture.md §7 honest empty-state). The
    // i18n catalog ships "No Timeline entities yet." under
    // `timeline.altView.empty`; match loosely so copy refinements don't
    // break the a11y guarantee.
    expect(screen.getByText(/no timeline entities|empty/i)).toBeInTheDocument();
  });
});

// ─── summarizeGraph — non-empty screen-reader live region ───────────────────

describe('TimelineCanvasAdapter.summarizeGraph — a11y live region (re-verification)', () => {
  function emptyGraph(): WorldKbGraphResponse {
    return { entities: [], source_anchors: [], relationships: [] };
  }

  it('returns a non-empty string for an empty graph (empty-state SR coverage)', () => {
    const adapter = createTimelineCanvasAdapter({
      current: { worldId: 'world-7' },
    });
    const summary = adapter.summarizeGraph(emptyGraph());
    expect(typeof summary).toBe('string');
    expect(summary.length).toBeGreaterThan(0);
    // The architect-locked §7 disclaimer MUST be present when the timeline
    // has zero events — the live region announces the empty state honestly.
    expect(summary).toContain(ORDERING_DISCLAIMER);
  });

  it('returns a non-empty string that includes the ordering disclaimer when temporal signals are partial', () => {
    const adapter = createTimelineCanvasAdapter({
      current: { worldId: 'world-7' },
    });
    const graph: WorldKbGraphResponse = {
      // One event WITH occurred_at + one event WITHOUT — partial signal.
      entities: [
        entity({
          key_block_id: 'kb-dated',
          block_type: 'event',
          canonical_name: 'Coronation',
          body: { attributes: { occurred_at: '1042-03-01T00:00:00Z' } },
        }),
        entity({
          key_block_id: 'kb-undated',
          block_type: 'event',
          canonical_name: 'Forgotten Battle',
        }),
      ],
      source_anchors: [],
      relationships: [],
    };
    const summary = adapter.summarizeGraph(graph);
    expect(summary.length).toBeGreaterThan(0);
    expect(summary).toContain(ORDERING_DISCLAIMER);
  });

  it('omits the disclaimer only when every event carries occurred_at', () => {
    const adapter = createTimelineCanvasAdapter({
      current: { worldId: 'world-7' },
    });
    const graph: WorldKbGraphResponse = {
      entities: [
        entity({
          key_block_id: 'kb-1',
          block_type: 'event',
          canonical_name: 'A',
          body: { attributes: { occurred_at: '1000-01-01T00:00:00Z' } },
        }),
        entity({
          key_block_id: 'kb-2',
          block_type: 'event',
          canonical_name: 'B',
          body: { attributes: { occurred_at: '1001-01-01T00:00:00Z' } },
        }),
      ],
      source_anchors: [],
      relationships: [],
    };
    const summary = adapter.summarizeGraph(graph);
    expect(summary.length).toBeGreaterThan(0);
    expect(summary).not.toContain(ORDERING_DISCLAIMER);
  });
});
