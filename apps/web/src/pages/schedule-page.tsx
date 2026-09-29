import { RefreshCw } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { StatusBadge } from '@/components/status-badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import { useActiveCreatorId, useSchedules } from '@/api/queries';
import { formatRelative, shortId } from '@/lib/format';
import { CreateScheduleDialog } from './dialogs/create-schedule-dialog';

/**
 * Schedule view (Control Room — read-only schedule identities) — web-ui.md §6.1 #3.
 *
 * ScheduleSummary does not carry a next-fire timestamp, so we show the
 * last-updated relative time — never a fabricated next-run (PL-17).
 * V1.171 P2 retains schedule creation via the existing POST endpoint. Per-Work
 * cron declaration editing remains on CLI `creator works cron`.
 */
export function SchedulePage() {
  const { t } = useTranslation('schedule');
  const schedules = useSchedules();
  const creatorId = useActiveCreatorId();
  const [createOpen, setCreateOpen] = useState(false);

  return (
    <div className="flex flex-col gap-4">
      <Card className="shadow-card">
        <CardHeader>
          <div className="flex items-center justify-between gap-2">
            <div>
              <CardTitle>{t('title')}</CardTitle>
              <CardDescription>{t('description')}</CardDescription>
            </div>
            <div className="flex items-center gap-2">
              <Button
                type="button"
                variant="primary"
                size="small"
                onClick={() => setCreateOpen(true)}
                disabled={!creatorId}
                title={creatorId ? undefined : t('create.noCreatorDescription')}
              >
                {t('create.trigger')}
              </Button>
              <Button
                type="button"
                variant="tertiary"
                size="small"
                onClick={() => schedules.refetch()}
                disabled={schedules.isFetching}
                aria-label={t('refreshAria')}
              >
                <RefreshCw className={`h-4 w-4 ${schedules.isFetching ? 'animate-spin' : ''}`} aria-hidden />
                {t('refresh')}
              </Button>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          {schedules.isError ? (
            <ErrorState description={t('errorDescription')} onRetry={() => schedules.refetch()} />
          ) : schedules.isLoading ? (
            <LoadingState label={t('loading')} />
          ) : !schedules.data || schedules.data.length === 0 ? (
            <EmptyState title={t('emptyTitle')} description={t('emptyDescription')} />
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t('columns.schedule')}</TableHead>
                  <TableHead>{t('columns.label')}</TableHead>
                  <TableHead>{t('columns.status')}</TableHead>
                  <TableHead>{t('columns.preset')}</TableHead>
                  <TableHead>{t('columns.coreCtx')}</TableHead>
                  <TableHead>{t('columns.updated')}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {schedules.data.map((s) => (
                  <TableRow key={s.schedule_id}>
                    <TableCell><span className="text-copy-13-mono text-gray-1000">{shortId(s.schedule_id)}</span></TableCell>
                    <TableCell>{s.label?.trim() ? s.label : <span className="text-gray-700">—</span>}</TableCell>
                    <TableCell><StatusBadge status={s.status} /></TableCell>
                    <TableCell><span className="text-copy-13-mono text-gray-900">{shortId(s.preset_id)}</span></TableCell>
                    <TableCell>
                      <span className="tabular-nums text-copy-13-mono text-gray-900">v{s.current_core_context_version}</span>
                    </TableCell>
                    <TableCell className="text-gray-900">{formatRelative(s.updated_at)}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>


      {creatorId && (
        <CreateScheduleDialog open={createOpen} onOpenChange={setCreateOpen} creatorId={creatorId} />
      )}
    </div>
  );
}
