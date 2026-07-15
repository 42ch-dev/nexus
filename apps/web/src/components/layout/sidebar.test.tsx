import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Route, Routes } from 'react-router-dom';

import { Sidebar } from './sidebar';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { worksList } from '@/test/handlers';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';

vi.mock('@/components/brand/nexus-logo', () => ({
  NexusLogo: () => <div data-testid="nexus-logo">Nexus</div>,
}));

function makeClient() {
  return new BrowserClient();
}

/** Creators + empty Works list — sidebar fetches both on mount. */
function useSidebarHandlers(works: unknown[] = []) {
  useHandlers(
    http.get('/v1/daemon/creators', () =>
      HttpResponse.json({
        items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
        pagination: { limit: 20, has_more: false },
      }),
    ),
    worksList(works),
  );
}

/** @deprecated use useSidebarHandlers */
function useCreatorHandler() {
  useSidebarHandlers();
}

describe('Sidebar', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  it('renders the Creator tab by default', async () => {
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    expect(screen.getByRole('tab', { name: 'Creator', selected: true })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'All Works' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Worlds' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Memories' })).toBeInTheDocument();
  });

  it('swaps to Orchestrator tab and shows runtime/strategy links', async () => {
    const user = userEvent.setup();
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    await user.click(screen.getByRole('tab', { name: 'Orchestrator' }));

    expect(screen.getByRole('tab', { name: 'Orchestrator', selected: true })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Sessions' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Schedule' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Capabilities' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Modules' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Strategies' })).toBeInTheDocument();

    expect(screen.queryByRole('link', { name: 'All Works' })).not.toBeInTheDocument();
  });

  it('exposes the Modules nav link under the Orchestrator tab with a valid route', async () => {
    const user = userEvent.setup();
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    await user.click(screen.getByRole('tab', { name: 'Orchestrator' }));
    const modulesLink = screen.getByRole('link', { name: 'Modules' });
    expect(modulesLink).toHaveAttribute('href', '/modules');
  });

  it('does not expose Connect or Daemon as top-level nav items', async () => {
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    expect(screen.queryByRole('link', { name: /Connect/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: /Daemon/i })).not.toBeInTheDocument();
  });

  it('wraps tabs in a tablist and exposes the nav groups as a tabpanel', async () => {
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    expect(screen.getByRole('tablist', { name: 'Primary navigation' })).toBeInTheDocument();
    expect(screen.getByRole('tabpanel')).toHaveAttribute('aria-labelledby', 'creator');
  });

  it('mounts the footer profile switcher', async () => {
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    await waitFor(() =>
      expect(screen.getByRole('toolbar', { name: 'Profiles' })).toBeInTheDocument(),
    );
  });

  it('exposes Settings as a footer utility link above profiles', async () => {
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    const link = screen.getByTestId('settings-footer-utility-link');
    expect(link).toHaveAttribute('href', '/settings');
    expect(link).toHaveTextContent('Settings');
    // Settings stays visible on Creator tab (not tab-scoped).
    expect(screen.getByRole('tab', { name: 'Creator', selected: true })).toBeInTheDocument();
  });

  it('renders localized labels when locale is zh-CN', async () => {
    window.localStorage.setItem('nexus-web-locale', 'zh-CN');
    const user = userEvent.setup();
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    expect(screen.getByRole('tab', { name: '创作', selected: true })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: '编排' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: '全部作品' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: '世界' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: '记忆' })).toBeInTheDocument();
    const link = screen.getByTestId('settings-footer-utility-link');
    expect(link).toHaveTextContent('设置');

    await user.click(screen.getByRole('tab', { name: '编排' }));
    expect(screen.getByRole('button', { name: '计算' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: '模块' })).toBeInTheDocument();
  });

  it('keeps parent groups as quiet labels and selected leaf with soft fill + thin bar', async () => {
    useSidebarHandlers();

    renderInApp(<Sidebar />, {
      client: makeClient(),
      activeCreatorId: 'creator-a',
      initialRouterEntries: ['/works'],
    });

    const worksGroup = screen.getByRole('button', { name: /Works/i });
    expect(worksGroup).toHaveClass('text-gray-600');
    expect(worksGroup).not.toHaveClass('bg-gray-alpha-100');

    const allWorks = screen.getByRole('link', { name: 'All Works' });
    expect(allWorks).toHaveClass('bg-gray-alpha-100', 'text-gray-1000');
    expect(allWorks.querySelector('[data-testid="sidebar-active-bar"]')).toHaveClass(
      'w-[2px]',
      'bg-blue-700',
    );

    const memories = screen.getByRole('link', { name: 'Memories' });
    expect(memories).toHaveClass('text-gray-600');
    expect(memories).not.toHaveClass('bg-gray-alpha-100');
  });

  it('highlights Memories on /memory via prefix match (V1.118 P1)', async () => {
    useSidebarHandlers();

    renderInApp(<Sidebar />, {
      client: makeClient(),
      activeCreatorId: 'creator-a',
      initialRouterEntries: ['/memory'],
    });

    const memories = screen.getByRole('link', { name: 'Memories' });
    expect(memories).toHaveClass('bg-gray-alpha-100', 'text-gray-1000');
    expect(memories.querySelector('[data-testid="sidebar-active-bar"]')).toHaveClass(
      'w-[2px]',
      'bg-blue-700',
    );

    const allWorks = screen.getByRole('link', { name: 'All Works' });
    expect(allWorks).not.toHaveClass('bg-gray-alpha-100');
  });

  it('shows three peer groups with no Creator meta-group mixing canvas (V1.118 P1)', async () => {
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    expect(screen.getByRole('button', { name: /Works/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Worlds/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Memories/i })).toBeInTheDocument();

    // No Creator meta-group label as a nav group (tab label "Creator" remains).
    expect(screen.queryByRole('button', { name: /^Creator$/i })).not.toBeInTheDocument();

    // Canvas surfaces are not list-mode Creation items anymore.
    expect(screen.queryByRole('link', { name: 'Outline' })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'World KB' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Canvas' })).not.toBeInTheDocument();

    expect(screen.getByRole('link', { name: 'Worlds' })).toHaveAttribute('href', '/worlds');
    expect(screen.getByRole('link', { name: 'Memories' })).toHaveAttribute('href', '/memory');
  });

  it('lists work rows from the Works query under the Works group', async () => {
    useSidebarHandlers([
      {
        work_id: 'work-alpha',
        title: 'Alpha Novel',
        status: 'active',
        intake_status: 'ready',
        primary_preset_id: 'preset-1',
        updated_at: '2026-01-01T00:00:00Z',
      },
    ]);

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    await waitFor(() =>
      expect(screen.getByRole('link', { name: 'Alpha Novel' })).toHaveAttribute(
        'href',
        '/works/work-alpha',
      ),
    );
  });

  it('nests Strategy under the Orchestration tab (AC-P2-4)', async () => {
    const user = userEvent.setup();
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    await user.click(screen.getByRole('tab', { name: 'Orchestrator' }));

    // AC-P2-4: Strategy lives under Orchestration as a plain /strategies link.
    expect(screen.getByRole('link', { name: 'Strategies' })).toHaveAttribute(
      'href',
      '/strategies',
    );
  });

});

