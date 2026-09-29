/**
 * TimelineCanvasAdapter — V1.156 P1 T1 (Moment layer projection).
 *
 * Verifies the World Timeline Moment layer projection contract locked by
 * `canvas-strategy-surface.md` §3.3.3 V1.156 amendment + product semantics
 * PD-3 (World Timeline Moment = READ/projection layer; Moments remain
 * Work-owned; no World-owned Moment write flow):
 *
 *   - `projectTimelineGraph(graph, 'moment', ...)` projects the V1.108
 *     `SceneBeatFixturePayload` scene/beat fixture onto the World Timeline
 *     Moment axis, mirroring the Work Timeline adapter's `projectMomentLayer`
 *     EXACTLY — same node-id prefix (`wt-scene:` / `wt-beat:`), same layout
 *     metrics, same node types (`work-timeline-moment-scene` /
 *     `work-timeline-moment-beat`), same `WorkTimelineNodeData` carrier
 *     (World-Moment feel ≡ Work-Moment feel per V1.123 layer-feel §2.4).
 *   - Scenes stack vertically (TB) by chapter order (numeric ascending);
 *     beats stack vertically inside their scene (chapter→scene→beat).
 *   - Manuscript-anchor badges are carried on scene + beat nodes.
 *   - Honest empty-state when the fixture is absent or empty (PD-3) — the
 *     adapter emits zero nodes; T2 owns the visible empty-state copy.
 *   - Orphan guards: beats whose scene is absent from the fixture are
 *     dropped (mirrors V1.108 rf-projection).
 *   - Node data `workId` derives from the graph's entity `world_id`
 *     (`WorldKbGraphResponse` carries no top-level `world_id`).
 *   - `createTimelineCanvasAdapter(ctxRef, 'moment')` reads the fixture from
 *     `ctxRef.current.sceneBeatFixture` and uses TB layout options.
 *   - Brief / Narrative projection unchanged (regression-safe — covered by
 *     `brief-projection.test.tsx` + `timeline-canvas-adapter.test.tsx`).
 *
 * `wire_contracts_changed: false` — frontend-only: the `TimelineLayer`
 * union extension is UI-only; the Moment carrier reuses the V1.108 fixture
 * slot (DR-26 tracks the future wire extension).
 */
