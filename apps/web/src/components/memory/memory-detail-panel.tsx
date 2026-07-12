/**
 * Memory detail/inspector panel — V1.78 Creator Memory review-loop surface.
 *
 * Spec: `.mstar/specs/web-ui.md` §24 + compass D-UX LOCKED. Read-only
 * context for the selected pending-review row, matching the V1.77
 * `FindingDetailPanel` layout (detail-panel + row-action hybrid). The Memory
 * surface is review/consume-only: there is no inline edit here (unlike
 * findings), only the row-level delete affordance the parent page owns.
 *
 * Renders all 6 `PendingReviewInfo` fields: `pending_id` (monospace badge),
 * `session_id`, `world_id` (or "(none)" per open item #3), `task_kind`
 * (humanized chip), `raw_digest` (scrollable preformatted area), `created_at`
 * (RFC 3339 → author's local time).
 */
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { TaskKindBadge } from '@/components/memory/task-kind-badge';
import { formatDateTime, shortId } from '@/lib/format';
import type { PendingReviewInfo } from '@42ch/nexus-contracts';
import { useTranslation } from 'react-i18next';

interface MemoryDetailPanelProps {
  pending: PendingReviewInfo;
  /** Pending state of the parent's delete mutation (disables the row action). */
  deletePending?: boolean;
  /** Delete the selected pending-review row. */
  onDelete?: () => void;
}

export function MemoryDetailPanel({ pending, deletePending, onDelete }: MemoryDetailPanelProps) {
  const { t } = useTranslation('memory');
  return (
    <div className="flex flex-col gap-4">
      <section className="flex flex-wrap items-center gap-2">
        <Badge className="text-copy-13-mono">{shortId(pending.pending_id)}</Badge>
        <TaskKindBadge taskKind={pending.task_kind} />
        {onDelete && (
          <Button
            type="button"
            variant="destructive"
            size="small"
            onClick={onDelete}
            disabled={deletePending}
            className="ml-auto"
            aria-label={`${t('pending.deleteRowAria', { id: shortId(pending.pending_id) })}`}
          >
            {t('detail.delete')}
          </Button>
        )}
      </section>

      <section className="flex flex-col gap-1.5 text-copy-13">
        <div className="flex flex-col gap-0.5">
          <Label className="text-gray-900">{t('detail.sessionLabel')}</Label>
          <span className="text-copy-13-mono text-gray-900" data-testid="memory-session-id">
            {pending.session_id}
          </span>
        </div>
        <div className="flex flex-col gap-0.5">
          <Label className="text-gray-900">{t('detail.worldLabel')}</Label>
          <span className="text-gray-1000" data-testid="memory-world-id">
            {pending.world_id?.trim() ? pending.world_id : t('detail.noWorld')}
          </span>
        </div>
        <div className="flex flex-col gap-0.5">
          <Label className="text-gray-900">{t('detail.capturedLabel')}</Label>
          <span className="text-gray-1000" data-testid="memory-created-at">
            {formatDateTime(pending.created_at)}
          </span>
        </div>
      </section>

      <section className="flex flex-col gap-1.5">
        <Label htmlFor="memory-raw-digest" className="text-gray-900">
          {t('detail.rawDigestLabel')}
        </Label>
        <pre
          id="memory-raw-digest"
          data-testid="memory-raw-digest"
          className="max-h-64 overflow-auto whitespace-pre-wrap break-words rounded-control border border-gray-alpha-400 bg-background-200 p-3 text-copy-13 text-gray-1000"
        >
          {pending.raw_digest}
        </pre>
      </section>

      <section className="flex flex-col gap-0.5 border-t border-gray-alpha-400 pt-3 text-copy-13 text-gray-900">
        <span>
          {t('detail.pendingIdLabel', { id: pending.pending_id })}
        </span>
      </section>
    </div>
  );
}
