import { fireEvent, screen, waitFor } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';
import { Route, Routes } from 'react-router-dom';

import { CreatorHubPage } from '@/pages/creator-hub-page';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { worksList } from '@/test/handlers';
import { BrowserClient } from '@/lib/nexus';

vi.mock('@/components/brand/nexus-logo', () => ({
  NexusLogo: () => <div data-testid="nexus-logo">Nexus</div>,
}));

function makeClient() {
  return new BrowserClient();
}

function renderCreatorHubFlow(options?: {
  works?: Parameters<typeof worksList>[0];
  worlds?: { world_id: string; title: string }[];
  initialEntry?: string;
}) {
  const works = options?.works ?? [
    {
      work_id: 'work-42',
      title: 'Drill Novel',
      status: 'active',
      intake_status: 'ready',
      primary_preset_id: 'preset-1',
      updated_at: '2026-01-01T00:00:00Z',
    },
  ];
  const worlds = options?.worlds ?? [];

  useHandlers(
    http.get('/v1/daemon/creators', () =>
      HttpResponse.json({
        items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
        pagination: { limit: 20, has_more: false },
      }),
    ),
    worksList(works),
    http.get('/v1/daemon/narrative/worlds', () => HttpResponse.json({ worlds })),
    http.post('/v1/daemon/agent-host/scan', () => HttpResponse.json({ agents: [] })),
  );

  renderInApp(
  <Routes>
    <Route path="/" element={<CreatorHubPage />} />
    <Route path="works" element={<CreatorHubPage />} />
    <Route path="works/:workId/outline" element={<div data-testid="work-outline-canvas" />} />
    <Route path="worlds" element={<CreatorHubPage />} />
    <Route path="worlds/:worldId/timeline" element={<div data-testid="world-timeline-canvas" />} />
  </Routes>,
    {
      client: makeClient(),
      activeCreatorId: 'creator-a',
      initialRouterEntries: [options?.initialEntry ?? '/works'],
    },
  );
}

describe('CreatorHubPage dual-pane IA (V1.134 P3)', () => {
  it('renders stable dual-pane chrome with shared tab bar', async () => {
    renderCreatorHubFlow();

    expect(await screen.findByTestId('creator-hub-dual-pane')).toBeInTheDocument();
    expect(screen.getByTestId('creator-hub-dual-pane-tab-bar-world')).toBeInTheDocument();
    expect(screen.getByTestId('creator-hub-dual-pane-tab-bar-work')).toBeInTheDocument();
    expect(screen.getByTestId('creator-hub-dual-pane-tabpanel')).toBeInTheDocument();
    expect(screen.queryByTestId('creator-hub-controller')).not.toBeInTheDocument();
  });

  it('shows Work cards on the right when Work tab is active', async () => {
    renderCreatorHubFlow();

    fireEvent.click(await screen.findByTestId('creator-hub-dual-pane-tab-bar-work'));

    await waitFor(() => {
      expect(
        screen.getByTestId('creator-hub-dual-pane-card-list-pane-work-card-work-42'),
      ).toBeInTheDocument();
    });
    expect(
      screen.queryByTestId('creator-hub-dual-pane-card-list-pane-world'),
    ).not.toBeInTheDocument();
  });

  it('shows empty-state copy when the active tab has no items', async () => {
    renderCreatorHubFlow({ works: [], worlds: [] });

    expect(
      await screen.findByTestId('creator-hub-dual-pane-card-list-pane-empty'),
    ).toBeInTheDocument();
    expect(
      screen.getByText('No Worlds yet — create one from the left'),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId('creator-hub-dual-pane-workspace-pane-inline-form'),
    ).toBeInTheDocument();
  });

  it('links tab switches across both panes', async () => {
    renderCreatorHubFlow({
      works: [],
      worlds: [{ world_id: 'world-1', title: 'Fantasy Realm' }],
    });

    fireEvent.click(await screen.findByTestId('creator-hub-dual-pane-tab-bar-work'));

    await waitFor(() => {
      expect(
        screen.getByText('No Works yet — create one from the left'),
      ).toBeInTheDocument();
    });
    expect(
      screen.getByTestId('creator-hub-dual-pane-workspace-pane-inline-form'),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('creator-hub-dual-pane-tab-bar-world'));

    await waitFor(() => {
      expect(
        screen.getByTestId('creator-hub-dual-pane-card-list-pane-world-card-world-1'),
      ).toBeInTheDocument();
    });
  });

  it('navigates to canvas when a card is clicked (no controller stub)', async () => {
    renderCreatorHubFlow();

    fireEvent.click(await screen.findByTestId('creator-hub-dual-pane-tab-bar-work'));
    fireEvent.click(
      await screen.findByTestId('creator-hub-dual-pane-card-list-pane-work-card-work-42'),
    );

    await waitFor(() => {
      expect(screen.getByTestId('work-outline-canvas')).toBeInTheDocument();
    });
    expect(screen.queryByTestId('creator-hub-controller')).not.toBeInTheDocument();
  });

  it('opens create dialog from inline workspace affordance (Task 4 wires full inline submit)', async () => {
    const createWorld = vi.fn().mockResolvedValue({ world_id: 'w-new', status: 'active' });
    const clientWithCreateWorld = Object.assign(new BrowserClient(), { createWorld });

    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      worksList([]),
      http.get('/v1/daemon/narrative/worlds', () => HttpResponse.json({ worlds: [] })),
      http.post('/v1/daemon/agent-host/scan', () => HttpResponse.json({ agents: [] })),
    );

    renderInApp(
      <Routes>
        <Route path="/works" element={<CreatorHubPage />} />
      </Routes>,
      {
        client: clientWithCreateWorld,
        activeCreatorId: 'creator-a',
        initialRouterEntries: ['/works'],
      },
    );

    const titleInput = await screen.findByTestId('creator-hub-dual-pane-workspace-pane-title-input');
    fireEvent.change(titleInput, { target: { value: 'New Realm' } });
    fireEvent.click(screen.getByTestId('creator-hub-dual-pane-workspace-pane-submit'));

    await waitFor(() => {
      expect(screen.getByRole('dialog')).toBeInTheDocument();
    });
  });
});