describe('Sidebar — layout structure (AD-P2-2 T1)', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  it('propagates height from the aside through the nav wrapper to the chrome', async () => {
    useSidebarHandlers();
    renderInApp(<Sidebar />, {
      client: makeClient(),
      activeCreatorId: 'creator-a',
    });

    // The nav wrapper fills its flex parent (the aside) and can shrink so the
    // chrome's internal scroll regions resolve against a definite height.
    const nav = screen.getByRole('navigation');
    expect(nav).toHaveClass('flex-1');
    expect(nav).toHaveClass('min-h-0');

    // The chrome root fills the nav (h-full) and lays out as a flex column.
    const chromeRoot = nav.firstElementChild as HTMLElement;
    expect(chromeRoot).toHaveClass('h-full');
    expect(chromeRoot).toHaveClass('flex-col');
  });

  it('scrolls nav internally (tabpanel overflow-auto) while the footer block stays pinned', async () => {
    useSidebarHandlers();
    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    // The nav items live in a tabpanel that absorbs free space and scrolls.
    const tabpanel = screen.getByRole('tabpanel');
    expect(tabpanel).toHaveClass('overflow-auto');
    expect(tabpanel).toHaveClass('flex-1');

    // Settings is a footer utility — it sits OUTSIDE the scrolling tabpanel.
    const settingsLink = screen.getByTestId('settings-footer-utility-link');
    expect(tabpanel.contains(settingsLink)).toBe(false);

    // The footer block container has a single border-t separating it from nav.
    expect(settingsLink.parentElement).toHaveClass('border-t');
  });

  it('places Settings and Profiles in one bottom-aligned footer block', async () => {
    useSidebarHandlers();
    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    // Wait for the footer profile switcher to mount.
    await waitFor(() =>
      expect(screen.getByRole('toolbar', { name: 'Profiles' })).toBeInTheDocument(),
    );

    const settingsLink = screen.getByTestId('settings-footer-utility-link');
    const toolbar = screen.getByRole('toolbar', { name: 'Profiles' });
    const tabpanel = screen.getByRole('tabpanel');

    // Both Settings and Profiles are outside the scrolling nav region.
    expect(tabpanel.contains(settingsLink)).toBe(false);
    expect(tabpanel.contains(toolbar)).toBe(false);

    // Both share the same bottom block (the element with border-t that contains
    // the Settings link). The toolbar may be nested one level deeper inside the
    // FooterProfilesChrome wrapper, so verify common ancestry.
    const bottomBlock = settingsLink.parentElement;
    expect(bottomBlock).toHaveClass('border-t');
    expect(bottomBlock!.contains(toolbar)).toBe(true);
  });
});

