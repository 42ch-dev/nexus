/**
 * Unit tests for the outline-canvas RF projection (V1.108 P0 T1).
 *
 * Verifies projection purity: deterministic output, no input mutation, correct
 * node/edge counts, stable ids, and layout determinism (same input → same
 * positions). Mirrors the World KB graph-projection test structure.
 */
import { describe, expect, it } from 'vitest';

import type { ChapterSummary, WorkOutline } from '@42ch/nexus-contracts';

import {
  beatNodeId,
  chapterNodeId,
  deriveContainsEdges,
  deriveForeshadowEdges,
  deriveRealizesEdges,
  eventNodeId,
  layoutChapterNodes,
  layoutTimelineEventNodes,
  layoutVolumeNodes,
  outlineGraphSummary,
  projectOutlineGraph,
  sceneNodeId,
  selectedBeatIdFromNodes,
  selectedChapterIdFromNodes,
  selectedSceneIdFromNodes,
  volumeNodeId,
} from '../rf-projection';
import type {
  BeatFixture,
  SceneBeatFixturePayload,
  SceneFixture,
} from '../graph-projection';

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

// ---------------------------------------------------------------------------
// projectOutlineGraph — purity + counts
// ---------------------------------------------------------------------------

describe('projectOutlineGraph', () => {
  it('projects volumes, chapters, and timeline events to nodes', () => {
    const o = outline({
      volumes: [
        { volume_id: 1, label: 'Volume 1', chapter_ids: [1, 2] },
        { volume_id: 2, label: 'Volume 2', chapter_ids: [3] },
      ],
      timeline_events: [
        { event_id: 'e1', title: 'Event 1' },
        { event_id: 'e2', title: 'Event 2', realizes_chapter_id: 1 },
      ],
    });
    const chapters = [
      chapter({ chapter: 1 }),
      chapter({ chapter: 2 }),
      chapter({ chapter: 3 }),
    ];
    const { nodes } = projectOutlineGraph(o, chapters);
    const types = nodes.map((n) => n.type);
    expect(types.filter((t) => t === 'outline-volume')).toHaveLength(2);
    expect(types.filter((t) => t === 'outline-chapter')).toHaveLength(3);
    expect(types.filter((t) => t === 'outline-timeline-event')).toHaveLength(2);
  });

  it('does not mutate the input outline or chapters', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'Volume 1', chapter_ids: [1] }],
      timeline_events: [{ event_id: 'e1', title: 'E1' }],
      foreshadows: [],
    });
    const chapters = [chapter({ chapter: 1 })];
    const oSnapshot = JSON.parse(JSON.stringify(o));
    const cSnapshot = JSON.parse(JSON.stringify(chapters));

    projectOutlineGraph(o, chapters);

    expect(JSON.parse(JSON.stringify(o))).toEqual(oSnapshot);
    expect(JSON.parse(JSON.stringify(chapters))).toEqual(cSnapshot);
  });

  it('does not mutate the input chapters array order (multi-element)', () => {
    // Regression: `chapters.sort()` mutated the caller's array in place.
    // Pass 3+ chapters in non-sorted order with some unassigned so the sort
    // path in layoutChapterNodes is exercised; the input order must survive.
    const o = outline({
      volumes: [{ volume_id: 1, label: 'Volume 1', chapter_ids: [1] }],
    });
    const chapters = [
      chapter({ chapter: 3 }),
      chapter({ chapter: 1 }),
      chapter({ chapter: 2 }),
    ];
    const inputOrder = chapters.map((c) => c.chapter);

    projectOutlineGraph(o, chapters);

    expect(chapters.map((c) => c.chapter)).toEqual(inputOrder);
  });

  it('produces identical output for identical input (determinism)', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1, 2] }],
      timeline_events: [{ event_id: 'e1', title: 'E1' }],
    });
    const chapters = [chapter({ chapter: 1 }), chapter({ chapter: 2 })];

    const first = projectOutlineGraph(o, chapters);
    const second = projectOutlineGraph(o, chapters);

    expect(second).toEqual(first);
  });

  it('produces contains edges from volumes to their chapters', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1, 2] }],
    });
    const { edges } = projectOutlineGraph(o, [chapter({ chapter: 1 }), chapter({ chapter: 2 })]);
    const contains = edges.filter((e) => (e.data as { relation: string }).relation === 'contains');
    expect(contains).toHaveLength(2);
    expect(contains[0].source).toBe(volumeNodeId(1));
  });

  it('produces realizes edges from chapters to timeline events', () => {
    const o = outline({
      volumes: [],
      timeline_events: [
        { event_id: 'e1', title: 'E1', realizes_chapter_id: 5 },
      ],
    });
    // I-QC1-002 — the chapter node must exist for the edge to survive the
    // dangling-edge filter. Pass chapter 5 so the realizes edge is not dropped.
    const { edges } = projectOutlineGraph(o, [chapter({ chapter: 5 })]);
    const realizes = edges.filter((e) => (e.data as { relation: string }).relation === 'realizes_event');
    expect(realizes).toHaveLength(1);
    expect(realizes[0].source).toBe(chapterNodeId(5));
    expect(realizes[0].target).toBe(eventNodeId('e1'));
  });

  it('produces foreshadow edges between events', () => {
    const o = outline({
      volumes: [],
      timeline_events: [
        { event_id: 'e1', title: 'Setup' },
        { event_id: 'e2', title: 'Payoff' },
      ],
      foreshadows: [{ source_event_id: 'e1', target_event_id: 'e2' }],
    });
    const { edges } = projectOutlineGraph(o, []);
    const foreshadow = edges.filter((e) => (e.data as { relation: string }).relation === 'foreshadows');
    expect(foreshadow).toHaveLength(1);
    expect(foreshadow[0].source).toBe(eventNodeId('e1'));
    expect(foreshadow[0].target).toBe(eventNodeId('e2'));
  });

  it('produces zero foreshadow edges when no links exist', () => {
    const o = outline({
      timeline_events: [{ event_id: 'e1', title: 'E1' }],
      foreshadows: [],
    });
    const { edges } = projectOutlineGraph(o, []);
    expect(edges.filter((e) => (e.data as { relation: string }).relation === 'foreshadows')).toHaveLength(0);
  });

  it('handles an empty outline gracefully', () => {
    const { nodes, edges } = projectOutlineGraph(outline({ volumes: [], timeline_events: [] }), []);
    expect(nodes).toEqual([]);
    expect(edges).toEqual([]);
  });

  // I-QC1-002 — when chapter data is incomplete (paginated, not all pages
  // loaded), edges that would point at non-existent chapter nodes must be
  // filtered out so the graph never shows dangling edges.
  it('filters out dangling contains/realizes edges when chapter nodes are missing', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1, 2, 3] }],
      timeline_events: [
        { event_id: 'e1', title: 'E1', realizes_chapter_id: 99 },
      ],
    });
    // Only chapter 1 is loaded — chapters 2, 3, and 99 are absent.
    const chapters = [chapter({ chapter: 1 })];
    const { nodes, edges } = projectOutlineGraph(o, chapters);

    const nodeIds = new Set(nodes.map((n) => n.id));
    // Every edge must have both source and target in the node set.
    for (const edge of edges) {
      expect(nodeIds.has(edge.source)).toBe(true);
      expect(nodeIds.has(edge.target)).toBe(true);
    }
    // The contains edge to chapter 1 should survive; edges to 2/3 should not.
    const contains = edges.filter(
      (e) => (e.data as { relation: string }).relation === 'contains',
    );
    expect(contains).toHaveLength(1);
    // The realizes edge to chapter 99 should be filtered (no chapter:99 node).
    const realizes = edges.filter(
      (e) => (e.data as { relation: string }).relation === 'realizes_event',
    );
    expect(realizes).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// Stable node ids
