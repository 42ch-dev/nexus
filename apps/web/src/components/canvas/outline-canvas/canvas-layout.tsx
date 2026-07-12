/**
 * Outline canvas — layout chrome (V1.73 B5 split, `R-V172P0-QC1-002`;
 * V1.108 P0 T3 — `CanvasHeader` with graph↔list toggle).
 *
 * Small presentational pieces used by the orchestrator header. Mirrors the
 * V1.71 `strategy-canvas/canvas-layout.tsx` module. Extracted from the
 * original `outline-canvas.tsx` monolith. T3 adds the `CanvasHeader` with
 * the alt-view toggle (FB-C1-004).
 */
import { useTranslation } from 'react-i18next';
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
  const { t } = useTranslation('canvas');
  const color =
    status === 'conflict'
      ? 'border-canvas-write-conflict text-canvas-write-conflict bg-canvas-write-conflict/10'
      : status === 'dirty'
        ? 'border-canvas-write-dirty text-canvas-write-dirty bg-canvas-write-dirty/10'
        : 'border-gray-alpha-400 text-gray-700 bg-background-100';
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-pill border px-2 py-0.5 text-label-12 ${color}`}
      title={status === 'conflict' ? t('outlineCanvas.revision.conflictTooltip') : undefined}
    >
      {status === 'conflict' ? <AlertTriangle className="h-3 w-3" aria-hidden /> : null}
      {t('outlineCanvas.revision.label', { revision })}
    </span>
  );
}

/**
 * Outline canvas toolbar header — Work title, revision badge, and the
 * graph↔list alt-view toggle (FB-C1-004).
 *
 * Mirrors `strategy-canvas/canvas-layout.tsx` `CanvasHeader`: the toggle
 * uses `aria-pressed` so keyboard/screen-reader users know which view is
 * active. Locked copy: **Show list view** (from graph) / **Show graph**
 * (from list) — matches Strategy/World KB for cross-canvas consistency.
 */
export function CanvasHeader({
  title,
  subtitle,
  revision,
  status,
  showAlt,
  setShowAlt,
}: {
  title: string;
  subtitle: string;
  revision: number;
  status: RevisionStatus;
  showAlt: boolean;
  setShowAlt: (v: boolean) => void;
}) {
  const { t } = useTranslation('canvas');
  return (
    <div className="flex flex-wrap items-center justify-between gap-2">
      <div>
        <h1 className="text-heading-24 font-heading text-gray-1000">{title}</h1>
        <p className="text-copy-14 text-gray-900">{subtitle}</p>
      </div>
      <div className="flex items-center gap-2">
        <RevisionBadge revision={revision} status={status} />
        <button
          type="button"
          onClick={() => setShowAlt(!showAlt)}
          aria-pressed={showAlt}
          className="rounded-control border border-gray-alpha-400 px-3 py-1.5 text-button-12 text-gray-900 hover:bg-gray-alpha-100"
        >
          {showAlt ? t('outlineCanvas.showGraph') : t('outlineCanvas.showListView')}
        </button>
      </div>
    </div>
  );
}
