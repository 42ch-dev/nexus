import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor, act } from '@testing-library/react';
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
  it('renders Continue as a wide prominent CTA and Back as a smaller tertiary button', async () => {
    useHandlers(
      http.get('/v1/daemon/runtime/health', () => HttpResponse.json({ status: 'ok', version: 'test' })),
    );

    renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      initialRouterEntries: ['/setup'],
    });

    const continueButton = await waitFor(() => screen.getByRole('button', { name: 'Continue' }));
    expect(continueButton).toHaveClass('w-full', 'max-w-setup-wizard-surface-cta-primary-max-width');

    const backButton = screen.getByRole('button', { name: 'Back' });
    expect(backButton).toHaveClass('self-start');
  });

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
    const getDaemonStatus = vi.fn(() => Promise.resolve({ state: 'starting', port: 8420 } as DaemonStatus));
    const onDaemonStatusChanged = vi.fn((callback: (status: DaemonStatus) => void) => {
      callback({ state: 'error', port: 8420, detail });
      return Promise.resolve(() => {});
    });

    renderInApp(<SetupStepDaemon onNext={onNext} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ getDaemonStatus, onDaemonStatusChanged }),
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
    const getDaemonStatus = vi.fn(() => Promise.resolve({ state: 'starting', port: 8420 } as DaemonStatus));
    const onDaemonStatusChanged = vi.fn((callback: (status: DaemonStatus) => void) => {
      callback({ state: 'error', port: 8420, detail });
      return Promise.resolve(() => {});
    });

    renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ getDaemonStatus, onDaemonStatusChanged, resetLocalDatabase, startDaemon }),
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
    const getDaemonStatus = vi.fn(() => Promise.resolve({ state: 'starting', port: 8420 } as DaemonStatus));
    const onDaemonStatusChanged = vi.fn((callback: (status: DaemonStatus) => void) => {
      callback({ state: 'error', port: 8420, detail });
      return Promise.resolve(() => {});
    });

    renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ getDaemonStatus, onDaemonStatusChanged, resetLocalDatabase, startDaemon }),
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
    const getDaemonStatus = vi.fn(() =>
      Promise.resolve({ state: 'starting', port: 8420 } as DaemonStatus),
    );
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
      desktop: makeDesktop({ getDaemonStatus, onDaemonStatusChanged, resetLocalDatabase, startDaemon }),
      initialRouterEntries: ['/setup'],
    });

    // First mount: getDaemonStatus is starting, subscription throws, probe fails → error state.
    await waitFor(() => expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument());
    expect(healthCalls).toBeGreaterThanOrEqual(1);

    await user.click(screen.getByRole('button', { name: 'Reset local database' }));

    await waitFor(() => expect(screen.getByText('Daemon is running.')).toBeInTheDocument());
    expect(healthCalls).toBeGreaterThanOrEqual(2);
    await waitFor(() => expect(resetLocalDatabase).toHaveBeenCalled());
    await waitFor(() => expect(startDaemon).toHaveBeenCalled());
  });

  it('probes getDaemonStatus on mount and skips polling when status is running', async () => {
    const getDaemonStatus = vi.fn(() =>
      Promise.resolve({ state: 'running', port: 8420, version: 'test' } as DaemonStatus),
    );
    const onDaemonStatusChanged = vi.fn(() => Promise.resolve(() => {}));
    let healthCalls = 0;

    useHandlers(
      http.get('/v1/daemon/runtime/health', () => {
        healthCalls += 1;
        return HttpResponse.error();
      }),
    );

    renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ getDaemonStatus, onDaemonStatusChanged }),
      initialRouterEntries: ['/setup'],
    });

    await waitFor(() => expect(screen.getByText('Daemon is running.')).toBeInTheDocument());
    expect(getDaemonStatus).toHaveBeenCalled();
    expect(healthCalls).toBe(0);
  });

  it('remains in loading state while daemon is starting', async () => {
    const getDaemonStatus = vi.fn(() => Promise.resolve({ state: 'starting', port: 8420 } as DaemonStatus));
    const onDaemonStatusChanged = vi.fn(() => Promise.resolve(() => {}));

    renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ getDaemonStatus, onDaemonStatusChanged }),
      initialRouterEntries: ['/setup'],
    });

    await waitFor(() => expect(getDaemonStatus).toHaveBeenCalled());
    expect(screen.getByText('Starting daemon…')).toBeInTheDocument();
    expect(screen.queryByText('Daemon is running.')).not.toBeInTheDocument();
  });

  it('times out after 25 seconds if daemon never reaches running', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const getDaemonStatus = vi.fn(() => Promise.resolve({ state: 'starting', port: 8420 } as DaemonStatus));
    const onDaemonStatusChanged = vi.fn(() => Promise.resolve(() => {}));

    renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ getDaemonStatus, onDaemonStatusChanged }),
      initialRouterEntries: ['/setup'],
    });

    await waitFor(() => expect(getDaemonStatus).toHaveBeenCalled());
    await act(async () => {
      await vi.advanceTimersByTimeAsync(25_000);
    });
    await waitFor(() =>
      expect(screen.getByText(/Daemon is taking longer than expected to start/)).toBeInTheDocument(),
    );
    vi.useRealTimers();
  });

  it('surfaces getDaemonStatus errors as fallback to polling', async () => {
    useHandlers(
      http.get('/v1/daemon/runtime/health', () => HttpResponse.json({ status: 'ok', version: 'test' })),
    );
    const getDaemonStatus = vi.fn(() => Promise.reject(new Error('invoke failed')));
    const onDaemonStatusChanged = vi.fn(() => Promise.reject(new Error('subscription failed')));

    renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ getDaemonStatus, onDaemonStatusChanged }),
      initialRouterEntries: ['/setup'],
    });

    await waitFor(() => expect(screen.getByText('Daemon is running.')).toBeInTheDocument());
  });

  it('clears error and re-subscribes when retry is clicked', async () => {
    const user = userEvent.setup();
    const startDaemon = vi.fn(() => Promise.resolve());
    const getDaemonStatus = vi
      .fn()
      .mockResolvedValueOnce({ state: 'error', port: 8420, detail: 'port conflict' } as DaemonStatus)
      .mockResolvedValueOnce({ state: 'running', port: 8420, version: 'test' } as DaemonStatus);
    const onDaemonStatusChanged = vi.fn(() => Promise.resolve(() => {}));

    renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ getDaemonStatus, onDaemonStatusChanged, startDaemon }),
      initialRouterEntries: ['/setup'],
    });

    await waitFor(() => expect(screen.getByText('port conflict')).toBeInTheDocument());
    await user.click(screen.getByRole('button', { name: 'Retry' }));
    await waitFor(() => expect(startDaemon).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByText('Daemon is running.')).toBeInTheDocument());
  });

  it('clears timeout on unmount', async () => {
    vi.useFakeTimers();
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    const getDaemonStatus = vi.fn(() => Promise.resolve({ state: 'starting', port: 8420 } as DaemonStatus));
    const onDaemonStatusChanged = vi.fn(() => Promise.resolve(() => {}));

    const { unmount } = renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ getDaemonStatus, onDaemonStatusChanged }),
      initialRouterEntries: ['/setup'],
    });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(getDaemonStatus).toHaveBeenCalled();

    unmount();
    act(() => {
      vi.advanceTimersByTime(25_000);
    });

    expect(
      consoleSpy.mock.calls.some((call) => String(call[0]).includes('unmounted component')),
    ).toBe(false);
    consoleSpy.mockRestore();
    vi.useRealTimers();
  });

  it('renders error detail verbatim including stderr tail', async () => {
    const detail = 'Daemon did not start.\n\nDaemon output:\nmigration 202606070001 was previously applied';
    const getDaemonStatus = vi.fn(() => Promise.resolve({ state: 'starting', port: 8420 } as DaemonStatus));
    const onDaemonStatusChanged = vi.fn((callback: (status: DaemonStatus) => void) => {
      callback({ state: 'error', port: 8420, detail });
      return Promise.resolve(() => {});
    });

    renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ getDaemonStatus, onDaemonStatusChanged }),
      initialRouterEntries: ['/setup'],
    });

    await waitFor(() => expect(screen.getByText(/Daemon did not start/)).toBeInTheDocument());
    expect(screen.getByText(/migration 202606070001 was previously applied/)).toBeInTheDocument();
  });
});
