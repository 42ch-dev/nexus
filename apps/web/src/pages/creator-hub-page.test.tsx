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

function renderCreatorHubFlow() {
  useHandlers(
    http.get('/v1/daemon/creators', () =>
      HttpResponse.json({
        items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
        pagination: { limit: 20, has_more: false },
      }),
    ),
    worksList([
      {
        work_id: 'work-42',
        title: 'Drill Novel',
        status: 'active',
        intake_status: 'ready',
        primary_preset_id: 'preset-1',
        updated_at: '2026-01-01T00:00:00Z',
      },
    ]),
    http.get('/v1/daemon/narrative/worlds', () => HttpResponse.json({ worlds: [] })),
    http.post('/v1/daemon/agent-host/scan', () => HttpResponse.json({ agents: [] })),
  );

  renderInApp(
    <>
      <Sidebar />
      <Routes>
        <Route path="works" element={<CreatorHubPage />} />
        <Route path="worlds" element={<CreatorHubPage />} />
      </Routes>
    </>,
    { client: makeClient(), activeCreatorId: 'creator-a', initialRouterEntries: ['/works'] },
  );
}

describe('CreatorHubPage + selection context (V1.132 P3 AC-8)', () => {
  it('shows Worlds/Works lists on the right when no entity is selected', async () => {
    renderCreatorHubFlow();

    expect(await screen.findByTestId('creator-hub-entity-lists')).toBeInTheDocument();
    expect(
      await screen.findByTestId('creator-hub-entity-lists-works-row-work-42'),
    ).toBeInTheDocument();
    expect(screen.queryByTestId('creator-hub-create')).not.toBeInTheDocument();
  });

  it('shows Create CTAs in the left sidebar, not in hub content', async () => {
    renderCreatorHubFlow();

    expect(await screen.findByTestId('sidebar-create-panel')).toBeInTheDocument();
    expect(screen.getByTestId('creator-create-work')).toBeInTheDocument();
    expect(screen.getByTestId('creator-create-world')).toBeEnabled();
  });

  it('enables Create World when the client exposes createWorld', async () => {
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
      <>
        <Sidebar />
        <Routes>
          <Route path="works" element={<CreatorHubPage />} />
        </Routes>
      </>,
      {
        client: clientWithCreateWorld,
        activeCreatorId: 'creator-a',
        initialRouterEntries: ['/works'],
      },
    );

    const card = await screen.findByTestId('creator-create-world');
    expect(card).toBeEnabled();
  });

  it('selecting a Work row shows Controller stub; Back returns to entity lists', async () => {
    renderCreatorHubFlow();

    const workRow = await screen.findByTestId('creator-hub-entity-lists-works-row-work-42');
    fireEvent.click(workRow);

    await waitFor(() => {
      expect(screen.getByTestId('creator-hub-controller')).toBeInTheDocument();
    });
    expect(screen.getByText('Controller Panel — coming soon')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Delete/i })).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('creator-controller-back'));

    await waitFor(() => {
      expect(screen.getByTestId('creator-hub-entity-lists')).toBeInTheDocument();
    });
  });
});
