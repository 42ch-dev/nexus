import { useState } from 'react';
import { NavLink, useLocation } from 'react-router-dom';
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
 * Sidebar nav — V1.94 two-tab IA (Creator | Orchestrator).
 *
 * Thin wrapper around {@link ShellSidebarChrome}: owns NavLink, the active
 * creator profile, and the route-derived active state. The chrome owns the
 * markup, classes, and `data-testid` SSOT.
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
