/**
 * WorldsPage render tests (V1.115 T3 — R-V1111P1-WORLDS-PICKER; V1.122 P1 T3
 * retarget).
 *
 * Verifies the picker UX: the page lists worlds from the existing
 * `GET /v1/daemon/narrative/worlds` endpoint, picking a world navigates to
 * `/worlds/<id>/timeline` (V1.122 P1 T3 retarget — Timeline is the default
 * World entry; World KB stays reachable as a peer surface), and an honest
 * empty state renders when the list is empty.
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';
import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Route, Routes, useLocation } from 'react-router-dom';

import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';
import { WorldsPage } from '@/pages/worlds-page';

const client = () => new BrowserClient();

function LocationDisplay() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}</div>;
}

/**
 * Mount WorldsPage inside a route tree with a location probe so navigation
 * (MemoryRouter, not `window.location`) is observable — mirrors the pattern in
 * `strategy-page.test.tsx`.
 */
function renderWorldsAt(initialPath = '/worlds') {
  return renderInApp(
    <>
      <LocationDisplay />
      <Routes>
        <Route path="worlds" element={<WorldsPage />} />
        {/* V1.122 P1 T3 — pick target is now the Timeline route. */}
        <Route path="worlds/:worldId/timeline" element={<div data-testid="timeline-outlet" />} />
        <Route path="worlds/:worldId/kb" element={<div data-testid="world-kb-outlet" />} />
      </Routes>
    </>,
    { client: client(), initialRouterEntries: [initialPath] },
  );
}

/** Render WorldsPage standalone (no navigation assertions). */
function renderWorlds() {
  return renderInApp(<WorldsPage />, { client: client() });
}

function world(over: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    schema_version: 1,
    world_id: 'w-1',
    owner_creator_id: 'creator-a',
    title: 'Eryndor',
    slug: 'w-1',
    status: 'active',
    visibility: 'private',
    time_policy: 'manual',
    created_at: '2026-07-01T00:00:00Z',
    ...over,
  };
}

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

describe('WorldsPage', () => {
  it('renders the world list with titles', async () => {
    useHandlers(
      http.get('/v1/daemon/narrative/worlds', () =>
        HttpResponse.json({
          worlds: [
            world({ world_id: 'eryndor', title: 'The Realms of Eryndor' }),
            world({ world_id: 'solara', title: 'Solara' }),
          ],
        }),
      ),
    );

    renderWorlds();

    expect(await screen.findByText('The Realms of Eryndor')).toBeInTheDocument();
    expect(screen.getByText('Solara')).toBeInTheDocument();
  });

  it('navigates to /worlds/<id>/timeline when a world is picked (V1.122 P1 T3 retarget)', async () => {
    const user = userEvent.setup();

    useHandlers(
      http.get('/v1/daemon/narrative/worlds', () =>
        HttpResponse.json({ worlds: [world({ world_id: 'eryndor', title: 'Eryndor' })] }),
      ),
    );

    renderWorldsAt();

    await screen.findByText('Eryndor');
    await user.click(screen.getByRole('button', { name: 'Open timeline' }));

    await waitFor(() => {
      expect(screen.getByTestId('location')).toHaveTextContent('/worlds/eryndor/timeline');
    });
  });

  it('encodes a space-bearing world id in the navigation target', async () => {
    const user = userEvent.setup();

    useHandlers(
      http.get('/v1/daemon/narrative/worlds', () =>
        HttpResponse.json({ worlds: [world({ world_id: 'w 7', title: 'Spaced' })] }),
      ),
    );

    renderWorldsAt();

    await screen.findByText('Spaced');
    await user.click(screen.getByRole('button', { name: 'Open timeline' }));

    await waitFor(() => {
      expect(screen.getByTestId('location')).toHaveTextContent('/worlds/w%207/timeline');
    });
  });

  it('renders a card-sized Work-create CTA when no worlds exist (V1.125 P2)', async () => {
    const user = userEvent.setup();
    useHandlers(
      http.get('/v1/daemon/narrative/worlds', () => HttpResponse.json({ worlds: [] })),
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({ items: [], pagination: { limit: 20, has_more: false } }),
      ),
    );

    renderWorlds();

    const cta = await screen.findByTestId('worlds-empty-create-work');
    expect(cta).toHaveTextContent('Create a Work to get started');
    expect(cta.className).toMatch(/\bmin-h-\[7\.5rem\]/);

    await user.click(cta);
    expect(await screen.findByRole('dialog', { name: 'Create Work' })).toBeInTheDocument();
  });

  it('shows Create World card when client exposes createWorld (V1.125 P2)', async () => {
    useHandlers(
      http.get('/v1/daemon/narrative/worlds', () => HttpResponse.json({ worlds: [] })),
    );

    const clientWithCreateWorld = Object.assign(new BrowserClient(), {
      createWorld: async () => ({ world_id: 'w-new' }),
    });

    renderInApp(<WorldsPage />, { client: clientWithCreateWorld });

    expect(await screen.findByTestId('worlds-empty-create-world')).toHaveTextContent(
      'Start a new World',
    );
    expect(screen.queryByTestId('worlds-empty-create-work')).not.toBeInTheDocument();
  });

  it('renders the honest empty state when no worlds exist', async () => {
    useHandlers(
      http.get('/v1/daemon/narrative/worlds', () => HttpResponse.json({ worlds: [] })),
    );

    renderWorlds();

    expect(await screen.findByTestId('worlds-empty-create-work')).toBeInTheDocument();
  });

  it('renders the loading state before data resolves', async () => {
    useHandlers(
      http.get('/v1/daemon/narrative/worlds', async () => {
        await new Promise((resolve) => {
          setTimeout(resolve, 50);
        });
        return HttpResponse.json({ worlds: [] });
      }),
    );

    renderWorlds();

    expect(await screen.findByText('Loading worlds…')).toBeInTheDocument();
  });

  it('renders the error state when the fetch fails', async () => {
    useHandlers(
      http.get('/v1/daemon/narrative/worlds', () =>
        HttpResponse.json({ error: { code: 'INTERNAL', message: 'boom' } }, { status: 500 }),
      ),
    );

    renderWorlds();

    expect(await screen.findByText('Could not load worlds.')).toBeInTheDocument();
  });

  it('falls back to world_id when a world has no title', async () => {
    useHandlers(
      http.get('/v1/daemon/narrative/worlds', () =>
        HttpResponse.json({
          worlds: [world({ world_id: 'id-only', title: '' })],
        }),
      ),
    );

    renderWorlds();

    // Empty title → the button label falls back to world_id.
    expect(await screen.findByText('id-only')).toBeInTheDocument();
  });

  it('switches to zh-CN locale without remounting', async () => {
    useHandlers(
      http.get('/v1/daemon/narrative/worlds', () =>
        HttpResponse.json({ worlds: [world({ world_id: 'eryndor', title: 'Eryndor' })] }),
      ),
    );

    renderWorlds();
    expect(await screen.findByRole('heading', { name: 'Worlds' })).toBeInTheDocument();

    act(() => {
      i18n.changeLanguage('zh-CN');
    });

    expect(await screen.findByRole('heading', { name: '世界' })).toBeInTheDocument();
  });
});

