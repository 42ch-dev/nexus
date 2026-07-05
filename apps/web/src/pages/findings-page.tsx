import { useMemo, useState } from 'react';
import { Download, RefreshCw } from 'lucide-react';

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
import {
  flattenPages,
  useBatchUpdateFindings,
  useFindings,
  useUpdateFinding,
  useWorks,
} from '@/api/queries';
import { humanizeStatus, shortId } from '@/lib/format';
import { FINDING_STATUSES } from '@/lib/findings-lifecycle';
import type { FindingDetailResponse, ListFindingsQuery } from '@42ch/nexus-contracts';

/**
 * Findings view (Control Room) — V1.77 remediation surface + V1.91 batch triage.
 *
 * Findings are scoped to a Work (`GET /v1/daemon/works/{work_id}/findings`).
 * The author picks a Work, then sees its findings as a table with row-level
 * status/severity badges. Selecting a row opens the detail/inspector panel
 * (`FindingDetailPanel`) with the three remediation affordances: status
 * transitions (6-state, invalid disabled), `target_executor` assignment, and
 * inline edit. All three persist via `PATCH .../findings/{id}` with optimistic
 * TanStack Query mutations (`useUpdateFinding`); the list refreshes on settle.
 *
 * V1.91 P1 adds multi-select checkboxes, a bulk action bar (status + executor),
 * and client-side CSV export of the currently loaded/filtered rows.
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
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());

  const query: ListFindingsQuery | undefined = useMemo(() => {
    const parts: ListFindingsQuery = {};
    if (severity.trim()) parts.severity = severity.trim();
    if (status.trim()) parts.status = status.trim();
    return Object.keys(parts).length > 0 ? parts : undefined;
  }, [severity, status]);

  const findings = useFindings(workId || undefined, query);
  const rows = useMemo(() => flattenPages(findings.data), [findings.data]);
  const updateFinding = useUpdateFinding();
  const batchUpdate = useBatchUpdateFindings();

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

  const allSelected = rows.length > 0 && rows.every((f) => selectedIds.has(f.finding_id));
  const someSelected = rows.some((f) => selectedIds.has(f.finding_id));

  const toggleRow = (findingId: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(findingId)) {
        next.delete(findingId);
      } else {
        next.add(findingId);
      }
      return next;
    });
  };

  const toggleAll = () => {
    if (allSelected) {
      setSelectedIds((prev) => {
        const next = new Set(prev);
        for (const f of rows) {
          next.delete(f.finding_id);
        }
        return next;
      });
    } else {
      setSelectedIds((prev) => {
        const next = new Set(prev);
        for (const f of rows) {
          next.add(f.finding_id);
        }
        return next;
      });
    }
  };

  const clearSelection = () => setSelectedIds(new Set());

  const runBatchStatus = (statusValue: string) => {
    if (!workId || selectedIds.size === 0 || !statusValue) return;
    batchUpdate.mutate(
      {
        workId,
        request: {
          finding_ids: Array.from(selectedIds),
          patch: { status: statusValue },
        },
      },
      { onSuccess: clearSelection },
    );
  };

  const runBatchExecutor = (targetExecutor: string) => {
    if (!workId || selectedIds.size === 0 || !targetExecutor) return;
    batchUpdate.mutate(
      {
        workId,
        request: {
          finding_ids: Array.from(selectedIds),
          patch: { target_executor: targetExecutor },
        },
      },
      { onSuccess: clearSelection },
    );
  };

  const exportCsv = () => {
    if (rows.length === 0) return;
    downloadFindingsCsv(rows, `findings-${workId || 'all'}-${Date.now()}.csv`);
  };

  const isBusy = updateFinding.isPending || batchUpdate.isPending;

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
                setSelectedIds(new Set());
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
              onChange={(e) => {
                setSeverity(e.target.value);
                setSelectedIds(new Set());
              }}
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
              onChange={(e) => {
                setStatus(e.target.value);
                setSelectedIds(new Set());
              }}
              placeholder="e.g. open"
              className="h-10 w-full max-w-[180px] rounded-control border border-gray-alpha-400 bg-background-100 px-3 text-copy-14 text-gray-1000 placeholder:text-gray-700"
            />
          </div>
        </div>

        {selectedIds.size > 0 && (
          <div
            className="mb-4 flex flex-wrap items-center gap-3 rounded-control border border-blue-700 bg-blue-50 p-3"
            data-testid="findings-bulk-bar"
          >
            <span className="text-copy-14 font-medium text-gray-1000">
              {selectedIds.size} selected
            </span>
            <div className="flex items-center gap-2">
              <Select
                aria-label="Set status for selected findings"
                value=""
                onChange={(e) => runBatchStatus(e.target.value)}
                disabled={isBusy}
                className="h-8 w-[150px] text-copy-13"
              >
                <option value="">Set status…</option>
                {FINDING_STATUSES.map((s) => (
                  <option key={s} value={s}>
                    {humanizeStatus(s)}
                  </option>
                ))}
              </Select>
              <Select
                aria-label="Assign target executor for selected findings"
                value=""
                onChange={(e) => runBatchExecutor(e.target.value)}
                disabled={isBusy}
                className="h-8 w-[150px] text-copy-13"
              >
                <option value="">Assign to…</option>
                <option value="none">None</option>
                <option value="write">Write</option>
                <option value="brainstorm">Brainstorm</option>
                <option value="master">Master</option>
              </Select>
              <Button
                type="button"
                variant="tertiary"
                size="small"
                onClick={clearSelection}
                disabled={isBusy}
              >
                Clear
              </Button>
            </div>
          </div>
        )}

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
              <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
                <span className="text-copy-13 text-gray-700">{rows.length} finding(s)</span>
                <Button
                  type="button"
                  variant="tertiary"
                  size="small"
                  onClick={exportCsv}
                  disabled={rows.length === 0}
                  aria-label="Export findings to CSV"
                >
                  <Download className="mr-1.5 h-4 w-4" aria-hidden />
                  Export CSV
                </Button>
              </div>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead className="w-10">
                      <input
                        type="checkbox"
                        aria-label="Select all visible findings"
                        checked={allSelected}
                        ref={(el) => {
                          if (el) el.indeterminate = someSelected && !allSelected;
                        }}
                        onChange={toggleAll}
                        disabled={isBusy}
                      />
                    </TableHead>
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
                    const isSelected = selectedIds.has(f.finding_id);
                    return (
                      <TableRow
                        key={f.finding_id}
                        onClick={() => setSelectedId(isActive ? null : f.finding_id)}
                        className={`cursor-pointer ${isActive ? 'bg-background-300' : ''}`}
                      >
                        <TableCell onClick={(e) => e.stopPropagation()}>
                          <input
                            type="checkbox"
                            aria-label={`Select finding ${shortId(f.finding_id)}`}
                            checked={isSelected}
                            onChange={() => toggleRow(f.finding_id)}
                            disabled={isBusy}
                          />
                        </TableCell>
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
                            disabled={isBusy}
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

/**
 * CSV column order required by the V1.91 P1 contract:
 * id, title, status, kind, severity, target_executor, created_at, rule_suggestion.
 * `rule_suggestion` is truncated to ~200 characters.
 */
