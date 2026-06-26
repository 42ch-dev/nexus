/**
 * DaemonStatusBar lifecycle-action tests.
 *
 * Pins the behavior of the manual daemon action:
 *   - Browser build renders nothing.
 *   - A running/degraded daemon shows "Restart Daemon" and confirms before
 *     stopping then starting (a real restart; start-only is a no-op when running).
 *   - A stopped/error daemon shows "Start Daemon" and calls startDaemon only.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, screen, waitFor } from '@testing-library/react';
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

  it('running daemon shows Restart Daemon and stops then starts when confirmed', async () => {
    const startDaemon = vi.fn().mockResolvedValue(undefined);
    const stopDaemon = vi.fn().mockResolvedValue(undefined);
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);

    renderInApp(<DaemonStatusBar />, {
      desktop: makeDesktop({ state: 'running' }, { startDaemon, stopDaemon }),
    });

    const button = await screen.findByRole('button', { name: /Restart Daemon/i });
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

    await userEvent.click(await screen.findByRole('button', { name: /Restart Daemon/i }));

    expect(stopDaemon).not.toHaveBeenCalled();
    expect(startDaemon).not.toHaveBeenCalled();

    confirmSpy.mockRestore();
  });

  it('degraded daemon restarts with stop then start', async () => {
    const startDaemon = vi.fn().mockResolvedValue(undefined);
    const stopDaemon = vi.fn().mockResolvedValue(undefined);
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);

    renderInApp(<DaemonStatusBar />, {
      desktop: makeDesktop({ state: 'degraded' }, { startDaemon, stopDaemon }),
    });

    await userEvent.click(await screen.findByRole('button', { name: /Restart Daemon/i }));

    expect(stopDaemon).toHaveBeenCalled();
    expect(startDaemon).toHaveBeenCalled();

    confirmSpy.mockRestore();
  });

  it('stopped daemon shows Start Daemon and calls startDaemon only', async () => {
    const startDaemon = vi.fn().mockResolvedValue(undefined);
    const stopDaemon = vi.fn().mockResolvedValue(undefined);

    renderInApp(<DaemonStatusBar />, {
      desktop: makeDesktop({ state: 'stopped' }, { startDaemon, stopDaemon }),
    });

    const button = screen.getByRole('button', { name: /Start Daemon/i });
    expect(button).toBeInTheDocument();

    await userEvent.click(button);

    expect(stopDaemon).not.toHaveBeenCalled();
    expect(startDaemon).toHaveBeenCalled();
  });

  it('error daemon shows Start Daemon and calls startDaemon only', async () => {
    const startDaemon = vi.fn().mockResolvedValue(undefined);
    const stopDaemon = vi.fn().mockResolvedValue(undefined);

    renderInApp(<DaemonStatusBar />, {
      desktop: makeDesktop({ state: 'error' }, { startDaemon, stopDaemon }),
    });

    await userEvent.click(screen.getByRole('button', { name: /Start Daemon/i }));

    expect(stopDaemon).not.toHaveBeenCalled();
    expect(startDaemon).toHaveBeenCalled();
  });

  it('updates status when the Rust side emits a status event', async () => {
    const desktop = makeDesktop({ state: 'starting' });

    renderInApp(<DaemonStatusBar />, { desktop });
    await screen.findByRole('button', { name: /Start Daemon/i });

    await act(async () => {
      (
        desktop as unknown as {
          _triggerStatusChange: (status: DaemonStatus) => void;
        }
      )._triggerStatusChange({
        state: 'running',
        version: '1.0.0',
        port: 8420,
      });
    });

    await waitFor(() => {
      expect(screen.getByText(/Daemon running/i)).toBeInTheDocument();
    });
  });

  it('falls back to periodic health re-sync when no event is received', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const getDaemonStatus = vi.fn().mockResolvedValue({ state: 'running' });

    renderInApp(<DaemonStatusBar />, {
      desktop: makeDesktop({ state: 'starting' }, { getDaemonStatus }),
    });

    // Initial fetch on mount.
    await waitFor(() => expect(getDaemonStatus).toHaveBeenCalledTimes(1));

    // Advance past the fallback interval.
    await act(async () => {
      vi.advanceTimersByTime(10_000);
    });

    expect(getDaemonStatus).toHaveBeenCalledTimes(2);

    // Cleanup.
    vi.useRealTimers();
  });
});
