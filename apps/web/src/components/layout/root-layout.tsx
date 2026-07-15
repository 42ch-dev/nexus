import { NavLink, Outlet, useLocation } from 'react-router-dom';
import { useTranslation } from 'react-i18next';

import { CanvasNavCommands } from '@/components/canvas/canvas-nav-commands';
import { CommandPalette, openPalette } from '@/components/command-palette';
import { DaemonStatusBar } from '@/components/layout/daemon-status-bar';
import { Header } from '@/components/layout/header';
import { MainBanner } from '@/components/layout/main-banner';
import { Sidebar } from '@/components/layout/sidebar';
import { useHotkey } from '@/lib/use-hotkey';
import { isWorkShellRoute } from '@/lib/work-shell-routes';
import { cn } from '@/lib/utils';

const ROUTE_KEYS: Record<string, string> = {
  '/works': 'works',
  '/sessions': 'sessions',
  '/schedule': 'schedule',
  '/capabilities': 'capabilities',
  '/findings': 'findings',
  '/memory': 'memory',
  '/settings': 'settings',
  '/strategies': 'strategies',
};

const MOBILE_NAV_KEYS = [
  { to: '/works', key: 'works' },
  { to: '/sessions', key: 'sessions' },
  { to: '/schedule', key: 'schedule' },
  { to: '/capabilities', key: 'capabilities' },
  { to: '/memory', key: 'memory' },
  { to: '/strategies', key: 'strategies' },
  { to: '/settings', key: 'settings' },
];

/** Resolve the header title from the active top-level route. */
function useRouteTitle(): string {
  const { t } = useTranslation('shell');
  const { pathname } = useLocation();
  const top = `/${pathname.split('/')[1] ?? ''}`;
  const routeKey = ROUTE_KEYS[top];
  return routeKey ? t(`route.${routeKey}`) : t('appTitle');
}

/**
 * Root layout — DESIGN.md §Spacing/Layout Rules + AD-P2-2 layout/scroll SSOT.
 *
 * The root is locked to the viewport (`h-screen overflow-hidden`): the sidebar
 * is a full-height rail that never scrolls at the top level, and the main
 * column constrains its children with `min-h-0` so only `<main>` (the content
 * region) scrolls. The header, banner, and {@link DaemonStatusBar} are fixed
 * flex children that stay on screen.
 *
 * Fixed 248px sidebar at `lg` and above; collapses to a horizontal top nav
 * below `lg`. Main content max-width 1200px with 24px desktop / 16px mobile
 * side padding. V1.94 adds the {@link MainBanner} for daemon degraded/error
 * states; the footer status bar is restart-icon-only when running.
 */
export function RootLayout() {
  const { t } = useTranslation('shell');
  const { pathname } = useLocation();
  const title = useRouteTitle();
  const workShell = isWorkShellRoute(pathname);

  // ⌘K/Ctrl+K opens the command palette. The palette owns its open/close
  // (module-level store in `command-palette.tsx`); the hotkey just calls
  // `openPalette()`. See V1.111 P0 T3.
  useHotkey('mod+k', () => openPalette());

  return (
    <div className="flex h-screen overflow-hidden bg-background-100 text-gray-1000">
      {/* Desktop sidebar — full-height rail (AD-P2-2): h-screen + overflow-hidden
          so the sidebar chrome manages its own internal scroll (nav scrolls in
          the middle; Settings + Profiles footer block is pinned at the bottom). */}
      <aside className="hidden h-screen w-[248px] shrink-0 flex-col overflow-hidden lg:flex">
        <Sidebar />
      </aside>

      {/* Main column — min-h-0 lets the flex child shrink so only <main> scrolls. */}
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        {/* Mobile top nav (below lg) */}
        <nav
          aria-label={t('aria.primary')}
          className="flex gap-1 overflow-x-auto border-b border-gray-alpha-400 bg-background-100 px-2 py-2 lg:hidden"
        >
          {MOBILE_NAV_KEYS.map(({ to, key }) => (
            <NavLink
              key={to}
              to={to}
              className={({ isActive }) =>
                cn(
                  'whitespace-nowrap rounded-control px-3 py-1.5 text-label-14 transition-colors duration-state ease-standard',
                  isActive
                    ? 'bg-gray-alpha-100 text-gray-1000'
                    : 'text-gray-800 hover:bg-gray-alpha-100 hover:text-gray-1000',
                )
              }
            >
              {t(`route.${key}`)}
            </NavLink>
          ))}
        </nav>

        <Header title={title} />

        <MainBanner />

        {/* Content region — the ONLY scroll region (AD-P2-2). min-h-0 allows
            the flex child to shrink within the column so overflow scrolls here
            instead of growing the column past the viewport. */}
        <main className="min-h-0 flex-1 overflow-y-auto">
          <div
            className={cn(
              'mx-auto w-full',
              workShell
                ? 'max-w-none px-0 py-0'
                : 'max-w-[1200px] px-4 py-6 md:px-6 md:py-8',
            )}
            data-testid={workShell ? 'main-work-shell' : 'main-standard'}
          >
            <Outlet />
          </div>
        </main>

        <DaemonStatusBar />
      </div>

      {/* V1.111 P0 T4 — registers canvas nav commands into the palette.
          Effect-only; renders nothing. Mounted here (not in a canvas) so the
          commands are available wherever the palette can open. */}
      <CanvasNavCommands />

      {/* Global command palette overlay (⌘K / Ctrl+K). Rendered last so it
          layers above the main column. */}
      <CommandPalette />
    </div>
  );
}
