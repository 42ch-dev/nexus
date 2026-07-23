import { fireEvent, screen, waitFor } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';
import { Route, Routes } from 'react-router-dom';

import { Sidebar } from '@/components/layout/sidebar';
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
  withSidebar?: boolean;
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

  const hubRoutes = (
    <Routes>
      <Route path="/" element={<CreatorHubPage />} />
      <Route path="works" element={<CreatorHubPage />} />
      <Route path="works/:workId/outline" element={<div data-testid="work-outline-canvas" />} />
      <Route path="worlds" element={<CreatorHubPage />} />
      <Route path="worlds/:worldId/timeline" element={<div data-testid="world-timeline-canvas" />} />
    </Routes>
  );

  const routes = options?.withSidebar ? (
    <Routes>
      <Route
        path="/works"
        element={
          <div className="flex h-full min-h-0">
            <Sidebar />
            <CreatorHubPage />
          </div>
        }
      />
    </Routes>
  ) : (
    hubRoutes
  );

  renderInApp(routes, {
    client: makeClient(),
    activeCreatorId: 'creator-a',
    initialRouterEntries: [options?.initialEntry ?? '/works'],
  });
}

describe('CreatorHubPage browse IA (V1.135 P0)', () => {
  it('renders browse chrome with shared tab bar', async () => {
    renderCreatorHubFlow();

    expect(await screen.findByTestId('creator-hub-dual-pane')).toBeInTheDocument();
    expect(screen.getByTestId('creator-hub-dual-pane-tab-bar-world')).toBeInTheDocument();
    expect(screen.getByTestId('creator-hub-dual-pane-tab-bar-work')).toBeInTheDocument();
    expect(screen.getByTestId('creator-hub-dual-pane-tabpanel')).toBeInTheDocument();
    expect(screen.queryByTestId('creator-hub-controller')).not.toBeInTheDocument();
    expect(
      screen.queryByTestId('creator-hub-dual-pane-workspace-pane-inline-form'),
    ).not.toBeInTheDocument();
  });

  it('shows sidebar create panel when hub mounts beside Sidebar', async () => {
    renderCreatorHubFlow({ works: [], worlds: [], withSidebar: true });

    expect(await screen.findByTestId('sidebar-create-panel')).toBeInTheDocument();
    expect(screen.getByTestId('shell-sidebar-panel')).toBeInTheDocument();
    expect(
      screen.queryByTestId('creator-hub-dual-pane-workspace-pane-inline-form'),
    ).not.toBeInTheDocument();
  });

  it('shows Work cards when Work tab is active', async () => {
    renderCreatorHubFlow();

    await waitFor(() => {
      expect(screen.getByTestId('creator-hub-dual-pane-tab-bar-work')).toHaveAttribute(
        'aria-selected',
        'true',
      );
    });

    expect(
      await screen.findByTestId('creator-hub-dual-pane-card-list-pane-work-card-work-42'),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId('creator-hub-dual-pane-card-list-pane-world'),
    ).not.toBeInTheDocument();
  });

  it('selects Work tab after hydrate when only works exist (IA §1.2)', async () => {
    renderCreatorHubFlow({
      works: [
        {
          work_id: 'work-42',
          title: 'Drill Novel',
          status: 'active',
          intake_status: 'ready',
          primary_preset_id: 'preset-1',
          updated_at: '2026-01-01T00:00:00Z',
        },
      ],
      worlds: [],
    });

    await waitFor(() => {
      expect(screen.getByTestId('creator-hub-dual-pane-tab-bar-work')).toHaveAttribute(
        'aria-selected',
        'true',
      );
    });
    expect(
      screen.getByTestId('creator-hub-dual-pane-card-list-pane-work-card-work-42'),
    ).toBeInTheDocument();
  });

  it('does not show empty state while lists are loading', async () => {
    let resolveWorks: (() => void) | undefined;
    const worksGate = new Promise<void>((resolve) => {
      resolveWorks = resolve;
    });

    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.get('/v1/daemon/works', async () => {
        await worksGate;
        return HttpResponse.json({
          items: [],
          pagination: { limit: 20, has_more: false },
        });
      }),
      http.get('/v1/daemon/narrative/worlds', async () => {
        await worksGate;
        return HttpResponse.json({ worlds: [] });
      }),
      http.post('/v1/daemon/agent-host/scan', () => HttpResponse.json({ agents: [] })),
    );

    renderInApp(
      <Routes>
        <Route path="/works" element={<CreatorHubPage />} />
      </Routes>,
      {
        client: makeClient(),
        activeCreatorId: 'creator-a',
        initialRouterEntries: ['/works'],
      },
    );

    expect(await screen.findByTestId('creator-hub-dual-pane')).toBeInTheDocument();
    expect(screen.getByTestId('creator-hub-dual-pane-card-list-pane-loading')).toBeInTheDocument();
    expect(screen.queryByTestId('creator-hub-dual-pane-card-list-pane-empty')).not.toBeInTheDocument();
    expect(
      screen.queryByTestId('creator-hub-dual-pane-workspace-pane-inline-form'),
    ).not.toBeInTheDocument();

    resolveWorks?.();

    expect(
      await screen.findByTestId('creator-hub-dual-pane-card-list-pane-empty'),
    ).toBeInTheDocument();
  });

  it('keeps manual tab choice after lists hydrate', async () => {
    renderCreatorHubFlow({
      works: [
        {
          work_id: 'work-42',
          title: 'Drill Novel',
          status: 'active',
          intake_status: 'ready',
          primary_preset_id: 'preset-1',
          updated_at: '2026-01-01T00:00:00Z',
        },
      ],
      worlds: [],
    });

    fireEvent.click(await screen.findByTestId('creator-hub-dual-pane-tab-bar-world'));

    await waitFor(() => {
      expect(screen.getByTestId('creator-hub-dual-pane-tab-bar-world')).toHaveAttribute(
        'aria-selected',
        'true',
      );
    });
    expect(
      screen.getByTestId('creator-hub-dual-pane-card-list-pane-empty'),
    ).toBeInTheDocument();
  });

  it('shows empty-state copy pointing to sidebar create', async () => {
    renderCreatorHubFlow({ works: [], worlds: [] });

    expect(
      await screen.findByTestId('creator-hub-dual-pane-card-list-pane-empty'),
    ).toBeInTheDocument();
    expect(
      screen.getByText('No Worlds yet — create one from the sidebar'),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId('creator-hub-dual-pane-workspace-pane-inline-form'),
    ).not.toBeInTheDocument();
  });

  it('links tab switches across browse content only', async () => {
    renderCreatorHubFlow({
      works: [],
      worlds: [{ world_id: 'world-1', title: 'Fantasy Realm' }],
    });

    fireEvent.click(await screen.findByTestId('creator-hub-dual-pane-tab-bar-work'));

    await waitFor(() => {
      expect(
        screen.getByText('No Works yet — create one from the sidebar'),
      ).toBeInTheDocument();
    });
    expect(
      screen.queryByTestId('creator-hub-dual-pane-workspace-pane-inline-form'),
    ).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('creator-hub-dual-pane-tab-bar-world'));

    await waitFor(() => {
      expect(
        screen.getByTestId('creator-hub-dual-pane-card-list-pane-world-card-world-1'),
      ).toBeInTheDocument();
    });
  });

  it('navigates to canvas when a card is clicked (no controller stub)', async () => {
    renderCreatorHubFlow();

    await waitFor(() => {
      expect(screen.getByTestId('creator-hub-dual-pane-tab-bar-work')).toHaveAttribute(
        'aria-selected',
        'true',
      );
    });

    fireEvent.click(
      await screen.findByTestId('creator-hub-dual-pane-card-list-pane-work-card-work-42'),
    );

    await waitFor(() => {
      expect(screen.getByTestId('work-outline-canvas')).toBeInTheDocument();
    });
    expect(screen.queryByTestId('creator-hub-controller')).not.toBeInTheDocument();
  });
});
