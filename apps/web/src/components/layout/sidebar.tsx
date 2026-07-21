import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { NavLink, useLocation, useNavigate } from 'react-router-dom';
import {
  BookOpen,
  BrainCircuit,
  CalendarClock,
  Globe,
  Layers,
  ListChecks,
  Pencil,
  Sparkles,
  Trash2,
  User,
} from 'lucide-react';

import { flattenPages, usePatchWork, useWorks, useNarrativeWorlds } from '@/api/queries';
import { NexusLogo } from '@/components/brand/nexus-logo';
import { useCreatorEntitySelection } from '@/components/layout/creator-entity-selection-context';
import { FooterProfiles } from '@/components/layout/footer-profiles';
import { useAgentPickerDialog } from '@/components/layout/use-agent-picker-dialog';
import { useDeleteEntityDialog } from '@/components/layout/use-delete-entity-dialog';
import {
  ShellSidebarChrome,
  type ShellNavGroup,
  type ShellNavItem,
  type ShellSidebarTab,
} from '@/components/layout/presentational/shell-sidebar-chrome';
import { SelectionSubmenu } from '@/components/selection-submenu/selection-submenu';
import { cn } from '@/lib/utils';

/**
 * Sidebar nav — V1.94 two-tab IA (Creator | Orchestrator).
 *
 * V1.125 P2 rewrites list-mode Creation IA into Worlds-first peer groups —
 * Worlds, then Works — with Timeline peer groups removed (deep links retained).
 * V1.118 P2 keeps Creator | Orchestrator tabs visible inside work routes
 * (AC-P2-5); enter-work UX is the canvas-first shell + right rail, not
 * whole-left drill-in.
 *
 * Thin wrapper around {@link ShellSidebarChrome}: owns NavLink, the active
 * creator profile, and the route-derived active state. The chrome owns the
 * markup, classes, and `data-testid` SSOT.
 *
 * V1.125 P1 moves Memory under Orchestrator (first group) and derives the
 * active tab from orchestration route prefixes so deep links select the right tab.
 *
 * Active-highlight note: peer items use prefix matching via
 * {@link ShellSidebarChrome}'s `isActiveItem` callback.
 */
const ORCHESTRATOR_ROUTE_PREFIXES = [
  '/memory',
  '/strategies',
  '/sessions',
  '/schedule',
  '/modules',
] as const;

function tabFromPathname(pathname: string): ShellSidebarTab {
  return ORCHESTRATOR_ROUTE_PREFIXES.some(
    (prefix) => pathname === prefix || pathname.startsWith(`${prefix}/`),
  )
    ? 'orchestrator'
    : 'creator';
}

