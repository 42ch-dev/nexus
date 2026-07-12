import { RefreshCw } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { StatusBadge } from '@/components/status-badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import { useSessions } from '@/api/queries';
import { shortId } from '@/lib/format';

/**
 * Orchestration sessions view (Control Room — READ) — web-ui.md §6.1 #2.
 *
 * Lists every orchestration session with its status (running/completed/failed),
 * preset, and creator. The endpoint is un-paginated; the F-F1 sort keeps a
 * stable order (SessionSummary has no timestamp).
 */
export function SessionsPage() {
  const { t } = useTranslation('sessions');
  const sessions = useSessions();

  return (
    <Card className="shadow-card">
      <CardHeader>
        <div className="flex items-center justify-between gap-2">
          <div>
            <CardTitle>{t('title')}</CardTitle>
            <CardDescription>{t('description')}</CardDescription>
          </div>
          <Button
            type="button"
            variant="tertiary"
            size="small"
            onClick={() => sessions.refetch()}
            disabled={sessions.isFetching}
            aria-label={t('refreshAria')}
          >
            <RefreshCw className={`h-4 w-4 ${sessions.isFetching ? 'animate-spin' : ''}`} aria-hidden />
            {t('refresh')}
          </Button>
        </div>
      </CardHeader>
      <CardContent>
        {sessions.isError ? (
          <ErrorState
            description={t('errorDescription')}
            onRetry={() => sessions.refetch()}
          />
        ) : sessions.isLoading ? (
          <LoadingState label={t('loading')} />
        ) : !sessions.data || sessions.data.length === 0 ? (
          <EmptyState title={t('emptyTitle')} description={t('emptyDescription')} />
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t('columns.session')}</TableHead>
                <TableHead>{t('columns.status')}</TableHead>
                <TableHead>{t('columns.preset')}</TableHead>
                <TableHead>{t('columns.creator')}</TableHead>
                <TableHead>{t('columns.currentTask')}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {sessions.data.map((s) => (
                <TableRow key={s.session_id}>
                  <TableCell><span className="text-copy-13-mono text-gray-1000">{shortId(s.session_id)}</span></TableCell>
                  <TableCell><StatusBadge status={s.status} /></TableCell>
                  <TableCell><span className="text-copy-13-mono text-gray-900">{shortId(s.preset_id)}</span></TableCell>
                  <TableCell><span className="text-copy-13-mono text-gray-900">{shortId(s.creator_id)}</span></TableCell>
                  <TableCell>
                    {s.current_task_id ? (
                      <span className="text-copy-13-mono text-gray-900">{shortId(s.current_task_id)}</span>
                    ) : (
                      <span className="text-gray-700">—</span>
                    )}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}
