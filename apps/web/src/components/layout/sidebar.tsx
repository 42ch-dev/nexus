import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Link, NavLink, useLocation, useParams } from 'react-router-dom';
import {
  Boxes,
  BrainCircuit,
  CalendarClock,
  Cpu,
  Layers,
  ListChecks,
  Sparkles,
} from 'lucide-react';

import { NexusLogo } from '@/components/brand/nexus-logo';
import { FooterProfiles } from '@/components/layout/footer-profiles';
import {
  CANVAS_ITEMS,
  CANVAS_NAV_GROUP,
  resolveActiveCanvasSurface,
  resolveCanvasNavTarget,
  type CanvasNavItem,
  type CanvasSurfaceId,
} from '@/components/layout/canvas-nav';
import {
  ShellSidebarChrome,
  type ShellNavGroup,
  type ShellSidebarTab,
} from '@/components/layout/presentational/shell-sidebar-chrome';
import { cn } from '@/lib/utils';

function surfaceKey(surfaceId: CanvasSurfaceId): 'outline' | 'worldKb' | 'strategy' {
  return surfaceId === 'world-kb' ? 'worldKb' : surfaceId;
}

/**
 * Sidebar nav — V1.94 two-tab IA (Creator | Orchestrator), extended in V1.111
 * to nest the Canvas group (Outline / World KB / Strategy) under the Creator
 * (Works) tab.
 *
 * Thin wrapper around {@link ShellSidebarChrome}: owns NavLink, the active
 * creator profile, and the route-derived active state. The chrome owns the
 * markup, classes, and `data-testid` SSOT.
 *
 * Active-highlight note (V1.111): the chrome derives each item's active state
 * from a built-in `activeRoute` prefix match (`activeRoute === item.to ||
 * activeRoute.startsWith(item.to + '/')`). That match is correct for non-canvas
 * items but TOO BROAD for Canvas items — e.g. "Outline" (`to: '/works'`) would
 * false-light on plain `/works/:workId` work-detail. The host passes an
 * `isActiveItem` callback so the chrome's per-item active state is
 * resolver-driven for Canvas items (via {@link resolveActiveCanvasSurface})
 * and prefix-driven for non-canvas items — keeping the chrome's markup SSOT
 * with no mirrored item markup in the host.
 *
 * Navigation note (V1.111 T3): Canvas items don't navigate to their static
 * `to` (a list-picker identity); the click target is computed by
 * {@link resolveCanvasNavTarget} from the URL-scoped Work/World context so the
 * active context is preserved (e.g. Outline on a Work route →
 * `/works/:workId/outline`, not `/works`). The app has no global active
 * work/world — ids come from `useParams`, exactly like the palette's
 * `canvas-nav-commands.tsx`. Every Canvas surface has a valid fallback target
 * (World KB → `/worlds` picker as of V1.115 T3), so items are always
 * focusable links.
 */
