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
    expect(screen.queryByRole('link', { name: 'Memories' })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Memory' })).not.toBeInTheDocument();
  });

  it('swaps to Orchestrator tab and shows runtime/strategy links', async () => {
    const user = userEvent.setup();
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    await user.click(screen.getByRole('tab', { name: 'Orchestrator' }));

    expect(screen.getByRole('tab', { name: 'Orchestrator', selected: true })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Memory' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Strategies' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Sessions' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Schedule' })).toBeInTheDocument();
    // AC-P2-2: Capabilities is soft-removed from the Orchestration sidebar.
    expect(screen.queryByRole('link', { name: 'Capabilities' })).not.toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Modules' })).toBeInTheDocument();

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

  // V1.125 P2 — Timeline peer groups removed from Creator sidebar; routes
  // remain deep-linkable via command palette and in-surface navigation.
  it('does not expose Timeline or Work Timelines in the Creator sidebar (V1.125 P2)', async () => {
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
      expect(screen.getByRole('link', { name: 'Alpha Novel' })).toBeInTheDocument(),
    );

    expect(screen.queryByRole('link', { name: 'Timeline' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Work Timelines/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Alpha Novel Timeline' })).not.toBeInTheDocument();
  });

  it('orders Creator groups Worlds before Works (V1.125 P2)', async () => {
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    const groupButtons = screen
      .getAllByRole('button')
      .filter((el) => ['Worlds', 'Works'].includes(el.textContent ?? ''));
    expect(groupButtons.map((el) => el.textContent)).toEqual(['Worlds', 'Works']);
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

  it('does not expose a Settings row in the sidebar footer (V1.125 P2)', async () => {
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    expect(screen.queryByTestId('settings-footer-utility-link')).not.toBeInTheDocument();
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
    expect(screen.queryByRole('link', { name: '记忆' })).not.toBeInTheDocument();

    await user.click(screen.getByRole('tab', { name: '编排' }));
    expect(screen.getByRole('link', { name: '记忆' })).toBeInTheDocument();
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

    const worksGroup = screen.getByRole('button', { name: 'Works' });
    expect(worksGroup).toHaveClass('text-gray-600');
    expect(worksGroup).not.toHaveClass('bg-gray-alpha-100');

    const allWorks = screen.getByRole('link', { name: 'All Works' });
    expect(allWorks).toHaveClass('bg-gray-alpha-100', 'text-gray-1000');
    expect(allWorks.querySelector('[data-testid="sidebar-active-bar"]')).toHaveClass(
      'w-[2px]',
      'bg-blue-700',
    );

    const memories = screen.queryByRole('link', { name: 'Memories' });
    expect(memories).not.toBeInTheDocument();
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

    const worksGroup = screen.getByRole('button', { name: 'Works' });
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

  it('selects Orchestrator tab and highlights Memory on /memory (V1.125 P1)', async () => {
    useSidebarHandlers();

    renderInApp(<Sidebar />, {
      client: makeClient(),
      activeCreatorId: 'creator-a',
      initialRouterEntries: ['/memory'],
    });

    expect(screen.getByRole('tab', { name: 'Orchestrator', selected: true })).toBeInTheDocument();
    const memory = screen.getByRole('link', { name: 'Memory' });
    expect(memory).toHaveAttribute('href', '/memory');
    expect(memory).toHaveClass('bg-gray-alpha-100', 'text-gray-1000');
    expect(memory.querySelector('[data-testid="sidebar-active-bar"]')).toHaveClass(
      'w-[2px]',
      'bg-blue-700',
    );
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

  it('shows Creator peer groups without Memories (V1.125 P1)', async () => {
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    expect(screen.getByRole('button', { name: 'Worlds' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Works' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Memories/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Memories' })).not.toBeInTheDocument();

    // No Creator meta-group label as a nav group (tab label "Creator" remains).
    expect(screen.queryByRole('button', { name: /^Creator$/i })).not.toBeInTheDocument();

    // Canvas surfaces are not list-mode Creation items anymore.
    expect(screen.queryByRole('link', { name: 'Outline' })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'World KB' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Canvas' })).not.toBeInTheDocument();

    expect(screen.getByRole('link', { name: 'Worlds' })).toHaveAttribute('href', '/worlds');
  });

  it('orders Orchestrator groups Memory → Strategies → Runtime → Compute (V1.125 P1)', async () => {
    const user = userEvent.setup();
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });
    await user.click(screen.getByRole('tab', { name: 'Orchestrator' }));

    const groupButtons = screen
      .getAllByRole('button')
      .filter((el) => ['Memory', 'Strategies', 'Runtime', 'Compute'].includes(el.textContent ?? ''));
    expect(groupButtons.map((el) => el.textContent)).toEqual([
      'Memory',
      'Strategies',
      'Runtime',
      'Compute',
    ]);
  });

  it('selects Orchestrator tab on /strategies deep link (V1.125 P1)', async () => {
    useSidebarHandlers();

    renderInApp(<Sidebar />, {
      client: makeClient(),
      activeCreatorId: 'creator-a',
      initialRouterEntries: ['/strategies/user%2Ffoo'],
    });

    expect(screen.getByRole('tab', { name: 'Orchestrator', selected: true })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Strategies' })).toHaveClass('bg-gray-alpha-100');
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

    await waitFor(() =>
      expect(screen.getByRole('toolbar', { name: 'Profiles' })).toBeInTheDocument(),
    );

    const toolbar = screen.getByRole('toolbar', { name: 'Profiles' });
    expect(tabpanel.contains(toolbar)).toBe(false);
    expect(toolbar.closest('.border-t')).not.toBeNull();
  });

  it('places Profiles in a bottom-aligned footer block', async () => {
    useSidebarHandlers();
    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    // Wait for the footer profile switcher to mount.
    await waitFor(() =>
      expect(screen.getByRole('toolbar', { name: 'Profiles' })).toBeInTheDocument(),
    );

    const toolbar = screen.getByRole('toolbar', { name: 'Profiles' });
    const tabpanel = screen.getByRole('tabpanel');

    expect(tabpanel.contains(toolbar)).toBe(false);
    expect(toolbar.closest('.border-t')).not.toBeNull();
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
    expect(screen.getByRole('button', { name: 'Works' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Worlds' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Memories/i })).not.toBeInTheDocument();
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

describe('Sidebar — Work Timeline routes (V1.125 P2)', () => {
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
          <Route path="works/:workId/outline" element={null} />
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

  it('highlights the Work row on /works/:id/timeline (no Timeline sidebar entry)', async () => {
    renderSidebarAtRoute('/works/work-42/timeline');

    const outlineLink = await waitFor(() =>
      screen.getByRole('link', { name: 'Drill Novel' }),
    );
    expect(outlineLink).toHaveClass('bg-gray-alpha-100', 'text-gray-1000');
    expect(outlineLink.querySelector('[data-testid="sidebar-active-bar"]')).toHaveClass(
      'w-[2px]',
      'bg-blue-700',
    );
    expect(screen.queryByRole('link', { name: 'Drill Novel Timeline' })).not.toBeInTheDocument();
  });
});

describe('Sidebar — submenu trigger (V1.126 P0 T1)', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  function renderSidebarWithWorks() {
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
  }

  it('renders a ••• button on World and Work rows (hover-visible)', async () => {
    renderSidebarWithWorks();

    await waitFor(() =>
      expect(screen.getByRole('link', { name: 'Alpha Novel' })).toBeInTheDocument(),
    );

    const worldsBtn = screen.getByRole('button', { name: /Open menu for Worlds/i });
    const alphaBtn = screen.getByRole('button', { name: /Open menu for Alpha Novel/i });
    expect(worldsBtn).toBeInTheDocument();
    expect(alphaBtn).toBeInTheDocument();
  });

  it('does not render ••• button on Orchestrator rows', async () => {
    const user = userEvent.setup();
    useSidebarHandlers();
    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    await user.click(screen.getByRole('tab', { name: 'Orchestrator' }));

    expect(screen.queryByRole('button', { name: /Open menu for Memory/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Open menu for Strategies/i })).not.toBeInTheDocument();
  });

  it('opens submenu on ••• button click', async () => {
    const user = userEvent.setup();
    renderSidebarWithWorks();

    await waitFor(() =>
      expect(screen.getByRole('link', { name: 'Alpha Novel' })).toBeInTheDocument(),
    );

    const menuBtn = screen.getByRole('button', { name: /Open menu for Alpha Novel/i });
    await user.click(menuBtn);

    await waitFor(() =>
      expect(screen.getByRole('menu', { name: 'Row actions' })).toBeInTheDocument(),
    );
  });

  it('opens submenu on Enter key when row is focused', async () => {
    const user = userEvent.setup();
    renderSidebarWithWorks();

    await waitFor(() =>
      expect(screen.getByRole('link', { name: 'Alpha Novel' })).toBeInTheDocument(),
    );

    const worldsLink = screen.getByRole('link', { name: 'Worlds' });
    worldsLink.focus();
    await user.keyboard('{Enter}');

    await waitFor(() =>
      expect(screen.getByRole('menu', { name: 'Row actions' })).toBeInTheDocument(),
    );
  });

  it('opens submenu on Ctrl+. / Cmd+. when row is focused', async () => {
    const user = userEvent.setup();
    renderSidebarWithWorks();

    await waitFor(() =>
      expect(screen.getByRole('link', { name: 'Alpha Novel' })).toBeInTheDocument(),
    );

    const worldsLink = screen.getByRole('link', { name: 'Worlds' });
    worldsLink.focus();
    await user.keyboard('{Control>}.{/Control}');

    await waitFor(() =>
      expect(screen.getByRole('menu', { name: 'Row actions' })).toBeInTheDocument(),
    );
  });

  it('closes submenu on Escape', async () => {
    const user = userEvent.setup();
    renderSidebarWithWorks();

    await waitFor(() =>
      expect(screen.getByRole('link', { name: 'Alpha Novel' })).toBeInTheDocument(),
    );

    const menuBtn = screen.getByRole('button', { name: /Open menu for Alpha Novel/i });
    await user.click(menuBtn);
    await waitFor(() =>
      expect(screen.getByRole('menu', { name: 'Row actions' })).toBeInTheDocument(),
    );

    await user.keyboard('{Escape}');
    await waitFor(() =>
      expect(screen.queryByRole('menu', { name: 'Row actions' })).not.toBeInTheDocument(),
    );
  });

  it('click on row body still navigates (existing behavior preserved)', async () => {
    const user = userEvent.setup();
    useSidebarHandlers();
    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    const worldsLink = screen.getByRole('link', { name: 'Worlds' });
    expect(worldsLink).toHaveAttribute('href', '/worlds');

    await user.click(worldsLink);

    expect(screen.queryByRole('menu', { name: 'Row actions' })).not.toBeInTheDocument();
  });

  it('does not render submenu on Orchestrator tab rows', async () => {
    const user = userEvent.setup();
    useSidebarHandlers();
    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    await user.click(screen.getByRole('tab', { name: 'Orchestrator' }));

    const memoryLink = screen.getByRole('link', { name: 'Memory' });
    memoryLink.focus();
    await user.keyboard('{Enter}');

    expect(screen.queryByRole('menu', { name: 'Row actions' })).not.toBeInTheDocument();
  });
});
