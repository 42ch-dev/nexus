import { http, HttpResponse } from 'msw';
import { describe, expect, it, beforeEach, vi } from 'vitest';
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

function useSidebarHandlers(works: unknown[] = []) {
  useHandlers(
    http.get('/v1/daemon/creators', () =>
      HttpResponse.json({
        items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
        pagination: { limit: 20, has_more: false },
      }),
    ),
    worksList(works),
    http.get('/v1/daemon/narrative/worlds', () => HttpResponse.json({ worlds: [] })),
  );
}

describe('Sidebar', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  it('renders the Creator tab with Create-only left panel (V1.132 P3 AC-8)', async () => {
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    expect(screen.getByRole('tab', { name: 'Creator', selected: true })).toBeInTheDocument();
    expect(screen.getByTestId('sidebar-create-panel')).toBeInTheDocument();
    expect(screen.getByTestId('creator-create-world')).toBeInTheDocument();
    expect(screen.getByTestId('creator-create-work')).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'All Works' })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Worlds' })).not.toBeInTheDocument();
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
    expect(screen.queryByRole('link', { name: 'Capabilities' })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Modules' })).not.toBeInTheDocument();
    expect(screen.queryByTestId('sidebar-create-panel')).not.toBeInTheDocument();
  });

  it('does not expose Modules in the Orchestrator tab (V1.130)', async () => {
    const user = userEvent.setup();
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    await user.click(screen.getByRole('tab', { name: 'Orchestrator' }));
    expect(screen.queryByRole('link', { name: 'Modules' })).not.toBeInTheDocument();
  });

  it('does not expose Connect or Daemon as top-level nav items', async () => {
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    expect(screen.queryByRole('link', { name: /Connect/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: /Daemon/i })).not.toBeInTheDocument();
  });

  it('wraps tabs in a tablist; Creator uses panelContent tabpanel', async () => {
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    expect(screen.getByRole('tablist', { name: 'Primary navigation' })).toBeInTheDocument();
    expect(screen.getByTestId('shell-sidebar-panel')).toHaveAttribute('aria-labelledby', 'creator');
  });

  it('mounts the 工作区 footer profile switcher on both Creator and Orchestrator (V1.132 P3 AC-6)', async () => {
    const user = userEvent.setup();
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    await waitFor(() =>
      expect(screen.getByRole('toolbar', { name: 'Workspace' })).toBeInTheDocument(),
    );

    await user.click(screen.getByRole('tab', { name: 'Orchestrator' }));

    expect(screen.getByRole('toolbar', { name: 'Workspace' })).toBeInTheDocument();
  });

  it('keeps the active workspace identity when switching Creator ↔ Orchestrator (V1.132 P3 AC-7)', async () => {
    const user = userEvent.setup();
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    const alice = await screen.findByTitle('Alice');
    expect(alice).toHaveAttribute('aria-pressed', 'true');

    await user.click(screen.getByRole('tab', { name: 'Orchestrator' }));
    expect(screen.getByTitle('Alice')).toHaveAttribute('aria-pressed', 'true');

    await user.click(screen.getByRole('tab', { name: 'Creator' }));
    expect(screen.getByTitle('Alice')).toHaveAttribute('aria-pressed', 'true');
  });

  it('keeps the footer mode switch as the only primary Creator|Orchestrator control', () => {
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    expect(screen.getByTestId('shell-mode-switch')).toBeInTheDocument();
    const tablists = screen.getAllByRole('tablist');
    expect(tablists).toHaveLength(1);
    expect(tablists[0]).toHaveAttribute('data-testid', 'shell-mode-switch');
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
    expect(screen.queryByRole('link', { name: '全部作品' })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: '世界' })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: '记忆' })).not.toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByRole('toolbar', { name: '工作区' })).toBeInTheDocument(),
    );

    await user.click(screen.getByRole('tab', { name: '编排' }));
    expect(screen.getByRole('link', { name: '记忆' })).toBeInTheDocument();
    expect(screen.getByRole('toolbar', { name: '工作区' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '计算' })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: '模块' })).not.toBeInTheDocument();
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

  it('orders Orchestrator groups Memory → Strategies → Runtime (V1.130)', async () => {
    const user = userEvent.setup();
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });
    await user.click(screen.getByRole('tab', { name: 'Orchestrator' }));

    const groupButtons = screen
      .getAllByRole('button')
      .filter((el) => ['Memory', 'Strategies', 'Runtime'].includes(el.textContent ?? ''));
    expect(groupButtons.map((el) => el.textContent)).toEqual([
      'Memory',
      'Strategies',
      'Runtime',
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

  it('nests Strategy under the Orchestration tab (AC-P2-4)', async () => {
    const user = userEvent.setup();
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    await user.click(screen.getByRole('tab', { name: 'Orchestrator' }));

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

    const nav = screen.getByRole('navigation');
    expect(nav).toHaveClass('flex-1');
    expect(nav).toHaveClass('min-h-0');

    const chromeRoot = nav.firstElementChild as HTMLElement;
    expect(chromeRoot).toHaveClass('h-full');
    expect(chromeRoot).toHaveClass('flex-col');
  });

  it('scrolls creator panel internally while the footer block stays pinned', async () => {
    useSidebarHandlers();
    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    const panel = screen.getByTestId('shell-sidebar-panel');
    expect(panel).toHaveClass('overflow-auto');
    expect(panel).toHaveClass('flex-1');

    await waitFor(() =>
      expect(screen.getByRole('toolbar', { name: 'Workspace' })).toBeInTheDocument(),
    );

    const toolbar = screen.getByRole('toolbar', { name: 'Workspace' });
    const tabpanel = screen.getByRole('tabpanel');
    expect(tabpanel.contains(toolbar)).toBe(false);
    expect(toolbar.closest('.border-t')).not.toBeNull();
  });
});

describe('Sidebar — work routes (V1.132 P3 AC-8)', () => {
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

  it('keeps Creator | Orchestrator tabs and Create-only left inside a work (AC-P2-5)', async () => {
    renderSidebarAtRoute('/works/work-42/outline');

    expect(screen.getByRole('tab', { name: 'Creator', selected: true })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Orchestrator' })).toBeInTheDocument();
    expect(screen.getByTestId('sidebar-create-panel')).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'All Works' })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Drill Novel' })).not.toBeInTheDocument();
  });

  it('shows Create-only left on the /works list route', async () => {
    renderSidebarAtRoute('/works');

    expect(screen.getByRole('tab', { name: 'Creator' })).toBeInTheDocument();
    expect(screen.getByTestId('sidebar-create-panel')).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'All Works' })).not.toBeInTheDocument();
  });
});
