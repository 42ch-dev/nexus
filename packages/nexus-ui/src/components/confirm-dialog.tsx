import { Button } from './button';
import { Dialog, DialogContent } from './dialog';
import { Spinner } from './states';

/**
 * ConfirmDialog — DESIGN.md §Component Primitives/ConfirmDialog.
 *
 * V1.149 P1 — L2 pattern component: the destructive/irreversible-action
 * confirmation over the promoted Dialog + Button primitives. Pure
 * presentational: caller owns all copy, the confirmation decision
 * (`onConfirm`), and open state (`open` / `onOpenChange`). `pending` is the
 * caller's mutation busy signal — it disables both actions and shows the
 * busy spinner so a slow confirm cannot be double-fired or cancelled
 * mid-flight. `destructive` maps the confirm action to the Button
 * destructive recipe (red-800 fill); otherwise it uses the primary recipe.
 */
export interface ConfirmDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  confirmLabel: string;
  cancelLabel: string;
  destructive?: boolean;
  /** Caller mutation busy state: disables both actions and shows the spinner. */
  pending?: boolean;
  onConfirm: () => void;
}

export function ConfirmDialog({
  open,
  onOpenChange,
  title,
  description,
  confirmLabel,
  cancelLabel,
  destructive = false,
  pending = false,
  onConfirm,
}: ConfirmDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent title={title} description={description}>
        <div className="flex justify-end gap-2 pt-2">
          <Button variant="secondary" disabled={pending} onClick={() => onOpenChange(false)}>
            {cancelLabel}
          </Button>
          <Button
            variant={destructive ? 'destructive' : 'primary'}
            disabled={pending}
            onClick={onConfirm}
          >
            {pending && <Spinner />}
            {confirmLabel}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
