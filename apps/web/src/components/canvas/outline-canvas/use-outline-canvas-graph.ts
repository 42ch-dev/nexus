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
import type { SceneBeatFixturePayload } from './graph-projection';
import {
  projectOutlineGraph,
  selectedBeatIdFromNodes,
  selectedChapterIdFromNodes,
  selectedSceneIdFromNodes,
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
  /**
   * Optional Scene/Beat fixture payload (V1.109 C2 T2). When provided, the
   * projection emits Scene/Beat child nodes inside their Chapter/Scene
   * parents. Empty/undefined on real Works — honest empty chrome.
   */
  sceneBeatFixture?: SceneBeatFixturePayload;
}

export interface UseOutlineCanvasGraphResult {
  rfNodes: Node[];
  rfEdges: Edge[];
  onNodesChange: OnNodesChange;
  selectedChapterId: number | null;
  setSelectedChapterId: React.Dispatch<React.SetStateAction<number | null>>;
  /**
   * V1.109 C2 T3 — selected Scene id (FB-C2-002). Drives the Scene inspector.
   * `null` when no Scene node is selected (cleared when a non-Scene node is
   * selected, kept intact when nothing is selected — same contract as
   * {@link selectedChapterId}).
   */
  selectedSceneId: string | null;
  /**
   * V1.109 C2 T3 — selected Beat id (FB-C2-002). Drives the Beat inspector.
   * Same selection contract as {@link selectedSceneId}.
   */
  selectedBeatId: string | null;
  projection: OutlineGraphProjection | null;
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

export function useOutlineCanvasGraph(
  args: UseOutlineCanvasGraphArgs,
): UseOutlineCanvasGraphResult {
  const { outline, chapters, initialSelectedChapterId = null, sceneBeatFixture } = args;

  const [selectedChapterId, setSelectedChapterId] = useState<number | null>(
    initialSelectedChapterId ?? null,
  );
  // V1.109 C2 T3 — Scene/Beat selection state (FB-C2-002). Graph-click drives
  // the Scene/Beat inspector the same way `selectedChapterId` drives the
  // Chapter inspector. The selection-sync effect below resolves all three from
  // RF node selection in one pass.
  const [selectedSceneId, setSelectedSceneId] = useState<string | null>(null);
  const [selectedBeatId, setSelectedBeatId] = useState<string | null>(null);

  // V1.108 P0 — project the outline into a spatial React Flow graph.
  // V1.109 C2 T2 — forward the scene/beat fixture payload so the projection
  // can emit Scene/Beat child nodes (empty/undefined on real Works today).
  const projection = useMemo(
    () => (outline ? projectOutlineGraph(outline, chapters, sceneBeatFixture) : null),
    [outline, chapters, sceneBeatFixture],
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

  // FB-GS-001 — selection-sync overfire guard. RF emits a NEW `rfNodes` array
  // ref on every node interaction, including position-only drags (via
  // `applyNodeChanges`). Depending the selection-sync effect directly on
  // `rfNodes` made it re-run on every drag, re-resolving the selected entity
  // and re-calling the inspector setters — a latent perf trap as graphs grow
  // (R-V1108P0QC3-W001). RF is single-select here, so the selected node's id
  // (or `''` when nothing is selected) fully identifies the selection;
  // resolving chapter/scene/beat from that id is deterministic. This memo
  // returns a primitive that changes ONLY when which node is selected changes.
  const selectionKey = useMemo(
    () => rfNodes.find((n) => n.selected)?.id ?? '',
    [rfNodes],
  );

  // Graph click → inspector selection sync (FB-C1-003 + FB-C2-002).
  // React Flow tracks selection via the node `selected` flag (set through
  // onNodesChange). Resolve it to `selectedChapterId` / `selectedSceneId` /
  // `selectedBeatId` so graph clicks drive the matching inspector — same
  // pattern as `world-kb-canvas.tsx`.
  //
  // PR-review fix: each `selectedXxxIdFromNodes` helper returns `null` both
  // when no node is selected AND when the selected node does not resolve to
  // that kind. Distinguish the two: when a node IS selected but resolves to
  // no entity of that kind, clear that kind's selection so the inspector does
  // not show an entity that is no longer the active graph selection. When
  // nothing is selected at all, leave all selections intact (preserves V1.75
  // `?chapter=N` preselect and click-to-keep-while-panning behavior).
  //
  // V1.109 C2 T3 — extended to resolve Scene + Beat in the same pass so a
  // single graph click coordinates all three inspectors (selecting a Beat
  // clears both Scene and Chapter selections, etc.).
  //
  // V1.109 P2 T2 — depend on `selectionKey` instead of `rfNodes` so the effect
  // re-fires only when the selected node id changes. The body still reads
  // `rfNodes`; that is safe because whenever `selectionKey` changes, `rfNodes`
  // has already updated in the same render, so the closure is current.
  useEffect(() => {
    const someSelected = rfNodes.some((n) => n.selected);

    const chapterId = selectedChapterIdFromNodes(rfNodes);
    if (chapterId !== null) {
      setSelectedChapterId(chapterId);
    } else if (someSelected) {
      setSelectedChapterId(null);
    }

    const sceneId = selectedSceneIdFromNodes(rfNodes);
    if (sceneId !== null) {
      setSelectedSceneId(sceneId);
    } else if (someSelected) {
      setSelectedSceneId(null);
    }

    const beatId = selectedBeatIdFromNodes(rfNodes);
    if (beatId !== null) {
      setSelectedBeatId(beatId);
    } else if (someSelected) {
      setSelectedBeatId(null);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectionKey]);

  return {
    rfNodes,
    rfEdges,
    onNodesChange,
    selectedChapterId,
    setSelectedChapterId,
    selectedSceneId,
    selectedBeatId,
    projection,
  };
}