// ---------------------------------------------------------------------------

describe('node id helpers', () => {
  it('produces prefixed ids that never collide', () => {
    expect(volumeNodeId(1)).toBe('volume:1');
    expect(chapterNodeId(1)).toBe('chapter:1');
    expect(eventNodeId('e1')).toBe('event:e1');
  });
});

// ---------------------------------------------------------------------------
// Layout determinism
// ---------------------------------------------------------------------------

describe('layoutVolumeNodes', () => {
  it('stacks volumes vertically at the same x coordinate', () => {
    const o = outline({
      volumes: [
        { volume_id: 1, label: 'V1', chapter_ids: [] },
        { volume_id: 2, label: 'V2', chapter_ids: [] },
      ],
    });
    const nodes = layoutVolumeNodes(o);
    expect(nodes).toHaveLength(2);
    expect(nodes[0].position.x).toBe(nodes[1].position.x);
    expect(nodes[1].position.y).toBeGreaterThan(nodes[0].position.y);
  });

  it('produces the same positions for the same input', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1] }],
    });
    const a = layoutVolumeNodes(o);
    const b = layoutVolumeNodes(o);
    expect(a[0].position).toEqual(b[0].position);
  });
});

describe('layoutChapterNodes', () => {
  it('places assigned chapters in volume-declaration order', () => {
    const o = outline({
      volumes: [
        { volume_id: 1, label: 'V1', chapter_ids: [3, 1] },
        { volume_id: 2, label: 'V2', chapter_ids: [2] },
      ],
    });
    const chapters = [chapter({ chapter: 1 }), chapter({ chapter: 2 }), chapter({ chapter: 3 })];
    const nodes = layoutChapterNodes(o, chapters);
    // Volume 1 declares [3, 1] so those come first; then volume 2's [2].
    expect(nodes.map((n) => (n.data as { chapterId: number }).chapterId)).toEqual([3, 1, 2]);
  });

  it('appends unassigned chapters after assigned ones', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1] }],
    });
    const chapters = [
      chapter({ chapter: 1 }),
      chapter({ chapter: 5 }),
      chapter({ chapter: 3 }),
    ];
    const nodes = layoutChapterNodes(o, chapters);
    const ids = nodes.map((n) => (n.data as { chapterId: number }).chapterId);
    expect(ids).toEqual([1, 3, 5]);
  });

  it('positions all chapters in the same column', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1, 2] }],
    });
    const nodes = layoutChapterNodes(o, [chapter({ chapter: 1 }), chapter({ chapter: 2 })]);
    expect(nodes[0].position.x).toBe(nodes[1].position.x);
  });
});

