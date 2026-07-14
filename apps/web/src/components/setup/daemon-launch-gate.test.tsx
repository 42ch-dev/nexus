import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi, afterEach } from 'vitest';
import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { DaemonLaunchGate } from './daemon-launch-gate';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import type { DaemonStatus, DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';

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
    ...overrides,
  };
}

function healthUnavailable() {
  useHandlers(
    http.get('/v1/daemon/runtime/health', () =>
      HttpResponse.json({ error: { code: 'unavailable', message: 'nope' } }, { status: 503 }),
    ),
  );
}

function renderGate(options: Parameters<typeof renderInApp>[1] = {}) {
  return renderInApp(
    <DaemonLaunchGate>
      <div data-testid="routes">Routes</div>
    </DaemonLaunchGate>,
    { client: makeClient(), ...options },
  );
}

describe('DaemonLaunchGate', () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('browser build passes instantly without splash', () => {
    renderGate();
    expect(screen.getByTestId('routes')).toBeInTheDocument();
    expect(screen.queryByText('Starting daemon…')).not.toBeInTheDocument();
  });

  it('desktop waits on splash until status is running (return visit / first launch)', async () => {
    healthUnavailable();
    let emit: ((status: DaemonStatus) => void) | undefined;
    const startDaemon = vi.fn(() => Promise.resolve());
    const onDaemonStatusChanged = vi.fn((callback: (status: DaemonStatus) => void) => {
      emit = callback;
      return Promise.resolve(() => {});
    });
    const getDaemonStatus = vi.fn(() =>
      Promise.resolve({ state: 'starting', port: 8420 } as DaemonStatus),
    );

    renderGate({
      desktop: makeDesktop({
        startDaemon,
        getDaemonStatus,
        onDaemonStatusChanged,
      }),
    });

    expect(screen.getByText('Starting daemon…')).toBeInTheDocument();
    expect(screen.queryByTestId('routes')).not.toBeInTheDocument();
    expect(startDaemon).not.toHaveBeenCalled();

    await waitFor(() => expect(onDaemonStatusChanged).toHaveBeenCalled());
    await waitFor(() => expect(emit).toBeDefined());

    act(() => {
      emit?.({ state: 'running', port: 8420, version: 'test' });
    });
    await waitFor(() => expect(screen.getByTestId('routes')).toBeInTheDocument());
    expect(startDaemon).not.toHaveBeenCalled();
  });

  it('desktop passes gate when initial status is degraded', async () => {
    healthUnavailable();
    const startDaemon = vi.fn(() => Promise.resolve());
    const getDaemonStatus = vi.fn(() =>
      Promise.resolve({ state: 'degraded', port: 8420 } as DaemonStatus),
    );

    renderGate({
      desktop: makeDesktop({
        startDaemon,
        getDaemonStatus,
        onDaemonStatusChanged: () => Promise.resolve(() => {}),
      }),
    });

    await waitFor(() => expect(screen.getByTestId('routes')).toBeInTheDocument());
    expect(startDaemon).not.toHaveBeenCalled();
  });

  it('desktop becomes ready via health when status subscription is unavailable', async () => {
    useHandlers(
      http.get('/v1/daemon/runtime/health', () =>
        HttpResponse.json({ status: 'ok', version: 'test' }),
      ),
    );

    const startDaemon = vi.fn(() => Promise.resolve());
    renderGate({
      desktop: makeDesktop({
        startDaemon,
        getDaemonStatus: () => Promise.reject(new Error('ipc unavailable')),
        onDaemonStatusChanged: () => Promise.reject(new Error('ipc unavailable')),
      }),
    });

    expect(screen.getByText('Starting daemon…')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByTestId('routes')).toBeInTheDocument());
    expect(startDaemon).not.toHaveBeenCalled();
  });

  it('surfaces timeout error with retry and reset affordances', async () => {
    healthUnavailable();
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const startDaemon = vi.fn(() => Promise.resolve());
    const getDaemonStatus = vi.fn(() =>
      Promise.resolve({ state: 'starting', port: 8420 } as DaemonStatus),
    );

    renderGate({
      desktop: makeDesktop({
        startDaemon,
        getDaemonStatus,
        onDaemonStatusChanged: () => Promise.resolve(() => {}),
      }),
    });

    await waitFor(() => expect(getDaemonStatus).toHaveBeenCalled());
    expect(screen.getByText('Starting daemon…')).toBeInTheDocument();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(25_000);
    });

    await waitFor(() => expect(screen.getByText('Daemon not ready')).toBeInTheDocument());
    expect(screen.getByText(/taking longer than expected/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Restart Nexus/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Reset local database/i })).toBeInTheDocument();
    expect(startDaemon).not.toHaveBeenCalled();
  });

  it('reloads the window when retry is clicked', async () => {
    healthUnavailable();
    const user = userEvent.setup();
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const getDaemonStatus = vi.fn(() =>
      Promise.resolve({ state: 'starting', port: 8420 } as DaemonStatus),
    );
    const reloadSpy = vi.fn();
    Object.defineProperty(window, 'location', {
      value: { ...window.location, reload: reloadSpy },
      writable: true,
    });

    renderGate({
      desktop: makeDesktop({
        getDaemonStatus,
        onDaemonStatusChanged: () => Promise.resolve(() => {}),
      }),
    });

    await waitFor(() => expect(screen.getByText('Starting daemon…')).toBeInTheDocument());

    await act(async () => {
      await vi.advanceTimersByTimeAsync(25_000);
    });

    await waitFor(() =>
      expect(screen.getByRole('button', { name: /Restart Nexus/i })).toBeInTheDocument(),
    );

    await user.click(screen.getByRole('button', { name: /Restart Nexus/i }));
    expect(reloadSpy).toHaveBeenCalled();
  });

  it('reset local database does not call startDaemon (reload owns D2 restart)', async () => {
    healthUnavailable();
    const user = userEvent.setup();
    const startDaemon = vi.fn(() => Promise.resolve());
    const resetLocalDatabase = vi.fn(() => Promise.resolve());
    const reloadSpy = vi.fn();
    Object.defineProperty(window, 'location', {
      value: { ...window.location, reload: reloadSpy },
      writable: true,
    });

    renderGate({
      desktop: makeDesktop({
        startDaemon,
        resetLocalDatabase,
        getDaemonStatus: () =>
          Promise.resolve({
            state: 'error',
            port: 8420,
            detail: 'sidecar crashed',
          }),
        onDaemonStatusChanged: () => Promise.resolve(() => {}),
      }),
    });

    await waitFor(() => expect(screen.getByText('Daemon not ready')).toBeInTheDocument());
    expect(screen.getByText('sidecar crashed')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /Reset local database/i }));
    await waitFor(() => expect(resetLocalDatabase).toHaveBeenCalled());
    expect(startDaemon).not.toHaveBeenCalled();
    expect(reloadSpy).toHaveBeenCalled();
  });

  it('keeps reset-failure error without re-subscribing (no retryToken bump)', async () => {
    healthUnavailable();
    const user = userEvent.setup();
    const startDaemon = vi.fn(() => Promise.resolve());
    const resetLocalDatabase = vi.fn(() => Promise.reject(new Error('reset denied')));
    const onDaemonStatusChanged = vi.fn(() => Promise.resolve(() => {}));
    const getDaemonStatus = vi.fn(() =>
      Promise.resolve({
        state: 'error',
        port: 8420,
        detail: 'sidecar crashed',
      } as DaemonStatus),
    );

    renderGate({
      desktop: makeDesktop({
        startDaemon,
        resetLocalDatabase,
        getDaemonStatus,
        onDaemonStatusChanged,
      }),
    });

    await waitFor(() => expect(screen.getByText('Daemon not ready')).toBeInTheDocument());
    expect(screen.getByText('sidecar crashed')).toBeInTheDocument();
    const subscribeCallsBefore = onDaemonStatusChanged.mock.calls.length;

    await user.click(screen.getByRole('button', { name: /Reset local database/i }));
    await waitFor(() => expect(resetLocalDatabase).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByText('reset denied')).toBeInTheDocument());

    // Failure must not re-run the wait effect (which would clear the error via applyStatus).
    expect(onDaemonStatusChanged.mock.calls.length).toBe(subscribeCallsBefore);
    expect(startDaemon).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: /Restart Nexus/i })).toBeInTheDocument();
  });
});
