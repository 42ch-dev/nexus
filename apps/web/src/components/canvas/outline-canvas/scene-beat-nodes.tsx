/**
 * Outline canvas Scene + Beat custom node types (V1.109 C2 — FB-C2-001).
 *
 * These node kinds render scene-level and beat-level structure **inside**
 * Chapter parent nodes on the Outline spatial graph (FB-C2-000). They consume
 * the `canvas-outline-scene-*` / `canvas-outline-beat-*` DESIGN tokens
 * (FB-C2-004) and follow the same NodeShell + selection pattern as the
 * Volume/Chapter/Timeline nodes in `outline-nodes.tsx`.
 *
 * Node kinds:
 *   • outline-scene — Scene card (title + status), child of a Chapter parent
 *   • outline-beat  — Beat card (title), child of a Scene parent
 *
 * The projection (`rf-projection.ts`) emits these node kinds from fixture-
 * injected data (T2). The data interfaces live in `rf-projection.ts` (the
 * projection SSOT) so the payloads carry the identity fields (`workId`,
 * `sceneId`, `chapterId`, `beatId`) the projection needs to emit `parentId` +
 * `extent: "parent"`. This module re-exports them for consumers that imported
 * them from the node-component barrel historically.
 */
import { memo } from 'react';
import { useTranslation } from 'react-i18next';
import { Handle, Position, type NodeProps } from '@xyflow/react';

import type {
  OutlineBeatNodeData,
  OutlineSceneNodeData,
  OutlineSceneStatus,
} from './rf-projection';
import { SCENE_STATUS_LABEL_KEYS } from './graph-projection';
import { NodeChromeShell } from '../presentational/node-chrome-shell';

// Re-export for type-only consumers that historically imported these from the
// node component module (T1 defined them here; T2 promoted the canonical
// definitions to `rf-projection.ts` — the projection SSOT — so the data
// payloads carry the identity fields `workId` / `sceneId` / `chapterId` /
// `beatId` the projection needs to emit `parentId` + `extent`).
export type {
  OutlineBeatNodeData,
  OutlineSceneNodeData,
  OutlineSceneStatus,
} from './rf-projection';

// ---------------------------------------------------------------------------
// Status → token + label (canvas-outline-scene-status-* — DESIGN.md)
// ---------------------------------------------------------------------------

const SCENE_STATUS_TOKEN_VAR: Record<OutlineSceneStatus, string> = {
  drafted: '--color-canvas-outline-scene-status-drafted',
  completed: '--color-canvas-outline-scene-status-completed',
};

function sceneStatusColorVar(status: OutlineSceneStatus): string {
  return `var(${SCENE_STATUS_TOKEN_VAR[status]})`;
}

// ---------------------------------------------------------------------------
// Scene node
// ---------------------------------------------------------------------------

/** Scene card node — title (**Untitled Scene** fallback) + status paint. */
export const OutlineSceneNode = memo(function OutlineSceneNode({
  data,
  selected,
  dragging,
}: NodeProps) {
  const { t } = useTranslation('canvas');
  const d = data as OutlineSceneNodeData;
  const title = d.title?.trim() ? d.title : t('outlineAltView.untitledScene');
  return (
    <NodeChromeShell
      selected={!!selected}
      accent="outline"
      dragging={dragging}
      className="min-w-canvas-node-outline-scene-beat"
      style={{
        background: 'var(--color-canvas-outline-scene-fill)',
        borderColor: selected ? undefined : 'var(--color-canvas-outline-scene-border)',
      }}
    >
      <Handle type="target" position={Position.Left} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
      <span className="truncate font-heading text-copy-14 font-semibold text-gray-1000" title={title}>
        {title}
      </span>
      {d.status ? (
        <div className="mt-1 flex flex-wrap items-center gap-1">
          <span
            className="flex items-center gap-1 rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 text-label-12"
            style={{
              color: sceneStatusColorVar(d.status),
              background: `color-mix(in srgb, ${sceneStatusColorVar(d.status)} 12%, transparent)`,
            }}
          >
            <span
              className="inline-block h-2 w-2 rounded-pill"
              style={{ background: sceneStatusColorVar(d.status) }}
              aria-hidden
            />
            {t(SCENE_STATUS_LABEL_KEYS[d.status])}
          </span>
        </div>
      ) : null}
      <Handle type="source" position={Position.Right} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
    </NodeChromeShell>
  );
});

// ---------------------------------------------------------------------------
// Beat node
// ---------------------------------------------------------------------------

/** Beat card node — title (**Untitled Beat** fallback). No status tier. */
export const OutlineBeatNode = memo(function OutlineBeatNode({
  data,
  selected,
  dragging,
}: NodeProps) {
  const { t } = useTranslation('canvas');
  const d = data as OutlineBeatNodeData;
  const title = d.title?.trim() ? d.title : t('outlineAltView.untitledBeat');
  return (
    <NodeChromeShell
      selected={!!selected}
      accent="outline"
      dragging={dragging}
      className="min-w-canvas-node-outline-scene-beat"
      style={{
        background: 'var(--color-canvas-outline-beat-fill)',
        borderColor: selected ? undefined : 'var(--color-canvas-outline-beat-border)',
      }}
    >
      <Handle type="target" position={Position.Left} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
      <span className="truncate font-heading text-copy-14 font-semibold text-gray-1000" title={title}>
        {title}
      </span>
      <Handle type="source" position={Position.Right} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
    </NodeChromeShell>
  );
});