const CSV_COLUMNS: { key: keyof FindingDetailResponse; label: string; truncate?: number }[] = [
  { key: 'finding_id', label: 'id' },
  { key: 'title', label: 'title' },
  { key: 'status', label: 'status' },
  { key: 'kind', label: 'kind' },
  { key: 'severity', label: 'severity' },
  { key: 'target_executor', label: 'target_executor' },
  { key: 'created_at', label: 'created_at' },
  { key: 'rule_suggestion', label: 'rule_suggestion', truncate: 200 },
];

/** Escape a single CSV field per RFC 4180. */
function csvField(value: unknown, truncate?: number): string {
  if (value === undefined || value === null) return '';
  let str = String(value);
  if (truncate !== undefined && str.length > truncate) {
    str = `${str.slice(0, truncate)}…`;
  }
  if (str.includes(',') || str.includes('"') || str.includes('\n') || str.includes('\r')) {
    return `"${str.replace(/"/g, '""')}"`;
  }
  return str;
}

/** Build and trigger a download for the current filtered findings as CSV. */
function downloadFindingsCsv(rows: FindingDetailResponse[], filename: string) {
  const header = CSV_COLUMNS.map((c) => csvField(c.label)).join(',');
  const lines = rows.map((row) =>
    CSV_COLUMNS
      .map((c) => csvField(row[c.key], c.truncate))
      .join(','),
  );
  const csv = [header, ...lines].join('\n');
  const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  document.body.removeChild(anchor);
  URL.revokeObjectURL(url);
}
