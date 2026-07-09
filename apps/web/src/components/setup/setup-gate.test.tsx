import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import { useLocation } from 'react-router-dom';

import { SetupGate } from './setup-gate';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';

function makeClient(): BrowserClient {
  return new BrowserClient();
}

function makeDesktop(overrides: Partial<DesktopCapabilities> = {}): DesktopCapabilities {
  return {
    openWith: () => Promise.resolve(),
    revealInFinder: () => Promise.resolve(),
    getDaemonStatus: () =>
      Promise.resolve({ state: 'running', port: 8420, version: 'test' }),
    onDaemonStatusChanged: () => Promise.resolve(() => {}),
    startDaemon: () => Promise.resolve(),
    stopDaemon: () => Promise.resolve(),
    resetLocalDatabase: () => Promise.resolve(),
    getSetupCompleted: () => Promise.resolve(true),
    setSetupCompleted: () => Promise.resolve(),
    setAgentProfile: () => Promise.resolve(),
    getWorkspaceRoot: () => Promise.resolve('/tmp/nexus'),
    pickDirectory: () => Promise.resolve(null),
    setWorkspacePath: () => Promise.resolve(),
    ensureSetupBootstrap: () =>
      Promise.resolve({ creator_id: 'ctr_local1234567890ab', already_bootstrapped: false }),
    ...overrides,
  };
}

function LocationDisplay() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}</div>;
}

function renderGate(options: Parameters<typeof renderInApp>[1] = {}) {
  return renderInApp(
    <>
      <LocationDisplay />
      <SetupGate>
        <div data-testid="main-shell">Control Room</div>
      </SetupGate>
    </>,
    { client: makeClient(), ...options },
  );
}

describe('SetupGate', () => {
  it('browser build renders the main shell immediately', () => {
    renderGate();
    expect(screen.getByTestId('main-shell')).toBeInTheDocument();
  });

  it('browser build does not flash the daemon splash', () => {
    renderGate();
    expect(screen.queryByText('Starting daemon…')).not.toBeInTheDocument();
    expect(screen.getByTestId('main-shell')).toBeInTheDocument();
  });

  it('redirects to /setup when setup has not been completed', async () => {
    renderGate({
      setupCompleted: false,
      initialRouterEntries: ['/works'],
    });

    await waitFor(() => expect(screen.getByTestId('location')).toHaveTextContent('/setup'));
    expect(screen.queryByTestId('main-shell')).not.toBeInTheDocument();
  });

  it('shows the daemon-ready splash until health succeeds on desktop', async () => {
    useHandlers(
      http.get('/v1/daemon/runtime/health', () =>
        HttpResponse.json({ status: 'ok', version: 'test' }),
      ),
    );

    renderGate({
      desktop: makeDesktop(),
      setupCompleted: true,
      initialRouterEntries: ['/works'],
    });

    expect(screen.getByText('Starting daemon…')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByTestId('main-shell')).toBeInTheDocument());
  });

  it('shows an error splash when the daemon health check fails', async () => {
    useHandlers(
      http.get('/v1/daemon/runtime/health', () =>
        HttpResponse.json({ error: { code: 'unavailable', message: 'nope' } }, { status: 503 }),
      ),
    );

    const reloadSpy = vi.fn();
    Object.defineProperty(window, 'location', {
      value: { ...window.location, reload: reloadSpy },
      writable: true,
    });

    renderGate({
      desktop: makeDesktop(),
      setupCompleted: true,
      initialRouterEntries: ['/works'],
    });

    await waitFor(() => expect(screen.getByText('Daemon not ready')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: /Restart Nexus/i })).toBeInTheDocument();
  });
});