// V1.121 v0.4 — voice-split discipline (DESIGN.md §Design Concept).
//
// Pins both directions of the serif contract on the Worlds page:
//   - page-level entity title (h1 "Worlds") → content voice (serif display-24);
//   - world list-item labels (the world titles) → content voice (serif
//     display-20) — world titles are creative-entity titles in a list;
//   - all sibling chrome (description, refresh button, world_id mono line)
//     → interface voice (sans) — no `font-display` leaks into chrome.
describe('WorldsPage voice-split (V1.121 v0.4)', () => {
  it('renders the page title in the content voice (serif display-24)', async () => {
    useHandlers(
      http.get('/v1/daemon/narrative/worlds', () =>
        HttpResponse.json({ worlds: [world({ world_id: 'eryndor', title: 'Eryndor' })] }),
      ),
    );

    renderWorlds();

    const title = await screen.findByRole('heading', { name: 'Worlds' });
    expect(title.tagName).toBe('H1');
    expect(title.className).toMatch(/\bfont-display\b/);
    expect(title.className).toMatch(/\btext-display-24\b/);
    // Interface-voice heading treatment is absent.
    expect(title.className).not.toMatch(/\btext-heading-24\b/);
    expect(title.className).not.toMatch(/\bfont-heading\b/);
  });

  it('renders world list-item titles in the content voice (serif display-20)', async () => {
    useHandlers(
      http.get('/v1/daemon/narrative/worlds', () =>
        HttpResponse.json({
          worlds: [world({ world_id: 'eryndor', title: 'The Realms of Eryndor' })],
        }),
      ),
    );

    renderWorlds();

    const label = await screen.findByText('The Realms of Eryndor');
    expect(label.className).toMatch(/\bfont-display\b/);
    expect(label.className).toMatch(/\btext-display-20\b/);
  });

  it('keeps the description, refresh button, and world_id mono line in the interface voice', async () => {
    useHandlers(
      http.get('/v1/daemon/narrative/worlds', () =>
        HttpResponse.json({ worlds: [world({ world_id: 'eryndor', title: 'Eryndor' })] }),
      ),
    );

    renderWorlds();
    await screen.findByText('Eryndor');

    // Page description stays sans. (The description string appears on both the
    // page header and the card description — both stay interface voice.)
    const descriptions = screen.getAllByText('Choose a world to open its timeline.');
    for (const d of descriptions) {
      expect(d.className).not.toMatch(/\bfont-display\b/);
    }

    // Refresh button stays sans.
    const refresh = screen.getByRole('button', { name: 'Refresh worlds' });
    expect(refresh.className).not.toMatch(/\bfont-display\b/);

    // The world_id mono secondary line stays sans-mono (interface voice).
    const idLine = screen.getByText('eryndor');
    expect(idLine.className).not.toMatch(/\bfont-display\b/);
    expect(idLine.className).toMatch(/\btext-copy-13-mono\b/);
  });
});
