/**
 * World KB relationship conflict modal (V1.74 A6).
 *
 * Reuses {@link ConflictModalBase} with relationship-adapted copy. A 409 on
 * `patch_relationship` means another session changed the row; the author can
 * use the server version or reapply their edit against the new version.
 */
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
  const label = relationshipLabel(draft.form);
  const serverChanges: ConflictField<keyof RelationshipForm>[] = [
    {
      id: 'relationType',
      label: 'Relationship',
      serverValue: 'Modified by another session',
    },
  ];
  const localChanges: ConflictField<keyof RelationshipForm>[] = [
    {
      id: 'relationType',
      label: 'Relationship',
      localValue: `${draft.sourceName} ${label} ${draft.targetName}`,
    },
  ];
  const reviewRows: ConflictReviewRow[] = [
    {
      label: 'Relationship',
      server: 'Changed by another session',
      draft: `${draft.sourceName} ${label} ${draft.targetName}`,
      changed: true,
    },
  ];

  return (
    <ConflictModalBase<keyof RelationshipForm>
      open={open}
      title="This relationship changed while you were editing it."
      description={
        <>
          Nexus updated the relationship between {bold(draft.sourceName)} and {bold(draft.targetName)}{' '}
          (version
        </>
      }
      descriptionSuffix={
        <>
          {') while you were editing it. Your change is still in the inspector.'}
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

function relationshipLabel(form: RelationshipForm): string {
  if (form.relationType === 'custom' && form.customLabel) return form.customLabel;
  return RELATIONSHIP_KIND_LABELS[form.relationType]?.toLowerCase() ?? form.relationType;
}

function bold(value: string): React.ReactNode {
  return <strong className="font-semibold">{value}</strong>;
}
