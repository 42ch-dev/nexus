import { http, HttpResponse } from 'msw';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
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

// V1.120 P0 T2: a shared toggle lets the load-error test simulate the canvas
// returning its internal `ErrorState` while the page-level Back stays visible.
// `vi.hoisted` keeps the object reference stable inside the hoisted mock
// factory.
const canvasMock = vi.hoisted(() => ({ error: false }));

function StrategyRoutes() {
  return (
    <>
      <LocationDisplay />
      <Routes>
        <Route path="strategies" element={<div data-testid="strategies-list">list</div>} />
        <Route path="strategies/:presetId" element={<StrategyPage />} />
      </Routes>
    </>
  );
}

vi.mock('@/components/canvas/strategy-canvas', () => ({
  StrategyCanvas: ({ presetId }: { presetId: string }) =>
    canvasMock.error ? (
      <div data-testid="strategy-canvas-error" role="alert">
        Could not load this view
      </div>
    ) : (
      <div data-testid="strategy-canvas">{presetId}</div>
    ),
}));

describe('StrategyPage', () => {
  afterEach(() => {
    canvasMock.error = false;
  });

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
    // AC-P0-2: a Back control to /strategies is rendered on the detail header.
    expect(screen.getByRole('button', { name: /Back/i })).toBeInTheDocument();
  });

  it('shows a Back control on the not-found empty state that navigates to /strategies', async () => {
    useHandlers(
      http.get('/v1/daemon/presets', () => HttpResponse.json({
        user: [],
        system: [],
        embedded: [],
      })),
    );

    const user = userEvent.setup();
    renderInApp(<StrategyRoutes />, {
      client: makeClient(),
      initialRouterEntries: ['/strategies/missing'],
    });

    await waitFor(() =>
      expect(screen.getByText('Strategy not found')).toBeInTheDocument(),
    );
    const backButton = screen.getByRole('button', { name: /Back/i });
    expect(backButton).toBeInTheDocument();

    await user.click(backButton);

    await waitFor(() =>
      expect(screen.getByTestId('location')).toHaveTextContent('/strategies'),
    );
    expect(screen.getByTestId('strategies-list')).toBeInTheDocument();
  });

  it('keeps Back visible and navigable when the canvas fails to load', async () => {
    canvasMock.error = true;
    useHandlers(
      http.get('/v1/daemon/presets', () => HttpResponse.json({
        user: [{ id: 'user/foo', source: 'user' }],
        system: [],
        embedded: [],
      })),
    );

    const user = userEvent.setup();
    renderInApp(<StrategyRoutes />, {
      client: makeClient(),
      initialRouterEntries: ['/strategies/user%2Ffoo'],
    });

    // The canvas is in its error shell, but the page header Back still renders.
    await waitFor(() =>
      expect(screen.getByTestId('strategy-canvas-error')).toBeInTheDocument(),
    );
    const backButton = screen.getByRole('button', { name: /Back/i });
    expect(backButton).toBeInTheDocument();

    await user.click(backButton);

    await waitFor(() =>
      expect(screen.getByTestId('location')).toHaveTextContent('/strategies'),
    );
    expect(screen.getByTestId('strategies-list')).toBeInTheDocument();
  });
});