import { describe, expect, it, vi } from 'vitest';
import { fireEvent, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import type { ReactElement } from 'react';
import type { Node } from '@xyflow/react';
import { http, HttpResponse } from 'msw';

import type {
  WorldKbEntityProjection,
  WorldKbGraphResponse,
  WorkOutline,
} from '@42ch/nexus-contracts';

import type { NexusClient } from '@/lib/nexus';
import { BrowserClient } from '@/lib/nexus';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import type {
  BeatFixture,
  SceneBeatFixturePayload,
  SceneFixture,
} from '../../outline-canvas/graph-projection';
import {
  createTimelineCanvasAdapter,
  projectTimelineGraph,
  type TimelineCanvasAdapterContext,
  type TimelineLayer,
  type TimelineNodeData,
} from '../timeline-canvas-adapter';
import { TimelineCanvas } from '../timeline-canvas';
import type { WorkTimelineNodeData } from '../../work-timeline-canvas/work-timeline-canvas-adapter';

// ─── Fixture builders ──────────────────────────────────────────────────────

function entity(
  overrides: Partial<WorldKbEntityProjection> &
    Pick<WorldKbEntityProjection, 'key_block_id' | 'block_type' | 'canonical_name'>,
): WorldKbEntityProjection {
  return {
    world_id: 'world-7',
    status: 'confirmed',
    version: 1,
    ...overrides,
  } as WorldKbEntityProjection;
}

function graph(overrides: Partial<WorldKbGraphResponse> = {}): WorldKbGraphResponse {
  return {
    entities: [
      entity({ key_block_id: 'kb-1', block_type: 'event', canonical_name: 'Anchor' }),
    ],
    source_anchors: [],
    relationships: [],
    ...overrides,
  };
}

function scene(partial: Partial<SceneFixture> & Pick<SceneFixture, 'sceneId'>): SceneFixture {
  return {
    sceneId: partial.sceneId,
    chapterId: partial.chapterId ?? 1,
    title: partial.title ?? `Scene ${partial.sceneId}`,
    status: partial.status ?? null,
    workId: partial.workId,
  };
}

function beat(partial: Partial<BeatFixture> & Pick<BeatFixture, 'beatId' | 'sceneId'>): BeatFixture {
  return {
    beatId: partial.beatId,
    sceneId: partial.sceneId,
    title: partial.title ?? `Beat ${partial.beatId}`,
    status: partial.status ?? null,
    workId: partial.workId,
  };
}

function fixture(scenes: SceneFixture[] = [], beats: BeatFixture[] = []): SceneBeatFixturePayload {
  return { scenes, beats };
}

function makeMockClient(): NexusClient {
  return {
    getWorldKbGraph: vi.fn(),
    worldKbPatchEntity: vi.fn(),
    worldKbPatchRelationship: vi.fn(),
    worldKbPromoteCandidate: vi.fn(),
    patchOutlineStructure: vi.fn(),
    patchOutlineChapter: vi.fn(),
    patchTimelineEvent: vi.fn(),
  } as unknown as NexusClient;
}

/**
 * Client mock for orchestrator-level `<TimelineCanvas>` mounts (F3 fix test).
 * Mirrors `layer-state-persistence.test.tsx::makeWorldMockClient`: the
 * orchestrator's read hooks (`useWorks`, `useWorldTimelineEvents`,
 * `useComputeModules`) degrade gracefully when their client methods are
 * absent, so only the graph read + workspace list + health need stubbing.
 */
function makeTimelineCanvasMockClient(graph: WorldKbGraphResponse): NexusClient {
  return {
    getWorldKbGraph: vi.fn().mockResolvedValue(graph),
    getWorks: vi.fn().mockResolvedValue({ items: [], total: 0 }),
    health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
  } as unknown as NexusClient;
}

function makeContext(
  overrides: Partial<TimelineCanvasAdapterContext> = {},
): TimelineCanvasAdapterContext {
  return {
    worldId: 'world-7',
    client: makeMockClient(),
    ...overrides,
  };
}

// The Moment nodes carry the reused `WorkTimelineNodeData` carrier at
// runtime; the adapter's return type is `Node<TimelineNodeData>`, so the
// tests narrow through the shared index signature.
function momentDataOf(node: Node<TimelineNodeData>): WorkTimelineNodeData {
  return node.data as unknown as WorkTimelineNodeData;
}

// ─── Moment projection (V1.156 P1 T1 — fixture-driven read-projection) ─────

describe('TimelineCanvasAdapter.projectTimelineGraph — Moment projection (Scene/Beat fixture)', () => {
  it('emits zero nodes when the scene/beat fixture is absent (honest empty-state per PD-3)', () => {
    const g = graph();

    const { nodes, edges } = projectTimelineGraph(g, 'moment');

    expect(nodes).toEqual([]);
    expect(edges).toEqual([]);
  });

  it('emits zero nodes when the scene/beat fixture is empty (no scenes, no beats)', () => {
    const g = graph();

    const { nodes } = projectTimelineGraph(g, 'moment', undefined, undefined, fixture());

    expect(nodes).toEqual([]);
  });

  it('projects scene cards from the fixture as work-timeline-moment-scene nodes with wt-scene: ids', () => {
    const g = graph();
    const fx = fixture([
      scene({ sceneId: 'sc-1', chapterId: 1, title: 'Opening' }),
      scene({ sceneId: 'sc-2', chapterId: 1, title: 'Rising Action' }),
      scene({ sceneId: 'sc-3', chapterId: 2, title: 'Twist' }),
    ]);

    const { nodes } = projectTimelineGraph(g, 'moment', undefined, undefined, fx);

    const sceneNodes = nodes.filter((n) => n.type === 'work-timeline-moment-scene');
    expect(sceneNodes).toHaveLength(3);
    expect(sceneNodes.every((n) => n.id.startsWith('wt-scene:'))).toBe(true);
  });

  it('projects beat pins from the fixture as work-timeline-moment-beat nodes with wt-beat: ids', () => {
    const g = graph();
    const fx = fixture(
      [scene({ sceneId: 'sc-1', chapterId: 1 })],
      [
        beat({ beatId: 'bt-1', sceneId: 'sc-1', title: 'Hook' }),
        beat({ beatId: 'bt-2', sceneId: 'sc-1', title: 'Turn' }),
      ],
    );

    const { nodes } = projectTimelineGraph(g, 'moment', undefined, undefined, fx);

    const beatNodes = nodes.filter((n) => n.type === 'work-timeline-moment-beat');
    expect(beatNodes).toHaveLength(2);
    expect(beatNodes.every((n) => n.id.startsWith('wt-beat:'))).toBe(true);
  });

  it('carries the Work Timeline node carrier (nodeKind scene + manuscript-anchor) on scene nodes', () => {
    const g = graph();
    const fx = fixture([
      scene({ sceneId: 'sc-1', chapterId: 5, title: 'Coronation Scene' }),
    ]);

    const { nodes } = projectTimelineGraph(g, 'moment', undefined, undefined, fx);
    const sceneNode = nodes.find((n) => n.id === 'wt-scene:sc-1')!;
    const data = momentDataOf(sceneNode);

    expect(data.nodeKind).toBe('scene');
    expect(data.sceneId).toBe('sc-1');
    expect(data.label).toBe('Coronation Scene');
    expect(data.realizesChapterId).toBe(5);
    expect(data.manuscriptAnchor).toEqual({
      chapterId: 5,
      sceneId: 'sc-1',
    });
  });

  it('carries manuscript-anchor data on beat nodes (chapter/scene/beat link)', () => {
    const g = graph();
    const fx = fixture(
      [scene({ sceneId: 'sc-1', chapterId: 7 })],
      [beat({ beatId: 'bt-1', sceneId: 'sc-1', title: 'Turn' })],
    );

    const { nodes } = projectTimelineGraph(g, 'moment', undefined, undefined, fx);
    const beatNode = nodes.find((n) => n.id === 'wt-beat:bt-1')!;
    const data = momentDataOf(beatNode);

    expect(data.nodeKind).toBe('beat');
    expect(data.beatId).toBe('bt-1');
    expect(data.sceneId).toBe('sc-1');
    expect(data.label).toBe('Turn');
    expect(data.realizesChapterId).toBe(7);
    expect(data.manuscriptAnchor).toEqual({
      chapterId: 7,
      sceneId: 'sc-1',
      beatId: 'bt-1',
    });
  });

  it('attributes node workId to the supplied worldId — never to the first KB entity (V1.200 DR-26)', () => {
    const g = graph({
      entities: [
        entity({
          key_block_id: 'kb-1',
          block_type: 'event',
          canonical_name: 'Anchor',
          world_id: 'world-42',
        }),
      ],
    });
    const fx = fixture([scene({ sceneId: 'sc-1', chapterId: 1 })]);

    const { nodes } = projectTimelineGraph(g, 'moment', undefined, undefined, fx, 'world-7');
    const sceneNode = nodes.find((n) => n.id === 'wt-scene:sc-1')!;

    expect(momentDataOf(sceneNode).workId).toBe('world-7');
  });

  it('carries bound-Work provenance with an empty World KB graph — two Works sharing chapter 1 stay distinct', () => {
    // The Moment carrier does NOT come from the World KB graph: a World with no
    // KB entities still renders the scenes of its bound Works. Chapter numbers
    // are Work-local, so the two chapter-1 regions must not collapse.
    const g = graph({ entities: [] });
    const fx = fixture(
      [
        scene({ sceneId: 'scn_a1', chapterId: 1, title: 'A one', workId: 'work-a' }),
        scene({ sceneId: 'scn_b1', chapterId: 1, title: 'B one', workId: 'work-b' }),
      ],
      [beat({ beatId: 'bet_a1', sceneId: 'scn_a1', workId: 'work-a' })],
    );

    const { nodes } = projectTimelineGraph(g, 'moment', undefined, undefined, fx, 'world-7');

    const sceneA = nodes.find((n) => n.id === 'wt-scene:scn_a1')!;
    const sceneB = nodes.find((n) => n.id === 'wt-scene:scn_b1')!;
    expect(momentDataOf(sceneA).workId).toBe('work-a');
    expect(momentDataOf(sceneB).workId).toBe('work-b');
    // The canonical content projects from an empty KB graph — not the empty
    // state (orchestrator-level counterpart:
    // `TimelineCanvas — bound-Work Moment carrier composition`).
    expect(momentDataOf(sceneA).label).toBe('A one');
    expect(momentDataOf(sceneB).label).toBe('B one');
    expect(sceneA.position.x).not.toBe(sceneB.position.x);

    const beatNode = nodes.find((n) => n.id === 'wt-beat:bet_a1')!;
    expect(momentDataOf(beatNode).workId).toBe('work-a');
  });

  it('stacks scenes vertically (TB) grouped by chapter region (X groups by chapter)', () => {
    const g = graph();
    const fx = fixture([
      scene({ sceneId: 'sc-c1-a', chapterId: 1 }),
      scene({ sceneId: 'sc-c2-a', chapterId: 2 }),
      scene({ sceneId: 'sc-c1-b', chapterId: 1 }), // earlier sceneId to test sort
      scene({ sceneId: 'sc-c2-b', chapterId: 2 }),
    ]);

    const { nodes } = projectTimelineGraph(g, 'moment', undefined, undefined, fx);

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
    const g = graph();
    const fx = fixture(
      [scene({ sceneId: 'sc-1', chapterId: 1 })],
      [
        beat({ beatId: 'bt-ok', sceneId: 'sc-1' }),
        beat({ beatId: 'bt-orphan', sceneId: 'missing' }), // no scene
      ],
    );

    const { nodes } = projectTimelineGraph(g, 'moment', undefined, undefined, fx);

    expect(nodes.find((n) => n.id === 'wt-beat:bt-ok')).toBeDefined();
    expect(nodes.find((n) => n.id === 'wt-beat:bt-orphan')).toBeUndefined();
  });

  it('emits zero edges on the Moment layer in V1.156 MVP (beat succession is spatial)', () => {
    const g = graph();
    const fx = fixture(
      [scene({ sceneId: 'sc-1', chapterId: 1 })],
      [beat({ beatId: 'bt-1', sceneId: 'sc-1' })],
    );

    const { edges } = projectTimelineGraph(g, 'moment', undefined, undefined, fx);

    // Beat succession is encoded spatially by the vertical stack; explicit
    // realizes_event light links (beat → Narrative event) are P4 polish.
    expect(edges).toEqual([]);
  });

  it('emits the directed-axis-spine node when scenes exist (V1.126 pattern)', () => {
    const g = graph();
    const fx = fixture([scene({ sceneId: 'sc-1', chapterId: 1 })]);

    const { nodes } = projectTimelineGraph(g, 'moment', undefined, undefined, fx);

    // Scene node + spine node.
    expect(nodes).toHaveLength(2);
    const spine = nodes.find((n) => n.id === 'directed-axis-spine');
    expect(spine).toBeDefined();
    expect(spine!.type).toBe('directedAxisSpine');
  });
});

// ─── Adapter context wiring (Moment reads fixture from ctxRef) ────────────

describe('TimelineCanvasAdapter — Moment projection reads fixture from ctxRef', () => {
  it("projectGraph delegates to Moment projection using ctxRef.current.sceneBeatFixture", () => {
    const g = graph();
    const fx = fixture([scene({ sceneId: 'sc-1', chapterId: 1 })]);

    const ctx = makeContext({ sceneBeatFixture: fx });
    const adapter = createTimelineCanvasAdapter({ current: ctx }, 'moment');
    const { nodes } = adapter.projectGraph(g);

    // Scene node + directed-axis-spine node.
    expect(nodes).toHaveLength(2);
    expect(nodes[0].type).toBe('work-timeline-moment-scene');
    expect(nodes[0].id).toBe('wt-scene:sc-1');
  });

  it('registers Moment scene + beat node types alongside the World Timeline node types', () => {
    const adapter = createTimelineCanvasAdapter({ current: makeContext() });

    const keys = Object.keys(adapter.nodeTypes).sort();
    expect(keys).toContain('work-timeline-moment-scene');
    expect(keys).toContain('work-timeline-moment-beat');
    // The Work Timeline Narrative event node is NOT registered on the World
    // surface (World Narrative keeps its own 'timeline-event' node).
    expect(keys).not.toContain('work-timeline-narrative-event');
    expect(keys).toContain('timeline-event');
    expect(keys).toContain('timeline-brief-era');
  });

  it("switching the adapter's active layer from 'narrative' to 'moment' changes layoutOptions.direction (LR → TB)", () => {
    const narrativeAdapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'narrative',
    );
    const momentAdapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'moment',
    );

    expect(narrativeAdapter.layoutOptions?.direction).toBe('LR');
    expect(momentAdapter.layoutOptions?.direction).toBe('TB');
    // Moment uses the tight scene-stack density (mirrors Work Timeline).
    expect(momentAdapter.layoutOptions?.rankSep).toBe(60);
    expect(momentAdapter.layoutOptions?.nodeSep).toBe(30);
  });

  it('exposes TimelineLayer type at runtime via the factory signature (Moment smoke test)', () => {
    const layers: TimelineLayer[] = ['brief', 'narrative', 'moment'];
    for (const layer of layers) {
      const adapter = createTimelineCanvasAdapter(
        { current: makeContext() },
        layer,
      );
      expect(adapter.surfaceKind).toBe('timeline');
    }
  });
});

