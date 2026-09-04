/**
 * Outline canvas custom node types — rendering for the node kinds produced by
 * the RF projection (`rf-projection.ts`; canvas-strategy-surface.md §3.3
 * surface 2 + §3.4).
 *
 * Node kinds:
 *   • outline-volume         — Volume lane node (label + chapter count)
 *   • outline-chapter        — Chapter card (title, status paint, word count)
 *   • outline-timeline-event — Timeline event lane node
 *   • outline-scene          — Scene card (title + status) — V1.109 C2 (FB-C2-001)
 *   • outline-beat           — Beat card (title only) — V1.109 C2 (FB-C2-001)
 *
 * Consumes shared `canvas-node-*` tokens for structural chrome plus the
 * `canvas-outline-*` token family (FB-C1-006) for outline-specific paint
 * (volume fill, chapter status, timeline event pin). Selection pairs
 * `canvas-node-border-selected` with the global focus ring so state is never
 * color-only (Draft §4.4 #6).
 */
import { memo } from 'react';
import { useTranslation } from 'react-i18next';
import { Handle, Position, type NodeProps } from '@xyflow/react';

import type { ChapterStatus } from '@42ch/nexus-contracts';

import type {
  OutlineChapterNodeData,
  OutlineTimelineEventNodeData,
  OutlineVolumeNodeData,
} from './rf-projection';
import { STATUS_LABEL_KEYS, STATUS_OPTIONS } from './graph-projection';
import { OutlineSceneNode, OutlineBeatNode } from './scene-beat-nodes';
import { NodeChromeShell } from '../presentational/node-chrome-shell';

// ---------------------------------------------------------------------------
// Status → token (canvas-outline-chapter-card-status-* — DESIGN.md)
// ---------------------------------------------------------------------------

/**
 * Maps a chapter status to the `canvas-outline-chapter-card-status-*` CSS
 * variable name (per DESIGN.md). DESIGN.md ships three status tokens
 * (pending/drafted/completed); the five-value `ChapterStatus` enum maps as
 * follows (V1.72 compass token alias):
 *   not_started → pending, outlined → pending, draft → drafted,
 *   finalized → completed, published → completed
 */
const STATUS_TOKEN_VAR: Record<ChapterStatus, string> = {
  not_started: '--color-canvas-outline-chapter-card-status-pending',
  outlined: '--color-canvas-outline-chapter-card-status-pending',
  draft: '--color-canvas-outline-chapter-card-status-drafted',
  finalized: '--color-canvas-outline-chapter-card-status-completed',
  published: '--color-canvas-outline-chapter-card-status-completed',
};

function statusColorVar(status: ChapterStatus): string {
  return `var(${STATUS_TOKEN_VAR[status]})`;
}

/**
 * AA text step per status (v1.183 P0 R-V1121P3T4-O001, AR-3): label-12 pill
 * text on the 12% status tint uses the hue's existing `-1000` step (DESIGN.md
 * §Contrast rule — body-copy status text uses `*-1000` on its tinted fill);
 * the raw `*-700` status color fails AA on light tints. pending → gray-1000,
 * drafted → blue-1000, completed → green-1000 (no new tokens).
 */
const STATUS_TEXT_TOKEN_VAR: Record<ChapterStatus, string> = {
  not_started: '--color-gray-1000',
  outlined: '--color-gray-1000',
  draft: '--color-blue-1000',
  finalized: '--color-green-1000',
  published: '--color-green-1000',
};

function statusTextColorVar(status: ChapterStatus): string {
  return `var(${STATUS_TEXT_TOKEN_VAR[status]})`;
}

// ---------------------------------------------------------------------------
// Volume node
// ---------------------------------------------------------------------------

/** Volume lane node — label + chapter count, uses `canvas-outline-volume-fill`. */
export const OutlineVolumeNode = memo(function OutlineVolumeNode({
  data,
  selected,
  dragging,
}: NodeProps) {
  const { t } = useTranslation('canvas');
  const d = data as OutlineVolumeNodeData;
  const label = d.label || t('chapter.volume', { volume: d.volumeId });
  return (
    <NodeChromeShell
      selected={!!selected}
      accent="outline"
      dragging={dragging}
      style={{
        background: 'var(--color-canvas-outline-volume-fill)',
      }}
    >
      <Handle type="source" position={Position.Right} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
      <span className="font-heading text-copy-14 font-semibold text-gray-1000">{label}</span>
      <p className="mt-0.5 text-label-12 text-gray-700">
        {t('structureInspector.chapterCount', { count: d.chapterCount })}
      </p>
    </NodeChromeShell>
  );
});