describe('layoutTimelineEventNodes', () => {
  it('places events in declaration order', () => {
    const o = outline({
      timeline_events: [
        { event_id: 'e1', title: 'First' },
        { event_id: 'e2', title: 'Second' },
      ],
    });
    const nodes = layoutTimelineEventNodes(o);
    expect(nodes.map((n) => (n.data as { eventId: string }).eventId)).toEqual(['e1', 'e2']);
  });

  it('positions all events in the same column (timeline lane)', () => {
    const o = outline({
      timeline_events: [
        { event_id: 'e1', title: 'A' },
        { event_id: 'e2', title: 'B' },
      ],
    });
    const nodes = layoutTimelineEventNodes(o);
    expect(nodes[0].position.x).toBe(nodes[1].position.x);
    expect(nodes[0].position.x).not.toBe(40); // Not the volume lane x
  });
});

// ---------------------------------------------------------------------------
// Edge derivation
// ---------------------------------------------------------------------------

describe('deriveContainsEdges', () => {
  it('creates one edge per chapter_id in every volume', () => {
    const o = outline({
      volumes: [
        { volume_id: 1, label: 'V1', chapter_ids: [1, 2, 3] },
        { volume_id: 2, label: 'V2', chapter_ids: [4] },
      ],
    });
    expect(deriveContainsEdges(o)).toHaveLength(4);
  });
});

describe('deriveRealizesEdges', () => {
  it('skips events without realizes_chapter_id', () => {
    const o = outline({
      timeline_events: [
        { event_id: 'e1', title: 'No chapter' },
        { event_id: 'e2', title: 'Linked', realizes_chapter_id: 1 },
      ],
    });
    expect(deriveRealizesEdges(o)).toHaveLength(1);
  });
});

describe('deriveForeshadowEdges', () => {
  it('produces non-selectable focusable edges', () => {
    const o = outline({
      foreshadows: [{ source_event_id: 'e1', target_event_id: 'e2' }],
    });
    const [edge] = deriveForeshadowEdges(o);
    expect(edge.selectable).toBe(false);
    expect(edge.focusable).toBe(true);
  });

  it('consumes the canvas-outline-foreshadow-edge token for stroke (FB-C1-006)', () => {
    const o = outline({
      foreshadows: [{ source_event_id: 'e1', target_event_id: 'e2' }],
    });
    const [edge] = deriveForeshadowEdges(o);
    expect(edge.style?.stroke).toBe('var(--color-canvas-outline-foreshadow-edge)');
  });
});

// ---------------------------------------------------------------------------
// SR summary
// ---------------------------------------------------------------------------

