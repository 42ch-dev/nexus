/**
 *
 * Verifies the per-layer empty-state contract locked by:
 *   - `iterations/v1.123/specs/three-layer-architecture.md` §7 (honest
 *     empty-state per layer).
 *   - `iterations/v1.123/specs/layer-feel-differentiation.md` §2.4 + §7
 *     (Moment empty-state copy: "No scene or beat data yet" + CTA toward
 *     Narrative / Outline).
 *   - Plan `2026-07-18-v1.123-work-timeline-narrative-moment.md` Global
 *     Constraints: "Moment empty (no scene/beat data) → fallback to Narrative
 *     with explanation; Narrative empty → today's empty-state equivalent."
 *
 * Coverage:
 *   - Outline with events, no fixture → default Narrative renders events.
 *   - User switches to Moment → Moment-empty panel renders with i18n title +
 *     body + CTA. The empty panel is testable via `work-timeline-moment-empty-state`.
 *   - Clicking the CTA flips back to Narrative (escape hatch).
 *   - Outline globally empty → V1.122-style global empty-state preserves;
 *     the Moment-empty panel MUST NOT render on the global-empty branch.
 *   - V1.156 P2 T2 — Brief-empty panel: no bound World / no era data →
 *     honest empty-state (PD-2 copy: world-shape context comes from the
 *     bound World's Brief; NO "create Brief" CTA); CTA flips back to
 *     Narrative; era data present → canvas renders (no crash on Brief-era
 *     nodes); global-empty branch → Brief panel MUST NOT render.
 *
 * Mount strategy mirrors `layer-switcher.test.tsx`: a mocked `NexusClient`
 * resolves `getWorkOutline` to a per-test fixture; MSW is not needed because
 * the client mock intercepts before HTTP.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, screen, waitFor, within } from '@testing-library/react';

import { renderInApp } from '@/test/test-providers';
import type { NexusClient } from '@/lib/nexus';
import type {
  WorldKbEntityProjection,
  WorldKbGraphResponse,
  WorkOutline,
} from '@42ch/nexus-contracts';

import { WorkTimelineCanvas } from '../work-timeline-canvas';

function outline(overrides: Partial<WorkOutline> = {}): WorkOutline {
  return {
    work_id: 'work-1',
    outline_revision: 1,
    volumes: [],
    timeline_events: [
      { event_id: 'evt-1', title: 'Inciting Incident', realizes_chapter_id: 1 },
    ],
    foreshadows: [],
    scenes: [],
    beats: [],
    chapter_titles: {},
    updated_at: '2026-07-18T00:00:00Z',
    ...overrides,
  } as WorkOutline;
}

function worldEntity(
  overrides: Partial<WorldKbEntityProjection> &
    Pick<WorldKbEntityProjection, 'key_block_id' | 'block_type' | 'canonical_name'>,
): WorldKbEntityProjection {
  return {
    world_id: 'world-es',
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
  return worldEntity({
    key_block_id,
    block_type: 'era',
    canonical_name,
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

/**
 *
 * `world` is optional (V1.156 P2 T2 — Work-Brief era data): when supplied,
 * the Work detail carries a bound `world_id` and `getWorldKbGraph` resolves
 * the bound World's graph (the Brief layer's era data source). When absent,
 * the Work is unbound → `useWorldKbGraph` is disabled → the Brief layer has
 * no era data (honest Brief-empty panel).
 *
 * `getWorldKbGraphImpl` overrides the graph fetch (V1.156 P2 fix-wave F1 —
 * loading/error status tests): pass a deferred or rejecting promise to
 * exercise the orchestrator's `worldKbGraphQuery.isLoading` / `isError`
 * gates instead of the immediate-resolve default.
 */
function makeMockClient(
  outlineData: WorkOutline,
  world?: { worldId: string; graph: WorldKbGraphResponse },
  getWorldKbGraphImpl?: (worldId: string) => Promise<WorldKbGraphResponse>,
): NexusClient {
  return {
    getWorkOutline: vi.fn().mockResolvedValue(outlineData),
    getWork: vi.fn().mockResolvedValue({
      work_id: 'work-1',
      world_id: world?.worldId ?? null,
    }),
    getWorldKbGraph:
      getWorldKbGraphImpl ??
      (world ? vi.fn().mockResolvedValue(world.graph) : vi.fn()),
    patchOutlineStructure: vi.fn(),
    patchOutlineChapter: vi.fn(),
    patchTimelineEvent: vi.fn(),
    health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
  } as unknown as NexusClient;
}

