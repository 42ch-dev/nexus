import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { NavLink, useLocation } from 'react-router-dom';
import {
  BookOpen,
  BrainCircuit,
  CalendarClock,
  CalendarRange,
  Cpu,
  Globe,
  Layers,
  ListChecks,
  Sparkles,
} from 'lucide-react';

import { flattenPages, useWorks } from '@/api/queries';
import { NexusLogo } from '@/components/brand/nexus-logo';
import { FooterProfiles } from '@/components/layout/footer-profiles';
import {
  ShellSidebarChrome,
  type ShellNavGroup,
  type ShellSidebarTab,
} from '@/components/layout/presentational/shell-sidebar-chrome';
import { cn } from '@/lib/utils';

/**
 * Sidebar nav — V1.94 two-tab IA (Creator | Orchestrator).
 *
 * V1.118 P1 rewrites list-mode Creation IA into three peer groups — Works,
 * Worlds, Memories — with no Creator meta-group mixing canvas surfaces.
 * V1.118 P2 keeps Creator | Orchestrator tabs visible inside work routes
 * (AC-P2-5); enter-work UX is the canvas-first shell + right rail, not
 * whole-left drill-in.
 *
 * Thin wrapper around {@link ShellSidebarChrome}: owns NavLink, the active
 * creator profile, and the route-derived active state. The chrome owns the
 * markup, classes, and `data-testid` SSOT.
 *
 * Active-highlight note: peer items use prefix matching via
 * {@link ShellSidebarChrome}'s `isActiveItem` callback.
 */
export function Sidebar() {
  const { t } = useTranslation('shell');
  const [activeTab, setActiveTab] = useState<ShellSidebarTab>('creator');
  const { pathname } = useLocation();
  const worksQuery = useWorks({ limit: 12 });
  const works = useMemo(() => flattenPages(worksQuery.data), [worksQuery.data]);

  const creatorGroups: ShellNavGroup[] = useMemo(
    () => [
      // V1.123 P3 Task 1 — Timeline is the central instrument per
      // `iterations/v1.123/specs/three-layer-product-spec.md`. Pinning it as
      // the FIRST Creator-tab entry gives the global Timeline view structural
      // prominence ("Timeline 一定要突出") over the Works / Worlds / Memories
      // peers. The route is `/timeline` (cross-World overview); per-World
      // Timeline stays at `/worlds/:worldId/timeline` (V1.122 P1 T3 hero).
      {
        id: 'timeline',
        label: t('nav.timeline'),
        items: [{ to: '/timeline', label: t('nav.timeline'), icon: CalendarRange }],
      },
      {
        id: 'works',
        label: t('nav.works'),
        items: [
          { to: '/works', label: t('nav.allWorks'), icon: Layers },
          ...works.map((work) => ({
            to: `/works/${encodeURIComponent(work.work_id)}/outline`,
            label: work.title,
            icon: BookOpen,
          })),
        ],
      },
      {
        id: 'worlds',
        label: t('nav.worlds'),
        items: [{ to: '/worlds', label: t('nav.worlds'), icon: Globe }],
      },
      {
        id: 'memories',
        label: t('nav.memories'),
        items: [{ to: '/memory', label: t('nav.memories'), icon: BrainCircuit }],
      },
    ],
    [t, works],
  );

  const orchestratorGroups: ShellNavGroup[] = useMemo(
    () => [
      {
        id: 'runtime',
        label: t('nav.runtime'),
        items: [
          { to: '/sessions', label: t('nav.sessions'), icon: ListChecks },
          { to: '/schedule', label: t('nav.schedule'), icon: CalendarClock },
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
          if (item.to === '/works') return route === '/works';
          // Peer work rows link to outline; highlight on any surface under that work.
          const workPeerMatch = /^\/works\/([^/]+)\/outline$/.exec(item.to);
          if (workPeerMatch) {
            const encodedWorkId = workPeerMatch[1];
            return (
              route === item.to || route.startsWith(`/works/${encodedWorkId}/`)
            );
          }
          return route === item.to || route.startsWith(`${item.to}/`);
        }}
        renderNavItem={(item, className, content, isActive) => (
          <NavLink
            to={item.to}
            className={cn(className, isActive ? 'bg-gray-alpha-100 text-gray-1000' : undefined)}
          >
            {content}
          </NavLink>
        )}
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
