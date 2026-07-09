/**
 * Settings shell — nested routes, section nav, Agent body, /connect redirect.
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Navigate, Route, Routes } from 'react-router-dom';

import { SettingsAgentSection } from '@/pages/settings/settings-agent-section';
import { SettingsConnectionSection } from '@/pages/settings/settings-connection-section';
import { SettingsSetupSection } from '@/pages/settings/settings-setup-section';
import { SettingsShellLayout } from '@/pages/settings/settings-shell-layout';
import { RootLayout } from '@/components/layout/root-layout';
import { ThemeProvider } from '@/components/theme-provider';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';

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

function healthHandler() {
  return http.get('/v1/daemon/runtime/health', () =>
    HttpResponse.json({ status: 'ok', version: 'test' }),
  );
}

/** Nested settings tree matching App.tsx (P0 — no workspace). */
const settingsRouteTree = (
  <Route path="settings" element={<SettingsShellLayout />}>
    <Route index element={<Navigate to="agent" replace />} />
    <Route path="agent" element={<SettingsAgentSection />} />
    <Route path="connection" element={<SettingsConnectionSection />} />
    <Route path="setup" element={<SettingsSetupSection />} />
  </Route>
);

describe('SettingsAgentSection', () => {
  it('renders AgentPicker body (browser, no desktop)', async () => {
    useHandlers(scanHandler(), creatorsHandler());

    renderInApp(
      <Routes>
        {settingsRouteTree}
      </Routes>,
      {
        client: makeClient(),
        initialRouterEntries: ['/settings/agent'],
      },
    );

    expect(screen.getByTestId('settings-shell')).toBeInTheDocument();
    expect(screen.getByTestId('settings-agent-section')).toBeInTheDocument();
    expect(
      screen.getByRole('heading', { name: 'Settings', level: 2 }),
    ).toBeInTheDocument();
    expect(screen.getByTestId('settings-host-picker-region')).toBeInTheDocument();
    expect(screen.queryByTestId('wizard-cta-row')).not.toBeInTheDocument();

    await waitFor(() =>
      expect(screen.getByTestId('agent-card-openai/codex')).toBeInTheDocument(),
    );
  });
});

describe('Settings shell routes', () => {
  beforeEach(() => {
    mockMatchMedia(false);
    window.localStorage.clear();
  });

  it('redirects /settings index to Agent section', async () => {
    useHandlers(scanHandler(), creatorsHandler());

    renderInApp(
      <Routes>
        {settingsRouteTree}
      </Routes>,
      {
        client: makeClient(),
        initialRouterEntries: ['/settings'],
      },
    );

    await waitFor(() =>
      expect(screen.getByTestId('settings-agent-section')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('settings-section-nav-agent')).toHaveAttribute(
      'aria-current',
      'page',
    );
  });

  it('renders section nav with Agent, Connection, Setup (no Workspace)', () => {
    useHandlers(scanHandler(), creatorsHandler());

    renderInApp(
      <Routes>
        {settingsRouteTree}
      </Routes>,
      {
        client: makeClient(),
        initialRouterEntries: ['/settings/agent'],
      },
    );

    const nav = screen.getByTestId('settings-section-nav');
    expect(within(nav).getByTestId('settings-section-nav-agent')).toHaveTextContent(
      'Agent',
    );
    expect(
      within(nav).getByTestId('settings-section-nav-connection'),
    ).toHaveTextContent('Connection');
    expect(within(nav).getByTestId('settings-section-nav-setup')).toHaveTextContent(
      'Setup',
    );
    expect(
      within(nav).queryByTestId('settings-section-nav-workspace'),
    ).not.toBeInTheDocument();
    expect(within(nav).queryByText('Workspace')).not.toBeInTheDocument();
  });

  it('switches outlet when section nav is clicked', async () => {
    const user = userEvent.setup();
    useHandlers(scanHandler(), creatorsHandler());

    renderInApp(
      <Routes>
        {settingsRouteTree}
      </Routes>,
      {
        client: makeClient(),
        initialRouterEntries: ['/settings/agent'],
      },
    );

    await user.click(screen.getByTestId('settings-section-nav-setup'));

    await waitFor(() =>
      expect(screen.getByTestId('settings-setup-section')).toBeInTheDocument(),
    );
    expect(screen.queryByTestId('settings-agent-section')).not.toBeInTheDocument();
    expect(screen.getByTestId('settings-section-nav-setup')).toHaveAttribute(
      'aria-current',
      'page',
    );
  });

  it('mounts Connection section outlet', async () => {
    useHandlers(scanHandler(), creatorsHandler());

    renderInApp(
      <Routes>
        {settingsRouteTree}
      </Routes>,
      {
        client: makeClient(),
        initialRouterEntries: ['/settings/connection'],
      },
    );

    expect(screen.getByTestId('settings-connection-section')).toBeInTheDocument();
    expect(screen.getByTestId('settings-section-nav-connection')).toHaveAttribute(
      'aria-current',
      'page',
    );
  });

  it('redirects /connect to /settings/connection', async () => {
    useHandlers(scanHandler(), creatorsHandler());

    renderInApp(
      <Routes>
        {settingsRouteTree}
        <Route
          path="connect"
          element={<Navigate to="/settings/connection" replace />}
        />
      </Routes>,
      {
        client: makeClient(),
        initialRouterEntries: ['/connect'],
      },
    );

    await waitFor(() =>
      expect(screen.getByTestId('settings-connection-section')).toBeInTheDocument(),
    );
  });

  it('exposes Settings in sidebar footer utility and opens Agent via /settings', async () => {
    const user = userEvent.setup();
    useHandlers(scanHandler(), creatorsHandler(), healthHandler());

    renderInApp(
      <Routes>
        <Route element={<LayoutWithTheme />}>
          {settingsRouteTree}
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
      expect(screen.getByTestId('settings-agent-section')).toBeInTheDocument(),
    );
    expect(
      within(screen.getByTestId('settings-shell')).getByRole('heading', {
        name: 'Settings',
        level: 2,
      }),
    ).toBeInTheDocument();
  });

  it('includes Settings in mobile nav', () => {
    useHandlers(creatorsHandler(), healthHandler());

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

    const links = screen.getAllByRole('link', { name: 'Settings' });
    expect(links.length).toBeGreaterThanOrEqual(1);
    expect(links.some((el) => el.getAttribute('href') === '/settings')).toBe(
      true,
    );
  });
});
