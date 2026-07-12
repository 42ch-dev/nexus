import { useTranslation } from 'react-i18next';
import { useState, type FormEvent } from 'react';
import { CheckCircle2, AlertCircle, AlertTriangle } from 'lucide-react';

import { Dialog, DialogContent } from '@/components/ui/dialog';
import { Input, Label } from '@/components/ui';
import { Button } from '@/components/ui/button';
import { useValidatePreset } from '@/api/queries';
import type { ValidatePresetResponse } from '@42ch/nexus-contracts';

/**
 * Validate Preset dialog — POST /v1/daemon/presets:validate (dry-run).
 *
 * Product-priority #1 for non-CLI authors (web-ui.md §6.2): tells the author a
 * preset is safe to run before they commit. The path targets a preset file on
 * disk (the daemon resolves it against the home layout); the response surfaces
 * structured errors/warnings inline so the author can act on them.
 */
export function ValidatePresetDialog({
  open,
  onOpenChange,
  initialPath,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  initialPath?: string;
}) {
  const { t } = useTranslation('strategies');
  const validate = useValidatePreset();
  const [path, setPath] = useState(initialPath ?? '');
  const [result, setResult] = useState<ValidatePresetResponse | null>(null);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    if (!path.trim()) return;
    const res = await validate.mutateAsync({ path: path.trim() });
    setResult(res);
  }

  function handleOpenChange(next: boolean) {
    if (!next) {
      setResult(null);
      setPath(initialPath ?? '');
    }
    onOpenChange(next);
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent
        title={t('dialog.validatePreset.title')}
        description={t('dialog.validatePreset.description')}
      >
        <form onSubmit={handleSubmit} className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="preset-path">{t('dialog.validatePreset.presetPathLabel')}</Label>
            <Input
              id="preset-path"
              value={path}
              onChange={(e) => setPath(e.target.value)}
              placeholder={t('dialog.validatePreset.presetPathPlaceholder')}
              className="text-copy-13-mono"
              autoFocus
            />
            <p className="text-copy-13 text-gray-700">
              {t('dialog.validatePreset.helpText')}
            </p>
          </div>

          {result && <ValidationResult result={result} />}

          <div className="flex justify-end gap-2 pt-2">
            <Button type="button" variant="tertiary" size="small" onClick={() => handleOpenChange(false)}>
              {t('dialog.validatePreset.close')}
            </Button>
            <Button type="submit" variant="primary" size="small" disabled={!path.trim() || validate.isPending}>
              {validate.isPending ? t('dialog.validatePreset.submitting') : t('dialog.validatePreset.submit')}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function ValidationResult({ result }: { result: ValidatePresetResponse }) {
  const { t } = useTranslation('strategies');
  if (result.valid && result.errors.length === 0) {
    return (
      <div
        role="status"
        className="flex items-start gap-2 rounded-card border border-[color-mix(in_srgb,var(--color-green-700)_30%,transparent)] bg-[color-mix(in_srgb,var(--color-green-700)_6%,transparent)] p-3"
      >
        <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-green-700" aria-hidden />
        <div className="flex flex-col gap-1">
          <p className="text-label-14 font-medium text-green-1000">{t('dialog.validatePreset.valid')}</p>
          <p className="text-copy-13 text-green-900">
            {t('dialog.validatePreset.safeToCommit')}
            {typeof result.state_count === 'number' ? t('dialog.validatePreset.stateCountSuffix', { count: result.state_count }) : ''}.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-3">
      <div
        role="alert"
        className="flex items-start gap-2 rounded-card border border-[color-mix(in_srgb,var(--color-red-700)_30%,transparent)] bg-[color-mix(in_srgb,var(--color-red-700)_6%,transparent)] p-3"
      >
        <AlertCircle className="mt-0.5 h-4 w-4 shrink-0 text-red-700" aria-hidden />
        <div className="flex flex-col gap-1">
          <p className="text-label-14 font-medium text-red-1000">{t('dialog.validatePreset.validationFailed')}</p>
          <ul className="list-disc pl-4 text-copy-13 text-red-900">
            {result.errors.map((err, i) => (
              <li key={i}>{err}</li>
            ))}
          </ul>
        </div>
      </div>
      {result.warnings && result.warnings.length > 0 && (
        <div
          role="status"
          className="flex items-start gap-2 rounded-card border border-[color-mix(in_srgb,var(--color-amber-700)_30%,transparent)] bg-[color-mix(in_srgb,var(--color-amber-700)_6%,transparent)] p-3"
        >
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-amber-700" aria-hidden />
          <div className="flex flex-col gap-1">
            <p className="text-label-14 font-medium text-amber-1000">{t('dialog.validatePreset.warnings')}</p>
            <ul className="list-disc pl-4 text-copy-13 text-amber-900">
              {result.warnings.map((warn, i) => (
                <li key={i}>{warn}</li>
              ))}
            </ul>
          </div>
        </div>
      )}
    </div>
  );
}
