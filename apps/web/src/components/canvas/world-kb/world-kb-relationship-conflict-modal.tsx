/**
 * World KB relationship conflict modal (V1.74 A6).
 *
 * Reuses {@link ConflictModalBase} with relationship-adapted copy. A 409 on
 * `patch_relationship` means another session changed the row; the author can
 * use the server version or reapply their edit against the new version.
 */
import { useTranslation } from 'react-i18next';

import {
  ConflictModalBase,
  type ConflictField,
  type ConflictReviewRow,
} from '@/components/canvas/conflict-modal-base';
import type { RelationshipForm } from './relationship-inspector';
import { RELATIONSHIP_KIND_LABELS } from './relationship-projection';

export interface WorldKbRelationshipConflictDraft {
  relationshipId: string;
  sourceName: string;
  targetName: string;
  form: RelationshipForm;
}

export interface WorldKbRelationshipConflictModalProps {
  open: boolean;
  draft: WorldKbRelationshipConflictDraft;
  currentVersion: number;
  onUseCurrent: () => void;
  onReapply: () => void;
  onDismiss: () => void;
}

export function WorldKbRelationshipConflictModal({
  open,
  draft,
  currentVersion,
  onUseCurrent,
  onReapply,
  onDismiss,
}: WorldKbRelationshipConflictModalProps) {
  const { t } = useTranslation('canvas');
  const label = relationshipLabel(draft.form);
  const fieldLabelKey = editedFieldLabelKeyFor(draft.form);
  const serverChanges: ConflictField<keyof RelationshipForm>[] = [
    {
      id: 'relationType',
      label: t(fieldLabelKey),
      serverValue: t('worldKb.conflict.modifiedByOther'),
    },
  ];
  const localChanges: ConflictField<keyof RelationshipForm>[] = [
    {
      id: 'relationType',
      label: t(fieldLabelKey),
      localValue: `${draft.sourceName} ${label} ${draft.targetName}`,
    },
  ];
  const reviewRows: ConflictReviewRow[] = [
    {
      label: t(fieldLabelKey),
      server: t('worldKb.conflict.changedByOther'),
      draft: `${draft.sourceName} ${label} ${draft.targetName}`,
      changed: true,
    },
  ];

  return (
    <ConflictModalBase<keyof RelationshipForm>
      open={open}
      title={t('worldKb.conflict.relationshipTitle')}
      description={
        <>
          {t('worldKb.conflict.relationshipDescription', { source: draft.sourceName, target: draft.targetName })}
          {' '}
          {t('worldKb.conflict.toVersion')}
        </>
      }
      descriptionSuffix={
        <>
          {' '}
          {t('worldKb.conflict.relationshipSuffix', { field: capitalize(t(fieldLabelKey)) })}
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

function editedFieldLabelKeyFor(form: RelationshipForm): string {
  if (form.relationType === 'custom') return 'worldKb.conflict.field.customLabel';
  return 'worldKb.conflict.field.relationType';
}

function capitalize(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1);
}

function relationshipLabel(form: RelationshipForm): string {
  if (form.relationType === 'custom' && form.customLabel) return form.customLabel;
  return RELATIONSHIP_KIND_LABELS[form.relationType]?.toLowerCase() ?? form.relationType;
}