// ─── F1: Moment nodes dispatch to the read-only Moment inspector ────────────
//
// Fix-wave 1 (qc1 I-001 / qc2 W-1 / qc3 F-1): Moment scene/beat nodes are
// selectable but previously fell through `renderInspector` to the generic KB
// `TimelineInspector`, whose Save fires `kb.patch_entity` with
// `entity_id: undefined` (the `WorkTimelineNodeData` carrier has no
// `key_block_id`) — a guaranteed-failing write request on a read/projection
// layer (PD-3 violation). The dispatch now routes Moment nodes to the
// read-only Moment inspector (layer-feel parity with Work-Moment).

describe('TimelineCanvasAdapter.renderInspector — Moment nodes route to the read-only Moment inspector (F1 fix)', () => {
  it('scene node renders the read-only Moment inspector — NOT the generic KB TimelineInspector', () => {
    const g = graph();
    const fx = fixture([
      scene({ sceneId: 'sc-1', chapterId: 5, title: 'Coronation Scene' }),
    ]);
    const adapter = createTimelineCanvasAdapter(
      { current: makeContext({ sceneBeatFixture: fx }) },
      'moment',
    );
    const { nodes } = adapter.projectGraph(g);
    const sceneNode = nodes.find((n) => n.id === 'wt-scene:sc-1')!;

    const inspector = adapter.renderInspector!(sceneNode);
    expect(inspector).not.toBeNull();

    const { container } = renderInApp(inspector as ReactElement);
    // The read-only Moment inspector form surfaces...
    expect(
      container.querySelector('[data-testid="timeline-moment-inspector"]'),
    ).not.toBeNull();
    // ...and the generic KB editor (title + Save → kb.patch_entity) does NOT.
    expect(
      container.querySelector('[data-testid="timeline-inspector-title"]'),
    ).toBeNull();
    expect(
      container.querySelector('[data-testid="timeline-inspector-save"]'),
    ).toBeNull();
    // Scene identity + chapter + manuscript anchor surface read-only.
    expect(container.textContent).toContain('Coronation Scene');
    expect(container.textContent).toContain('sc-1');
    expect(container.textContent).toContain('5');
    // No Edit-in-Outline CTA — the Work Timeline CTA would navigate to
    // /works/<worldId>/outline (qc3 F-4 footgun: the node's `workId` field
    // carries the World id on this surface).
    expect(
      container.querySelector(
        '[data-testid="work-timeline-inspector-edit-in-outline"]',
      ),
    ).toBeNull();
  });

  it('beat node renders the read-only Moment inspector with beat-level fields', () => {
    const g = graph();
    const fx = fixture(
      [scene({ sceneId: 'sc-1', chapterId: 7 })],
      [beat({ beatId: 'bt-1', sceneId: 'sc-1', title: 'Hook Beat' })],
    );
    const adapter = createTimelineCanvasAdapter(
      { current: makeContext({ sceneBeatFixture: fx }) },
      'moment',
    );
    const { nodes } = adapter.projectGraph(g);
    const beatNode = nodes.find((n) => n.id === 'wt-beat:bt-1')!;

    const inspector = adapter.renderInspector!(beatNode);
    expect(inspector).not.toBeNull();

    const { container } = renderInApp(inspector as ReactElement);
    expect(
      container.querySelector('[data-testid="timeline-moment-inspector"]'),
    ).not.toBeNull();
    expect(container.textContent).toContain('Hook Beat');
    expect(container.textContent).toContain('bt-1');
    expect(container.textContent).toContain('sc-1');
    expect(
      container.querySelector('[data-testid="timeline-inspector-title"]'),
    ).toBeNull();
    expect(
      container.querySelector('[data-testid="timeline-inspector-save"]'),
    ).toBeNull();
  });

  it('Narrative event node still routes to the generic Timeline inspector (V1.122 regression)', () => {
    // F1 dispatch is ADDITIVE — the Moment branch must not steal the
    // entity path. A `timeline-event` node still renders the generic KB
    // inspector (title + body JSON editor).
    const g = graph();
    const adapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'narrative',
    );
    const { nodes } = adapter.projectGraph(g);
    const eventNode = nodes.find((n) => n.id === 'entity:kb-1')!;

    const inspector = adapter.renderInspector!(eventNode);
    expect(inspector).not.toBeNull();

    const { container } = renderInApp(inspector as ReactElement);
    expect(
      container.querySelector('[data-testid="timeline-inspector-title"]'),
    ).not.toBeNull();
    expect(
      container.querySelector('[data-testid="timeline-moment-inspector"]'),
    ).toBeNull();
  });
});

