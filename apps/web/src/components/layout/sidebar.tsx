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
  // for these items is resolver-driven (see `renderNavItem` below), NOT the
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
 * false-light on plain `/works/:workId` work-detail. The Canvas group's active
 * surface derives from {@link resolveActiveCanvasSurface} instead. The chrome
 * has no per-item active-override hook, so Canvas items are rendered here with
 * resolver-driven fill + active bar + `aria-current`. The markup mirrors
 * `NavGroupChrome` in `shell-sidebar-chrome.tsx` — keep the two in sync (and
 * see `task-2-report.md` for the recommended chrome follow-up).
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
        renderNavItem={(item, className, content, isActive) => {
          if ('surfaceId' in item) {
            // Canvas item — active state from the resolver, not the chrome's
            // prefix match. `<Link>` is used because we own the active state
            // here; NavLink's own prefix detection would re-introduce the
            // false positive the resolver exists to fix.
            const canvasItem = item as CanvasNavItem;
            const active = resolveActiveCanvasSurface(pathname) === canvasItem.surfaceId;
            return (
              <Link
                to={canvasItem.to}
                aria-current={active ? 'page' : undefined}
                // Mirror shell-sidebar-chrome.tsx NavGroupChrome (active/inactive
                // branches) — keep in sync.
                className={cn(
                  'group relative flex h-sidebar-nav-item-height items-center gap-2 rounded-control px-3 text-label-14 transition-colors duration-state ease-standard',
                  active
                    ? 'bg-gray-alpha-100 text-gray-1000'
                    : 'text-gray-600 hover:bg-gray-alpha-100 hover:text-gray-900',
                )}
              >
                {active && (
                  <span
                    aria-hidden
                    data-testid="sidebar-active-bar"
                    className="absolute left-0 top-1/2 h-5 w-[2px] -translate-y-1/2 rounded-pill bg-blue-700"
                  />
                )}
                <canvasItem.icon
                  className={cn('h-4 w-4 shrink-0', active ? 'opacity-100' : 'opacity-70')}
                  aria-hidden
                />
                <span>{canvasItem.label}</span>
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
