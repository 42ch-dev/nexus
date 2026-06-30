/**
 * Memory detail/inspector panel — V1.78 Creator Memory review-loop surface.
 *
 * Spec: `.mstar/knowledge/specs/web-ui.md` §24 + compass D-UX LOCKED. Read-only
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

interface MemoryDetailPanelProps {
  pending: PendingReviewInfo;
  /** Pending state of the parent's delete mutation (disables the row action). */
  deletePending?: boolean;
  /** Delete the selected pending-review row. */
  onDelete?: () => void;
}

export function MemoryDetailPanel({ pending, deletePending, onDelete }: MemoryDetailPanelProps) {
  return (
    <div className="flex flex-col gap-4">
      {/* ── Identity + delete action ─────────────────────────────────────── */}
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
            aria-label={`Delete pending review ${shortId(pending.pending_id)}`}
          >
            Delete
          </Button>
        )}
      </section>

      {/* ── Context fields ──────────────────────────────────────────────── */}
      <section className="flex flex-col gap-1.5 text-copy-13">
        <div className="flex flex-col gap-0.5">
          <Label className="text-gray-900">Session</Label>
          <span className="text-copy-13-mono text-gray-900" data-testid="memory-session-id">
            {pending.session_id}
          </span>
        </div>
        <div className="flex flex-col gap-0.5">
          <Label className="text-gray-900">World</Label>
          {/* open item #3: absent world_id reads as "(none)" in the inspector. */}
          <span className="text-gray-1000" data-testid="memory-world-id">
            {pending.world_id?.trim() ? pending.world_id : '(none)'}
          </span>
        </div>
        <div className="flex flex-col gap-0.5">
          <Label className="text-gray-900">Captured</Label>
          {/* created_at is RFC 3339; display in the author's local time. */}
          <span className="text-gray-1000" data-testid="memory-created-at">
            {formatDateTime(pending.created_at)}
          </span>
        </div>
      </section>

      {/* ── Raw digest (scrollable; max 64 KB per handler validation) ────── */}
      <section className="flex flex-col gap-1.5">
        <Label htmlFor="memory-raw-digest" className="text-gray-900">
          Raw Digest
        </Label>
        <pre
          id="memory-raw-digest"
          data-testid="memory-raw-digest"
          className="max-h-64 overflow-auto whitespace-pre-wrap break-words rounded-control border border-gray-alpha-400 bg-background-200 p-3 text-copy-13 text-gray-1000"
        >
          {pending.raw_digest}
        </pre>
      </section>

      {/* ── Full id readout ─────────────────────────────────────────────── */}
      <section className="flex flex-col gap-0.5 border-t border-gray-alpha-400 pt-3 text-copy-13 text-gray-900">
        <span>
          Pending ID:{' '}
          <span className="text-copy-13-mono text-gray-700" data-testid="memory-pending-id-full">
            {pending.pending_id}
          </span>
        </span>
      </section>
    </div>
  );
}