// ─── F2: alt-view survives Moment rows (no Kind-sort crash) ────────────────
//
// Fix-wave 1 (qc2 W-2): `compareNodes` case 'kind' called
// `a.block_type.localeCompare` on Moment rows (`WorkTimelineNodeData` has no
// `block_type`) → TypeError → React render crash of the accessible
// non-spatial view. The wrapper now filters to KB-entity rows AND the sort
// comparator is null-safe.

describe('TimelineCanvasAdapter.renderAltView — Moment rows do not crash the table (F2 fix)', () => {
  it('filters Moment rows out while entity rows still render; the Kind sort does not throw', () => {
    const g = graph();
    const fx = fixture([
      scene({ sceneId: 'sc-1', chapterId: 1, title: 'Opening Scene' }),
    ]);
    const momentAdapter = createTimelineCanvasAdapter(
      { current: makeContext({ sceneBeatFixture: fx }) },
      'moment',
    );
    const momentNodes = momentAdapter.projectGraph(g)
      .nodes as Node<TimelineNodeData>[];

    const eventNode: Node<TimelineNodeData> = {
      id: 'entity:kb-event-1',
      type: 'timeline-event',
      position: { x: 0, y: 0 },
      data: {
        ...entity({
          key_block_id: 'kb-event-1',
          block_type: 'event',
          canonical_name: 'Coronation',
        }),
        layoutHint: 'event',
      },
    };

    const ctxRef = {
      current: makeContext({
        nodes: [...momentNodes, eventNode],
        selectedNodeId: null,
        onSelectNode: vi.fn(),
      }),
    };
    const adapter = createTimelineCanvasAdapter(ctxRef, 'moment');
    const { container } = renderInApp(<>{adapter.renderAltView!()}</>);

    // Moment rows (scene card + directed-axis-spine) are filtered out — only
    // the KB entity row renders.
    expect(container.textContent).toContain('Coronation');
    expect(container.textContent).not.toContain('Opening Scene');

    // Clicking the "Kind" column header must not throw on Moment data
    // (regression: `block_type` undefined → `localeCompare` TypeError).
    const kindHeader = container.querySelectorAll('thead button')[1];
    expect(kindHeader).toBeDefined();
    expect(() => fireEvent.click(kindHeader)).not.toThrow();
    // The entity row survives the sort.
    expect(container.textContent).toContain('Coronation');
  });

  it('renders the honest empty row when the layer projects only Moment nodes', () => {
    const g = graph();
    const fx = fixture([
      scene({ sceneId: 'sc-1', chapterId: 1, title: 'Opening Scene' }),
    ]);
    const momentAdapter = createTimelineCanvasAdapter(
      { current: makeContext({ sceneBeatFixture: fx }) },
      'moment',
    );
    const momentNodes = momentAdapter.projectGraph(g)
      .nodes as Node<TimelineNodeData>[];

    const ctxRef = {
      current: makeContext({
        nodes: momentNodes,
        selectedNodeId: null,
        onSelectNode: vi.fn(),
      }),
    };
    const adapter = createTimelineCanvasAdapter(ctxRef, 'moment');
    const { container } = renderInApp(<>{adapter.renderAltView!()}</>);

    // No KB-entity rows → honest empty copy, no crash (the previous
    // behavior would render blank Moment rows that crashed on Kind sort).
    expect(container.textContent).not.toContain('Opening Scene');
    expect(container.querySelectorAll('tbody tr')).toHaveLength(1);
    expect(() => fireEvent.click(container.querySelectorAll('thead button')[1])).not.toThrow();
  });
});

