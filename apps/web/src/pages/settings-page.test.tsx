/**
 * SettingsPage — thin host route + AgentPicker mount + setAgentProfile persist.
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Route, Routes } from 'react-router-dom';

import { SettingsPage } from '@/pages/settings-page';
import { RootLayout } from '@/components/layout/root-layout';
import { ThemeProvider } from '@/components/theme-provider';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import type { DesktopCapabilities } from '@/lib/nexus/desktop-capabilities';

vi.mock('@/components/brand/nexus-logo', () => ({
  NexusLogo: () => <div data-testid="nexus-logo">Nexus</div>,
}));

function mockMatchMedia(prefersDark = false) {
  const media = {
    matches: prefersDark,
    media: '(prefers-color-scheme: dark)',
    onchange: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
  };
  vi.spyOn(window, 'matchMedia').mockReturnValue(media as unknown as MediaQueryList);
}

/** RootLayout → Header needs ThemeProvider (not in default renderInApp stack). */
function LayoutWithTheme() {
  return (
    <ThemeProvider>
      <RootLayout />
    </ThemeProvider>
  );
}

function makeClient() {
  return new BrowserClient();
}

function makeDesktop(
  overrides: Partial<DesktopCapabilities> = {},
): DesktopCapabilities {
  return {
    openWith: () => Promise.resolve(),
    revealInFinder: () => Promise.resolve(),
    getDaemonStatus: () => Promise.resolve({ state: 'running', port: 8420 }),
    onDaemonStatusChanged: (callback) => {
      callback({ state: 'running', port: 8420 });
      return Promise.resolve(() => {});
    },
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
      Promise.resolve({
        creator_id: 'ctr_local1234567890ab',
        already_bootstrapped: true,
      }),
    ...overrides,
  };
}

function scanHandler(
  agents: Array<Record<string, unknown>> = [
    {
      name: 'codex',
      registry_agent_id: 'openai/codex',
      launch_command: 'codex',
      installed: true,
      version: '1.0.0',
    },
  ],
) {
  return http.post('/v1/daemon/agent-host/scan', () =>
    HttpResponse.json({ agents }),
  );
}

function creatorsHandler() {
  return http.get('/v1/daemon/creators', () =>
    HttpResponse.json({
      items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
      pagination: { limit: 20, has_more: false },
    }),
  );
}

describe('SettingsPage', () => {
  it('renders thin host chrome and mounts AgentPicker (browser, no desktop)', async () => {
    useHandlers(scanHandler(), creatorsHandler());

    renderInApp(<SettingsPage />, {
      client: makeClient(),
      initialRouterEntries: ['/settings'],
    });

    expect(screen.getByTestId('settings-page')).toBeInTheDocument();
    expect(
      screen.getByRole('heading', { name: 'Settings', level: 1 }),
    ).toBeInTheDocument();
    expect(screen.getByTestId('settings-host-picker-region')).toBeInTheDocument();
    expect(screen.queryByTestId('wizard-cta-row')).not.toBeInTheDocument();

    await waitFor(() =>
      expect(screen.getByTestId('agent-card-openai/codex')).toBeInTheDocument(),
    );
  });

  it('persists via setAgentProfile on Save Agent (desktop)', async () => {
    const user = userEvent.setup();
    const setAgentProfile = vi.fn(() => Promise.resolve());
    useHandlers(scanHandler(), creatorsHandler());

    renderInApp(<SettingsPage />, {
      client: makeClient(),
      desktop: makeDesktop({ setAgentProfile }),
      initialRouterEntries: ['/settings'],
    });

    await waitFor(() =>
      expect(screen.getByTestId('agent-card-openai/codex')).toBeInTheDocument(),
    );

    await user.click(screen.getByTestId('settings-save-agent'));

    await waitFor(() =>
      expect(setAgentProfile).toHaveBeenCalledWith('codex', 'codex'),
    );
    expect(await screen.findByText('Agent profile saved')).toBeInTheDocument();
  });

  it('shows desktop-only toast when saving without desktop caps', async () => {
    const user = userEvent.setup();
    useHandlers(scanHandler(), creatorsHandler());

    renderInApp(<SettingsPage />, {
      client: makeClient(),
      desktop: null,
      initialRouterEntries: ['/settings'],
    });

    await waitFor(() =>
      expect(screen.getByTestId('agent-card-openai/codex')).toBeInTheDocument(),
    );

    await user.click(screen.getByTestId('settings-save-agent'));

    expect(await screen.findByText('Desktop only')).toBeInTheDocument();
  });
});

describe('Settings route + shell nav', () => {
  beforeEach(() => {
    mockMatchMedia(false);
    window.localStorage.clear();
  });

  it('exposes Settings in sidebar footer utility and opens /settings', async () => {
    const user = userEvent.setup();
    useHandlers(
      scanHandler(),
      creatorsHandler(),
      http.get('/v1/daemon/runtime/health', () =>
        HttpResponse.json({ status: 'ok', version: 'test' }),
      ),
    );

    renderInApp(
      <Routes>
        <Route element={<LayoutWithTheme />}>
          <Route path="settings" element={<SettingsPage />} />
          <Route path="works" element={<div>Works stub</div>} />
        </Route>
      </Routes>,
      {
        client: makeClient(),
        activeCreatorId: 'creator-a',
        setupCompleted: true,
        initialRouterEntries: ['/works'],
      },
    );

    const settingsLink = await screen.findByTestId('settings-footer-utility-link');
    expect(settingsLink).toHaveTextContent('Settings');

    await user.click(settingsLink);

    await waitFor(() =>
      expect(screen.getByTestId('settings-page')).toBeInTheDocument(),
    );
    expect(
      within(screen.getByTestId('settings-page')).getByRole('heading', {
        name: 'Settings',
        level: 1,
      }),
    ).toBeInTheDocument();
  });

  it('includes Settings in mobile nav', () => {
    useHandlers(
      creatorsHandler(),
      http.get('/v1/daemon/runtime/health', () =>
        HttpResponse.json({ status: 'ok', version: 'test' }),
      ),
    );

    renderInApp(
      <Routes>
        <Route element={<LayoutWithTheme />}>
          <Route path="works" element={<div>Works stub</div>} />
        </Route>
      </Routes>,
      {
        client: makeClient(),
        activeCreatorId: 'creator-a',
        setupCompleted: true,
        initialRouterEntries: ['/works'],
      },
    );

    // Mobile strip + desktop footer both expose Settings; assert at least one link.
    const links = screen.getAllByRole('link', { name: 'Settings' });
    expect(links.length).toBeGreaterThanOrEqual(1);
    expect(links.some((el) => el.getAttribute('href') === '/settings')).toBe(
      true,
    );
  });
});
