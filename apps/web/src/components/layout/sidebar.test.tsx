import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Route, Routes } from 'react-router-dom';

import { Sidebar } from './sidebar';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';

vi.mock('@/components/brand/nexus-logo', () => ({
  NexusLogo: () => <div data-testid="nexus-logo">Nexus</div>,
}));

function makeClient() {
  return new BrowserClient();
}

/** The creators-list handler the footer profile switcher fetches on mount. */
function useCreatorHandler() {
  useHandlers(
    http.get('/v1/daemon/creators', () =>
      HttpResponse.json({
        items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
        pagination: { limit: 20, has_more: false },
      }),
    ),
  );
}

describe('Sidebar', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  it('renders the Creator tab by default', async () => {
    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    expect(screen.getByRole('tab', { name: 'Creator', selected: true })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'All Works' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Memory' })).toBeInTheDocument();
  });

  it('swaps to Orchestrator tab and shows runtime/strategy links', async () => {
    const user = userEvent.setup();
    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

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
    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    await user.click(screen.getByRole('tab', { name: 'Orchestrator' }));
    const modulesLink = screen.getByRole('link', { name: 'Modules' });
    expect(modulesLink).toHaveAttribute('href', '/modules');
  });

  it('does not expose Connect or Daemon as top-level nav items', async () => {
    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    expect(screen.queryByRole('link', { name: /Connect/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: /Daemon/i })).not.toBeInTheDocument();
  });

  it('wraps tabs in a tablist and exposes the nav groups as a tabpanel', async () => {
    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    expect(screen.getByRole('tablist', { name: 'Primary navigation' })).toBeInTheDocument();
    expect(screen.getByRole('tabpanel')).toHaveAttribute('aria-labelledby', 'creator');
  });

  it('mounts the footer profile switcher', async () => {
    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    await waitFor(() =>
      expect(screen.getByRole('toolbar', { name: 'Profiles' })).toBeInTheDocument(),
    );
  });

  it('exposes Settings as a footer utility link above profiles', async () => {
    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

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
    useCreatorHandler();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    expect(screen.getByRole('tab', { name: '创作', selected: true })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: '编排' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: '全部作品' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: '记忆' })).toBeInTheDocument();
    const link = screen.getByTestId('settings-footer-utility-link');
    expect(link).toHaveTextContent('设置');

    await user.click(screen.getByRole('tab', { name: '编排' }));
    expect(screen.getByRole('button', { name: '计算' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: '模块' })).toBeInTheDocument();
  });

  it('keeps parent groups as quiet labels and selected leaf with soft fill + thin bar', async () => {
    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

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

    const memory = screen.getByRole('link', { name: 'Memory' });
    expect(memory).toHaveClass('text-gray-600');
    expect(memory).not.toHaveClass('bg-gray-alpha-100');
  });

  it('nests the Canvas group (Outline / World KB / Strategy) under the Creator tab', async () => {
    useCreatorHandler();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    // The Canvas group is a disclosure (collapsible), open by default so its
    // three items are visible without an extra click.
    const canvasDisclosure = screen.getByRole('button', { name: 'Canvas' });
    expect(canvasDisclosure).toBeInTheDocument();
    expect(canvasDisclosure).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByRole('link', { name: 'Outline' })).toBeInTheDocument();
    // World KB has no worldId at the default route, so it falls back to the
    // `/worlds` picker (V1.115 T3) — a focusable link, never disabled.
    expect(screen.getByRole('link', { name: 'World KB' })).toHaveAttribute('href', '/worlds');
    expect(screen.getByRole('link', { name: 'Strategy' })).toBeInTheDocument();
  });

  it('highlights the Outline canvas surface on /works/:id/outline (resolver-driven)', async () => {
    useCreatorHandler();

    renderInApp(<Sidebar />, {
      client: makeClient(),
      activeCreatorId: 'creator-a',
      initialRouterEntries: ['/works/work-1/outline'],
    });

    const outline = screen.getByRole('link', { name: 'Outline' });
    expect(outline).toHaveClass('bg-gray-alpha-100', 'text-gray-1000');
    expect(outline).toHaveAttribute('aria-current', 'page');
    expect(outline.querySelector('[data-testid="sidebar-active-bar"]')).toHaveClass(
      'bg-blue-700',
    );
    // Non-outline canvas surfaces stay inactive on the outline route. World KB
    // has no worldId here and falls back to the `/worlds` picker (a link, but
    // inactive); Strategy stays a link but inactive.
    expect(screen.getByRole('link', { name: 'World KB' })).not.toHaveClass('bg-gray-alpha-100');
    expect(screen.getByRole('link', { name: 'Strategy' })).not.toHaveClass('bg-gray-alpha-100');
  });

  it('does NOT highlight Outline on plain /works/:id — resolver null suppresses the chrome prefix match', async () => {
    // The chrome's built-in `item.to` prefix match would light "Outline"
    // (`to: '/works'`) on `/works/:id`; the resolver returns null here, so the
    // canvas item must render inactive.
    useCreatorHandler();

    renderInApp(<Sidebar />, {
      client: makeClient(),
      activeCreatorId: 'creator-a',
      initialRouterEntries: ['/works/work-1'],
    });

    const outline = screen.getByRole('link', { name: 'Outline' });
    expect(outline).not.toHaveClass('bg-gray-alpha-100');
    expect(outline).not.toHaveAttribute('aria-current', 'page');
    expect(outline.querySelector('[data-testid="sidebar-active-bar"]')).toBeNull();
    // Non-canvas "All Works" (`to: '/works'`) keeps its chrome prefix-match
    // highlight — unchanged V1.94 behavior.
    expect(screen.getByRole('link', { name: 'All Works' })).toHaveClass('bg-gray-alpha-100');
  });
});