describe('WorkTimelineCanvas — global empty-state preserved (V1.122 family)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('renders the global empty-state when the outline has zero events (Narrative empty)', async () => {
    const client = makeMockClient(outline({ timeline_events: [] }));
    renderInApp(<WorkTimelineCanvas workId="work-1" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });

    expect(screen.getByText("This work's timeline is empty")).toBeInTheDocument();

    expect(screen.queryByTestId('work-timeline-moment-empty-state')).toBeNull();
    expect(screen.queryByTestId('work-timeline-layer-tab-moment')).toBeNull();
  });
});

describe('WorkTimelineCanvas — Moment-layer honest empty-state (V1.123 P2 Task 7)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('does NOT render the Moment-empty panel when default layer is Narrative (events present, no fixture)', async () => {
    const client = makeMockClient(outline());
    renderInApp(<WorkTimelineCanvas workId="work-1" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });

    expect(screen.queryByTestId('work-timeline-moment-empty-state')).toBeNull();
  });

  it('renders the Moment-empty panel when the user switches to Moment with no fixture', async () => {
    const client = makeMockClient(outline());
    renderInApp(<WorkTimelineCanvas workId="work-1" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });

    expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
      'data-active-layer',
      'narrative',
    );

    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-moment'));

    const momentEmpty = await screen.findByTestId('work-timeline-moment-empty-state');
    expect(momentEmpty).toBeInTheDocument();
    expect(momentEmpty).toHaveTextContent('No scene or beat data yet');
    expect(momentEmpty).toHaveTextContent('Switch to Narrative');
  });

  it('clicking the Moment-empty CTA switches back to Narrative (escape hatch)', async () => {
    const client = makeMockClient(outline());
    renderInApp(<WorkTimelineCanvas workId="work-1" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-moment'));
    const cta = await screen.findByTestId('work-timeline-moment-empty-cta');
    expect(cta).toBeInTheDocument();

    fireEvent.click(cta);
    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });

    expect(screen.queryByTestId('work-timeline-moment-empty-state')).toBeNull();
  });

  it('does NOT render the Moment-empty panel when a scene/beat fixture provides data', async () => {
    const client = makeMockClient(outline());
    renderInApp(
      <WorkTimelineCanvas
        workId="work-1"
        sceneBeatFixture={{
          scenes: [
            {
              sceneId: 'sc-1',
              chapterId: 1,
              title: 'Opening',
              status: null,
            },
          ],
          beats: [],
        }}
      />,
      { client },
    );

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-moment'));

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'moment',
      );
    });
    expect(screen.queryByTestId('work-timeline-moment-empty-state')).toBeNull();
  });
});

