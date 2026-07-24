/**
 * PendingReviewsSection — extracted from MemoryPage (R-V179P1-QC1-001).
 *
 * Pending-review list (cursor-paginated) with a live count badge + delete, plus
 * the "Review & Summarize" CTA and the side inspector (detail-panel + row-action
 * hybrid, matching the V1.77 findings-page pattern). The page shell owns active
 * creator lookup + Card layout; this component owns the section's data + rows.
 *
 * API note: consumes the shipped memory Daemon API hooks as-is; the
 * useReviewMemory drain semantics live in @/api/queries (P0-owned, untouched).
 */
import { useMemo, useState } from 'react';
import { Loader2, RefreshCw, Sparkles, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';

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
  const { t } = useTranslation('memory');
  const reviews = usePendingReviews(creatorId);
  const count = usePendingReviewCount(creatorId);
  const deleteReview = useDeletePendingReview();
  const reviewMemory = useReviewMemory();
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const rows = useMemo(() => flattenPages(reviews.data), [reviews.data]);
  const pendingCount = count.data?.count;
  const hasPending = typeof pendingCount === 'number' ? pendingCount > 0 : rows.length > 0;

  const selected = useMemo(
    () => rows.find((r) => r.pending_id === selectedId) ?? null,
    [rows, selectedId],
  );

  const confirmDelete = (pending: PendingReviewInfo) => {
    if (
      !window.confirm(
        t('pending.deleteConfirm', { id: shortId(pending.pending_id), kind: t(`taskKind.${pending.task_kind}` as const) }),
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
          <h2 className="text-heading-16 text-gray-1000">{t('pending.title')}</h2>
          <Badge
            variant="error"
            className="tabular-nums"
            data-testid="memory-pending-count"
            aria-label={t('pending.countAria', { count: pendingCount ?? 0 })}
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
            aria-label={t('pending.refreshAria')}
          >
            <RefreshCw className={`h-4 w-4 ${reviews.isFetching ? 'animate-spin' : ''}`} aria-hidden />
            {t('pending.refresh')}
          </Button>
          <Button
            type="button"
            variant="primary"
            size="small"
            onClick={runReview}
            disabled={!hasPending || reviewMemory.isPending}
            aria-label={t('pending.reviewAria')}
          >
            {reviewMemory.isPending ? (
              <Loader2 className="h-4 w-4 animate-spin" aria-hidden />
            ) : (
              <Sparkles className="h-4 w-4" aria-hidden />
            )}
            {reviewMemory.isPending ? t('pending.summarizing') : t('pending.review')}
          </Button>
        </div>
      </div>

      {reviews.isError ? (
        <ErrorState description={t('pending.error')} onRetry={() => reviews.refetch()} />
      ) : reviews.isLoading ? (
        <LoadingState label={t('pending.loading')} />
      ) : rows.length === 0 ? (
        <EmptyState
          title={t('pending.emptyTitle')}
          description={t('pending.emptyDescription')}
        />
      ) : (
        <div className="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_360px]">
          <div className="min-w-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t('pending.columns.kind')}</TableHead>
                  <TableHead>{t('pending.columns.session')}</TableHead>
                  <TableHead>{t('pending.columns.digest')}</TableHead>
                  <TableHead>{t('pending.columns.captured')}</TableHead>
                  <TableHead aria-label={t('pending.columns.actions')} />
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
                            aria-label={`${t('pending.deleteRowAria', { id: shortId(r.pending_id) })}`}
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
              label={t('pending.loadMore')}
            />
          </div>

          <aside className="lg:sticky lg:top-4 lg:self-start">
            {selected ? (
              <Card className="shadow-card">
                <CardHeader>
                  <CardTitle className="text-heading-16">{t('pending.detailTitle')}</CardTitle>
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
                title={t('pending.noSelectionTitle')}
                description={t('pending.noSelectionDescription')}
              />
            )}
          </aside>
        </div>
      )}
    </section>
  );
}
