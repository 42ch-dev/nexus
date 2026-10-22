import { useState } from 'react';

import {
  Button,
  ConfirmDialog,
} from '@42ch/nexus-ui';

/**
 * Studio Components fixture for `<ConfirmDialog>` (V1.149 P1 promotion).
 *
 * The promoted destructive/irreversible-action confirmation over the
 * promoted Dialog + Button primitives. Caller owns open state and all copy;
 * the pending busy state is demoed live via the toggle. Dismissal (X,
 * Escape, overlay) is guarded while pending.
 */
export function ConfirmDialogFixtures() {
  const [open, setOpen] = useState(false);
  const [pending, setPending] = useState(false);
  const [confirmed, setConfirmed] = useState(0);
  const [cancelled, setCancelled] = useState(0);

  return (
    <div data-testid="confirm-dialog-fixtures">
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        <div className="flex flex-wrap items-center gap-3">
          <Button
            variant="destructive"
            data-testid="confirm-dialog-open"
            onClick={() => setOpen(true)}
          >
            Delete workspace…
          </Button>
          <label className="inline-flex items-center gap-2 text-copy-14 text-gray-900">
            <input
              type="checkbox"
              data-testid="confirm-dialog-pending-toggle"
              checked={pending}
              onChange={(e) => setPending(e.target.checked)}
            />
            Simulate pending (mutation busy)
          </label>
        </div>
        <p className="text-copy-13 text-gray-700 mt-4">
          Confirmed: <span data-testid="confirm-dialog-confirmed-count">{confirmed}</span>
          {' · '}Cancelled:{' '}
          <span data-testid="confirm-dialog-cancelled-count">{cancelled}</span>
        </p>
        <ConfirmDialog
          open={open}
          onOpenChange={(next) => {
            setOpen(next);
            if (!next) setCancelled((n) => n + 1);
          }}
          title="Delete workspace?"
          description="This permanently deletes the workspace and all of its runs. This action cannot be undone."
          confirmLabel="Delete workspace"
          cancelLabel="Cancel"
          destructive
          pending={pending}
          onConfirm={() => {
            setConfirmed((n) => n + 1);
            setOpen(false);
          }}
        />
        <p className="text-copy-13 text-gray-700 mt-4">
          Promoted{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @42ch/nexus-ui
          </code>{' '}
          ConfirmDialog — destructive confirmation over the promoted Dialog.
          While pending, both actions disable, the confirm shows a spinner,
          and X / Escape / overlay dismissal is guarded at the dialog root.
        </p>
      </div>
    </div>
  );
}
