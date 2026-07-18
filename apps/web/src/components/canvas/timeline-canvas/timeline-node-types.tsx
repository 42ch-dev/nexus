/**
 * Timeline canvas node types — V1.122 P1 T2.
 *
 * Two node kinds, both projecting from `WorldKbEntityProjection`:
 *   • timeline-event      — `block_type=event` projected onto the when-axis.
 *                           Renders the event's canonical name + temporal
 *                           signal (or a temporal-unknown badge when
 *                           `occurred_at` is absent) + source anchor count.
 *   • timeline-key-block  — non-event KeyBlock entity in the Context cluster.
 *                           Renders the entity's canonical name + BlockType
 *                           label + source anchor count.
 *
 * Chrome tokens: reuses V1.121 `canvas-node-fill` / `canvas-node-border`
 * via `NodeChromeShell`. The Timeline surface is World-scoped, so it adopts
 * the existing `worldkb` accent spine (teal-700 per DESIGN.md §Canvas
 * Surface) — V1.122 introduces NO new accent token (`wire_contracts_changed:
 * false`; no new tokens.css entry).
 *
 * No `TimelineForkMarkerNode` exists in V1.122. Fork data is reserved for
 * an optional canvas-header badge from the `WorldState` sidecar (T3 chrome).
 */
import { memo } from 'react';
import { useTranslation } from 'react-i18next';
import { Handle, Position, type NodeProps } from '@xyflow/react';

import { NodeChromeShell } from '../presentational/node-chrome-shell';
import { BLOCK_TYPE_LABELS } from '../world-kb/types';
import type { TimelineNodeData } from './timeline-canvas-adapter';

function anchorCountOf(d: TimelineNodeData): number {
  return typeof d.source_anchor_count === 'number' ? d.source_anchor_count : 0;
}

/**
 * Event node — projected onto the when-axis. The temporal-unknown badge
 * surfaces the architect's honest-empty-state rule: when the entity lacks
 * `body.attributes.occurred_at`, the adapter MUST NOT fabricate chronology.
 * The badge names that explicitly (§7 of the architecture spec).
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
      <div className="flex items-center justify-between gap-2">
        <span
          className="truncate font-heading text-copy-14 font-semibold text-gray-1000"
          title={d.canonical_name}
        >
          {d.canonical_name || t('timeline.eventNode.unnamed')}
        </span>
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-1">
        <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
          {BLOCK_TYPE_LABELS[d.block_type]}
        </span>
        {d.occurredAtHint ? (
          <span className="rounded-pill border border-canvas-worldkb-accent/30 bg-canvas-worldkb-accent/15 px-1.5 py-0.5 text-label-12 text-canvas-worldkb-accent">
            {d.occurredAtHint}
          </span>
        ) : (
          <span className="rounded-pill border border-gray-alpha-400 bg-gray-alpha-100 px-1.5 py-0.5 text-label-12 text-gray-700">
            {t('timeline.eventNode.temporalUnknown')}
          </span>
        )}
      </div>
      <p className="mt-1 text-label-12 text-gray-700">
        {t('timeline.eventNode.sourceAnchorCount', { count: anchorCount })} · v{d.version}
      </p>
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
 * Context cluster node — non-event KeyBlock entity (character / scene /
 * organization / item / etc.) projected off the when-axis. Visually paired
 * with the event nodes via typed relationship edges.
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
      <div className="flex items-center justify-between gap-2">
        <span
          className="truncate font-heading text-copy-14 font-semibold text-gray-1000"
          title={d.canonical_name}
        >
          {d.canonical_name || t('timeline.keyBlockNode.unnamed')}
        </span>
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-1">
        <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
          {BLOCK_TYPE_LABELS[d.block_type]}
        </span>
      </div>
      <p className="mt-1 text-label-12 text-gray-700">
        {t('timeline.keyBlockNode.sourceAnchorCount', { count: anchorCount })} · v{d.version}
      </p>
      <Handle
        type="source"
        position={Position.Right}
        className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port"
      />
      {selected ? <span className="sr-only">{t('timeline.keyBlockNode.selected')}</span> : null}
    </NodeChromeShell>
  );
});

export const timelineNodeTypes = {
  'timeline-event': TimelineEventNode,
  'timeline-key-block': TimelineKeyBlockNode,
} as const;
