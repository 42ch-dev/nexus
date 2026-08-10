/**
 * RootLayout — scroll split structure tests (AD-P2-2, V1.117 P2 T1).
 *
 * jsdom does not compute CSS layout, so these tests assert the class
 * composition that ESTABLISHES the scroll split rather than visual scroll
 * behavior. The classes are the SSOT — if they are present and correctly
 * nested, a real browser produces the intended viewport-locked layout:
 *
 *   - Root locked to viewport: `h-screen overflow-hidden` (no page scroll).
 *   - Sidebar aside: `h-screen overflow-hidden flex-col` (full-height rail;
 *     chrome manages internal scroll).
 *   - Main column: `min-h-0` (flex child can shrink → content scrolls, not page).
 *   - Content `<main>`: `overflow-y-auto min-h-0` (the ONLY scroll region).
 */
import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { screen, within } from '@testing-library/react';
import { Route, Routes } from 'react-router';

import { RootLayout } from './root-layout';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';

// Mock the command-palette surface — not layout-relevant, keeps the DOM clean
// for structural assertions on the root / aside / main column.
vi.mock('@/components/canvas/canvas-nav-commands', () => ({
  CanvasNavCommands: () => null,
}));
vi.mock('@/components/command-palette', () => ({
  CommandPalette: () => null,
  openPalette: vi.fn(),
}));
// NexusLogo uses useTheme() which requires a ThemeProvider not mounted by
// renderInApp. Mock to a plain div (same pattern as sidebar.test.tsx).
vi.mock('@/components/brand/nexus-logo', () => ({
  NexusLogo: () => <div data-testid="nexus-logo">Nexus</div>,
}));
// ChronosTitlebar uses useTheme() and settings context; mock to keep DOM structural.
vi.mock('@/components/layout/chronos-titlebar', () => ({
  ChronosTitlebar: ({ title }: { title: string }) => (
    <header data-testid="chronos-titlebar">{title}</header>
  ),
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

function renderLayout() {
  return renderInApp(
    <Routes>
      <Route element={<RootLayout />}>
        <Route path="/" element={<div data-testid="outlet-content">Content</div>} />
      </Route>
    </Routes>,
    { client: makeClient(), activeCreatorId: 'creator-a' },
  );
}

describe('RootLayout — scroll split (AD-P2-2)', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  it('locks the root to the viewport (h-screen + overflow-hidden)', () => {
    useCreatorHandler();
    const { container } = renderLayout();

    const root = container.firstElementChild as HTMLElement;
    expect(root).toHaveClass('h-screen');
    expect(root).toHaveClass('overflow-hidden');
  });

  it('establishes the sidebar as a full-height rail (h-full + overflow-hidden + flex-col)', () => {
    useCreatorHandler();
    const { container } = renderLayout();

    const aside = container.querySelector('aside');
    expect(aside).not.toBeNull();
    expect(aside).toHaveClass('h-full');
    expect(aside).toHaveClass('overflow-hidden');
    expect(aside).toHaveClass('flex-col');
  });

  it('makes the main column shrinkable (min-h-0) so only content scrolls', () => {
    useCreatorHandler();
    const { container } = renderLayout();

    const main = container.querySelector('main');
    expect(main).not.toBeNull();
    const mainColumn = main!.parentElement;
    expect(mainColumn).toHaveClass('min-h-0');
  });

  it('makes only the content region scrollable (main has overflow-y-auto + min-h-0)', () => {
    useCreatorHandler();
    const { container } = renderLayout();

    const main = container.querySelector('main');
    expect(main).toHaveClass('overflow-y-auto');
    expect(main).toHaveClass('min-h-0');
  });

  it('renders the Chronos titlebar above the sidebar/content row', () => {
    useCreatorHandler();
    const { container } = renderLayout();

    const root = container.firstElementChild as HTMLElement;
    expect(root.querySelector('[data-testid="chronos-titlebar"]')).not.toBeNull();
  });
});

describe('RootLayout — route title (V1.132 Bugbot)', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  it('shows Worlds in the titlebar on /worlds', () => {
    useCreatorHandler();
    renderInApp(
      <Routes>
        <Route element={<RootLayout />}>
          <Route path="/worlds" element={<div data-testid="outlet-content">Worlds hub</div>} />
        </Route>
      </Routes>,
      { client: makeClient(), activeCreatorId: 'creator-a', initialRouterEntries: ['/worlds'] },
    );

    expect(screen.getByTestId('chronos-titlebar')).toHaveTextContent('Worlds');
  });
});

describe('RootLayout — mobile nav key list (V1.120 P2 T2)', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });

  /** The mobile top nav is the <nav> outside the desktop sidebar <aside>. */
  function getMobileNav(container: HTMLElement) {
    const aside = container.querySelector('aside');
    return [...container.querySelectorAll('nav')].find(
      (n) => aside === null || !aside.contains(n),
    ) as HTMLElement | undefined;
  }

  it('exposes the mobile top-nav links (AC-P2-5)', () => {
    useCreatorHandler();
    const { container } = renderLayout();

    const mobileNav = getMobileNav(container);
    expect(mobileNav).toBeDefined();

    // Control: the list still renders the expected author-facing surfaces.
    expect(mobileNav!.querySelector('a[href="/sessions"]')).not.toBeNull();
    expect(mobileNav!.querySelector('a[href="/works"]')).not.toBeNull();
  });

  it('has no Capabilities item in the mobile top nav (AC-P2-2)', () => {
    useCreatorHandler();
    const { container } = renderLayout();

    const mobileNav = getMobileNav(container);
    expect(mobileNav).toBeDefined();

    // No Capabilities link by text or by href (MOBILE_NAV_KEYS updated).
    expect(
      within(mobileNav!).queryByRole('link', { name: 'Capabilities' }),
    ).not.toBeInTheDocument();
    expect(mobileNav!.querySelector('a[href="/capabilities"]')).toBeNull();
  });
});
