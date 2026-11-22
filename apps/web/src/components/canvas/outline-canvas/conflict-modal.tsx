/**
 *
 * Thin wrapper that composes the shared `OutlineConflictModal` shell with the
 * outline-specific field projection (`changedFieldsOf`). Extracted from the
 * original `outline-canvas.tsx` monolith so the orchestrator stays focused on
 * state + mutation dispatch; behavior is unchanged.
 */
import { OutlineConflictModal } from '@/components/canvas/outline-conflict-modal';

import { changedFieldsOf, type ConflictState } from './graph-projection';

interface OutlineConflictDialogProps {
  conflict: ConflictState | null;
  onUseCurrent: () => void;
  onReapply: () => void;
  onDismiss: () => void;
}

/**
 */
export function OutlineConflictDialog({
  conflict,
  onUseCurrent,
  onReapply,
  onDismiss,
}: OutlineConflictDialogProps) {
  if (!conflict) return null;
  return (
    <OutlineConflictModal
      open
      currentRevision={conflict.currentRevision}
      draft={{
        fields: changedFieldsOf(conflict.pendingRequest),
        conflictingPath: conflict.conflictingPath,
      }}
      onUseCurrent={onUseCurrent}
      onReapply={onReapply}
      onDismiss={onDismiss}
    />
  );
}
