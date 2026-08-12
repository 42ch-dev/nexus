/**
 * Cross-surface EVENT binding — V1.163 P1 Task 3 (Work → World deep-link).
 *
 * Upgrades the V1.123 P3 surface-level "View on World Timeline" CTA to an
 * event-level deep link: when the selected Work Narrative event carries
 * `WorkOutline.timeline_events[].world_event_id` (Task 1 carrier; referent =
 * World KB entity `key_block_id`), the CTA navigates to
 * `/worlds/:worldId/timeline?layer=narrative&event=<worldEventId>`.
 *
 * Locks pinned here:
 *   - Plan `2026-08-12-v1.163-p1-cross-surface-event-binding.md` Task 3.
 *   - AC-V1163-1 (event-level forward CTA + URL), AC-V1163-3 (inbound focus —
 *     `?event=<workEventId>` selects `wt-event:<id>`; unknown id → no phantom
 *     selection, no hard failure), AC-V1163-4/5 (Work side: surface bind
 *     fallback preserves V1.123 `?layer=narrative` CTA; no bind → hidden),
 *     AC-V1163-6 (Moment scene/beat inspectors MUST NOT surface the
 *     cross-surface CTA).
 *   - PD-5 three-state matrix: event bind → `?layer=narrative&event=<id>`;
 *     surface bind only → V1.123 `?layer=narrative`; no bind → hidden.
 *
 * Two layers of coverage:
 *   1. Inspector-level (mirrors `cross-surface-nav.test.tsx`) — the CTA slot
 *      wiring: presence (with `data-event-id`) / V1.123 fallback (no event
 *      attribute) / hide / Moment exclusion.
 *   2. Orchestrator-level (mocked NexusClient, real router) — the composed
 *      forward navigation URL (event-level + surface fallback + no-bind hide)
 *      and the inbound `?event=` focus on the Work Timeline.
 *   3. Adapter-level — `world_event_id` projects onto
 *      `WorkTimelineNodeData.worldEventId` (the inspector's event-level input).
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router';
import type { Node } from '@xyflow/react';

import { renderInApp } from '@/test/test-providers';
import type { NexusClient } from '@/lib/nexus';
import type { WorkOutline } from '@42ch/nexus-contracts';

import { WorkTimelineCanvas } from '../work-timeline-canvas';
import {
  WorkTimelineEventInspector,
  WorkTimelineMomentBeatInspector,
  WorkTimelineMomentSceneInspector,
} from '../work-timeline-inspector';
import {
  projectWorkTimelineGraph,
  type WorkTimelineNodeData,
} from '../work-timeline-canvas-adapter';

// ─── Fixtures ───────────────────────────────────────────────────────────────

const WORLD_EVENT_ID = 'kb-evt-1';

function outline(
  workId: string,
  timelineEvents: WorkOutline['timeline_events'],
): WorkOutline {
  return {
    work_id: workId,
    outline_revision: 1,
    volumes: [],
    timeline_events: timelineEvents,
    foreshadows: [],
    chapter_titles: {},
    updated_at: '2026-08-01T00:00:00Z',
  };
}

function workDetail(workId: string, worldId: string | null) {
  return {
    work_id: workId,
    status: 'active',
    title: `Work ${workId}`,
    long_term_goal: '',
    initial_idea: '',
    intake_status: 'complete',
    world_id: worldId ?? undefined,
    inspiration_log: [],
    primary_preset_id: 'preset-a',
    schedule_ids: [],
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    current_stage: 'draft',
    stage_status: 'active',
    auto_chain_enabled: false,
    auto_chain_interrupted: false,
    auto_review_master_on_timeout: false,
    total_planned_chapters: 0,
    current_chapter: 0,
  };
}

function makeMockClient(outlineData: WorkOutline, worldId: string | null): NexusClient {
  return {
    getWorkOutline: vi.fn().mockResolvedValue(outlineData),
    getWork: vi.fn().mockResolvedValue(workDetail('work-1', worldId)),
    getWorldKbGraph: vi.fn().mockResolvedValue({
      entities: [],
      source_anchors: [],
      relationships: [],
    }),
    patchOutlineStructure: vi.fn(),
    patchOutlineChapter: vi.fn(),
    patchTimelineEvent: vi.fn(),
    health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
  } as unknown as NexusClient;
}

// ─── Inspector-level fixtures (mirrors cross-surface-nav.test.tsx) ──────────

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
      label: 'Coronation beat',
      realizesChapterId: 3,
      ...overrides,
    },
  };
}

function momentSceneNode(): Node<WorkTimelineNodeData> {
  return {
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
      worldEventId: WORLD_EVENT_ID,
    },
  };
}

function momentBeatNode(): Node<WorkTimelineNodeData> {
  return {
    id: 'wt-beat:bt-1',
    type: 'work-timeline-moment-beat',
    position: { x: 0, y: 0 },
    data: {
      workId: 'work-1',
      nodeKind: 'beat',
      nodeId: 'bt-1',
      beatId: 'bt-1',
      label: 'Turn',
      realizesChapterId: 1,
      manuscriptAnchor: { chapterId: 1, sceneId: 'sc-1', beatId: 'bt-1' },
      worldEventId: WORLD_EVENT_ID,
    },
  };
}

// ─── Orchestrator-level journey (mocked client + real router) ───────────────

function WorkTimelineRoutes() {
  const location = useLocation();
  return (
    <>
      <Routes location={location}>
        <Route
          path="works/:workId/timeline"
          element={<WorkTimelineCanvas workId="work-1" />}
        />
        <Route path="*" element={<div data-testid="fallback-route" />} />
      </Routes>
      <div data-testid="location-probe">{`${location.pathname}${location.search}`}</div>
    </>
  );
}

function renderWorkApp(
  over: { outlineData?: WorkOutline; worldId?: string | null } = {},
  initial = ['/works/work-1/timeline'],
) {
  const { outlineData, worldId } = over;
  const client = makeMockClient(
    outlineData ?? outline('work-1', []),
    worldId ?? null,
  );
  return renderInApp(<WorkTimelineRoutes />, {
    client,
    initialRouterEntries: initial,
  });
}

afterEach(() => {
  vi.restoreAllMocks();
});

// ─── Inspector-level: CTA slot wiring ───────────────────────────────────────

describe('V1.163 Task 3 — Work inspector CTA slots (event-level)', () => {
  it('renders "View on World Timeline" with the event id when the node carries worldEventId', () => {
    render(
      <MemoryRouter>
        <WorkTimelineEventInspector
          node={workTimelineEventNode({ worldEventId: WORLD_EVENT_ID })}
          workId="work-1"
          worldId="world-9"
          onViewOnWorldTimeline={() => undefined}
        />
      </MemoryRouter>,
    );

    const cta = screen.queryByTestId('work-timeline-view-on-world-timeline');
    expect(cta).not.toBeNull();
    expect(cta).toHaveTextContent('View on World Timeline');
    // The CTA target carries the event id when an event-level bind exists
    // (PD-5 state 1: deep-link `?layer=narrative&event=…`).
    expect(cta).toHaveAttribute('data-world-id', 'world-9');
    expect(cta).toHaveAttribute('data-event-id', WORLD_EVENT_ID);
  });

  it('renders the CTA WITHOUT an event id when the node has no worldEventId (V1.123 fallback)', () => {
    // PD-5 state 2: surface bind only → the CTA stays but carries no event id
    // (the orchestrator falls back to `?layer=narrative`).
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
    expect(cta).not.toHaveAttribute('data-event-id');
  });

  it('hides the CTA when no bound World exists (even with a worldEventId)', () => {
    // An event bind without a surface bind is not a valid target — never
    // surface a dead CTA (honest scope cut).
    render(
      <MemoryRouter>
        <WorkTimelineEventInspector
          node={workTimelineEventNode({ worldEventId: WORLD_EVENT_ID })}
          workId="work-1"
          onViewOnWorldTimeline={() => undefined}
        />
      </MemoryRouter>,
    );

    expect(screen.queryByTestId('work-timeline-view-on-world-timeline')).toBeNull();
  });

  it('hides the CTA when the navigation callback is not wired (no phantom CTA)', () => {
    render(
      <MemoryRouter>
        <WorkTimelineEventInspector
          node={workTimelineEventNode({ worldEventId: WORLD_EVENT_ID })}
          workId="work-1"
          worldId="world-9"
        />
      </MemoryRouter>,
    );

    expect(screen.queryByTestId('work-timeline-view-on-world-timeline')).toBeNull();
  });

  it('AC-V1163-6: does NOT render the affordance on Moment scene nodes even with every slot wired', () => {
    render(
      <MemoryRouter>
        <WorkTimelineMomentSceneInspector
          node={momentSceneNode()}
          workId="work-1"
          worldId="world-9"
          onViewOnWorldTimeline={() => undefined}
        />
      </MemoryRouter>,
    );

    expect(screen.queryByTestId('work-timeline-view-on-world-timeline')).toBeNull();
  });

  it('AC-V1163-6: does NOT render the affordance on Moment beat nodes', () => {
    render(
      <MemoryRouter>
        <WorkTimelineMomentBeatInspector node={momentBeatNode()} workId="work-1" />
      </MemoryRouter>,
    );

    expect(screen.queryByTestId('work-timeline-view-on-world-timeline')).toBeNull();
  });
});

// ─── Adapter-level: world_event_id projection ───────────────────────────────

describe('V1.163 Task 3 — Narrative projection carries worldEventId (Task 1 carrier)', () => {
  it('projects world_event_id onto WorkTimelineNodeData.worldEventId; absent stays undefined', () => {
    const g = outline('work-1', [
      { event_id: 'evt-1', title: 'Coronation beat', world_event_id: WORLD_EVENT_ID },
      { event_id: 'evt-2', title: 'Unbound beat' },
    ]);

    const { nodes } = projectWorkTimelineGraph(g, 'narrative');

    const bound = nodes.find((n) => n.id === 'wt-event:evt-1')!;
    const unbound = nodes.find((n) => n.id === 'wt-event:evt-2')!;
    expect((bound.data as WorkTimelineNodeData).worldEventId).toBe(WORLD_EVENT_ID);
    expect((unbound.data as WorkTimelineNodeData).worldEventId).toBeUndefined();
  });
});

// ─── Orchestrator-level: forward deep-link + navigation ─────────────────────

describe('V1.163 Task 3 — Work → World event deep-link (AC-V1163-1/4/5, PD-5)', () => {
  it('AC-V1163-1: Work event with world_event_id + bound World → CTA click URL includes event=<worldEventId>', async () => {
    renderWorkApp(
      {
        outlineData: outline('work-1', [
          { event_id: 'evt-1', title: 'Coronation beat', world_event_id: WORLD_EVENT_ID },
        ]),
        worldId: 'world-9',
      },
      ['/works/work-1/timeline?layer=narrative&event=evt-1'],
    );

    // Inbound `?event=evt-1` selects `wt-event:evt-1` → the event inspector
    // opens with the event-level CTA.
    const cta = await screen.findByTestId('work-timeline-view-on-world-timeline');
    expect(cta).toHaveAttribute('data-event-id', WORLD_EVENT_ID);

    fireEvent.click(cta);

    await waitFor(() =>
      expect(screen.getByTestId('location-probe').textContent).toBe(
        `/worlds/world-9/timeline?layer=narrative&event=${WORLD_EVENT_ID}`,
      ),
    );
  });

  it('PD-5 (AC-V1163-4): surface bind only → CTA stays, URL has layer=narrative and NO event', async () => {
    renderWorkApp(
      {
        outlineData: outline('work-1', [
          { event_id: 'evt-1', title: 'Coronation beat' },
        ]),
        worldId: 'world-9',
      },
      ['/works/work-1/timeline?layer=narrative&event=evt-1'],
    );

    const cta = await screen.findByTestId('work-timeline-view-on-world-timeline');
    expect(cta).not.toHaveAttribute('data-event-id');

    fireEvent.click(cta);

    await waitFor(() =>
      expect(screen.getByTestId('location-probe').textContent).toBe(
        '/worlds/world-9/timeline?layer=narrative',
      ),
    );
  });

  it('AC-V1163-5: no bound World → CTA hidden (honest scope cut), even with an event bind', async () => {
    renderWorkApp(
      {
        outlineData: outline('work-1', [
          { event_id: 'evt-1', title: 'Coronation beat', world_event_id: WORLD_EVENT_ID },
        ]),
        worldId: null,
      },
      ['/works/work-1/timeline?layer=narrative&event=evt-1'],
    );

    // The inspector still opens (inbound focus), but the CTA must be absent.
    const inspector = await screen.findByTestId('work-timeline-inspector');
    expect(
      within(inspector).queryByTestId('work-timeline-view-on-world-timeline'),
    ).toBeNull();
  });
});

// ─── Orchestrator-level: inbound event focus (AC-V1163-3, Work side) ────────

describe('V1.163 Task 3 — inbound `?event=` focus on the Work Timeline (AC-V1163-3)', () => {
  it('selects the React Flow node wt-event:<id> when the event param matches a projected event', async () => {
    renderWorkApp(
      {
        outlineData: outline('work-1', [
          { event_id: 'evt-1', title: 'Coronation beat' },
        ]),
        worldId: null,
      },
      ['/works/work-1/timeline?layer=narrative&event=evt-1'],
    );

    // Selection drives the inspector (existing shell behavior — no new focus
    // primitive). The deep-linked event's inspector opening is the observable
    // proof that `wt-event:evt-1` got selected.
    const inspector = await screen.findByTestId('work-timeline-inspector');
    expect(within(inspector).getByText('Coronation beat')).toBeInTheDocument();
  });

  it('unknown event id → no phantom selection, no hard failure (Narrative layer still renders)', async () => {
    renderWorkApp(
      {
        outlineData: outline('work-1', [
          { event_id: 'evt-1', title: 'Coronation beat' },
        ]),
        worldId: null,
      },
      ['/works/work-1/timeline?layer=narrative&event=kb-unknown'],
    );

    // Canvas renders normally (no error state) on the Narrative layer.
    const canvas = await screen.findByTestId('work-timeline-canvas');
    await waitFor(() =>
      expect(canvas).toHaveAttribute('data-active-layer', 'narrative'),
    );
    // The unknown id must NOT fabricate a selection (no inspector opens).
    expect(screen.queryByTestId('work-timeline-inspector')).toBeNull();
  });
});
