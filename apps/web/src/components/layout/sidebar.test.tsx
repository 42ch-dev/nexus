import { fireEvent, screen, waitFor } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { describe, expect, it, beforeEach, vi } from 'vitest';
import userEvent from '@testing-library/user-event';
import { Route, Routes, useLocation } from 'react-router';

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

  it('renders the Creator tab with inline create panel (V1.136 P1)', async () => {
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    expect(screen.getByRole('tab', { name: 'Creator', selected: true })).toBeInTheDocument();
    expect(screen.getByTestId('sidebar-create-panel')).toBeInTheDocument();
    expect(screen.getByTestId('sidebar-create-tab-bar')).toBeInTheDocument();
    expect(screen.getByTestId('sidebar-create-form-world')).toBeInTheDocument();
    expect(screen.queryByTestId('creator-create-world')).not.toBeInTheDocument();
    expect(screen.queryByTestId('creator-create-work')).not.toBeInTheDocument();
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
    expect(screen.getByRole('link', { name: 'Harness' })).toBeInTheDocument();
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
    expect(tablists).toHaveLength(2);
    expect(tablists.some((el) => el.getAttribute('data-testid') === 'shell-mode-switch')).toBe(true);
    expect(screen.getByTestId('sidebar-create-tab-bar').querySelector('[role="tablist"]')).toBeInTheDocument();
  });

  it('does not expose a Settings row in the sidebar footer (V1.125 P2)', async () => {
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    expect(screen.queryByTestId('settings-footer-utility-link')).not.toBeInTheDocument();
  });

  it('renders localized labels when locale is zh-CN', async () => {
    window.localStorage.setItem('nexus-web-locale', 'zh-CN');
    await i18n.changeLanguage('zh-CN');
    const user = userEvent.setup();
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    expect(screen.getByRole('tab', { name: '创作', selected: true })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: '编排' })).toBeInTheDocument();
    expect(screen.getByTestId('sidebar-create-tab-world')).toHaveTextContent('世界');
    expect(screen.getByTestId('sidebar-create-tab-work')).toHaveTextContent('作品');
    expect(screen.getByLabelText('标题')).toBeInTheDocument();
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
      'bg-blue-1000',
    );
  });

  it('orders Orchestrator groups Memory → Harness → Runtime (V1.130)', async () => {
    const user = userEvent.setup();
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });
    await user.click(screen.getByRole('tab', { name: 'Orchestrator' }));

    const groupButtons = screen
      .getAllByRole('button')
      .filter((el) => ['Memory', 'Harness', 'Runtime'].includes(el.textContent ?? ''));
    expect(groupButtons.map((el) => el.textContent)).toEqual([
      'Memory',
      'Harness',
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
    expect(screen.getByRole('link', { name: 'Harness' })).toHaveClass('bg-gray-alpha-100');
  });

  it('nests Strategy under the Orchestration tab (AC-P2-4)', async () => {
    const user = userEvent.setup();
    useSidebarHandlers();

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    await user.click(screen.getByRole('tab', { name: 'Orchestrator' }));

    expect(screen.getByRole('link', { name: 'Harness' })).toHaveAttribute(
      'href',
      '/strategies',
    );
  });

  it('navigates to /works when switching to Creator from an orchestrator route', async () => {
    const user = userEvent.setup();
    useSidebarHandlers();

    function LocationProbe() {
      const { pathname } = useLocation();
      return <div data-testid="location">{pathname}</div>;
    }

    renderInApp(
      <Routes>
        <Route
          path="*"
          element={
            <>
              <Sidebar />
              <LocationProbe />
            </>
          }
        />
      </Routes>,
      {
        client: makeClient(),
        activeCreatorId: 'creator-a',
        initialRouterEntries: ['/strategies/user%2Ffoo'],
      },
    );

    await user.click(screen.getByRole('tab', { name: 'Creator' }));

    expect(screen.getByTestId('location')).toHaveTextContent('/works');
    expect(screen.getByRole('tab', { name: 'Creator', selected: true })).toBeInTheDocument();
  });

  it('navigates to /strategies when switching to Orchestrator from a creator route', async () => {
    const user = userEvent.setup();
    useSidebarHandlers();

    function LocationProbe() {
      const { pathname } = useLocation();
      return <div data-testid="location">{pathname}</div>;
    }

    renderInApp(
      <Routes>
        <Route
          path="*"
          element={
            <>
              <Sidebar />
              <LocationProbe />
            </>
          }
        />
      </Routes>,
      {
        client: makeClient(),
        activeCreatorId: 'creator-a',
        initialRouterEntries: ['/works'],
      },
    );

    await user.click(screen.getByRole('tab', { name: 'Orchestrator' }));

    expect(screen.getByTestId('location')).toHaveTextContent('/strategies');
    expect(screen.getByRole('tab', { name: 'Orchestrator', selected: true })).toBeInTheDocument();
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
    const tabpanel = screen.getByTestId('shell-sidebar-panel');
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

  it('shows sidebar create panel on hub /works surface (V1.135 P0)', async () => {
    renderSidebarAtRoute('/works');

    expect(screen.getByRole('tab', { name: 'Creator' })).toBeInTheDocument();
    expect(screen.getByTestId('sidebar-create-panel')).toBeInTheDocument();
    expect(screen.getByTestId('shell-sidebar-panel')).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'All Works' })).not.toBeInTheDocument();
  });

  it('shows sidebar create panel on hub /worlds surface (V1.135 P0)', async () => {
    renderSidebarAtRoute('/worlds');

    expect(screen.getByRole('tab', { name: 'Creator' })).toBeInTheDocument();
    expect(screen.getByTestId('sidebar-create-panel')).toBeInTheDocument();
  });
});

