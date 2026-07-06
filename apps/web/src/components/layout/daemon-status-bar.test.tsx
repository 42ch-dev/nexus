/**
 * DaemonStatusBar lifecycle-action tests.
 *
 * V1.94 simplification: the status bar renders only when the daemon is running,
 * showing a restart-icon button (no pill, no state text). Non-running states are
 * surfaced by the top-of-main-content {@link MainBanner}.
 */
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { DaemonStatusBar } from '@/components/layout/daemon-status-bar';
import { renderInApp } from '@/test/test-providers';
import type { DesktopCapabilities, DaemonStatus } from '@/lib/nexus/desktop-capabilities';

function makeDesktop(
  status: { state: string },
  impl: Partial<DesktopCapabilities> = {},
): DesktopCapabilities {
  const listeners = new Set<(status: DaemonStatus) => void>();
  const trigger = (next: DaemonStatus) => listeners.forEach((cb) => cb(next));

  return {
    openWith: vi.fn().mockResolvedValue(undefined),
    revealInFinder: vi.fn().mockResolvedValue(undefined),
    getSetupCompleted: vi.fn().mockResolvedValue(true),
    setSetupCompleted: vi.fn().mockResolvedValue(undefined),
    setAgentProfile: vi.fn().mockResolvedValue(undefined),
    getWorkspaceRoot: vi.fn().mockResolvedValue('~/Documents/nexus42/default'),
    getDaemonStatus: vi.fn().mockResolvedValue(status),
    onDaemonStatusChanged: vi.fn().mockImplementation((cb) => {
      listeners.add(cb);
      return Promise.resolve(() => listeners.delete(cb));
    }),
    startDaemon: vi.fn().mockResolvedValue(undefined),
    stopDaemon: vi.fn().mockResolvedValue(undefined),
    ...impl,
    // Expose a test-only trigger so event-driven updates can be simulated.
    _triggerStatusChange: trigger,
  } as DesktopCapabilities;
}

describe('DaemonStatusBar lifecycle action', () => {
  it('browser build renders nothing', () => {
    const { container } = renderInApp(<DaemonStatusBar />);
    expect(container.firstChild).toBeNull();
  });

  it('running daemon shows a restart-icon button and stops then starts when confirmed', async () => {
    const startDaemon = vi.fn().mockResolvedValue(undefined);
    const stopDaemon = vi.fn().mockResolvedValue(undefined);
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);

    renderInApp(<DaemonStatusBar />, {
      desktop: makeDesktop({ state: 'running' }, { startDaemon, stopDaemon }),
    });

    const button = await screen.findByRole('button', { name: /Restart daemon/i });
    expect(button).toBeInTheDocument();

    await userEvent.click(button);

    expect(confirmSpy).toHaveBeenCalledWith(
      'Restarting the daemon will interrupt any running orchestration. Continue?',
    );
    expect(stopDaemon).toHaveBeenCalled();
    expect(startDaemon).toHaveBeenCalled();

    confirmSpy.mockRestore();
  });

  it('does nothing when the restart confirmation is cancelled', async () => {
    const startDaemon = vi.fn().mockResolvedValue(undefined);
    const stopDaemon = vi.fn().mockResolvedValue(undefined);
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(false);

    renderInApp(<DaemonStatusBar />, {
      desktop: makeDesktop({ state: 'running' }, { startDaemon, stopDaemon }),
    });

    await userEvent.click(await screen.findByRole('button', { name: /Restart daemon/i }));

    expect(stopDaemon).not.toHaveBeenCalled();
    expect(startDaemon).not.toHaveBeenCalled();

    confirmSpy.mockRestore();
  });

  it('does not render for non-running daemon states', () => {
    const { container: degraded } = renderInApp(<DaemonStatusBar />, {
      desktop: makeDesktop({ state: 'degraded' }),
    });
    expect(degraded.firstChild).toBeNull();

    const { container: stopped } = renderInApp(<DaemonStatusBar />, {
      desktop: makeDesktop({ state: 'stopped' }),
    });
    expect(stopped.firstChild).toBeNull();

    const { container: error } = renderInApp(<DaemonStatusBar />, {
      desktop: makeDesktop({ state: 'error' }),
    });
    expect(error.firstChild).toBeNull();
  });

  it('updates status when the Rust side emits a status event', async () => {
    const desktop = makeDesktop({ state: 'running' });

    renderInApp(<DaemonStatusBar />, { desktop });
    await screen.findByRole('button', { name: /Restart daemon/i });

    // Simulate a transition to running; the restart button should remain.
    (
      desktop as unknown as {
        _triggerStatusChange: (status: DaemonStatus) => void;
      }
    )._triggerStatusChange({
      state: 'running',
      version: '1.0.0',
      port: 8420,
    });

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Restart daemon/i })).toBeInTheDocument();
    });
  });

  it('falls back to periodic health re-sync when no event is received', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const getDaemonStatus = vi.fn().mockResolvedValue({ state: 'running' });

    renderInApp(<DaemonStatusBar />, {
      desktop: makeDesktop({ state: 'running' }, { getDaemonStatus }),
    });

    // Initial fetch on mount.
    await waitFor(() => expect(getDaemonStatus).toHaveBeenCalledTimes(1));

    // Advance past the fallback interval.
    await vi.advanceTimersByTimeAsync(10_000);

    expect(getDaemonStatus).toHaveBeenCalledTimes(2);

    // Cleanup.
    vi.useRealTimers();
  });
});
