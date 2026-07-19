/**
 * Timeline canvas node types — V1.122 P1 T2 + V1.123 P1 T2.
 *
 * Three node kinds, all projecting from `WorldKbEntityProjection`:
 *   • timeline-brief-era  — V1.123 P1 T2 Brief-era marker
 *                           (`block_type=era` projected onto the Brief
 *                           when-axis). Renders the era's canonical name +
 *                           era-icon + time-span label (start_hint → end_hint)
 *                           + optional world-summary line. Compact era
 *                           marker card distinct from the Narrative event
 *                           node (layer-feel-differentiation.md §2.2 —
 *                           minimal density, era sweep).
 *   • timeline-event      — V1.122 `block_type=event` projected onto the
 *                           Narrative when-axis. Renders the event's
 *                           canonical name + temporal signal (or a temporal-
 *                           unknown badge when `occurred_at` is absent) +
 *                           source anchor count.
 *   • timeline-key-block  — V1.122 non-event, non-era KeyBlock entity in the
 *                           Narrative Context cluster. Renders the entity's
 *                           canonical name + BlockType label + source anchor
 *                           count.
 *
 * Chrome tokens: reuses V1.121 `canvas-node-fill` / `canvas-node-border`
 * via `NodeChromeShell`. The Timeline surface is World-scoped, so it adopts
 * the existing `worldkb` accent spine (teal-700 per DESIGN.md §Canvas
 * Surface) — V1.122 + V1.123 P1 T2 introduce NO new accent token
 * (`wire_contracts_changed: true` attributable to Task 1's single additive
 * enum value only).
 *
 * V1.123 P4 Task 2 — per-layer feel accent migration (layer-feel-
 * differentiation.md §6.1 + AC-V1123-20): the Brief-era node now carries
 * the dedicated `--color-canvas-layer-brief-accent` token (gold-bronze
 * "age" tone) instead of the worldkb teal spine. This is the per-LAYER
 * accent within the World Timeline surface — the era-icon + time-span
 * badge both read against the gold-bronze hue so a screenshot of Brief
 * vs Narrative reads as a different instrument without reading chrome
 * labels. The card's surface spine stays `accent="worldkb"` because the
 * Timeline surface identity is still World-scoped (the layer accent is
 * an INTRA-surface differentiator, not a surface identity override).
 *
 * No `TimelineForkMarkerNode` exists in V1.122 / V1.123. Fork data is
 * reserved for an optional canvas-header badge from the `WorldState`
 * sidecar (V1.122 T3 chrome).
 */
import { memo } from 'react';
import { useTranslation } from 'react-i18next';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { Hourglass } from 'lucide-react';

import { NodeChromeShell } from '../presentational/node-chrome-shell';
import { BLOCK_TYPE_LABELS } from '../world-kb/types';
import type { TimelineNodeData } from './timeline-canvas-adapter';

function anchorCountOf(d: TimelineNodeData): number {
  return typeof d.source_anchor_count === 'number' ? d.source_anchor_count : 0;
}

/**
 * V1.123 P1 T2 — Brief-era node. Compact era marker card on the Brief
 * when-axis. Carries era markers (`eraId`, `startHint`, `endHint`,
 * `worldSummary`) extracted from `body.attributes` per architect §2.3 + §8.
 *
 * Visual differentiation from the Narrative event node (layer-feel-
 * differentiation.md §2.2): the era-icon (`Hourglass` — gold/bronze literary
 * tone via `text-canvas-layer-brief-accent`) leads the card so a screenshot
 * of Brief vs Narrative reads as a different instrument without reading
 * chrome labels. The Brief-era layer accent is the dedicated
 * `--color-canvas-layer-brief-accent` token (P4 Task 2 — alias amber-700
 * gold-bronze per layer-feel §6.1).
 *
 * The time-span label renders `start_hint → end_hint` when both are present,
 * either hint alone when only one is present, and a temporal-unknown pill
 * when neither is present — mirroring the V1.122 event node's temporal-
 * unknown badge so the undated era cluster reads as "and these existed, we
 * don't know when" rather than "broken data".
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
      <div className="flex items-center gap-2">
        <Hourglass
          className="h-4 w-4 flex-shrink-0 text-canvas-layer-brief-accent"
          aria-hidden
        />
        <span
          className="truncate font-heading text-copy-14 font-semibold text-gray-1000"
          title={d.canonical_name}
        >
          {d.canonical_name || t('timeline.briefEraNode.unnamed')}
        </span>
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-1">
        <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
          {BLOCK_TYPE_LABELS[d.block_type]}
        </span>
        {span ? (
          <span className="rounded-pill border border-canvas-layer-brief-accent/30 bg-canvas-layer-brief-accent/15 px-1.5 py-0.5 text-label-12 text-canvas-layer-brief-accent">
            {span}
          </span>
        ) : (
          <span className="rounded-pill border border-gray-alpha-400 bg-gray-alpha-100 px-1.5 py-0.5 text-label-12 text-gray-700">
            {t('timeline.briefEraNode.temporalUnknown')}
          </span>
        )}
        {d.eraId ? (
          <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
            {d.eraId}
          </span>
        ) : null}
      </div>
      {d.worldSummary ? (
        <p
          className="mt-1 line-clamp-2 text-label-12 text-gray-700"
          title={d.worldSummary}
        >
          {d.worldSummary}
        </p>
      ) : null}
      <p className="mt-1 text-label-12 text-gray-700">
        {t('timeline.briefEraNode.sourceAnchorCount', { count: anchorCount })} · v{d.version}
      </p>
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
 * Context cluster node — non-event, non-era KeyBlock entity (character /
 * scene / organization / item / etc.) projected off the when-axis. Visually
 * paired with the event nodes via typed relationship edges.
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
  'timeline-brief-era': TimelineBriefEraNode,
  'timeline-event': TimelineEventNode,
  'timeline-key-block': TimelineKeyBlockNode,
} as const;
