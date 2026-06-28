/**
 * Conflict resolution modal for the Strategy canvas write boundary.
 *
 * Implements the acceptance UX from canvas-strategy-surface.md §3.5:
 * - Headline: "This state changed while you were editing."
 * - Field-level summaries of server-side changes and local draft changes.
 * - Three actions: Use current, Reapply my edit, Review side-by-side.
 * - Accessibility: focus trap, live region, return focus, reduced-motion.
 *
 * This module is now a thin wrapper around {@link ConflictModalBase} so the
 * same shell can be reused for the Outline+Timeline canvas.
 */
import type { PresetState } from '@/lib/canvas/preset-yaml';
import {
  ConflictModalBase,
  type ConflictField,
  type ConflictReviewRow,
} from '@/components/canvas/conflict-modal-base';

export type ChangedField = 'label' | 'description' | 'nextTarget' | 'promptBody';

export interface ConflictModalDraft {
  label: string;
  description: string;
  nextTarget: string;
  promptBody: string;
}

export interface ConflictModalProps {
  open: boolean;
  currentRevision: number;
  draft: ConflictModalDraft;
  canonicalState?: PresetState;
  promptTemplateRef?: string;
  changedFields: readonly ChangedField[];
  onUseCurrent: () => void;
  onReapply: () => void;
  onDismiss: () => void;
}

export function ConflictModal({
  open,
  currentRevision,
  draft,
  canonicalState,
  promptTemplateRef,
  changedFields,
  onUseCurrent,
  onReapply,
  onDismiss,
}: ConflictModalProps) {
  const canonicalLabel = canonicalState?.id ?? '';
  const canonicalDescription = canonicalState?.description ?? '';
  const canonicalNext =
    typeof canonicalState?.next === 'string' ? canonicalState.next : '';

  const fieldMeta: Record<ChangedField, { label: string; server: string; draft: string }> = {
    label: { label: 'State label', server: canonicalLabel, draft: draft.label },
    description: { label: 'Description', server: canonicalDescription, draft: draft.description },
    nextTarget: { label: 'Next target', server: canonicalNext, draft: draft.nextTarget },
    promptBody: {
      label: 'Prompt template',
      server: promptTemplateRef ? '(server content not fetched)' : '(none)',
      draft: draft.promptBody || '(empty)',
    },
  };

  const serverChanges: ConflictField[] = [];
  if (canonicalLabel !== draft.label) {
    serverChanges.push({ id: 'label', label: fieldMeta.label.label, serverValue: canonicalLabel });
  }
  if (canonicalDescription !== draft.description) {
    serverChanges.push({
      id: 'description',
      label: fieldMeta.description.label,
      serverValue: canonicalDescription,
    });
  }
  if (canonicalNext !== draft.nextTarget) {
    serverChanges.push({
      id: 'nextTarget',
      label: fieldMeta.nextTarget.label,
      serverValue: canonicalNext,
    });
  }
  // Prompt body changes are not reflected in the manifest; we cannot detect
  // server-side prompt edits without a separate template-read contract.
  // Only treat the prompt as conflicted when the user actually changed it AND
  // the changedFields list confirms the conflict was on the prompt path.
  if (draft.promptBody && changedFields.includes('promptBody')) {
    serverChanges.push({
      id: 'promptBody',
      label: fieldMeta.promptBody.label,
      serverValue: promptTemplateRef ? '(server content not fetched)' : undefined,
    });
  }

  const localChanges: ConflictField[] = changedFields.map((id) => ({
    id,
    label: fieldMeta[id].label,
    localValue: fieldMeta[id].draft,
  }));

  const reviewRows: ConflictReviewRow[] = [
    { label: 'State label', server: canonicalLabel, draft: draft.label, changed: changedFields.includes('label') },
    {
      label: 'Description',
      server: canonicalDescription,
      draft: draft.description,
      changed: changedFields.includes('description'),
    },
    {
      label: 'Next target',
      server: canonicalNext,
      draft: draft.nextTarget,
      changed: changedFields.includes('nextTarget'),
    },
    ...(promptTemplateRef
      ? [
          {
            label: `Prompt template (${promptTemplateRef})`,
            server: '(server content not fetched)',
            draft: draft.promptBody || '(empty)',
            changed: changedFields.includes('promptBody'),
          } satisfies ConflictReviewRow,
        ]
      : []),
  ];

  return (
    <ConflictModalBase
      open={open}
      title="This state changed while you were editing."
      currentRevision={currentRevision}
      serverChanges={serverChanges}
      localChanges={localChanges}
      reviewRows={reviewRows}
      onUseCurrent={onUseCurrent}
      onReapply={onReapply}
      onDismiss={onDismiss}
    />
  );
}

