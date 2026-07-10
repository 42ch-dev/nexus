/**
 * Settings shell layout — V1.103 P0 (settings-shell-ia.md) + V1.104 W2.
 *
 * Owns page title/helper, secondary section nav, and `<Outlet />`.
 * Section bodies live in sibling route modules. Workspace nav added in V1.104.
 */

import { Bot, FolderOpen, Settings, type LucideIcon } from 'lucide-react';
import { NavLink, Outlet } from 'react-router-dom';

import { cn } from '@/lib/utils';

const SHELL_HELPER =
  'Manage your local agent, daemon connection, and setup options from one place.';

/** V1.106 P2 three-tab nav — Agent / Workspace / Advanced. */
const SETTINGS_SECTIONS: {
  id: 'agent' | 'workspace' | 'advanced';
  label: string;
  to: string;
  icon: LucideIcon;
}[] = [
  { id: 'agent', label: 'Agent', to: '/settings/agent', icon: Bot },
  {
    id: 'workspace',
    label: 'Workspace',
    to: '/settings/workspace',
    icon: FolderOpen,
  },
  {
    id: 'advanced',
    label: 'Advanced',
    to: '/settings/advanced',
    icon: Settings,
  },
];

export function SettingsShellLayout() {
  return (
    <div
      className="flex flex-col gap-6 max-w-2xl w-full"
      data-testid="settings-shell"
    >
      <div className="flex flex-col gap-2">
        {/* Visual page title; document h1 lives in RootLayout Header. */}
        <h2 className="text-heading-24 font-heading text-gray-1000">Settings</h2>
        <p className="text-copy-14 text-gray-900">{SHELL_HELPER}</p>
      </div>

      <nav
        aria-label="Settings sections"
        className="flex flex-wrap gap-1 border-b border-gray-alpha-200 pb-px"
        data-testid="settings-section-nav"
      >
        {SETTINGS_SECTIONS.map(({ id, label, to, icon: Icon }) => (
          <NavLink
            key={id}
            to={to}
            data-testid={`settings-section-nav-${id}`}
            className={({ isActive }) =>
              cn(
                'inline-flex items-center gap-2 px-3 py-2 text-label-14 font-medium',
                'border-b-2 -mb-px transition-colors duration-state ease-standard',
                isActive
                  ? 'text-gray-1000 border-blue-700'
                  : 'text-gray-700 border-transparent hover:text-gray-1000 hover:border-gray-alpha-400',
              )
            }
          >
            <Icon className="size-4 shrink-0" aria-hidden="true" />
            <span>{label}</span>
          </NavLink>
        ))}
      </nav>

      <div data-testid="settings-shell-outlet">
        <Outlet />
      </div>
    </div>
  );
}
