import { type FormEventHandler, type ReactNode } from 'react';

import { Button } from './button';
import { Dialog, DialogContent } from './dialog';
import { Spinner } from './states';

/**
 * FormDialog — DESIGN.md §Component Primitives/FormDialog.
 *
 * V1.149 P1 — L2 pattern component: the form-in-modal pattern over the
 * promoted Dialog + Button primitives. Pure presentational: the caller owns
 * the field controls (rendered as children inside a native form), native
 * validation, the submit handler (`onSubmit`), and all copy. The dialog owns
 * layout only — title/description header, optional inline error slot
 * (role=alert + error-surface tokens), the right-aligned action bar, and the
 * `pending` busy state (actions disabled + spinner) so a slow submit cannot
 * be double-fired or cancelled mid-flight.
 */
export interface FormDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  /** Caller-owned field controls; rendered inside the native form. */
  children: ReactNode;
  /** Native form submit; the caller prevents default and runs its mutation. */
  onSubmit?: FormEventHandler<HTMLFormElement>;
  submitLabel: string;
  cancelLabel: string;
  /** Accessible name for the header dismiss control; defaults to "Close dialog". */
  closeLabel?: string;
  /** Caller mutation busy state: disables both actions and shows the spinner. */
  pending?: boolean;
  /** Inline error copy (e.g. a failed mutation message); renders the error slot. */
  error?: ReactNode;
}

export function FormDialog({
  open,
  onOpenChange,
  title,
  description,
  children,
  onSubmit,
  submitLabel,
  cancelLabel,
  closeLabel,
  pending = false,
  error,
}: FormDialogProps) {
  return (
    // Guard dismissal at the Root: while pending, the X control, Escape, and
    // overlay click all funnel through onOpenChange and must not fire.
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!pending) onOpenChange(next);
      }}
    >
      <DialogContent title={title} description={description} closeLabel={closeLabel}>
        {/* Always preventDefault: implicit submit (Enter key, requestSubmit)
            must never fall through to native navigation/reload, and pending
            intercepts the event before it reaches the caller's handler. */}
        <form
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            if (!pending) onSubmit?.(event);
          }}
        >
          {children}
          {error && (
            <div
              role="alert"
              className="rounded-card border border-error-surface-border bg-error-surface p-3 text-copy-14 text-red-900"
            >
              {error}
            </div>
          )}
          <div className="flex justify-end gap-2 pt-2">
            <Button
              type="button"
              variant="secondary"
              disabled={pending}
              onClick={() => onOpenChange(false)}
            >
              {cancelLabel}
            </Button>
            <Button type="submit" variant="primary" disabled={pending}>
              {pending && <Spinner />}
              {submitLabel}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
