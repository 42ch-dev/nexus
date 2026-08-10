/**
 * Cross-surface Timeline navigation — V1.123 P3 Task 4.
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

// ─── Work Timeline — View on World Timeline affordance ──────────────────────

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
    // Per plan §"If binding is missing or unreliable, P3 hides the affordance".
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
    // Even if a worldId is somehow present, the CTA hides when the orchestrator
    // has not wired the navigation callback. Guards against a phantom CTA that
    // navigates nowhere.
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
    // The callback receives the selected node so the orchestrator can read
    // node.data.realizesChapterId etc. when composing the navigation URL.
    expect(handler.mock.calls[0][0]).toEqual(workTimelineEventNode());
  });

  it('hides the affordance on Moment-layer scene nodes (cross-surface nav is Narrative-event-bound)', () => {
    // The plan calls out "Work Timeline Moment ↔ World Timeline Narrative" as
    // the binding axis, but the durable V1.123 slice is Narrative-event-driven
    // (WorkDetailResponse.world_id is the only join the wire exposes today).
    // Moment scene/beat nodes do NOT surface this CTA — the inspector's
    // signature still accepts the slots for forward compatibility.
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

    // The cross-surface affordance is reserved for Narrative-event binding
    // (architect §3.4 — Moment-on-Outline carrier has no Work-event →
    // World-event binding today). Moment inspectors MUST NOT surface it.
    expect(screen.queryByTestId('work-timeline-view-on-world-timeline')).toBeNull();
  });
});

// ─── World Timeline — View in Work Timeline affordance ──────────────────────

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
    // Worlds with no realizing Work (zero Works in the workspace, or every Work
    // binds to a different World) hide the affordance. Silent degradation is
    // forbidden.
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
    // The cross-surface affordance is reserved for `layoutHint='event'` nodes
    // (the World-event → Work-Timeline binding axis). Context entities
    // (characters, locations) do not surface this CTA even when a realizing
    // Work exists — they are not on the narrative when-axis.
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