// ─── F3: fixture identity change re-projects without a layer swap ──────────
//
// Fix-wave 1 (qc3 F-2): `sceneBeatFixture` was excluded from the adapter
// memo deps (and read late via `ctxRef.current` AFTER the memo), so a
// fixture identity change after mount left the Moment projection stale until
// a layer swap / graph refetch. The adapter now captures the fixture and the
// memo deps include the stable `EMPTY_SCENE_BEAT_FIXTURE`-backed reference.

describe('TimelineCanvas — fixture identity change re-projects the Moment layer (F3 fix)', () => {
  it('injecting a sceneBeatFixture after mount removes the Moment empty-state without a layer swap', async () => {
    const g = graph({
      entities: [
        entity({
          key_block_id: 'kb-era-1',
          block_type: 'era',
          canonical_name: 'The First Age',
        }),
      ],
    });

    function FixtureSwapHarness() {
      const [fx, setFx] = useState<SceneBeatFixturePayload | undefined>(
        undefined,
      );
      return (
        <>
          <button
            type="button"
            onClick={() =>
              setFx(
                fixture([
                  scene({
                    sceneId: 'sc-1',
                    chapterId: 1,
                    title: 'Opening',
                  }),
                ]),
              )
            }
          >
            inject-fixture
          </button>
          <TimelineCanvas worldId="world-7" sceneBeatFixture={fx} />
        </>
      );
    }

    renderInApp(<FixtureSwapHarness />, {
      client: makeTimelineCanvasMockClient(g),
      initialRouterEntries: ['/worlds/world-7/timeline?layer=moment'],
    });

    // No fixture → honest Moment empty-state panel (zero projected nodes).
    await waitFor(() => {
      expect(
        screen.getByTestId('timeline-moment-empty-state'),
      ).toBeInTheDocument();
    });

    // Fixture identity change → adapter rebuilt (memo deps include the
    // fixture) → re-projection → nodes exist → the empty-state panel
    // disappears. No layer swap, no graph refetch.
    fireEvent.click(screen.getByText('inject-fixture'));
    await waitFor(() => {
      expect(
        screen.queryByTestId('timeline-moment-empty-state'),
      ).toBeNull();
    });
  });
});

