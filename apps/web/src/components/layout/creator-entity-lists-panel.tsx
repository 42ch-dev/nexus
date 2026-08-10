import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { useLocation, useNavigate } from 'react-router';
import { BookOpen, Ellipsis, Globe, Pencil, Trash2, User } from 'lucide-react';

import { flattenPages, usePatchWork, useWorks, useNarrativeWorlds } from '@/api/queries';
import { useCreatorEntitySelection } from '@/components/layout/creator-entity-selection-context';
import {
  CreatorEntityLists,
  type CreatorEntityListItem,
} from '@/components/layout/presentational/creator-entity-lists';
import { useAgentPickerDialog } from '@/components/layout/use-agent-picker-dialog';
import { useDeleteEntityDialog } from '@/components/layout/use-delete-entity-dialog';
import { SelectionSubmenu } from '@/components/selection-submenu/selection-submenu';
import { cn } from '@/lib/utils';

type RenamingTarget = { kind: 'work' | 'world'; id: string } | null;

/**
 * Wired Creator hub right-side Worlds / Works lists with entity selection
 * and row actions (V1.132 P3 AC-8).
 */
export function CreatorEntityListsPanel() {
  const { t } = useTranslation('shell');
  const { pathname } = useLocation();
  const navigate = useNavigate();
  const { selectedEntity, setSelectedEntity } = useCreatorEntitySelection();
  const worksQuery = useWorks({ limit: 12 });
  const works = useMemo(() => flattenPages(worksQuery.data), [worksQuery.data]);
  const worldsQuery = useNarrativeWorlds({ limit: 12 });
  const worlds = useMemo(() => worldsQuery.data ?? [], [worldsQuery.data]);
  const patchWork = usePatchWork();
  const agentDialog = useAgentPickerDialog();
  const deleteDialog = useDeleteEntityDialog();

  const [submenuItem, setSubmenuItem] = useState<CreatorEntityListItem & { kind: 'work' | 'world' } | null>(
    null,
  );
  const [submenuAnchor, setSubmenuAnchor] = useState<HTMLElement | null>(null);
  const triggerRef = useRef<HTMLElement | null>(null);
  const [renamingTarget, setRenamingTarget] = useState<RenamingTarget>(null);
  const [renameValue, setRenameValue] = useState('');
  const renameInputRef = useRef<HTMLInputElement>(null);

  const worldItems: CreatorEntityListItem[] = useMemo(
    () =>
      worlds.map((world) => ({
        id: world.world_id,
        label: world.title || world.world_id,
      })),
    [worlds],
  );

  const workItems: CreatorEntityListItem[] = useMemo(
    () =>
      works.map((work) => ({
        id: work.work_id,
        label: work.title,
      })),
    [works],
  );

  useEffect(() => {
    setRenamingTarget(null);
    setRenameValue('');
    setSubmenuItem(null);
    setSubmenuAnchor(null);
    triggerRef.current = null;
  }, [pathname]);

  useEffect(() => {
    if (renamingTarget && renameInputRef.current) {
      renameInputRef.current.focus();
      renameInputRef.current.select();
    }
  }, [renamingTarget]);

  function closeSubmenu() {
    setSubmenuItem(null);
    setSubmenuAnchor(null);
    triggerRef.current?.focus();
    triggerRef.current = null;
  }

  function openSubmenu(item: CreatorEntityListItem & { kind: 'work' | 'world' }, anchor: HTMLElement) {
    triggerRef.current = document.activeElement as HTMLElement;
    setSubmenuItem(item);
    setSubmenuAnchor(anchor);
  }

  function handleSelectWorld(id: string) {
    const world = worldItems.find((item) => item.id === id);
    if (!world) return;
    setSelectedEntity({ kind: 'world', id: world.id, label: world.label });
    navigate('/worlds');
  }

  function handleSelectWork(id: string) {
    const work = workItems.find((item) => item.id === id);
    if (!work) return;
    setSelectedEntity({ kind: 'work', id: work.id, label: work.label });
    navigate('/works');
  }

  function handleRenameSubmit() {
    if (!renamingTarget || !renameValue.trim()) {
      setRenamingTarget(null);
      return;
    }
    if (renamingTarget.kind === 'work') {
      patchWork.mutate({
        workId: renamingTarget.id,
        request: { title: renameValue.trim() },
      });
    }
    setRenamingTarget(null);
  }

  const renderSubmenu = useCallback(
    (item: CreatorEntityListItem & { kind: 'work' | 'world' }) => {
      const isWorld = item.kind === 'world';
      const workId = item.kind === 'work' ? item.id : null;
      const worldId = item.kind === 'world' ? item.id : null;

      return (
        <SelectionSubmenu
          open
          onClose={closeSubmenu}
          anchorEl={submenuAnchor}
          ariaLabel={t('submenu.ariaLabel')}
          items={[
            {
              id: 'open-timeline',
              label: t('submenu.openTimeline'),
              icon: BookOpen,
              onSelect: () => {
                if (workId) {
                  navigate(`/works/${encodeURIComponent(workId)}/timeline`);
                } else if (worldId) {
                  navigate(`/worlds/${encodeURIComponent(worldId)}/timeline`);
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
                } else if (worldId) {
                  navigate(`/worlds/${encodeURIComponent(worldId)}/kb`);
                }
              },
            },
            {
              id: 'agent',
              label: t('submenu.agentLabel', { status: t('submenu.unassigned') }),
              icon: User,
              onSelect: () => {
                agentDialog.setTitle(t('submenu.agentDialogTitle', { entityName: item.label }));
                agentDialog.setOpen(true);
                closeSubmenu();
              },
            },
            // World rename is hidden until a narrative-world PATCH API exists.
            ...(isWorld
              ? []
              : [
                  {
                    id: 'rename',
                    label: t('submenu.rename'),
                    icon: Pencil,
                    onSelect: () => {
                      setRenamingTarget({ kind: item.kind, id: item.id });
                      setRenameValue(item.label);
                      closeSubmenu();
                    },
                  },
                ]),
            {
              id: 'delete',
              label: t('submenu.delete'),
              icon: Trash2,
              variant: 'danger' as const,
              onSelect: () => {
                if (workId) {
                  deleteDialog.openDelete({ kind: 'work', id: workId, label: item.label });
                } else if (worldId) {
                  deleteDialog.openDelete({ kind: 'world', id: worldId, label: item.label });
                }
                closeSubmenu();
              },
            },
          ]}
        />
      );
    },
    [t, navigate, agentDialog, deleteDialog, submenuAnchor],
  );

  function renderRowActions(item: CreatorEntityListItem, kind: 'work' | 'world') {
    const entityItem = { ...item, kind };
    const isSubmenuOpen = submenuItem?.id === item.id && submenuItem.kind === kind;

    return (
      <button
        type="button"
        aria-haspopup="menu"
        aria-expanded={isSubmenuOpen}
        aria-label={t('submenu.triggerAriaLabel', { label: item.label })}
        tabIndex={-1}
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          openSubmenu(entityItem, e.currentTarget);
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
    );
  }

  function renderRowContent(
    item: CreatorEntityListItem,
    kind: 'work' | 'world',
    defaultContent: ReactNode,
  ) {
    const isRenaming =
      renamingTarget?.kind === kind && renamingTarget.id === item.id;

    if (!isRenaming) {
      return defaultContent;
    }

    return (
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
            setRenamingTarget(null);
          }
        }}
        onBlur={() => handleRenameSubmit()}
        className="w-full rounded-control border border-blue-1000 bg-background-100 px-2 py-0.5 text-label-14 text-gray-1000 outline-none dark:border-blue-700"
        onClick={(e) => e.stopPropagation()}
        data-testid="creator-entity-rename-input"
      />
    );
  }

  return (
    <>
      <CreatorEntityLists
        labels={{
          worldsTitle: t('nav.worlds'),
          worksTitle: t('nav.works'),
        }}
        worlds={worldItems}
        works={workItems}
        selectedEntity={selectedEntity}
        onSelectWorld={handleSelectWorld}
        onSelectWork={handleSelectWork}
        renderWorldRowActions={(item) => renderRowActions(item, 'world')}
        renderWorkRowActions={(item) => renderRowActions(item, 'work')}
        renderWorldRowContent={(item, defaultContent) =>
          renderRowContent(item, 'world', defaultContent)
        }
        renderWorkRowContent={(item, defaultContent) =>
          renderRowContent(item, 'work', defaultContent)
        }
        data-testid="creator-hub-entity-lists"
      />
      {submenuItem && submenuAnchor ? renderSubmenu(submenuItem) : null}
      {agentDialog.dialog}
      {deleteDialog.dialog}
    </>
  );
}
