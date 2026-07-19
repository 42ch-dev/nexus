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
    // AC-P2-2: Capabilities is soft-removed from the Orchestration sidebar.
    expect(screen.queryByRole('link', { name: 'Capabilities' })).not.toBeInTheDocument();
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

  // V1.123 P3 Task 1 — Timeline primary-nav entry. The global Timeline
  // view (`/timeline`) is reachable from the Creator tab as a peer to Works
  // / Worlds / Memories. Pinning it FIRST gives the central instrument
  // structural prominence per `three-layer-product-spec.md`.
  it('exposes the Timeline nav link under the Creator tab with a valid route (V1.123 P3 T1)', async () => {
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    const timelineLink = screen.getByRole('link', { name: 'Timeline' });
    expect(timelineLink).toHaveAttribute('href', '/timeline');
    // The link is on the Creator tab (selected by default).
    expect(screen.getByRole('tab', { name: 'Creator', selected: true })).toBeInTheDocument();
  });

  it('renders localized Timeline label when locale is zh-CN (V1.123 P3 T1)', async () => {
    window.localStorage.setItem('nexus-web-locale', 'zh-CN');
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    expect(screen.getByRole('link', { name: '时间线' })).toHaveAttribute('href', '/timeline');
  });

  // V1.123 P5 (PR #157 Greptile fix) — per-Work Timeline discoverability.
  // `/works/:id/timeline` was reachable only via the Canvas shell ⌘K palette
  // (`go.work-timeline`); the primary sidebar now mirrors each per-Work
  // Outline row with a Timeline entry so authors can switch surfaces without
  // ⌘K. Outline remains the per-Work default (V1.118); the Work Timelines
  // group sits AFTER Works so Timeline stays discoverable but secondary.
  it('exposes a per-Work Timeline link in a Work Timelines sidebar group (V1.123 P5)', async () => {
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

    // Wait for the Works query to resolve so the conditional Work Timelines
    // group mounts alongside the per-Work Outline rows.
    await waitFor(() =>
      expect(screen.getByRole('button', { name: /Work Timelines/i })).toBeInTheDocument(),
    );
    // The per-Work Timeline entry links to the timeline route (not outline).
    expect(screen.getByRole('link', { name: 'Alpha Novel Timeline' })).toHaveAttribute(
      'href',
      '/works/work-alpha/timeline',
    );
  });

  it('keeps Outline as the per-Work default route alongside the new Timeline entry (V1.123 P5)', async () => {
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

    // Outline entry still uses the bare work title (per-Work default — V1.118).
    await waitFor(() =>
      expect(screen.getByRole('link', { name: 'Alpha Novel' })).toHaveAttribute(
        'href',
        '/works/work-alpha/outline',
      ),
    );
  });

  it('does not render the Work Timelines group when no Works exist (V1.123 P5)', async () => {
    useSidebarHandlers([]);

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    // An empty group would be sidebar noise — suppress it entirely.
    expect(screen.queryByRole('button', { name: /Work Timelines/i })).not.toBeInTheDocument();
  });

  it('renders localized Work Timelines labels when locale is zh-CN (V1.123 P5)', async () => {
    window.localStorage.setItem('nexus-web-locale', 'zh-CN');
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
      expect(screen.getByRole('button', { name: /作品时间线/ })).toBeInTheDocument(),
    );
    expect(screen.getByRole('link', { name: 'Alpha Novel 时间线' })).toHaveAttribute(
      'href',
      '/works/work-alpha/timeline',
    );
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

  it('group disclosure transitions at duration-state with a rotating chevron (V1.121 P2 T1)', async () => {
    const user = userEvent.setup();
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

    // Wait for the async Works query so the group has >1 item (chevron only
    // renders for multi-item groups).
    await waitFor(() =>
      expect(screen.getByRole('link', { name: 'Alpha Novel' })).toBeInTheDocument(),
    );

    const worksGroup = screen.getByRole('button', { name: /Works/i });
    expect(worksGroup).toHaveAttribute('aria-expanded', 'true');
    expect(worksGroup.className).toMatch(/\bduration-state\b/);
    expect(worksGroup.className).toMatch(/\bmotion-reduce:transition-none\b/);

    // Multi-item group renders the disclosure chevron; open = rotated 90°.
    // (SVG className is an SVGAnimatedString — assert via the class attribute.)
    let chevron = worksGroup.querySelector('svg');
    expect(chevron).not.toBeNull();
    expect(chevron!.getAttribute('class')).toMatch(/\btransition-transform\b/);
    expect(chevron!.getAttribute('class')).toMatch(/\brotate-90\b/);

    // Collapse: chevron rotation removed (120ms state transition, ARIA unchanged).
    await user.click(worksGroup);
    expect(worksGroup).toHaveAttribute('aria-expanded', 'false');
    chevron = worksGroup.querySelector('svg');
    expect(chevron!.getAttribute('class')).not.toMatch(/\brotate-90\b/);
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

  it('highlights Worlds on /worlds via prefix match (V1.118 P1)', async () => {
    useSidebarHandlers();

    renderInApp(<Sidebar />, {
      client: makeClient(),
      activeCreatorId: 'creator-a',
      initialRouterEntries: ['/worlds'],
    });

    const worlds = screen.getByRole('link', { name: 'Worlds' });
    expect(worlds).toHaveClass('bg-gray-alpha-100', 'text-gray-1000');
    expect(worlds.querySelector('[data-testid="sidebar-active-bar"]')).toHaveClass(
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
        '/works/work-alpha/outline',
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

describe('Sidebar — work routes (V1.118 P2)', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  function renderSidebarAtRoute(initialPath: string) {
    useSidebarHandlers([
      {
        work_id: 'work-42',
        title: 'Drill Novel',
        status: 'active',
        intake_status: 'ready',
        primary_preset_id: 'preset-1',
        updated_at: '2026-01-01T00:00:00Z',
      },
    ]);
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

  it('keeps Creator | Orchestrator tabs and peer groups inside a work (AC-P2-5)', async () => {
    renderSidebarAtRoute('/works/work-42/outline');

    expect(screen.getByRole('tab', { name: 'Creator', selected: true })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Orchestrator' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Works/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Worlds/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Memories/i })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'All Works' })).toBeInTheDocument();

    await waitFor(() =>
      expect(screen.getByRole('link', { name: 'Drill Novel' })).toHaveAttribute(
        'href',
        '/works/work-42/outline',
      ),
    );

    expect(screen.queryByRole('link', { name: 'Back to all' })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Outline' })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Body' })).not.toBeInTheDocument();
  });

  it('does not enter drill-in on the /works list route', async () => {
    renderSidebarAtRoute('/works');

    expect(screen.getByRole('tab', { name: 'Creator' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'All Works' })).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Back to all' })).not.toBeInTheDocument();
  });

  it('highlights the active work row via prefix match on outline', async () => {
    renderSidebarAtRoute('/works/work-42/outline');

    const workLink = await waitFor(() => screen.getByRole('link', { name: 'Drill Novel' }));
    expect(workLink).toHaveClass('bg-gray-alpha-100', 'text-gray-1000');
    expect(workLink.querySelector('[data-testid="sidebar-active-bar"]')).toHaveClass(
      'w-[2px]',
      'bg-blue-700',
    );
  });

  it('does not highlight All Works on a work detail route', async () => {
    renderSidebarAtRoute('/works/work-42/outline');

    const allWorks = screen.getByRole('link', { name: 'All Works' });
    expect(allWorks).not.toHaveClass('bg-gray-alpha-100');
    expect(allWorks.querySelector('[data-testid="sidebar-active-bar"]')).toBeNull();
  });

  it('highlights the active work row on non-outline work sub-routes', async () => {
    renderSidebarAtRoute('/works/work-42/chapters');

    const workLink = await waitFor(() => screen.getByRole('link', { name: 'Drill Novel' }));
    expect(workLink).toHaveClass('bg-gray-alpha-100', 'text-gray-1000');
    expect(workLink.querySelector('[data-testid="sidebar-active-bar"]')).toHaveClass(
      'w-[2px]',
      'bg-blue-700',
    );
  });

  it('does not highlight All Works on a non-outline work sub-route', async () => {
    renderSidebarAtRoute('/works/work-42/chapters');

    const allWorks = screen.getByRole('link', { name: 'All Works' });
    expect(allWorks).not.toHaveClass('bg-gray-alpha-100');
    expect(allWorks.querySelector('[data-testid="sidebar-active-bar"]')).toBeNull();
  });
});

describe('Sidebar — Work Timeline active state (V1.123 P5)', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  /**
   * Per-Work Timeline surfaces (`/works/:id/timeline`, V1.123 P2 T5) now have
   * a peer sidebar entry. The active-state contract is:
   *   - `/works/:id/timeline` → Timeline row active, Outline row inactive.
   *   - `/works/:id/outline`  → Outline row active, Timeline row inactive.
   *   - `/works/:id/chapters` → Outline row still active (Outline owns sibling
   *     surfaces), Timeline row inactive.
   * This avoids double-highlight and gives the author an unambiguous signal
   * of which Work surface they are on.
   */
  function renderSidebarAtRoute(initialPath: string) {
    useSidebarHandlers([
      {
        work_id: 'work-42',
        title: 'Drill Novel',
        status: 'active',
        intake_status: 'ready',
        primary_preset_id: 'preset-1',
        updated_at: '2026-01-01T00:00:00Z',
      },
    ]);
    renderInApp(
      <Routes>
        <Route element={<Sidebar />}>
          <Route path="works/:workId/outline" element={null} />
          <Route path="works/:workId/chapters" element={null} />
          <Route path="works/:workId/timeline" element={null} />
        </Route>
      </Routes>,
      {
        client: makeClient(),
        activeCreatorId: 'creator-a',
        initialRouterEntries: [initialPath],
      },
    );
  }

  it('highlights the Timeline row on /works/:id/timeline and NOT the Outline row', async () => {
    renderSidebarAtRoute('/works/work-42/timeline');

    const timelineLink = await waitFor(() =>
      screen.getByRole('link', { name: 'Drill Novel Timeline' }),
    );
    expect(timelineLink).toHaveClass('bg-gray-alpha-100', 'text-gray-1000');
    expect(timelineLink.querySelector('[data-testid="sidebar-active-bar"]')).toHaveClass(
      'w-[2px]',
      'bg-blue-700',
    );

    // Outline row is no longer claiming /timeline (V1.123 P5 refinement).
    const outlineLink = screen.getByRole('link', { name: 'Drill Novel' });
    expect(outlineLink).not.toHaveClass('bg-gray-alpha-100');
    expect(outlineLink.querySelector('[data-testid="sidebar-active-bar"]')).toBeNull();
  });

  it('keeps Outline highlighted on /works/:id/outline and leaves Timeline inactive', async () => {
    renderSidebarAtRoute('/works/work-42/outline');

    const outlineLink = await waitFor(() =>
      screen.getByRole('link', { name: 'Drill Novel' }),
    );
    expect(outlineLink).toHaveClass('bg-gray-alpha-100', 'text-gray-1000');

    const timelineLink = screen.getByRole('link', { name: 'Drill Novel Timeline' });
    expect(timelineLink).not.toHaveClass('bg-gray-alpha-100');
    expect(timelineLink.querySelector('[data-testid="sidebar-active-bar"]')).toBeNull();
  });

  it('keeps Outline highlighted on sibling surfaces like /works/:id/chapters', async () => {
    renderSidebarAtRoute('/works/work-42/chapters');

    const outlineLink = await waitFor(() =>
      screen.getByRole('link', { name: 'Drill Novel' }),
    );
    expect(outlineLink).toHaveClass('bg-gray-alpha-100', 'text-gray-1000');

    const timelineLink = screen.getByRole('link', { name: 'Drill Novel Timeline' });
    expect(timelineLink).not.toHaveClass('bg-gray-alpha-100');
  });
});
