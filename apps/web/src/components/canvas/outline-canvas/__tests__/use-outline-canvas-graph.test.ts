/**
 * `useOutlineCanvasGraph` — regression coverage for the V1.108 → V1.109 extract
 * (R-V1108P0QC1-S001).
 *
 * The hook is a pure refactor of behavior that previously lived inline in
 * `outline-canvas.tsx`: the projection memo, the `rfNodes`/`rfEdges` RF state,
 * the projection→RF position-merge sync effect, the graph-click → inspector
 * selection-sync effect, and the `selectedChapterId` state + setter. These
 * tests pin each behavior so the extract cannot silently change it.
 *
 * CanvasShell / React Flow are not mounted — the hook is exercised directly via
 * `renderHook`, and RF node selection is simulated by mutating `rfNodes`
 * through the exposed `onNodesChange` applier (which delegates to
 * `applyNodeChanges`).
 *
 * Fixture note: args are passed via `initialProps` (stable object refs across
 * re-renders) so the hook's `useMemo`/`useEffect` deps do not change on every
 * render. In the real orchestrator, `outline.data` (React Query) and `chapters`
 * (`useMemo(flattenPages)`) are similarly stable across renders that do not
 * change the underlying data.
 */
import { describe, expect, it } from 'vitest';
import { renderHook, act } from '@testing-library/react';

import type { ChapterSummary, WorkOutline } from '@42ch/nexus-contracts';
import type { NodeChange } from '@xyflow/react';

import { useOutlineCanvasGraph } from '../use-outline-canvas-graph';
import type { UseOutlineCanvasGraphResult } from '../use-outline-canvas-graph';
import { chapterNodeId, sceneNodeId, beatNodeId, volumeNodeId } from '../rf-projection';
import type { SceneBeatFixturePayload } from '../graph-projection';

// ---------------------------------------------------------------------------
// Fixtures (module-level so refs are stable across hook re-renders)
// ---------------------------------------------------------------------------

function makeChapter(partial: Partial<ChapterSummary> = {}): ChapterSummary {
  return {
    work_id: 'wk_test',
    chapter: 1,
    volume: 1,
    title: undefined,
    slug: undefined,
    status: 'draft',
    planned_word_count: 1000,
    actual_word_count: 500,
    outline_path: undefined,
    body_path: undefined,
    created_at: '',
    updated_at: '',
    ...partial,
  } as ChapterSummary;
}

