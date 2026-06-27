import { NavLink, Outlet, useLocation } from 'react-router-dom';

import { DaemonStatusBar } from '@/components/layout/daemon-status-bar';
import { Header } from '@/components/layout/header';
import { NAV_ITEMS } from '@/components/layout/sidebar';
import { Sidebar } from '@/components/layout/sidebar';
import { cn } from '@/lib/utils';

const ROUTE_TITLES: Record<string, string> = {
  '/works': 'Works',
  '/sessions': 'Sessions',
  '/schedule': 'Schedule',
  '/capabilities': 'Capabilities',
  '/findings': 'Findings',
  '/presets': 'Presets',
  '/strategy': 'Strategy',
};

/** Resolve the header title from the active top-level route. */
function useRouteTitle(): string {
  const { pathname } = useLocation();
  const top = `/${pathname.split('/')[1] ?? ''}`;
  return ROUTE_TITLES[top] ?? 'Control Room';
}

/**
 * Root layout — DESIGN.md §Spacing/Layout Rules.
 *
 * Fixed 248px sidebar at `lg` and above; collapses to a horizontal top nav
 * below `lg`. Main content max-width 1200px with 24px desktop / 16px mobile
 * side padding.
 */
export function RootLayout() {
  const title = useRouteTitle();

  return (
    <div className="flex min-h-screen bg-background-100 text-gray-1000">
      {/* Desktop sidebar */}
      <aside className="hidden w-[248px] shrink-0 border-r border-gray-alpha-400 bg-background-100 lg:block">
        <Sidebar />
      </aside>

      {/* Main column */}
      <div className="flex min-w-0 flex-1 flex-col">
        {/* Mobile top nav (below lg) */}
        <nav
          aria-label="Primary"
          className="flex gap-1 overflow-x-auto border-b border-gray-alpha-400 bg-background-100 px-2 py-2 lg:hidden"
        >
          {NAV_ITEMS.map(({ to, label }) => (
            <NavLink
              key={to}
              to={to}
              className={({ isActive }) =>
                cn(
                  'whitespace-nowrap rounded-control px-3 py-1.5 text-label-14 transition-colors duration-state ease-standard',
                  isActive
                    ? 'bg-gray-alpha-100 text-gray-1000'
                    : 'text-gray-800 hover:bg-gray-alpha-100 hover:text-gray-1000',
                )
              }
            >
              {label}
            </NavLink>
          ))}
        </nav>

        <Header title={title} />

        <main className="flex-1 overflow-y-auto">
          <div className="mx-auto w-full max-w-[1200px] px-4 py-6 md:px-6 md:py-8">
            <Outlet />
          </div>
        </main>

        <DaemonStatusBar />
      </div>
    </div>
  );
}
