/**
 * DaemonStatusBar lifecycle + agent badge tests.
 *
 * V1.117 P2 (T3): single-line footer with left status dot + "Daemon running"
 * label + lowercase `running` tag, and right clickable agent badge (name+version
 * or placeholder → `/settings/agent`) + Restart. Renders when the daemon is
 * running or mid-restart (`starting`); other non-running states are surfaced
 * by the top-of-main-content {@link MainBanner}.
 */
import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi, afterEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useLocation } from 'react-router';

import { DaemonStatusBar } from '@/components/layout/daemon-status-bar';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import type { DesktopCapabilities, DaemonStatus } from '@/lib/nexus/desktop-capabilities';
import type { AgentScanEntry } from '@42ch/nexus-contracts';

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
    getEntrance: vi.fn().mockResolvedValue('content-creator'),
    setEntrance: vi.fn().mockResolvedValue(undefined),
    setAgentProfile: vi.fn().mockResolvedValue(undefined),
    getAgentProfile: vi.fn().mockResolvedValue(null),
    getWorkspaceRoot: vi.fn().mockResolvedValue('~/Documents/nexus42/default'),
    pickDirectory: vi.fn().mockResolvedValue(null),
    setWorkspacePath: vi.fn().mockResolvedValue(undefined),
    getDaemonStatus: vi.fn().mockResolvedValue(status),
    onDaemonStatusChanged: vi.fn().mockImplementation((cb) => {
      listeners.add(cb);
      return Promise.resolve(() => listeners.delete(cb));
    }),
    startDaemon: vi.fn().mockResolvedValue(undefined),
    stopDaemon: vi.fn().mockResolvedValue(undefined),
    resetLocalDatabase: vi.fn().mockResolvedValue(undefined),
    ...impl,
    // Expose a test-only trigger so event-driven updates can be simulated.
    _triggerStatusChange: trigger,
  } as DesktopCapabilities;
}

/** MSW handler returning a scan response with the given agents. */
function scanHandler(agents: AgentScanEntry[]) {
  return http.post('/v1/daemon/agent-host/scan', () =>
    HttpResponse.json({ agents }),
  );
}

