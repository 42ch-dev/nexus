/**
 * Conflict resolution modal for the World KB canvas write boundary (V1.73).
 *
 * Two KB-flavored variants (compass §1.1 A6, exact copy) reuse the generic
 * {@link ConflictModalBase} shell so the World KB surface inherits focus
 * trapping, the ARIA live-region announcement, the reapply/use-current pattern,
 * and the side-by-side review panel from the V1.71/V1.72 surfaces. The variant
 * is selected by the originating inspector (`patch_entity` vs `promote_candidate`).
 */
import {
  ConflictModalBase,
  type ConflictField,
  type ConflictReviewRow,
} from '@/components/canvas/conflict-modal-base';

/** Fields editable through `world_kb.patch_entity`. */
export type WorldKbEntityField = 'title' | 'body' | 'aliases' | 'block_type';

const ENTITY_FIELD_LABELS: Record<WorldKbEntityField, string> = {
  title: 'Title',
  body: 'Body',
  aliases: 'Aliases',
  block_type: 'Block Type',
};

/** Draft carried by the `patch_entity` conflict modal. */
export interface WorldKbEntityConflictDraft {
  entityName: string;
  /** Fields the user's draft touches (drives overlap detection). */
  fields: WorldKbEntityField[];
  /** Canonical values that now differ from the user's last known version. */
  changedFields: Array<{ field: WorldKbEntityField; from?: string; to?: string }>;
  /** The user's pending field values, for the "What you were about to do" block. */
  draftValues: Partial<Record<WorldKbEntityField, string>>;
}

export interface WorldKbEntityConflictModalProps {
  open: boolean;
  draft: WorldKbEntityConflictDraft;
  currentVersion: number;
  onUseCurrent: () => void;
  onReapply: () => void;
  onDismiss: () => void;
}

/** `patch_entity` variant — entity inspector conflict. */
export function WorldKbEntityConflictModal({
  open,
  draft,
  currentVersion,
  onUseCurrent,
  onReapply,
  onDismiss,
}: WorldKbEntityConflictModalProps) {
  const serverChanges: ConflictField<WorldKbEntityField>[] = draft.changedFields.map((c) => ({
    id: c.field,
    label: ENTITY_FIELD_LABELS[c.field],
    serverValue: c.to,
  }));

  const localChanges: ConflictField<WorldKbEntityField>[] = draft.fields.map((id) => ({
    id,
    label: ENTITY_FIELD_LABELS[id],
    localValue: draft.draftValues[id],
  }));

  const reviewRows: ConflictReviewRow[] = draft.fields.map((id) => {
    const change = draft.changedFields.find((c) => c.field === id);
    return {
      label: ENTITY_FIELD_LABELS[id],
      server: change?.to ?? change?.from ?? 'Modified by another session',
      draft: draft.draftValues[id] ?? 'Your pending edit',
      changed: Boolean(change),
    };
  });

  const editedFieldLabel = draft.fields[0] ? ENTITY_FIELD_LABELS[draft.fields[0]] : 'fields';

  return (
    <ConflictModalBase<WorldKbEntityField>
      open={open}
      title="This world entry changed while you were editing."
      description={<>Nexus updated {bold(draft.entityName)} to version</>}
      descriptionSuffix={
        <>
          {' '}
          while you were editing its {bold(editedFieldLabel.toLowerCase())}. Your change is still
          in the inspector.
        </>
      }
      currentRevision={currentVersion}
      serverSectionTitle="What changed"
      localSectionTitle="What you were about to do"
      serverChanges={serverChanges}
      localChanges={localChanges}
      reviewRows={reviewRows}
      onUseCurrent={onUseCurrent}
      onReapply={onReapply}
      onDismiss={onDismiss}
      useCurrentLabel="Use current"
      reapplyLabel="Reapply my edit"
      keepEditingLabel="Cancel"
    />
  );
}