export function Sidebar() {
  const { t } = useTranslation('shell');
  const [activeTab, setActiveTab] = useState<ShellSidebarTab>('creator');
  const { pathname } = useLocation();
  // URL-scoped context for Canvas nav targets (mirrors canvas-nav-commands).
  // Sidebar lives in RootLayout (a layout route), so useParams returns the leaf
  // route's params — workId/worldId are populated on Work-/World-scoped routes
  // and undefined elsewhere.
  const { workId, worldId } = useParams<{ workId?: string; worldId?: string }>();

  const translatedCanvasGroup = useMemo<ShellNavGroup>(
    () => ({
      ...CANVAS_NAV_GROUP,
      label: t('nav.canvas'),
      items: CANVAS_ITEMS.map((item) => ({
        ...item,
        label: t(`nav.${surfaceKey(item.surfaceId)}`),
      })),
    }),
    [t],
  );

  const creatorGroups: ShellNavGroup[] = useMemo(
    () => [
      {
        id: 'works',
        label: t('nav.works'),
        items: [{ to: '/works', label: t('nav.allWorks'), icon: Layers }],
      },
      translatedCanvasGroup,
      {
        id: 'creator',
        label: t('nav.creator'),
        items: [{ to: '/memory', label: t('nav.memory'), icon: BrainCircuit }],
      },
    ],
    [t, translatedCanvasGroup],
  );

  const orchestratorGroups: ShellNavGroup[] = useMemo(
    () => [
      {
        id: 'runtime',
        label: t('nav.runtime'),
        items: [
          { to: '/sessions', label: t('nav.sessions'), icon: ListChecks },
          { to: '/schedule', label: t('nav.schedule'), icon: CalendarClock },
          { to: '/capabilities', label: t('nav.capabilities'), icon: Boxes },
        ],
      },
      {
        id: 'compute',
        label: t('nav.compute'),
        items: [{ to: '/modules', label: t('nav.modules'), icon: Cpu }],
      },
      {
        id: 'strategies',
        label: t('nav.strategies'),
        items: [{ to: '/strategies', label: t('nav.strategies'), icon: Sparkles }],
      },
    ],
    [t],
  );

  const groups = activeTab === 'creator' ? creatorGroups : orchestratorGroups;

  return (
    <nav aria-label={t('aria.primary')} className="min-h-0 flex-1">
      <ShellSidebarChrome
        activeTab={activeTab}
        activeRoute={pathname}
        settingsActive={pathname.startsWith('/settings')}
        navGroups={groups}
        onTabChange={setActiveTab}
        logo={<NexusLogo />}
        footer={<FooterProfiles />}
        creatorTabLabel={t('nav.creator')}
        orchestratorTabLabel={t('nav.orchestrator')}
        settingsLabel={t('nav.settings')}
        primaryNavigationAriaLabel={t('aria.primaryNavigation')}
        isActiveItem={(item, route) => {
          // Canvas items: resolver-driven (precise surface match — NOT the
          // broad `item.to` prefix). Non-canvas items: the chrome's built-in
          // prefix match, replicated here so this callback is the single
          // active-resolution source for the host-owned groups.
          if ('surfaceId' in item) {
            return resolveActiveCanvasSurface(route) === (item as CanvasNavItem).surfaceId;
          }
          return route === item.to || route.startsWith(`${item.to}/`);
        }}
        renderNavItem={(item, className, content, isActive) => {
          // Canvas items: compute the context-aware click target (T3). The
          // static `item.to` is the chrome-keyed identity only; the real
          // destination preserves the active Work/World context. A `null`
          // target means the surface has no valid entry point — currently no
          // Canvas surface returns null (World KB falls back to `/worlds` as of
          // V1.115 T3), but the disabled branch is retained for safety.
          if ('surfaceId' in item) {
            const target = resolveCanvasNavTarget((item as CanvasNavItem).surfaceId, {
              workId,
              worldId,
            });
            if (target === null) {
              return (
                <span
                  aria-disabled="true"
                  className={cn(className, 'cursor-not-allowed opacity-disabled pointer-events-none')}
                >
                  {content}
                </span>
              );
            }
            // `<Link>` (not `<NavLink>`) so react-router's own prefix detection
            // can't re-introduce the false positive the resolver exists to fix.
            // aria-current is host-owned for Canvas items.
            return (
              <Link
                to={target}
                aria-current={isActive ? 'page' : undefined}
                className={className}
              >
                {content}
              </Link>
            );
          }

          // Non-canvas item — chrome-driven active state, verbatim (V1.94 behavior).
          return (
            <NavLink
              to={item.to}
              className={cn(className, isActive ? 'bg-gray-alpha-100 text-gray-1000' : undefined)}
            >
              {content}
            </NavLink>
          );
        }}
        renderSettingsLink={(to, className, content, isActive) => (
          <NavLink
            to={to}
            data-testid="settings-footer-utility-link"
            className={cn(className, isActive ? 'bg-gray-alpha-100 text-gray-1000' : undefined)}
          >
            {content}
          </NavLink>
        )}
      />
    </nav>
  );
}

// Re-export the chrome types for convenience.
export type { ShellNavGroup, ShellSidebarTab };
