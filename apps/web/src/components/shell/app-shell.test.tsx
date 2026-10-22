/**
 * App shell (plan v1219 T2) — product-behaviour tests:
 *
 *  - Both entrances render the same shell with different rail contents.
 *  - Locked rail items render disabled with the § Copy reason as tooltip AND
 *    toast (same sentence) and never navigate — the chunk stays unrequested.
 *  - Theme toggle switches light/dark through the existing token mechanism.
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it, vi, type Mock } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Route, Routes } from 'react-router';
import { Globe } from 'lucide-react';

import { AppRail } from '@/components/shell/app-rail';
import { AppShell } from '@/components/shell/app-shell';
import { SettingsModalProvider } from '@/components/layout/settings-modal-context';
import { ThemeProvider } from '@/components/theme-provider';
import { i18n } from '@/lib/i18n/config';
import { RAIL_LOCK_REASON, type AppDef } from '@/lib/app-registry';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';

// Keep the DOM focused on shell assertions.
vi.mock('@/components/canvas/canvas-nav-commands', () => ({
  CanvasNavCommands: () => null,
}));
vi.mock('@/components/command-palette', () => ({
  CommandPalette: () => null,
  openPalette: vi.fn(),
}));
vi.mock('@/components/brand/nexus-logo', () => ({
  NexusLogo: () => <div data-testid="nexus-logo">Nexus</div>,
}));

function useShellHandlers() {
  useHandlers(
    http.get('/v1/daemon/creators', () =>
      HttpResponse.json({
        items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
        pagination: { limit: 20, has_more: false },
      }),
    ),
    http.post('/v1/daemon/agent-host/scan', () => HttpResponse.json({ agents: [] })),
  );
}

function renderShell(entrance: 'content-creator' | 'developer' = 'content-creator') {
  window.localStorage.setItem('nexus-entrance', entrance);
  return renderInApp(
    <ThemeProvider>
      <SettingsModalProvider>
        <Routes>
          <Route element={<AppShell />}>
            <Route path="*" element={<div data-testid="outlet">content</div>} />
          </Route>
        </Routes>
      </SettingsModalProvider>
    </ThemeProvider>,
    { activeCreatorId: 'creator-a' },
  );
}

type MockLoad = Mock<AppDef['load']>;

interface LoadSpies {
  world: MockLoad;
  work: MockLoad;
  session: MockLoad;
}

function injectApps(loads: LoadSpies): readonly AppDef[] {
  const world: AppDef = { id: 'world', label: 'rail.world', icon: Globe, route: '/worlds', load: loads.world };
  return [
    world,
    {
      ...world,
      id: 'work',
      label: 'rail.work',
      route: '/works',
      load: loads.work,
      lockedUntil: 'brief-saved',
      lockReason: RAIL_LOCK_REASON.work,
    },
    {
      ...world,
      id: 'session',
      label: 'rail.session',
      route: '/sessions',
      load: loads.session,
      lockReason: RAIL_LOCK_REASON.session,
    },
  ];
}

function makeLoadSpies(): LoadSpies {
  const make = () => vi.fn<AppDef['load']>(() => Promise.resolve({ default: () => null }));
  return { world: make(), work: make(), session: make() };
}
describe('AppShell', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    document.documentElement.classList.remove('dark');
    await i18n.changeLanguage('zh-CN');
  });

  it('renders the same shell for both entrances with different rail contents', () => {
    useShellHandlers();
    renderShell('content-creator');
    expect(screen.getByTestId('app-shell')).toBeInTheDocument();
    expect(screen.getByTestId('topnav')).toBeInTheDocument();
    for (const id of ['world', 'work', 'session', 'memory']) {
      expect(screen.getByTestId(`rail-item-${id}`)).toBeInTheDocument();
    }
  });

  it('renders the developer rail with hub-first apps and no locks', () => {
    useShellHandlers();
    renderShell('developer');
    for (const id of ['hub', 'harness', 'modules', 'caps', 'session']) {
      expect(screen.getByTestId(`rail-item-${id}`)).not.toHaveAttribute('aria-disabled');
    }
  });

  it('shows search disabled in the topnav', () => {
    useShellHandlers();
    renderShell('content-creator');
    expect(screen.getByRole('button', { name: i18n.t('topnav.searchDisabled', { ns: 'shell' }) })).toBeDisabled();
  });

  it('toggles light/dark theme via the existing token class', async () => {
    useShellHandlers();
    const user = userEvent.setup();
    renderShell('content-creator');
    const themeButton = () => screen.getByRole('button', { name: i18n.t('topnav.theme', { ns: 'shell' }) });
    expect(document.documentElement.classList.contains('dark')).toBe(false);
    await user.click(themeButton());
    await waitFor(() => expect(document.documentElement.classList.contains('dark')).toBe(true));
    await user.click(themeButton());
    await waitFor(() => expect(document.documentElement.classList.contains('dark')).toBe(false));
  });
});

describe('AppRail — progressive activation', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('zh-CN');
  });

  it('locked items render disabled with the reason and never request the chunk', async () => {
    useShellHandlers();
    const user = userEvent.setup();
    const loads = makeLoadSpies();
    renderInApp(
      <Routes>
        <Route path="/worlds" element={<div data-testid="world-page">world</div>} />
        <Route path="/works" element={<div data-testid="work-page">work</div>} />
        <Route path="/sessions" element={<div data-testid="session-page">session</div>} />
        <Route path="*" element={<AppRail apps={injectApps(loads)} />} />
      </Routes>,
      { activeCreatorId: 'creator-a', initialRouterEntries: ['/'] },
    );

    const work = screen.getByTestId('rail-item-work');
    expect(work).toHaveAttribute('aria-disabled', 'true');
    expect(work).toHaveAttribute('title', '完成 Brief 后解锁「作品」');

    // Clicking the locked item shows the same sentence as a toast and does
    // NOT navigate — the load factory stays uninvoked.
    await user.click(work);
    expect(await screen.findByText('完成 Brief 后解锁「作品」')).toBeInTheDocument();
    expect(loads.work).not.toHaveBeenCalled();
    expect(loads.session).not.toHaveBeenCalled();

    const session = screen.getByTestId('rail-item-session');
    expect(session).toHaveAttribute('title', '创建作品后解锁「会话」');
    await user.click(session);
    expect(await screen.findByText('创建作品后解锁「会话」')).toBeInTheDocument();
    expect(loads.session).not.toHaveBeenCalled();
  });

  it('unlocked items navigate to their route', async () => {
    useShellHandlers();
    const user = userEvent.setup();
    const loads = makeLoadSpies();
    renderInApp(
      <Routes>
        <Route path="/worlds" element={<div data-testid="world-page">world</div>} />
        <Route path="*" element={<AppRail apps={injectApps(loads)} />} />
      </Routes>,
      { activeCreatorId: 'creator-a', initialRouterEntries: ['/'] },
    );
    await user.click(screen.getByTestId('rail-item-world'));
    expect(await screen.findByTestId('world-page')).toBeInTheDocument();
  });
});
