import { useMemo, useState } from 'react';
import { RefreshCw } from 'lucide-react';

import { LoadMore } from '@/components/load-more';
import { FindingDetailPanel } from '@/components/findings/finding-detail-panel';
import { FindingStatusBadge, severityVariant } from '@/components/status-badge';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Label } from '@/components/ui/label';
import { Select } from '@/components/ui/select';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { flattenPages, useFindings, useUpdateFinding, useWorks } from '@/api/queries';
import { humanizeStatus, shortId } from '@/lib/format';
import type { FindingDetailResponse, ListFindingsQuery } from '@42ch/nexus-contracts';

/**
 * Findings view (Control Room) — V1.77 remediation surface (web-ui.md §23).
 *
 * Findings are scoped to a Work (`GET /v1/local/works/{work_id}/findings`).
 * The author picks a Work, then sees its findings as a table with row-level
 * status/severity badges. Selecting a row opens the detail/inspector panel
 * (`FindingDetailPanel`) with the three remediation affordances: status
 * transitions (6-state, invalid disabled), `target_executor` assignment, and
 * inline edit. All three persist via `PATCH .../findings/{id}` with optimistic
 * TanStack Query mutations (`useUpdateFinding`); the list refreshes on settle.
 *
 * Layout (D4 LOCKED): detail-panel + row-action hybrid — the page stays a
 * Control-Room table, not a canvas graph.
 */
export function FindingsPage() {
  const works = useWorks({ limit: 100 });
  const workOptions = useMemo(() => flattenPages(works.data), [works.data]);
  const [workId, setWorkId] = useState('');
  const [severity, setSeverity] = useState('');
  const [status, setStatus] = useState('');
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const query: ListFindingsQuery | undefined = useMemo(() => {
    const parts: ListFindingsQuery = {};
    if (severity.trim()) parts.severity = severity.trim();
    if (status.trim()) parts.status = status.trim();
    return Object.keys(parts).length > 0 ? parts : undefined;
  }, [severity, status]);

  const findings = useFindings(workId || undefined, query);
  const rows = useMemo(() => flattenPages(findings.data), [findings.data]);
  const updateFinding = useUpdateFinding();

  // The selected finding comes from the list cache (optimistically updated by
  // useUpdateFinding), so the inspector reflects in-flight mutations without a
  // separate detail fetch. Falls back to null if the row paginated out.
  const selected: FindingDetailResponse | null = useMemo(
    () => rows.find((f) => f.finding_id === selectedId) ?? null,
    [rows, selectedId],
  );

  const quickAssign = (findingId: string, target_executor: string) => {
    if (!workId) return;
    updateFinding.mutate({ workId, findingId, patch: { target_executor } });
  };

  return (
    <Card className="shadow-card">
      <CardHeader>
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div>
            <CardTitle>Findings</CardTitle>
            <CardDescription>
              Triage findings — advance status, assign routing, or edit details inline.
            </CardDescription>
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
              onChange={(e) => {
                setWorkId(e.target.value);
                setSelectedId(null);
              }}
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
          <div className="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_360px]">
            <div className="min-w-0">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Severity</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead>Title</TableHead>
                    <TableHead>Kind</TableHead>
                    <TableHead>Chapter</TableHead>
                    <TableHead>Assign To</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {rows.map((f) => {
                    const isActive = f.finding_id === selectedId;
                    return (
                      <TableRow
                        key={f.finding_id}
                        onClick={() => setSelectedId(isActive ? null : f.finding_id)}
                        className={`cursor-pointer ${isActive ? 'bg-background-300' : ''}`}
                      >
                        <TableCell>
                          <Badge variant={severityVariant(f.severity)}>{humanizeStatus(f.severity)}</Badge>
                        </TableCell>
                        <TableCell>
                          <FindingStatusBadge status={f.status} />
                        </TableCell>
                        <TableCell className="text-gray-1000">{f.title || '(untitled finding)'}</TableCell>
                        <TableCell className="text-gray-900">{humanizeStatus(f.kind)}</TableCell>
                        <TableCell className="tabular-nums text-gray-900">{f.chapter ?? '—'}</TableCell>
                        <TableCell onClick={(e) => e.stopPropagation()}>
                          <Select
                            aria-label={`Assign target executor for finding ${shortId(f.finding_id)}`}
                            value={f.target_executor}
                            onChange={(e) => quickAssign(f.finding_id, e.target.value)}
                            disabled={updateFinding.isPending}
                            className="h-8 w-[130px] text-copy-13"
                          >
                            <option value="none">None</option>
                            <option value="write">Write</option>
                            <option value="brainstorm">Brainstorm</option>
                            <option value="master">Master</option>
                          </Select>
                        </TableCell>
                      </TableRow>
                    );
                  })}
                </TableBody>
              </Table>
              <LoadMore
                isFetchingNextPage={findings.isFetchingNextPage}
                hasNextPage={findings.hasNextPage}
                fetchNextPage={() => findings.fetchNextPage()}
                label="Load more findings"
              />
            </div>

            <aside className="lg:sticky lg:top-4 lg:self-start">
              {selected ? (
                <Card className="shadow-card">
                  <CardHeader>
                    <CardTitle className="text-heading-16">Finding Details</CardTitle>
                    <CardDescription className="text-copy-13-mono">
                      {shortId(selected.finding_id)}
                    </CardDescription>
                  </CardHeader>
                  <CardContent>
                    <FindingDetailPanel workId={workId} finding={selected} />
                  </CardContent>
                </Card>
              ) : (
                <EmptyState
                  title="No finding selected"
                  description="Select a row to triage status, assign routing, or edit details."
                />
              )}
            </aside>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
