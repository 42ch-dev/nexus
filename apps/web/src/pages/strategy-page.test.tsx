import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import { Route, Routes, useLocation } from 'react-router-dom';

import { StrategyPage } from '@/pages/strategy-page';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';

function makeClient() {
  return new BrowserClient();
}

function LocationDisplay() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}</div>;
}

function StrategyRoutes() {
  return (
    <>
      <LocationDisplay />
      <Routes>
        <Route path="strategies/:presetId" element={<StrategyPage />} />
      </Routes>
    </>
  );
}

vi.mock('@/components/canvas/strategy-canvas', () => ({
  StrategyCanvas: ({ presetId }: { presetId: string }) => (
    <div data-testid="strategy-canvas">{presetId}</div>
  ),
}));

describe('StrategyPage', () => {
  it('renders the strategy detail at /strategies/:presetId', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () => HttpResponse.json({
        user: [{ id: 'user/foo', source: 'user' }],
        system: [],
        embedded: [],
      })),
    );

    renderInApp(<StrategyRoutes />, {
      client: makeClient(),
      initialRouterEntries: ['/strategies/user%2Ffoo'],
    });

    await waitFor(() => expect(screen.getByRole('heading', { name: 'Strategy' })).toBeInTheDocument());
    expect(screen.getByTestId('strategy-canvas')).toHaveTextContent('user/foo');
    expect(screen.getByTestId('location')).toHaveTextContent('/strategies/user%2Ffoo');
  });

  it('shows an empty state when the preset id is unknown', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () => HttpResponse.json({
        user: [],
        system: [],
        embedded: [],
      })),
    );

    renderInApp(
      <Routes>
        <Route path="strategies/:presetId" element={<StrategyPage />} />
      </Routes>,
      { client: makeClient(), initialRouterEntries: ['/strategies/missing'] },
    );

    await waitFor(() =>
      expect(screen.getByText('Strategy not found')).toBeInTheDocument(),
    );
  });
});
