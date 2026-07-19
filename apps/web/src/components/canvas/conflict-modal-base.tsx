/**
 * Conflict-modal base — App i18n adapter over presentational chrome.
 *
 * Domain wrappers (Strategy / Outline / World KB) keep importing this module
 * for translated defaults. The visual shell lives in
 * `presentational/conflict-modal-chrome.tsx` (Studio: `@web-canvas/conflict-modal-chrome`).
 */
import { useTranslation } from 'react-i18next';

import {
  ConflictModalChrome,
  type ConflictField,
  type ConflictModalChromeProps,
  type ConflictReviewRow,
} from '@/components/canvas/presentational/conflict-modal-chrome';

export type { ConflictField, ConflictReviewRow };

/** @deprecated Prefer ConflictModalChromeProps — kept for existing wrappers. */
export type ConflictModalBaseProps<T extends string = string> =
  ConflictModalChromeProps<T>;

/**
 * i18n-aware conflict modal shell. Supplies canvas-namespace defaults, then
 * delegates markup/a11y to {@link ConflictModalChrome}.
 */
export function ConflictModalBase<T extends string = string>({
  open,
  title,
  description,
  descriptionSuffix,
  currentRevision,
  revisionLabel,
  serverSectionTitle,
  localSectionTitle,
  serverChanges,
  localChanges,
  reviewRows,
  onUseCurrent,
  onReapply,
  onDismiss,
  useCurrentLabel,
  reapplyLabel,
  keepEditingLabel,
  reviewLabel,
}: ConflictModalBaseProps<T>) {
  const { t } = useTranslation('canvas');

  const localIds = new Set(localChanges.map((f) => f.id));
  const overlap = serverChanges.filter((f) => localIds.has(f.id));

  return (
    <ConflictModalChrome
      open={open}
      title={title}
      description={description}
      descriptionSuffix={descriptionSuffix}
      currentRevision={currentRevision}
      revisionLabel={
        revisionLabel ??
        t('conflict.revisionLabel', { revision: currentRevision })
      }
      defaultDescription={t('conflict.defaultDescription')}
      serverSectionTitle={
        serverSectionTitle ?? t('conflict.serverSectionTitle')
      }
      localSectionTitle={localSectionTitle ?? t('conflict.localSectionTitle')}
      serverNoChangesLabel={t('conflict.server.noChanges')}
      localNoChangesLabel={t('conflict.local.noChanges')}
      serverChanges={serverChanges}
      localChanges={localChanges}
      reviewRows={reviewRows}
      onUseCurrent={onUseCurrent}
      onReapply={onReapply}
      onDismiss={onDismiss}
      useCurrentLabel={useCurrentLabel ?? t('conflict.useCurrent')}
      reapplyLabel={reapplyLabel ?? t('conflict.reapplyLabel')}
      keepEditingLabel={keepEditingLabel ?? t('conflict.keepEditing')}
      reviewLabel={reviewLabel ?? t('conflict.reviewButton')}
      reapplyTitleEnabled={t('conflict.reapply.title.enabled')}
      reapplyTitleDisabled={t('conflict.reapply.title.disabled', {
        fields: overlap.map((f) => f.label).join(', '),
      })}
      liveRevisionText={t('conflict.liveRegion.revision', {
        revision: currentRevision,
      })}
      liveLocalChangesText={t('conflict.liveRegion.localChanges', {
        fields:
          localChanges.map((f) => f.label).join(', ') ||
          t('conflict.liveRegion.nothing'),
      })}
      liveServerChangesText={t('conflict.liveRegion.serverChanges', {
        fields:
          serverChanges.map((f) => f.label).join(', ') ||
          t('conflict.liveRegion.nothingDetectable'),
      })}
      liveOverlapText={t('conflict.liveRegion.overlap', {
        fields: overlap.map((f) => f.label).join(', '),
      })}
      liveNoOverlapText={t('conflict.liveRegion.noOverlap')}
      liveNothingLabel={t('conflict.liveRegion.nothing')}
      liveNothingDetectableLabel={t('conflict.liveRegion.nothingDetectable')}
      reviewServerLabel={(label) => t('conflict.review.server', { label })}
      reviewLocalLabel={(label) => t('conflict.review.local', { label })}
      reviewUnchangedSuffix={t('conflict.review.unchanged')}
    />
  );
}
