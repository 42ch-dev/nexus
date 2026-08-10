/**
 * Settings modal primary — section registry, deep links, background restore.
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { screen, waitFor, within, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useEffect } from 'react';
import { Route, Routes, useLocation, useNavigate } from 'react-router';

import { RootLayout } from '@/components/layout/root-layout';
import {
  SettingsModalProvider,
  useSettingsModal,
} from '@/components/layout/settings-modal-context';
import { SettingsModalHost } from '@/components/layout/settings-modal-host';
import { ThemeProvider } from '@/components/theme-provider';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';

vi.mock('@/components/brand/nexus-logo', () => ({
  NexusLogo: () => <div data-testid="nexus-logo">Nexus</div>,
}));

vi.mock('@/components/brand/nexus-ink-logo', () => ({
  NexusInkLogo: () => <div data-testid="nexus-ink-logo">Nexus</div>,
}));

vi.mock('@/components/daemon-health-indicator', () => ({
  DaemonHealthIndicator: () => <div data-testid="daemon-health">ok</div>,
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

/** App-shaped shell: product routes + single Settings modal host. */
function ModalAppRoutes() {
  const location = useLocation();
  const { open, backgroundLocation } = useSettingsModal();
  const routesLocation = open ? backgroundLocation : location;

  return (
    <Routes location={routesLocation}>
      <Route element={<RootLayout />}>
        <Route path="works" element={<div data-testid="works-stub">Works stub</div>} />
        <Route path="sessions" element={<div data-testid="sessions-stub">Sessions stub</div>} />
        <Route
          path="connect"
          element={<span data-testid="connect-redirect-marker">redirecting</span>}
        />
      </Route>
    </Routes>
  );
}

function ModalAppShell() {
  return (
    <ThemeProvider>
      <SettingsModalProvider>
        <ModalAppRoutes />
        <SettingsModalHost />
      </SettingsModalProvider>
    </ThemeProvider>
  );
}

function makeClient() {
  return new BrowserClient();
}

function LocationProbe() {
  const location = useLocation();
  return (
    <span data-testid="location-probe">
      {location.pathname}
      {location.hash}
    </span>
  );
}

