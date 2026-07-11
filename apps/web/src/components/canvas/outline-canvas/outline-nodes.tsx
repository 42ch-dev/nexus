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
import { Handle, Position, type NodeProps } from '@xyflow/react';

import type { ChapterStatus } from '@42ch/nexus-contracts';

import type {
  OutlineChapterNodeData,
  OutlineTimelineEventNodeData,
  OutlineVolumeNodeData,
} from './rf-projection';
import { STATUS_OPTIONS } from './graph-projection';
import { OutlineSceneNode, OutlineBeatNode } from './scene-beat-nodes';

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

const STATUS_LABEL: Record<ChapterStatus, string> = {
  not_started: 'Not started',
  outlined: 'Outlined',
  draft: 'Draft',
  finalized: 'Finalized',
  published: 'Published',
};

function statusColorVar(status: ChapterStatus): string {
  return `var(${STATUS_TOKEN_VAR[status]})`;
}

// ---------------------------------------------------------------------------
// Shared node shell
// ---------------------------------------------------------------------------

interface NodeShellProps {
  selected: boolean;
  children: React.ReactNode;
  className?: string;
  style?: React.CSSProperties;
}

function NodeShell({ selected, children, className, style }: NodeShellProps) {
  return (
    <div
      className={[
        'min-w-[176px] rounded-card border bg-canvas-node-fill px-3 py-2 shadow-card transition-colors duration-state ease-standard',
        selected ? 'border-canvas-node-border-selected' : 'border-canvas-node-border',
        className ?? '',
      ].join(' ')}
      style={style}
    >
      {children}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Volume node
// ---------------------------------------------------------------------------

/** Volume lane node — label + chapter count, uses `canvas-outline-volume-fill`. */
export const OutlineVolumeNode = memo(function OutlineVolumeNode({
  data,
  selected,
}: NodeProps) {
  const d = data as OutlineVolumeNodeData;
  return (
    <NodeShell
      selected={!!selected}
      style={{
        background: 'var(--color-canvas-outline-volume-fill)',
      }}
    >
      <Handle type="source" position={Position.Right} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
      <span className="font-heading text-copy-14 font-semibold text-gray-1000">{d.label}</span>
      <p className="mt-0.5 text-label-12 text-gray-700">
        {d.chapterCount} {d.chapterCount === 1 ? 'chapter' : 'chapters'}
      </p>
    </NodeShell>
  );
});

// ---------------------------------------------------------------------------
// Chapter node
// ---------------------------------------------------------------------------

/** Chapter card node — title, status paint dot, slug, word count. */
export const OutlineChapterNode = memo(function OutlineChapterNode({
  data,
  selected,
}: NodeProps) {
  const d = data as OutlineChapterNodeData;
  const statusColor = statusColorVar(d.status);
  return (
    <NodeShell selected={!!selected}>
      <Handle type="target" position={Position.Left} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
      <div className="flex items-center justify-between gap-2">
        <span className="truncate font-heading text-copy-14 font-semibold text-gray-1000" title={d.title}>
          {d.title}
        </span>
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-1">
        <span
          className="flex items-center gap-1 rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 text-label-12"
          style={{ color: statusColor, background: `color-mix(in srgb, ${statusColor} 12%, transparent)` }}
        >
          <span className="inline-block h-2 w-2 rounded-pill" style={{ background: statusColor }} aria-hidden />
          {STATUS_LABEL[d.status]}
        </span>
        {d.slug ? (
          <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
            {d.slug}
          </span>
        ) : null}
      </div>
      <p className="mt-1 text-label-12 text-gray-700">
        {d.actualWordCount ?? 0}/{d.plannedWordCount} words
      </p>
      <Handle type="source" position={Position.Right} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
    </NodeShell>
  );
});

// ---------------------------------------------------------------------------
// Timeline event node
// ---------------------------------------------------------------------------

/** Timeline event lane node — title, realizes-chapter hint. */
export const OutlineTimelineEventNode = memo(function OutlineTimelineEventNode({
  data,
  selected,
}: NodeProps) {
  const d = data as OutlineTimelineEventNodeData;
  return (
    <NodeShell
      selected={!!selected}
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
        {d.realizesChapterId !== null ? `Realizes chapter ${d.realizesChapterId}` : 'Unattached event'}
      </p>
      <Handle type="source" position={Position.Right} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
    </NodeShell>
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
