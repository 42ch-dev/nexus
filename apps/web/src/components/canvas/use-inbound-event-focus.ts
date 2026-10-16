import { useEffect, useRef } from 'react';
import type { Node, OnNodesChange } from '@xyflow/react';

/**
 * The slice of `useCanvasSurface`'s result the inbound focus reads.
 * (`UseCanvasSurfaceResult<TNodeData, TEdgeData>` is structurally compatible,
 * so both Timeline orchestrators pass their `surface` straight through.)
 */
export interface InboundFocusSurface<TNodeData extends Record<string, unknown>> {
  nodes: Node<TNodeData>[];
  selectedNodeId: string | null;
  onNodesChange: OnNodesChange;
}

/**
 * V1.163 P1 inbound `?event=` focus — the shared body of the World
 * (`timeline-canvas.tsx` Task 2) and Work (`work-timeline-canvas.tsx` Task 3)
 * Timeline effects (R-V1163P1QC1-002). Selects the projected React Flow node
 * named by the deep-link's `?event=` param; selection drives the existing
 * inspector — no new focus primitive.
 *
 * AC-V1163-3 honest empty focus: an unknown id matches no node → nothing is
 * selected and nothing errors — the layer loads normally. The `appliedRef`
 * one-shot guard (per URL value) ensures a LATER user selection is never
 * fought by a re-running effect: once the deep-link selection has been
 * applied (or the id is unknown), the effect stays quiet until the target id
 * itself changes.
 *
 * The two surfaces differ in exactly one way — whether an id match is enough.
 * The World Timeline's `entity:` namespace is SHARED with context-entity and
 * Brief-era nodes, so it passes `isFocusTarget` to keep focus on Narrative
 * EVENT nodes (`QC2 F-001`: a crafted `?event=<context-entity-id>` must not
 * select those). The Work Timeline's `wt-event:` namespace is event-exclusive,
 * so it omits the rule; omitting it means "any node with this id".
 */
export function useInboundEventFocus<TNodeData extends Record<string, unknown>>(
  surface: InboundFocusSurface<TNodeData>,
  targetNodeId: string | null,
  isFocusTarget?: (node: Node<TNodeData>) => boolean,
): void {
  const appliedRef = useRef<string | null>(null);
  const { nodes, selectedNodeId, onNodesChange } = surface;

  useEffect(() => {
    if (!targetNodeId) return;
    if (appliedRef.current === targetNodeId) return;
    if (selectedNodeId === targetNodeId) {
      // Selection already applied (e.g. it survived a projection rebuild) —
      // mark it applied so we never fight a later user selection.
      appliedRef.current = targetNodeId;
      return;
    }
    const exists = nodes.some(
      (n) => n.id === targetNodeId && (isFocusTarget?.(n) ?? true),
    );
    if (!exists) return; // unknown / non-target id — no fabricated selection
    onNodesChange(
      nodes.map((n) => ({
        type: 'select' as const,
        id: n.id,
        selected: n.id === targetNodeId,
      })),
    );
    appliedRef.current = targetNodeId;
  }, [targetNodeId, nodes, selectedNodeId, onNodesChange, isFocusTarget]);
}
