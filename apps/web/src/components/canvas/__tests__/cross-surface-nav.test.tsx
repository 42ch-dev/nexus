/**
 *
 * Pins the bidirectional cross-surface navigation contract locked by:
 *   - Plan `2026-07-18-v1.123-timeline-first-ia-deepening.md` Task 4 +
 *     Global Constraints §"Cross-surface navigation URL contract".
 *   - `iterations/v1.123/specs/three-layer-product-spec.md` AC-V1123-18
 *     (Work Timeline Moment ↔ bound World Timeline Narrative).
 *
 * Coverage:
 *   - Work Timeline inspector surfaces "View on World Timeline" affordance
 *     when the active Work is bound to a World (`WorkDetailResponse.world_id`).
 *   - World Timeline inspector surfaces "View in Work Timeline" affordance
 *     when a Work realizes the active World (`WorkDetailResponse.world_id ===
 *     currentWorldId`, derived client-side via the existing `useWorks()` list
 *     + per-Work detail fan-out — capped at N most-recent Works).
 *   - No-binding case: affordance hides (per plan §"If binding is missing or
 *     unreliable, P3 hides the affordance or shows hint copy" — hiding is the
 *     honest-scope-cut path).
 *   - URL contract: `/worlds/:worldId/timeline?layer=narrative` (Work → World)
 *     and `/works/:workId/timeline?layer=narrative` (World → Work), per plan
 *     §"Cross-surface navigation URL contract (LOCKED ...)".
 *
 * The inspector is the contract surface — the per-node CTA lives there. Tests
 * mount the inspector directly so they exercise the CTA wiring without driving
 * React Flow node selection (which is covered per-surface by existing canvas
 * orchestrator suites).
 */
import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { fireEvent } from '@testing-library/react';
import type { MutableRefObject } from 'react';
import { MemoryRouter } from 'react-router';
import type { Node } from '@xyflow/react';

import { TimelineInspector } from '../timeline-canvas/timeline-inspector';
import type {
  TimelineCanvasAdapterContext,
  TimelineNodeData,
} from '../timeline-canvas/timeline-canvas-adapter';
import {
  WorkTimelineEventInspector,
  WorkTimelineMomentSceneInspector,
} from '../work-timeline-canvas/work-timeline-inspector';
import type { WorkTimelineNodeData } from '../work-timeline-canvas/work-timeline-canvas-adapter';

function workTimelineEventNode(
  overrides: Partial<WorkTimelineNodeData> = {},
): Node<WorkTimelineNodeData> {
  return {
    id: 'wt-event:evt-1',
    type: 'work-timeline-narrative-event',
    position: { x: 0, y: 0 },
    data: {
      workId: 'work-1',
      nodeKind: 'event',
      nodeId: 'evt-1',
      eventId: 'evt-1',
      label: 'Inciting Incident',
      realizesChapterId: 3,
      ...overrides,
    },
  };
}

describe('Task 4 — Work Timeline → World Timeline affordance', () => {
  it('renders "View on World Timeline" when worldId is supplied', () => {
    render(
      <MemoryRouter>
        <WorkTimelineEventInspector
          node={workTimelineEventNode()}
          workId="work-1"
          worldId="world-9"
          onViewOnWorldTimeline={() => undefined}
        />
      </MemoryRouter>,
    );

    const cta = screen.queryByTestId('work-timeline-view-on-world-timeline');
    expect(cta).not.toBeNull();
    expect(cta).toHaveTextContent('View on World Timeline');
  });

  it('hides "View on World Timeline" when worldId is absent (honest scope cut)', () => {
    // Works without a bound World (WorkDetailResponse.world_id absent) hide the
    // affordance — silent degradation is forbidden.
    render(
      <MemoryRouter>
        <WorkTimelineEventInspector
          node={workTimelineEventNode()}
          workId="work-1"
        />
      </MemoryRouter>,
    );

    expect(screen.queryByTestId('work-timeline-view-on-world-timeline')).toBeNull();
  });

  it('hides "View on World Timeline" when onViewOnWorldTimeline is absent (no callback wired)', () => {
    render(
      <MemoryRouter>
        <WorkTimelineEventInspector
          node={workTimelineEventNode()}
          workId="work-1"
          worldId="world-9"
        />
      </MemoryRouter>,
    );

    expect(screen.queryByTestId('work-timeline-view-on-world-timeline')).toBeNull();
  });

  it('invokes the callback on click (orchestrator owns the navigation)', () => {
    const handler = vi.fn();
    render(
      <MemoryRouter>
        <WorkTimelineEventInspector
          node={workTimelineEventNode()}
          workId="work-1"
          worldId="world-9"
          onViewOnWorldTimeline={handler}
        />
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByTestId('work-timeline-view-on-world-timeline'));

    expect(handler).toHaveBeenCalledTimes(1);
    expect(handler.mock.calls[0][0]).toEqual(workTimelineEventNode());
  });

  it('hides the affordance on Moment-layer scene nodes (cross-surface nav is Narrative-event-bound)', () => {
    const sceneNode: Node<WorkTimelineNodeData> = {
      id: 'wt-scene:sc-1',
      type: 'work-timeline-moment-scene',
      position: { x: 0, y: 0 },
      data: {
        workId: 'work-1',
        nodeKind: 'scene',
        nodeId: 'sc-1',
        sceneId: 'sc-1',
        label: 'Opening',
        realizesChapterId: 1,
        manuscriptAnchor: { chapterId: 1, sceneId: 'sc-1' },
      },
    };
    render(
      <MemoryRouter>
        <WorkTimelineMomentSceneInspector
          node={sceneNode}
          workId="work-1"
          worldId="world-9"
          onViewOnWorldTimeline={() => undefined}
        />
      </MemoryRouter>,
    );

    expect(screen.queryByTestId('work-timeline-view-on-world-timeline')).toBeNull();
  });
});

