import { ChevronRight, Ellipsis, type LucideIcon } from 'lucide-react';
import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import { useLocation } from 'react-router-dom';

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

export type RenderNavItem = (
  item: ShellNavItem,
  className: string,
  content: ReactNode,
  isActive: boolean,
) => ReactNode;

export interface ShellSidebarChromeProps {
  activeTab: ShellSidebarTab;
  activeRoute: string;
  navGroups: ShellNavGroup[];
  onTabChange: (tab: ShellSidebarTab) => void;
  logo?: ReactNode;
  footer?: ReactNode;
  renderNavItem?: RenderNavItem;
  isActiveItem?: (item: ShellNavItem, activeRoute: string) => boolean;
  drillInItems?: ShellNavItem[];
  creatorTabLabel?: string;
  orchestratorTabLabel?: string;
  primaryNavigationAriaLabel?: string;
  'data-testid'?: string;
  renderSubmenu?: (
    item: ShellNavItem,
    close: () => void,
    anchorEl: HTMLElement,
  ) => ReactNode;
  hasSubmenu?: (item: ShellNavItem) => boolean;
}

export function ShellSidebarChrome({
  activeTab,
  activeRoute,
  navGroups,
  onTabChange,
  logo,
  footer,
  renderNavItem = defaultRenderNavItem,
  isActiveItem,
  drillInItems,
  creatorTabLabel = 'Creator',
  orchestratorTabLabel = 'Orchestrator',
  primaryNavigationAriaLabel = 'Primary navigation',
  'data-testid': dataTestId,
  renderSubmenu,
  hasSubmenu,
}: ShellSidebarChromeProps) {
  const [submenuItem, setSubmenuItem] = useState<ShellNavItem | null>(null);
  const [submenuAnchor, setSubmenuAnchor] = useState<HTMLElement | null>(null);
  const triggerRef = useRef<HTMLElement | null>(null);
  const { pathname } = useLocation();

  const closeSubmenu = useCallback(() => {
    setSubmenuItem(null);
    setSubmenuAnchor(null);
    triggerRef.current?.focus();
    triggerRef.current = null;
  }, []);

  useEffect(() => {
    closeSubmenu();
  }, [pathname, closeSubmenu]);

  const handleOpenSubmenu = useCallback(
    (item: ShellNavItem, el: HTMLElement) => {
      triggerRef.current = document.activeElement as HTMLElement;
      setSubmenuItem(item);
      setSubmenuAnchor(el);
    },
    [],
  );

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
              renderSubmenu={renderSubmenu}
              hasSubmenu={hasSubmenu}
              onOpenSubmenu={handleOpenSubmenu}
              isSubmenuOpenForItem={submenuItem?.to === item.to}
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
              renderSubmenu={renderSubmenu}
              hasSubmenu={hasSubmenu}
              onOpenSubmenu={handleOpenSubmenu}
              submenuItem={submenuItem}
            />
          ))}
        </ul>
      )}

      {submenuItem && renderSubmenu && submenuAnchor
        ? renderSubmenu(submenuItem, closeSubmenu, submenuAnchor)
        : null}

      {footer ? (
        <div className="mt-auto flex flex-col gap-2 border-t border-gray-alpha-400 pt-3">
          {footer}
        </div>
      ) : null}
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
        'rounded-control px-2 py-1.5 text-button-14 font-button transition-colors duration-state ease-standard motion-reduce:transition-none',
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
  renderSubmenu,
  hasSubmenu,
  onOpenSubmenu,
  submenuItem,
}: {
  group: ShellNavGroup;
  activeRoute: string;
  renderNavItem: RenderNavItem;
  isActiveItem?: ShellSidebarChromeProps['isActiveItem'];
  renderSubmenu?: ShellSidebarChromeProps['renderSubmenu'];
  hasSubmenu?: ShellSidebarChromeProps['hasSubmenu'];
  onOpenSubmenu?: (item: ShellNavItem, el: HTMLElement) => void;
  submenuItem?: ShellNavItem | null;
}) {
  const [open, setOpen] = useState(group.defaultOpen ?? true);
  const hasMultiple = group.items.length > 1;

  return (
    <li className="flex flex-col gap-1">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        className="flex items-center gap-1 px-3 py-1 text-label-12 font-medium uppercase tracking-wide text-gray-600 transition-colors duration-state ease-standard motion-reduce:transition-none hover:text-gray-900"
      >
        {hasMultiple && (
          <ChevronRight
            className={cn(
              'h-3.5 w-3.5 transition-transform duration-state ease-standard motion-reduce:transition-none',
              open && 'rotate-90',
            )}
            aria-hidden
          />
        )}
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
              renderSubmenu={renderSubmenu}
              hasSubmenu={hasSubmenu}
              onOpenSubmenu={onOpenSubmenu}
              isSubmenuOpenForItem={submenuItem?.to === item.to}
            />
          ))}
        </ul>
      )}
    </li>
  );
}

