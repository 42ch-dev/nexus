/**
 * MainBanner tests — top-of-main-content daemon failure banner.
 *
 * V1.94: degraded / stopped / error daemon states surface a banner with detail
 * + Restart CTA. Running state renders nothing.
 */
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { MainBanner } from '@/components/layout/main-banner';
import { renderInApp } from '@/test/test-providers';
import type { DesktopCapabilities, DaemonStatus } from '@/lib/nexus/desktop-capabilities';

function makeDesktop(
  status: { state: string; detail?: string },
  impl: Partial<DesktopCapabilities> = {},
): DesktopCapabilities {
  const listeners = new Set<(status: DaemonStatus) => void>();
  const trigger = (next: DaemonStatus) => listeners.forEach((cb) => cb(next));

  return {
    openWith: vi.fn().mockResolvedValue(undefined),
    revealInFinder: vi.fn().mockResolvedValue(undefined),
    getSetupCompleted: vi.fn().mockResolvedValue(true),
    setSetupCompleted: vi.fn().mockResolvedValue(undefined),
    getWorkspaceRoot: vi.fn().mockResolvedValue('~/Documents/nexus42/default'),
    getDaemonStatus: vi.fn().mockResolvedValue(status),
    onDaemonStatusChanged: vi.fn().mockImplementation((cb) => {
      listeners.add(cb);
      return Promise.resolve(() => listeners.delete(cb));
    }),
    startDaemon: vi.fn().mockResolvedValue(undefined),
    stopDaemon: vi.fn().mockResolvedValue(undefined),
    ...impl,
    _triggerStatusChange: trigger,
  } as DesktopCapabilities;
}

describe('MainBanner daemon failure surfaces', () => {
  it('browser build renders nothing', () => {
    const { container } = renderInApp(<MainBanner />);
    expect(container.firstChild).toBeNull();
  });

  it('running daemon renders nothing', async () => {
    const { container } = renderInApp(<MainBanner />, {
      desktop: makeDesktop({ state: 'running' }),
    });
    await waitFor(() => expect(container.firstChild).toBeNull());
  });

  it('stopped daemon shows detail and starts the daemon', async () => {
    const startDaemon = vi.fn().mockResolvedValue(undefined);
    const stopDaemon = vi.fn().mockResolvedValue(undefined);

    renderInApp(<MainBanner />, {
      desktop: makeDesktop(
        { state: 'stopped', detail: 'Restart the daemon to use local workspace features.' },
        { startDaemon, stopDaemon },
      ),
    });

    expect(
      await screen.findByText(/Restart the daemon to use local workspace features/i),
    ).toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: /Restart Daemon/i }));

    expect(stopDaemon).not.toHaveBeenCalled();
    expect(startDaemon).toHaveBeenCalled();
  });

  it('error daemon restarts with stop then start', async () => {
    const startDaemon = vi.fn().mockResolvedValue(undefined);
    const stopDaemon = vi.fn().mockResolvedValue(undefined);

    renderInApp(<MainBanner />, {
      desktop: makeDesktop({ state: 'error', detail: 'Daemon crashed.' }, { startDaemon, stopDaemon }),
    });

    await userEvent.click(await screen.findByRole('button', { name: /Restart Daemon/i }));

    expect(stopDaemon).toHaveBeenCalled();
    expect(startDaemon).toHaveBeenCalled();
  });

  it('degraded daemon restarts with stop then start', async () => {
    const startDaemon = vi.fn().mockResolvedValue(undefined);
    const stopDaemon = vi.fn().mockResolvedValue(undefined);

    renderInApp(<MainBanner />, {
      desktop: makeDesktop({ state: 'degraded' }, { startDaemon, stopDaemon }),
    });

    await userEvent.click(await screen.findByRole('button', { name: /Restart Daemon/i }));

    expect(stopDaemon).toHaveBeenCalled();
    expect(startDaemon).toHaveBeenCalled();
  });

  it('updates when the Rust side emits a running event', async () => {
    const desktop = makeDesktop({ state: 'stopped' });

    renderInApp(<MainBanner />, { desktop });
    await screen.findByRole('button', { name: /Restart Daemon/i });

    (
      desktop as unknown as {
        _triggerStatusChange: (status: DaemonStatus) => void;
      }
    )._triggerStatusChange({ state: 'running', port: 8420 });

    await waitFor(() => {
      expect(screen.queryByRole('button', { name: /Restart Daemon/i })).not.toBeInTheDocument();
    });
  });
});
