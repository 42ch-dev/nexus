/**
 * Conflict resolution modal for the Outline+Timeline canvas write boundary.
 *
 * Reuses the generic {@link ConflictModalBase} shell so the Strategy and Outline
 * surfaces share focus trapping, live-region announcements, and the reapply/
 * use-current action pattern. The outline conflict payload does not carry full
 * server-side field values, so we surface the conflicting path and the local
 * draft fields.
 */
import {
  ConflictModalBase,
  type ConflictField,
  type ConflictReviewRow,
} from '@/components/canvas/conflict-modal-base';

export type OutlineChangedField =
  | 'chapter_title'
  | 'chapter_slug'
  | 'chapter_volume'
  | 'chapter_status'
  | 'chapter_planned_word_count'
  | 'chapter_actual_word_count'
  | 'move_chapter'
  | 'attach_to_volume'
  | 'link_event'
  | 'add_event'
  | 'remove_event'
  | 'attach_event_to_chapter'
  | 'link_foreshadow';

const FIELD_LABELS: Record<OutlineChangedField, string> = {
  chapter_title: 'Chapter title',
  chapter_slug: 'Chapter slug',
  chapter_volume: 'Chapter volume',
  chapter_status: 'Chapter status',
  chapter_planned_word_count: 'Planned word count',
  chapter_actual_word_count: 'Actual word count',
  move_chapter: 'Move chapter',
  attach_to_volume: 'Attach chapter to volume',
  link_event: 'Link event to chapter',
  add_event: 'Add timeline event',
  remove_event: 'Remove timeline event',
  attach_event_to_chapter: 'Attach event to chapter',
  link_foreshadow: 'Foreshadow link',
};

export interface OutlineConflictModalDraft {
  fields: OutlineChangedField[];
  conflictingPath: string;
}

export interface OutlineConflictModalProps {
  open: boolean;
  currentRevision: number;
  draft: OutlineConflictModalDraft;
  onUseCurrent: () => void;
  onReapply: () => void;
  onDismiss: () => void;
}

export function OutlineConflictModal({
  open,
  currentRevision,
  draft,
  onUseCurrent,
  onReapply,
  onDismiss,
}: OutlineConflictModalProps) {
  const serverChanges: ConflictField[] = draft.conflictingPath
    ? [
        {
          id: 'conflicting_path',
          label: 'Outline structure changed',
          serverValue: draft.conflictingPath,
        },
      ]
    : [];

  const localChanges: ConflictField[] = draft.fields.map((id) => ({
    id,
    label: FIELD_LABELS[id],
  }));

  const reviewRows: ConflictReviewRow[] = draft.fields.map((id) => ({
    label: FIELD_LABELS[id],
    server: draft.conflictingPath ? 'Modified by another session' : '(unknown)',
    draft: 'Your pending edit',
    changed: true,
  }));

  return (
    <ConflictModalBase
      open={open}
      title="This outline changed while you were editing."
      currentRevision={currentRevision}
      serverSectionTitle="What changed on the server"
      localSectionTitle="What you were about to do"
      serverChanges={serverChanges}
      localChanges={localChanges}
      reviewRows={reviewRows}
      onUseCurrent={onUseCurrent}
      onReapply={onReapply}
      onDismiss={onDismiss}
    />
  );
}
