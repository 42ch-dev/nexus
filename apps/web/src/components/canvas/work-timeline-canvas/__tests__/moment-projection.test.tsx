/**
 * WorkTimelineCanvasAdapter — V1.123 P2 Task 3 (Moment layer projection).
 *
 * Verifies the Moment projection contract locked by
 *   - `iterations/v1.123/specs/three-layer-architecture.md` §3 (Moment-
 *     on-Outline carrier LOCK) + §7 + §8 (Moment data composition from
 *     outline Scene/Beat fixture data, frontend-only projection).
 *   - `iterations/v1.123/specs/layer-feel-differentiation.md` §2.4
 *     (Moment feel: vertical scene-stack, dense, scene-icon + beat-pin +
 *     manuscript-anchor badge).
 *   - Plan `2026-07-18-v1.123-work-timeline-narrative-moment.md` Task 3.
 *
 * Coverage:
 *   - `projectGraphForLayer(graph, 'moment')` returns scene + beat nodes
 *     from the V1.108 `SceneBeatFixturePayload` injected via the adapter
 *     context (`ctxRef.current.sceneBeatFixture`).
 *   - Scenes stack vertically (TB) by chapter order (numeric ascending);
 *     beats stack vertically inside their scene (chapter→scene→beat).
 *   - Manuscript-anchor badges are mandatory on scene + beat nodes when
 *     anchor data exists (layer-feel §2.4).
 *   - Honest empty-state when the fixture is absent or empty (architect
 *     §3.2 + product spec §4.5) — adapter emits zero nodes; Task 7 owns
 *     the visible empty-state copy.
 *   - Orphan guards: scenes whose chapter is unknown + beats whose scene
 *     is unknown are dropped (mirrors V1.108 rf-projection).
 *   - Node types registered: `work-timeline-moment-scene` + `work-timeline-
 *     moment-beat` (Task 2's Narrative event node preserved).
 *
 * Architect lock: Moment-on-Outline (frontend-only projection; backend
 * stays V1.72 `WorkOutline`). No wire diff in P2.
 */
import { describe, expect, it, vi } from 'vitest';
import type { Node } from '@xyflow/react';

import type { WorkOutline } from '@42ch/nexus-contracts';

import type { NexusClient } from '@/lib/nexus';
import type {
  BeatFixture,
  SceneBeatFixturePayload,
  SceneFixture,
} from '../../outline-canvas/graph-projection';
import {
  createWorkTimelineCanvasAdapter,
  projectWorkTimelineGraph,
  type WorkTimelineCanvasAdapterContext,
  type WorkTimelineNodeData,
} from '../work-timeline-canvas-adapter';

// ─── Fixture builders ──────────────────────────────────────────────────────

function outline(overrides: Partial<WorkOutline> = {}): WorkOutline {
  return {
    work_id: 'work-1',
    outline_revision: 1,
    volumes: [],
    timeline_events: [],
    foreshadows: [],
    chapter_titles: {},
    updated_at: '2026-07-18T00:00:00Z',
    ...overrides,
  } as WorkOutline;
}

function scene(partial: Partial<SceneFixture> & Pick<SceneFixture, 'sceneId'>): SceneFixture {
  return {
    sceneId: partial.sceneId,
    chapterId: partial.chapterId ?? 1,
    title: partial.title ?? `Scene ${partial.sceneId}`,
    status: partial.status ?? null,
  };
}

function beat(partial: Partial<BeatFixture> & Pick<BeatFixture, 'beatId' | 'sceneId'>): BeatFixture {
  return {
    beatId: partial.beatId,
    sceneId: partial.sceneId,
    title: partial.title ?? `Beat ${partial.beatId}`,
    status: partial.status ?? null,
  };
}

function fixture(scenes: SceneFixture[] = [], beats: BeatFixture[] = []): SceneBeatFixturePayload {
  return { scenes, beats };
}

function makeMockClient(): NexusClient {
  return {
    getWorkOutline: vi.fn(),
    patchOutlineStructure: vi.fn(),
    patchOutlineChapter: vi.fn(),
    patchTimelineEvent: vi.fn(),
    health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
  } as unknown as NexusClient;
}

function makeContext(
  overrides: Partial<WorkTimelineCanvasAdapterContext> = {},
): WorkTimelineCanvasAdapterContext {
  return {
    workId: 'work-1',
    client: makeMockClient(),
    ...overrides,
  };
}

// ─── Moment projection (architect §3 + §7 + §8) ───────────────────────────