// ─── V1.156 P5 — World-Moment NLE band ─────────────────────────────────────
//
// Greptile finding: `nle-timeline-band-overlay.tsx` fell the World `moment`
// layer through to the Narrative projection, which matches only
// `timeline-event`/`timeline-compute-result` nodes — so a Moment fixture
// (scene/beat nodes) produced zero tracks and the band vanished. The overlay
// now projects World-Moment nodes into Scenes/Beats tracks; this test pins
// the band at the orchestrator level (not null / not disappeared).

describe('TimelineCanvas — World-Moment NLE band (V1.156 P5)', () => {
  it('renders Scenes/Beats NLE tracks when the Moment layer has fixture data', async () => {
    const g = graph({
      entities: [
        entity({
          key_block_id: 'kb-era-1',
          block_type: 'era',
          canonical_name: 'The First Age',
        }),
      ],
    });

    renderInApp(
      <TimelineCanvas
        worldId="world-7"
        sceneBeatFixture={fixture(
          [scene({ sceneId: 'sc-1', chapterId: 1, title: 'Opening' })],
          [beat({ beatId: 'bt-1', sceneId: 'sc-1', title: 'Hook' })],
        )}
      />,
      {
        client: makeTimelineCanvasMockClient(g),
        initialRouterEntries: ['/worlds/world-7/timeline?layer=moment'],
      },
    );

    // The band overlay renders (not null) and carries both Moment tracks —
    // the pre-fix Narrative fallthrough produced zero tracks for scene/beat
    // nodes and the overlay returned null.
    await waitFor(() => {
      expect(screen.getByTestId('nle-timeline-band-overlay')).toBeInTheDocument();
    });
    expect(screen.getByTestId('nle-timeline-label-scenes')).toHaveTextContent(
      'Scenes',
    );
    expect(screen.getByTestId('nle-timeline-label-beats')).toHaveTextContent(
      'Beats',
    );
  });
});