/** Pending promotion action carried by the `promote_candidate` conflict modal. */
export type WorldKbPromoteAction = 'adopt' | 'reject' | 'merge';

/** Canonical promotion action that already occurred server-side. */
export type WorldKbCanonicalStatus = 'adopted' | 'rejected' | 'merged';

export interface WorldKbPromoteConflictDraft {
  candidateName: string;
  /** The canonical promotion action that already occurred. */
  newStatus: WorldKbCanonicalStatus;
  /** The user's pending promote action. */
  action: WorldKbPromoteAction;
  /** Merge target id, when the user's action is `merge`. */
  mergeTargetId?: string;
  /** Merge target label, when the user's action is `merge`. */
  mergeTargetLabel?: string;
}

export interface WorldKbPromoteConflictModalProps {
  open: boolean;
  draft: WorldKbPromoteConflictDraft;
  currentVersion: number;
  onUseCurrent: () => void;
  onReapply: () => void;
  onDismiss: () => void;
}

const PROMOTE_ACTION_LABEL: Record<WorldKbPromoteAction, string> = {
  adopt: 'Adopt',
  reject: 'Reject',
  merge: 'Merge',
};

/** `promote_candidate` variant — promotion inspector conflict. */
export function WorldKbPromoteConflictModal({
  open,
  draft,
  currentVersion,
  onUseCurrent,
  onReapply,
  onDismiss,
}: WorldKbPromoteConflictModalProps) {
  // The promote variant models a single promotion slot, but the user's pending
  // action is intentionally non-overlapping with the canonical action: "reapply"
  // means "redo my decision against the new version", not "clobber the same
  // field". Distinct ids keep ConflictModalBase's overlap guard from disabling
  // Reapply my decision (compass §1.1 A6 promote variant action tray).
  const serverChanges: ConflictField<'canonical-promotion'>[] = [
    {
      id: 'canonical-promotion',
      label: `Candidate ${draft.newStatus}`,
      serverValue: `${draft.candidateName} was ${draft.newStatus}`,
    },
  ];

  const mergeSuffix = draft.mergeTargetLabel ? ` into ${bold(draft.mergeTargetLabel)} (confirmed)` : '';
  const localChanges: ConflictField<'pending-action'>[] = [
    {
      id: 'pending-action',
      label: PROMOTE_ACTION_LABEL[draft.action],
      localValue: `${PROMOTE_ACTION_LABEL[draft.action]} ${draft.candidateName}${mergeSuffix}`,
    },
  ];

  const reviewRows: ConflictReviewRow[] = [
    {
      label: 'Promotion state',
      server: `${draft.candidateName} is now ${draft.newStatus}`,
      draft: `${PROMOTE_ACTION_LABEL[draft.action]} ${draft.candidateName}${mergeSuffix}`,
      changed: true,
    },
  ];

  return (
    <ConflictModalBase<'canonical-promotion' | 'pending-action'>
      open={open}
      title="This candidate's state changed while you were reviewing it."
      description={
        <>
          Nexus {draft.newStatus} {bold(draft.candidateName)} (version
        </>
      }
      descriptionSuffix={
        <>
          {') while you were about to '}
          {bold(draft.action)}
          {' it. Your decision is still in the inspector.'}
        </>
      }
      currentRevision={currentVersion}
      serverSectionTitle="What changed"
      localSectionTitle="What you were about to do"
      serverChanges={serverChanges}
      localChanges={localChanges}
      reviewRows={reviewRows}
      onUseCurrent={onUseCurrent}
      onReapply={onReapply}
      onDismiss={onDismiss}
      useCurrentLabel="Use current"
      reapplyLabel="Reapply my decision"
      keepEditingLabel="Cancel"
    />
  );
}

/**
 * Wrap a value in a <strong> so the entity/candidate name reads with emphasis in
 * the conflict body. The base modal renders the description inline, so this
 * returns a React node rather than a templated string.
 */
function bold(value: string): React.ReactNode {
  return <strong className="font-semibold">{value}</strong>;
}
