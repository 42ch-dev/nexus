/**
 * Layer breadcrumbs + cross-layer navigation affordances — V1.123 P4 Task 5.
 *
 * Locks the breadcrumb contract from
 * `iterations/v1.123/specs/layer-feel-differentiation.md` §3.4:
 *
 *   | Surface        | Breadcrumb pattern                          |
 *   |----------------|---------------------------------------------|
 *   | World Timeline | `Brief` or `Brief > Narrative` or `Narrative`|
 *   | Work Timeline  | `Brief` or `Brief > Narrative` or `Narrative`|
 *
 * Breadcrumbs are clickable zoom-out targets (parent layer).
 *
 * Coverage:
 *   - World Timeline: when on Narrative layer, breadcrumb shows the path
 *     `Brief > Narrative`; clicking the Brief segment switches back to Brief
 *     (zoom-out affordance).
 *   - World Timeline: when on Brief layer, breadcrumb shows just `Brief`
 *     (no parent layer to zoom out to).
 *   - Work Timeline (V1.156 P2 T2 — Work×Brief closed): the breadcrumb pair
 *     becomes `Brief > Narrative`, mirroring the World Timeline — Brief is
 *     the coarsest (world-shape) layer. Moment sits outside the pair (same
 *     as World Timeline Moment); the switcher tabs remain its affordance.
 *   - Work Timeline: when on Narrative layer, breadcrumb shows `Brief > Narrative`;
 *     clicking the Brief segment switches back to Brief (zoom-out to world
 *     shape). When on Brief layer, breadcrumb shows just `Brief`.
 *
 * The breadcrumb is the cross-layer navigation affordance on the canvas header
 * (sibling to the explicit layer switcher tabs). It reuses the same
 * `onLayerChange` callback — no new control surface.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, screen, waitFor } from '@testing-library/react';

import { renderInApp } from '@/test/test-providers';
import type { NexusClient } from '@/lib/nexus';
import type {
  WorldKbEntityProjection,
  WorldKbGraphResponse,
  WorkOutline,
} from '@42ch/nexus-contracts';

import { TimelineCanvas } from '../timeline-canvas/timeline-canvas';
import { WorkTimelineCanvas } from '../work-timeline-canvas/work-timeline-canvas';

// ─── Fixture builders ──────────────────────────────────────────────────────

function worldEntity(
  overrides: Partial<WorldKbEntityProjection> &
    Pick<WorldKbEntityProjection, 'key_block_id' | 'block_type' | 'canonical_name'>,
): WorldKbEntityProjection {
  return {
    world_id: 'world-bc',
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

function makeWorldMockClient(graph: WorldKbGraphResponse): NexusClient {
  return {
    getWorldKbGraph: vi.fn().mockResolvedValue(graph),
    worldKbPatchEntity: vi.fn(),
    worldKbPatchRelationship: vi.fn(),
    worldKbPromoteCandidate: vi.fn(),
    patchTimelineEvent: vi.fn(),
    patchOutlineStructure: vi.fn(),
    patchOutlineChapter: vi.fn(),
    getWorks: vi.fn().mockResolvedValue({ items: [], total: 0 }),
    health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
  } as unknown as NexusClient;
}

function makeWorkMockClient(outline: WorkOutline): NexusClient {
  return {
    getWorkOutline: vi.fn().mockResolvedValue(outline),
    getWork: vi.fn().mockResolvedValue({ work_id: 'work-1', world_id: null }),
    health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
  } as unknown as NexusClient;
}

// ─── World Timeline — Brief ↔ Narrative breadcrumb ───────────────────────

describe('TimelineCanvas — layer breadcrumb (P4 Task 5)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('renders just the Brief segment when the Brief layer is active (no parent)', async () => {
    const graph: WorldKbGraphResponse = {
      entities: [
        eraEntity({
          key_block_id: 'kb-era-1',
          canonical_name: 'The First Age',
        }),
      ],
      source_anchors: [],
      relationships: [],
    };
    renderInApp(<TimelineCanvas worldId="world-bc" />, {
      client: makeWorldMockClient(graph),
    });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'brief',
      );
    });

    const breadcrumb = screen.getByTestId('timeline-layer-breadcrumb');
    expect(breadcrumb).toBeInTheDocument();
    // Brief segment present.
    expect(breadcrumb).toHaveTextContent('Brief');
    // No "Narrative" segment when Brief is active — Brief has no parent.
    expect(breadcrumb).not.toHaveTextContent('Narrative');
  });

  it('renders Brief > Narrative path when Narrative is active; clicking Brief zooms out', async () => {
    const graph: WorldKbGraphResponse = {
      entities: [
        eraEntity({
          key_block_id: 'kb-era-1',
          canonical_name: 'The First Age',
        }),
        worldEntity({
          key_block_id: 'kb-event-1',
          block_type: 'event',
          canonical_name: 'Coronation',
          body: { attributes: { occurred_at: '1042-03-01T00:00:00Z' } },
        }),
      ],
      source_anchors: [],
      relationships: [],
    };
    renderInApp(<TimelineCanvas worldId="world-bc" />, {
      client: makeWorldMockClient(graph),
    });

    // Default layer is Brief (era data exists).
    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'brief',
      );
    });

    // Switch to Narrative via the layer tab (the primary affordance).
    fireEvent.click(screen.getByTestId('timeline-layer-tab-narrative'));
    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });

    // Breadcrumb now shows the path Brief > Narrative.
    const breadcrumb = screen.getByTestId('timeline-layer-breadcrumb');
    expect(breadcrumb).toHaveTextContent('Brief');
    expect(breadcrumb).toHaveTextContent('Narrative');

    // Click the Brief segment (the zoom-out target).
    fireEvent.click(screen.getByTestId('timeline-layer-breadcrumb-segment-brief'));
    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'brief',
      );
    });
  });
});

// ─── Work Timeline — Brief ↔ Narrative breadcrumb (V1.156 P2 T2) ─────────
//
// V1.156 closes the Work×Brief cell: Brief is the coarsest (world-shape)
// layer on the Work Timeline, so the breadcrumb pair becomes Brief ›
// Narrative — mirroring the World Timeline's breadcrumb exactly. Moment
// sits outside the coarse/fine pair (reached via the switcher tabs), the
// same as the World Timeline's Moment.

describe('WorkTimelineCanvas — layer breadcrumb (P4 Task 5 + V1.156 P2 T2)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('renders the Brief > Narrative path when Narrative is active; clicking Brief zooms out', async () => {
    const outline: WorkOutline = {
      work_id: 'work-1',
      outline_revision: 1,
      volumes: [],
      timeline_events: [
        { event_id: 'evt-1', title: 'Inciting Incident', realizes_chapter_id: 1 },
      ],
      foreshadows: [],
      chapter_titles: {},
      updated_at: '2026-07-18T00:00:00Z',
    } as WorkOutline;

    renderInApp(<WorkTimelineCanvas workId="work-1" />, {
      client: makeWorkMockClient(outline),
    });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });

    // Breadcrumb shows the path Brief > Narrative (Brief = the coarsest
    // layer — the world-shape zoom-out target). No "Moment" segment.
    const breadcrumb = screen.getByTestId('work-timeline-layer-breadcrumb');
    expect(breadcrumb).toBeInTheDocument();
    expect(breadcrumb).toHaveTextContent('Brief');
    expect(breadcrumb).toHaveTextContent('Narrative');
    expect(breadcrumb).not.toHaveTextContent('Moment');

    // Click the Brief segment (zoom-out target) → switches to Brief.
    fireEvent.click(
      screen.getByTestId('work-timeline-layer-breadcrumb-segment-brief'),
    );
    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'brief',
      );
    });
  });

  it('renders just the Brief segment when the Brief layer is active (no parent)', async () => {
    const outline: WorkOutline = {
      work_id: 'work-1',
      outline_revision: 1,
      volumes: [],
      timeline_events: [
        { event_id: 'evt-1', title: 'Inciting Incident', realizes_chapter_id: 1 },
      ],
      foreshadows: [],
      chapter_titles: {},
      updated_at: '2026-07-18T00:00:00Z',
    } as WorkOutline;

    renderInApp(<WorkTimelineCanvas workId="work-1" />, {
      client: makeWorkMockClient(outline),
    });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });

    // Switch to Brief via the layer tab (no bound World → Brief-empty
    // panel, but the header chrome with the breadcrumb still renders).
    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-brief'));
    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'brief',
      );
    });

    // Breadcrumb shows just the Brief segment — Brief has no parent layer.
    const breadcrumb = screen.getByTestId('work-timeline-layer-breadcrumb');
    expect(breadcrumb).toBeInTheDocument();
    expect(breadcrumb).toHaveTextContent('Brief');
    expect(breadcrumb).not.toHaveTextContent('Narrative');
    expect(breadcrumb).not.toHaveTextContent('Moment');
  });

  it('keeps the breadcrumb safe when Moment is active (Moment sits outside the Brief > Narrative pair)', async () => {
    // P1 mirror: the World Timeline breadcrumb pair is Brief › Narrative
    // with Moment outside it; the Work Timeline mirrors that exactly. The
    // breadcrumb must render without crashing on the Moment layer (the
    // switcher tabs remain the primary Moment affordance).
    const outline: WorkOutline = {
      work_id: 'work-1',
      outline_revision: 1,
      volumes: [],
      timeline_events: [
        { event_id: 'evt-1', title: 'Inciting Incident', realizes_chapter_id: 1 },
        { event_id: 'evt-2', title: 'Turning Point', realizes_chapter_id: 2 },
      ],
      foreshadows: [],
      chapter_titles: {},
      updated_at: '2026-07-18T00:00:00Z',
    } as WorkOutline;

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
      {
        client: makeWorkMockClient(outline),
      },
    );

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });

    // Switch to Moment via the layer tab.
    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-moment'));
    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'moment',
      );
    });

    // The breadcrumb stays mounted (Moment is outside its coarse/fine pair
    // — the pair renders the Narrative fine segment). No crash on Moment.
    expect(screen.getByTestId('work-timeline-layer-breadcrumb')).toBeInTheDocument();
  });
});
