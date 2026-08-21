import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AlertTriangle } from 'lucide-react';

import { Dialog, DialogContent } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { useDeleteSchedule } from '@/api/queries';
import type { ScheduleSummary } from '@42ch/nexus-contracts';

/**
 * Delete schedule confirm dialog — DELETE /v1/daemon/orchestration/schedules/{id}
 * (V1.171 P2 — PL-15/AR-31).
 *
 * Destructive confirm following the existing delete-dialog precedent
 * (strategies page / use-delete-entity-dialog): title names the schedule,
 * body states irreversibility, primary CTA is the destructive Delete.
 * The daemon owns the enforcement (it cancels non-terminal schedules before
 * deletion; unknown ids → 404) — this UI does NOT pre-filter client-side.
 * Daemon errors surface via the mutation's error toast plus an inline
 * message so the dialog never closes silently on failure.
 */
export function DeleteScheduleDialog({
  schedule,
  open,
  onOpenChange,
}: {
  schedule: ScheduleSummary;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation('schedule');
  const remove = useDeleteSchedule();
  const isPending = remove.isPending;
  const [error, setError] = useState<string | null>(null);
  // Title names the item: the label when present, else the schedule id
  // (mirrors the row's label rendering — empty/whitespace labels fall back).
  const displayName = schedule.label?.trim() ? schedule.label : schedule.schedule_id;

  async function handleConfirm() {
    setError(null);
    try {
      await remove.mutateAsync(schedule.schedule_id);
      onOpenChange(false);
    } catch {
      // Error toast already fired by the mutation's onError callback; the
      // inline message below keeps the dialog from closing silently.
      setError(t('delete.genericError'));
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!isPending) onOpenChange(next);
      }}
    >
      <DialogContent title={t('delete.title', { name: displayName })}>
        <div className="flex items-start gap-3 pb-2">
          <AlertTriangle
            className="mt-0.5 h-5 w-5 shrink-0 text-amber-700 dark:text-amber-400"
            aria-hidden
          />
          <p className="text-copy-14 text-gray-900">{t('delete.description')}</p>
        </div>
        {error && <p className="text-copy-13 text-red-700">{error}</p>}
        <div className="flex justify-end gap-2 pt-2">
          <Button
            type="button"
            variant="tertiary"
            size="small"
            onClick={() => onOpenChange(false)}
            disabled={isPending}
          >
            {t('common:action.cancel')}
          </Button>
          <Button
            type="button"
            variant="destructive"
            size="small"
            onClick={handleConfirm}
            disabled={isPending}
          >
            {isPending ? t('delete.deleting') : t('delete.delete')}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
