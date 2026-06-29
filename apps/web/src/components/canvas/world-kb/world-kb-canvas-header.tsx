/**
 * World KB canvas header (V1.74 A10 split).
 *
 * Title, entry count, last-fetched staleness, view toggle, and refresh action.
 */
import { List, RefreshCw, Workflow } from 'lucide-react';

import { Button } from '@/components/ui/button';

interface WorldKbHeaderProps {
  entryCount: number;
  lastFetched: string;
  showList: boolean;
  onToggleView: () => void;
  onRefresh: () => void;
  refreshing: boolean;
}

export function WorldKbHeader({
  entryCount,
  lastFetched,
  showList,
  onToggleView,
  onRefresh,
  refreshing,
}: WorldKbHeaderProps) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-2">
      <div>
        <div className="flex items-center gap-2">
          <h2 className="text-heading-20 font-heading text-gray-1000">World KB</h2>
          {entryCount === 0 ? (
            <span className="rounded-pill bg-gray-alpha-100 px-2 py-0.5 text-label-12 text-gray-700">
              no entries yet
            </span>
          ) : (
            <span className="rounded-pill bg-gray-alpha-100 px-2 py-0.5 text-label-12 text-gray-700">
              {entryCount} {entryCount === 1 ? 'entry' : 'entries'} · fetched {lastFetched}
            </span>
          )}
        </div>
        <p className="text-copy-13 text-gray-700">
          {entryCount === 0
            ? 'Start adding characters, abilities, or scenes from the command palette (kb adopt/snapshot).'
            : 'Browse entities and promotion candidates. Edits are guarded by per-row version checks.'}
        </p>
      </div>
      <div className="flex items-center gap-2">
        <Button
          type="button"
          variant="tertiary"
          size="small"
          onClick={onToggleView}
          aria-pressed={showList}
        >
          {showList ? (
            <>
              <Workflow className="h-4 w-4" aria-hidden /> Show graph
            </>
          ) : (
            <>
              <List className="h-4 w-4" aria-hidden /> Show list view
            </>
          )}
        </Button>
        <Button type="button" variant="secondary" size="small" onClick={onRefresh} disabled={refreshing}>
          <RefreshCw className={`h-4 w-4 ${refreshing ? 'animate-spin' : ''}`} aria-hidden />
          Refresh now
        </Button>
      </div>
    </div>
  );
}
