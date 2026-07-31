import { cn } from '../lib/cn';

import { Button } from './button';
import { RunStatusBadge, type RunStatus } from './run-status-badge';

/**
 * One row in the Runs history table. Caller maps RunSummary wire rows into
 * this shape (preformatted local-time strings, resolved module/world
 * titles, i18n status label) — the table never formats or fetches.
 */
export interface RunTableRow {
  runId: string;
  moduleName: string;
  moduleVersion: string;
  worldTitle: string;
  status: RunStatus;
  /** Caller-owned product label (Needs review / Applied / Discarded / Failed / Running). */
  statusLabel: string;
  /** Preformatted local-time strings. */
  startedAt: string;
  finishedAt?: string;
}

/** Caller-owned copy for the Runs table (i18n lives in the app). */
export interface RunsTableCopy {
  moduleColumn: string;
  worldColumn: string;
  statusColumn: string;
  startedColumn: string;
  finishedColumn: string;
  runIdColumn: string;
  /** Row action opening the Run result inspector. */
  openRunLabel: string;
  /** aria-label for the copy-correlation-id affordance. */
  copyIdLabel: string;
  emptyTitle: string;
  emptyDescription: string;
}

export interface RunsTableProps {
  /** Newest-first rows. Empty renders the empty-state block. */
  rows: RunTableRow[];
  copy: RunsTableCopy;
  /** Row action — opens the Run inspector (re-review or read-only). */
  onOpenRun?: (runId: string) => void;
  className?: string;
}

/**
 * RunsTable — Runs history list chrome for the Compute Run Studio
 * (V1.147 P1, behavior spec §4). Columns: Module (name + version), World,
 * Status badge, Started / Finished, short correlation id (monospace,
 * copyable). Pure presentational: rows are pre-mapped, copy caller-owned;
 * filter chrome lives in the caller (app) above this table.
 */
export function RunsTable({ rows, copy, onOpenRun, className }: RunsTableProps) {
  if (rows.length === 0) {
    return (
      <div
        data-testid="runs-table-empty"
        className={cn(
          'rounded-card border border-dashed border-gray-alpha-400 bg-background-200 p-6',
          className,
        )}
      >
        <p className="text-label-14 font-medium text-gray-1000">{copy.emptyTitle}</p>
        <p className="mt-1 text-copy-13 text-gray-700">{copy.emptyDescription}</p>
      </div>
    );
  }

  return (
    <div
      className={cn(
        'overflow-x-auto rounded-card border border-gray-alpha-300 bg-background-100',
        className,
      )}
      data-testid="runs-table"
    >
      <table className="w-full border-collapse text-left">
        <thead>
          <tr className="border-b border-gray-alpha-400 bg-background-200">
            <th scope="col" className="px-4 py-2 text-label-12 font-medium text-gray-700">
              {copy.moduleColumn}
            </th>
            <th scope="col" className="px-4 py-2 text-label-12 font-medium text-gray-700">
              {copy.worldColumn}
            </th>
            <th scope="col" className="px-4 py-2 text-label-12 font-medium text-gray-700">
              {copy.statusColumn}
            </th>
            <th scope="col" className="px-4 py-2 text-label-12 font-medium text-gray-700">
              {copy.startedColumn}
            </th>
            <th scope="col" className="px-4 py-2 text-label-12 font-medium text-gray-700">
              {copy.finishedColumn}
            </th>
            <th scope="col" className="px-4 py-2 text-label-12 font-medium text-gray-700">
              {copy.runIdColumn}
            </th>
            {onOpenRun && <th scope="col" className="px-4 py-2" aria-label={copy.openRunLabel} />}
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr
              key={row.runId}
              className="border-b border-gray-alpha-200 last:border-b-0 hover:bg-background-200"
              data-testid={`runs-table-row-${row.runId}`}
            >
              <td className="px-4 py-2">
                <span className="text-copy-14 text-gray-1000">{row.moduleName}</span>{' '}
                <span className="text-copy-13 text-gray-700">v{row.moduleVersion}</span>
              </td>
              <td className="px-4 py-2 text-copy-14 text-gray-1000">{row.worldTitle}</td>
              <td className="px-4 py-2">
                <RunStatusBadge status={row.status} label={row.statusLabel} />
              </td>
              <td className="px-4 py-2 text-copy-13 text-gray-700">{row.startedAt}</td>
              <td className="px-4 py-2 text-copy-13 text-gray-700">{row.finishedAt ?? '—'}</td>
              <td className="px-4 py-2">
                <span className="inline-flex items-center gap-1">
                  <span className="text-copy-13-mono text-gray-700" title={row.runId}>
                    {row.runId}
                  </span>
                  <button
                    type="button"
                    aria-label={copy.copyIdLabel}
                    className="text-label-12 text-gray-700 underline decoration-gray-alpha-500 underline-offset-2 hover:text-gray-1000"
                    onClick={() => {
                      // Clipboard is a fire-and-forget affordance; failures
                      // leave the visible mono id as the fallback.
                      void navigator.clipboard?.writeText(row.runId).catch(() => {});
                    }}
                    data-testid={`runs-table-copy-${row.runId}`}
                  >
                    {copy.copyIdLabel}
                  </button>
                </span>
              </td>
              {onOpenRun && (
                <td className="px-4 py-2">
                  <Button
                    variant="tertiary"
                    size="small"
                    onClick={() => onOpenRun(row.runId)}
                    data-testid={`runs-table-open-${row.runId}`}
                  >
                    {copy.openRunLabel}
                  </Button>
                </td>
              )}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
