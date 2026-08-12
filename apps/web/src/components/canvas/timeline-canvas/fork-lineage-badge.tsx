/**
 * Fork lineage badge — V1.162 P2 T2 (read-only lineage chrome, §3.3.2).
 *
 * Tells the author the ACTIVE branch is a fork (marker-derived `is_fork`),
 * shows the parent branch + fork-point event read-only (from the
 * `fork_created` marker's `extensions.fork_lineage` — spec §6.6.3 carrier
 * B), and offers ONE-HOP return to the parent Timeline.
 *
 * Read-only by contract (plan Global Constraints): no merge, no lineage
 * edit, no branch-comparison workspace — the only affordance is the
 * parent-hop control. `is_fork` comes from branch-level marker presence,
 * NEVER the world-level WorldState fork fields.
 *
 * Dumb presentational component: the orchestrator passes the already-
 * derived `ForkLineage` + an `onOpenParent` callback wired to T1's
 * branch-context mechanism (`setActiveBranchId(parent_branch_id)` → the
 * timeline-events query re-keys → the parent Timeline renders).
 */
import { useTranslation } from 'react-i18next';
import { GitFork } from 'lucide-react';

import { Button } from '@/components/ui';
import { shortId } from '@/lib/format';
import type { ForkLineage } from '@/api/queries';

export interface ForkLineageBadgeProps {
  /** Branch-level marker-derived lineage (T2 hook output). */
  lineage: ForkLineage;
  /**
   * One-hop parent hand-off. The orchestrator wires
   * `handleBranchChange(lineage.parent_branch_id)`; omitted when the
   * lineage carries no parent (defensive — a fork marker always has one).
   */
  onOpenParent?: () => void;
}

export function ForkLineageBadge({ lineage, onOpenParent }: ForkLineageBadgeProps) {
  const { t } = useTranslation('canvas');
  if (!lineage.is_fork) return null;
  return (
    <div
      data-testid="fork-lineage-badge"
      role="note"
      className="flex flex-wrap items-center gap-x-3 gap-y-2 rounded-card border border-gray-alpha-400 bg-background-100 px-3 py-2 text-copy-13 text-gray-700 shadow-elevation-2"
    >
      <span className="flex items-center gap-1.5 font-medium text-gray-900">
        <GitFork className="h-4 w-4 text-gray-700" aria-hidden />
        {t('timeline.forkLineage.badge')}
      </span>
      <span data-testid="fork-lineage-parent">
        {t('timeline.forkLineage.parentLabel')}:{' '}
        {lineage.parent_branch_id ? shortId(lineage.parent_branch_id) : '—'}
      </span>
      <span data-testid="fork-lineage-fork-point">
        {t('timeline.forkLineage.forkPointLabel')}:{' '}
        {lineage.forked_from_event_id ? shortId(lineage.forked_from_event_id) : '—'}
      </span>
      {lineage.parent_branch_id && onOpenParent ? (
        <Button
          type="button"
          variant="secondary"
          size="small"
          onClick={onOpenParent}
          data-testid="fork-lineage-open-parent"
          aria-label={t('timeline.forkLineage.openParentAria')}
        >
          {t('timeline.forkLineage.openParent')}
        </Button>
      ) : null}
    </div>
  );
}
