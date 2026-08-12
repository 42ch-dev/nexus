/**
 * Timeline canvas node types — V1.122 P1 T2 + V1.123 P1 T2 + V1.124 P0 T2 +
 * V1.147 P2 T3.
 *
 * Four node kinds, all projecting onto the Timeline surface:
 *   • timeline-brief-era  — V1.123 P1 T2 Brief-era marker
 *                           (`block_type=era` projected onto the Brief
 *                           when-axis). Body chrome: `TimelineBriefEraChrome`.
 *   • timeline-event      — V1.122 `block_type=event` projected onto the
 *                           Narrative when-axis. Body chrome: `TimelineEventChrome`.
 *   • timeline-key-block  — V1.122 non-event, non-era KeyBlock entity in the
 *                           Narrative Context cluster. Body chrome:
 *                           `TimelineKeyBlockChrome`.
 *   • timeline-compute-result — V1.147 P2 T3 machine-written
 *                           `event_type=compute_result` log events merged
 *                           into the Narrative when-axis (accepted compute
 *                           Runs). Body chrome: `ComputeResultNodeChrome`
 *                           (promoted primitive, @42ch/nexus-ui). Compute
 *                           nodes are NOT KB entities — the KB inspector +
 *                           `kb.patch_entity` write path never sees them.
 *
 * V1.124 P0 T2 — RF wrappers are thin App-local shells:
 *   `NodeChromeShell` + `Handle`s + presentational body extract + RF
 *   `selected`/`dragging`. Body chrome lives in
 *   `../presentational/timeline-node-chrome` (Studio-reachable as
 *   `@web-canvas/timeline-node-chrome`). i18n + `BLOCK_TYPE_LABELS` stay here.
 *
 * Chrome tokens: reuses V1.121 `canvas-node-fill` / `canvas-node-border`
 * via `NodeChromeShell`. Surface spine stays `accent="worldkb"`. Layer
 * accents (brief / narrative) live on the body extract badges/icons.
 *
 * No `TimelineForkMarkerNode` exists. Fork chrome is branch-level and
 * marker-derived (V1.162 P2 T2): the `ForkLineageBadge` reads the active
 * branch's canon `fork_created` marker (spec §6.6.3 carrier B) — NEVER the
 * world-level WorldState fork fields.
 */
import { memo } from 'react';
import { useTranslation } from 'react-i18next';
import { Handle, Position, type NodeProps } from '@xyflow/react';

import { ComputeResultNodeChrome } from '@42ch/nexus-ui';

import { shortId } from '@/lib/format';
import { NodeChromeShell } from '../presentational/node-chrome-shell';
import {
  TimelineBriefEraChrome,
  TimelineEventChrome,
  TimelineKeyBlockChrome,
} from '../presentational/timeline-node-chrome';
import { BLOCK_TYPE_LABELS } from '../world-kb/types';
import { DirectedAxisSpine } from './directed-axis-spine';
import type { TimelineNodeData } from './timeline-canvas-adapter';

function anchorCountOf(d: TimelineNodeData): number {
  return typeof d.source_anchor_count === 'number' ? d.source_anchor_count : 0;
}

/**
 * V1.123 P1 T2 — Brief-era node. Compact era marker card on the Brief
 * when-axis. Body chrome extracted to `TimelineBriefEraChrome` (V1.124).
 */
export const TimelineBriefEraNode = memo(function TimelineBriefEraNode({
  data,
  selected,
  dragging,
}: NodeProps) {
  const d = data as TimelineNodeData;
  const { t } = useTranslation('canvas');
  const anchorCount = anchorCountOf(d);

  // Time-span label: prefer `start_hint → end_hint`; fall back to whichever
  // hint exists; fall back to the temporal-unknown pill when neither is set.
  const span = (() => {
    if (d.startHint && d.endHint) {
      return t('timeline.briefEraNode.span', {
        start: d.startHint,
        end: d.endHint,
      });
    }
    if (d.startHint) return d.startHint;
    if (d.endHint) return d.endHint;
    return null;
  })();

  return (
    <NodeChromeShell
      selected={selected}
      dragging={dragging}
      accent="worldkb"
      aria-label={t('timeline.briefEraNode.aria', { name: d.canonical_name })}
    >
      <Handle
        type="target"
        position={Position.Left}
        className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port"
      />
      <TimelineBriefEraChrome
        title={d.canonical_name || t('timeline.briefEraNode.unnamed')}
        blockTypeLabel={BLOCK_TYPE_LABELS[d.block_type]}
        timeSpan={span}
        temporalUnknownLabel={t('timeline.briefEraNode.temporalUnknown')}
        eraId={d.eraId}
        worldSummary={d.worldSummary}
        sourceAnchorLabel={t('timeline.briefEraNode.sourceAnchorCount', {
          count: anchorCount,
        })}
        version={d.version}
      />
      <Handle
        type="source"
        position={Position.Right}
        className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port"
      />
      {selected ? <span className="sr-only">{t('timeline.briefEraNode.selected')}</span> : null}
    </NodeChromeShell>
  );
});

