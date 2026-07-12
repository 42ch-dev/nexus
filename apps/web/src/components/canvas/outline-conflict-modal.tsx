/**
 * Conflict resolution modal for the Outline+Timeline canvas write boundary.
 *
 * Reuses the generic {@link ConflictModalBase} shell so the Strategy and Outline
 * surfaces share focus trapping, live-region announcements, and the reapply/
 * use-current action pattern. The outline conflict payload does not carry full
 * server-side field values, so we surface the conflicting path and the local
 * draft fields.
 */
import { useTranslation } from 'react-i18next';
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
  | 'chapter_outline_content'
  | 'move_chapter'
  | 'attach_to_volume'
  | 'link_event'
  | 'add_event'
  | 'remove_event'
  | 'attach_event_to_chapter'
  | 'link_foreshadow'
  | 'unlink_foreshadow';

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
  const { t } = useTranslation('canvas');

  const fieldLabels: Record<OutlineChangedField, string> = {
    chapter_title: t('outlineConflict.fields.chapter_title'),
    chapter_slug: t('outlineConflict.fields.chapter_slug'),
    chapter_volume: t('outlineConflict.fields.chapter_volume'),
    chapter_status: t('outlineConflict.fields.chapter_status'),
    chapter_planned_word_count: t('outlineConflict.fields.chapter_planned_word_count'),
    chapter_actual_word_count: t('outlineConflict.fields.chapter_actual_word_count'),
    chapter_outline_content: t('outlineConflict.fields.chapter_outline_content'),
    move_chapter: t('outlineConflict.fields.move_chapter'),
    attach_to_volume: t('outlineConflict.fields.attach_to_volume'),
    link_event: t('outlineConflict.fields.link_event'),
    add_event: t('outlineConflict.fields.add_event'),
    remove_event: t('outlineConflict.fields.remove_event'),
    attach_event_to_chapter: t('outlineConflict.fields.attach_event_to_chapter'),
    link_foreshadow: t('outlineConflict.fields.link_foreshadow'),
    unlink_foreshadow: t('outlineConflict.fields.unlink_foreshadow'),
  };

  const serverChanges: ConflictField[] = draft.conflictingPath
    ? [
        {
          id: 'conflicting_path',
          label: t('outlineConflict.server.structureChanged'),
          serverValue: draft.conflictingPath,
        },
      ]
    : [];

  const localChanges: ConflictField[] = draft.fields.map((id) => ({
    id,
    label: fieldLabels[id],
  }));

  const reviewRows: ConflictReviewRow[] = draft.fields.map((id) => ({
    label: fieldLabels[id],
    server: draft.conflictingPath ? t('outlineConflict.review.modifiedByOther') : t('outlineConflict.review.unknown'),
    draft: t('outlineConflict.review.yourEdit'),
    changed: true,
  }));

  return (
    <ConflictModalBase
      open={open}
      title={t('outlineConflict.title')}
      currentRevision={currentRevision}
      serverSectionTitle={t('outlineConflict.serverSectionTitle')}
      localSectionTitle={t('outlineConflict.localSectionTitle')}
      serverChanges={serverChanges}
      localChanges={localChanges}
      reviewRows={reviewRows}
      onUseCurrent={onUseCurrent}
      onReapply={onReapply}
      onDismiss={onDismiss}
    />
  );
}
