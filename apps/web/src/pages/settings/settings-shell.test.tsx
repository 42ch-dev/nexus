/**
 * Settings shell — nested routes, section nav, Agent body, /connect redirect.
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Navigate, Route, Routes, useLocation } from 'react-router-dom';

import { SettingsAgentSection } from '@/pages/settings/settings-agent-section';
import { SettingsAdvancedSection } from '@/pages/settings/settings-advanced-section';
import { SettingsShellLayout } from '@/pages/settings/settings-shell-layout';
import { SettingsAppearanceSection } from '@/pages/settings/settings-appearance-section';
import { SettingsWorkspaceSection } from '@/pages/settings/settings-workspace-section';
import { RootLayout } from '@/components/layout/root-layout';
import { ThemeProvider } from '@/components/theme-provider';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';

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

function HashProbe() {
  const { hash } = useLocation();
  return <span data-testid="location-hash">{hash}</span>;
}

function scanHandler(
  agents: Array<Record<string, unknown>> = [
    {
      name: 'codex',
      registry_agent_id: 'codex-acp',
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

/** Nested settings tree matching App.tsx (V1.106 P2 — Advanced hosts Connection + Setup). */
const settingsRouteTree = (
  <Route path="settings" element={<SettingsShellLayout />}>
    <Route index element={<Navigate to="agent" replace />} />
    <Route path="agent" element={<SettingsAgentSection />} />
    <Route path="advanced" element={<SettingsAdvancedSection />} />
    <Route path="workspace" element={<SettingsWorkspaceSection />} />
    <Route path="appearance" element={<SettingsAppearanceSection />} />
    <Route
      path="connection"
      element={<Navigate to="/settings/advanced#connection" replace />}
    />
    <Route
      path="setup"
      element={<Navigate to="/settings/advanced#setup" replace />}
    />
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
      expect(screen.getByTestId('agent-card-codex-acp')).toBeInTheDocument(),
    );
  });
});

describe('Settings shell routes', () => {
  beforeEach(async () => {
    mockMatchMedia(false);
    window.localStorage.clear();
    await i18n.changeLanguage('en');
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

  it('renders section nav with Agent, Workspace, Appearance, and Advanced', () => {
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
      within(nav).getByTestId('settings-section-nav-workspace'),
    ).toHaveTextContent('Profiles');
    expect(
      within(nav).getByTestId('settings-section-nav-appearance'),
    ).toHaveTextContent('Appearance');
    expect(within(nav).getByTestId('settings-section-nav-advanced')).toHaveTextContent(
      'Advanced',
    );
    expect(
      within(nav).queryByTestId('settings-section-nav-connection'),
    ).not.toBeInTheDocument();
    expect(
      within(nav).queryByTestId('settings-section-nav-setup'),
    ).not.toBeInTheDocument();
  });

  it('renders localized section nav labels in zh-CN', () => {
    window.localStorage.setItem('nexus-web-locale', 'zh-CN');
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
      '智能体',
    );
    expect(
      within(nav).getByTestId('settings-section-nav-workspace'),
    ).toHaveTextContent('Profiles');
    expect(
      within(nav).getByTestId('settings-section-nav-appearance'),
    ).toHaveTextContent('外观');
    expect(within(nav).getByTestId('settings-section-nav-advanced')).toHaveTextContent(
      '高级',
    );
  });

  it('mounts Workspace section outlet and marks nav active', async () => {
    renderInApp(
      <Routes>
        {settingsRouteTree}
      </Routes>,
      {
        client: makeClient(),
        initialRouterEntries: ['/settings/workspace'],
      },
    );

    await waitFor(() =>
      expect(screen.getByTestId('settings-workspace-section')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('settings-section-nav-workspace')).toHaveAttribute(
      'aria-current',
      'page',
    );
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

    await user.click(screen.getByTestId('settings-section-nav-advanced'));

    await waitFor(() =>
      expect(screen.getByTestId('settings-advanced-section')).toBeInTheDocument(),
    );
    expect(screen.queryByTestId('settings-agent-section')).not.toBeInTheDocument();
    expect(screen.getByTestId('settings-section-nav-advanced')).toHaveAttribute(
      'aria-current',
      'page',
    );
  });

  it('mounts Advanced page with Connection and Setup sections', async () => {
    useHandlers(scanHandler(), creatorsHandler());

    renderInApp(
      <Routes>
        {settingsRouteTree}
      </Routes>,
      {
        client: makeClient(),
        initialRouterEntries: ['/settings/advanced'],
      },
    );

    expect(screen.getByTestId('settings-advanced-section')).toBeInTheDocument();
    expect(screen.getByTestId('settings-connection-section')).toBeInTheDocument();
    expect(screen.getByTestId('settings-setup-section')).toBeInTheDocument();
    expect(screen.getByTestId('connect-daemon-form')).toBeInTheDocument();
    expect(screen.getByTestId('settings-section-nav-advanced')).toHaveAttribute(
      'aria-current',
      'page',
    );
    expect(
      screen.getByText(
        /Connect this app to a remote Nexus daemon\. Your local daemon stays the default/i,
      ),
    ).toBeInTheDocument();
  });

  it('permanently redirects /connect to /settings/advanced#connection', async () => {
    useHandlers(scanHandler(), creatorsHandler());

    renderInApp(
      <>
        <Routes>
          {settingsRouteTree}
          <Route
            path="connect"
            element={<Navigate to="/settings/advanced#connection" replace />}
          />
        </Routes>
        <HashProbe />
      </>,
      {
        client: makeClient(),
        initialRouterEntries: ['/connect'],
      },
    );

    await waitFor(() =>
      expect(screen.getByTestId('settings-advanced-section')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('settings-connection-section')).toBeInTheDocument();
    expect(screen.getByTestId('connect-daemon-form')).toBeInTheDocument();
    expect(screen.getByTestId('settings-section-nav-advanced')).toHaveAttribute(
      'aria-current',
      'page',
    );
    expect(screen.getByTestId('location-hash')).toHaveTextContent('#connection');
  });

  it('permanently redirects /settings/connection to /settings/advanced#connection', async () => {
    useHandlers(scanHandler(), creatorsHandler());

    renderInApp(
      <>
        <Routes>{settingsRouteTree}</Routes>
        <HashProbe />
      </>,
      {
        client: makeClient(),
        initialRouterEntries: ['/settings/connection'],
      },
    );

    await waitFor(() =>
      expect(screen.getByTestId('settings-advanced-section')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('settings-connection-section')).toBeInTheDocument();
    expect(screen.getByTestId('settings-section-nav-advanced')).toHaveAttribute(
      'aria-current',
      'page',
    );
    expect(screen.getByTestId('location-hash')).toHaveTextContent('#connection');
  });

  it('permanently redirects /settings/setup to /settings/advanced#setup', async () => {
    useHandlers(scanHandler(), creatorsHandler());

    renderInApp(
      <>
        <Routes>{settingsRouteTree}</Routes>
        <HashProbe />
      </>,
      {
        client: makeClient(),
        initialRouterEntries: ['/settings/setup'],
      },
    );

    await waitFor(() =>
      expect(screen.getByTestId('settings-advanced-section')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('settings-setup-section')).toBeInTheDocument();
    expect(screen.getByTestId('settings-section-nav-advanced')).toHaveAttribute(
      'aria-current',
      'page',
    );
    expect(screen.getByTestId('location-hash')).toHaveTextContent('#setup');
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
