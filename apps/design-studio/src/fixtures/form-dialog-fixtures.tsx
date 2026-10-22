import { useState } from 'react';

import {
  Button,
  FormDialog,
  Input,
  Label,
} from '@42ch/nexus-ui';

/**
 * Studio Components fixture for `<FormDialog>` (V1.149 P1 promotion).
 *
 * The promoted form-in-modal pattern over the promoted Dialog + Button
 * primitives. The caller owns the fields (rendered inside a native form),
 * native validation, the submit handler, and all copy. The pending busy
 * state and the inline error slot are demoed live via toggles.
 */
export function FormDialogFixtures() {
  const [open, setOpen] = useState(false);
  const [pending, setPending] = useState(false);
  const [showError, setShowError] = useState(false);
  const [submitted, setSubmitted] = useState(0);

  return (
    <div data-testid="form-dialog-fixtures">
      <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-6">
        <div className="flex flex-wrap items-center gap-3">
          <Button variant="primary" data-testid="form-dialog-open" onClick={() => setOpen(true)}>
            New project…
          </Button>
          <label className="inline-flex items-center gap-2 text-copy-14 text-gray-900">
            <input
              type="checkbox"
              data-testid="form-dialog-pending-toggle"
              checked={pending}
              onChange={(e) => setPending(e.target.checked)}
            />
            Simulate pending
          </label>
          <label className="inline-flex items-center gap-2 text-copy-14 text-gray-900">
            <input
              type="checkbox"
              data-testid="form-dialog-error-toggle"
              checked={showError}
              onChange={(e) => setShowError(e.target.checked)}
            />
            Show submit error
          </label>
        </div>
        <p className="text-copy-13 text-gray-700 mt-4">
          Submitted: <span data-testid="form-dialog-submit-count">{submitted}</span>
        </p>
        <FormDialog
          open={open}
          onOpenChange={setOpen}
          title="New project"
          description="Create a project inside the current workspace."
          submitLabel="Create project"
          cancelLabel="Cancel"
          pending={pending}
          error={showError ? 'Creation failed: the name is already taken.' : undefined}
          onSubmit={() => setSubmitted((n) => n + 1)}
        >
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="form-dialog-name">Project name</Label>
            <Input
              id="form-dialog-name"
              data-testid="form-dialog-name-input"
              required
              placeholder="e.g. Harbor light"
            />
          </div>
        </FormDialog>
        <p className="text-copy-13 text-gray-700 mt-4">
          Promoted{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @42ch/nexus-ui
          </code>{' '}
          FormDialog — form-in-modal over the promoted Dialog. Fields render
          inside a native form (implicit submit is always preventDefaulted);
          while pending, actions disable and dismissal is guarded at the root.
        </p>
      </div>
    </div>
  );
}
