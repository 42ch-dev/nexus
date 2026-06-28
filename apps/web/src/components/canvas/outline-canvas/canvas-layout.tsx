/**
 * Outline canvas — layout chrome (V1.73 B5 split, `R-V172P0-QC1-002`).
 *
 * Small presentational pieces used by the orchestrator header. Mirrors the
 * V1.71 `strategy-canvas/canvas-layout.tsx` module. Extracted from the
 * original `outline-canvas.tsx` monolith; behavior is unchanged.
 */
import { AlertTriangle } from 'lucide-react';

type RevisionStatus = 'clean' | 'dirty' | 'conflict';

/** Revision + write-state badge rendered next to the Work title. */
export function RevisionBadge({
  revision,
  status,
}: {
  revision: number;
  status: RevisionStatus;
}) {
  const color =
    status === 'conflict'
      ? 'border-canvas-write-conflict text-canvas-write-conflict bg-canvas-write-conflict/10'
      : status === 'dirty'
        ? 'border-canvas-write-dirty text-canvas-write-dirty bg-canvas-write-dirty/10'
        : 'border-gray-alpha-400 text-gray-700 bg-background-100';
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-pill border px-2 py-0.5 text-label-12 ${color}`}
      title={status === 'conflict' ? 'Revision conflict — refetch before editing' : undefined}
    >
      {status === 'conflict' ? <AlertTriangle className="h-3 w-3" aria-hidden /> : null}
      rev {revision}
    </span>
  );
}