describe('WorkTimelineCanvas — Brief-layer honest empty-state (V1.156 P2 T2)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('renders the Brief-empty panel when the user switches to Brief with no bound World', async () => {
    const client = makeMockClient(outline());
    renderInApp(<WorkTimelineCanvas workId="work-1" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-brief'));

    const briefEmpty = await screen.findByTestId('work-timeline-brief-empty-state');
    expect(briefEmpty).toBeInTheDocument();
    expect(briefEmpty).toHaveTextContent('No world-shape context yet');
    expect(briefEmpty).toHaveTextContent('bound to a World with era markers');
    expect(briefEmpty).toHaveTextContent('Switch to Narrative');
    expect(briefEmpty).not.toHaveTextContent('Create Brief');
    expect(briefEmpty).not.toHaveTextContent('Add era');
  });

  it('renders the Brief-empty panel when the bound World has no era markers', async () => {
    const client = makeMockClient(outline(), {
      worldId: 'world-es',
      graph: {
        entities: [
          worldEntity({
            key_block_id: 'kb-event-1',
            block_type: 'event',
            canonical_name: 'Coronation',
          }),
        ],
        source_anchors: [],
        relationships: [],
      },
    });
    renderInApp(<WorkTimelineCanvas workId="work-1" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-brief'));

    const briefEmpty = await screen.findByTestId('work-timeline-brief-empty-state');
    expect(briefEmpty).toBeInTheDocument();
    expect(briefEmpty).toHaveTextContent('No world-shape context yet');
  });

  it('clicking the Brief-empty CTA switches back to Narrative (escape hatch)', async () => {
    const client = makeMockClient(outline());
    renderInApp(<WorkTimelineCanvas workId="work-1" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-brief'));
    const cta = await screen.findByTestId('work-timeline-brief-empty-cta');
    expect(cta).toBeInTheDocument();

    fireEvent.click(cta);
    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });

    expect(screen.queryByTestId('work-timeline-brief-empty-state')).toBeNull();
  });

  it('does NOT render the Brief-empty panel when the bound World has era data', async () => {
    const client = makeMockClient(outline(), {
      worldId: 'world-es',
      graph: {
        entities: [
          eraEntity({
            key_block_id: 'kb-era-1',
            canonical_name: 'The First Age',
          }),
        ],
        source_anchors: [],
        relationships: [],
      },
    });
    renderInApp(<WorkTimelineCanvas workId="work-1" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-brief'));

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'brief',
      );
    });
    expect(screen.queryByTestId('work-timeline-brief-empty-state')).toBeNull();
    expect(
      screen.queryByTestId('work-timeline-canvas-layer-transition'),
    ).not.toBeNull();
  });

  it('does NOT render the Brief-empty panel on the global-empty branch', async () => {
    const client = makeMockClient(outline({ timeline_events: [] }));
    renderInApp(<WorkTimelineCanvas workId="work-1" />, {
      client,
      initialRouterEntries: ['/works/work-1/timeline?layer=brief'],
    });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });

    expect(screen.getByText("This work's timeline is empty")).toBeInTheDocument();
    expect(screen.queryByTestId('work-timeline-brief-empty-state')).toBeNull();
    expect(screen.queryByTestId('work-timeline-layer-tab-brief')).toBeNull();
  });

  it('shows a loading state (not the Brief-empty panel) while the bound-World graph is in flight', async () => {
    let resolveGraph!: (value: WorldKbGraphResponse) => void;
    const graphPromise = new Promise<WorldKbGraphResponse>((resolve) => {
      resolveGraph = resolve;
    });
    const graphFn = vi.fn(() => graphPromise);

    const client = makeMockClient(
      outline(),
      {
        worldId: 'world-es',
        graph: { entities: [], source_anchors: [], relationships: [] },
      },
      graphFn,
    );
    renderInApp(<WorkTimelineCanvas workId="work-1" />, {
      client,
      initialRouterEntries: ['/works/work-1/timeline?layer=brief'],
    });

    await waitFor(() => {
      expect(graphFn).toHaveBeenCalled();
    });
    expect(screen.getByText('Loading Work Timeline…')).toBeInTheDocument();
    expect(screen.queryByTestId('work-timeline-brief-empty-state')).toBeNull();

    resolveGraph({
      entities: [
        eraEntity({
          key_block_id: 'kb-era-1',
          canonical_name: 'The First Age',
        }),
      ],
      source_anchors: [],
      relationships: [],
    });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'brief',
      );
    });
    expect(screen.queryByTestId('work-timeline-brief-empty-state')).toBeNull();
  });

  it('shows an ErrorState with a retry CTA (not the Brief-empty panel) when the bound-World graph fetch fails', async () => {
    const client = makeMockClient(
      outline(),
      {
        worldId: 'world-es',
        graph: { entities: [], source_anchors: [], relationships: [] },
      },
      vi
        .fn()
        .mockRejectedValueOnce(new Error('graph fetch failed'))
        .mockResolvedValueOnce({
          entities: [
            eraEntity({
              key_block_id: 'kb-era-1',
              canonical_name: 'The First Age',
            }),
          ],
          source_anchors: [],
          relationships: [],
        }),
    );
    renderInApp(<WorkTimelineCanvas workId="work-1" />, {
      client,
      initialRouterEntries: ['/works/work-1/timeline?layer=brief'],
    });

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('Could not load the work timeline.');
    expect(screen.queryByTestId('work-timeline-brief-empty-state')).toBeNull();

    fireEvent.click(within(alert).getByRole('button'));

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'brief',
      );
    });
    expect(screen.queryByTestId('work-timeline-brief-empty-state')).toBeNull();
  });
});
