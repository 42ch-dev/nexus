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
  Globe,
  Layers,
  ListChecks,
  ListTree,
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
 * Outline / World KB remain reachable via drill-in (work shell) and the command
 * palette; Strategy stays under Orchestrator (V1.117 AC-P2-4). When a `workId`
 * is in the route (`/works/:workId/*`), the sidebar enters **drill-in mode**
 * (AD-P2-1): the Creator/Orchestrator tabs are hidden and only the
 * work-context skeleton (Back to all / Outline / Body) is shown.
 *
 * Thin wrapper around {@link ShellSidebarChrome}: owns NavLink, the active
 * creator profile, and the route-derived active state. The chrome owns the
 * markup, classes, and `data-testid` SSOT.
 *
 * Active-highlight note: list-mode peer items use prefix matching via
 * {@link ShellSidebarChrome}'s `isActiveItem` callback. Drill-in links use
 * host-owned `aria-current` so "Back to all" never false-lights inside a work.
 */
export function Sidebar() {
  const { t } = useTranslation('shell');
  const [activeTab, setActiveTab] = useState<ShellSidebarTab>('creator');
  const { pathname } = useLocation();
  // Sidebar lives in RootLayout (a layout route), so useParams returns the leaf
  // route's params — workId is populated on Work-scoped routes and undefined
  // elsewhere.
  const { workId } = useParams<{ workId?: string }>();
  const worksQuery = useWorks({ limit: 12 });
  const works = useMemo(() => flattenPages(worksQuery.data), [worksQuery.data]);

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
          return route === item.to || route.startsWith(`${item.to}/`);
        }}
        renderNavItem={(item, className, content, isActive) => {
          // Drill-in links use `<Link>` with host-owned aria-current so "Back
          // to all" (`/works`) doesn't prefix-match every `/works/:workId/*`
          // route.
          if (isDrillIn) {
            return (
              <Link
                to={item.to}
                aria-current={isActive ? 'page' : undefined}
                className={className}
              >
                {content}
              </Link>
            );
          }

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