describe('outlineGraphSummary', () => {
  it('describes volumes, chapters, events, and foreshadow links', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1] }],
      timeline_events: [{ event_id: 'e1', title: 'E1' }],
      foreshadows: [{ source_event_id: 'e1', target_event_id: 'e2' }],
    });
    expect(outlineGraphSummary(o, 1)).toMatch(/1 volume, 1 chapter, 1 timeline event, 1 foreshadow link/);
  });

  it('uses plural forms correctly', () => {
    const o = outline({
      volumes: [
        { volume_id: 1, label: 'V1', chapter_ids: [1] },
        { volume_id: 2, label: 'V2', chapter_ids: [] },
      ],
      timeline_events: [
        { event_id: 'e1', title: 'E1' },
        { event_id: 'e2', title: 'E2' },
      ],
      foreshadows: [],
    });
    expect(outlineGraphSummary(o, 3)).toMatch(/2 volumes, 3 chapters, 2 timeline events, 0 foreshadow links/);
  });

  it('reports not-loaded for undefined outline', () => {
    expect(outlineGraphSummary(undefined, 0)).toMatch(/not loaded/);
  });
});

// ---------------------------------------------------------------------------
// selectedChapterIdFromNodes — graph click → inspector selection (FB-C1-003)
// ---------------------------------------------------------------------------

describe('selectedChapterIdFromNodes', () => {
  it('returns null when no node is selected', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1] }],
    });
    const { nodes } = projectOutlineGraph(o, [chapter({ chapter: 1 })]);
    expect(selectedChapterIdFromNodes(nodes)).toBeNull();
  });

  it('returns the chapterId when an outline-chapter node is selected', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1, 2] }],
    });
    const { nodes } = projectOutlineGraph(o, [chapter({ chapter: 1 }), chapter({ chapter: 2 })]);
    const chapter2 = nodes.find((n) => n.type === 'outline-chapter' && (n.data as { chapterId: number }).chapterId === 2)!;
    chapter2.selected = true;

    expect(selectedChapterIdFromNodes(nodes)).toBe(2);
  });

  it('returns realizesChapterId when an outline-timeline-event node is selected', () => {
    const o = outline({
      timeline_events: [{ event_id: 'e1', title: 'E1', realizes_chapter_id: 7 }],
    });
    const { nodes } = projectOutlineGraph(o, []);
    const eventNode = nodes.find((n) => n.type === 'outline-timeline-event')!;
    eventNode.selected = true;

    expect(selectedChapterIdFromNodes(nodes)).toBe(7);
  });

  it('returns null when an unattached timeline event is selected (realizesChapterId null)', () => {
    const o = outline({
      timeline_events: [{ event_id: 'e1', title: 'Unattached' }],
    });
    const { nodes } = projectOutlineGraph(o, []);
    const eventNode = nodes.find((n) => n.type === 'outline-timeline-event')!;
    eventNode.selected = true;

    expect(selectedChapterIdFromNodes(nodes)).toBeNull();
  });

  it('returns null when a volume node is selected (structural, no chapter)', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1] }],
    });
    const { nodes } = projectOutlineGraph(o, [chapter({ chapter: 1 })]);
    const volNode = nodes.find((n) => n.type === 'outline-volume')!;
    volNode.selected = true;

    expect(selectedChapterIdFromNodes(nodes)).toBeNull();
  });

  it('respects only the selected node (ignores non-selected chapter nodes)', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1, 2] }],
    });
    const { nodes } = projectOutlineGraph(o, [chapter({ chapter: 1 }), chapter({ chapter: 2 })]);
    // Select chapter 1, not chapter 2.
    const ch1 = nodes.find((n) => n.type === 'outline-chapter' && (n.data as { chapterId: number }).chapterId === 1)!;
    ch1.selected = true;

    expect(selectedChapterIdFromNodes(nodes)).toBe(1);
  });
});

// ---------------------------------------------------------------------------
// Scene/Beat projection (V1.109 C2 T2 — fixture-driven child nodes)
// ---------------------------------------------------------------------------

function sceneFixture(partial: Partial<SceneFixture> = {}): SceneFixture {
  return {
    sceneId: 's1',
    chapterId: 1,
    title: 'Scene One',
    status: 'drafted',
    ...partial,
  };
}

function beatFixture(partial: Partial<BeatFixture> = {}): BeatFixture {
  return {
    beatId: 'b1',
    sceneId: 's1',
    title: 'Beat One',
    status: null,
    ...partial,
  };
}

function sceneBeatPayload(
  scenes: SceneFixture[] = [],
  beats: BeatFixture[] = [],
): SceneBeatFixturePayload {
  return { scenes, beats };
}

