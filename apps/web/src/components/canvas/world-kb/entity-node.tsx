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
import { useTranslation } from 'react-i18next';
import { Handle, Position, type NodeProps } from '@xyflow/react';

import { useContextMenu } from '@/components/path-context-menu';

import { BLOCK_TYPE_LABELS, type EntityLifecycle, type WorldKbNodeData } from './types';
import { WorldKbEntityContextMenu } from './world-kb-entity-context-menu';

const LIFECYCLE_BADGE: Record<EntityLifecycle, { labelKey: string; className: string }> = {
  pending: {
    labelKey: 'worldKb.entityNode.lifecycle.pending',
    className:
      'bg-canvas-worldkb-promotion-pending/15 text-canvas-worldkb-promotion-pending border-canvas-worldkb-promotion-pending/30',
  },
  confirmed: {
    labelKey: 'worldKb.entityNode.lifecycle.confirmed',
    className:
      'bg-canvas-worldkb-promotion-confirmed/15 text-canvas-worldkb-promotion-confirmed border-canvas-worldkb-promotion-confirmed/30',
  },
  rejected: {
    labelKey: 'worldKb.entityNode.lifecycle.rejected',
    className:
      'bg-canvas-worldkb-promotion-rejected/15 text-canvas-worldkb-promotion-rejected border-canvas-worldkb-promotion-rejected/30',
  },
  merged: {
    labelKey: 'worldKb.entityNode.lifecycle.merged',
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
  dragging,
}: NodeProps) {
  const d = data as WorldKbNodeData;
  const { t } = useTranslation('canvas');
  const { open, position, openMenu, close } = useContextMenu();
  const badge = LIFECYCLE_BADGE[d.lifecycle];
  return (
    <div
      data-dragging={dragging ? 'true' : undefined}
      className={[
        // V1.121 P3 T2 — v0.4 elevation recipe (DESIGN.md §Elevation):
        // rest shadow-card (elevation-1) → hover elevation-2 → dragging
        // elevation-4. The data-dragging attribute variant lets the RF
        // wrapper forward the dragging prop without a class-name branch,
        // mirroring NodeChromeShell's pattern.
        'min-w-[200px] max-w-[240px] rounded-card border bg-canvas-worldkb-entity-card-fill-default px-3 py-2 shadow-card transition-shadow duration-state ease-standard hover:shadow-elevation-2 data-[dragging=true]:shadow-elevation-4 focus-visible:outline-none',
        // V1.121 P3 T2 — per-surface accent spine (World KB = teal-700 per
        // DESIGN.md §Canvas Surface). Mirrors NodeChromeShell's spine; this
        // node keeps its bespoke 3-state fill tokens (default/hover/selected)
        // rather than routing through NodeChromeShell because the entity
        // card's selected-fill diverges from `canvas-node-fill`.
        'border-l-[3px] border-l-canvas-worldkb-accent',
        selected
          ? 'border-canvas-worldkb-entity-card-stroke-selected bg-canvas-worldkb-entity-card-fill-selected'
          : 'border-canvas-worldkb-entity-card-stroke-default hover:bg-canvas-worldkb-entity-card-fill-hover',
      ].join(' ')}
      onContextMenu={d.keyBlockId ? openMenu : undefined}
    >
      <Handle
        type="target"
        position={Position.Top}
        className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port"
      />
      <div className="flex items-center justify-between gap-2">
        <span className="truncate font-heading text-copy-14 font-semibold text-gray-1000" title={d.name}>
          {d.name || t('worldKb.entityNode.unnamed')}
        </span>
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-1">
        <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
          {BLOCK_TYPE_LABELS[d.entityKind]}
        </span>
        <span className={`rounded-pill border px-1.5 py-0.5 text-label-12 ${badge.className}`}>
          {t(badge.labelKey)}
        </span>
        {d.computable ? (
          <span className="rounded-pill border border-canvas-worldkb-computable-badge/30 bg-canvas-worldkb-computable-badge/15 px-1.5 py-0.5 text-label-12 text-canvas-worldkb-computable-badge">
            {t('worldKb.entityNode.computable')}
          </span>
        ) : null}
      </div>
      <p className="mt-1 text-label-12 text-gray-700">
        {t('worldKb.entityNode.sourceAnchorCount', { count: d.sourceAnchorCount })} · v{d.version}
      </p>
      <Handle
        type="source"
        position={Position.Bottom}
        className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port"
      />
      {selected ? <span className="sr-only">{t('worldKb.entityNode.selected')}</span> : null}
      {d.keyBlockId && open ? (
        <WorldKbEntityContextMenu
          position={position}
          entityName={d.name || t('worldKb.entityNode.unnamed')}
          onClose={close}
          onConnectTo={() => {
            close();
            // React Flow node data is not directly wired to onConnectTo; the
            // canvas listens for selection change after the menu closes.
            const event = new CustomEvent('world-kb-connect-to', {
              detail: { sourceEntityId: d.keyBlockId! },
            });
            window.dispatchEvent(event);
          }}
        />
      ) : null}
    </div>
  );
});

/** Read-only source-anchor provenance origin node. */
export const WorldKbSourceAnchorNode = memo(function WorldKbSourceAnchorNode({
  data,
  dragging,
}: NodeProps) {
  const d = data as SourceAnchorNodeData;
  const { t } = useTranslation('canvas');
  return (
    <div
      data-dragging={dragging ? 'true' : undefined}
      className="min-w-[140px] max-w-[180px] rounded-card border border-canvas-worldkb-source-anchor-edge/40 border-l-[3px] border-l-canvas-worldkb-accent bg-canvas-worldkb-source-anchor-node px-2 py-1 shadow-card transition-shadow duration-state ease-standard hover:shadow-elevation-2 data-[dragging=true]:shadow-elevation-4"
      aria-label={t('worldKb.entityNode.sourceAnchorAria', { reference: d.reference })}
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
