import { afterEach, describe, expect, it, vi } from 'vitest';
import { screen, within } from '@testing-library/react';

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
});