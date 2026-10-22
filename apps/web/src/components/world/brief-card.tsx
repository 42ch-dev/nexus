/**
 * Brief saved card (plan v1219-world-module T2) — the era KnowledgeEntry
 * rendered after a successful save: tag `BRIEF · {relative timestamp}`, the
 * one-sentence summary, and a follow-up `世界已就绪` card announcing the Work
 * module unlock (product-owned copy). An edit affordance reopens the inline
 * editor (per-row OCC update on save).
 */
import { useTranslation } from 'react-i18next';

import { formatRelative } from '@/lib/format';

export interface BriefCardProps {
  text: string;
  updatedAt?: string;
  /** Canonical title — rendered as a heading when it differs from the summary. */
  title?: string;
  onEdit: () => void;
}

/**
 * Product-owned relative timestamp: `刚刚` for a fresh save (<45s, the
 * second-bucket cutover formatRelative uses), the shared relative formatter
 * afterwards.
 */
function briefWhen(updatedAt: string, justNow: string): string {
  const ms = Date.parse(updatedAt);
  if (Number.isNaN(ms)) return formatRelative(updatedAt);
  return Date.now() - ms < 45_000 ? justNow : formatRelative(updatedAt);
}

export function BriefCard({ text, updatedAt, title, onEdit }: BriefCardProps) {
  const { t } = useTranslation('worlds');
  return (
    <div className="rounded-card border border-gray-500 p-4" data-testid="world-brief-card">
      <div className="flex items-start justify-between gap-2">
        <p className="text-copy-14 text-gray-700" data-testid="world-brief-tag">
          {t('worldApp.briefCard.tag', {
            when: briefWhen(updatedAt ?? new Date().toISOString(), t('worldApp.briefCard.justNow')),
          })}
        </p>
        <button
          type="button"
          data-testid="world-brief-edit"
          className="rounded-control px-2 py-0.5 text-copy-14 text-gray-700 hover:bg-gray-alpha-100"
          onClick={onEdit}
        >
          {t('worldApp.briefCard.edit')}
        </button>
      </div>
      {title && title !== text && (
        <h3 className="text-heading-16 font-heading text-gray-1000">{title}</h3>
      )}
      <p className="text-copy-14 text-gray-1000">{text}</p>
    </div>
  );
}

/** Follow-up card shown after the Brief saves — announces the Work unlock. */
export function BriefReadyCard() {
  const { t } = useTranslation('worlds');
  return (
    <div className="rounded-card border border-gray-500 p-4" data-testid="world-brief-ready">
      <h3 className="text-heading-16 font-heading text-gray-1000">
        {t('worldApp.briefReady.title')}
      </h3>
      <p className="text-copy-14 text-gray-900">{t('worldApp.briefReady.body')}</p>
    </div>
  );
}
