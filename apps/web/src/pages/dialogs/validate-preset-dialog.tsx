import { useTranslation } from 'react-i18next';
import { useState } from 'react';

import {
  ValidatePresetDialogChrome,
  type ValidatePresetCopy,
  type ValidatePresetResultView,
} from '@/components/harness/presentational/validate-preset-dialog-chrome';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { useValidatePreset } from '@/api/queries';

/**
 * Validate Preset dialog — POST /v1/daemon/presets:validate (dry-run).
 *
 * Product-priority #1 for non-CLI authors (web-ui.md §6.2): tells the author a
 * preset is safe to run before they commit. The path targets a preset file on
 * disk (the daemon resolves it against the home layout); the response surfaces
 * structured errors/warnings inline so the author can act on them.
 *
 * The body is the presentational `ValidatePresetDialogChrome` (also rendered by
 * the Design Studio fixture); this component owns the path state, the mutation
 * and the copy.
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
  const [result, setResult] = useState<ValidatePresetResultView | null>(null);

  const copy: ValidatePresetCopy = {
    pathLabel: t('dialog.validatePreset.presetPathLabel'),
    pathPlaceholder: t('dialog.validatePreset.presetPathPlaceholder'),
    helpText: t('dialog.validatePreset.helpText'),
    close: t('dialog.validatePreset.close'),
    submit: t('dialog.validatePreset.submit'),
    submitting: t('dialog.validatePreset.submitting'),
    valid: t('dialog.validatePreset.valid'),
    safeToCommit: t('dialog.validatePreset.safeToCommit'),
    validationFailed: t('dialog.validatePreset.validationFailed'),
    warnings: t('dialog.validatePreset.warnings'),
  };

  async function handleSubmit() {
    if (!path.trim()) return;
    try {
      const response = await validate.mutateAsync({ path: path.trim() });
      setResult({
        valid: response.valid,
        errors: response.errors,
        warnings: response.warnings ?? [],
        stateSuffix:
          typeof response.state_count === 'number'
            ? t('dialog.validatePreset.stateCountSuffix', { count: response.state_count })
            : '',
      });
    } catch {
      // The mutation hook already surfaced the transport failure as a toast;
      // the previous result (if any) stays on screen.
    }
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
        <ValidatePresetDialogChrome
          copy={copy}
          path={path}
          onPathChange={setPath}
          pending={validate.isPending}
          result={result}
          onClose={() => handleOpenChange(false)}
          onSubmit={() => void handleSubmit()}
        />
      </DialogContent>
    </Dialog>
  );
}
