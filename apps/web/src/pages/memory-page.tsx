import { useMemo, useState } from 'react';
import { Loader2, RefreshCw, Sparkles, Trash2 } from 'lucide-react';

import { LoadMore } from '@/components/load-more';
import { MemoryDetailPanel } from '@/components/memory/memory-detail-panel';
import { TaskKindBadge } from '@/components/memory/task-kind-badge';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Label } from '@/components/ui/label';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import {
  flattenPages,
  useActiveCreatorId,
  useDeletePendingReview,
  useMemoryFragments,
  usePendingReviewCount,
  usePendingReviews,
  useReviewMemory,
} from '@/api/queries';
import { formatRelative, shortId } from '@/lib/format';
import type { PendingReviewInfo } from '@42ch/nexus-contracts';

/**
 * Memory view (Control Room) — V1.78 Creator Memory review-loop surface
 * (web-ui.md §24).
 *
 * Creator-scoped (`creator_id` on every memory endpoint). The active creator id
 * is derived from existing sessions (mirrors the canvas `useDerivedCreatorId`
 * pattern); the daemon rejects a mismatched creator with 403. Three affordances
 * consuming the shipped memory Local API:
 *   1. Pending-review list (cursor-paginated) with a live count badge + delete.
 *   2. "Review & Summarize" CTA → POST /memory/review with a processing state,
 *      then a result-counters toast; invalidates pending + count + fragments.
 *   3. Fragments browser (read-only, optional keyword filter).
 *
 * Layout (D-UX LOCKED): detail-panel + row-action hybrid — a Control-Room table
 * with a side inspector (matching the V1.77 findings-page pattern), not a
 * canvas graph. `createPendingReview` stays CLI/producer-only.
 */
