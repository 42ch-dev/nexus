/**
 * Timeline compute inspector — V1.147 P2 T3 (app wiring of the promoted
 * `ComputeInspectorSections` primitive for a Narrative Compute result node).
 *
 * Renders the compute context the behavior spec §5 Inspector row locks:
 * module name + version, provenance chip ("From module run" / "From preset"),
 * report digest, affected KnowledgeEntries, Run id + Open Run affordance.
 * All copy is caller-owned (`canvas` namespace); data + callbacks are
 * app-owned; the primitive stays pure presentational.
 *
 * Params digest: the log event itself does not carry invocation params (the
 * daemon stamps provenance only), so the digest is derived from the Run
 * detail (`invocation_params`) via the shared `useComputeRun` query — cached
 * with the Runs table, so reopening a run from history is a cache hit. The
 * Parameters section renders only when a digest can be built (honest sparse
 * events).
 *
 * Open Run: hands the FULL run id + module id back to the orchestrator via
 * `ctxRef.current.onOpenRun`, which navigates to Settings → Modules run
 * detail (deep link). The Run section displays the short correlation id
 * (spec §4 "Short correlation id; monospace") while the callback carries the
 * full id.
 *
 * Read-only when the orchestrator supplies no `onOpenRun` (test mounts) —
 * the Open Run button hides (the primitive's contract).
 */
import { useMemo, type MutableRefObject } from 'react';
import { useTranslation } from 'react-i18next';
import type { Node } from '@xyflow/react';

import {
  ComputeInspectorSections,
  type ComputeInspectorSectionsCopy,
} from '@42ch/nexus-ui';

import { useComputeRun } from '@/api/queries';
import { shortId } from '@/lib/format';
import type {
  TimelineCanvasAdapterContext,
  TimelineNodeData,
} from './timeline-canvas-adapter';

/** Build a "key: value · key: value" digest from run invocation params. */
export function buildParamsDigest(params: Record<string, unknown> | undefined): string | undefined {
  if (!params) return undefined;
  const parts: string[] = [];
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === null || value === '') continue;
    const rendered =
      typeof value === 'string' ? value : JSON.stringify(value);
    parts.push(`${key}: ${rendered}`);
  }
  return parts.length > 0 ? parts.join(' · ') : undefined;
}

export interface TimelineComputeInspectorProps {
  node: Node<TimelineNodeData>;
  ctxRef: MutableRefObject<TimelineCanvasAdapterContext>;
}

export function TimelineComputeInspector({
  node,
  ctxRef,
}: TimelineComputeInspectorProps) {
  const { t } = useTranslation('canvas');
  const ctx = ctxRef.current;
  const payload = node.data.compute;

  // Params digest from the shared run-detail cache (see module doc). The
  // hook must run unconditionally (hooks order); `useComputeRun` is
  // disabled for run-less (preset-path) nodes.
  const runDetail = useComputeRun(payload?.runId);
  const paramsDigest = useMemo(
    () => buildParamsDigest(runDetail.data?.invocation_params),
    [runDetail.data],
  );

  // No payload → nothing honest to render (defensive; the adapter only
  // creates compute nodes with a payload).
  if (!payload) return null;

  const provenanceLabel =
    payload.sourceKind === 'preset'
      ? t('timeline.computeNode.provenance.preset')
      : t('timeline.computeNode.provenance.direct');

  const copy: ComputeInspectorSectionsCopy = {
    moduleTitle: t('timeline.computeInspector.moduleTitle'),
    reportTitle: t('timeline.computeInspector.reportTitle'),
    affectedTitle: t('timeline.computeInspector.affectedTitle'),
    runTitle: t('timeline.computeInspector.runTitle'),
    paramsTitle: t('timeline.computeInspector.paramsTitle'),
    openRunLabel: t('timeline.computeInspector.openRun'),
  };

  return (
    <div
      data-testid="timeline-compute-inspector"
      aria-label={t('timeline.computeInspector.aria')}
    >
      <ComputeInspectorSections
        moduleName={payload.moduleId}
        moduleVersion={payload.moduleVersion}
        reportDigest={payload.reportDigest}
        affectedEntries={payload.affectedEntries}
        runId={payload.runId ? shortId(payload.runId) : undefined}
        paramsDigest={paramsDigest}
        provenanceLabel={provenanceLabel}
        copy={copy}
        onOpenRun={
          ctx.onOpenRun && payload.runId
            ? () => ctx.onOpenRun?.(payload.runId!, payload.moduleId)
            : undefined
        }
      />
      {/* V1.162 P2 T1 — fork-point affordance (World Timeline only). The
          picked compute event is the fork point; the orchestrator opens the
          fork-create dialog (PD-6 landing). Hidden when no wiring (read-only
          test mounts). Copy follows the PD-5 lazy-branch model. */}
      {ctx.onCreateFork ? (
        <button
          type="button"
          onClick={() => ctx.onCreateFork?.(payload.eventId)}
          data-testid="compute-inspector-fork-here"
          className="mt-3 w-full rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-2 text-copy-13 text-gray-900 hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700"
        >
          {t('timeline.computeInspector.forkHere')}
        </button>
      ) : null}
    </div>
  );
}