describe('WorkTimelineCanvasAdapter.projectGraphForLayer — Moment projection (Scene/Beat fixture)', () => {
  it('emits zero nodes when the scene/beat fixture is absent (honest empty-state)', () => {
    const g = outline();

    const { nodes, edges } = projectWorkTimelineGraph(g, 'moment');

    expect(nodes).toEqual([]);
    expect(edges).toEqual([]);
  });

  it('emits zero nodes when the scene/beat fixture is empty (no scenes, no beats)', () => {
    const g = outline();

    const { nodes } = projectWorkTimelineGraph(g, 'moment', fixture());

    expect(nodes).toEqual([]);
  });

  it('projects scene cards from the fixture as work-timeline-moment-scene nodes', () => {
    const g = outline();
    const fx = fixture([
      scene({ sceneId: 'sc-1', chapterId: 1, title: 'Opening' }),
      scene({ sceneId: 'sc-2', chapterId: 1, title: 'Rising Action' }),
      scene({ sceneId: 'sc-3', chapterId: 2, title: 'Twist' }),
    ]);

    const { nodes } = projectWorkTimelineGraph(g, 'moment', fx);

    const sceneNodes = nodes.filter((n) => n.type === 'work-timeline-moment-scene');
    expect(sceneNodes).toHaveLength(3);
    expect(sceneNodes.every((n) => n.id.startsWith('wt-scene:'))).toBe(true);
  });

  it('projects beat pins from the fixture as work-timeline-moment-beat nodes', () => {
    const g = outline();
    const fx = fixture(
      [scene({ sceneId: 'sc-1', chapterId: 1 })],
      [
        beat({ beatId: 'bt-1', sceneId: 'sc-1', title: 'Hook' }),
        beat({ beatId: 'bt-2', sceneId: 'sc-1', title: 'Turn' }),
      ],
    );

    const { nodes } = projectWorkTimelineGraph(g, 'moment', fx);

    const beatNodes = nodes.filter((n) => n.type === 'work-timeline-moment-beat');
    expect(beatNodes).toHaveLength(2);
    expect(beatNodes.every((n) => n.id.startsWith('wt-beat:'))).toBe(true);
  });

  it('carries manuscript-anchor data on scene nodes (mandatory per layer-feel §2.4)', () => {
    const g = outline();
    const fx = fixture([
      scene({ sceneId: 'sc-1', chapterId: 5, title: 'Coronation Scene' }),
    ]);

    const { nodes } = projectWorkTimelineGraph(g, 'moment', fx);
    const sceneNode = nodes.find((n) => n.id === 'wt-scene:sc-1') as Node<WorkTimelineNodeData>;

    expect(sceneNode.data.nodeKind).toBe('scene');
    expect(sceneNode.data.sceneId).toBe('sc-1');
    expect(sceneNode.data.label).toBe('Coronation Scene');
    expect(sceneNode.data.realizesChapterId).toBe(5);
    expect(sceneNode.data.manuscriptAnchor).toEqual({
      chapterId: 5,
      sceneId: 'sc-1',
    });
  });

  it('carries manuscript-anchor data on beat nodes (chapter/scene/beat link)', () => {
    const g = outline();
    const fx = fixture(
      [scene({ sceneId: 'sc-1', chapterId: 7 })],
      [beat({ beatId: 'bt-1', sceneId: 'sc-1', title: 'Turn' })],
    );

    const { nodes } = projectWorkTimelineGraph(g, 'moment', fx);
    const beatNode = nodes.find((n) => n.id === 'wt-beat:bt-1') as Node<WorkTimelineNodeData>;

    expect(beatNode.data.nodeKind).toBe('beat');
    expect(beatNode.data.beatId).toBe('bt-1');
    expect(beatNode.data.sceneId).toBe('sc-1');
    expect(beatNode.data.label).toBe('Turn');
    expect(beatNode.data.realizesChapterId).toBe(7);
    expect(beatNode.data.manuscriptAnchor).toEqual({
      chapterId: 7,
      sceneId: 'sc-1',
      beatId: 'bt-1',
    });
  });

  it('stacks scenes vertically (TB) grouped by chapter region (X groups by chapter)', () => {
    const g = outline();
    const fx = fixture([
      scene({ sceneId: 'sc-c1-a', chapterId: 1 }),
      scene({ sceneId: 'sc-c2-a', chapterId: 2 }),
      scene({ sceneId: 'sc-c1-b', chapterId: 1 }), // earlier sceneId to test sort
      scene({ sceneId: 'sc-c2-b', chapterId: 2 }),
    ]);

    const { nodes } = projectWorkTimelineGraph(g, 'moment', fx);

    // Chapter 1 scenes share X; chapter 2 scenes share X (different from ch1).
    const ch1A = nodes.find((n) => n.id === 'wt-scene:sc-c1-a')!;
    const ch1B = nodes.find((n) => n.id === 'wt-scene:sc-c1-b')!;
    const ch2A = nodes.find((n) => n.id === 'wt-scene:sc-c2-a')!;
    const ch2B = nodes.find((n) => n.id === 'wt-scene:sc-c2-b')!;

    // Same chapter → same X.
    expect(ch1A.position.x).toBe(ch1B.position.x);
    expect(ch2A.position.x).toBe(ch2B.position.x);
    // Different chapters → different X (ch2 is to the right of ch1).
    expect(ch2A.position.x).toBeGreaterThan(ch1A.position.x);
    // Within a chapter, scenes stack vertically (Y differs by sceneId sort).
    expect(ch1A.position.y).not.toBe(ch1B.position.y);
  });

  it('drops beats whose scene is absent from the fixture (orphan guard)', () => {
    const g = outline();
    const fx = fixture(
      [scene({ sceneId: 'sc-1', chapterId: 1 })],
      [
        beat({ beatId: 'bt-ok', sceneId: 'sc-1' }),
        beat({ beatId: 'bt-orphan', sceneId: 'missing' }), // no scene
      ],
    );

    const { nodes } = projectWorkTimelineGraph(g, 'moment', fx);

    expect(nodes.find((n) => n.id === 'wt-beat:bt-ok')).toBeDefined();
    expect(nodes.find((n) => n.id === 'wt-beat:bt-orphan')).toBeUndefined();
  });

  it('emits zero edges on the Moment layer in V1.123 MVP (beat succession is spatial)', () => {
    const g = outline();
    const fx = fixture(
      [scene({ sceneId: 'sc-1', chapterId: 1 })],
      [beat({ beatId: 'bt-1', sceneId: 'sc-1' })],
    );

    const { edges } = projectWorkTimelineGraph(g, 'moment', fx);

    // Architect §6.2 + layer-feel §2.4: beat succession is encoded
    // spatially by the vertical stack; explicit realizes_event light links
    // (beat → Narrative event) are P4 polish.
    expect(edges).toEqual([]);
  });
});

