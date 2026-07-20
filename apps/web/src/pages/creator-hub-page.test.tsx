import { fireEvent, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';
import { Route, Routes, useLocation } from 'react-router-dom';

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

function LocationDisplay() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}</div>;
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
    http.get('/v1/daemon/narrative/worlds', () => HttpResponse.json([])),
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

describe('CreatorHubPage + selection context (V1.128 P2 T2)', () => {
  it('shows Create page CTAs when no entity is selected', async () => {
    renderCreatorHubFlow();

    expect(await screen.findByTestId('creator-hub-create')).toBeInTheDocument();
    expect(screen.getByTestId('creator-create-work')).toBeInTheDocument();
    expect(screen.getByTestId('creator-create-world')).toBeDisabled();
  });

  it('navigates to /worlds/<id>/timeline when createWorld client is present', async () => {
    const user = userEvent.setup();
    const createWorld = vi.fn().mockResolvedValue({ world_id: 'w-new' });
    const clientWithCreateWorld = Object.assign(new BrowserClient(), { createWorld });

    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      worksList([]),
      http.get('/v1/daemon/narrative/worlds', () => HttpResponse.json([])),
    );

    renderInApp(
      <>
        <LocationDisplay />
        <Routes>
          <Route path="works" element={<CreatorHubPage />} />
          <Route path="worlds/:worldId/timeline" element={<div data-testid="timeline-outlet" />} />
        </Routes>
      </>,
      {
        client: clientWithCreateWorld,
        activeCreatorId: 'creator-a',
        initialRouterEntries: ['/works'],
      },
    );

    const card = await screen.findByTestId('creator-create-world');
    expect(card).not.toBeDisabled();
    await user.click(card);

    await waitFor(() => {
      expect(createWorld).toHaveBeenCalledTimes(1);
      expect(screen.getByTestId('location')).toHaveTextContent('/worlds/w-new/timeline');
    });
  });

  it('selecting a Work row shows Controller stub; Back returns to Create page', async () => {
    renderCreatorHubFlow();

    const workLink = await screen.findByRole('link', { name: 'Drill Novel' });
    fireEvent.click(workLink);

    await waitFor(() => {
      expect(screen.getByTestId('creator-hub-controller')).toBeInTheDocument();
    });
    expect(screen.getByText('Controller Panel — coming soon')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Delete/i })).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('creator-controller-back'));

    await waitFor(() => {
      expect(screen.getByTestId('creator-hub-create')).toBeInTheDocument();
    });
  });
});
