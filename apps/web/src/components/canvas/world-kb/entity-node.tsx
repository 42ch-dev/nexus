/**
 * World KB canvas custom node types — rendering for the node kinds produced by
 * the graph adapter (canvas-strategy-surface.md §3.3 surface 3 + §3.4).
 *
 * Node kinds:
 *   • worldkb-entity       — confirmed/rejected/merged KeyBlock or pending candidate
 *   • worldkb-source-anchor — read-only provenance origin (derived from kb_source_anchors)
 *
 * Lifecycle is rendered as a colored badge + text label (state is never
 * color-only per Draft §4.4 #6); selection pairs the
 * `canvas-worldkb-entity-card-stroke-selected` token with the global focus ring.
 * The `canvas-worldkb-*` tokens are the V1.73 DESIGN.md SSOT.
 */
import { memo } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';

import { BLOCK_TYPE_LABELS, type EntityLifecycle, type WorldKbNodeData } from './types';

const LIFECYCLE_BADGE: Record<EntityLifecycle, { label: string; className: string }> = {
  pending: {
    label: 'Pending',
    className:
      'bg-canvas-worldkb-promotion-pending/15 text-canvas-worldkb-promotion-pending border-canvas-worldkb-promotion-pending/30',
  },
  confirmed: {
    label: 'Confirmed',
    className:
      'bg-canvas-worldkb-promotion-confirmed/15 text-canvas-worldkb-promotion-confirmed border-canvas-worldkb-promotion-confirmed/30',
  },
  rejected: {
    label: 'Rejected',
    className:
      'bg-canvas-worldkb-promotion-rejected/15 text-canvas-worldkb-promotion-rejected border-canvas-worldkb-promotion-rejected/30',
  },
  merged: {
    label: 'Merged',
    className:
      'bg-canvas-worldkb-promotion-merged/15 text-canvas-worldkb-promotion-merged border-canvas-worldkb-promotion-merged/30',
  },
};

interface SourceAnchorNodeData {
  /** React Flow requires an index signature on node data. */
  [key: string]: unknown;
  relationType: 'source_anchor';
  reference: string;
  sourceType: string;
}

/** Entity card node — name / BlockType / lifecycle badge / source-anchor count. */
export const WorldKbEntityNode = memo(function WorldKbEntityNode({
  data,
  selected,
}: NodeProps) {
  const d = data as WorldKbNodeData;
  const badge = LIFECYCLE_BADGE[d.lifecycle];
  return (
    <div
      className={[
        'min-w-[200px] max-w-[240px] rounded-card border bg-canvas-worldkb-entity-card-fill-default px-3 py-2 shadow-card transition-colors duration-state ease-standard focus-visible:outline-none',
        selected
          ? 'border-canvas-worldkb-entity-card-stroke-selected bg-canvas-worldkb-entity-card-fill-selected'
          : 'border-canvas-worldkb-entity-card-stroke-default hover:bg-canvas-worldkb-entity-card-fill-hover',
      ].join(' ')}
    >
      <Handle
        type="target"
        position={Position.Top}
        className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port"
      />
      <div className="flex items-center justify-between gap-2">
        <span className="truncate font-heading text-copy-14 font-semibold text-gray-1000" title={d.name}>
          {d.name || '(unnamed)'}
        </span>
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-1">
        <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
          {BLOCK_TYPE_LABELS[d.entityKind]}
        </span>
        <span className={`rounded-pill border px-1.5 py-0.5 text-label-12 ${badge.className}`}>
          {badge.label}
        </span>
        {d.computable ? (
          <span className="rounded-pill border border-canvas-worldkb-computable-badge/30 bg-canvas-worldkb-computable-badge/15 px-1.5 py-0.5 text-label-12 text-canvas-worldkb-computable-badge">
            Computable
          </span>
        ) : null}
      </div>
      <p className="mt-1 text-label-12 text-gray-700">
        {d.sourceAnchorCount} {d.sourceAnchorCount === 1 ? 'source anchor' : 'source anchors'} · v{d.version}
      </p>
      <Handle
        type="source"
        position={Position.Bottom}
        className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port"
      />
      {selected ? <span className="sr-only">Selected World KB entity</span> : null}
    </div>
  );
});

/** Read-only source-anchor provenance origin node. */
export const WorldKbSourceAnchorNode = memo(function WorldKbSourceAnchorNode({
  data,
}: NodeProps) {
  const d = data as SourceAnchorNodeData;
  return (
    <div
      className="min-w-[140px] max-w-[180px] rounded-card border border-canvas-worldkb-source-anchor-edge/40 bg-canvas-worldkb-source-anchor-node px-2 py-1 shadow-card"
      aria-label={`Source anchor: ${d.reference}`}
    >
      <Handle
        type="source"
        position={Position.Right}
        className="!h-2 !w-2 !border-canvas-worldkb-source-anchor-edge !bg-canvas-worldkb-source-anchor-edge"
      />
      <p className="truncate font-mono text-label-12 text-gray-700" title={d.reference}>
        {d.sourceType}
      </p>
      <p className="truncate text-label-12 text-gray-900" title={d.reference}>
        {d.reference}
      </p>
    </div>
  );
});

export const worldKbNodeTypes = {
  'worldkb-entity': WorldKbEntityNode,
  'worldkb-source-anchor': WorldKbSourceAnchorNode,
} as const;
