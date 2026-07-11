/**
 * Real React Flow integration test harness (V1.109 P2 T3; FB-GS-002).
 *
 * CanvasShell was fully mocked in the outline-canvas orchestrator tests
 * (`R-V1108P0QC3-S003`), so the graph-click → inspector selection wiring — the
 * very integration `useOutlineCanvasGraph` exists to provide — had no real
 * React Flow coverage: a regression that silently broke RF node selection would
 * pass every div-stub test.
 *
 * This harness renders a genuine `<ReactFlowProvider>` + `<ReactFlow>` tree
 * consuming the same node/edge/selection props the orchestrator passes to
 * `CanvasShell`, so integration tests exercise real RF state (node click →
 * `onNodesChange` → hook selection-sync → inspector) without mounting the full
 * MiniMap / Controls / Background chrome. The ResizeObserver polyfill installed
 * in `src/test/setup.ts` is sufficient for RF to mount in jsdom (the same path
 * `outline-page.test.tsx` relies on when it mounts the real page).
 *
 * Install via the canvas-shell module mock so the orchestrator's `<CanvasShell>`
 * call site renders real RF:
 *
 * ```ts
 * vi.mock('@/components/canvas/canvas-shell', async () => {
 *   const h = await import('@/components/canvas/__tests__/rf-integration-harness');
 *   return {
 *     CanvasShell: h.RFIntegrationHarness,
 *     useNodeChangeHandler: h.testUseNodeChangeHandler,
 *   };
 * });
 * ```
 *
 * The harness intentionally keeps the prop shape compatible with `CanvasShell`
 * (`nodes`, `edges`, `nodeTypes`, `onNodesChange`, `summaryText`, `ariaLabel`,
 * `children`, `surfaceKey`) so the orchestrator's JSX needs no test-only
 * branching. `surfaceKey` is accepted for parity but unused — viewport caching
 * is a CanvasShell chrome concern, not an RF integration concern.
 */
import { useCallback, type ReactNode } from 'react';
import {
  ReactFlow,
  ReactFlowProvider,
  applyNodeChanges,
  type Edge,
  type Node,
  type NodeChange,
  type NodeTypes,
  type OnNodesChange,
} from '@xyflow/react';

export interface RFIntegrationHarnessProps {
  nodes: Node[];
  edges: Edge[];
  nodeTypes: NodeTypes;
  onNodesChange: OnNodesChange;
  /** Graph-level SR summary (A8) — rendered in a live region, mirrors CanvasShell. */
  summaryText?: string;
  /** Accessible label for the canvas region. */
  ariaLabel?: string;
  /** Overlay children rendered above the graph (EmptyState overlay, etc.). */
  children?: ReactNode;
  /** Accepted for CanvasShell prop-shape parity; unused by the harness. */
  surfaceKey?: string;
}

function HarnessInner({
  nodes,
  edges,
  nodeTypes,
  onNodesChange,
  summaryText,
  ariaLabel,
  children,
}: RFIntegrationHarnessProps) {
  return (
    <div
      data-testid="rf-integration-harness"
      aria-label={ariaLabel}
      className="relative h-[480px] w-full overflow-hidden"
    >
      {/* Screen-reader graph summary — live region, matches CanvasShell. */}
      {summaryText ? (
        <div className="sr-only" role="status" aria-live="polite" aria-atomic="true">
          {summaryText}
        </div>
      ) : null}
      {/*
        Nodes are non-draggable in jsdom (no real pointer geometry); selection
        via click is the integration path under test, so `nodesFocusable` keeps
        the keyboard/click surface real. `fitView` is off so node positions do
        not depend on measured dimensions (zero in jsdom) — nodes render at
        their projected absolute positions.

        Pan/zoom gestures are disabled: jsdom dispatches synthetic MouseEvents
        with `view: null`, so when a node click's `mousedown` bubbles to the RF
        pane, d3-zoom's handler calls d3-drag's `nodrag(event.view)` which reads
        `view.document` and throws (`Cannot read properties of null`). Real
        CanvasShell keeps panning on (the browser provides a real `view`); the
        harness only needs node rendering + click selection, so we turn the
        mousedown/wheel-triggered gestures off to keep the jsdom run clean.
      */}
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        onNodesChange={onNodesChange}
        nodesFocusable
        nodesDraggable={false}
        edgesFocusable
        panOnDrag={false}
        zoomOnScroll={false}
        zoomOnPinch={false}
        zoomOnDoubleClick={false}
        proOptions={{ hideAttribution: true }}
        aria-label={ariaLabel}
      />
      {children}
    </div>
  );
}

/**
 * Real-RF wrapper. The `ReactFlowProvider` is required so any child overlay
 * using `useReactFlow` (and RF's internal store) is wired the same way it is
 * inside the real `CanvasShell`.
 */
export function RFIntegrationHarness(props: RFIntegrationHarnessProps) {
  return (
    <ReactFlowProvider>
      <HarnessInner {...props} />
    </ReactFlowProvider>
  );
}

/**
 * Test-friendly `useNodeChangeHandler` — identical RF state plumbing to the
 * real CanvasShell helper (`applyNodeChanges` over `setNodes`). Install this as
 * the mock for `useNodeChangeHandler` so the real `useOutlineCanvasGraph` hook
 * gets a working `onNodesChange` that applies RF selection / drag changes to
 * its node state. Without it, RF's selection change events would be dropped
 * and graph-click → inspector selection could never fire.
 */
export function testUseNodeChangeHandler(
  setNodes: React.Dispatch<React.SetStateAction<Node[]>>,
): OnNodesChange {
  return useCallback(
    (changes: NodeChange[]) => {
      setNodes((nds) => applyNodeChanges(changes, nds));
    },
    [setNodes],
  );
}