/**
 * Event node — projected onto the when-axis. Body chrome extracted to
 * `TimelineEventChrome` (V1.124). Temporal-unknown badge surfaces the
 * architect's honest-empty-state rule when `occurred_at` is absent.
 */
export const TimelineEventNode = memo(function TimelineEventNode({
  data,
  selected,
  dragging,
}: NodeProps) {
  const d = data as TimelineNodeData;
  const { t } = useTranslation('canvas');
  const anchorCount = anchorCountOf(d);

  return (
    <NodeChromeShell
      selected={selected}
      dragging={dragging}
      accent="worldkb"
      aria-label={t('timeline.eventNode.aria', { name: d.canonical_name })}
    >
      <Handle
        type="target"
        position={Position.Left}
        className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port"
      />
      <TimelineEventChrome
        title={d.canonical_name || t('timeline.eventNode.unnamed')}
        blockTypeLabel={BLOCK_TYPE_LABELS[d.block_type]}
        occurredAtHint={d.occurredAtHint ?? null}
        temporalUnknownLabel={t('timeline.eventNode.temporalUnknown')}
        sourceAnchorLabel={t('timeline.eventNode.sourceAnchorCount', {
          count: anchorCount,
        })}
        version={d.version}
      />
      <Handle
        type="source"
        position={Position.Right}
        className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port"
      />
      {selected ? <span className="sr-only">{t('timeline.eventNode.selected')}</span> : null}
    </NodeChromeShell>
  );
});

/**
 * Context cluster node — non-event, non-era KeyBlock entity. Body chrome
 * extracted to `TimelineKeyBlockChrome` (V1.124).
 */
export const TimelineKeyBlockNode = memo(function TimelineKeyBlockNode({
  data,
  selected,
  dragging,
}: NodeProps) {
  const d = data as TimelineNodeData;
  const { t } = useTranslation('canvas');
  const anchorCount = anchorCountOf(d);

  return (
    <NodeChromeShell
      selected={selected}
      dragging={dragging}
      accent="worldkb"
      aria-label={t('timeline.keyBlockNode.aria', { name: d.canonical_name })}
    >
      <Handle
        type="target"
        position={Position.Left}
        className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port"
      />
      <TimelineKeyBlockChrome
        title={d.canonical_name || t('timeline.keyBlockNode.unnamed')}
        blockTypeLabel={BLOCK_TYPE_LABELS[d.block_type]}
        sourceAnchorLabel={t('timeline.keyBlockNode.sourceAnchorCount', {
          count: anchorCount,
        })}
        version={d.version}
      />
      <Handle
        type="source"
        position={Position.Right}
        className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port"
      />
      {selected ? <span className="sr-only">{t('timeline.keyBlockNode.selected')}</span> : null}
    </NodeChromeShell>
  );
});

/**
 * V1.147 P2 T3 — Compute result node (Narrative layer). Thin App-local RF
 * wrapper over the promoted `ComputeResultNodeChrome` primitive: i18n copy
 * resolution + `NodeChromeShell` + Handles only. The chrome reads the
 * adapter-built `compute` payload (provenance + digest).
 *
 * Same-family rule (behavior spec §5): shares the Narrative visual language
 * with `TimelineEventNode` (worldkb spine + narrative accents); the Cpu icon,
 * "Compute result" kind pill, and provenance chip carry the distinction.
 */
export const TimelineComputeResultNode = memo(function TimelineComputeResultNode({
  data,
  selected,
  dragging,
}: NodeProps) {
  const d = data as TimelineNodeData;
  const { t } = useTranslation('canvas');
  const payload = d.compute;

  // Defensive: a compute node always carries a payload (the adapter builds
  // them together); render nothing honest otherwise.
  if (!payload) return null;

  const provenanceLabel =
    payload.sourceKind === 'preset'
      ? t('timeline.computeNode.provenance.preset')
      : t('timeline.computeNode.provenance.direct');

  return (
    <NodeChromeShell
      selected={selected}
      dragging={dragging}
      accent="worldkb"
      aria-label={t('timeline.computeNode.aria', { name: d.canonical_name })}
    >
      <Handle
        type="target"
        position={Position.Left}
        className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port"
      />
      <ComputeResultNodeChrome
        title={d.canonical_name || payload.moduleName}
        kindLabel={t('timeline.computeNode.kindLabel')}
        provenanceLabel={provenanceLabel}
        moduleName={payload.moduleName}
        moduleVersion={payload.moduleVersion}
        runId={payload.runId ? shortId(payload.runId) : undefined}
      />
      <Handle
        type="source"
        position={Position.Right}
        className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port"
      />
      {selected ? <span className="sr-only">{t('timeline.computeNode.selected')}</span> : null}
    </NodeChromeShell>
  );
});

export const timelineNodeTypes = {
  'timeline-brief-era': TimelineBriefEraNode,
  'timeline-event': TimelineEventNode,
  'timeline-key-block': TimelineKeyBlockNode,
  'timeline-compute-result': TimelineComputeResultNode,
  'directedAxisSpine': DirectedAxisSpine,
} as const;
