import { NavLink } from 'react-router-dom';
import {
  AlertTriangle,
  Boxes,
  BrainCircuit,
  CalendarClock,
  Layers,
  ListChecks,
  NotebookText,
  Sparkles,
  Workflow,
  type LucideIcon,
} from 'lucide-react';

import { cn } from '@/lib/utils';

/**
 * Sidebar nav — DESIGN.md §Component Primitives/Sidebar Nav.
 *
 * Width 248px, background-100, divider gray-alpha-400. Item height 36px,
 * radius-control, label-14. Active item: gray-alpha-100 fill + gray-1000 text
 * + left blue-700 bar. The six MVP screen groups (web-ui.md §6) — collapsed to
 * top navigation below `lg` (handled by the root layout, not here).
 */
interface NavItem {
  to: string;
  label: string;
  icon: LucideIcon;
}

export const NAV_ITEMS: NavItem[] = [
  { to: '/works', label: 'Works', icon: Layers },
  { to: '/works/chapters', label: 'Chapters', icon: NotebookText },
  { to: '/sessions', label: 'Sessions', icon: ListChecks },
  { to: '/schedule', label: 'Schedule', icon: CalendarClock },
  { to: '/capabilities', label: 'Capabilities', icon: Boxes },
  { to: '/findings', label: 'Findings', icon: AlertTriangle },
  { to: '/memory', label: 'Memory', icon: BrainCircuit },
  { to: '/presets', label: 'Presets', icon: Sparkles },
  { to: '/strategy', label: 'Strategy', icon: Workflow },
];

export function Sidebar() {
  return (
    <nav aria-label="Primary" className="flex h-full w-full flex-col gap-1 p-3">
      <div className="flex h-12 items-center px-3">
        <span className="text-heading-16 font-heading tracking-tight text-gray-1000">Nexus</span>
      </div>
      <div className="my-2 h-px bg-gray-alpha-400" role="separator" />
      <ul className="flex flex-col gap-1">
        {NAV_ITEMS.map(({ to, label, icon: Icon }) => (
          <li key={to}>
            <NavLink
              to={to}
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
                  <span>{label}</span>
                </>
              )}
            </NavLink>
          </li>
        ))}
      </ul>
    </nav>
  );
}
