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
 *   • outline-beat  — Beat card (title only), child of a Scene parent
 *
 * NOTE: The projection (`rf-projection.ts`) does not emit these node kinds
 * yet — Task 2 extends the projection to produce Scene/Beat nodes from
 * fixture-injected data. These components are registered in `outlineNodeTypes`
 * now so Task 2 only touches the projection.
 *
 * Data interfaces (`OutlineSceneNodeData`, `OutlineBeatNodeData`) are defined
 * here for T1; Task 2 will extend `rf-projection.ts` to emit them and may
 * promote/re-export the interfaces there.
 */
import { memo } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';

// ---------------------------------------------------------------------------
// Node data payloads (UI-only; wire DTOs in @42ch/nexus-contracts remain SSOT)
// ---------------------------------------------------------------------------

/** Scene status model — two-value (drafted/completed), no pending tier. */
export type OutlineSceneStatus = 'drafted' | 'completed';

/** React Flow node data for a Scene card node. */
export interface OutlineSceneNodeData {
  /** React Flow requires an index signature on node data. */
  [key: string]: unknown;
  /** Scene title; `null`/empty → **Untitled Scene** fallback (Voice & Content). */
  title: string | null;
  /** Scene status; `null` when not yet set → no status chip rendered. */
  status: OutlineSceneStatus | null;
}

/** React Flow node data for a Beat card node. */
export interface OutlineBeatNodeData {
  /** React Flow requires an index signature on node data. */
  [key: string]: unknown;
  /** Beat title; `null`/empty → **Untitled Beat** fallback (Voice & Content). */
  title: string | null;
}

// ---------------------------------------------------------------------------
// Status → token + label (canvas-outline-scene-status-* — DESIGN.md)
// ---------------------------------------------------------------------------

const SCENE_STATUS_TOKEN_VAR: Record<OutlineSceneStatus, string> = {
  drafted: '--color-canvas-outline-scene-status-drafted',
  completed: '--color-canvas-outline-scene-status-completed',
};

const SCENE_STATUS_LABEL: Record<OutlineSceneStatus, string> = {
  drafted: 'Drafted',
  completed: 'Completed',
};

function sceneStatusColorVar(status: OutlineSceneStatus): string {
  return `var(${SCENE_STATUS_TOKEN_VAR[status]})`;
}

// ---------------------------------------------------------------------------
// Shared node shell (mirrors outline-nodes.tsx NodeShell but consumes
// scene/beat fill + border tokens instead of the shared canvas-node-* tokens)
// ---------------------------------------------------------------------------

interface SceneBeatNodeShellProps {
  selected: boolean;
  /** CSS variable name for the fill token (e.g. `var(--color-canvas-outline-scene-fill)`). */
  fillVar: string;
  /** CSS variable name for the border token. */
  borderVar: string;
  children: React.ReactNode;
}

function SceneBeatNodeShell({ selected, fillVar, borderVar, children }: SceneBeatNodeShellProps) {
  return (
    <div
      className={[
        'min-w-[160px] rounded-card border px-3 py-2 shadow-card transition-colors duration-state ease-standard',
        selected ? 'border-canvas-node-border-selected' : '',
      ].join(' ')}
      style={{
        background: fillVar,
        borderColor: selected ? undefined : borderVar,
      }}
    >
      {children}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Scene node
// ---------------------------------------------------------------------------

/** Scene card node — title (**Untitled Scene** fallback) + status paint. */
export const OutlineSceneNode = memo(function OutlineSceneNode({
  data,
  selected,
}: NodeProps) {
  const d = data as OutlineSceneNodeData;
  const title = d.title || 'Untitled Scene';
  return (
    <SceneBeatNodeShell
      selected={!!selected}
      fillVar="var(--color-canvas-outline-scene-fill)"
      borderVar="var(--color-canvas-outline-scene-border)"
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
            {SCENE_STATUS_LABEL[d.status]}
          </span>
        </div>
      ) : null}
      <Handle type="source" position={Position.Right} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
    </SceneBeatNodeShell>
  );
});

// ---------------------------------------------------------------------------
// Beat node
// ---------------------------------------------------------------------------

/** Beat card node — title (**Untitled Beat** fallback). No status tier. */
export const OutlineBeatNode = memo(function OutlineBeatNode({
  data,
  selected,
}: NodeProps) {
  const d = data as OutlineBeatNodeData;
  const title = d.title || 'Untitled Beat';
  return (
    <SceneBeatNodeShell
      selected={!!selected}
      fillVar="var(--color-canvas-outline-beat-fill)"
      borderVar="var(--color-canvas-outline-beat-border)"
    >
      <Handle type="target" position={Position.Left} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
      <span className="truncate font-heading text-copy-14 font-semibold text-gray-1000" title={title}>
        {title}
      </span>
      <Handle type="source" position={Position.Right} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
    </SceneBeatNodeShell>
  );
});
