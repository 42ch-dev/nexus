/**
 * `useOutlineCanvasGraph` — extracted from `outline-canvas.tsx` (V1.109 P0 T1;
 * closes R-V1108P0QC1-S001).
 *
 * Owns the React Flow graph state that previously lived inline in the
 * orchestrator:
 *   • the projection memo (`projectOutlineGraph(outline, chapters)`),
 *   • `rfNodes` / `rfEdges` RF state,
 *   • the projection → RF **position-merge** sync effect (the V1.108 PR-review
 *     fix: merge instead of replace so dragged positions + selection survive
 *     incremental chapter-page loads),
 *   • the graph-click → inspector **selection-sync** effect (FB-C1-003), and
 *   • `selectedChapterId` state + setter.
 *
 * Pure refactor: same inputs, same outputs, same behavior — only the location
 * changed. The orchestrator stays responsible for layout, conflict handling,
 * and patch handlers. Splitting here keeps the orchestrator thin and makes
 * P2's FB-GS-001 selection-memo fix file-disjoint (P2 edits this hook, not the
 * orchestrator).
 */
import { useEffect, useMemo, useState } from 'react';
import type { Edge, Node, OnNodesChange } from '@xyflow/react';

import type { ChapterSummary, WorkOutline } from '@42ch/nexus-contracts';

import { useNodeChangeHandler } from '@/components/canvas/canvas-shell';
import {
  projectOutlineGraph,
  selectedChapterIdFromNodes,
  type OutlineGraphProjection,
} from './rf-projection';

// ---------------------------------------------------------------------------
// Public interface
// ---------------------------------------------------------------------------

export interface UseOutlineCanvasGraphArgs {
  outline: WorkOutline | undefined;
  chapters: ChapterSummary[];
  /** Optional chapter id to preselect on mount (V1.75 F-QC3-001). */
  initialSelectedChapterId?: number | null;
}

export interface UseOutlineCanvasGraphResult {
  rfNodes: Node[];
  rfEdges: Edge[];
  onNodesChange: OnNodesChange;
  selectedChapterId: number | null;
  setSelectedChapterId: React.Dispatch<React.SetStateAction<number | null>>;
  projection: OutlineGraphProjection | null;
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

export function useOutlineCanvasGraph(
  args: UseOutlineCanvasGraphArgs,
): UseOutlineCanvasGraphResult {
  const { outline, chapters, initialSelectedChapterId = null } = args;

  const [selectedChapterId, setSelectedChapterId] = useState<number | null>(
    initialSelectedChapterId ?? null,
  );

  // V1.108 P0 — project the outline into a spatial React Flow graph.
  const projection = useMemo(
    () => (outline ? projectOutlineGraph(outline, chapters) : null),
    [outline, chapters],
  );

  const [rfNodes, setRfNodes] = useState<Node[]>([]);
  const [rfEdges, setRfEdges] = useState<Edge[]>([]);
  const onNodesChange = useNodeChangeHandler(setRfNodes);

  // Sync RF state when the projection changes (data refetch, chapter list
  // update). PR-review fix: merge instead of replace so the author's graph
  // interactions (dragged positions, selection) survive incremental chapter-
  // page loads. `chapters` grows as each cursor page arrives (I-QC1-002
  // auto-fetch), which rebuilds the projection; a bare
  // `setRfNodes(projection.nodes)` wiped every node's user-moved position and
  // selection on each page fetch. For nodes that persist across the rebuild
  // (same id), preserve their `position` and `selected` flag; new nodes use
  // the projected position, dropped nodes fall away. Edges carry no
  // per-interaction state, so they are replaced directly.
  useEffect(() => {
    if (!projection) return;
    setRfEdges(projection.edges);
    setRfNodes((prev) => {
      if (prev.length === 0) return projection.nodes;
      const prevById = new Map(prev.map((n) => [n.id, n]));
      return projection.nodes.map((node) => {
        const existing = prevById.get(node.id);
        if (!existing) return node;
        return { ...node, position: existing.position, selected: existing.selected };
      });
    });
  }, [projection]);

  // Graph click → inspector selection sync (FB-C1-003).
  // React Flow tracks selection via the node `selected` flag (set through
  // onNodesChange). Resolve it to `selectedChapterId` so graph clicks drive the
  // chapter inspector — same pattern as `world-kb-canvas.tsx`.
  //
  // PR-review fix: `selectedChapterIdFromNodes` returns `null` both when no
  // node is selected AND when the selected node does not resolve to a chapter
  // (volume node, or a timeline event with no `realizes_chapter_id`). Distinguish
  // the two: when a node IS selected but resolves to no chapter, clear the
  // chapter selection so the inspector does not show a chapter that is no
  // longer the active graph selection. When nothing is selected at all, leave
  // the current selection intact (preserves V1.75 `?chapter=N` preselect and
  // click-to-keep-while-panning behavior).
  useEffect(() => {
    const chapterId = selectedChapterIdFromNodes(rfNodes);
    if (chapterId !== null) {
      setSelectedChapterId(chapterId);
    } else if (rfNodes.some((n) => n.selected)) {
      setSelectedChapterId(null);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rfNodes]);

  return {
    rfNodes,
    rfEdges,
    onNodesChange,
    selectedChapterId,
    setSelectedChapterId,
    projection,
  };
}
