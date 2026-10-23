/**
 * App shell — topnav + entrance-scoped app rail + scrollable content slot
 * (plan v1219 Task 2; IA spec §3). Replaces the old `RootLayout` chrome in
 * `App.tsx`; legacy pages keep their explicit routes and render through the
 * `<Outlet/>` content slot.
 *
 * {@link AppModuleRoute} is the route-level lazy boundary for registry apps:
 * a locked app redirects to the entrance land route (with the § Copy reason as
 * toast) BEFORE the lazy factory is created, so its chunk is never requested.
 */
import { Suspense, lazy, useEffect, useMemo } from 'react';
import { NavLink, Navigate, Outlet, useLocation, useNavigate } from 'react-router';
import { useTranslation } from 'react-i18next';

import { CanvasNavCommands } from '@/components/canvas/canvas-nav-commands';
import { CommandPalette, openPalette } from '@/components/command-palette';
import { DaemonStatusBar } from '@/components/layout/daemon-status-bar';
import { resolveEntranceBounce } from '@/components/layout/entrance-guard';
import { ServiceDegradedBanner } from '@/components/setup/service-degraded-surface';
import { Topnav } from '@/components/shell/topnav';
import { AppRail } from '@/components/shell/app-rail';
import { LoadingState } from '@/components/ui/states';
import { appsForEntrance, isAppLocked, type AppId } from '@/lib/app-registry';
import { useActivation } from '@/lib/activation-store';
import { useActiveCreatorId } from '@/lib/active-creator-context';
import { useEntrance } from '@/lib/entrance-context';
import { useHotkey } from '@/lib/use-hotkey';
import { useTimelineShortcut } from '@/lib/keyboard-shortcuts';
import { useToast } from '@/lib/use-toast';
import { isWorkShellRoute } from '@/lib/work-shell-routes';
import { cn } from '@/lib/utils';

// Mobile top nav (below lg) — same entrance-bounce filter as the retired
// RootLayout chrome; the dock rail takes over at `lg` and above.
const MOBILE_NAV_KEYS = [
  { to: '/works', key: 'works' },
  { to: '/sessions', key: 'sessions' },
  { to: '/schedule', key: 'schedule' },
  { to: '/memory', key: 'memory' },
  { to: '/strategies', key: 'strategies' },
  { to: '/settings', key: 'settings' },
] as const;

export function AppShell() {
  const { t } = useTranslation('shell');
  const { pathname } = useLocation();
  const navigate = useNavigate();
  const { entrance } = useEntrance();
  const workShell = isWorkShellRoute(pathname);

  // ⌘K/Ctrl+K opens the command palette (V1.111 P0 T3 behaviour, unchanged).
  useHotkey('mod+k', () => openPalette());
  // V1.123 P3 Task 5 — global Timeline chord (`g` then `t`).
  useTimelineShortcut(() => navigate('/timeline'));

  const mobileNav = MOBILE_NAV_KEYS.filter(
    ({ to }) => resolveEntranceBounce(entrance, to, '') === null,
  );

  return (
    <div
      data-testid="app-shell"
      className="flex h-screen flex-col overflow-hidden bg-background-100 text-gray-1000"
    >
      <ServiceDegradedBanner />
      <Topnav />

      <div className="flex min-h-0 flex-1 overflow-hidden">
        {/* Desktop dock rail; mobile keeps the horizontal top nav below. */}
        <AppRail />

        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          <nav
            aria-label={t('aria.primary')}
            className="flex gap-1 overflow-x-auto border-b border-gray-alpha-400 bg-background-100 px-2 py-2 lg:hidden"
          >
            {mobileNav.map(({ to, key }) => (
              <NavLink
                key={to}
                to={to}
                className={({ isActive }) =>
                  cn(
                    'whitespace-nowrap rounded-control px-3 py-1.5 text-label-14 transition-colors duration-state ease-standard motion-reduce:transition-none',
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

          {/* Content region — the ONLY scroll region (AD-P2-2). The
              workspace chip row (T3) mounts inside content apps, not here. */}
          <main className="min-h-0 flex-1 overflow-y-auto" data-testid="shell-main">
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

          {/* Desktop-only daemon footer (browser build renders null). */}
          <DaemonStatusBar />
        </div>
      </div>

      <CanvasNavCommands />
      <CommandPalette />
    </div>
  );
}

/**
 * Route element for a registry app. Resolves the app for the current
 * entrance; locked apps redirect to the entrance land route with the lock
 * reason as toast and NEVER create the lazy factory — the route chunk stays
 * unrequested. Unlocked apps render through `React.lazy(app.load)`.
 */
export function AppModuleRoute({ appId }: { appId: AppId }) {
  const { entrance } = useEntrance();
  const activeCreatorId = useActiveCreatorId();
  const { isUnlocked } = useActivation(activeCreatorId);
  const { toast } = useToast();
  const app = appsForEntrance(entrance).find((a) => a.id === appId) ?? null;
  const locked = app !== null && isAppLocked(app, isUnlocked);

  // Created only for unlocked apps: a locked app's `load` is never referenced.
  const LazyComponent = useMemo(
    () => (app !== null && !locked ? lazy(app.load) : null),
    [app, locked],
  );

  useEffect(() => {
    if (locked && app?.lockReason) {
      toast({ title: app.lockReason, variant: 'info' });
    }
  }, [locked, app, toast]);

  if (app === null || LazyComponent === null) {
    // Locked (or unknown) app: bounce to the first unlocked app of this
    // entrance. The entrance landRoute is NOT safe here — for the creator
    // entrance it IS the work app, which locks first (IA §3.2 progressive
    // activation), so landing there would self-redirect.
    const fallback = appsForEntrance(entrance).find((a) => !isAppLocked(a, isUnlocked));
    return <Navigate to={fallback?.route ?? '/'} replace />;
  }
  return (
    <Suspense fallback={<LoadingState />}>
      <LazyComponent />
    </Suspense>
  );
}