describe('Sidebar — Canvas nav target wiring (V1.111 P1 T3)', () => {
  // Sidebar reads workId/worldId via `useParams`, which only populates when a
  // route tree matches (as in production under RootLayout). renderInApp
  // provides a MemoryRouter but no routes, so these tests mount Sidebar inside
  // a layout route mirroring RootLayout, with child routes carrying the params.
  function renderSidebarAtRoute(initialPath: string) {
    useCreatorHandler();
    renderInApp(
      <Routes>
        <Route element={<Sidebar />}>
          <Route path="works" element={null} />
          <Route path="works/:workId" element={null} />
          <Route path="works/:workId/outline" element={null} />
          <Route path="worlds" element={null} />
          <Route path="worlds/:worldId/kb" element={null} />
          <Route path="strategies" element={null} />
          <Route path="sessions" element={null} />
        </Route>
      </Routes>,
      {
        client: makeClient(),
        activeCreatorId: 'creator-a',
        initialRouterEntries: [initialPath],
      },
    );
  }

  it('Outline navigates to the work-scoped surface when a workId is in the URL', async () => {
    renderSidebarAtRoute('/works/work-42');

    expect(screen.getByRole('link', { name: 'Outline' })).toHaveAttribute(
      'href',
      '/works/work-42/outline',
    );
  });

  it('Outline falls back to the /works picker when no workId is in the URL', async () => {
    renderSidebarAtRoute('/sessions');

    expect(screen.getByRole('link', { name: 'Outline' })).toHaveAttribute('href', '/works');
  });

  it('Outline encodes a space-bearing workId in the href', async () => {
    renderSidebarAtRoute('/works/w%204');

    expect(screen.getByRole('link', { name: 'Outline' })).toHaveAttribute(
      'href',
      '/works/w%204/outline',
    );
  });

  it('World KB navigates to the world-scoped surface when a worldId is in the URL', async () => {
    renderSidebarAtRoute('/worlds/world-9/kb');

    expect(screen.getByRole('link', { name: 'World KB' })).toHaveAttribute(
      'href',
      '/worlds/world-9/kb',
    );
  });

  it('World KB navigates to the /worlds picker when no worldId is in the URL', async () => {
    // V1.115 T3: the `/worlds` picker route exists, so without a worldId the
    // item falls back to it — a focusable link, not an aria-disabled span.
    renderSidebarAtRoute('/works/work-1');

    expect(screen.getByRole('link', { name: 'World KB' })).toHaveAttribute('href', '/worlds');
  });

  it('Strategy always navigates to /strategies regardless of context', async () => {
    renderSidebarAtRoute('/works/work-1');

    expect(screen.getByRole('link', { name: 'Strategy' })).toHaveAttribute('href', '/strategies');
  });
});

