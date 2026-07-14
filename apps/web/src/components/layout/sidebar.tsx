import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Link, NavLink, useLocation, useParams } from 'react-router-dom';
import {
  ArrowLeft,
  BookOpen,
  Boxes,
  BrainCircuit,
  CalendarClock,
  Cpu,
  Layers,
  ListChecks,
  ListTree,
  Sparkles,
} from 'lucide-react';

import { NexusLogo } from '@/components/brand/nexus-logo';
import { FooterProfiles } from '@/components/layout/footer-profiles';
import {
  CANVAS_ITEMS,
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
 * Sidebar nav — V1.94 two-tab IA (Creator | Orchestrator).
 *
 * V1.111 nested a "Canvas" group (Outline / World KB / Strategy) under the
 * Creator tab. V1.117 removes that group (AC-P2-3): Outline + World KB fold
 * into the Creator group as resolver-driven canvas items; Strategy moves to the
 * Orchestration tab as a plain `/strategies` link (AC-P2-4). When a `workId` is
 * in the route (`/works/:workId/*`), the sidebar enters **drill-in mode**
 * (AD-P2-1): the Creator/Orchestrator tabs are hidden and only the
 * work-context skeleton (Back to all / Outline / Body) is shown.
 *
 * Thin wrapper around {@link ShellSidebarChrome}: owns NavLink, the active
 * creator profile, and the route-derived active state. The chrome owns the
 * markup, classes, and `data-testid` SSOT.
 *
 * Active-highlight note (V1.111): the chrome derives each item's active state
 * from a built-in `activeRoute` prefix match. That match is correct for
 * non-canvas items but TOO BROAD for Canvas items — e.g. "Outline"
 * (`to: '/works'`) would false-light on plain `/works/:workId` work-detail. The
 * host passes an `isActiveItem` callback so the chrome's per-item active state
 * is resolver-driven for Canvas items (via {@link resolveActiveCanvasSurface})
 * and prefix-driven for non-canvas items — keeping the chrome's markup SSOT
 * with no mirrored item markup in the host.
 *
 * Navigation note (V1.111 T3): Canvas items don't navigate to their static
 * `to` (a list-picker identity); the click target is computed by
 * {@link resolveCanvasNavTarget} from the URL-scoped Work/World context so the
 * active context is preserved (e.g. Outline on a Work route →
 * `/works/:workId/outline`, not `/works`). Every Canvas surface has a valid
 * fallback target (World KB → `/worlds` picker), so items are always
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

  // V1.117 AD-P2-1: when a workId is in the route (/works/:workId/*), the
  // sidebar enters drill-in mode — the Creator/Orchestrator tabs are hidden and
  // only the work-context skeleton (Back to all / Outline / Body) is shown.
  // Derived from the existing workId param; no new route params (V1.111 locks).
  const isDrillIn = Boolean(workId);

  const creatorGroups: ShellNavGroup[] = useMemo(
    () => [
      {
        id: 'works',
        label: t('nav.works'),
        items: [{ to: '/works', label: t('nav.allWorks'), icon: Layers }],
      },
      {
        // V1.117 regroup (AC-P2-3 / AC-P2-5): the "Canvas" group label is gone;
        // Outline + World KB fold into the Creator group alongside Memory. They
        // stay resolver-driven canvas items (context-aware targets); Memory is a
        // plain link. The three destinations (/works, /worlds, /memory) are
        // unique, so the chrome's per-group `key={item.to}` has no collision —
        // Outline's `/works` lives in this group, distinct from All Works'
        // `/works` in the "works" group above.
        id: 'creator',
        label: t('nav.creator'),
        items: [
          ...CANVAS_ITEMS.map((item) => ({
            ...item,
            label: t(`nav.${surfaceKey(item.surfaceId)}`),
          })),
          { to: '/memory', label: t('nav.memory'), icon: BrainCircuit },
        ],
      },
    ],
    [t],
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
        // Strategy lives under Orchestration (AC-P2-4) — a plain /strategies
        // link. It was a Canvas item in V1.111; V1.117 drops the canvas
        // resolver path for it (the prefix match highlights /strategies/*).
        id: 'strategies',
        label: t('nav.strategies'),
        items: [{ to: '/strategies', label: t('nav.strategies'), icon: Sparkles }],
      },
    ],
    [t],
  );

  const groups = activeTab === 'creator' ? creatorGroups : orchestratorGroups;

  // Drill-in skeleton (AD-P2-1): three flat links that replace the tab+group
  // nav while inside a work. Targets are built from the URL-scoped workId
  // (encoded like the canvas resolver).
  const drillInItems = useMemo(() => {
    if (!workId) return undefined;
    const encoded = encodeURIComponent(workId);
    return [
      { to: '/works', label: t('nav.drillIn.backToAll'), icon: ArrowLeft },
      { to: `/works/${encoded}/outline`, label: t('nav.drillIn.outline'), icon: ListTree },
      { to: `/works/${encoded}/chapters`, label: t('nav.drillIn.body'), icon: BookOpen },
    ];
  }, [workId, t]);

  return (
    <nav aria-label={t('aria.primary')} className="min-h-0 flex-1">
      <ShellSidebarChrome
        activeTab={activeTab}
        activeRoute={pathname}
        settingsActive={pathname.startsWith('/settings')}
        navGroups={groups}
        drillInItems={isDrillIn ? drillInItems : undefined}
        onTabChange={setActiveTab}
        logo={<NexusLogo />}
        footer={<FooterProfiles />}
        creatorTabLabel={t('nav.creator')}
        orchestratorTabLabel={t('nav.orchestrator')}
        settingsLabel={t('nav.settings')}
        primaryNavigationAriaLabel={t('aria.primaryNavigation')}
        isActiveItem={(item, route) => {
          // Drill-in: Back-to-all is a "go back" action, never the current
          // location inside a work — exact match so `/works` never lights up on
          // `/works/:workId/*`. The two surface links use the standard prefix
          // match so their sub-routes stay highlighted.
          if (isDrillIn) {
            if (item.to === '/works') return route === '/works';
            return route === item.to || route.startsWith(`${item.to}/`);
          }
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
          // Canvas items + drill-in links use `<Link>` with host-owned
          // aria-current so react-router's own prefix detection can't
          // false-light items. For Canvas items the static `item.to` is only
          // the chrome-keyed identity — the real destination is the
          // context-aware resolver target. For drill-in links the target IS
          // `item.to`, but `<Link>` is still used so "Back to all" (`/works`)
          // doesn't prefix-match every `/works/:workId/*` route. A `null`
          // resolver target means no valid entry point — currently no Canvas
          // surface returns null, but the disabled branch is retained for
          // safety.
          if (isDrillIn || 'surfaceId' in item) {
            const target =
              'surfaceId' in item
                ? resolveCanvasNavTarget((item as CanvasNavItem).surfaceId, { workId, worldId })
                : item.to;
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

          // Non-canvas normal item — chrome-driven active state, verbatim
          // (V1.94 behavior).
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
