/**
 * Compose conflict dialog (v239-p3-canvas-compose-ui T4) — a stale
 * `base_revision` is the typed 409 `strategy_candidate_conflict`. The rendered
 * graph is unchanged and nothing auto-retries. Reload reads the canonical
 * candidate at the revision the daemon holds, so the user reapplies the op
 * against current bytes.
 */
import { useTranslation } from 'react-i18next';

import { Dialog, DialogContent } from '@/components/ui/dialog';
import type { ComposeConflict, ComposeRefusal } from './hooks/use-strategy-compose';

export interface ComposeConflictDialogProps {
  conflict: ComposeConflict | null;
  refusal: ComposeRefusal | null;
  onReload: () => void;
  onDismiss: () => void;
}

export function ComposeConflictDialog({
  conflict,
  refusal,
  onReload,
  onDismiss,
}: ComposeConflictDialogProps) {
  const { t } = useTranslation('canvas');
  const open = conflict !== null || refusal !== null;
  return (
    <Dialog open={open} onOpenChange={(next) => (next ? undefined : onDismiss())}>
      <DialogContent title={t('strategy.composeConflict.title')}>
        {conflict ? (
          <div className="mt-2 flex flex-col gap-2">
            <p data-testid="compose-conflict" role="alert" className="text-copy-13 text-gray-1000">
              {conflict.message}
            </p>
            <p className="rounded-card border border-gray-alpha-400 px-3 py-2 text-copy-13 text-gray-900">
              {t('strategy.composeConflict.reload', { revision: conflict.currentRevision })}
            </p>
          </div>
        ) : null}
        {refusal ? (
          <p data-testid="compose-refusal" role="alert" className="mt-2 text-copy-13 text-gray-1000">
            {refusal.message}
          </p>
        ) : null}
        <div className="mt-4 flex justify-end gap-2">
          {conflict ? (
            <button
              type="button"
              data-testid="compose-conflict-reload"
              onClick={onReload}
              className="rounded-control border border-gray-alpha-400 px-3 py-1.5 text-button-12 text-gray-1000 hover:bg-gray-alpha-100"
            >
              {t('strategy.composeConflict.reloadAction')}
            </button>
          ) : null}
          <button
            type="button"
            data-testid="compose-conflict-dismiss"
            onClick={onDismiss}
            className="rounded-control border border-gray-alpha-400 px-3 py-1.5 text-button-12 text-gray-900 hover:bg-gray-alpha-100"
          >
            {t('strategy.composeConflict.dismiss')}
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