export function MemoryPage() {
  const creatorId = useActiveCreatorId();

  return (
    <Card className="shadow-card">
      <CardHeader>
        <CardTitle>Memory</CardTitle>
        <CardDescription>
          Review pending captures, summarize them into long-term memory, and browse fragments.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {!creatorId ? (
          <EmptyState
            title="No active creator"
            description="Start a session or schedule so the memory surface can resolve your creator identity."
          />
        ) : (
          <div className="flex flex-col gap-8">
            <PendingReviewsSection creatorId={creatorId} />
            <FragmentsSection creatorId={creatorId} />
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// ── Pending reviews + Review & Summarize CTA + inspector ─────────────────────

function PendingReviewsSection({ creatorId }: { creatorId: string }) {
  const reviews = usePendingReviews(creatorId);
  const count = usePendingReviewCount(creatorId);
  const deleteReview = useDeletePendingReview();
  const reviewMemory = useReviewMemory();
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const rows = useMemo(() => flattenPages(reviews.data), [reviews.data]);
  const pendingCount = count.data?.count;
  const hasPending = typeof pendingCount === 'number' ? pendingCount > 0 : rows.length > 0;

  // The selected row comes from the list cache (optimistically updated by
  // useDeletePendingReview), so the inspector reflects in-flight deletes and
  // dismisses cleanly once the row is gone.
  const selected: PendingReviewInfo | null = useMemo(
    () => rows.find((r) => r.pending_id === selectedId) ?? null,
    [rows, selectedId],
  );

  const confirmDelete = (pending: PendingReviewInfo) => {
    if (
      !window.confirm(
        `Delete this pending review?\n\n${shortId(pending.pending_id)} · ${pending.task_kind}`,
      )
    ) {
      return;
    }
    deleteReview.mutate({ pendingId: pending.pending_id, creatorId });
    if (selected?.pending_id === pending.pending_id) setSelectedId(null);
  };

  const runReview = () => {
    reviewMemory.mutate(creatorId);
    setSelectedId(null);
  };

  return (
    <section data-testid="memory-pending-section">
      <div className="mb-4 flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <h2 className="text-heading-16 text-gray-1000">Pending Reviews</h2>
          {/* memory-pending-count badge — red numeric indicator (DESIGN.md token). */}
          <Badge
            variant="error"
            className="tabular-nums"
            data-testid="memory-pending-count"
            aria-label={`${pendingCount ?? 0} pending reviews`}
          >
            {pendingCount ?? '—'}
          </Badge>
        </div>
        <div className="flex items-center gap-2">
          <Button
            type="button"
            variant="tertiary"
            size="small"
            onClick={() => {
              void reviews.refetch();
              void count.refetch();
            }}
            disabled={reviews.isFetching}
            aria-label="Refresh pending reviews"
          >
            <RefreshCw className={`h-4 w-4 ${reviews.isFetching ? 'animate-spin' : ''}`} aria-hidden />
            Refresh
          </Button>
          {/* memory-review-button — primary accent CTA (DESIGN.md token). */}
          <Button
            type="button"
            variant="primary"
            size="small"
            onClick={runReview}
            disabled={!hasPending || reviewMemory.isPending}
            aria-label="Review and summarize pending captures"
          >
            {reviewMemory.isPending ? (
              <Loader2 className="h-4 w-4 animate-spin" aria-hidden />
            ) : (
              <Sparkles className="h-4 w-4" aria-hidden />
            )}
            {reviewMemory.isPending ? 'Summarizing…' : 'Review & Summarize'}
          </Button>
        </div>
      </div>

      {reviews.isError ? (
        <ErrorState description="Could not load pending reviews." onRetry={() => reviews.refetch()} />
      ) : reviews.isLoading ? (
        <LoadingState label="Loading pending reviews…" />
      ) : rows.length === 0 ? (
        <EmptyState
          title="No pending reviews"
          description="Captures from your sessions will appear here for review."
        />
      ) : (
        <div className="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_360px]">
          <div className="min-w-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Kind</TableHead>
                  <TableHead>Session</TableHead>
                  <TableHead>Digest</TableHead>
                  <TableHead>Captured</TableHead>
                  <TableHead aria-label="Row actions" />
                </TableRow>
              </TableHeader>
              <TableBody>
                {rows.map((r) => {
                  const isActive = r.pending_id === selectedId;
                  return (
                    <TableRow
                      key={r.pending_id}
                      onClick={() => setSelectedId(isActive ? null : r.pending_id)}
                      className={`cursor-pointer ${isActive ? 'bg-background-300' : ''}`}
                    >
                      <TableCell>
                        <TaskKindBadge taskKind={r.task_kind} />
                      </TableCell>
                      <TableCell className="text-copy-13-mono text-gray-900">
                        {shortId(r.session_id)}
                      </TableCell>
                      <TableCell className="max-w-[320px] truncate text-gray-900" title={r.raw_digest}>
                        {r.raw_digest}
                      </TableCell>
                      <TableCell className="whitespace-nowrap tabular-nums text-gray-900">
                        {formatRelative(r.created_at)}
                      </TableCell>
                      <TableCell onClick={(e) => e.stopPropagation()}>
                        <Button
                          type="button"
                          variant="tertiary"
                          size="small"
                          onClick={() => confirmDelete(r)}
                          disabled={deleteReview.isPending}
                          aria-label={`Delete pending review ${shortId(r.pending_id)}`}
                        >
                          <Trash2 className="h-4 w-4" aria-hidden />
                        </Button>
                      </TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
            <LoadMore
              isFetchingNextPage={reviews.isFetchingNextPage}
              hasNextPage={reviews.hasNextPage}
              fetchNextPage={() => reviews.fetchNextPage()}
              label="Load more pending reviews"
            />
          </div>

          <aside className="lg:sticky lg:top-4 lg:self-start">
            {selected ? (
              <Card className="shadow-card">
                <CardHeader>
                  <CardTitle className="text-heading-16">Pending Review Details</CardTitle>
                  <CardDescription className="text-copy-13-mono">
                    {shortId(selected.pending_id)}
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  <MemoryDetailPanel
                    pending={selected}
                    deletePending={deleteReview.isPending}
                    onDelete={() => confirmDelete(selected)}
                  />
                </CardContent>
              </Card>
            ) : (
              <EmptyState
                title="No pending review selected"
                description="Select a row to inspect its full context, or delete it."
              />
            )}
          </aside>
        </div>
      )}
    </section>
  );
}

// ── Fragments browser (read-only; optional keyword filter) ──────────────────

function FragmentsSection({ creatorId }: { creatorId: string }) {
  const [keyword, setKeyword] = useState('');
  const trimmed = keyword.trim();
  const fragments = useMemoryFragments(creatorId, trimmed ? { keyword: trimmed } : undefined);
  const rows = fragments.data?.fragments ?? [];

  return (
    <section data-testid="memory-fragments-section">
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <h2 className="text-heading-16 text-gray-1000">Fragments</h2>
        <p className="text-copy-13 text-gray-700">
          Long-term memory produced by reviewing pending captures. Read-only.
        </p>
      </div>
      {/* memory-fragment-filter-input — keyword filter (DESIGN.md token). */}
      <div className="mb-4 flex flex-col gap-1.5">
        <Label htmlFor="memory-fragment-filter">Filter by keyword</Label>
        <input
          id="memory-fragment-filter"
          type="search"
          value={keyword}
          onChange={(e) => setKeyword(e.target.value)}
          placeholder="Filter fragments (case-insensitive)"
          className="h-10 w-full max-w-[320px] rounded-control border border-gray-alpha-400 bg-background-100 px-3 text-copy-14 text-gray-1000 placeholder:text-gray-700"
        />
      </div>
      {fragments.isError ? (
        <ErrorState description="Could not load fragments." onRetry={() => fragments.refetch()} />
      ) : fragments.isLoading ? (
        <LoadingState label="Loading fragments…" />
      ) : rows.length === 0 ? (
        <EmptyState
          title="No fragments"
          description={trimmed ? 'No fragments match this keyword.' : 'Run Review & Summarize to produce fragments.'}
        />
      ) : (
        <ul className="flex flex-col divide-y divide-gray-alpha-400 rounded-control border border-gray-alpha-400">
          {rows.map((f) => (
            <li
              key={f.fragment_id}
              className="flex flex-col gap-1 px-3 py-2.5"
              data-testid="memory-fragment-row"
            >
              <div className="flex items-center gap-2">
                {/* memory-fragment-id — monospace badge (DESIGN.md token). */}
                <span className="text-copy-13-mono text-gray-800">{f.fragment_id}</span>
              </div>
              {/* memory-fragment-summary — text token (DESIGN.md token). */}
              <p className="whitespace-pre-wrap break-words text-copy-14 leading-[1.5] text-gray-1000">
                {f.summary}
              </p>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
