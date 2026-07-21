import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AlertTriangle } from 'lucide-react';

import {
  useDeleteWork,
  useDeleteWorld,
} from '@/api/queries';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { useCreatorEntitySelection } from '@/components/layout/creator-entity-selection-context';

/**
 * Confirm dialog for destructive Work / World delete (V1.129 P2 — R-V1126P0-T2-001).
 *
 * Per architect lock (Seat 2, 2026-07-21):
 * - Title names the item.
 * - Body names cascaded items + irreversibility.
 * - Primary CTA: "Delete" (destructive). Secondary: "Cancel".
 *
 * The dialog is a thin wrapper over `Dialog` + the two delete hooks. Callers
 * pass `{ kind: 'work' | 'world', id, label }` to open. Transport errors
 * route through `useErrorToast` in the hooks (which surfaces
 * `<TransportErrorBlock>` copy for known kinds via the shared toast path).
 */
export interface DeleteEntityTarget {
  kind: 'work' | 'world';
  id: string;
  label: string;
}

export interface DeleteEntityDialogHandle {
  open: boolean;
  target: DeleteEntityTarget | null;
  openDelete: (target: DeleteEntityTarget) => void;
  setOpen: (open: boolean) => void;
  dialog: React.ReactNode;
}

/**
 * Hook that owns the delete-confirm dialog state + JSX.
 *
 * Mirrors the `useAgentPickerDialog` shape so the sidebar can render it the
 * same way (`{handle.dialog}`).
 */
export function useDeleteEntityDialog(): DeleteEntityDialogHandle {
  const { t } = useTranslation('shell');
  const [open, setOpen] = useState(false);
  const [target, setTarget] = useState<DeleteEntityTarget | null>(null);
  const deleteWork = useDeleteWork();
  const deleteWorld = useDeleteWorld();
  // Creator hub selection SSOT (V1.128 P2). When the deleted entity is the
  // currently-selected one, clearing it prevents the /worlds or /works hub
  // from continuing to render a now-deleted item until the user navigates.
  const { selectedEntity, clearSelectedEntity } = useCreatorEntitySelection();

  const openDelete = (next: DeleteEntityTarget) => {
    setTarget(next);
    setOpen(true);
  };

  const isPending =
    (target?.kind === 'work' && deleteWork.isPending) ||
    (target?.kind === 'world' && deleteWorld.isPending);

  const handleConfirm = () => {
    if (!target) return;
    const onSuccess = () => {
      // Clear the selection if it matches the deleted target. `handleConfirm`
      // is recreated every render so `selectedEntity` is fresh at click time;
      // the dialog is modal, so selection cannot change while it is open.
      if (
        selectedEntity &&
        selectedEntity.kind === target.kind &&
        selectedEntity.id === target.id
      ) {
        clearSelectedEntity();
      }
      setOpen(false);
      setTarget(null);
    };
    if (target.kind === 'work') {
      deleteWork.mutate(target.id, { onSuccess });
    } else {
      deleteWorld.mutate(target.id, { onSuccess });
    }
  };

  const dialog = (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!isPending) setOpen(next);
      }}
    >
      {target && (
        <DialogContent
          title={
            target.kind === 'work'
              ? t('submenu.deleteDialog.workTitle', { name: target.label })
              : t('submenu.deleteDialog.worldTitle', { name: target.label })
          }
        >
          <div className="flex items-start gap-3 pb-2">
            <AlertTriangle
              className="mt-0.5 h-5 w-5 shrink-0 text-amber-700 dark:text-amber-400"
              aria-hidden
            />
            <p className="text-copy-14 text-gray-900">
              {target.kind === 'work'
                ? t('submenu.deleteDialog.workBody')
                : t('submenu.deleteDialog.worldBody')}
            </p>
          </div>
          <div className="flex items-center justify-end gap-2 pt-2">
            <Button
              type="button"
              variant="tertiary"
              size="small"
              onClick={() => setOpen(false)}
              disabled={isPending}
            >
              {t('submenu.deleteDialog.cancel')}
            </Button>
            <Button
              type="button"
              variant="destructive"
              size="small"
              onClick={handleConfirm}
              disabled={isPending}
            >
              {isPending ? t('submenu.deleteDialog.confirm') : t('submenu.deleteDialog.confirm')}
            </Button>
          </div>
        </DialogContent>
      )}
    </Dialog>
  );

  return { open, target, openDelete, setOpen, dialog };
}
