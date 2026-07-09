import { NavLink } from 'react-router-dom';

/**
 * Primary navigation skeleton for the Design Studio.
 *
 * Five gallery routes per IA guide §3: Tokens, Brand, Components, Voice,
 * Surfaces. Surfaces keeps nested Studio-only section routes under
 * `/surfaces/*` (V1.102 P2); the top-nav Surfaces link stays active for all
 * of them.
 */
const NAV_ITEMS = [
  { label: 'Tokens', path: '/tokens' },
  { label: 'Brand', path: '/brand' },
  { label: 'Components', path: '/components' },
  { label: 'Voice', path: '/voice' },
  { label: 'Surfaces', path: '/surfaces' },
] as const;

export function TopNav() {
  return (
    <nav aria-label="Gallery sections" className="flex items-center gap-1">
      {NAV_ITEMS.map(({ label, path }) => (
        <NavLink
          key={path}
          to={path}
          className={({ isActive }) =>
            `px-3 py-1.5 rounded-md text-label-14 transition-colors ${
              isActive
                ? 'bg-gray-alpha-200 text-gray-1000 font-medium'
                : 'text-gray-700 hover:text-gray-1000 hover:bg-gray-alpha-100'
            }`
          }
        >
          {label}
        </NavLink>
      ))}
    </nav>
  );
}
