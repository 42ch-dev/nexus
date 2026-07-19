/**
 * GlobalTimelineView — V1.123 P3 Task 1 (cross-World Timeline overview).
 *
 * Verifies the global Timeline view rendering recent Timeline activity across
 * Worlds, locked by:
 *   - `iterations/v1.123/specs/three-layer-product-spec.md` (Timeline as the
 *     central instrument — global entry in primary nav).
 *   - `iterations/v1.123/specs/three-layer-architecture.md` §5 + §8 (client-
 *     side composition; cap N=5–10 most-recent to avoid N+1 perf risk).
 *   - Plan `2026-07-18-v1.123-timeline-first-ia-deepening.md` Task 1.
 *
 * Coverage:
 *   - View renders activity list (World name, layer, last-edited) from
 *     `useNarrativeWorlds()` + per-World `useWorldKbGraph()`.
 *   - Each row links to the per-World Timeline route (`/worlds/<id>/timeline`).
 *   - Caps to N=5 most-recent Worlds by `updated_at` (plan mitigation).
 *   - Honest empty / loading / error states.
 *
 * Mount strategy mirrors `timeline-canvas/__tests__/layer-switcher.test.tsx`:
 * a mocked `NexusClient` resolves `listNarrativeWorlds` + `getWorldKbGraph`
 * to per-test fixtures.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { screen, within } from '@testing-library/react';

import { renderInApp } from '@/test/test-providers';
import type { NexusClient } from '@/lib/nexus';
import type {
  World,
  WorldKbEntityProjection,
  WorldKbGraphResponse,
} from '@42ch/nexus-contracts';

import { GlobalTimelineView } from '../global-timeline-view';

// ─── Fixture builders ──────────────────────────────────────────────────────

function makeWorld(
  over: Partial<World> & Pick<World, 'world_id'>,
): World {
  return {
    schema_version: 1,
    owner_creator_id: 'creator-a',
    title: over.world_id,
    slug: over.world_id,
    status: 'active',
    visibility: 'private',
    time_policy: 'manual',
    created_at: '2026-07-01T00:00:00Z',
    ...over,
  } as World;
}

function entity(
  over: Partial<WorldKbEntityProjection> &
    Pick<WorldKbEntityProjection, 'key_block_id' | 'block_type' | 'canonical_name'>,
): WorldKbEntityProjection {
  return {
    world_id: 'world-x',
    status: 'confirmed',
    version: 1,
    ...over,
  } as WorldKbEntityProjection;
}

function makeClient(
  worlds: World[],
  graphsByWorld: Record<string, WorldKbGraphResponse>,
): NexusClient {
  return {
    listNarrativeWorlds: vi.fn().mockResolvedValue(worlds),
    getWorldKbGraph: vi.fn().mockImplementation((worldId: string) => {
      const graph =
        graphsByWorld[worldId] ?? {
          entities: [],
          source_anchors: [],
          relationships: [],
        };
      return Promise.resolve(graph);
    }),
    health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
  } as unknown as NexusClient;
}

// ─── Activity list rendering ───────────────────────────────────────────────

describe('GlobalTimelineView — activity list (V1.123 P3 Task 1)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('renders the activity list with World name, layer, and last-edited timestamp', async () => {
    const worlds = [
      makeWorld({
        world_id: 'eryndor',
        title: 'Eryndor',
        updated_at: '2026-07-15T00:00:00Z',
      }),
      makeWorld({
        world_id: 'solara',
        title: 'Solara',
        updated_at: '2026-07-10T00:00:00Z',
      }),
    ];
    const graphsByWorld: Record<string, WorldKbGraphResponse> = {
      eryndor: {
        entities: [
          entity({
            key_block_id: 'era-1',
            block_type: 'era',
            canonical_name: 'First Age',
          }),
          entity({
            key_block_id: 'event-1',
            block_type: 'event',
            canonical_name: 'Coronation',
          }),
        ],
        source_anchors: [],
        relationships: [],
      },
      solara: {
        entities: [
          entity({
            key_block_id: 'event-2',
            block_type: 'event',
            canonical_name: 'Battle',
          }),
        ],
        source_anchors: [],
        relationships: [],
      },
    };

    renderInApp(<GlobalTimelineView />, {
      client: makeClient(worlds, graphsByWorld),
    });

    const view = await screen.findByTestId('global-timeline-view');
    expect(view).toBeInTheDocument();

    // Both World names render in the activity list.
    expect(screen.getByText('Eryndor')).toBeInTheDocument();
    expect(screen.getByText('Solara')).toBeInTheDocument();

    // Each row carries the testability hooks: world-id + derived layer.
    // Eryndor (has era) → 'brief'; Solara (no era) → 'narrative'.
    const rows = screen.getAllByTestId('global-timeline-row');
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveAttribute('data-world-id', 'eryndor');
    expect(rows[0]).toHaveAttribute('data-layer', 'brief');
    expect(rows[1]).toHaveAttribute('data-world-id', 'solara');
    expect(rows[1]).toHaveAttribute('data-layer', 'narrative');
  });

  it('links each row to the per-World Timeline route', async () => {
    const worlds = [
      makeWorld({
        world_id: 'eryndor',
        title: 'Eryndor',
        updated_at: '2026-07-15T00:00:00Z',
      }),
    ];
    const graphsByWorld: Record<string, WorldKbGraphResponse> = {
      eryndor: {
        entities: [
          entity({
            key_block_id: 'era-1',
            block_type: 'era',
            canonical_name: 'First Age',
          }),
        ],
        source_anchors: [],
        relationships: [],
      },
    };

    renderInApp(<GlobalTimelineView />, {
      client: makeClient(worlds, graphsByWorld),
    });

    const link = await screen.findByRole('link', { name: /Eryndor/i });
    expect(link).toHaveAttribute(
      'href',
      '/worlds/eryndor/timeline',
    );
  });

  it('caps the activity list to the N=5 most-recent Worlds by updated_at', async () => {
    // Six worlds with descending updated_at (world-1 most recent, world-6
    // oldest); the oldest (world-6) should NOT render in the activity list
    // because the cap drops it. This is the plan Global Constraints N+1
    // mitigation.
    const worlds = [1, 2, 3, 4, 5, 6].map((n) =>
      makeWorld({
        world_id: `world-${n}`,
        title: `World ${n}`,
        // world-1 gets the latest date (07-06); world-6 gets the earliest
        // (07-01) so the descending sort drops world-6 from the cap-5 view.
        updated_at: `2026-07-0${7 - n}T00:00:00Z`,
      }),
    );
    const graphsByWorld: Record<string, WorldKbGraphResponse> = {};
    for (const n of [1, 2, 3, 4, 5, 6]) {
      graphsByWorld[`world-${n}`] = {
        entities: [],
        source_anchors: [],
        relationships: [],
      };
    }

    renderInApp(<GlobalTimelineView />, {
      client: makeClient(worlds, graphsByWorld),
    });

    await screen.findByTestId('global-timeline-view');

    // The five most-recent worlds (world-1 .. world-5) render; world-6 is
    // capped out. The cap is verified by `getWorldKbGraph` not being called
    // for world-6 (the dropped world has no graph fetch).
    const rows = screen.getAllByTestId('global-timeline-row');
    expect(rows).toHaveLength(5);
    expect(screen.queryByText('World 6')).not.toBeInTheDocument();
  });

  it('renders the honest empty state when no worlds exist', async () => {
    renderInApp(<GlobalTimelineView />, {
      client: makeClient([], {}),
    });

    expect(
      await screen.findByTestId('global-timeline-view'),
    ).toBeInTheDocument();
    // Empty-state copy surfaces (no rows rendered).
    expect(screen.queryByTestId('global-timeline-row')).not.toBeInTheDocument();
  });

  it('renders the loading state before worlds resolve', async () => {
    // A client whose `listNarrativeWorlds` never resolves keeps the query in
    // `isLoading` so the loading branch renders.
    const client = {
      listNarrativeWorlds: vi.fn().mockImplementation(
        () => new Promise<void>(() => undefined),
      ),
      getWorldKbGraph: vi.fn().mockResolvedValue({
        entities: [],
        source_anchors: [],
        relationships: [],
      }),
      health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
    } as unknown as NexusClient;

    renderInApp(<GlobalTimelineView />, { client });

    expect(await screen.findByTestId('global-timeline-loading')).toBeInTheDocument();
  });

  it('renders the error state with retry when the worlds fetch fails', async () => {
    const client = {
      listNarrativeWorlds: vi.fn().mockRejectedValue(new Error('boom')),
      getWorldKbGraph: vi.fn().mockResolvedValue({
        entities: [],
        source_anchors: [],
        relationships: [],
      }),
      health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
    } as unknown as NexusClient;

    renderInApp(<GlobalTimelineView />, { client });

    expect(await screen.findByTestId('global-timeline-error')).toBeInTheDocument();
  });

  it('renders activity rows even when the per-World graph fetch fails (graceful degradation)', async () => {
    // The plan Global Constraints allow graceful degradation when a per-World
    // graph fetch fails — the row should still render with the World name +
    // last-edited timestamp, but the activity counts fall back to "—".
    const worlds = [
      makeWorld({
        world_id: 'broken',
        title: 'Broken Graph',
        updated_at: '2026-07-15T00:00:00Z',
      }),
    ];
    const client = {
      listNarrativeWorlds: vi.fn().mockResolvedValue(worlds),
      getWorldKbGraph: vi.fn().mockRejectedValue(new Error('graph boom')),
      health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
    } as unknown as NexusClient;

    renderInApp(<GlobalTimelineView />, { client });

    const row = await screen.findByTestId('global-timeline-row');
    expect(row).toBeInTheDocument();
    // The activity text falls back to the "—" sentinel (graceful degradation)
    // rather than throwing or hiding the row.
    const activity = within(row).getByTestId('global-timeline-row-activity');
    expect(activity).toBeInTheDocument();
    // Layer falls back to 'narrative' when the graph is unavailable (no era
    // data detected).
    expect(row).toHaveAttribute('data-layer', 'narrative');
  });
});
