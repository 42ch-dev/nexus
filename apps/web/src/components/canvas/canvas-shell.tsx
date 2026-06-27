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
import { useCallback, type ReactNode } from 'react';
import {
  Background,
  BackgroundVariant,
  Controls,
  MiniMap,
  ReactFlow,
  ReactFlowProvider,
  applyNodeChanges,
  type Edge,
  type Node,
  type NodeChange,
  type NodeTypes,
  type OnEdgesChange,
  type OnNodesChange,
} from '@xyflow/react';

import '@xyflow/react/dist/style.css';

export interface CanvasShellProps {
  nodes: Node[];
  edges: Edge[];
  nodeTypes: NodeTypes;
  onNodesChange: OnNodesChange;
  onEdgesChange?: OnEdgesChange;
  /** Graph-level summary spoken to assistive tech (A8). */
  summaryText: string;
  /** Accessible label for the canvas region. */
  ariaLabel: string;
  /** Overlay children rendered above the graph (idea input, inspector, etc.). */
  children?: ReactNode;
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
  summaryText,
  ariaLabel,
  children,
}: CanvasShellProps) {
  return (
    <div className="relative h-[calc(100vh-180px)] min-h-[420px] w-full overflow-hidden rounded-card border border-gray-alpha-400 bg-canvas-surface">
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
        nodesFocusable
        edgesFocusable
        fitView
        fitViewOptions={{ padding: 0.2 }}
        proOptions={{ hideAttribution: true }}
        aria-label={ariaLabel}
        className="bg-canvas-surface"
      >
        <Background
          variant={BackgroundVariant.Dots}
          gap={20}
          size={1.5}
          color="var(--color-canvas-grid)"
        />
        <Controls
          className="!rounded-card !border !border-gray-alpha-400 !bg-background-100 !shadow-popover"
          showInteractive={false}
        />
        <MiniMap
          className="!rounded-card !border !border-gray-alpha-400 !bg-background-100"
          maskColor="var(--color-canvas-minimap)"
          nodeColor={() => 'var(--color-canvas-strategy-accent)'}
          pannable
          zoomable
        />
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
