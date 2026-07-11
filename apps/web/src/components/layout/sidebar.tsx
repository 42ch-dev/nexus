import { useState } from 'react';
import { Link, NavLink, useLocation } from 'react-router-dom';
import {
  Boxes,
  BrainCircuit,
  CalendarClock,
  Layers,
  ListChecks,
  Sparkles,
} from 'lucide-react';

import { NexusLogo } from '@/components/brand/nexus-logo';
import { FooterProfiles } from '@/components/layout/footer-profiles';
import {
  CANVAS_NAV_GROUP,
  resolveActiveCanvasSurface,
  type CanvasNavItem,
} from '@/components/layout/canvas-nav';
import {
  ShellSidebarChrome,
  type ShellNavGroup,
  type ShellSidebarTab,
} from '@/components/layout/presentational/shell-sidebar-chrome';
import { cn } from '@/lib/utils';

const CREATOR_GROUPS: ShellNavGroup[] = [
  {
    id: 'works',
    label: 'Works',
    items: [{ to: '/works', label: 'All Works', icon: Layers }],
  },
  // Canvas group — the three canvas-surface entry points (Outline / World KB /
  // Strategy), nested under the Creator (Works) tab. Active-surface highlight
  // for these items is resolver-driven (see `isActiveItem` below), NOT the
  // chrome's built-in prefix match.
  CANVAS_NAV_GROUP,
  {
    id: 'creator',
    label: 'Creator',
    items: [{ to: '/memory', label: 'Memory', icon: BrainCircuit }],
  },
];

const ORCHESTRATOR_GROUPS: ShellNavGroup[] = [
  {
    id: 'runtime',
    label: 'Runtime',
    items: [
      { to: '/sessions', label: 'Sessions', icon: ListChecks },
      { to: '/schedule', label: 'Schedule', icon: CalendarClock },
      { to: '/capabilities', label: 'Capabilities', icon: Boxes },
    ],
  },
  {
    id: 'strategies',
    label: 'Strategies',
    items: [{ to: '/strategies', label: 'Strategies', icon: Sparkles }],
  },
];

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
 */
export function Sidebar() {
  const [activeTab, setActiveTab] = useState<ShellSidebarTab>('creator');
  const { pathname } = useLocation();
  const groups = activeTab === 'creator' ? CREATOR_GROUPS : ORCHESTRATOR_GROUPS;

  return (
    <nav aria-label="Primary">
      <ShellSidebarChrome
        activeTab={activeTab}
        activeRoute={pathname}
        settingsActive={pathname.startsWith('/settings')}
        navGroups={groups}
        onTabChange={setActiveTab}
        logo={<NexusLogo />}
        footer={<FooterProfiles />}
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
          // Canvas items use <Link> (not <NavLink>) so react-router's own
          // prefix detection can't re-introduce the false positive the
          // resolver exists to fix. aria-current is host-owned for these.
          if ('surfaceId' in item) {
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
