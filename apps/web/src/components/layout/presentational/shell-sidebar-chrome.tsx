import { ChevronDown, ChevronRight, Settings, type LucideIcon } from 'lucide-react';
import { useState, type ReactNode } from 'react';

import { cn } from '@/lib/utils';

export type ShellSidebarTab = 'creator' | 'orchestrator';

export interface ShellNavItem {
  to: string;
  label: string;
  icon: LucideIcon;
}

export interface ShellNavGroup {
  id: string;
  label: string;
  defaultOpen?: boolean;
  items: ShellNavItem[];
}

/** Custom renderer for a nav item (receives the computed className, SSOT inner
 * content, and active state). The chrome always resolves a default before
 * passing it to its sub-renderers, so helpers take this as required. */
export type RenderNavItem = (
  item: ShellNavItem,
  className: string,
  content: ReactNode,
  isActive: boolean,
) => ReactNode;

export interface ShellSidebarChromeProps {
  activeTab: ShellSidebarTab;
  activeRoute: string;
  settingsActive: boolean;
  navGroups: ShellNavGroup[];
  onTabChange: (tab: ShellSidebarTab) => void;
  /** Optional logo slot — apps should pass their theme-aware wordmark. */
  logo?: ReactNode;
  /** Optional footer slot rendered below the Settings utility (e.g. profile switcher). */
  footer?: ReactNode;
  renderNavItem?: RenderNavItem;
  /** Optional per-item active-state override. When provided, the chrome calls it
   * for every item INSTEAD of the built-in `activeRoute === item.to ||
   * activeRoute.startsWith(item.to + '/')` prefix match. Lets hosts that need
   * resolver-driven active state (e.g. Canvas surface matching) keep the
   * chrome's markup SSOT instead of mirroring it. When absent, the built-in
   * match is preserved (backward-compatible).
   */
  isActiveItem?: (item: ShellNavItem, activeRoute: string) => boolean;
  /**
   * Optional work-context drill-in mode (V1.117 AD-P2-1). When provided, the
   * chrome hides the Creator/Orchestrator tablist and renders these items as a
   * flat list (no group disclosures) in the scrollable nav region. The footer
   * (Settings + profiles) is unaffected. When absent, the normal tab + group
   * IA is rendered (backward-compatible).
   */
  drillInItems?: ShellNavItem[];
  /** Optional custom renderer for the Settings footer utility link. */
  renderSettingsLink?: (
    to: string,
    className: string,
    content: ReactNode,
    isActive: boolean,
  ) => ReactNode;
  /** Optional label for the Creator tab (defaults to English for fixtures). */
  creatorTabLabel?: string;
  /** Optional label for the Orchestrator tab (defaults to English for fixtures). */
  orchestratorTabLabel?: string;
  /** Optional label for the Settings footer utility (defaults to English). */
  settingsLabel?: string;
  /** Optional aria-label for the primary navigation tablist. */
  primaryNavigationAriaLabel?: string;
  /** Optional test id for the root sidebar chrome. */
  'data-testid'?: string;
}

/**
 * Presentational app shell sidebar — DESIGN.md §Sidebar Nav SSOT.
 *
 * No routing, no daemon hooks, no profile state. The host owns the active tab,
 * active route resolution, and link implementation (NavLink in the App wrapper).
 */
export function ShellSidebarChrome({
  activeTab,
  activeRoute,
  settingsActive,
  navGroups,
  onTabChange,
  logo,
  footer,
  renderNavItem = defaultRenderNavItem,
  renderSettingsLink = defaultRenderSettingsLink,
  isActiveItem,
  drillInItems,
  creatorTabLabel = 'Creator',
  orchestratorTabLabel = 'Orchestrator',
  settingsLabel = 'Settings',
  primaryNavigationAriaLabel = 'Primary navigation',
  'data-testid': dataTestId,
}: ShellSidebarChromeProps) {
  return (
    <div
      className="flex h-full w-full flex-col gap-2 border-r border-gray-alpha-400 bg-background-100 p-3"
      data-testid={dataTestId}
    >
      <div className="flex h-12 items-center px-3">{logo}</div>

      {drillInItems ? null : (
        <>
          <div
            className="grid grid-cols-2 gap-1 rounded-card bg-gray-alpha-100 p-1"
            role="tablist"
            aria-label={primaryNavigationAriaLabel}
          >
            <TabButton
              id="creator"
              label={creatorTabLabel}
              active={activeTab === 'creator'}
              onClick={() => onTabChange('creator')}
            />
            <TabButton
              id="orchestrator"
              label={orchestratorTabLabel}
              active={activeTab === 'orchestrator'}
              onClick={() => onTabChange('orchestrator')}
            />
          </div>

          <div className="my-1 h-px bg-gray-alpha-400" role="separator" />
        </>
      )}

      {drillInItems ? (
        <ul className="flex flex-1 flex-col gap-0.5 overflow-auto py-1">
          {drillInItems.map((item) => (
            <NavItemLi
              key={item.to}
              item={item}
              activeRoute={activeRoute}
              isActiveItem={isActiveItem}
              renderNavItem={renderNavItem}
            />
          ))}
        </ul>
      ) : (
        <ul
          className="flex flex-1 flex-col gap-4 overflow-auto py-1"
          role="tabpanel"
          aria-labelledby={activeTab}
        >
          {navGroups.map((group) => (
            <NavGroupChrome
              key={group.id}
              group={group}
              activeRoute={activeRoute}
              renderNavItem={renderNavItem}
              isActiveItem={isActiveItem}
            />
          ))}
        </ul>
      )}

      <div className="mt-auto flex flex-col gap-2 border-t border-gray-alpha-400 pt-3">
        {/* Footer utility — Settings is cross-cutting (not tab-scoped). */}
        {renderSettingsLink(
          '/settings',
          cn(
            'group relative flex h-sidebar-nav-item-height items-center gap-2 rounded-control px-3 text-label-14 transition-colors duration-state ease-standard',
            settingsActive
              ? 'bg-gray-alpha-100 text-gray-1000'
              : 'text-gray-600 hover:bg-gray-alpha-100 hover:text-gray-900',
          ),
          <>
            <Settings className="h-4 w-4 shrink-0" aria-hidden />
            <span>{settingsLabel}</span>
          </>,
          settingsActive,
        )}
        {footer}
      </div>
    </div>
  );
}

