import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { SetupStepDaemon } from '@/pages/setup-step-daemon';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import type { DaemonStatus, DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';

function makeClient() {
  return new BrowserClient();
}

function makeDesktop(overrides: Partial<DesktopCapabilities> = {}): DesktopCapabilities {
  return {
    openWith: () => Promise.resolve(),
    revealInFinder: () => Promise.resolve(),
    getDaemonStatus: () => Promise.resolve({ state: 'running', port: 8420 }),
    onDaemonStatusChanged: () => Promise.resolve(() => {}),
    startDaemon: () => Promise.resolve(),
    stopDaemon: () => Promise.resolve(),
    resetLocalDatabase: () => Promise.resolve(),
    getSetupCompleted: () => Promise.resolve(false),
    setSetupCompleted: () => Promise.resolve(),
    setAgentProfile: () => Promise.resolve(),
    getWorkspaceRoot: () => Promise.resolve('/tmp/nexus'),
    pickDirectory: () => Promise.resolve(null),
    setWorkspacePath: () => Promise.resolve(),
    ...overrides,
  };
}

describe('SetupStepDaemon', () => {
  it('uses the HTTP health probe in browser mode and reaches the running state', async () => {
    useHandlers(
      http.get('/v1/daemon/runtime/health', () => HttpResponse.json({ status: 'ok', version: 'test' })),
    );

    renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      initialRouterEntries: ['/setup'],
    });

    await waitFor(() => expect(screen.getByText('Daemon is running.')).toBeInTheDocument());
  });

  it('does not probe on desktop and surfaces status.detail verbatim when the daemon errors', async () => {
    const onNext = vi.fn();
    const detail = 'Daemon did not start: port conflict on 8420';
    const onDaemonStatusChanged = vi.fn((callback: (status: DaemonStatus) => void) => {
      callback({ state: 'error', port: 8420, detail });
      return Promise.resolve(() => {});
    });

    renderInApp(<SetupStepDaemon onNext={onNext} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ onDaemonStatusChanged }),
      initialRouterEntries: ['/setup'],
    });

    await waitFor(() => expect(screen.getByText(detail)).toBeInTheDocument());
    expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Reset local database' })).toBeInTheDocument();
    expect(
      screen.getByText(/This will clear the daemon's local state database.*Your creative files.*not affected/),
    ).toBeInTheDocument();
  });

  it('calls resetLocalDatabase then startDaemon when the reset button is clicked', async () => {
    const user = userEvent.setup();
    const resetLocalDatabase = vi.fn(() => Promise.resolve());
    const startDaemon = vi.fn(() => Promise.resolve());
    const detail = 'Daemon did not start';
    const onDaemonStatusChanged = vi.fn((callback: (status: DaemonStatus) => void) => {
      callback({ state: 'error', port: 8420, detail });
      return Promise.resolve(() => {});
    });

    renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ onDaemonStatusChanged, resetLocalDatabase, startDaemon }),
      initialRouterEntries: ['/setup'],
    });

    await waitFor(() => expect(screen.getByText(detail)).toBeInTheDocument());
    await user.click(screen.getByRole('button', { name: 'Reset local database' }));

    await waitFor(() => expect(resetLocalDatabase).toHaveBeenCalled());
    await waitFor(() => expect(startDaemon).toHaveBeenCalled());
  });

  it('surfaces reset errors and does not call startDaemon when reset fails', async () => {
    const user = userEvent.setup();
    const resetLocalDatabase = vi.fn(() => Promise.reject(new Error('permission denied')));
    const startDaemon = vi.fn(() => Promise.resolve());
    const detail = 'Daemon did not start';
    const onDaemonStatusChanged = vi.fn((callback: (status: DaemonStatus) => void) => {
      callback({ state: 'error', port: 8420, detail });
      return Promise.resolve(() => {});
    });

    renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ onDaemonStatusChanged, resetLocalDatabase, startDaemon }),
      initialRouterEntries: ['/setup'],
    });

    await waitFor(() => expect(screen.getByText(detail)).toBeInTheDocument());
    await user.click(screen.getByRole('button', { name: 'Reset local database' }));

    await waitFor(() => expect(screen.getByText('permission denied')).toBeInTheDocument());
    expect(startDaemon).not.toHaveBeenCalled();
  });

  it('re-probes after reset when the subscription threw on mount', async () => {
    const user = userEvent.setup();
    const resetLocalDatabase = vi.fn(() => Promise.resolve());
    const startDaemon = vi.fn(() => Promise.resolve());
    let healthCalls = 0;

    useHandlers(
      http.get('/v1/daemon/runtime/health', () => {
        healthCalls += 1;
        // Succeed only on the second probe (after reset).
        if (healthCalls >= 2) {
          return HttpResponse.json({ status: 'ok', version: 'test' });
        }
        return HttpResponse.error();
      }),
    );

    const onDaemonStatusChanged = vi.fn(() => Promise.reject(new Error('subscription failed')));

    renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ onDaemonStatusChanged, resetLocalDatabase, startDaemon }),
      initialRouterEntries: ['/setup'],
    });

    // First mount: subscription throws, probe fails → error state.
    await waitFor(() => expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument());
    expect(healthCalls).toBeGreaterThanOrEqual(1);

    await user.click(screen.getByRole('button', { name: 'Reset local database' }));

    await waitFor(() => expect(screen.getByText('Daemon is running.')).toBeInTheDocument());
    expect(healthCalls).toBeGreaterThanOrEqual(2);
    await waitFor(() => expect(resetLocalDatabase).toHaveBeenCalled());
    await waitFor(() => expect(startDaemon).toHaveBeenCalled());
  });
});
