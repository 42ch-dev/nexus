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
    ensureSetupBootstrap: () =>
      Promise.resolve({ creator_id: 'ctr_local1234567890ab', already_bootstrapped: false }),
    ...overrides,
  };
}

describe('SetupStepDaemon', () => {
  it('renders Continue as a wide prominent CTA and Back adjacent in a horizontal row', async () => {
    useHandlers(
      http.get('/v1/daemon/runtime/health', () => HttpResponse.json({ status: 'ok', version: 'test' })),
    );

    renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      initialRouterEntries: ['/setup'],
    });

    const continueButton = await waitFor(() => screen.getByRole('button', { name: 'Continue' }));
    expect(continueButton).toHaveClass('w-full', 'max-w-setup-wizard-surface-cta-primary-max-width');

    const cta = screen.getByTestId('wizard-cta-row');
    expect(cta).toHaveAttribute('data-layout', 'horizontal-adjacent');
    expect(cta).toHaveClass('flex', 'items-center');
    expect(cta).not.toHaveClass('flex-col');

    const buttons = cta.querySelectorAll('button');
    expect(buttons[0]).toHaveTextContent('Back');
    expect(buttons[1]).toHaveTextContent('Continue');
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
    expect(onDaemonStatusChanged).not.toHaveBeenCalled();
    expect(healthCalls).toBe(0);
  });

  it('subscribes once while starting and does not open a second listener', async () => {
    const getDaemonStatus = vi.fn(() => Promise.resolve({ state: 'starting', port: 8420 } as DaemonStatus));
    const onDaemonStatusChanged = vi.fn(() => Promise.resolve(() => {}));

    renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ getDaemonStatus, onDaemonStatusChanged }),
      initialRouterEntries: ['/setup'],
    });

    await waitFor(() => expect(onDaemonStatusChanged).toHaveBeenCalledTimes(1));
    expect(screen.getByText('Starting daemon…')).toBeInTheDocument();
    expect(getDaemonStatus).toHaveBeenCalledTimes(1);
  });

  it('re-probes after auto-start and shows running without waiting for an event', async () => {
    const startDaemon = vi.fn(() => Promise.resolve());
    const getDaemonStatus = vi
      .fn()
      .mockResolvedValueOnce({ state: 'stopped', port: 8420 } as DaemonStatus)
      .mockResolvedValueOnce({ state: 'running', port: 8420, version: 'test' } as DaemonStatus);
    const onDaemonStatusChanged = vi.fn(() => Promise.resolve(() => {}));

    renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ startDaemon, getDaemonStatus, onDaemonStatusChanged }),
      initialRouterEntries: ['/setup'],
    });

    await waitFor(() => expect(startDaemon).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByText('Daemon is running.')).toBeInTheDocument());
    expect(getDaemonStatus).toHaveBeenCalledTimes(2);
    expect(onDaemonStatusChanged).not.toHaveBeenCalled();
  });

  it('re-probes getDaemonStatus on remount (Back re-entry) without relying on a stale subscription', async () => {
    const getDaemonStatus = vi.fn(() =>
      Promise.resolve({ state: 'running', port: 8420, version: 'test' } as DaemonStatus),
    );
    const onDaemonStatusChanged = vi.fn(() => Promise.resolve(() => {}));

    const { unmount } = renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ getDaemonStatus, onDaemonStatusChanged }),
      initialRouterEntries: ['/setup'],
    });

    await waitFor(() => expect(screen.getByText('Daemon is running.')).toBeInTheDocument());
    expect(getDaemonStatus).toHaveBeenCalledTimes(1);
    unmount();

    renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ getDaemonStatus, onDaemonStatusChanged }),
      initialRouterEntries: ['/setup'],
    });

    await waitFor(() => expect(screen.getByText('Daemon is running.')).toBeInTheDocument());
    expect(getDaemonStatus).toHaveBeenCalledTimes(2);
    expect(onDaemonStatusChanged).not.toHaveBeenCalled();
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
    expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument();
    expect(screen.queryByText('Starting daemon…')).not.toBeInTheDocument();
    vi.useRealTimers();
  });

  it('surfaces Retry when the 25s re-probe returns stopped (no permanent spinner)', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const getDaemonStatus = vi
      .fn()
      .mockResolvedValueOnce({ state: 'starting', port: 8420 } as DaemonStatus)
      .mockResolvedValueOnce({ state: 'stopped', port: 8420 } as DaemonStatus);
    const onDaemonStatusChanged = vi.fn(() => Promise.resolve(() => {}));

    renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ getDaemonStatus, onDaemonStatusChanged }),
      initialRouterEntries: ['/setup'],
    });

    await waitFor(() => expect(getDaemonStatus).toHaveBeenCalled());
    expect(screen.getByText('Starting daemon…')).toBeInTheDocument();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(25_000);
    });

    await waitFor(() =>
      expect(screen.getByText(/Daemon is taking longer than expected to start/)).toBeInTheDocument(),
    );
    expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument();
    expect(screen.queryByText('Starting daemon…')).not.toBeInTheDocument();
    vi.useRealTimers();
  });

  it('unsubscribes when unmount races ahead of onDaemonStatusChanged resolving', async () => {
    let resolveListen!: (unlisten: () => void) => void;
    const unlisten = vi.fn();
    const getDaemonStatus = vi.fn(() => Promise.resolve({ state: 'starting', port: 8420 } as DaemonStatus));
    const onDaemonStatusChanged = vi.fn(
      () =>
        new Promise<() => void>((resolve) => {
          resolveListen = resolve;
        }),
    );

    const { unmount } = renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ getDaemonStatus, onDaemonStatusChanged }),
      initialRouterEntries: ['/setup'],
    });

    await waitFor(() => expect(onDaemonStatusChanged).toHaveBeenCalledTimes(1));
    unmount();

    await act(async () => {
      resolveListen(unlisten);
    });

    expect(unlisten).toHaveBeenCalledTimes(1);
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

  it('recovers via auto-start when mount status is error instead of surfacing the detail', async () => {
    const startDaemon = vi.fn(() => Promise.resolve());
    const getDaemonStatus = vi
      .fn()
      .mockResolvedValueOnce({ state: 'error', port: 8420, detail: 'port conflict' } as DaemonStatus)
      .mockResolvedValueOnce({ state: 'starting', port: 8420 } as DaemonStatus);
    let emitStatus: (status: DaemonStatus) => void;
    const onDaemonStatusChanged = vi.fn((callback: (status: DaemonStatus) => void) => {
      emitStatus = callback;
      return Promise.resolve(() => {});
    });

    renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ startDaemon, getDaemonStatus, onDaemonStatusChanged }),
      initialRouterEntries: ['/setup'],
    });

    await waitFor(() => expect(startDaemon).toHaveBeenCalled());
    // Error detail should NOT be surfaced — auto-start handled the recovery.
    expect(screen.queryByText('port conflict')).not.toBeInTheDocument();
    await waitFor(() => expect(screen.getByText('Starting daemon…')).toBeInTheDocument());
    expect(onDaemonStatusChanged).toHaveBeenCalledTimes(1);

    // Daemon recovers after auto-start.
    await waitFor(() => {
      emitStatus!({ state: 'running', port: 8420, version: 'test' });
    });
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

  it('auto-starts the daemon on clean-state when status is stopped', async () => {
    const startDaemon = vi.fn(() => Promise.resolve());
    const getDaemonStatus = vi
      .fn()
      .mockResolvedValueOnce({ state: 'stopped', port: 8420 } as DaemonStatus)
      .mockResolvedValueOnce({ state: 'starting', port: 8420 } as DaemonStatus);
    let emitStatus: (status: DaemonStatus) => void;
    const onDaemonStatusChanged = vi.fn((callback: (status: DaemonStatus) => void) => {
      emitStatus = callback;
      return Promise.resolve(() => {});
    });

    renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ startDaemon, getDaemonStatus, onDaemonStatusChanged }),
      initialRouterEntries: ['/setup'],
    });

    await waitFor(() => expect(startDaemon).toHaveBeenCalled());
    // Should show loading state, not the stopped error.
    expect(screen.getByText('Starting daemon…')).toBeInTheDocument();

    // Simulate daemon reaching running after auto-start.
    await waitFor(() => {
      emitStatus!({ state: 'running', port: 8420, version: 'test' });
    });
    await waitFor(() => expect(screen.getByText('Daemon is running.')).toBeInTheDocument());
  });

  it('auto-starts the daemon on clean-state when status is error', async () => {
    const startDaemon = vi.fn(() => Promise.resolve());
    const getDaemonStatus = vi
      .fn()
      .mockResolvedValueOnce({
        state: 'error',
        port: 8420,
        detail: 'Daemon did not start: port conflict',
      } as DaemonStatus)
      .mockResolvedValueOnce({ state: 'starting', port: 8420 } as DaemonStatus);
    let emitStatus: (status: DaemonStatus) => void;
    const onDaemonStatusChanged = vi.fn((callback: (status: DaemonStatus) => void) => {
      emitStatus = callback;
      return Promise.resolve(() => {});
    });

    renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ startDaemon, getDaemonStatus, onDaemonStatusChanged }),
      initialRouterEntries: ['/setup'],
    });

    await waitFor(() => expect(startDaemon).toHaveBeenCalled());
    // Should show loading state, not the error detail.
    expect(screen.getByText('Starting daemon…')).toBeInTheDocument();
    expect(screen.queryByText('port conflict')).not.toBeInTheDocument();

    // Simulate daemon reaching running after auto-start.
    await waitFor(() => {
      emitStatus!({ state: 'running', port: 8420, version: 'test' });
    });
    await waitFor(() => expect(screen.getByText('Daemon is running.')).toBeInTheDocument());
  });

  it('surfaces auto-start failure immediately when startDaemon throws', async () => {
    const startDaemon = vi.fn(() => Promise.reject(new Error('sidecar launch failed')));
    const getDaemonStatus = vi
      .fn()
      .mockResolvedValueOnce({ state: 'stopped', port: 8420 } as DaemonStatus);
    const onDaemonStatusChanged = vi.fn(() => Promise.resolve(() => {}));

    renderInApp(<SetupStepDaemon onNext={() => {}} onBack={() => {}} />, {
      client: makeClient(),
      desktop: makeDesktop({ startDaemon, getDaemonStatus, onDaemonStatusChanged }),
      initialRouterEntries: ['/setup'],
    });

    await waitFor(() => expect(startDaemon).toHaveBeenCalled());
    await waitFor(() =>
      expect(
        screen.getByText(/Could not start the local service.*sidecar launch failed/),
      ).toBeInTheDocument(),
    );
    expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument();
    expect(screen.queryByText('Starting daemon…')).not.toBeInTheDocument();
  });
});