function defaultRenderNavItem(
  item: ShellNavItem,
  className: string,
  content: ReactNode,
): ReactNode {
  return (
    <a href={item.to} className={className}>
      {content}
    </a>
  );
}

function defaultRenderSettingsLink(
  to: string,
  className: string,
  content: ReactNode,
  isActive: boolean,
): ReactNode {
  return (
    <a
      href={to}
      className={className}
      data-testid="settings-footer-utility-link"
      aria-current={isActive ? 'page' : undefined}
    >
      {content}
    </a>
  );
}

function TabButton({
  id,
  label,
  active,
  onClick,
}: {
  id: ShellSidebarTab;
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      id={id}
      role="tab"
      aria-selected={active}
      onClick={onClick}
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

function NavGroupChrome({
  group,
  activeRoute,
  renderNavItem,
  isActiveItem,
}: {
  group: ShellNavGroup;
  activeRoute: string;
  renderNavItem: RenderNavItem;
  isActiveItem?: ShellSidebarChromeProps['isActiveItem'];
}) {
  const [open, setOpen] = useState(group.defaultOpen ?? true);
  const hasMultiple = group.items.length > 1;

  return (
    <li className="flex flex-col gap-1">
      {/* Parent = group/disclosure label only — no competing selected fill */}
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        className="flex items-center gap-1 px-3 py-1 text-label-12 font-medium uppercase tracking-wide text-gray-600"
      >
        {hasMultiple && (open ? (
          <ChevronDown className="h-3.5 w-3.5" aria-hidden />
        ) : (
          <ChevronRight className="h-3.5 w-3.5" aria-hidden />
        ))}
        {group.label}
      </button>
      {open && (
        <ul className="flex flex-col gap-0.5">
          {group.items.map((item) => (
            <NavItemLi
              key={item.to}
              item={item}
              activeRoute={activeRoute}
              isActiveItem={isActiveItem}
              renderNavItem={renderNavItem}
            />
          ))}
        </ul>
      )}
    </li>
  );
}

/**
 * Shared per-item `<li>` renderer — used by both group disclosures and the
 * flat drill-in list so item markup (active bar, icon, label, active classes)
 * stays in one place. The active state comes from `isActiveItem` when provided,
 * else the built-in `item.to` prefix match.
 */
function NavItemLi({
  item,
  activeRoute,
  isActiveItem,
  renderNavItem,
}: {
  item: ShellNavItem;
  activeRoute: string;
  isActiveItem?: ShellSidebarChromeProps['isActiveItem'];
  renderNavItem: RenderNavItem;
}) {
  const isActive = isActiveItem
    ? isActiveItem(item, activeRoute)
    : activeRoute === item.to || activeRoute.startsWith(`${item.to}/`);
  return (
    <li>
      {renderNavItem(
        item,
        cn(
          'group relative flex h-sidebar-nav-item-height items-center gap-2 rounded-control px-3 text-label-14 transition-colors duration-state ease-standard',
          isActive
            ? 'bg-gray-alpha-100 text-gray-1000'
            : 'text-gray-600 hover:bg-gray-alpha-100 hover:text-gray-900',
        ),
        <>
          {isActive && (
            <span
              aria-hidden
              data-testid="sidebar-active-bar"
              className="absolute left-0 top-1/2 h-5 w-[2px] -translate-y-1/2 rounded-pill bg-blue-700"
            />
          )}
          <item.icon
            className={cn('h-4 w-4 shrink-0', isActive ? 'opacity-100' : 'opacity-70')}
            aria-hidden
          />
          <span>{item.label}</span>
        </>,
        isActive,
      )}
    </li>
  );
}
