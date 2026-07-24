import type { FindingDetailResponse } from '@42ch/nexus-contracts';

/**
 * CSV column order required by the V1.91 P1 contract:
 * id, title, status, kind, severity, target_executor, created_at, rule_suggestion.
 * `rule_suggestion` is truncated to ~200 characters.
 */
export const CSV_COLUMNS: { key: keyof FindingDetailResponse; label: string; truncate?: number }[] = [
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
export function csvField(value: unknown, truncate?: number): string {
  if (value === undefined || value === null) return '';
  let str =
    typeof value === 'string'
      ? value
      : typeof value === 'number' || typeof value === 'boolean' || typeof value === 'bigint'
        ? String(value)
        : '';
  if (truncate !== undefined && str.length > truncate) {
    str = `${str.slice(0, truncate)}…`;
  }
  if (str.includes(',') || str.includes('"') || str.includes('\n') || str.includes('\r')) {
    return `"${str.replace(/"/g, '""')}"`;
  }
  return str;
}

/** Build and trigger a download for the current filtered findings as CSV. */
export function downloadFindingsCsv(rows: FindingDetailResponse[], filename: string): void {
  const header = CSV_COLUMNS.map((c) => csvField(c.label)).join(',');
  const lines = rows.map((row) =>
    CSV_COLUMNS.map((c) => csvField(row[c.key], c.truncate)).join(','),
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
