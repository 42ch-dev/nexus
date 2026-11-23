/**
 *
 * The author-facing create step of the fork authoring loop: opened from a
 * World Timeline compute node's "Branch this world's timeline from here"
 * affordance, it confirms the picked fork-point event and optionally labels
 * the fork, then calls P1's `POST /v1/daemon/worlds/:world_id/forks` with
 * `{ parent_branch_id, forked_from_event_id, label? }`.
 *
 * Reuses the V1.159 era-create-dialog pattern (Dialog/DialogContent + form,
 * inline error slot, submit-disabled-while-pending, `onSuccess` hand-off):
 *   - On success the dialog closes and `onSuccess(response)` fires with the
 *     full `CreateForkResponse` — the orchestrator owns the PD-6 landing
 *     (`handleBranchChange(response.branch_id)` — URL-as-SSOT `?branch=` +
 *     success notice), so the dialog stays dumb.
 *   - 422 (`invalid_input` — bad / non-existent fork point) surfaces
 *     INLINE and the dialog stays open: the author remains on the parent
 *     Timeline, no branch switch (plan §2 error handling).
 *   - 403 / 5xx / network are already toasted by `useCreateFork`'s onError;
 *     the dialog mirrors a generic inline message so it never closes
 *     silently on failure.
 *
 * UI copy follows the PD-5 lazy-branch model — "Branch this world's
 * timeline from here" / fork language only; never "copy/duplicate world".
 */
import { useEffect, useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import type { CreateForkResponse } from '@42ch/nexus-contracts';

import { Dialog, DialogContent } from '@/components/ui/dialog';
import { Button, Input, Label } from '@/components/ui';
import { isForkInvalidInputError, useCreateFork } from '@/api/queries';

export interface ForkCreateDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  worldId: string;
  parentBranchId: string | undefined;
  /** The picked timeline event id (the fork point). */
  forkedFromEventId: string;
  /** Event title of the fork point, shown as a read-only reference line. */
  forkPointLabel?: string;
  onSuccess?: (response: CreateForkResponse, label?: string) => void;
}

export function ForkCreateDialog({
  open,
  onOpenChange,
  worldId,
  parentBranchId,
  forkedFromEventId,
  forkPointLabel,
  onSuccess,
}: ForkCreateDialogProps) {
  const { t } = useTranslation('canvas');
  const createFork = useCreateFork();

  const [label, setLabel] = useState('');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setLabel('');
      setError(null);
    }
  }, [open]);

  const submitting = createFork.isPending;

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    if (createFork.isPending) return;
    if (!forkedFromEventId) {
      setError(t('timeline.forkCreateDialog.missingForkPoint'));
      return;
    }
    if (!parentBranchId) {
      setError(t('timeline.forkCreateDialog.missingBranchContext'));
      return;
    }
    setError(null);

    const trimmedLabel = label.trim();
    try {
      const response = await createFork.mutateAsync({
        worldId,
        request: {
          parent_branch_id: parentBranchId,
          forked_from_event_id: forkedFromEventId,
          ...(trimmedLabel.length > 0 ? { label: trimmedLabel } : {}),
        },
      });
      onOpenChange(false);
      onSuccess?.(response, trimmedLabel.length > 0 ? trimmedLabel : undefined);
    } catch (err) {
      if (isForkInvalidInputError(err)) {
        setError(t('timeline.forkCreateDialog.invalidForkPoint'));
      } else {
        setError(t('timeline.forkCreateDialog.genericError'));
      }
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title={t('timeline.forkCreateDialog.title')}
        description={t('timeline.forkCreateDialog.description')}
      >
        <form onSubmit={handleSubmit} className="flex flex-col gap-4">
          {forkPointLabel ? (
            <p
              className="text-copy-13 text-gray-700"
              data-testid="fork-create-fork-point"
            >
              {t('timeline.forkCreateDialog.forkPointLabel')}:{' '}
              <strong>{forkPointLabel}</strong>
            </p>
          ) : null}

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="fork-create-label">
              {t('timeline.forkCreateDialog.labelLabel')}
            </Label>
            <Input
              id="fork-create-label"
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder={t('timeline.forkCreateDialog.labelPlaceholder')}
            />
          </div>

          {error ? (
            <p
              className="text-copy-13 text-red-700"
              role="alert"
              data-testid="fork-create-dialog-error"
            >
              {error}
            </p>
          ) : null}

          <div className="flex justify-end gap-2 pt-2">
            <Button
              type="button"
              variant="tertiary"
              size="small"
              onClick={() => onOpenChange(false)}
            >
              {t('common:action.cancel')}
            </Button>
            <Button
              type="submit"
              variant="primary"
              size="small"
              disabled={submitting}
              data-testid="fork-create-submit"
            >
              {submitting
                ? t('timeline.forkCreateDialog.creating')
                : t('timeline.forkCreateDialog.create')}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
