import { Cpu } from 'lucide-react';

import { cn } from '../lib/cn';

/**
 * Compute result node body chrome (V1.147 P2) — pure presentational extract
 * for a Narrative-layer Timeline node produced by an accepted compute Run
 * (behavior spec §5 Timeline citizenship).
 *
 * Pair with the app's canvas node shell (`NodeChromeShell` in
 * `apps/web/src/components/canvas/presentational/node-chrome-shell.tsx`) for
 * the outer card + surface spine + selection ring — exactly like
 * `TimelineEventChrome` for KB event nodes. The RF wrapper
 * (`TimelineComputeResultNode`) stays App-local in Task 3 (handles, selected/
 * dragging, i18n resolution) per the V1.124 extract pattern.
 *
 * Same-family rule (spec §5): compute nodes share the Narrative visual
 * language — `canvas-layer-narrative-accent` iconography and badge chrome —
 * so "the world reacted via compute" reads as one family with manual events.
 * Distinguishability comes from the Cpu icon, the "Compute result" kind pill,
 * and the provenance chip (module · version · run id) — not from a different
 * accent color.
 *
 * Presentational boundary: no `@xyflow/react`, no wire-contract package, no
 * i18n hook, no router. Props are resolved strings (and optional run id)
 * only — App RF wrappers resolve `t()` before calling; Studio fixtures pass
 * literal English product vocabulary.
 */
export interface ComputeResultNodeChromeProps {
  /**
   * Resolved node title. Per spec §5: the module event summary when present
   * (e.g. the damage line), else the module name. Caller-owned fallback.
   */
  title: string;
  /**
   * Kind pill label — "Compute result" (caller-owned; i18n lives in the app).
   * Distinguishes compute nodes from manual Event nodes at a glance.
   */
  kindLabel: string;
  /**
   * Provenance chip label — "From module run" (direct lane) vs "From preset"
   * (preset path), caller-resolved when distinguishable (spec §5).
   */
  provenanceLabel: string;
  /** Module display name (product vocabulary — never protocol jargon). */
  moduleName: string;
  /** Module version string, e.g. "1.0.0" (rendered as `v{moduleVersion}`). */
  moduleVersion: string;
  /**
   * Optional short run correlation id. Rendered as a mono suffix on the meta
   * line. Absent for the preset path (no direct Run row exists).
   */
  runId?: string;
  className?: string;
}

/**
 * ComputeResultNodeChrome — body content for a Narrative-layer Compute result
 * node. Icon + title row, kind + provenance badge row, module/version + run id
 * meta line. Pure presentational; compose with the app node shell.
 */
export function ComputeResultNodeChrome({
  title,
  kindLabel,
  provenanceLabel,
  moduleName,
  moduleVersion,
  runId,
  className,
}: ComputeResultNodeChromeProps) {
  return (
    <div className={cn('flex flex-col gap-1', className)} data-testid="compute-result-node-chrome">
      <div className="flex items-center gap-2">
        <Cpu
          className="h-4 w-4 flex-shrink-0 text-canvas-layer-narrative-accent"
          aria-hidden
        />
        <span
          className="truncate font-heading text-copy-14 font-semibold text-gray-1000"
          title={title}
        >
          {title}
        </span>
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-1">
        <span
          data-testid="compute-node-kind-pill"
          className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700"
        >
          {kindLabel}
        </span>
        <span
          data-testid="compute-node-provenance-chip"
          className="rounded-pill border border-canvas-layer-narrative-accent/30 bg-canvas-layer-narrative-accent/15 px-1.5 py-0.5 text-label-12 text-canvas-layer-narrative-accent"
        >
          {provenanceLabel}
        </span>
      </div>
      <p data-testid="compute-node-meta" className="mt-1 text-label-12 text-gray-700">
        {moduleName} · v{moduleVersion}
        {runId ? (
          <span data-testid="compute-node-run-id" className="ml-1 font-mono text-gray-700">
            {runId}
          </span>
        ) : null}
      </p>
    </div>
  );
}
