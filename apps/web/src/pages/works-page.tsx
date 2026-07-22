import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Link, useNavigate } from 'react-router-dom';
import { Plus, RefreshCw } from 'lucide-react';

import { LoadMore } from '@/components/load-more';
import { StatusBadge } from '@/components/status-badge';
import { Button } from '@/components/ui/button';
import {
  Card, CardContent, CardDescription, CardHeader, CardTitle,
} from '@/components/ui/card';
import { EmptyCreateCard } from '@/components/ui/empty-create-card';
import {
  Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
} from '@/components/ui/table';
import { ErrorState, LoadingState } from '@/components/ui/states';
import { flattenPages, useWorks } from '@/api/queries';
import { formatRelative, shortId } from '@/lib/format';

import { CreateWorkDialog } from './dialogs/create-work-dialog';

/**
 * Works dashboard (Control Room — READ) — web-ui.md §6.1 #1.
 *
 * Cursor-paginated list (F-P1) of every Work with status + intake badges and a
 * relative "updated" timestamp. Status filter narrows the list server-side via
 * the `status` query param. Clicking a row opens the Work detail view.
 */
export function WorksPage() {
  const { t } = useTranslation('works');
  const navigate = useNavigate();
  const [statusFilter, setStatusFilter] = useState('');
  const [createOpen, setCreateOpen] = useState(false);
  const query = useWorks(statusFilter.trim() ? { status: statusFilter.trim() } : undefined);
  const works = useMemo(() => flattenPages(query.data), [query.data]);

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-1 flex-wrap items-center gap-2">
          <label htmlFor="works-status-filter" className="sr-only">
            {t('filterLabel')}
          </label>
          <input
            id="works-status-filter"
            type="search"
            value={statusFilter}
            onChange={(e) => setStatusFilter(e.target.value)}
            placeholder={t('filterPlaceholder')}
            className="h-9 w-full max-w-xs rounded-control border border-gray-alpha-400 bg-background-100 px-3 text-copy-14 text-gray-1000 placeholder:text-gray-700"
          />
          <Button
            type="button"
            variant="tertiary"
            size="small"
            onClick={() => query.refetch()}
            disabled={query.isFetching}
            aria-label={t('refreshAria')}
          >
            <RefreshCw className={`h-4 w-4 ${query.isFetching ? 'animate-spin' : ''}`} aria-hidden />
            {t('refresh')}
          </Button>
        </div>
        <Button type="button" variant="primary" size="small" onClick={() => setCreateOpen(true)}>
          <Plus className="h-4 w-4" aria-hidden />
          {t('create')}
        </Button>
      </div>

      <Card className="shadow-card">
        <CardHeader>
          {/* V1.121 v0.4: Works list page title is content voice (serif
              display-20) per DESIGN.md §Design Concept — the Works list is the
              creative-entity index surface. CardTitle voice="content" is the
              canonical P1 contract for card-level entity titles. */}
          <CardTitle voice="content">{t('title')}</CardTitle>
          <CardDescription>{t('description')}</CardDescription>
        </CardHeader>
        <CardContent>
          {query.isError ? (
            <ErrorState
              description={t('errorDescription')}
              onRetry={() => query.refetch()}
            />
          ) : query.isLoading ? (
            <LoadingState label={t('loading')} />
          ) : works.length === 0 ? (
            <EmptyCreateCard
              icon={Plus}
              title={t('emptyCreateTitle')}
              description={t('emptyCreateDescription')}
              onClick={() => setCreateOpen(true)}
              data-testid="works-empty-create"
            />
          ) : (
            <>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>{t('columns.title')}</TableHead>
                    <TableHead>{t('columns.status')}</TableHead>
                    <TableHead>{t('columns.intake')}</TableHead>
                    <TableHead>{t('columns.preset')}</TableHead>
                    <TableHead>{t('columns.updated')}</TableHead>
                    {/* V1.123 P3 Task 3 — Timeline activity surface. The
                        Works dashboard doubles as a Timeline activity
                        index: each row exposes a peer link to the Work
                        Timeline surface (`/works/<workId>/timeline`,
                        V1.123 P2 T5). The existing Updated column stays —
                        it was already a Timeline activity proxy via
                        `formatRelative(updated_at)`. Per plan honest scope
                        cut, per-Work outline fetches for event counts are
                        deferred to a future composite endpoint. */}
                    <TableHead>{t('columns.timeline')}</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {works.map((w) => (
                    <TableRow key={w.work_id}>
                      <TableCell>
                        <Link
                          to={`/works/${encodeURIComponent(w.work_id)}/outline`}
                          className="font-medium text-brand-deep-blue hover:underline dark:text-blue-700 dark:hover:text-blue-800"
                        >
                          {w.title || t('untitled')}
                        </Link>
                        <div className="text-copy-13-mono text-gray-700">{shortId(w.work_id)}</div>
                      </TableCell>
                      <TableCell><StatusBadge status={w.status} /></TableCell>
                      <TableCell><StatusBadge status={w.intake_status} /></TableCell>
                      <TableCell>
                        <span className="text-copy-13-mono text-gray-900">{shortId(w.primary_preset_id)}</span>
                      </TableCell>
                      <TableCell className="text-gray-900">{formatRelative(w.updated_at)}</TableCell>
                      {/* V1.123 P3 Task 3 — per-work Timeline link. The
                          testid is namespaced by work_id so tests can
                          target a specific work row without ambiguity. The
                          link is a peer of the row's primary outline link;
                          both preserve their workId in the path. */}
                      <TableCell>
                        <Link
                          to={`/works/${encodeURIComponent(w.work_id)}/timeline`}
                          data-testid={`work-timeline-link-${w.work_id}`}
                          aria-label={t('viewTimelineAria', { workId: w.work_id })}
                          className="inline-flex items-center gap-1 text-brand-deep-blue hover:underline dark:text-blue-700 dark:hover:text-blue-800"
                        >
                          {t('viewTimeline')}
                        </Link>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
              <LoadMore
                isFetchingNextPage={query.isFetchingNextPage}
                hasNextPage={query.hasNextPage}
                fetchNextPage={() => query.fetchNextPage()}
              />
            </>
          )}
        </CardContent>
      </Card>

      <CreateWorkDialog
        open={createOpen}
        onOpenChange={setCreateOpen}
        onCreated={(workId) => {
          navigate(`/works/${encodeURIComponent(workId)}/outline`);
        }}
      />
    </div>
  );
}