describe('Sidebar — Canvas active-surface highlight (V1.111 P1 T4)', () => {
  // Like the T3 block, these tests mount Sidebar inside a layout route so
  // `useParams` populates workId/worldId the way RootLayout does in production.
  // Without the matching route tree, World KB on `/worlds/:worldId/kb` would
  // render disabled (no worldId) even though the resolver marks it active —
  // a state that cannot occur in production. The route helper keeps the
  // highlight assertions faithful to real behavior.
  function renderSidebarAtRoute(initialPath: string) {
    useCreatorHandler();
    renderInApp(
      <Routes>
        <Route element={<Sidebar />}>
          <Route path="works" element={null} />
          <Route path="works/:workId" element={null} />
          <Route path="works/:workId/outline" element={null} />
          <Route path="worlds" element={null} />
          <Route path="worlds/:worldId/kb" element={null} />
          <Route path="strategies" element={null} />
          <Route path="strategies/:presetId" element={null} />
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

  it('highlights the World KB canvas surface on /worlds/:worldId/kb (resolver-driven)', async () => {
    renderSidebarAtRoute('/worlds/world-9/kb');

    const worldKb = screen.getByRole('link', { name: 'World KB' });
    expect(worldKb).toHaveClass('bg-gray-alpha-100', 'text-gray-1000');
    expect(worldKb).toHaveAttribute('aria-current', 'page');
    expect(worldKb.querySelector('[data-testid="sidebar-active-bar"]')).toHaveClass(
      'bg-blue-700',
    );
    // Non-World-KB canvas surfaces stay inactive. Outline renders as a link
    // (no workId → /works fallback); Strategy stays a link but inactive.
    expect(screen.getByRole('link', { name: 'Outline' })).not.toHaveClass('bg-gray-alpha-100');
    expect(screen.getByRole('link', { name: 'Strategy' })).not.toHaveClass('bg-gray-alpha-100');
  });

  it('highlights the Strategy canvas surface on /strategies/:presetId (resolver-driven)', async () => {
    renderSidebarAtRoute('/strategies/preset-1');

    const strategy = screen.getByRole('link', { name: 'Strategy' });
    expect(strategy).toHaveClass('bg-gray-alpha-100', 'text-gray-1000');
    expect(strategy).toHaveAttribute('aria-current', 'page');
    expect(strategy.querySelector('[data-testid="sidebar-active-bar"]')).toHaveClass(
      'bg-blue-700',
    );
    // Non-strategy canvas surfaces stay inactive. Outline renders as a link
    // (no workId → /works fallback); World KB has no worldId here and falls
    // back to the `/worlds` picker (a link, but inactive).
    expect(screen.getByRole('link', { name: 'Outline' })).not.toHaveClass('bg-gray-alpha-100');
    expect(screen.getByRole('link', { name: 'World KB' })).not.toHaveClass('bg-gray-alpha-100');
  });

  it('keeps all Canvas items inactive on a non-canvas Creator-tab route (/memory)', async () => {
    // On a non-canvas route the resolver returns null, so every Canvas item
    // must render inactive. This is the clean no-canvas baseline (distinct
    // from the /works/:id case, which specifically probes the resolver-vs-
    // prefix-match conflict for Outline).
    renderSidebarAtRoute('/memory');

    // Outline + Strategy render as inactive links (both have valid fallback
    // targets even without context). Neither lights up.
    const outline = screen.getByRole('link', { name: 'Outline' });
    expect(outline).not.toHaveClass('bg-gray-alpha-100');
    expect(outline).not.toHaveAttribute('aria-current', 'page');
    expect(outline.querySelector('[data-testid="sidebar-active-bar"]')).toBeNull();

    const strategy = screen.getByRole('link', { name: 'Strategy' });
    expect(strategy).not.toHaveClass('bg-gray-alpha-100');
    expect(strategy).not.toHaveAttribute('aria-current', 'page');
    expect(strategy.querySelector('[data-testid="sidebar-active-bar"]')).toBeNull();

    // World KB has no worldId and falls back to the `/worlds` picker — a
    // focusable link, inactive like the other Canvas items here.
    const worldKb = screen.getByRole('link', { name: 'World KB' });
    expect(worldKb).not.toHaveClass('bg-gray-alpha-100');
    expect(worldKb).not.toHaveAttribute('aria-current', 'page');

    // Contrast: the non-canvas "Memory" item (to: /memory) DOES highlight via
    // the chrome's prefix match — V1.94 behavior preserved for non-canvas
    // items even while every Canvas item stays quiet.
    expect(screen.getByRole('link', { name: 'Memory' })).toHaveClass('bg-gray-alpha-100');
  });
});

describe('Sidebar — layout structure (AD-P2-2 T1)', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  it('propagates height from the aside through the nav wrapper to the chrome', async () => {
    useCreatorHandler();
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
    useCreatorHandler();
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
    useCreatorHandler();
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
