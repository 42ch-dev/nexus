import { RefreshCw } from 'lucide-react';

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
  const sessions = useSessions();

  return (
    <Card className="shadow-card">
      <CardHeader>
        <div className="flex items-center justify-between gap-2">
          <div>
            <CardTitle>Sessions</CardTitle>
            <CardDescription>What the runtime is doing right now.</CardDescription>
          </div>
          <Button
            type="button"
            variant="tertiary"
            size="small"
            onClick={() => sessions.refetch()}
            disabled={sessions.isFetching}
            aria-label="Refresh sessions"
          >
            <RefreshCw className={`h-4 w-4 ${sessions.isFetching ? 'animate-spin' : ''}`} aria-hidden />
            Refresh
          </Button>
        </div>
      </CardHeader>
      <CardContent>
        {sessions.isError ? (
          <ErrorState
            description="Could not load orchestration sessions."
            onRetry={() => sessions.refetch()}
          />
        ) : sessions.isLoading ? (
          <LoadingState label="Loading sessions…" />
        ) : !sessions.data || sessions.data.length === 0 ? (
          <EmptyState title="No active sessions" description="Orchestration sessions will appear here when the runtime runs." />
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Session</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Preset</TableHead>
                <TableHead>Creator</TableHead>
                <TableHead>Current task</TableHead>
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