// ---------------------------------------------------------------------------
// Chapter node
// ---------------------------------------------------------------------------

/** Chapter card node — title, status paint dot, slug, word count. */
export const OutlineChapterNode = memo(function OutlineChapterNode({
  data,
  selected,
  dragging,
}: NodeProps) {
  const { t } = useTranslation('canvas');
  const d = data as OutlineChapterNodeData;
  const statusColor = statusColorVar(d.status);
  return (
    <NodeChromeShell selected={!!selected} accent="outline" dragging={dragging}>
      <Handle type="target" position={Position.Left} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
      <div className="flex items-center justify-between gap-2">
        <span className="truncate font-heading text-copy-14 font-semibold text-gray-1000" title={d.title}>
          {d.title}
        </span>
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-1">
        <span
          className="flex items-center gap-1 rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 text-label-12"
          style={{ color: statusTextColorVar(d.status), background: `color-mix(in srgb, ${statusColor} 12%, transparent)` }}
        >
          <span className="inline-block h-2 w-2 rounded-pill" style={{ background: statusColor }} aria-hidden />
          {t(STATUS_LABEL_KEYS[d.status])}
        </span>
        {d.slug ? (
          <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
            {d.slug}
          </span>
        ) : null}
      </div>
      <p className="mt-1 text-label-12 text-gray-700">
        {t('chapter.wordCount', { actual: d.actualWordCount ?? 0, planned: d.plannedWordCount })}
      </p>
      <Handle type="source" position={Position.Right} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
    </NodeChromeShell>
  );
});

// ---------------------------------------------------------------------------
// Timeline event node
// ---------------------------------------------------------------------------

/** Timeline event lane node — title, realizes-chapter hint. */
export const OutlineTimelineEventNode = memo(function OutlineTimelineEventNode({
  data,
  selected,
  dragging,
}: NodeProps) {
  const { t } = useTranslation('canvas');
  const d = data as OutlineTimelineEventNodeData;
  return (
    <NodeChromeShell
      selected={!!selected}
      accent="outline"
      dragging={dragging}
      style={{
        borderLeftColor: 'var(--color-canvas-outline-timeline-event-pin)',
        borderLeftWidth: '3px',
      }}
    >
      <Handle type="target" position={Position.Left} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
      <span className="truncate font-heading text-copy-14 font-semibold text-gray-1000" title={d.title}>
        {d.title}
      </span>
      {d.description ? (
        <p className="mt-0.5 text-copy-13 text-gray-900 line-clamp-2">{d.description}</p>
      ) : null}
      <p className="mt-0.5 text-label-12 text-gray-700">
        {d.realizesChapterId !== null
          ? t('outlineAltView.realizesChapter', { chapter: d.realizesChapterId })
          : t('outlineAltView.unattachedEvent')}
      </p>
      <Handle type="source" position={Position.Right} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
    </NodeChromeShell>
  );
});

// ---------------------------------------------------------------------------
// nodeTypes map (consumed by CanvasShell — mirrors strategy-nodes.tsx pattern)
// ---------------------------------------------------------------------------

export const outlineNodeTypes = {
  'outline-volume': OutlineVolumeNode,
  'outline-chapter': OutlineChapterNode,
  'outline-timeline-event': OutlineTimelineEventNode,
  'outline-scene': OutlineSceneNode,
  'outline-beat': OutlineBeatNode,
} as const;

// Re-export for consumers that inspect status options from this barrel.
export { STATUS_OPTIONS };
// Re-export Scene/Beat data interfaces + components for type-only consumers
// (projection in Task 2 will import these when emitting scene/beat nodes).
export type { OutlineSceneNodeData, OutlineBeatNodeData, OutlineSceneStatus } from './scene-beat-nodes';
export { OutlineSceneNode, OutlineBeatNode } from './scene-beat-nodes';