// ─── Adapter context wiring (Moment reads fixture from ctxRef) ────────────

describe('WorkTimelineCanvasAdapter — Moment projection reads fixture from ctxRef', () => {
  it("projectGraph delegates to Moment projection using ctxRef.current.sceneBeatFixture", () => {
    const g = outline();
    const fx = fixture([scene({ sceneId: 'sc-1', chapterId: 1 })]);

    const ctx = makeContext({ sceneBeatFixture: fx });
    const adapter = createWorkTimelineCanvasAdapter(
      { current: ctx },
      'moment',
    );
    const { nodes } = adapter.projectGraph(g);

    // V1.126 P1: directed-axis spine node is also added.
    expect(nodes).toHaveLength(2);
    expect(nodes[0].type).toBe('work-timeline-moment-scene');
    expect(nodes[0].id).toBe('wt-scene:sc-1');
  });

  it('registers Moment scene + beat node types alongside the Narrative event node (Task 3 registry extension)', () => {
    const adapter = createWorkTimelineCanvasAdapter({ current: makeContext() });

    // Task 2 registered only the Narrative event node; Task 3 adds the
    // Moment scene + beat nodes. V1.126 P1 adds directedAxisSpine.
    // V1.156 P2 T1 adds the reused World Timeline `timeline-brief-era` node
    // (Work-Brief layer — no new node component family).
    expect(Object.keys(adapter.nodeTypes).sort()).toEqual([
      'directedAxisSpine',
      'timeline-brief-era',
      'work-timeline-moment-beat',
      'work-timeline-moment-scene',
      'work-timeline-narrative-event',
    ]);
  });

  it("switching the adapter's active layer from 'narrative' to 'moment' changes layoutOptions.direction (LR → TB)", () => {
    const narrativeAdapter = createWorkTimelineCanvasAdapter(
      { current: makeContext() },
      'narrative',
    );
    const momentAdapter = createWorkTimelineCanvasAdapter(
      { current: makeContext() },
      'moment',
    );

    expect(narrativeAdapter.layoutOptions?.direction).toBe('LR');
    expect(momentAdapter.layoutOptions?.direction).toBe('TB');
  });
});
