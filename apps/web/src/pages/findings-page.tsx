import { useMemo, useState } from 'react';
import { RefreshCw } from 'lucide-react';

import { LoadMore } from '@/components/load-more';
import { StatusBadge, severityVariant } from '@/components/status-badge';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Label } from '@/components/ui/label';
import { Select } from '@/components/ui/select';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { flattenPages, useFindings, useWorks } from '@/api/queries';
import { humanizeStatus, shortId } from '@/lib/format';
import type { ListFindingsQuery } from '@42ch/nexus-contracts';

/**
 * Findings view (Control Room — READ) — web-ui.md §6.1 #5.
 *
 * Findings are scoped to a Work (`GET /v1/local/works/{work_id}/findings`,
 * F-P2). The view first asks the author to pick a Work, then lists its
 * findings with severity + status filtering and cursor pagination. Remediation
 * actions are deferred to V1.65+ (web-ui.md §8).
 */
export function FindingsPage() {
  const works = useWorks({ limit: 100 });
  const workOptions = useMemo(() => flattenPages(works.data), [works.data]);
  const [workId, setWorkId] = useState('');
  const [severity, setSeverity] = useState('');
  const [status, setStatus] = useState('');

  const query: ListFindingsQuery | undefined = useMemo(() => {
    const parts: ListFindingsQuery = {};
    if (severity.trim()) parts.severity = severity.trim();
    if (status.trim()) parts.status = status.trim();
    return Object.keys(parts).length > 0 ? parts : undefined;
  }, [severity, status]);

  const findings = useFindings(workId || undefined, query);
  const rows = useMemo(() => flattenPages(findings.data), [findings.data]);

  return (
    <Card className="shadow-card">
      <CardHeader>
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div>
            <CardTitle>Findings</CardTitle>
            <CardDescription>Findings raised against a Work, with severity filtering.</CardDescription>
          </div>
          <Button
            type="button"
            variant="tertiary"
            size="small"
            onClick={() => findings.refetch()}
            disabled={!workId || findings.isFetching}
            aria-label="Refresh findings"
          >
            <RefreshCw className={`h-4 w-4 ${findings.isFetching ? 'animate-spin' : ''}`} aria-hidden />
            Refresh
          </Button>
        </div>
      </CardHeader>
      <CardContent>
        <div className="mb-4 flex flex-wrap items-end gap-3">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="findings-work">Work</Label>
            <Select
              id="findings-work"
              value={workId}
              onChange={(e) => setWorkId(e.target.value)}
              disabled={works.isLoading}
            >
              <option value="">{works.isLoading ? 'Loading works…' : 'Select a Work'}</option>
              {workOptions.map((w) => (
                <option key={w.work_id} value={w.work_id}>
                  {w.title || shortId(w.work_id)}
                </option>
              ))}
            </Select>
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="findings-severity">Severity</Label>
            <input
              id="findings-severity"
              type="search"
              value={severity}
              onChange={(e) => setSeverity(e.target.value)}
              placeholder="e.g. critical"
              className="h-10 w-full max-w-[180px] rounded-control border border-gray-alpha-400 bg-background-100 px-3 text-copy-14 text-gray-1000 placeholder:text-gray-700"
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="findings-status">Status</Label>
            <input
              id="findings-status"
              type="search"
              value={status}
              onChange={(e) => setStatus(e.target.value)}
              placeholder="e.g. open"
              className="h-10 w-full max-w-[180px] rounded-control border border-gray-alpha-400 bg-background-100 px-3 text-copy-14 text-gray-1000 placeholder:text-gray-700"
            />
          </div>
        </div>

        {!workId ? (
          <EmptyState title="Select a Work" description="Pick a Work above to see its findings." />
        ) : findings.isError ? (
          <ErrorState description="Could not load findings for this Work." onRetry={() => findings.refetch()} />
        ) : findings.isLoading ? (
          <LoadingState label="Loading findings…" />
        ) : rows.length === 0 ? (
          <EmptyState title="No findings" description="No findings match these filters for this Work." />
        ) : (
          <>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Severity</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Title</TableHead>
                  <TableHead>Kind</TableHead>
                  <TableHead>Chapter</TableHead>
                  <TableHead>Finding</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {rows.map((f) => (
                  <TableRow key={f.finding_id}>
                    <TableCell><Badge variant={severityVariant(f.severity)}>{humanizeStatus(f.severity)}</Badge></TableCell>
                    <TableCell><StatusBadge status={f.status} /></TableCell>
                    <TableCell className="text-gray-1000">{f.title || '(untitled finding)'}</TableCell>
                    <TableCell className="text-gray-900">{humanizeStatus(f.kind)}</TableCell>
                    <TableCell className="tabular-nums text-gray-900">{f.chapter ?? '—'}</TableCell>
                    <TableCell><span className="text-copy-13-mono text-gray-700">{shortId(f.finding_id)}</span></TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
            <LoadMore
              isFetchingNextPage={findings.isFetchingNextPage}
              hasNextPage={findings.hasNextPage}
              fetchNextPage={() => findings.fetchNextPage()}
              label="Load more findings"
            />
          </>
        )}
      </CardContent>
    </Card>
  );
}