// ─── Orchestrator — bound-Work canonical carrier (V1.200 DR-26) ────────────
//
// The World Moment layer's carrier is composed from the bound Works' canonical
// `WorkOutline.scenes[]` / `beats[]` — the bound-Work outline fan-out must be
// enabled on the Moment layer with NO Narrative event selection (the former
// selection-only gate left the layer permanently empty), and the composed
// entries must keep their Work provenance.

function workSummary(workId: string, updatedAt: string) {
  return {
    work_id: workId,
    title: `Work ${workId}`,
    status: 'active',
    intake_status: 'complete',
    primary_preset_id: 'preset-a',
    updated_at: updatedAt,
  };
}

function workDetail(workId: string) {
  return {
    work_id: workId,
    status: 'active',
    title: `Work ${workId}`,
    long_term_goal: '',
    initial_idea: '',
    intake_status: 'complete',
    world_id: 'world-7',
    inspiration_log: [],
    primary_preset_id: 'preset-a',
    schedule_ids: [],
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    current_stage: 'draft',
    stage_status: 'active',
    auto_chain_enabled: false,
    auto_chain_interrupted: false,
    auto_review_master_on_timeout: false,
    total_planned_chapters: 0,
    current_chapter: 0,
  };
}

function workOutline(workId: string, sceneTitle: string, sceneId: string): WorkOutline {
  return {
    work_id: workId,
    outline_revision: 1,
    volumes: [],
    timeline_events: [],
    foreshadows: [],
    scenes: [{ scene_id: sceneId, chapter_id: 1, title: sceneTitle, status: 'drafted' }],
    beats: [],
    chapter_titles: {},
    updated_at: '2026-08-01T00:00:00Z',
  };
}

