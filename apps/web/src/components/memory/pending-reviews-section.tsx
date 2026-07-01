/**
 * PendingReviewsSection — extracted from MemoryPage (R-V179P1-QC1-001).
 *
 * Pending-review list (cursor-paginated) with a live count badge + delete, plus
 * the "Review & Summarize" CTA and the side inspector (detail-panel + row-action
 * hybrid, matching the V1.77 findings-page pattern). The page shell owns active
 * creator lookup + Card layout; this component owns the section's data + rows.
 *
 * API note: consumes the shipped memory Local API hooks as-is; the
 * useReviewMemory drain semantics live in @/api/queries (P0-owned, untouched).
 */
import { useMemo, useState } from 'react';
import { Loader2, RefreshCw, Sparkles, Trash2 } from 'lucide-react';

import { LoadMore } from '@/components/load-more';
import { MemoryDetailPanel } from '@/components/memory/memory-detail-panel';
import { TaskKindBadge } from '@/components/memory/task-kind-badge';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import {
  flattenPages,
  useDeletePendingReview,
  usePendingReviewCount,
  usePendingReviews,
  useReviewMemory,
} from '@/api/queries';
import { formatRelative, shortId } from '@/lib/format';
import type { PendingReviewInfo } from '@42ch/nexus-contracts';

export function PendingReviewsSection({ creatorId }: { creatorId: string }) {
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
