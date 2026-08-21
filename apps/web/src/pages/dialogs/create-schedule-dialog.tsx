import { useEffect, useState, type FormEvent } from 'react';

import { useTranslation } from 'react-i18next';

import { Dialog, DialogContent } from '@/components/ui/dialog';
import { Input, Label, Select } from '@/components/ui';
import { Button } from '@/components/ui/button';
import { useCreateSchedule, usePresets } from '@/api/queries';

/**
 * Create schedule dialog — POST /v1/daemon/orchestration/schedules
 * (V1.171 P2 — PL-15/PL-16).
 *
 * Honest fields only (PL-16): the form carries exactly what
 * `AddScheduleRequest` already carries — preset (USER + embedded, ids only),
 * optional label, optional seed. No new scheduler fields, no firing-cadence
 * promise (PL-17). Daemon 400s surface via the mutation's error toast plus
 * an inline message so the dialog never closes silently on failure.
 */
export function CreateScheduleDialog({
  open,
  onOpenChange,
  creatorId,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  creatorId: string;
}) {
  const { t } = useTranslation('schedule');
  const presets = usePresets();
  const create = useCreateSchedule();
  const [presetId, setPresetId] = useState('');
  const [label, setLabel] = useState('');
  const [seed, setSeed] = useState('');
  const [error, setError] = useState<string | null>(null);

  // Reset the form whenever the dialog opens.
  useEffect(() => {
    if (open) {
      setPresetId('');
      setLabel('');
      setSeed('');
      setError(null);
    }
  }, [open]);

  const presetOptions = [
    ...(presets.data?.user ?? []),
    ...(presets.data?.embedded ?? []),
  ];

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    if (!presetId) {
      setError(t('create.presetRequired'));
      return;
    }
    setError(null);
    try {
      await create.mutateAsync({
        creatorId,
        presetId,
        ...(label.trim() ? { label: label.trim() } : {}),
        ...(seed.trim() ? { seed: seed.trim() } : {}),
      });
      onOpenChange(false);
    } catch {
      // Error toast already fired by the mutation's onError callback; the
      // inline message below keeps the dialog from closing silently.
      setError(t('create.genericError'));
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent title={t('create.title')} description={t('create.description')}>
        <form onSubmit={handleSubmit} className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="schedule-preset">{t('create.presetLabel')}</Label>
            {presets.isLoading ? (
              <p className="text-copy-13 text-gray-700">{t('create.presetLoading')}</p>
            ) : presetOptions.length === 0 ? (
              <p className="text-copy-13 text-gray-700">{t('create.presetEmpty')}</p>
            ) : (
              <Select
                id="schedule-preset"
                value={presetId}
                onChange={(e) => setPresetId(e.target.value)}
                invalid={Boolean(error) && !presetId}
              >
                <option value="" disabled>
                  {t('create.presetPlaceholder')}
                </option>
                {presetOptions.map((preset) => (
                  <option key={preset.id} value={preset.id}>
                    {preset.id}
                  </option>
                ))}
              </Select>
            )}
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="schedule-label">{t('create.labelLabel')}</Label>
            <Input
              id="schedule-label"
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder={t('create.labelPlaceholder')}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="schedule-seed">{t('create.seedLabel')}</Label>
            <Input
              id="schedule-seed"
              value={seed}
              onChange={(e) => setSeed(e.target.value)}
              placeholder={t('create.seedPlaceholder')}
            />
            <p className="text-copy-13 text-gray-700">{t('create.seedHint')}</p>
          </div>
          {error && <p className="text-copy-13 text-red-700">{error}</p>}
          <div className="flex justify-end gap-2 pt-2">
            <Button type="button" variant="tertiary" size="small" onClick={() => onOpenChange(false)}>
              {t('common:action.cancel')}
            </Button>
            <Button
              type="submit"
              variant="primary"
              size="small"
              disabled={!presetId || create.isPending}
            >
              {create.isPending ? t('create.creating') : t('create.submit')}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
