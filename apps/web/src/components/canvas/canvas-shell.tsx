/**
 * Canvas Shell — shared React Flow chrome for infinite-canvas surfaces
 * (canvas-strategy-surface.md Draft §3.3).
 *
 * Provides the ReactFlowProvider, pan/zoom controls, minimap, dot-grid
 * background, selection model, keyboard shortcuts, and a screen-reader graph
 * summary (A1 + A8). Per-surface adapters feed `nodes`/`edges`/`nodeTypes`;
 * the shell owns only the interactive chrome and accessibility summary.
 *
 * Route-split: this module (and therefore `@xyflow/react`) is imported only by
 * canvas routes, not by the Control Room bootstrap (Draft §3.1
 * bundle/performance). The React Flow stylesheet is imported here so it lands
 * in the canvas route chunk only.
 */
import { useCallback, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Background,
  BackgroundVariant,
  Controls,
  MiniMap,
  Panel,
  ReactFlow,
  ReactFlowProvider,
  applyNodeChanges,
  type Edge,
  type Node,
  type NodeChange,
  type NodeTypes,
  type OnConnect,
  type OnEdgesChange,
  type OnNodesChange,
  type OnReconnect,
} from '@xyflow/react';

import { useCanvasViewport } from './use-canvas-viewport';

import '@xyflow/react/dist/style.css';

/**
 * Read a numeric CSS custom property from `:root`, with a fallback.
 *
 * V1.121 v0.4 — the dot-grid `gap` and `size` metrics are projected from
 * DESIGN.md §canvas tokens (`canvas-grid-gap`, `canvas-grid-dot-size`) as
 * `--color-canvas-grid-*` vars, so per-theme tuning lives in tokens.css
 * (dark canvas is ink, not a neutral flip — §Design Concept). React Flow's
 * `<Background>` accepts only numeric pixel values for these props, so we
 * resolve the token once on mount instead of hardcoding literals here.
 *
 * `simplify:` reads the value lazily on first render; runtime theme-toggle
 * of gap/size (currently identical across themes) is not subscribed — if the
 * DESIGN pair ever diverges per theme, observe `document.documentElement`
 * class mutations and re-read.
 */
function readCanvasGridMetric(varName: string, fallback: number): number {
  if (typeof document === 'undefined') return fallback;
  const raw = getComputedStyle(document.documentElement)
    .getPropertyValue(varName)
    .trim();
  const parsed = Number.parseFloat(raw);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function useCanvasGridMetrics(): { gap: number; size: number } {
  const [metrics] = useState(() => ({
    gap: readCanvasGridMetric('--color-canvas-grid-gap', 20),
    size: readCanvasGridMetric('--color-canvas-grid-dot-size', 1.5),
  }));
  return metrics;
}

export interface CanvasShellProps {
  nodes: Node[];
  edges: Edge[];
  nodeTypes: NodeTypes;
  onNodesChange: OnNodesChange;
  onEdgesChange?: OnEdgesChange;
  onEdgeClick?: (event: React.MouseEvent, edge: Edge) => void;
  onConnect?: OnConnect;
  /** Edge reconnect handler — when set, edges become draggable to a new end (RF `edgesReconnectable` defaults to true). */
  onReconnect?: OnReconnect<Edge>;
  /** Graph-level summary spoken to assistive tech (A8). */
  summaryText: string;
  /** Accessible label for the canvas region. */
  ariaLabel: string;
  /** Overlay children rendered above the graph (idea input, inspector, etc.). */
  children?: ReactNode;
  /**
   * Stable key for viewport caching across graph↔list toggles (FB-GS-000).
   * When provided, the pan/zoom viewport is cached on user interaction and
   * restored on re-mount instead of re-fitting. Omit to opt out (surfaces
   * that have not opted in keep the previous re-fit behaviour).
   */
  surfaceKey?: string;
  /** Optional re-layout action rendered inside the canvas when provided. */
  relayout?: () => void;
}

/**
 * Inner shell rendered inside a `ReactFlowProvider`. Owns the controlled
 * node/edge state plumbing and the interactive chrome.
 */
function CanvasShellInner({
  nodes,
  edges,
  nodeTypes,
  onNodesChange,
  onEdgesChange,
  onEdgeClick,
  onConnect,
  onReconnect,
  summaryText,
  ariaLabel,
  children,
  surfaceKey,
  relayout,
}: CanvasShellProps) {
  const { t } = useTranslation('canvas');
  // FB-GS-000 — cache pan/zoom so a graph↔list toggle does not drop the
  // viewport. When a cached viewport exists, restore it instead of fitting.
  const { cachedViewport, onViewportChange } = useCanvasViewport(surfaceKey);
  const hasCachedViewport = cachedViewport !== null;
  // V1.121 v0.4 — ambient dot-grid metrics consumed from DESIGN.md §canvas
  // tokens (per-theme tunable; dark canvas is ink, not neutral flip).
  const gridMetrics = useCanvasGridMetrics();

  return (
    <div
      data-command-palette-ignore
      className="relative h-[calc(100vh-180px)] min-h-[420px] w-full overflow-hidden rounded-card border border-gray-alpha-400 bg-canvas-surface"
    >
      {/* Screen-reader graph summary (A8 #3) — live region, polite. */}
      <div className="sr-only" role="status" aria-live="polite" aria-atomic="true">
        {summaryText}
      </div>

      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onEdgeClick={onEdgeClick}
        onConnect={onConnect}
        onReconnect={onReconnect}
        nodesFocusable
        edgesFocusable
        fitView={!hasCachedViewport}
        fitViewOptions={{ padding: 0.2 }}
        defaultViewport={hasCachedViewport ? cachedViewport : undefined}
        onMove={(_, viewport) => onViewportChange(viewport)}
        proOptions={{ hideAttribution: true }}
        aria-label={ariaLabel}
        className="bg-canvas-surface"
      >
        <Background
          variant={BackgroundVariant.Dots}
          gap={gridMetrics.gap}
          size={gridMetrics.size}
          color="var(--color-canvas-grid)"
        />
        <Controls
          className="!rounded-card !border !border-gray-alpha-400 !bg-background-100 !shadow-elevation-2"
          showInteractive={false}
        />
        <MiniMap
          className="!rounded-card !border !border-gray-alpha-400 !bg-background-100 !shadow-elevation-2"
          maskColor="var(--color-canvas-minimap)"
          nodeColor={() => 'var(--color-canvas-strategy-accent)'}
          pannable
          zoomable
        />
        {relayout ? (
          <Panel position="top-right" className="m-0">
            <button
              type="button"
              onClick={relayout}
              className="rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-1.5 text-button-12 text-gray-900 shadow-elevation-2 hover:bg-gray-alpha-100"
            >
              {t('canvasShell.relayout')}
            </button>
          </Panel>
        ) : null}
      </ReactFlow>

      {children}
    </div>
  );
}

/** Controlled-state helper: a minimal `onNodesChange` applier for read-only α. */
export function useNodeChangeHandler(
  setNodes: React.Dispatch<React.SetStateAction<Node[]>>,
): OnNodesChange {
  return useCallback(
    (changes: NodeChange[]) => {
      setNodes((nds) => applyNodeChanges(changes, nds));
    },
    [setNodes],
  );
}

/**
 * Canvas Shell — wraps the inner shell in a `ReactFlowProvider` so child
 * overlays can use React Flow hooks (useReactFlow) if needed.
 */
export function CanvasShell(props: CanvasShellProps) {
  return (
    <ReactFlowProvider>
      <CanvasShellInner {...props} />
    </ReactFlowProvider>
  );
}
