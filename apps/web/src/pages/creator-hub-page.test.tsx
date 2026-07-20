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