describe('DaemonStatusBar lifecycle action', () => {
  it('browser build renders nothing', () => {
    const { container } = renderInApp(<DaemonStatusBar />);
    expect(container.firstChild).toBeNull();
  });

  it('running daemon shows status dot + Daemon running label + running tag + Restart, and calls restartDaemon when confirmed', async () => {
    const restartDaemon = vi.fn().mockResolvedValue(undefined);
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);

    renderInApp(<DaemonStatusBar />, {
      desktop: makeDesktop({ state: 'running' }, { restartDaemon }),
    });

    const bar = await screen.findByTestId('daemon-status-bar');
    expect(bar).toHaveTextContent('Daemon running');
    // Health tag copy was replaced with the lowercase `running` tag (AD-P2-5).
    expect(bar).not.toHaveTextContent('healthy');
    expect(screen.getByText('running')).toBeInTheDocument();
    expect(bar).not.toHaveTextContent(/reachable/i);

    // State dot encodes health (present, green).
    expect(bar.querySelector('.bg-green-700')).not.toBeNull();

    const button = await screen.findByRole('button', { name: /Restart daemon/i });
    expect(button).toBeInTheDocument();

    await userEvent.click(button);

    expect(confirmSpy).toHaveBeenCalledWith(
      'Restarting the daemon will interrupt any running orchestration. Continue?',
    );
    expect(restartDaemon).toHaveBeenCalled();

    confirmSpy.mockRestore();
  });

  it('does nothing when the restart confirmation is cancelled', async () => {
    const restartDaemon = vi.fn().mockResolvedValue(undefined);
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(false);

    renderInApp(<DaemonStatusBar />, {
      desktop: makeDesktop({ state: 'running' }, { restartDaemon }),
    });

    await userEvent.click(await screen.findByRole('button', { name: /Restart daemon/i }));

    expect(restartDaemon).not.toHaveBeenCalled();

    confirmSpy.mockRestore();
  });

  it('does not render for degraded/stopped/error daemon states', () => {
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

  it('stays mounted during starting with Restart pending copy', async () => {
    renderInApp(<DaemonStatusBar />, {
      desktop: makeDesktop({ state: 'starting' }),
    });

    const bar = await screen.findByTestId('daemon-status-bar');
    expect(bar).toHaveTextContent('Restarting…');
    expect(screen.getByRole('button', { name: /Restart daemon/i })).toBeDisabled();
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

describe('DaemonStatusBar agent badge (V1.117 P2 T3)', () => {
  it('shows the placeholder when no agent profile is saved (still clickable)', async () => {
    renderInApp(<DaemonStatusBar />, {
      desktop: makeDesktop({ state: 'running' }),
      client: new BrowserClient(),
    });

    const badge = await screen.findByTestId('daemon-status-agent-badge');
    expect(badge).toHaveTextContent('No agent');
  });

  it('shows displayName + version when the saved profile matches an installed scan entry', async () => {
    useHandlers(
      scanHandler([
        {
          name: 'Claude Code',
          registry_agent_id: 'claude-native',
          launch_command: '/usr/local/bin/claude',
          installed: true,
          version: '1.0.42',
        },
      ]),
    );

    renderInApp(<DaemonStatusBar />, {
      desktop: makeDesktop(
        { state: 'running' },
        {
          getAgentProfile: vi.fn().mockResolvedValue({
            name: 'Claude Code',
            launchCommand: '/usr/local/bin/claude',
          }),
        },
      ),
      client: new BrowserClient(),
    });

    const badge = await screen.findByTestId('daemon-status-agent-badge');
    // Override maps claude-native → "Claude"; version from the scan entry.
    await waitFor(() => {
      expect(badge).toHaveTextContent('Claude v1.0.42');
    });
  });

  it('omits the version segment when no scan entry matches the profile', async () => {
    useHandlers(scanHandler([]));

    renderInApp(<DaemonStatusBar />, {
      desktop: makeDesktop(
        { state: 'running' },
        {
          getAgentProfile: vi.fn().mockResolvedValue({ name: 'Claude Code' }),
        },
      ),
      client: new BrowserClient(),
    });

    const badge = await screen.findByTestId('daemon-status-agent-badge');
    // Falls back to the raw profile name; no " v..." suffix.
    await waitFor(() => {
      expect(badge).toHaveTextContent('Claude Code');
    });
    expect(badge).not.toHaveTextContent(/ v/);
  });

  it('navigates to /settings/agent when the badge is clicked', async () => {
    useHandlers(scanHandler([]));
    let currentPath = '/';

    renderInApp(
      <>
        <DaemonStatusBar />
        <LocationSentinel onChange={(p) => (currentPath = p)} />
      </>,
      {
        desktop: makeDesktop({ state: 'running' }),
        client: new BrowserClient(),
        initialRouterEntries: ['/'],
      },
    );

    const badge = await screen.findByTestId('daemon-status-agent-badge');
    await userEvent.click(badge);

    await waitFor(() => expect(currentPath).toBe('/settings/agent'));
  });
});

describe('DaemonStatusBar locale parity (V1.117 P2 T4)', () => {
  // Isolate localStorage so the zh-CN preference does not leak into the other
  // describe blocks in this file (they do not clear localStorage themselves).
  afterEach(() => {
    window.localStorage.clear();
  });

  it('renders the running tag + agent badge placeholder in zh-CN (AC-P2-7/8)', async () => {
    window.localStorage.setItem('nexus-web-locale', 'zh-CN');
    renderInApp(<DaemonStatusBar />, {
      desktop: makeDesktop({ state: 'running' }),
      client: new BrowserClient(),
    });

    const bar = await screen.findByTestId('daemon-status-bar');
    // AC-P2-7: `running` tag is lowercase in BOTH locales (AD-P2-5 / grill-me #7).
    expect(screen.getByText('running')).toBeInTheDocument();
    // AC-P2-8: zh-CN daemon-running label + agent badge placeholder.
    expect(bar).toHaveTextContent('守护进程运行中');
    expect(bar).toHaveTextContent('未选择智能体');
    // The deprecated `healthy` / `健康` status-tag copy must not appear.
    expect(bar).not.toHaveTextContent('健康');
    expect(bar).not.toHaveTextContent('healthy');
  });
});

describe('DaemonStatusBar Restart/agent a11y (V1.120 P1 T3, AC-P1-6)', () => {
  // Isolate localStorage so the zh-CN preference does not leak into other
  // describe blocks (they do not clear localStorage themselves).
  afterEach(() => {
    window.localStorage.clear();
  });

  it('Restart control exposes an accessible name + tooltip that name the daemon', async () => {
    renderInApp(<DaemonStatusBar />, {
      desktop: makeDesktop({ state: 'running' }),
      client: new BrowserClient(),
    });

    // AC-P1-6 / AD-P1-5: the icon-only control must not be a bare "Restart" —
    // its accessible name names the daemon, and the hover tooltip (title)
    // carries the same object so users know what restarts.
    const restart = await screen.findByRole('button', { name: 'Restart daemon' });
    expect(restart).toHaveAccessibleName('Restart daemon');
    expect(restart).toHaveAttribute('title', 'Restart daemon');
  });

  it('agent badge exposes an accessible name and a hover tooltip (AC-P1-6 context)', async () => {
    renderInApp(<DaemonStatusBar />, {
      desktop: makeDesktop({ state: 'running' }),
      client: new BrowserClient(),
    });

    const badge = await screen.findByTestId('daemon-status-agent-badge');
    // Visible label doubles as the accessible name; title provides the tooltip.
    expect(badge).toHaveAccessibleName('No agent');
    expect(badge).toHaveAttribute('title', 'No agent');
  });

  it('Restart accessible name + tooltip name the daemon in zh-CN too (locale parity)', async () => {
    window.localStorage.setItem('nexus-web-locale', 'zh-CN');
    renderInApp(<DaemonStatusBar />, {
      desktop: makeDesktop({ state: 'running' }),
      client: new BrowserClient(),
    });

    const restart = await screen.findByRole('button', { name: '重启守护进程' });
    expect(restart).toHaveAttribute('title', '重启守护进程');
  });
});

/**
 * Records react-router's current pathname on every render so a test can assert
 * where a navigation landed without mocking `useNavigate`.
 */
function LocationSentinel({ onChange }: { onChange: (pathname: string) => void }) {
  const location = useLocation();
  onChange(location.pathname);
  return null;
}