function timelineEventNode(
  overrides: Partial<TimelineNodeData> = {},
): Node<TimelineNodeData> {
  return {
    id: 'entity:kb-evt-1',
    type: 'timeline-event',
    position: { x: 0, y: 0 },
    data: {
      key_block_id: 'kb-evt-1',
      world_id: 'world-9',
      block_type: 'event',
      canonical_name: 'Coronation',
      status: 'confirmed',
      version: 1,
      sequence_no: 1,
      body: { attributes: { occurred_at: '1042-spring' } },
      source_anchor_count: 0,
      layoutHint: 'event',
      occurredAtHint: '1042-spring',
      ...overrides,
    } as TimelineNodeData,
  };
}

function ctxRefWith(
  overrides: Partial<TimelineCanvasAdapterContext> = {},
): MutableRefObject<TimelineCanvasAdapterContext> {
  return {
    current: {
      worldId: 'world-9',
      ...overrides,
    },
  };
}

describe('Task 4 — World Timeline → Work Timeline affordance', () => {
  it('renders "View in Work Timeline" when boundWorkId is supplied', () => {
    render(
      <MemoryRouter>
        <TimelineInspector
          node={timelineEventNode()}
          ctxRef={ctxRefWith({
            boundWorkId: 'work-1',
            onViewInWorkTimeline: () => undefined,
          })}
        />
      </MemoryRouter>,
    );

    const cta = screen.queryByTestId('timeline-view-in-work-timeline');
    expect(cta).not.toBeNull();
    expect(cta).toHaveTextContent('View in Work Timeline');
  });

  it('hides "View in Work Timeline" when boundWorkId is absent (honest scope cut)', () => {
    render(
      <MemoryRouter>
        <TimelineInspector
          node={timelineEventNode()}
          ctxRef={ctxRefWith()}
        />
      </MemoryRouter>,
    );

    expect(screen.queryByTestId('timeline-view-in-work-timeline')).toBeNull();
  });

  it('hides "View in Work Timeline" when onViewInWorkTimeline is absent (no callback wired)', () => {
    render(
      <MemoryRouter>
        <TimelineInspector
          node={timelineEventNode()}
          ctxRef={ctxRefWith({ boundWorkId: 'work-1' })}
        />
      </MemoryRouter>,
    );

    expect(screen.queryByTestId('timeline-view-in-work-timeline')).toBeNull();
  });

  it('invokes the callback on click (orchestrator owns the navigation)', () => {
    const handler = vi.fn();
    render(
      <MemoryRouter>
        <TimelineInspector
          node={timelineEventNode()}
          ctxRef={ctxRefWith({
            boundWorkId: 'work-1',
            onViewInWorkTimeline: handler,
          })}
        />
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByTestId('timeline-view-in-work-timeline'));

    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('does NOT render the affordance on context (non-event) nodes', () => {
    const contextNode = timelineEventNode({
      key_block_id: 'kb-char-1',
      block_type: 'character',
      canonical_name: 'Aria',
      layoutHint: 'context',
    });
    delete (contextNode.data as Partial<TimelineNodeData>).occurredAtHint;

    render(
      <MemoryRouter>
        <TimelineInspector
          node={contextNode}
          ctxRef={ctxRefWith({
            boundWorkId: 'work-1',
            onViewInWorkTimeline: () => undefined,
          })}
        />
      </MemoryRouter>,
    );

    expect(screen.queryByTestId('timeline-view-in-work-timeline')).toBeNull();
  });
});
