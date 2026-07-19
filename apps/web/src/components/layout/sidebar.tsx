import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { NavLink, useLocation } from 'react-router-dom';
import {
  BookOpen,
  BrainCircuit,
  CalendarClock,
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
 * V1.125 P2 rewrites list-mode Creation IA into Worlds-first peer groups —
 * Worlds, then Works — with Timeline peer groups removed (deep links retained).
 * V1.118 P2 keeps Creator | Orchestrator tabs visible inside work routes
 * (AC-P2-5); enter-work UX is the canvas-first shell + right rail, not
 * whole-left drill-in.
 *
 * Thin wrapper around {@link ShellSidebarChrome}: owns NavLink, the active
 * creator profile, and the route-derived active state. The chrome owns the
 * markup, classes, and `data-testid` SSOT.
 *
 * V1.125 P1 moves Memory under Orchestrator (first group) and derives the
 * active tab from orchestration route prefixes so deep links select the right tab.
 *
 * Active-highlight note: peer items use prefix matching via
 * {@link ShellSidebarChrome}'s `isActiveItem` callback.
 */
const ORCHESTRATOR_ROUTE_PREFIXES = [
  '/memory',
  '/strategies',
  '/sessions',
  '/schedule',
  '/modules',
] as const;

function tabFromPathname(pathname: string): ShellSidebarTab {
  return ORCHESTRATOR_ROUTE_PREFIXES.some(
    (prefix) => pathname === prefix || pathname.startsWith(`${prefix}/`),
  )
    ? 'orchestrator'
    : 'creator';
}

export function Sidebar() {
  const { t } = useTranslation('shell');
  const { pathname } = useLocation();
  const [activeTab, setActiveTab] = useState<ShellSidebarTab>(() => tabFromPathname(pathname));
  const worksQuery = useWorks({ limit: 12 });
  const works = useMemo(() => flattenPages(worksQuery.data), [worksQuery.data]);

  useEffect(() => {
    setActiveTab(tabFromPathname(pathname));
  }, [pathname]);

  const creatorGroups: ShellNavGroup[] = useMemo(
    () => [
      // V1.125 P2 — Worlds-first Creator IA (AC-V1125-5). Timeline and Work
      // Timelines peer groups are removed; `/timeline` and
      // `/works/:id/timeline` remain deep-linkable via command palette and
      // in-surface navigation.
      {
        id: 'worlds',
        label: t('nav.worlds'),
        items: [{ to: '/worlds', label: t('nav.worlds'), icon: Globe }],
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
    ],
    [t, works],
  );

  const orchestratorGroups: ShellNavGroup[] = useMemo(
    () => [
      {
        id: 'memory',
        label: t('nav.memory'),
        items: [{ to: '/memory', label: t('nav.memory'), icon: BrainCircuit }],
      },
      {
        id: 'strategies',
        label: t('nav.strategies'),
        items: [{ to: '/strategies', label: t('nav.strategies'), icon: Sparkles }],
      },
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
    ],
    [t],
  );

  const groups = activeTab === 'creator' ? creatorGroups : orchestratorGroups;

  return (
    <nav aria-label={t('aria.primary')} className="min-h-0 flex-1">
      <ShellSidebarChrome
        activeTab={activeTab}
        activeRoute={pathname}
        navGroups={groups}
        onTabChange={setActiveTab}
        logo={<NexusLogo />}
        footer={<FooterProfiles />}
        creatorTabLabel={t('nav.creator')}
        orchestratorTabLabel={t('nav.orchestrator')}
        primaryNavigationAriaLabel={t('aria.primaryNavigation')}
        isActiveItem={(item, route) => {
          if (item.to === '/works') return route === '/works';
          // Per-Work Outline row — claim Outline + sibling surfaces under the
          // Work (e.g. /chapters, /body, /timeline) so the Work stays visually
          // selected while the author moves between surfaces.
          const outlineMatch = /^\/works\/([^/]+)\/outline$/.exec(item.to);
          if (outlineMatch) {
            const encodedWorkId = outlineMatch[1];
            if (route === item.to) return true;
            return route.startsWith(`/works/${encodedWorkId}/`);
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
      />
    </nav>
  );
}

// Re-export the chrome types for convenience.
export type { ShellNavGroup, ShellSidebarTab };
