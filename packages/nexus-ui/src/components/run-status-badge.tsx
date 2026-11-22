import { cn } from '../lib/cn';

import { Badge } from './badge';

/**
 * Mirrors the wire union on RunSummary/RunDetail; product label mapping
 * (Needs review / Applied / Discarded / Failed / Running) is caller-owned.
 */
export type RunStatus = 'running' | 'succeeded' | 'failed' | 'applied' | 'discarded';

export interface RunStatusBadgeProps {
  /** Wire lifecycle status; drives the semantic Badge variant. */
  status: RunStatus;
  /** Caller-owned display label (i18n product vocabulary). */
  label: string;
  className?: string;
}

/**
 * succeeded → warning (amber — author action needed: Accept/Discard),
 * failed → error, running → queued (teal, in-flight), discarded → neutral.
 * Variants are token-backed and theme-safe in both light and dark.
 */
const STATUS_VARIANT = {
  running: 'queued',
  succeeded: 'warning',
  failed: 'error',
  applied: 'running',
  discarded: 'neutral',
} as const satisfies Record<RunStatus, 'neutral' | 'running' | 'queued' | 'warning' | 'error' | 'preset'>;

/**
 * the label is caller-owned copy; the component only owns the status →
 * semantic-variant mapping.
 */
export function RunStatusBadge({ status, label, className }: RunStatusBadgeProps) {
  return (
    <Badge
      variant={STATUS_VARIANT[status]}
      className={cn(className)}
      data-testid="run-status-badge"
      data-status={status}
    >
      {label}
    </Badge>
  );
}
