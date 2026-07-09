import { useState } from 'react';
import { NavLink } from 'react-router-dom';
import {
  Boxes,
  BrainCircuit,
  CalendarClock,
  ChevronDown,
  ChevronRight,
  Layers,
  ListChecks,
  Settings,
  Sparkles,
  type LucideIcon,
} from 'lucide-react';

import { NexusLogo } from '@/components/brand/nexus-logo';
import { FooterProfiles } from '@/components/layout/footer-profiles';
import { cn } from '@/lib/utils';

type TabId = 'creator' | 'orchestrator';

interface NavGroup {
  id: string;
  label: string;
  defaultOpen?: boolean;
  items: NavItem[];
}

interface NavItem {
  to: string;
  label: string;
  icon: LucideIcon;
}

const CREATOR_GROUPS: NavGroup[] = [
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

const ORCHESTRATOR_GROUPS: NavGroup[] = [
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
 * Width 248px, background-100, divider gray-alpha-400. Footer profile switcher
 * is always visible at the bottom of the sidebar. Active item uses DESIGN.md
 * tokens: gray-alpha-100 fill + gray-1000 text + left blue-700 bar.
 */
export function Sidebar() {
  const [activeTab, setActiveTab] = useState<TabId>('creator');
  const groups = activeTab === 'creator' ? CREATOR_GROUPS : ORCHESTRATOR_GROUPS;

  return (
    <nav
      aria-label="Primary"
      className="flex h-full w-full flex-col gap-2 border-r border-gray-alpha-400 bg-background-100 p-3"
    >
      <div className="flex h-12 items-center px-3">
        <NexusLogo />
      </div>

      <div
        className="grid grid-cols-2 gap-1 rounded-card bg-gray-alpha-100 p-1"
        role="tablist"
        aria-label="Primary navigation"
      >
        <TabButton id="creator" label="Creator" active={activeTab === 'creator'} onClick={setActiveTab} />
        <TabButton
          id="orchestrator"
          label="Orchestrator"
          active={activeTab === 'orchestrator'}
          onClick={setActiveTab}
        />
      </div>

      <div className="my-1 h-px bg-gray-alpha-400" role="separator" />

      <ul
        className="flex flex-1 flex-col gap-4 overflow-auto py-1"
        role="tabpanel"
        aria-labelledby={activeTab}
      >
        {groups.map((group) => (
          <NavGroup key={group.id} group={group} />
        ))}
      </ul>

      <div className="mt-auto border-t border-gray-alpha-400 pt-3 flex flex-col gap-2">
        {/* Footer utility — Settings is cross-cutting (not tab-scoped). */}
        <NavLink
          to="/settings"
          data-testid="settings-footer-utility-link"
          className={({ isActive }) =>
            cn(
              'group relative flex h-9 items-center gap-2 rounded-control px-3 text-label-14 transition-colors duration-state ease-standard',
              isActive
                ? 'bg-gray-alpha-100 text-gray-1000'
                : 'text-gray-800 hover:bg-gray-alpha-100 hover:text-gray-1000',
            )
          }
        >
          {({ isActive }) => (
            <>
              {isActive && (
                <span
                  aria-hidden
                  className="absolute left-0 top-1/2 h-5 w-[3px] -translate-y-1/2 rounded-pill bg-blue-700"
                />
              )}
              <Settings className="h-4 w-4 shrink-0" aria-hidden />
              <span>Settings</span>
            </>
          )}
        </NavLink>
        <FooterProfiles />
      </div>
    </nav>
  );
}

function TabButton({
  id,
  label,
  active,
  onClick,
}: {
  id: TabId;
  label: string;
  active: boolean;
  onClick: (id: TabId) => void;
}) {
  return (
    <button
      type="button"
      id={id}
      role="tab"
      aria-selected={active}
      onClick={() => onClick(id)}
      className={cn(
        'rounded-control px-2 py-1.5 text-button-14 font-button transition-colors',
        active
          ? 'bg-background-100 text-gray-1000 shadow-card'
          : 'text-gray-700 hover:bg-gray-alpha-200 hover:text-gray-1000',
      )}
    >
      {label}
    </button>
  );
}

function NavGroup({ group }: { group: NavGroup }) {
  const [open, setOpen] = useState(group.defaultOpen ?? true);
  const hasMultiple = group.items.length > 1;

  return (
    <li className="flex flex-col gap-1">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        className="flex items-center gap-1 px-3 py-1 text-label-12 font-medium uppercase tracking-wide text-gray-700"
      >
        {hasMultiple && (open ? (
          <ChevronDown className="h-3.5 w-3.5" aria-hidden />
        ) : (
          <ChevronRight className="h-3.5 w-3.5" aria-hidden />
        ))}
        {group.label}
      </button>
      {open && (
        <ul className="flex flex-col gap-1">
          {group.items.map((item) => (
            <NavItemLink key={item.to} item={item} />
          ))}
        </ul>
      )}
    </li>
  );
}

function NavItemLink({ item }: { item: NavItem }) {
  const Icon = item.icon;
  return (
    <li>
      <NavLink
        to={item.to}
        className={({ isActive }) =>
          cn(
            'group relative flex h-9 items-center gap-2 rounded-control px-3 text-label-14 transition-colors duration-state ease-standard',
            isActive
              ? 'bg-gray-alpha-100 text-gray-1000'
              : 'text-gray-800 hover:bg-gray-alpha-100 hover:text-gray-1000',
          )
        }
      >
        {({ isActive }) => (
          <>
            {isActive && (
              <span
                aria-hidden
                className="absolute left-0 top-1/2 h-5 w-[3px] -translate-y-1/2 rounded-pill bg-blue-700"
              />
            )}
            <Icon className="h-4 w-4 shrink-0" aria-hidden />
            <span>{item.label}</span>
          </>
        )}
      </NavLink>
    </li>
  );
}