function NavItemLi({
  item,
  activeRoute,
  isActiveItem,
  renderNavItem,
  renderSubmenu,
  hasSubmenu: hasSubmenuProp,
  onOpenSubmenu,
  isSubmenuOpenForItem,
}: {
  item: ShellNavItem;
  activeRoute: string;
  isActiveItem?: ShellSidebarChromeProps['isActiveItem'];
  renderNavItem: RenderNavItem;
  renderSubmenu?: ShellSidebarChromeProps['renderSubmenu'];
  hasSubmenu?: ShellSidebarChromeProps['hasSubmenu'];
  onOpenSubmenu?: (item: ShellNavItem, el: HTMLElement) => void;
  isSubmenuOpenForItem?: boolean;
}) {
  const isActive = isActiveItem
    ? isActiveItem(item, activeRoute)
    : activeRoute === item.to || activeRoute.startsWith(`${item.to}/`);
  const hasSubmenu = !!(hasSubmenuProp ? hasSubmenuProp(item) : renderSubmenu);
  const submenuButtonRef = useRef<HTMLButtonElement>(null);

  function handleKeyDown(e: React.KeyboardEvent) {
    if (!hasSubmenu) return;
    if (e.key === 'Enter') {
      e.preventDefault();
      e.stopPropagation();
      onOpenSubmenu?.(item, e.currentTarget as HTMLElement);
      return;
    }
    if (e.key === '.' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      e.stopPropagation();
      onOpenSubmenu?.(item, e.currentTarget as HTMLElement);
    }
  }

  return (
    <li
      onKeyDown={handleKeyDown}
      className={cn(
        'group relative',
        hasSubmenu && 'pr-2',
      )}
    >
      <div className="flex items-center">
        <div className="flex-1 min-w-0">
          {renderNavItem(
            item,
            cn(
              'group relative flex h-sidebar-nav-item-height items-center gap-2 rounded-control px-3 text-label-14 transition-colors duration-state ease-standard motion-reduce:transition-none',
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
              <span className="truncate">{item.label}</span>
            </>,
            isActive,
          )}
        </div>
        {hasSubmenu && (
          <button
            ref={submenuButtonRef}
            type="button"
            aria-haspopup="menu"
            aria-expanded={isSubmenuOpenForItem}
            aria-label={`Open menu for ${item.label}`}
            tabIndex={-1}
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              onOpenSubmenu?.(item, e.currentTarget);
            }}
            className={cn(
              'flex h-6 w-6 shrink-0 items-center justify-center rounded-control text-gray-400 opacity-0 transition-opacity duration-state ease-standard motion-reduce:transition-none',
              'group-hover:opacity-100 group-focus-within:opacity-100',
              'hover:bg-gray-alpha-200 hover:text-gray-700',
              'focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-1',
            )}
          >
            <Ellipsis className="h-4 w-4" aria-hidden />
          </button>
        )}
      </div>
    </li>
  );
}
