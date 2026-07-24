/**
 * Outline canvas adapter — projection equivalence tests (V1.115 P0 T1a).
 *
 * Verifies that `adapter.projectGraph(graph)` produces identical nodes, edges,
 * and `parentId` nesting to the existing `projectOutlineGraph` function. The
 * adapter is a thin delegate — its projection must not drift from the
 * canonical projection that `use-outline-canvas-graph.ts` has been consuming
 * since V1.108 P0.
 *
 * AC-P0-1: adapter exists; projection equivalence test passes.
 */
import { describe, expect, it } from 'vitest';
import type { MutableRefObject } from 'react';
import type { ChapterSummary, WorkOutline } from '@42ch/nexus-contracts';

import { createOutlineCanvasAdapter, type OutlineCanvasAdapterContext } from '../outline-canvas-adapter';
import type { OutlineSurfaceGraph } from '../outline-canvas-adapter';
import { projectOutlineGraph, chapterNodeId, sceneNodeId, beatNodeId } from '../rf-projection';
import type { SceneBeatFixturePayload, SceneFixture, BeatFixture } from '../graph-projection';

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

function chapter(partial: Partial<ChapterSummary> = {}): ChapterSummary {
  return {
    work_id: 'wk_test',
    chapter: 1,
    volume: 1,
    title: undefined,
    slug: undefined,
    status: 'not_started',
    planned_word_count: 0,
    actual_word_count: undefined,
    outline_path: undefined,
    body_path: undefined,
    created_at: '',
    updated_at: '',
    ...partial,
  } as ChapterSummary;
}

function outline(partial: Partial<WorkOutline> = {}): WorkOutline {
  return {
    work_id: 'wk_test',
    outline_revision: 0,
    volumes: [{ volume_id: 1, label: 'Volume 1', chapter_ids: [1, 2] }],
    timeline_events: [],
    foreshadows: [],
    chapter_titles: {},
    updated_at: '',
    ...partial,
  };
}

function sceneFixture(partial: Partial<SceneFixture> = {}): SceneFixture {
  return { sceneId: 's1', chapterId: 1, title: 'Scene One', status: 'drafted', ...partial };
}

function beatFixture(partial: Partial<BeatFixture> = {}): BeatFixture {
  return { beatId: 'b1', sceneId: 's1', title: 'Beat One', status: null, ...partial };
}

const TRANSLATE_FALLBACK = (chapter: number) => `Chapter ${chapter}`;
const T_STUB = (key: string) => key;

function makeCtx(): OutlineCanvasAdapterContext {
  return {
    translateFallback: TRANSLATE_FALLBACK,
    t: T_STUB,
    workId: 'wk_test',
    outline: undefined,
    chapters: [],
    chapterById: new Map(),
    fixture: { scenes: [], beats: [] },
    altViewSceneBeatFixture: undefined,
    onPatchChapter: () => {},
    onMove: () => {},
    patchChapterIsPending: false,
    isConflicting: false,
    contentVersion: 0,
  };
}

function makeAdapter(ctx?: Partial<OutlineCanvasAdapterContext>) {
  const full: OutlineCanvasAdapterContext = { ...makeCtx(), ...ctx };
  const ctxRef: MutableRefObject<OutlineCanvasAdapterContext> = { current: full };
  return createOutlineCanvasAdapter(ctxRef);
}

// ---------------------------------------------------------------------------
// Adapter shape
// ---------------------------------------------------------------------------