function makeOutline(partial: Partial<WorkOutline> = {}): WorkOutline {
  return {
    work_id: 'wk_test',
    outline_revision: 1,
    volumes: [{ volume_id: 1, label: 'Volume 1', chapter_ids: [1] }],
    timeline_events: [],
    foreshadows: [],
    chapter_titles: {},
    updated_at: '',
    ...partial,
  };
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

type HookApi = { current: UseOutlineCanvasGraphResult };

/** Apply a selection change through the hook's `onNodesChange`, mirroring RF. */
function selectNode(hook: HookApi, nodeId: string) {
  const target = hook.current.rfNodes.find((n) => n.id === nodeId);
  expect(target).toBeDefined();
  // RF emits a `select` change for the clicked node (true) and `select:false`
  // for previously selected nodes. Simulate the simplest single-select path.
  const changes: NodeChange[] = hook.current.rfNodes.map((n) => ({
    id: n.id,
    type: 'select',
    selected: n.id === nodeId,
  }));
  act(() => {
    hook.current.onNodesChange(changes);
  });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('useOutlineCanvasGraph — projection', () => {
  it('returns a null projection when outline is undefined', () => {
    const { result } = renderHook(useOutlineCanvasGraph, {
      initialProps: { outline: undefined, chapters: [] },
    });
    expect(result.current.projection).toBeNull();
    expect(result.current.rfNodes).toEqual([]);
    expect(result.current.rfEdges).toEqual([]);
  });

  it('projects outline + chapters into RF nodes + edges', () => {
    const ch = makeChapter();
    const o = makeOutline();
    const { result } = renderHook(useOutlineCanvasGraph, {
      initialProps: { outline: o, chapters: [ch] },
    });
    expect(result.current.projection).not.toBeNull();
    const ids = result.current.rfNodes.map((n) => n.id);
    expect(ids).toContain(volumeNodeId(1));
    expect(ids).toContain(chapterNodeId(1));
    // contains edge volume:1 → chapter:1
    expect(result.current.rfEdges.some(
      (e) => e.source === volumeNodeId(1) && e.target === chapterNodeId(1),
    )).toBe(true);
  });
});

describe('useOutlineCanvasGraph — position-merge sync (V1.108 PR-review fix)', () => {
  it('preserves user-moved position + selection across a projection rebuild', () => {
    const ch1 = makeChapter({ chapter: 1 });
    const o1 = makeOutline();
    const hook = renderHook(useOutlineCanvasGraph, {
      initialProps: { outline: o1, chapters: [ch1] },
    });

    // Simulate the author dragging the chapter node + selecting it.
    act(() => {
      const changes: NodeChange[] = [
        { id: chapterNodeId(1), type: 'position', position: { x: 999, y: 999 } },
        { id: chapterNodeId(1), type: 'select', selected: true },
      ];
      hook.result.current.onNodesChange(changes);
    });

    const moved = hook.result.current.rfNodes.find((n) => n.id === chapterNodeId(1))!;
    expect(moved.position).toEqual({ x: 999, y: 999 });
    expect(moved.selected).toBe(true);

    // Projection rebuilds because a new chapter page loaded (new array ref).
    // The new chapter is appended, but the existing chapter's drag + selection
    // must survive — a bare `setRfNodes(projection.nodes)` would wipe them.
    const ch2 = makeChapter({ chapter: 2 });
    const o2 = makeOutline({
      volumes: [{ volume_id: 1, label: 'Volume 1', chapter_ids: [1, 2] }],
    });
    hook.rerender({ outline: o2, chapters: [ch1, ch2] });

    const preserved = hook.result.current.rfNodes.find((n) => n.id === chapterNodeId(1))!;
    expect(preserved.position).toEqual({ x: 999, y: 999 });
    expect(preserved.selected).toBe(true);
    // The new chapter is present with its projected position.
    const appended = hook.result.current.rfNodes.find((n) => n.id === chapterNodeId(2))!;
    expect(appended).toBeDefined();
  });

  it('uses projected positions on the first sync (no previous state to merge)', () => {
    const { result } = renderHook(useOutlineCanvasGraph, {
      initialProps: { outline: makeOutline(), chapters: [makeChapter()] },
    });
    const node = result.current.rfNodes.find((n) => n.id === volumeNodeId(1))!;
    expect(node.position).toBeDefined();
    expect(typeof node.position.x).toBe('number');
  });
});

describe('useOutlineCanvasGraph — selection sync (FB-C1-003)', () => {
  it('seeds selectedChapterId from initialSelectedChapterId', () => {
    const { result } = renderHook(useOutlineCanvasGraph, {
      initialProps: {
        outline: makeOutline(),
        chapters: [makeChapter()],
        initialSelectedChapterId: 1,
      },
    });
    expect(result.current.selectedChapterId).toBe(1);
  });

  it('drives selectedChapterId when a chapter node is selected', () => {
    const hook = renderHook(useOutlineCanvasGraph, {
      initialProps: { outline: makeOutline(), chapters: [makeChapter({ chapter: 1 })] },
    });
    expect(hook.result.current.selectedChapterId).toBeNull();
    selectNode(hook.result, chapterNodeId(1));
    expect(hook.result.current.selectedChapterId).toBe(1);
  });

  it('clears selectedChapterId when a non-chapter node (volume) is selected', () => {
    const hook = renderHook(useOutlineCanvasGraph, {
      initialProps: {
        outline: makeOutline(),
        chapters: [makeChapter({ chapter: 1 })],
        initialSelectedChapterId: 1,
      },
    });
    expect(hook.result.current.selectedChapterId).toBe(1);
    // Selecting a volume node resolves to no chapter → clear (V1.108 PR fix).
    selectNode(hook.result, volumeNodeId(1));
    expect(hook.result.current.selectedChapterId).toBeNull();
  });

  it('leaves selectedChapterId intact when nothing is selected', () => {
    const hook = renderHook(useOutlineCanvasGraph, {
      initialProps: {
        outline: makeOutline(),
        chapters: [makeChapter({ chapter: 1 })],
        initialSelectedChapterId: 1,
      },
    });
    expect(hook.result.current.selectedChapterId).toBe(1);
    // Deselect everything (e.g. click empty canvas) → keep current selection.
    act(() => {
      const changes: NodeChange[] = hook.result.current.rfNodes.map((n) => ({
        id: n.id,
        type: 'select',
        selected: false,
      }));
      hook.result.current.onNodesChange(changes);
    });
    expect(hook.result.current.selectedChapterId).toBe(1);
  });
});

describe('useOutlineCanvasGraph — setter passthrough', () => {
  it('exposes setSelectedChapterId that updates the state', () => {
    const { result } = renderHook(useOutlineCanvasGraph, {
      initialProps: { outline: makeOutline(), chapters: [makeChapter()] },
    });
    expect(result.current.selectedChapterId).toBeNull();
    act(() => {
      result.current.setSelectedChapterId(7);
    });
    expect(result.current.selectedChapterId).toBe(7);
  });
});

// ---------------------------------------------------------------------------
// V1.109 C2 T3 — Scene/Beat selection sync (FB-C2-002 graph click -> inspector)
// ---------------------------------------------------------------------------

const sceneBeatFixture: SceneBeatFixturePayload = {
  scenes: [{ sceneId: 's1', chapterId: 1, title: 'The Arrival', status: 'drafted' }],
  beats: [{ beatId: 'b1', sceneId: 's1', title: 'Turn', status: null }],
};

describe('useOutlineCanvasGraph — Scene/Beat selection sync (FB-C2-002)', () => {
  it('exposes selectedSceneId / selectedBeatId (null by default)', () => {
    const { result } = renderHook(useOutlineCanvasGraph, {
      initialProps: {
        outline: makeOutline(),
        chapters: [makeChapter()],
        sceneBeatFixture,
      },
    });
    expect(result.current.selectedSceneId).toBeNull();
    expect(result.current.selectedBeatId).toBeNull();
  });

  it('drives selectedSceneId when a Scene node is selected', () => {
    const hook = renderHook(useOutlineCanvasGraph, {
      initialProps: {
        outline: makeOutline(),
        chapters: [makeChapter({ chapter: 1 })],
        sceneBeatFixture,
      },
    });
    expect(hook.result.current.selectedSceneId).toBeNull();
    selectNode(hook.result, sceneNodeId('s1'));
    expect(hook.result.current.selectedSceneId).toBe('s1');
  });

  it('drives selectedBeatId when a Beat node is selected', () => {
    const hook = renderHook(useOutlineCanvasGraph, {
      initialProps: {
        outline: makeOutline(),
        chapters: [makeChapter({ chapter: 1 })],
        sceneBeatFixture,
      },
    });
    expect(hook.result.current.selectedBeatId).toBeNull();
    selectNode(hook.result, beatNodeId('b1'));
    expect(hook.result.current.selectedBeatId).toBe('b1');
  });

  it('clears selectedSceneId and selectedChapterId when a Beat node is selected', () => {
    const hook = renderHook(useOutlineCanvasGraph, {
      initialProps: {
        outline: makeOutline(),
        chapters: [makeChapter({ chapter: 1 })],
        sceneBeatFixture,
        initialSelectedChapterId: 1,
      },
    });
    // Preselect chapter 1, then select the beat — chapter + scene must clear.
    expect(hook.result.current.selectedChapterId).toBe(1);
    selectNode(hook.result, beatNodeId('b1'));
    expect(hook.result.current.selectedBeatId).toBe('b1');
    expect(hook.result.current.selectedSceneId).toBeNull();
    expect(hook.result.current.selectedChapterId).toBeNull();
  });

  it('clears selectedBeatId when a Scene node is selected', () => {
    const hook = renderHook(useOutlineCanvasGraph, {
      initialProps: {
        outline: makeOutline(),
        chapters: [makeChapter({ chapter: 1 })],
        sceneBeatFixture,
      },
    });
    selectNode(hook.result, beatNodeId('b1'));
    expect(hook.result.current.selectedBeatId).toBe('b1');
    selectNode(hook.result, sceneNodeId('s1'));
    expect(hook.result.current.selectedSceneId).toBe('s1');
    expect(hook.result.current.selectedBeatId).toBeNull();
  });

  it('clears Scene/Beat selection when a Chapter node is selected', () => {
    const hook = renderHook(useOutlineCanvasGraph, {
      initialProps: {
        outline: makeOutline(),
        chapters: [makeChapter({ chapter: 1 })],
        sceneBeatFixture,
      },
    });
    selectNode(hook.result, sceneNodeId('s1'));
    expect(hook.result.current.selectedSceneId).toBe('s1');
    selectNode(hook.result, chapterNodeId(1));
    expect(hook.result.current.selectedChapterId).toBe(1);
    expect(hook.result.current.selectedSceneId).toBeNull();
    expect(hook.result.current.selectedBeatId).toBeNull();
  });

  it('leaves Scene/Beat selection intact when nothing is selected', () => {
    const hook = renderHook(useOutlineCanvasGraph, {
      initialProps: {
        outline: makeOutline(),
        chapters: [makeChapter({ chapter: 1 })],
        sceneBeatFixture,
      },
    });
    selectNode(hook.result, sceneNodeId('s1'));
    expect(hook.result.current.selectedSceneId).toBe('s1');
    // Deselect everything (click empty canvas) -> keep current selection.
    act(() => {
      const changes: NodeChange[] = hook.result.current.rfNodes.map((n) => ({
        id: n.id,
        type: 'select',
        selected: false,
      }));
      hook.result.current.onNodesChange(changes);
    });
    expect(hook.result.current.selectedSceneId).toBe('s1');
  });
});

// ---------------------------------------------------------------------------
// V1.109 P2 T2 — selection-sync overfire guard (FB-GS-001)
//
// Before the fix the selection-sync `useEffect` depended on `[rfNodes]`, and
// RF emits a new `rfNodes` array ref on EVERY node interaction (position drags
// included, via `applyNodeChanges`). The effect therefore re-ran on every
// drag, re-resolving the selected entity and re-calling the inspector setters
// — a latent perf trap as graphs grow (R-V1108P0QC3-W001).
//
// These tests pin the guard: a position-only drag must NOT re-fire the
// selection-sync side effect, while a real selection change still must.
// ---------------------------------------------------------------------------

describe('useOutlineCanvasGraph — selection overfire guard (FB-GS-001)', () => {
  it('does NOT re-fire selection sync on a position-only drag', () => {
    const hook = renderHook(useOutlineCanvasGraph, {
      initialProps: { outline: makeOutline(), chapters: [makeChapter({ chapter: 1 })] },
    });

    // 1. Select the chapter node -> effect fires -> selectedChapterId = 1.
    selectNode(hook.result, chapterNodeId(1));
    expect(hook.result.current.selectedChapterId).toBe(1);

    // 2. Diverge the inspector from the graph selection. In production this
    //    happens when a list-view click or `?chapter=N` preselect drives
    //    `selectedChapterId` to a value the graph did not set. The graph
    //    still has chapter:1 selected.
    act(() => {
      hook.result.current.setSelectedChapterId(99);
    });
    expect(hook.result.current.selectedChapterId).toBe(99);

    // 3. Position-only drag of the (still-selected) chapter node. RF produces
    //    a new `rfNodes` array ref, but the selected node id is unchanged, so
    //    the guarded effect must NOT re-run. The inspector must stay at the
    //    externally-set value rather than being reset to the graph-resolved id.
    act(() => {
      const changes: NodeChange[] = [
        { id: chapterNodeId(1), type: 'position', position: { x: 500, y: 500 } },
      ];
      hook.result.current.onNodesChange(changes);
    });

    expect(hook.result.current.selectedChapterId).toBe(99);
    // The position drag itself did land on the node.
    const dragged = hook.result.current.rfNodes.find((n) => n.id === chapterNodeId(1))!;
    expect(dragged.position).toEqual({ x: 500, y: 500 });
  });

  it('does NOT re-fire selection sync on repeated position-only drags', () => {
    const hook = renderHook(useOutlineCanvasGraph, {
      initialProps: { outline: makeOutline(), chapters: [makeChapter({ chapter: 1 })] },
    });
    selectNode(hook.result, chapterNodeId(1));
    act(() => {
      hook.result.current.setSelectedChapterId(7);
    });

    // Several position updates in sequence (RF emits one per drag tick).
    for (const [x, y] of [[10, 10], [20, 20], [30, 30]] as const) {
      act(() => {
        const changes: NodeChange[] = [
          { id: chapterNodeId(1), type: 'position', position: { x, y } },
        ];
        hook.result.current.onNodesChange(changes);
      });
    }

    // Selection id never changed -> inspector must hold the externally-set value.
    expect(hook.result.current.selectedChapterId).toBe(7);
  });

  it('still drives the inspector when the selected node id changes', () => {
    // Regression guard: a genuine selection change must still fire the effect.
    const hook = renderHook(useOutlineCanvasGraph, {
      initialProps: { outline: makeOutline(), chapters: [makeChapter({ chapter: 1 })] },
    });
    selectNode(hook.result, chapterNodeId(1));
    expect(hook.result.current.selectedChapterId).toBe(1);

    // Select a volume node (resolves to no chapter) -> effect fires -> clears.
    selectNode(hook.result, volumeNodeId(1));
    expect(hook.result.current.selectedChapterId).toBeNull();
  });

  it('re-fires selection sync after a drag re-selects a different node', () => {
    // Combined path: drag (no fire) then click-select another node (fires).
    const hook = renderHook(useOutlineCanvasGraph, {
      initialProps: { outline: makeOutline(), chapters: [makeChapter({ chapter: 1 })] },
    });
    selectNode(hook.result, chapterNodeId(1));
    act(() => {
      const changes: NodeChange[] = [
        { id: chapterNodeId(1), type: 'position', position: { x: 800, y: 0 } },
      ];
      hook.result.current.onNodesChange(changes);
    });
    expect(hook.result.current.selectedChapterId).toBe(1);

    // Now select the volume node -> selection id changes -> effect fires.
    selectNode(hook.result, volumeNodeId(1));
    expect(hook.result.current.selectedChapterId).toBeNull();
  });
});
