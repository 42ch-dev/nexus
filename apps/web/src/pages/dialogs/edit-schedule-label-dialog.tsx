import { useEffect, useState, type FormEvent } from 'react';

import { useTranslation } from 'react-i18next';

import { Dialog, DialogContent } from '@/components/ui/dialog';
import { Input, Label } from '@/components/ui';
import { Button } from '@/components/ui/button';
import { useEditSchedule } from '@/api/queries';
import type { ScheduleSummary } from '@42ch/nexus-contracts';

/**
 * Edit schedule label dialog — PATCH /v1/daemon/orchestration/schedules/{id}
 * (V1.171 P2 — PL-16/AR-29).
 *
 * Only the label is updateable today. An empty input sends `label: ""`,
 * which the daemon normalizes to NULL (label cleared — never stored as an
 * empty string). Daemon 400s surface via the mutation's error toast plus an
 * inline message so the dialog never closes silently on failure.
 */
export function EditScheduleLabelDialog({
  schedule,
  open,
  onOpenChange,
}: {
  schedule: ScheduleSummary;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation('schedule');
  const edit = useEditSchedule();
  const [label, setLabel] = useState('');
  const [error, setError] = useState<string | null>(null);

  // Reset the form whenever the dialog opens for a (possibly different) row.
  useEffect(() => {
    if (open) {
      setLabel(schedule.label ?? '');
      setError(null);
    }
  }, [open, schedule]);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    try {
      // Empty input → `label: ""` → daemon clears the label to NULL.
      await edit.mutateAsync({
        scheduleId: schedule.schedule_id,
        request: { label: label.trim() },
      });
      onOpenChange(false);
    } catch {
      // Error toast already fired by the mutation's onError callback; the
      // inline message below keeps the dialog from closing silently.
      setError(t('editLabel.genericError'));
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent title={t('editLabel.title')} description={t('editLabel.description')}>
        <form onSubmit={handleSubmit} className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="schedule-label-edit">{t('editLabel.labelLabel')}</Label>
            <Input
              id="schedule-label-edit"
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder={t('editLabel.labelPlaceholder')}
            />
            <p className="text-copy-13 text-gray-700">{t('editLabel.clearHint')}</p>
          </div>
          {error && <p className="text-copy-13 text-red-700">{error}</p>}
          <div className="flex justify-end gap-2 pt-2">
            <Button type="button" variant="tertiary" size="small" onClick={() => onOpenChange(false)}>
              {t('common:action.cancel')}
            </Button>
            <Button type="submit" variant="primary" size="small" disabled={edit.isPending}>
              {edit.isPending ? t('editLabel.saving') : t('editLabel.submit')}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