describe('Sidebar — work drill-in skeleton (AD-P2-1)', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  // Mount Sidebar inside a layout route so `useParams` populates workId the way
  // RootLayout does in production. Without the matching route tree, workId
  // would be undefined and drill-in would never trigger.
  function renderSidebarAtRoute(initialPath: string) {
    useSidebarHandlers();
    renderInApp(
      <Routes>
        <Route element={<Sidebar />}>
          <Route path="works" element={null} />
          <Route path="works/:workId" element={null} />
          <Route path="works/:workId/outline" element={null} />
          <Route path="works/:workId/chapters" element={null} />
          <Route path="worlds" element={null} />
          <Route path="memory" element={null} />
        </Route>
      </Routes>,
      {
        client: makeClient(),
        activeCreatorId: 'creator-a',
        initialRouterEntries: [initialPath],
      },
    );
  }

  it('shows the three drill-in links and hides tabs when a workId is present', async () => {
    renderSidebarAtRoute('/works/work-42/outline');

    // AC-P2-6: the three skeleton links replace the top nav.
    expect(screen.getByRole('link', { name: 'Back to all' })).toHaveAttribute('href', '/works');
    expect(screen.getByRole('link', { name: 'Outline' })).toHaveAttribute(
      'href',
      '/works/work-42/outline',
    );
    expect(screen.getByRole('link', { name: 'Body' })).toHaveAttribute(
      'href',
      '/works/work-42/chapters',
    );

    // The Creator/Orchestrator tabs are hidden in drill-in mode.
    expect(screen.queryByRole('tab', { name: 'Creator' })).not.toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'Orchestrator' })).not.toBeInTheDocument();

    // Normal group items are gone (replaced by the skeleton).
    expect(screen.queryByRole('link', { name: 'All Works' })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Memories' })).not.toBeInTheDocument();
  });

  it('triggers drill-in on the work-detail route too (/works/:workId)', async () => {
    renderSidebarAtRoute('/works/work-42');

    expect(screen.getByRole('link', { name: 'Back to all' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Outline' })).toHaveAttribute(
      'href',
      '/works/work-42/outline',
    );
    expect(screen.getByRole('link', { name: 'Body' })).toHaveAttribute(
      'href',
      '/works/work-42/chapters',
    );
  });

  it('encodes a space-bearing workId in the drill-in targets', async () => {
    renderSidebarAtRoute('/works/w%204');

    expect(screen.getByRole('link', { name: 'Outline' })).toHaveAttribute(
      'href',
      '/works/w%204/outline',
    );
    expect(screen.getByRole('link', { name: 'Body' })).toHaveAttribute(
      'href',
      '/works/w%204/chapters',
    );
  });

  it('does NOT false-light Back to all while inside a work (host-owned aria-current)', async () => {
    renderSidebarAtRoute('/works/work-42/outline');

    const backToAll = screen.getByRole('link', { name: 'Back to all' });
    // Back to all is a "go back" action — never the current location inside a
    // work. It must not pick up an aria-current from react-router's prefix
    // detection (the reason drill-in links render via <Link>, not <NavLink>).
    expect(backToAll).not.toHaveAttribute('aria-current');
    expect(backToAll).not.toHaveClass('bg-gray-alpha-100');

    // The Outline surface link IS the current location.
    expect(screen.getByRole('link', { name: 'Outline' })).toHaveAttribute(
      'aria-current',
      'page',
    );
  });

  it('renders localized drill-in labels in zh-CN', async () => {
    window.localStorage.setItem('nexus-web-locale', 'zh-CN');
    renderSidebarAtRoute('/works/work-42/outline');

    expect(screen.getByRole('link', { name: '返回所有' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: '大纲' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: '正文' })).toBeInTheDocument();
  });

  it('keeps the Settings footer utility in drill-in mode', async () => {
    renderSidebarAtRoute('/works/work-42/outline');

    // The footer (Settings + profiles) is independent of drill-in mode.
    const link = screen.getByTestId('settings-footer-utility-link');
    expect(link).toHaveAttribute('href', '/settings');
    expect(link).toHaveTextContent('Settings');
  });

  it('does NOT enter drill-in on the /works list route (no workId)', async () => {
    renderSidebarAtRoute('/works');

    // Normal IA: tabs visible, All Works present, no drill-in Back-to-all link.
    expect(screen.getByRole('tab', { name: 'Creator' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'All Works' })).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Back to all' })).not.toBeInTheDocument();
  });

  it('does NOT enter drill-in on a world route (worldId, not workId)', async () => {
    renderSidebarAtRoute('/worlds');

    expect(screen.getByRole('tab', { name: 'Creator' })).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Back to all' })).not.toBeInTheDocument();
  });
});