describe('TimelineCanvas — bound-Work Moment carrier composition (V1.200 DR-26)', () => {
  it('projects both bound Works’ chapter-1 scenes on the Moment layer with no Narrative selection', async () => {
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/kb/graph', () =>
        HttpResponse.json({
          entities: [
            entity({ key_block_id: 'kb-era-1', block_type: 'era', canonical_name: 'The First Age' }),
          ],
          source_anchors: [],
          relationships: [],
        }),
      ),
      http.get('/v1/daemon/worlds/:worldId/timeline/events', () =>
        HttpResponse.json({ items: [], has_more: false, next_cursor: undefined }),
      ),
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({
          items: [
            workSummary('work-a', '2026-01-01T00:00:00Z'),
            workSummary('work-b', '2026-01-02T00:00:00Z'),
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.get('/v1/daemon/works/:workId', ({ params }) =>
        HttpResponse.json(workDetail(String(params.workId))),
      ),
      http.get('/v1/daemon/works/:workId/outline', ({ params }) => {
        const workId = String(params.workId);
        return HttpResponse.json(
          workId === 'work-a'
            ? workOutline('work-a', 'A one', 'scn_a1')
            : workOutline('work-b', 'B one', 'scn_b1'),
        );
      }),
      http.get('/v1/daemon/compute/modules', () => HttpResponse.json({ items: [], has_more: false })),
    );

    renderInApp(<TimelineCanvas worldId="world-7" />, {
      client: new BrowserClient(),
      initialRouterEntries: ['/worlds/world-7/timeline?layer=moment'],
    });

    // Both Works' canonical scenes reach the Moment layer (no selection, no
    // fixture prop) — the empty-state panel is gone.
    await waitFor(() => {
      expect(screen.queryByTestId('timeline-moment-empty-state')).toBeNull();
    });
    expect((await screen.findAllByText('A one')).length).toBeGreaterThan(0);
    expect((await screen.findAllByText('B one')).length).toBeGreaterThan(0);
  });

  it('renders the canonical scenes when the World KB graph AND the compute log are both empty', async () => {
    // Task 3 review Important #1 — the required empty-KB World Moment case.
    // The global empty-state gate used to own this World (zero KB entities +
    // zero compute events), so the bound Work's canonical scenes were
    // unreachable on the Moment layer. The emptiness decision now includes the
    // canonical Moment carrier: this World renders its scenes, and the global
    // EmptyState is reserved for Worlds with truly nothing.
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/kb/graph', () =>
        HttpResponse.json({ entities: [], source_anchors: [], relationships: [] }),
      ),
      http.get('/v1/daemon/worlds/:worldId/timeline/events', () =>
        HttpResponse.json({ items: [], has_more: false, next_cursor: undefined }),
      ),
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({
          items: [workSummary('work-a', '2026-01-01T00:00:00Z')],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.get('/v1/daemon/works/:workId', ({ params }) =>
        HttpResponse.json(workDetail(String(params.workId))),
      ),
      http.get('/v1/daemon/works/:workId/outline', () =>
        HttpResponse.json(workOutline('work-a', 'A one', 'scn_a1')),
      ),
      http.get('/v1/daemon/compute/modules', () => HttpResponse.json({ items: [], has_more: false })),
    );

    renderInApp(<TimelineCanvas worldId="world-7" />, {
      client: new BrowserClient(),
      initialRouterEntries: ['/worlds/world-7/timeline?layer=moment'],
    });

    // The bound Work's canonical scene is the World's Moment content.
    expect((await screen.findAllByText('A one')).length).toBeGreaterThan(0);
    expect(screen.queryByTestId('timeline-moment-empty-state')).toBeNull();
    // NOT the global empty state — the World is not empty.
    expect(screen.queryByText("This World's timeline is empty")).toBeNull();
  });
});
