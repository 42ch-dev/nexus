/**
 * Conflict resolution modal for the World KB canvas write boundary (V1.73).
 *
 * Two KB-flavored variants (compass §1.1 A6, exact copy) reuse the generic
 * {@link ConflictModalBase} shell so the World KB surface inherits focus
 * trapping, the ARIA live-region announcement, the reapply/use-current pattern,
 * and the side-by-side review panel from the V1.71/V1.72 surfaces. The variant
 * is selected by the originating inspector (`patch_entity` vs `promote_candidate`).
 */
import { useTranslation } from 'react-i18next';

import {
  ConflictModalBase,
  type ConflictField,
  type ConflictReviewRow,
} from '@/components/canvas/conflict-modal-base';

/** Fields editable through `world_kb.patch_entity`. */
export type WorldKbEntityField = 'title' | 'body' | 'aliases' | 'block_type';

const ENTITY_FIELD_LABEL_KEYS: Record<WorldKbEntityField, string> = {
  title: 'worldKb.conflict.field.title',
  body: 'worldKb.conflict.field.body',
  aliases: 'worldKb.conflict.field.aliases',
  block_type: 'worldKb.conflict.field.blockType',
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
  const { t } = useTranslation('canvas');
  const serverChanges: ConflictField<WorldKbEntityField>[] = draft.changedFields.map((c) => ({
    id: c.field,
    label: t(ENTITY_FIELD_LABEL_KEYS[c.field]),
    serverValue: c.to,
  }));

  const localChanges: ConflictField<WorldKbEntityField>[] = draft.fields.map((id) => ({
    id,
    label: t(ENTITY_FIELD_LABEL_KEYS[id]),
    localValue: draft.draftValues[id],
  }));

  const reviewRows: ConflictReviewRow[] = draft.fields.map((id) => {
    const change = draft.changedFields.find((c) => c.field === id);
    return {
      label: t(ENTITY_FIELD_LABEL_KEYS[id]),
      server: change?.to ?? change?.from ?? t('worldKb.conflict.modifiedByOther'),
      draft: draft.draftValues[id] ?? t('worldKb.conflict.yourEdit'),
      changed: Boolean(change),
    };
  });

  const editedFieldLabel = draft.fields[0] ? t(ENTITY_FIELD_LABEL_KEYS[draft.fields[0]]) : t('worldKb.conflict.fields');

  return (
    <ConflictModalBase<WorldKbEntityField>
      open={open}
      title={t('worldKb.conflict.entityTitle')}
      description={<>Nexus updated {bold(draft.entityName)} {t('worldKb.conflict.toVersion')}</>}
      descriptionSuffix={
        <>
          {' '}
          {t('worldKb.conflict.whileEditing', { field: editedFieldLabel.toLowerCase() })}
        </>
      }
      currentRevision={currentVersion}
      serverSectionTitle={t('worldKb.conflict.serverSection')}
      localSectionTitle={t('worldKb.conflict.localSection')}
      serverChanges={serverChanges}
      localChanges={localChanges}
      reviewRows={reviewRows}
      onUseCurrent={onUseCurrent}
      onReapply={onReapply}
      onDismiss={onDismiss}
      useCurrentLabel={t('worldKb.conflict.useCurrent')}
      reapplyLabel={t('worldKb.conflict.reapply')}
      keepEditingLabel={t('worldKb.conflict.cancel')}
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

export function WorldKbPromoteConflictModal({
  open,
  draft,
  currentVersion,
  onUseCurrent,
  onReapply,
  onDismiss,
}: WorldKbPromoteConflictModalProps) {
  const { t } = useTranslation('canvas');
  // The promote variant models a single promotion slot, but the user's pending
  // action is intentionally non-overlapping with the canonical action: "reapply"
  // means "redo my decision against the new version", not "clobber the same
  // field". Distinct ids keep ConflictModalBase's overlap guard from disabling
  // Reapply my decision (compass §1.1 A6 promote variant action tray).
  const serverChanges: ConflictField<'canonical-promotion'>[] = [
    {
      id: 'canonical-promotion',
      label: t('worldKb.conflict.candidateStatus', { status: draft.newStatus }),
      serverValue: t('worldKb.conflict.candidateWas', { name: draft.candidateName, status: draft.newStatus }),
    },
  ];

  const actionLabel = t(`worldKb.promotionInspector.action.${draft.action}.label`);
  const mergeSuffix = draft.mergeTargetLabel
    ? t('worldKb.conflict.mergeSuffix', { target: draft.mergeTargetLabel })
    : '';
  const localChanges: ConflictField<'pending-action'>[] = [
    {
      id: 'pending-action',
      label: actionLabel,
      localValue: t('worldKb.conflict.pendingAction', {
        action: actionLabel,
        name: draft.candidateName,
        suffix: mergeSuffix,
      }),
    },
  ];

  const reviewRows: ConflictReviewRow[] = [
    {
      label: t('worldKb.conflict.promotionState'),
      server: t('worldKb.conflict.promotionServer', { name: draft.candidateName, status: draft.newStatus }),
      draft: t('worldKb.conflict.pendingAction', {
        action: actionLabel,
        name: draft.candidateName,
        suffix: mergeSuffix,
      }),
      changed: true,
    },
  ];

  return (
    <ConflictModalBase<'canonical-promotion' | 'pending-action'>
      open={open}
      title={t('worldKb.conflict.promoteTitle')}
      description={
        <>
          {t('worldKb.conflict.promoteDescription', { status: draft.newStatus, name: draft.candidateName })}{' '}
          {t('worldKb.conflict.toVersion')}
        </>
      }
      descriptionSuffix={
        <>
          {t('worldKb.conflict.promoteSuffix', { action: draft.action })}
        </>
      }
      currentRevision={currentVersion}
      serverSectionTitle={t('worldKb.conflict.serverSection')}
      localSectionTitle={t('worldKb.conflict.localSection')}
      serverChanges={serverChanges}
      localChanges={localChanges}
      reviewRows={reviewRows}
      onUseCurrent={onUseCurrent}
      onReapply={onReapply}
      onDismiss={onDismiss}
      useCurrentLabel={t('worldKb.conflict.useCurrent')}
      reapplyLabel={t('worldKb.conflict.promoteReapply')}
      keepEditingLabel={t('worldKb.conflict.cancel')}
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