export function Sidebar() {
  const { t } = useTranslation('shell');
  const { pathname } = useLocation();
  const navigate = useNavigate();
  const [activeTab, setActiveTab] = useState<ShellSidebarTab>(() => tabFromPathname(pathname));
  const worksQuery = useWorks({ limit: 12 });
  const works = useMemo(() => flattenPages(worksQuery.data), [worksQuery.data]);
  const worldsQuery = useNarrativeWorlds({ limit: 12 });
  const worlds = useMemo(() => worldsQuery.data ?? [], [worldsQuery.data]);
  const patchWork = usePatchWork();
  const agentDialog = useAgentPickerDialog();
  const deleteDialog = useDeleteEntityDialog();
  const { setSelectedEntity, clearSelectedEntity } = useCreatorEntitySelection();

  const [renamingItem, setRenamingItem] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState('');
  const renameInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setActiveTab(tabFromPathname(pathname));
  }, [pathname]);

  function extractWorkId(item: ShellNavItem): string | null {
    const match = /^\/works\/([^/]+)/.exec(item.to);
    if (match) {
      const id = decodeURIComponent(match[1]);
      return id === '' ? null : id;
    }
    return null;
  }

  function extractWorldId(item: ShellNavItem): string | null {
    const match = /^\/worlds\/([^/]+)/.exec(item.to);
    if (match) {
      const id = decodeURIComponent(match[1]);
      return id === '' ? null : id;
    }
    return null;
  }

  function isEntityItem(item: ShellNavItem): boolean {
    return extractWorkId(item) !== null || extractWorldId(item) !== null;
  }

  function isHubAggregateItem(item: ShellNavItem): boolean {
    return item.to === '/worlds' || item.to === '/works';
  }

  function activateEntitySelection(item: ShellNavItem) {
    const workId = extractWorkId(item);
    if (workId) {
      setSelectedEntity({ kind: 'work', id: workId, label: item.label });
      navigate('/works');
      return;
    }
    const worldId = extractWorldId(item);
    if (worldId) {
      setSelectedEntity({ kind: 'world', id: worldId, label: item.label });
      navigate('/worlds');
    }
  }

  useEffect(() => {
    if (renamingItem && renameInputRef.current) {
      renameInputRef.current.focus();
      renameInputRef.current.select();
    }
  }, [renamingItem]);

  useEffect(() => {
    // V1.127 P0 T5 (AC-V1127-5): clear stale rename state on navigation.
    // Without this, renamingItem survives route changes and a stale rename
    // input renders on the destination view. Dep is pathname ONLY — a
    // :workId swap inside the same route does NOT change pathname, so this
    // effect does not fire and an in-flight rename is never interrupted. The
    // chrome's submenuItem is reset separately (chrome-owned).
    setRenamingItem(null);
    setRenameValue('');
  }, [pathname]);

  function handleRenameSubmit() {
    const itemTo = renamingItem;
    if (!itemTo || !renameValue.trim()) {
      setRenamingItem(null);
      return;
    }
    const wid = extractWorkId({ to: itemTo, label: '', icon: BookOpen });
    if (wid) {
      patchWork.mutate({ workId: wid, request: { title: renameValue.trim() } });
    }
    setRenamingItem(null);
  }

  const creatorGroups: ShellNavGroup[] = useMemo(
    () => [
      // V1.125 P2 — Worlds-first Creator IA (AC-V1125-5). Timeline and Work
      // Timelines peer groups are removed; `/timeline` and
      // `/works/:id/timeline` remain deep-linkable via command palette and
      // in-surface navigation.
      {
        id: 'worlds',
        label: t('nav.worlds'),
        items: [
          { to: '/worlds', label: t('nav.worlds'), icon: Globe },
          ...worlds.map((world) => ({
            to: `/worlds/${encodeURIComponent(world.world_id)}/timeline`,
            label: world.title || world.world_id,
            icon: Globe,
          })),
        ],
      },
      {
        id: 'works',
        label: t('nav.works'),
        items: [
          { to: '/works', label: t('nav.allWorks'), icon: Layers },
          ...works.map((work) => ({
            to: `/works/${encodeURIComponent(work.work_id)}/outline`,
            label: work.title,
            icon: BookOpen,
          })),
        ],
      },
    ],
    [t, works, worlds],
  );

  const orchestratorGroups: ShellNavGroup[] = useMemo(
    () => [
      {
        id: 'memory',
        label: t('nav.memory'),
        items: [{ to: '/memory', label: t('nav.memory'), icon: BrainCircuit }],
      },
      {
        id: 'strategies',
        label: t('nav.strategies'),
        items: [{ to: '/strategies', label: t('nav.strategies'), icon: Sparkles }],
      },
      {
        id: 'runtime',
        label: t('nav.runtime'),
        items: [
          { to: '/sessions', label: t('nav.sessions'), icon: ListChecks },
          { to: '/schedule', label: t('nav.schedule'), icon: CalendarClock },
        ],
      },
      ],
    [t],
  );

  const groups = activeTab === 'creator' ? creatorGroups : orchestratorGroups;

  const renderSubmenu = useCallback(
    (item: ShellNavItem, close: () => void, anchorEl: HTMLElement) => {
      const isWorld = item.to.startsWith('/worlds');
      const isWork = item.to.startsWith('/works');
      if (!isWorld && !isWork) return null;

      const workId = extractWorkId(item);
      const isEntity = isEntityItem(item);

      return (
        <>
          <SelectionSubmenu
            open
            onClose={close}
            anchorEl={anchorEl}
            ariaLabel={t('submenu.ariaLabel')}
            items={[
              {
                id: 'open-timeline',
                label: t('submenu.openTimeline'),
                icon: BookOpen,
                onSelect: () => {
                  if (workId) {
                    navigate(`/works/${encodeURIComponent(workId)}/timeline`);
                  } else if (isWorld) {
                    navigate('/worlds/timeline');
                  } else {
                    navigate('/works/timeline');
                  }
                },
              },
              {
                id: 'open-secondary',
                label: isWorld ? t('submenu.openKb') : t('submenu.openOutline'),
                icon: isWorld ? Globe : BookOpen,
                onSelect: () => {
                  if (workId) {
                    navigate(`/works/${encodeURIComponent(workId)}/outline`);
                  } else if (isWorld) {
                    navigate('/worlds/kb');
                  } else {
                    navigate('/works/outline');
                  }
                },
              },
              ...(isEntity
                ? [
                    {
                      id: 'agent',
                      label: t('submenu.agentLabel', { status: t('submenu.unassigned') }),
                      icon: User,
                      onSelect: () => {
                        agentDialog.setTitle(t('submenu.agentDialogTitle', { entityName: item.label }));
                        agentDialog.setOpen(true);
                        close();
                      },
                    },
                    {
                      id: 'rename',
                      label: t('submenu.rename'),
                      icon: Pencil,
                      onSelect: () => {
                        setRenamingItem(item.to);
                        setRenameValue(item.label);
                        close();
                      },
                    },
                    // V1.129 P2 (R-V1126P0-T2-001) — hard-delete via confirm dialog.
                    // Title and body name the item + cascade + irreversibility
                    // per architect lock (Seat 2).
                    {
                      id: 'delete',
                      label: t('submenu.delete'),
                      icon: Trash2,
                      variant: 'danger' as const,
                      onSelect: () => {
                        const workId = extractWorkId(item);
                        const worldId = extractWorldId(item);
                        if (workId) {
                          deleteDialog.openDelete({
                            kind: 'work',
                            id: workId,
                            label: item.label,
                          });
                        } else if (worldId) {
                          deleteDialog.openDelete({
                            kind: 'world',
                            id: worldId,
                            label: item.label,
                          });
                        }
                        close();
                      },
                    },
                  ]
                : []),
            ]}
          />
        </>
      );
    },
    [t, navigate, agentDialog, deleteDialog],
  );

  return (
    <nav aria-label={t('aria.primary')} className="min-h-0 flex-1">
      <ShellSidebarChrome
        activeTab={activeTab}
        activeRoute={pathname}
        navGroups={groups}
        onTabChange={setActiveTab}
        logo={<NexusLogo />}
        footer={<FooterProfiles />}
        creatorTabLabel={t('nav.creator')}
        orchestratorTabLabel={t('nav.orchestrator')}
        primaryNavigationAriaLabel={t('aria.primaryNavigation')}
        renderSubmenu={renderSubmenu}
        hasSubmenu={(item) => isEntityItem(item)}
        isActiveItem={(item, route) => {
          if (item.to === '/works') return route === '/works';
          const outlineMatch = /^\/works\/([^/]+)\/outline$/.exec(item.to);
          if (outlineMatch) {
            const encodedWorkId = outlineMatch[1];
            if (route === item.to) return true;
            return route.startsWith(`/works/${encodedWorkId}/`);
          }
          return route === item.to || route.startsWith(`${item.to}/`);
        }}
        renderNavItem={(item, className, content, isActive) => {
          // V1.126 P0 T2: if this item is being renamed, render an inline edit
          const isRenaming = renamingItem === item.to;
          return (
            <NavLink
              to={item.to}
              className={cn(className, isActive ? 'bg-gray-alpha-100 text-gray-1000' : undefined)}
              onClick={(e) => {
                if (isRenaming) {
                  e.preventDefault();
                  return;
                }
                if (isHubAggregateItem(item)) {
                  clearSelectedEntity();
                  return;
                }
                if (isEntityItem(item)) {
                  e.preventDefault();
                  activateEntitySelection(item);
                }
              }}
            >
              {isRenaming ? (
                <input
                  ref={renameInputRef}
                  type="text"
                  value={renameValue}
                  onChange={(e) => setRenameValue(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                      e.preventDefault();
                      e.stopPropagation();
                      handleRenameSubmit();
                    }
                    if (e.key === 'Escape') {
                      e.preventDefault();
                      e.stopPropagation();
                      setRenamingItem(null);
                    }
                  }}
                  onBlur={() => handleRenameSubmit()}
                  className="w-full rounded-control border border-blue-700 bg-background-100 px-2 py-0.5 text-label-14 text-gray-1000 outline-none"
                  onClick={(e) => e.stopPropagation()}
                  data-testid="sidebar-rename-input"
                />
              ) : (
                content
              )}
            </NavLink>
          );
        }}
      />
      {agentDialog.dialog}
      {deleteDialog.dialog}
    </nav>
  );
}

// Re-export the chrome types for convenience.
export type { ShellNavGroup, ShellSidebarTab };
