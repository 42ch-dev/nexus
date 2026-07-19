/**
 * Layer-state persistence — V1.123 P4 Task 6.
 *
 * Pins the layer-state persistence contract from
 * `iterations/v1.123/specs/layer-feel-differentiation.md` §5 + plan
 * Global Constraints (URL `?layer=brief|narrative|moment`):
 *
 *   | Requirement          | Spec                                            |
 *   |----------------------|-------------------------------------------------|
 *   | Survive surface switch | URL `?layer=` on the Timeline route preserves   |
 *   |                      | the layer across Timeline → peer → back.        |
 *   | Invalid layer        | `?layer=moment` on World Timeline → ignored;    |
 *   |                      | `?layer=brief` on Work Timeline → ignored.      |
 *   | Default when absent  | World: Brief-if-era-data-else-Narrative;        |
 *   |                      | Work: Narrative (architect §7.3 override).      |
 *
 * The test exercises both the read path (initial URL → active layer) and
 * the write path (layer swap → URL updated). It uses `MemoryRouter` with
 * explicit `initialRouterEntries` so the URL state is observable.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, screen, waitFor } from '@testing-library/react';
import { useLocation } from 'react-router-dom';

import { renderInApp } from '@/test/test-providers';
import type { NexusClient } from '@/lib/nexus';
import type {
  WorldKbEntityProjection,
  WorldKbGraphResponse,
  WorkOutline,
} from '@42ch/nexus-contracts';

import { TimelineCanvas } from '../timeline-canvas/timeline-canvas';
import { WorkTimelineCanvas } from '../work-timeline-canvas/work-timeline-canvas';

// ─── Location spy — captures MemoryRouter's current location.search ───────
//
// `MemoryRouter` does NOT update `window.location` — it manages its own
// internal history. To assert that a layer swap writes `?layer=` back to the
// URL, we mount a tiny spy component inside the router that records
// `useLocation().search` into a mutable ref.
function makeLocationSpy() {
  const captured: { search: string } = { search: '' };
  function LocationSpy() {
    const location = useLocation();
    captured.search = location.search;
    return null;
  }
  return { captured, LocationSpy };
}

// ─── Fixture builders ──────────────────────────────────────────────────────

function worldEntity(
  overrides: Partial<WorldKbEntityProjection> &
    Pick<WorldKbEntityProjection, 'key_block_id' | 'block_type' | 'canonical_name'>,
): WorldKbEntityProjection {
  return {
    world_id: 'world-ps',
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

// ─── Location spy usage pattern ───────────────────────────────────────────
//
// Tests that assert URL writes mount `<LocationSpy />` as a sibling of the
// canvas inside the same `MemoryRouter`. The spy records `location.search`
// into a mutable ref on every render so the test can assert after a layer
// swap that the URL was updated.

// ─── World Timeline — URL ?layer= drives + reflects active layer ──────────

describe('TimelineCanvas — layer-state persistence via URL ?layer= (P4 Task 6)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('restores the Narrative layer from URL ?layer=narrative even when era data exists', async () => {
    // Era data present → default would be Brief. URL ?layer=narrative must
    // override the default so the user returns to the layer they left.
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
    renderInApp(<TimelineCanvas worldId="world-ps" />, {
      client: makeWorldMockClient(graph),
      initialRouterEntries: ['/worlds/world-ps/timeline?layer=narrative'],
    });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });
  });

  it('ignores invalid layer values (Moment not valid on World Timeline) and falls back to default', async () => {
    // Moment is a Work-Timeline-only layer. URL ?layer=moment on the World
    // Timeline must be ignored — the surface falls back to the era-driven
    // default (Brief when era data exists).
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
    renderInApp(<TimelineCanvas worldId="world-ps" />, {
      client: makeWorldMockClient(graph),
      initialRouterEntries: ['/worlds/world-ps/timeline?layer=moment'],
    });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'brief',
      );
    });
  });

  it('writes the active layer back to the URL when the user swaps layers', async () => {
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
    const { captured, LocationSpy } = makeLocationSpy();
    renderInApp(
      <>
        <TimelineCanvas worldId="world-ps" />
        <LocationSpy />
      </>,
      {
        client: makeWorldMockClient(graph),
        initialRouterEntries: ['/worlds/world-ps/timeline'],
      },
    );

    // Default — Brief (era data exists, no URL param).
    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'brief',
      );
    });

    // User swaps to Narrative via the layer tab.
    fireEvent.click(screen.getByTestId('timeline-layer-tab-narrative'));
    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });

    // URL must now carry ?layer=narrative so the choice survives a refresh
    // or a peer-surface round-trip.
    expect(captured.search).toContain('layer=narrative');
  });

  it('clears the URL param when the user swaps back to the default layer', async () => {
    // When the user returns to the default layer (Brief, era data exists),
    // the URL should NOT carry ?layer=narrative — it would be redundant with
    // the default-derived layer and would prevent the surface from tracking
    // graph changes. The test asserts the URL drops the narrative param
    // (either to `?layer=brief` or to no param at all — both signal "not on
    // Narrative"). The contract surface is "URL no longer says narrative".
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
    const { captured, LocationSpy } = makeLocationSpy();
    renderInApp(
      <>
        <TimelineCanvas worldId="world-ps" />
        <LocationSpy />
      </>,
      {
        client: makeWorldMockClient(graph),
        initialRouterEntries: ['/worlds/world-ps/timeline?layer=narrative'],
      },
    );

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });

    // Swap back to Brief via the layer tab.
    fireEvent.click(screen.getByTestId('timeline-layer-tab-brief'));
    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'brief',
      );
    });

    // URL must NOT still say narrative — either `?layer=brief` or no param.
    expect(captured.search).not.toContain('layer=narrative');
  });
});

// ─── Work Timeline — URL ?layer= drives + reflects active layer ───────────

describe('WorkTimelineCanvas — layer-state persistence via URL ?layer= (P4 Task 6)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('restores the Moment layer from URL ?layer=moment', async () => {
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
        initialRouterEntries: ['/works/work-1/timeline?layer=moment'],
      },
    );

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'moment',
      );
    });
  });

  it('ignores invalid layer values (Brief not valid on Work Timeline) and falls back to Narrative default', async () => {
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
      initialRouterEntries: ['/works/work-1/timeline?layer=brief'],
    });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });
  });

  it('writes the active layer back to the URL when the user swaps to Moment', async () => {
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

    const { captured, LocationSpy } = makeLocationSpy();
    renderInApp(
      <>
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
        />
        <LocationSpy />
      </>,
      {
        client: makeWorkMockClient(outline),
        initialRouterEntries: ['/works/work-1/timeline'],
      },
    );

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });

    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-moment'));
    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'moment',
      );
    });

    expect(captured.search).toContain('layer=moment');
  });
});