function scanHandler(
  agents: Array<Record<string, unknown>> = [
    {
      name: 'codex',
      registry_agent_id: 'codex-native',
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

function modulesListHandler() {
  return http.get('/v1/daemon/compute/modules', () =>
    HttpResponse.json({ items: [] }),
  );
}

function renderModalApp(initialRouterEntries: string[]) {
  return renderInApp(
    <>
      <ModalAppShell />
      <LocationProbe />
    </>,
    {
      client: makeClient(),
      activeCreatorId: 'creator-a',
      setupCompleted: true,
      initialRouterEntries,
    },
  );
}

describe('Settings modal primary', () => {
  beforeEach(async () => {
    mockMatchMedia(false);
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  it('opens Agent section from a direct /settings load over /works background', async () => {
    useHandlers(scanHandler(), creatorsHandler(), healthHandler());
    renderModalApp(['/settings']);

    await waitFor(() =>
      expect(screen.getByTestId('settings-modal-body')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('settings-agent-section')).toBeInTheDocument();
    expect(screen.getByTestId('works-stub')).toBeInTheDocument();
    expect(screen.getByTestId('location-probe')).toHaveTextContent('/settings/agent');
    expect(screen.queryByTestId('settings-shell-page-chrome')).not.toBeInTheDocument();
  });

  it('renders section nav with Agent, Profiles, Appearance, Modules, and Advanced', async () => {
    useHandlers(scanHandler(), creatorsHandler(), healthHandler());
    renderModalApp(['/settings/agent']);

    await waitFor(() =>
      expect(screen.getByTestId('settings-section-nav')).toBeInTheDocument(),
    );
    const nav = screen.getByTestId('settings-section-nav');
    expect(within(nav).getByTestId('settings-section-nav-agent')).toHaveTextContent('Agent');
    expect(within(nav).getByTestId('settings-section-nav-workspace')).toHaveTextContent(
      'Profiles',
    );
    expect(within(nav).getByTestId('settings-section-nav-appearance')).toHaveTextContent(
      'Appearance',
    );
    expect(within(nav).getByTestId('settings-section-nav-modules')).toHaveTextContent(
      'Modules',
    );
    expect(within(nav).getByTestId('settings-section-nav-advanced')).toHaveTextContent(
      'Advanced',
    );
  });

  it('renders localized section nav labels in zh-CN', async () => {
    window.localStorage.setItem('nexus-web-locale', 'zh-CN');
    useHandlers(scanHandler(), creatorsHandler(), healthHandler());
    renderModalApp(['/settings/agent']);

    await waitFor(() =>
      expect(screen.getByTestId('settings-section-nav')).toBeInTheDocument(),
    );
    const nav = screen.getByTestId('settings-section-nav');
    expect(within(nav).getByTestId('settings-section-nav-agent')).toHaveTextContent('智能体');
    expect(within(nav).getByTestId('settings-section-nav-modules')).toHaveTextContent('模块');
    expect(within(nav).getByTestId('settings-section-nav-advanced')).toHaveTextContent('高级');
  });

  it('switches section content when section nav is clicked', async () => {
    const user = userEvent.setup();
    useHandlers(scanHandler(), creatorsHandler(), healthHandler());
    renderModalApp(['/settings/agent']);

    await waitFor(() =>
      expect(screen.getByTestId('settings-agent-section')).toBeInTheDocument(),
    );

    await user.click(screen.getByTestId('settings-section-nav-advanced'));

    await waitFor(() =>
      expect(screen.getByTestId('settings-advanced-section')).toBeInTheDocument(),
    );
    expect(screen.queryByTestId('settings-agent-section')).not.toBeInTheDocument();
    expect(screen.getByTestId('location-probe')).toHaveTextContent('/settings/advanced');
  });

  it('mounts Modules section body inside the modal', async () => {
    useHandlers(
      scanHandler(),
      creatorsHandler(),
      healthHandler(),
      modulesListHandler(),
    );
    renderModalApp(['/settings/modules']);

    await waitFor(() =>
      expect(screen.getByTestId('settings-modules-section')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('modules-page-body')).toBeInTheDocument();
  });

  it('normalizes /modules into the Modules section', async () => {
    useHandlers(
      scanHandler(),
      creatorsHandler(),
      healthHandler(),
      modulesListHandler(),
    );
    renderModalApp(['/modules']);

    await waitFor(() =>
      expect(screen.getByTestId('settings-modules-section')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('location-probe')).toHaveTextContent('/settings/modules');
  });

  it('normalizes /settings/connection to advanced#connection', async () => {
    useHandlers(scanHandler(), creatorsHandler(), healthHandler());
    renderModalApp(['/settings/connection']);

    await waitFor(() =>
      expect(screen.getByTestId('settings-advanced-section')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('settings-connection-section')).toBeInTheDocument();
    expect(screen.getByTestId('location-probe')).toHaveTextContent(
      '/settings/advanced#connection',
    );
  });

  it('normalizes /settings/setup to advanced#setup', async () => {
    useHandlers(scanHandler(), creatorsHandler(), healthHandler());
    renderModalApp(['/settings/setup']);

    await waitFor(() =>
      expect(screen.getByTestId('settings-setup-section')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('location-probe')).toHaveTextContent('/settings/advanced#setup');
  });

  it('unknown sections fall back to Agent', async () => {
    useHandlers(scanHandler(), creatorsHandler(), healthHandler());
    renderModalApp(['/settings/not-a-section']);

    await waitFor(() =>
      expect(screen.getByTestId('settings-agent-section')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('location-probe')).toHaveTextContent('/settings/agent');
  });

  it('restores the prior safe route on clean close', async () => {
    const user = userEvent.setup();
    useHandlers(scanHandler(), creatorsHandler(), healthHandler());
    renderModalApp(['/sessions']);

    expect(screen.getByTestId('sessions-stub')).toBeInTheDocument();

    const settingsGear = await screen.findByTestId('chronos-titlebar-settings-gear');
    await user.click(settingsGear);

    await waitFor(() =>
      expect(screen.getByTestId('settings-modal-body')).toBeInTheDocument(),
    );
    // Background product route stays mounted.
    expect(screen.getByTestId('sessions-stub')).toBeInTheDocument();

    await user.keyboard('{Escape}');

    await waitFor(() =>
      expect(screen.queryByTestId('settings-modal-body')).not.toBeInTheDocument(),
    );
    expect(screen.getByTestId('location-probe')).toHaveTextContent('/sessions');
    expect(screen.getByTestId('sessions-stub')).toBeInTheDocument();
  });

  it('shows discard confirmation when a dirty source is registered', async () => {
    const user = userEvent.setup();
    useHandlers(scanHandler(), creatorsHandler(), healthHandler());

    function DirtySourceHost() {
      const { registerDirtySource, open } = useSettingsModal();
      useEffect(() => {
        registerDirtySource('test-dirty', open);
        return () => registerDirtySource('test-dirty', false);
      }, [open, registerDirtySource]);
      return null;
    }

    function DirtyTestRoutes() {
      const location = useLocation();
      const { open, backgroundLocation } = useSettingsModal();
      const routesLocation = open ? backgroundLocation : location;
      return (
        <Routes location={routesLocation}>
          <Route element={<RootLayout />}>
            <Route path="works" element={<div data-testid="works-stub">Works</div>} />
          </Route>
        </Routes>
      );
    }

    renderInApp(
      <ThemeProvider>
        <SettingsModalProvider>
          <DirtyTestRoutes />
          <DirtySourceHost />
          <SettingsModalHost />
          <LocationProbe />
        </SettingsModalProvider>
      </ThemeProvider>,
      {
        client: makeClient(),
        activeCreatorId: 'creator-a',
        setupCompleted: true,
        initialRouterEntries: ['/works'],
      },
    );

    const gear = await screen.findByTestId('chronos-titlebar-settings-gear');
    await user.click(gear);

    await waitFor(() =>
      expect(screen.getByTestId('settings-modal-body')).toBeInTheDocument(),
    );

    await user.keyboard('{Escape}');

    await waitFor(() =>
      expect(screen.getByTestId('settings-discard-confirm')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('settings-modal-body')).toBeInTheDocument();

    await user.click(screen.getByTestId('settings-discard-confirm-button'));

    await waitFor(() =>
      expect(screen.queryByTestId('settings-modal-body')).not.toBeInTheDocument(),
    );
    expect(screen.getByTestId('location-probe')).toHaveTextContent('/works');
  });

  it('blocks dirty route leave to a non-settings path until discard confirms', async () => {
    const user = userEvent.setup();
    useHandlers(scanHandler(), creatorsHandler(), healthHandler());

    let navigateAway: (() => void) | undefined;

    function DirtySourceHost() {
      const { registerDirtySource } = useSettingsModal();
      useEffect(() => {
        // Keep dirty asserted across the brief non-settings URL beat while RR7
        // applies a blocked leave (open flickers false before restore).
        registerDirtySource('test-dirty-route', true);
        return () => registerDirtySource('test-dirty-route', false);
      }, [registerDirtySource]);
      return null;
    }

    function LeaveSettingsBridge() {
      const navigate = useNavigate();
      useEffect(() => {
        navigateAway = () => {
          navigate('/sessions');
        };
        return () => {
          navigateAway = undefined;
        };
      }, [navigate]);
      return null;
    }

    renderInApp(
      <ThemeProvider>
        <SettingsModalProvider>
          <ModalAppRoutes />
          <DirtySourceHost />
          <LeaveSettingsBridge />
          <SettingsModalHost />
          <LocationProbe />
        </SettingsModalProvider>
      </ThemeProvider>,
      {
        client: makeClient(),
        activeCreatorId: 'creator-a',
        setupCompleted: true,
        initialRouterEntries: ['/works'],
      },
    );

    const gear = await screen.findByTestId('chronos-titlebar-settings-gear');
    await user.click(gear);

    await waitFor(() =>
      expect(screen.getByTestId('settings-modal-body')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('location-probe')).toHaveTextContent('/settings/agent');

    expect(navigateAway).toBeTypeOf('function');
    act(() => {
      navigateAway!();
    });

    await waitFor(() =>
      expect(screen.getByTestId('settings-discard-confirm')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('settings-modal-body')).toBeInTheDocument();
    expect(screen.getByTestId('location-probe')).toHaveTextContent('/settings/agent');

    await user.click(screen.getByTestId('settings-discard-confirm-button'));

    await waitFor(() =>
      expect(screen.queryByTestId('settings-modal-body')).not.toBeInTheDocument(),
    );
    expect(screen.getByTestId('location-probe')).toHaveTextContent('/sessions');
    expect(screen.getByTestId('sessions-stub')).toBeInTheDocument();
  });

  it('exposes Settings on the Chronos titlebar gear (V1.131 P0/P2)', async () => {
    const user = userEvent.setup();
    useHandlers(scanHandler(), creatorsHandler(), healthHandler());
    renderModalApp(['/works']);

    const settingsGear = await screen.findByTestId('chronos-titlebar-settings-gear');
    expect(settingsGear).toHaveAttribute('aria-label', 'Settings');

    await user.click(settingsGear);

    await waitFor(() =>
      expect(screen.getByTestId('settings-modal-body')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('settings-agent-section')).toBeInTheDocument();
  });

  it('includes Settings in mobile nav linking to /settings', () => {
    useHandlers(creatorsHandler(), healthHandler());
    renderModalApp(['/works']);

    const links = screen.getAllByRole('link', { name: 'Settings' });
    expect(links.length).toBeGreaterThanOrEqual(1);
    expect(links.some((el) => el.getAttribute('href') === '/settings')).toBe(true);
  });
});

describe('SettingsAgentSection in modal', () => {
  beforeEach(async () => {
    mockMatchMedia(false);
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  it('renders AgentPicker body (browser, no desktop)', async () => {
    useHandlers(scanHandler(), creatorsHandler(), healthHandler());
    renderModalApp(['/settings/agent']);

    await waitFor(() =>
      expect(screen.getByTestId('settings-agent-section')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('settings-shell')).toBeInTheDocument();
    expect(screen.getByTestId('settings-host-picker-region')).toBeInTheDocument();
    expect(screen.queryByTestId('wizard-cta-row')).not.toBeInTheDocument();

    await waitFor(() =>
      expect(screen.getByTestId('agent-picker')).toBeInTheDocument(),
    );
    expect(await screen.findByTestId('agent-card-codex-native')).toBeInTheDocument();
  });
});
