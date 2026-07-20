import { afterEach, describe, expect, it, vi } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { renderInApp } from '@/test/test-providers';
import type { NexusClient } from '@/lib/nexus';
import type { TimelineOverviewResponse } from '@42ch/nexus-contracts';

import { GlobalTimelineView } from '../global-timeline-view';

function makeOverview(
  worlds: Array<{
    world_id: string;
    title: string | null;
    era_count: number;
    event_count: number;
    last_event_at: string | null;
  }>,
  cursor?: string | null,
  total_worlds?: number,
): TimelineOverviewResponse {
  return {
    worlds,
    cursor: cursor ?? null,
    total_worlds: total_worlds ?? worlds.length,
  };
}

function makeClient(overview: TimelineOverviewResponse): NexusClient {
  return {
    getTimelineOverview: vi.fn().mockResolvedValue(overview),
    health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
  } as unknown as NexusClient;
}

describe('GlobalTimelineView — V1.126 P2 composite endpoint', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('renders the activity list with World name, layer, and last-edited timestamp', async () => {
    const overview = makeOverview([
      {
        world_id: 'eryndor',
        title: 'Eryndor',
        era_count: 2,
        event_count: 1,
        last_event_at: '2026-07-15T00:00:00Z',
      },
      {
        world_id: 'solara',
        title: 'Solara',
        era_count: 0,
        event_count: 1,
        last_event_at: '2026-07-10T00:00:00Z',
      },
    ]);

    renderInApp(<GlobalTimelineView />, {
      client: makeClient(overview),
    });

    const view = await screen.findByTestId('global-timeline-view');
    expect(view).toBeInTheDocument();

    expect(screen.getByText('Eryndor')).toBeInTheDocument();
    expect(screen.getByText('Solara')).toBeInTheDocument();

    const rows = screen.getAllByTestId('global-timeline-row');
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveAttribute('data-world-id', 'eryndor');
    expect(rows[0]).toHaveAttribute('data-layer', 'brief');
    expect(rows[1]).toHaveAttribute('data-world-id', 'solara');
    expect(rows[1]).toHaveAttribute('data-layer', 'narrative');
  });

  it('links each row to the per-World Timeline route', async () => {
    const overview = makeOverview([
      {
        world_id: 'eryndor',
        title: 'Eryndor',
        era_count: 1,
        event_count: 0,
        last_event_at: '2026-07-15T00:00:00Z',
      },
    ]);

    renderInApp(<GlobalTimelineView />, {
      client: makeClient(overview),
    });

    const link = await screen.findByRole('link', { name: /Eryndor/i });
    expect(link).toHaveAttribute('href', '/worlds/eryndor/timeline');
  });

  it('renders all worlds from the overview (no client-side cap)', async () => {
    const worlds = [1, 2, 3, 4, 5, 6].map((n) => ({
      world_id: `world-${n}`,
      title: `World ${n}`,
      era_count: 0,
      event_count: 0,
      last_event_at: null,
    }));
    const overview = makeOverview(worlds, null, 6);

    renderInApp(<GlobalTimelineView />, {
      client: makeClient(overview),
    });

    await screen.findByTestId('global-timeline-view');
    const rows = screen.getAllByTestId('global-timeline-row');
    expect(rows).toHaveLength(6);
    expect(screen.getByText('World 6')).toBeInTheDocument();
  });

  it('renders the honest empty state when no worlds exist', async () => {
    const overview = makeOverview([]);

    renderInApp(<GlobalTimelineView />, {
      client: makeClient(overview),
    });

    expect(
      await screen.findByTestId('global-timeline-view'),
    ).toBeInTheDocument();
    expect(screen.queryByTestId('global-timeline-row')).not.toBeInTheDocument();
  });

  it('renders the loading state before overview resolves', async () => {
    const client = {
      getTimelineOverview: vi.fn().mockImplementation(
        () => new Promise<void>(() => undefined),
      ),
      health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
    } as unknown as NexusClient;

    renderInApp(<GlobalTimelineView />, { client });

    expect(await screen.findByTestId('global-timeline-loading')).toBeInTheDocument();
  });

  it('renders the error state with retry when the overview fetch fails', async () => {
    const client = {
      getTimelineOverview: vi.fn().mockRejectedValue(new Error('boom')),
      health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
    } as unknown as NexusClient;

    renderInApp(<GlobalTimelineView />, { client });

    expect(await screen.findByTestId('global-timeline-error')).toBeInTheDocument();
  });

  it('renders activity rows with zero counts when overview has no kb data', async () => {
    const overview = makeOverview([
      {
        world_id: 'empty',
        title: 'Empty World',
        era_count: 0,
        event_count: 0,
        last_event_at: null,
      },
    ]);

    renderInApp(<GlobalTimelineView />, {
      client: makeClient(overview),
    });

    const row = await screen.findByTestId('global-timeline-row');
    expect(row).toBeInTheDocument();
    expect(row).toHaveAttribute('data-layer', 'narrative');
    const activity = within(row).getByTestId('global-timeline-row-activity');
    expect(activity).toBeInTheDocument();
  });

  // V1.127 P0 T2 — cursor pagination (AC-V1127-2). The composite overview
  // response carries the NEXT cursor directly (no separate has_more flag); a
  // non-null cursor means another page exists.
  describe('Load More pagination (V1.127 P0 T2)', () => {
    it('renders Load More when the overview returns a non-null cursor, fetches the next page with the cursor, and hides it when the cursor goes null', async () => {
      const user = userEvent.setup();
      const firstPage: TimelineOverviewResponse = {
        worlds: [
          {
            world_id: 'w-1',
            title: 'World One',
            era_count: 0,
            event_count: 0,
            last_event_at: null,
          },
        ],
        cursor: 'next-cursor',
        total_worlds: 2,
      };
      const secondPage: TimelineOverviewResponse = {
        worlds: [
          {
            world_id: 'w-2',
            title: 'World Two',
            era_count: 0,
            event_count: 0,
            last_event_at: null,
          },
        ],
        cursor: null,
        total_worlds: 2,
      };
      const getTimelineOverview = vi
        .fn()
        .mockResolvedValueOnce(firstPage)
        .mockResolvedValueOnce(secondPage);
      const client = {
        getTimelineOverview,
        health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
      } as unknown as NexusClient;

      renderInApp(<GlobalTimelineView />, { client });

      // First page renders; Load More visible because cursor is non-null.
      expect(await screen.findByText('World One')).toBeInTheDocument();
      const loadMore = await screen.findByTestId('global-timeline-load-more');
      expect(loadMore).not.toBeDisabled();

      await user.click(loadMore);

      // Second page fetched with the first page's cursor; both worlds visible.
      expect(await screen.findByText('World Two')).toBeInTheDocument();
      expect(screen.getByText('World One')).toBeInTheDocument();
      expect(getTimelineOverview).toHaveBeenNthCalledWith(2, 'next-cursor');

      // Second page cursor is null → no more → Load More removed.
      await waitFor(() => {
        expect(screen.queryByTestId('global-timeline-load-more')).not.toBeInTheDocument();
      });
    });

    it('hides Load More when the overview cursor is null (no next page)', async () => {
      const overview: TimelineOverviewResponse = {
        worlds: [
          {
            world_id: 'w-1',
            title: 'World One',
            era_count: 0,
            event_count: 0,
            last_event_at: null,
          },
        ],
        cursor: null,
        total_worlds: 1,
      };

      renderInApp(<GlobalTimelineView />, { client: makeClient(overview) });

      await screen.findByTestId('global-timeline-view');
      expect(screen.queryByTestId('global-timeline-load-more')).not.toBeInTheDocument();
    });
  });
});