describe('Sidebar — inline create (V1.136 P1)', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  function LocationProbe() {
    const { pathname } = useLocation();
    return <div data-testid="location">{pathname}</div>;
  }

  it('submits inline World create via POST /v1/daemon/worlds without a dialog', async () => {
    let postedBody: unknown = null;
    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      worksList([]),
      http.get('/v1/daemon/narrative/worlds', () => HttpResponse.json({ worlds: [] })),
      http.post('/v1/daemon/worlds', async ({ request }) => {
        postedBody = await request.json();
        return HttpResponse.json({ world_id: 'w-new' });
      }),
    );

    renderInApp(
      <Routes>
        <Route
          path="*"
          element={
            <>
              <Sidebar />
              <LocationProbe />
            </>
          }
        />
      </Routes>,
      { client: makeClient(), activeCreatorId: 'creator-a', initialRouterEntries: ['/works'] },
    );

    fireEvent.change(screen.getByLabelText('Title'), { target: { value: 'Aurora' } });
    fireEvent.click(screen.getByTestId('sidebar-create-submit-world'));

    await waitFor(() => expect(postedBody).toEqual({ title: 'Aurora' }));
    await waitFor(() =>
      expect(screen.getByTestId('location')).toHaveTextContent('/worlds/w-new/timeline'),
    );
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('submits inline Work create via POST /v1/daemon/works without a dialog', async () => {
    const user = userEvent.setup();
    let postedBody: unknown = null;
    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      worksList([]),
      http.get('/v1/daemon/narrative/worlds', () => HttpResponse.json({ worlds: [] })),
      http.post('/v1/daemon/works', async ({ request }) => {
        postedBody = await request.json();
        return HttpResponse.json({ work_id: 'work-new', status: 'draft' });
      }),
    );

    renderInApp(
      <Routes>
        <Route
          path="*"
          element={
            <>
              <Sidebar />
              <LocationProbe />
            </>
          }
        />
      </Routes>,
      { client: makeClient(), activeCreatorId: 'creator-a', initialRouterEntries: ['/works'] },
    );

    await user.click(screen.getByTestId('sidebar-create-tab-work'));
    await user.type(screen.getByLabelText('Title'), 'Drill Novel');
    await user.type(screen.getByLabelText('Long-term goal'), 'Finish arc one');
    await user.type(screen.getByLabelText('Initial idea'), 'A heist in the sky');
    await user.click(screen.getByTestId('sidebar-create-submit-work'));

    await waitFor(() =>
      expect(postedBody).toEqual({
        title: 'Drill Novel',
        long_term_goal: 'Finish arc one',
        initial_idea: 'A heist in the sky',
      }),
    );
    await waitFor(() =>
      expect(screen.getByTestId('location')).toHaveTextContent('/works/work-new/outline'),
    );
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('retains inline world form fields when POST /v1/daemon/worlds fails', async () => {
    useHandlers(
      http.get('/v1/daemon/creators', () =>
        HttpResponse.json({
          items: [{ creator_id: 'creator-a', display_name: 'Alice' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      worksList([]),
      http.get('/v1/daemon/narrative/worlds', () => HttpResponse.json({ worlds: [] })),
      http.post('/v1/daemon/worlds', () => HttpResponse.json({ message: 'server error' }, { status: 500 })),
    );

    renderInApp(<Sidebar />, { client: makeClient(), activeCreatorId: 'creator-a' });

    const titleInput = screen.getByLabelText('Title');
    fireEvent.change(titleInput, { target: { value: 'Aurora' } });
    fireEvent.click(screen.getByTestId('sidebar-create-submit-world'));

    await waitFor(() => {
      expect(titleInput).toHaveValue('Aurora');
    });
  });
});