describe('projectOutlineGraph — Scene/Beat child nodes (fixture-driven)', () => {
  it('emits zero scene/beat nodes when no fixture payload is passed (honest empty chrome)', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1] }],
    });
    const { nodes } = projectOutlineGraph(o, [chapter({ chapter: 1 })]);
    expect(nodes.filter((n) => n.type === 'outline-scene')).toHaveLength(0);
    expect(nodes.filter((n) => n.type === 'outline-beat')).toHaveLength(0);
  });

  it('emits zero scene/beat nodes when fixture payload is empty', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1] }],
    });
    const { nodes } = projectOutlineGraph(
      o,
      [chapter({ chapter: 1 })],
      sceneBeatPayload(),
    );
    expect(nodes.filter((n) => n.type === 'outline-scene')).toHaveLength(0);
    expect(nodes.filter((n) => n.type === 'outline-beat')).toHaveLength(0);
  });

  it('emits a Scene node as a child of its owning Chapter with parentId + extent parent', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1] }],
    });
    const fixture = sceneBeatPayload([
      sceneFixture({ sceneId: 's1', chapterId: 1, title: 'The Arrival', status: 'drafted' }),
    ]);
    const { nodes } = projectOutlineGraph(o, [chapter({ chapter: 1 })], fixture);

    const sceneNodes = nodes.filter((n) => n.type === 'outline-scene');
    expect(sceneNodes).toHaveLength(1);
    const scene = sceneNodes[0];
    expect(scene.id).toBe(sceneNodeId('s1'));
    expect(scene.parentId).toBe(chapterNodeId(1));
    expect(scene.extent).toBe('parent');
    const data = scene.data as { workId: string; sceneId: string; chapterId: number; title: string | null; status: string | null };
    expect(data.workId).toBe('wk_test');
    expect(data.sceneId).toBe('s1');
    expect(data.chapterId).toBe(1);
    expect(data.title).toBe('The Arrival');
    expect(data.status).toBe('drafted');
  });

  it('emits a Beat node as a child of its owning Scene with parentId + extent parent (Scene→Beat nesting)', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1] }],
    });
    const fixture = sceneBeatPayload(
      [sceneFixture({ sceneId: 's1', chapterId: 1 })],
      [beatFixture({ beatId: 'b1', sceneId: 's1', title: 'Turn: the call' })],
    );
    const { nodes } = projectOutlineGraph(o, [chapter({ chapter: 1 })], fixture);

    const beatNodes = nodes.filter((n) => n.type === 'outline-beat');
    expect(beatNodes).toHaveLength(1);
    const beat = beatNodes[0];
    expect(beat.id).toBe(beatNodeId('b1'));
    expect(beat.parentId).toBe(sceneNodeId('s1'));
    expect(beat.extent).toBe('parent');
    const data = beat.data as { workId: string; beatId: string; sceneId: string; title: string | null };
    expect(data.workId).toBe('wk_test');
    expect(data.beatId).toBe('b1');
    expect(data.sceneId).toBe('s1');
    expect(data.title).toBe('Turn: the call');
  });

  it('filters out scenes whose parent chapter does not exist (no dangling parentId)', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1] }],
    });
    const fixture = sceneBeatPayload([
      sceneFixture({ sceneId: 's-real', chapterId: 1 }),
      sceneFixture({ sceneId: 's-orphan', chapterId: 99 }),
    ]);
    const { nodes } = projectOutlineGraph(o, [chapter({ chapter: 1 })], fixture);

    const sceneIds = nodes
      .filter((n) => n.type === 'outline-scene')
      .map((n) => n.id);
    expect(sceneIds).toContain(sceneNodeId('s-real'));
    expect(sceneIds).not.toContain(sceneNodeId('s-orphan'));
  });

  it('filters out beats whose parent scene does not exist (no dangling parentId)', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1] }],
    });
    const fixture = sceneBeatPayload(
      [sceneFixture({ sceneId: 's1', chapterId: 1 })],
      [
        beatFixture({ beatId: 'b-real', sceneId: 's1' }),
        beatFixture({ beatId: 'b-orphan', sceneId: 's-missing' }),
      ],
    );
    const { nodes } = projectOutlineGraph(o, [chapter({ chapter: 1 })], fixture);

    const beatIds = nodes
      .filter((n) => n.type === 'outline-beat')
      .map((n) => n.id);
    expect(beatIds).toContain(beatNodeId('b-real'));
    expect(beatIds).not.toContain(beatNodeId('b-orphan'));
  });

  it('stacks multiple scenes under the same chapter in fixture declaration order', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1] }],
    });
    const fixture = sceneBeatPayload([
      sceneFixture({ sceneId: 's1', chapterId: 1 }),
      sceneFixture({ sceneId: 's2', chapterId: 1 }),
      sceneFixture({ sceneId: 's3', chapterId: 1 }),
    ]);
    const { nodes } = projectOutlineGraph(o, [chapter({ chapter: 1 })], fixture);

    const scenes = nodes.filter((n) => n.type === 'outline-scene');
    expect(scenes.map((n) => n.id)).toEqual([
      sceneNodeId('s1'),
      sceneNodeId('s2'),
      sceneNodeId('s3'),
    ]);
    // y increases down the stack (relative to parent); x stays constant.
    expect(scenes[0].position.x).toBe(scenes[1].position.x);
    expect(scenes[1].position.y).toBeGreaterThan(scenes[0].position.y);
  });

  it('does not mutate the fixture payload', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1] }],
    });
    const fixture = sceneBeatPayload(
      [sceneFixture({ sceneId: 's1', chapterId: 1 })],
      [beatFixture({ beatId: 'b1', sceneId: 's1' })],
    );
    const snapshot = JSON.parse(JSON.stringify(fixture));

    projectOutlineGraph(o, [chapter({ chapter: 1 })], fixture);

    expect(JSON.parse(JSON.stringify(fixture))).toEqual(snapshot);
  });
});