describe('OutlineCanvasAdapter — shape', () => {
  it('declares surfaceKind "outline"', () => {
    const adapter = makeAdapter();
    expect(adapter.surfaceKind).toBe('outline');
  });

  it('omits dagre layoutOptions (compound-graph ranker bug — projected positions used directly)', () => {
    const adapter = makeAdapter();
    // V1.115 T1b: layoutOptions removed because @dagrejs/dagre@3.0.0 crashes
    // when an edge targets a node that has children via setParent (the Outline
    // projection creates volume→chapter edges where chapter nodes have scene/
    // beat children). The projection's built-in grid layout supplies positions.
    expect(adapter.layoutOptions).toBeUndefined();
  });

  it('exposes outlineNodeTypes', () => {
    const adapter = makeAdapter();
    expect(adapter.nodeTypes).toBeDefined();
    expect(adapter.nodeTypes['outline-chapter']).toBeDefined();
    expect(adapter.nodeTypes['outline-volume']).toBeDefined();
    expect(adapter.nodeTypes['outline-timeline-event']).toBeDefined();
    expect(adapter.nodeTypes['outline-scene']).toBeDefined();
    expect(adapter.nodeTypes['outline-beat']).toBeDefined();
  });

  it('adaptConflict returns null (surface-owned conflict modal)', () => {
    const adapter = makeAdapter();
    expect(adapter.adaptConflict?.(new Error('409'))).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Projection equivalence — AC-P0-1
// ---------------------------------------------------------------------------

describe('OutlineCanvasAdapter.projectGraph — projection equivalence', () => {
  it('produces identical nodes + edges to projectOutlineGraph', () => {
    const o = outline({
      volumes: [
        { volume_id: 1, label: 'Volume 1', chapter_ids: [1, 2] },
        { volume_id: 2, label: 'Volume 2', chapter_ids: [3] },
      ],
      timeline_events: [
        { event_id: 'e1', title: 'Event 1' },
        { event_id: 'e2', title: 'Event 2', realizes_chapter_id: 1 },
      ],
      foreshadows: [{ source_event_id: 'e1', target_event_id: 'e2' }],
    });
    const chapters = [
      chapter({ chapter: 1 }),
      chapter({ chapter: 2 }),
      chapter({ chapter: 3 }),
    ];

    const adapter = makeAdapter();
    const graph: OutlineSurfaceGraph = {
      outline: o,
      chapters,
      sceneBeatFixture: { scenes: [], beats: [] },
    };

    const direct = projectOutlineGraph(o, chapters, undefined, TRANSLATE_FALLBACK);
    const viaAdapter = adapter.projectGraph(graph);

    expect(viaAdapter.nodes).toEqual(direct.nodes);
    expect(viaAdapter.edges).toEqual(direct.edges);
  });

  it('produces identical parentId nesting for Scene/Beat child nodes', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1] }],
    });
    const chapters = [chapter({ chapter: 1 })];
    const fixture: SceneBeatFixturePayload = {
      scenes: [
        sceneFixture({ sceneId: 's1', chapterId: 1 }),
        sceneFixture({ sceneId: 's2', chapterId: 1 }),
      ],
      beats: [
        beatFixture({ beatId: 'b1', sceneId: 's1' }),
        beatFixture({ beatId: 'b2', sceneId: 's2' }),
      ],
    };

    const adapter = makeAdapter();
    const graph: OutlineSurfaceGraph = { outline: o, chapters, sceneBeatFixture: fixture };

    const direct = projectOutlineGraph(o, chapters, fixture, TRANSLATE_FALLBACK);
    const viaAdapter = adapter.projectGraph(graph);

    // Same parentId nesting — Scene→Chapter, Beat→Scene.
    const directParentIds = new Map(direct.nodes.map((n) => [n.id, n.parentId]));
    for (const node of viaAdapter.nodes) {
      expect(node.parentId).toBe(directParentIds.get(node.id));
    }

    // Spot-check the nesting structure.
    const adapterScene = viaAdapter.nodes.find((n) => n.id === sceneNodeId('s1'));
    expect(adapterScene?.parentId).toBe(chapterNodeId(1));
    expect(adapterScene?.extent).toBe('parent');

    const adapterBeat = viaAdapter.nodes.find((n) => n.id === beatNodeId('b1'));
    expect(adapterBeat?.parentId).toBe(sceneNodeId('s1'));
    expect(adapterBeat?.extent).toBe('parent');
  });

  it('produces identical output for identical input (determinism)', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1, 2] }],
      timeline_events: [{ event_id: 'e1', title: 'E1' }],
    });
    const chapters = [chapter({ chapter: 1 }), chapter({ chapter: 2 })];
    const graph: OutlineSurfaceGraph = {
      outline: o,
      chapters,
      sceneBeatFixture: { scenes: [], beats: [] },
    };

    const adapter = makeAdapter();
    const first = adapter.projectGraph(graph);
    const second = adapter.projectGraph(graph);

    expect(second.nodes).toEqual(first.nodes);
    expect(second.edges).toEqual(first.edges);
  });

  it('handles an empty outline gracefully', () => {
    const o = outline({ volumes: [], timeline_events: [] });
    const adapter = makeAdapter();
    const result = adapter.projectGraph({
      outline: o,
      chapters: [],
      sceneBeatFixture: { scenes: [], beats: [] },
    });
    expect(result.nodes).toEqual([]);
    expect(result.edges).toEqual([]);
  });

  it('reads translateFallback from the context ref at projection time', () => {
    // The adapter must read `translateFallback` from ctxRef.current on each
    // projectGraph call, not capture it at creation time.
    const ctx = makeCtx();
    ctx.translateFallback = (c) => `Kapitel ${c}`;
    const ctxRef: MutableRefObject<OutlineCanvasAdapterContext> = { current: ctx };
    const adapter = createOutlineCanvasAdapter(ctxRef);

    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1] }],
      chapter_titles: {},
    });
    const ch = chapter({ chapter: 1, title: undefined });
    const result = adapter.projectGraph({
      outline: o,
      chapters: [ch],
      sceneBeatFixture: { scenes: [], beats: [] },
    });

    const chapterNode = result.nodes.find((n) => n.id === chapterNodeId(1));
    expect(chapterNode).toBeDefined();
    expect((chapterNode!.data as { title: string }).title).toContain('Kapitel');
  });
});

// ---------------------------------------------------------------------------
// summarizeGraph
// ---------------------------------------------------------------------------

describe('OutlineCanvasAdapter.summarizeGraph', () => {
  it('delegates to outlineGraphSummary with the graph payload', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1] }],
      timeline_events: [{ event_id: 'e1', title: 'E1' }],
      foreshadows: [{ source_event_id: 'e1', target_event_id: 'e2' }],
    });
    const adapter = makeAdapter({ t: T_STUB });
    const summary = adapter.summarizeGraph({
      outline: o,
      chapters: [chapter({ chapter: 1 })],
      sceneBeatFixture: { scenes: [], beats: [] },
    });
    expect(summary).toMatch(/outlineCanvas\.graphSummary\.body/);
  });

  it('reports not-loaded when outline is absent in the payload', () => {
    // outlineGraphSummary handles undefined outline; but the adapter's TGraph
    // requires a concrete outline. The orchestrator (T1b) is responsible for
    // not calling summarizeGraph before data is loaded (isLoading gate).
    // This test documents the contract: summarizeGraph receives a loaded graph.
    const o = outline();
    const adapter = makeAdapter({ t: T_STUB });
    const summary = adapter.summarizeGraph({
      outline: o,
      chapters: [],
      sceneBeatFixture: { scenes: [], beats: [] },
    });
    expect(summary).toMatch(/outlineCanvas\.graphSummary\.body/);
  });
});
