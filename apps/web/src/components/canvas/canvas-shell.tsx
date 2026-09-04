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
  type FitViewOptions,
  type Node,
  type NodeChange,
  type NodeTypes,
  type OnConnect,
  type OnEdgesChange,
  type OnNodesChange,
  type OnReconnect,
} from '@xyflow/react';

import type { CanvasSurfaceKind } from './canvas-surface-adapter';
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
  /**
   * V1.123 P3 T2 — surface identity driving visual prominence treatment.
   * When set to `'timeline'` or `'work-timeline'`, the shell renders a
   * {@link CanvasShellTimelineBadge} overlay so the Timeline surface reads
   * as the central instrument (per `three-layer-product-spec.md`).
   * Other surfaces (`'strategy'`, `'outline'`, `'world-kb-*'`) render no
   * badge; their per-surface accent already lives in the node stroke color
   * (`--color-canvas-{strategy,outline,worldkb}-accent`).
   *
   * The attribute also surfaces on the shell root as
   * `data-surface-kind` for downstream styling / integration tests.
   */
  surfaceKind?: CanvasSurfaceKind;
  /**
   * V1.126 P1 — optional fitView options override. When provided, these
   * options are merged into the default `{ padding: 0.2 }`. Surfaces that
   * need to exclude certain nodes from fit-bounds calculations (e.g. the
   * directed-axis-spine decoration node) can pass a `nodes` filter here.
   */
  fitViewOptions?: FitViewOptions;
  /**
   * v1.183 P0 (R-V1121P3QC1-S002) — surface-aware MiniMap node swatch color.
   * Defaults to the strategy accent (the shell's original hardcoded color);
   * surfaces with their own accent token pass their
   * `var(--color-canvas-*-accent)` so the minimap reads as part of the active
   * surface instead of always strategy-purple. Outline, World KB, World
   * Timeline, and Work Timeline all opt in; only the strategy surface keeps
   * the default.
   */
  minimapAccent?: string;
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
  surfaceKind,
  fitViewOptions,
  // Strategy accent is the original hardcoded swatch; surfaces with their
  // own accent token opt in via the minimapAccent prop (R-V1121P3QC1-S002).
  minimapAccent = 'var(--color-canvas-strategy-accent)',
}: CanvasShellProps) {
  const { t } = useTranslation('canvas');
  // FB-GS-000 — cache pan/zoom so a graph↔list toggle does not drop the
  // viewport. When a cached viewport exists, restore it instead of fitting.
  const { cachedViewport, onViewportChange } = useCanvasViewport(surfaceKey);
  const hasCachedViewport = cachedViewport !== null;
  // V1.121 v0.4 — ambient dot-grid metrics consumed from DESIGN.md §canvas
  // tokens (per-theme tunable; dark canvas is ink, not neutral flip).
  const gridMetrics = useCanvasGridMetrics();

  // V1.123 P3 T2 — Timeline visual prominence. The shell renders a small
  // accent badge overlay when the active surface is the World Timeline or
  // Work Timeline so the central instrument reads as visually distinct from
  // Strategy / Outline / World KB canvases. Other surfaces render no badge.
  const showTimelineBadge =
    surfaceKind === 'timeline' || surfaceKind === 'work-timeline';

  return (
    <div
      data-command-palette-ignore
      data-surface-kind={surfaceKind}
      className="relative h-[calc(100vh-180px)] min-h-[420px] w-full overflow-hidden rounded-card border border-gray-alpha-400 bg-canvas-surface"
    >
      {/* Screen-reader graph summary (A8 #3) — live region, polite. */}
      <div className="sr-only" role="status" aria-live="polite" aria-atomic="true">
        {summaryText}
      </div>

      {/* V1.123 P3 T2 — Timeline prominence badge overlay. Rendered ABOVE
          the React Flow pane (z-10) so the accent treatment stays visible
          across viewport pans / zooms. Sibling to the SR summary so the
          i18n namespace resolves once for both. */}
      {showTimelineBadge ? (
        <CanvasShellTimelineBadge
          label={t('canvasShell.timelineBadge')}
          ariaLabel={t('canvasShell.timelineBadgeAria')}
        />
      ) : null}

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
        fitViewOptions={{ padding: 0.2, ...fitViewOptions }}
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
          nodeColor={() => minimapAccent}
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

/**
 * V1.123 P3 T2 — Timeline visual prominence badge (presentational).
 *
 * A small overlay rendered inside `CanvasShell` when the active surface is
 * the World Timeline or Work Timeline. The badge:
 *   - carries the Timeline accent color (`--color-canvas-timeline-accent`,
 *     the brand-blue per the Canvas/SOUL invariant) so the Timeline surface
 *     reads as visually distinct from Strategy (purple) / Outline (amber) /
 *     World KB (teal);
 *   - exposes `role="status"` + `aria-label` so assistive tech announces the
 *     surface context without sighted-only cues (the accent dot is
 *     `aria-hidden`);
 *   - is exported so the prominence contract can be tested without mounting
 *     the full React Flow chrome.
 *
 * `simplify:` the inline `style` references the CSS variable directly
 * (mirrors the existing per-surface accent pattern in
 * `timeline-canvas-adapter.tsx::deriveTimelineEdges`). No Tailwind utility
 * class is introduced, so no `cn.ts` class-group registration is needed; if
 * a future iteration adds a `text-canvas-timeline-accent` Tailwind class,
 * that class MUST be registered in `packages/nexus-ui/src/lib/cn.ts` per the
 * V1.94/V1.121 tailwind-merge lesson.
 */
export function CanvasShellTimelineBadge({
  label,
  ariaLabel,
}: {
  label: string;
  ariaLabel: string;
}) {
  return (
    <div
      data-testid="canvas-shell-timeline-badge"
      role="status"
      aria-label={ariaLabel}
      className="pointer-events-none absolute left-3 top-3 z-10 inline-flex items-center gap-1.5 rounded-control border bg-background-100 px-2 py-1 text-button-12 font-semibold shadow-elevation-2"
      style={{
        borderColor: 'var(--color-canvas-timeline-accent)',
        color: 'var(--color-canvas-timeline-accent)',
      }}
    >
      <span
        aria-hidden
        className="inline-block h-1.5 w-1.5 rounded-full"
        style={{ backgroundColor: 'var(--color-canvas-timeline-accent)' }}
      />
      {label}
    </div>
  );
}