// ---------------------------------------------------------------------------
// selectedSceneIdFromNodes / selectedBeatIdFromNodes (FB-C2-002 graph click)
// ---------------------------------------------------------------------------

describe('selectedSceneIdFromNodes', () => {
  it('returns null when no node is selected', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1] }],
    });
    const fixture = sceneBeatPayload([sceneFixture({ sceneId: 's1', chapterId: 1 })]);
    const { nodes } = projectOutlineGraph(o, [chapter({ chapter: 1 })], fixture);
    expect(selectedSceneIdFromNodes(nodes)).toBeNull();
  });

  it('returns the sceneId when an outline-scene node is selected', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1] }],
    });
    const fixture = sceneBeatPayload([sceneFixture({ sceneId: 's1', chapterId: 1 })]);
    const { nodes } = projectOutlineGraph(o, [chapter({ chapter: 1 })], fixture);
    const sceneNode = nodes.find((n) => n.type === 'outline-scene')!;
    sceneNode.selected = true;

    expect(selectedSceneIdFromNodes(nodes)).toBe('s1');
  });

  it('returns null when a non-scene node is selected', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1] }],
    });
    const fixture = sceneBeatPayload([sceneFixture({ sceneId: 's1', chapterId: 1 })]);
    const { nodes } = projectOutlineGraph(o, [chapter({ chapter: 1 })], fixture);
    const chapterNode = nodes.find((n) => n.type === 'outline-chapter')!;
    chapterNode.selected = true;

    expect(selectedSceneIdFromNodes(nodes)).toBeNull();
  });
});

describe('selectedBeatIdFromNodes', () => {
  it('returns null when no node is selected', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1] }],
    });
    const fixture = sceneBeatPayload(
      [sceneFixture({ sceneId: 's1', chapterId: 1 })],
      [beatFixture({ beatId: 'b1', sceneId: 's1' })],
    );
    const { nodes } = projectOutlineGraph(o, [chapter({ chapter: 1 })], fixture);
    expect(selectedBeatIdFromNodes(nodes)).toBeNull();
  });

  it('returns the beatId when an outline-beat node is selected', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1] }],
    });
    const fixture = sceneBeatPayload(
      [sceneFixture({ sceneId: 's1', chapterId: 1 })],
      [beatFixture({ beatId: 'b1', sceneId: 's1' })],
    );
    const { nodes } = projectOutlineGraph(o, [chapter({ chapter: 1 })], fixture);
    const beatNode = nodes.find((n) => n.type === 'outline-beat')!;
    beatNode.selected = true;

    expect(selectedBeatIdFromNodes(nodes)).toBe('b1');
  });

  it('returns null when a scene node is selected (not a beat)', () => {
    const o = outline({
      volumes: [{ volume_id: 1, label: 'V1', chapter_ids: [1] }],
    });
    const fixture = sceneBeatPayload(
      [sceneFixture({ sceneId: 's1', chapterId: 1 })],
      [beatFixture({ beatId: 'b1', sceneId: 's1' })],
    );
    const { nodes } = projectOutlineGraph(o, [chapter({ chapter: 1 })], fixture);
    const sceneNode = nodes.find((n) => n.type === 'outline-scene')!;
    sceneNode.selected = true;

    expect(selectedBeatIdFromNodes(nodes)).toBeNull();
  });
});
