import { describe, expect, it } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import { useLocation } from 'react-router-dom';

import { SetupGate } from './setup-gate';
import { renderInApp } from '@/test/test-providers';
import { BrowserClient } from '@/lib/nexus';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';

function makeClient(): BrowserClient {
  return new BrowserClient();
}

function makeDesktop(overrides: Partial<DesktopCapabilities> = {}): DesktopCapabilities {
  return {
    openWith: () => Promise.resolve(),
    openExternalUrl: () => Promise.resolve(),
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
    getAgentProfile: () => Promise.resolve(null),
    getWorkspaceRoot: () => Promise.resolve('/tmp/nexus'),
    pickDirectory: () => Promise.resolve(null),
    setWorkspacePath: () => Promise.resolve(),
    ensureSetupBootstrap: () =>
      Promise.resolve({ creator_id: 'ctr_local1234567890ab', already_bootstrapped: false }),
    switchActiveCreator: () => Promise.resolve('/tmp/nexus'),
    restartDaemon: () => Promise.resolve(),
    toggleMaximizeWindow: () => Promise.resolve(),
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
  it('renders children when setup is completed (routing only — no splash)', () => {
    renderGate({ setupCompleted: true, initialRouterEntries: ['/works'] });
    expect(screen.getByTestId('main-shell')).toBeInTheDocument();
    expect(screen.queryByText('Starting daemon…')).not.toBeInTheDocument();
  });

  it('redirects to /setup when setup has not been completed', async () => {
    renderGate({
      setupCompleted: false,
      initialRouterEntries: ['/works'],
    });

    await waitFor(() => expect(screen.getByTestId('location')).toHaveTextContent('/setup'));
    expect(screen.queryByTestId('main-shell')).not.toBeInTheDocument();
    expect(screen.queryByText('Starting daemon…')).not.toBeInTheDocument();
  });

  it('does not show daemon splash on desktop (outer gate owns wait)', () => {
    renderGate({
      desktop: makeDesktop(),
      setupCompleted: true,
      initialRouterEntries: ['/works'],
    });

    expect(screen.getByTestId('main-shell')).toBeInTheDocument();
    expect(screen.queryByText('Starting daemon…')).not.toBeInTheDocument();
    expect(screen.queryByText('Daemon not ready')).not.toBeInTheDocument();
  });
});
