import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Route, Routes } from 'react-router-dom';

import { Sidebar } from './sidebar';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';

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
    expect(screen.getByRole('link', { name: 'Strategies' })).toBeInTheDocument();

    expect(screen.queryByRole('link', { name: 'All Works' })).not.toBeInTheDocument();
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

    expect(screen.getByRole('button', { name: 'Canvas' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Outline' })).toBeInTheDocument();
    // World KB has no worldId at the default route and no `/worlds` picker
    // exists, so it renders disabled (not a link) — see T3 navigation wiring.
    const worldKb = screen.getByText('World KB');
    expect(worldKb.closest('[aria-disabled="true"]')).toBeInTheDocument();
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
    // has no worldId here and renders disabled (not a link); Strategy stays a
    // link but inactive.
    expect(screen.queryByRole('link', { name: 'World KB' })).not.toBeInTheDocument();
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

  it('World KB renders disabled (not a link) when no worldId is in the URL', async () => {
    // No `/worlds` picker route exists, so without a worldId the item has no
    // valid target — it must render as an aria-disabled span, not navigate.
    renderSidebarAtRoute('/works/work-1');

    expect(screen.queryByRole('link', { name: 'World KB' })).not.toBeInTheDocument();
    const worldKb = screen.getByText('World KB');
    expect(worldKb.closest('[aria-disabled="true"]')).toBeInTheDocument();
  });

  it('Strategy always navigates to /strategies regardless of context', async () => {
    renderSidebarAtRoute('/works/work-1');

    expect(screen.getByRole('link', { name: 'Strategy' })).toHaveAttribute('href', '/strategies');
  });
});
