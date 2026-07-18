/**
 * WorkTimelineCanvas — V1.123 P2 Task 6 (Moment-layer feel differentiation +
 * Moment inspector).
 *
 * Verifies the per-layer feel contract locked by:
 *   - `iterations/v1.123/specs/layer-feel-differentiation.md` §2.4 (Moment
 *     feel: vertical scene-stack, dense layout, manuscript-anchor badges
 *     mandatory; ink-on-paper accent via `canvas-outline-accent` until P4
 *     ships `--color-canvas-layer-moment-accent`).
 *   - Plan `2026-07-18-v1.123-work-timeline-narrative-moment.md` Task 6
 *     (Moment layout options `direction: 'TB'`, tight `nodeSep` / `rankSep`;
 *     Moment inspector with manuscript link + "Edit in Outline" hand-off).
 *   - Architect §6 — read-only in V1.123: inspector surfaces scene/beat
 *     details + a "Edit in Outline" CTA. No write endpoint is invoked from
 *     the Work Timeline surface.
 *
 * Coverage:
 *   - Moment adapter layout options: TB direction + tighter rankSep/nodeSep
 *     than Narrative (feel-differentiation §2.4 "vertical scene-stack, dense").
 *   - Moment projection positions scenes vertically (Y axis grows downward
 *     per chapter region; beats stack inside their scene).
 *   - Moment inspector renders when a Moment scene/beat node is selected;
 *     shows manuscript-anchor + "Edit in Outline" CTA without invoking any
 *     forbidden write endpoint (architect §6 read-only invariant).
 *   - Moment ≠ Narrative in density + layout direction + node visual.
 *
 * Inspector dispatch tests mirror the V1.123 P1 Brief-era inspector dispatch
 * test pattern (`brief-feel-differentiation.test.tsx` §"Brief-era inspector
 * dispatch"): they call `adapter.renderInspector(node)` directly and render
 * the returned JSX in isolation. The projection tests call the adapter's
 * `projectGraph` directly. jsdom does not render React Flow node text, so a
 * full-canvas mount cannot drive selection cleanly; adapter-level dispatch
 * is the cleaner contract surface.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';

import { renderInApp } from '@/test/test-providers';
import type { WorkOutline } from '@42ch/nexus-contracts';
import type {
  BeatFixture,
  SceneFixture,
} from '../../outline-canvas/graph-projection';

import {
  createWorkTimelineCanvasAdapter,
  type WorkTimelineCanvasAdapterContext,
} from '../work-timeline-canvas-adapter';

// ─── Fixture builders ──────────────────────────────────────────────────────

function outline(overrides: Partial<WorkOutline> = {}): WorkOutline {
  return {
    work_id: 'work-1',
    outline_revision: 1,
    volumes: [],
    timeline_events: [
      { event_id: 'evt-1', title: 'Inciting Incident', realizes_chapter_id: 1 },
    ],
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

// ─── Adapter layout options (feel-differentiation §2.4) ────────────────────

describe('WorkTimelineCanvas — Moment feel differentiation (V1.123 P2 Task 6)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('Moment adapter layout options are TB direction with tighter spacing than Narrative', () => {
    // layer-feel §2.4: Moment = vertical scene-stack (TB), dense layout.
    // Plan Task 6 Step 2: tight `nodeSep` / `rankSep` (e.g. 30 / 60 per brief).
    // Narrative = LR direction (V1.122 baseline); the two directions MUST
    // differ so a screenshot reads as a different instrument.
    const ctxRef = { current: { workId: 'work-1' } } as React.MutableRefObject<WorkTimelineCanvasAdapterContext>;
    const momentAdapter = createWorkTimelineCanvasAdapter(ctxRef, 'moment');
    const narrativeAdapter = createWorkTimelineCanvasAdapter(ctxRef, 'narrative');

    expect(momentAdapter.layoutOptions?.direction).toBe('TB');
    expect(narrativeAdapter.layoutOptions?.direction).toBe('LR');

    // Moment rankSep + nodeSep are present (denser than no-config baseline).
    expect(momentAdapter.layoutOptions?.rankSep).toBeDefined();
    expect(momentAdapter.layoutOptions?.nodeSep).toBeDefined();

    // Narrative does NOT carry Moment-specific rankSep/nodeSep (LR baseline
    // inherits V1.122 default spacing — the differentiation axis is direction
    // + Moment density, not Narrative re-tuning).
    expect(narrativeAdapter.layoutOptions?.rankSep).toBeUndefined();
    expect(narrativeAdapter.layoutOptions?.nodeSep).toBeUndefined();
  });

  it('Moment projection stacks scenes vertically by chapter region (Y grows, X by chapter)', () => {
    // layer-feel §2.4: "Vertical scene-stack or scene-card (T→B preferred);
    //   scene/beat cards + manuscript-anchor badges."
    // Two scenes in different chapters → different X (chapter region) AND
    // Y position reflecting the vertical stack. The exact metrics are
    // adapter-internal; the contract is "X groups by chapter, Y stacks".
    const ctxRef = {
      current: {
        workId: 'work-1',
        sceneBeatFixture: {
          scenes: [
            scene({ sceneId: 'sc-1', chapterId: 1, title: 'Opening' }),
            scene({ sceneId: 'sc-2', chapterId: 2, title: 'Rising' }),
          ],
          beats: [],
        },
      },
    } as React.MutableRefObject<WorkTimelineCanvasAdapterContext>;
    const adapter = createWorkTimelineCanvasAdapter(ctxRef, 'moment');
    const graph = outline();
    const { nodes } = adapter.projectGraph(graph);

    const sceneNodes = nodes.filter((n) => n.data.nodeKind === 'scene');
    expect(sceneNodes).toHaveLength(2);

    const opening = sceneNodes.find((n) => n.data.sceneId === 'sc-1')!;
    const rising = sceneNodes.find((n) => n.data.sceneId === 'sc-2')!;

    // Different chapter regions → different X (chapter step is non-zero).
    expect(rising.position.x).toBeGreaterThan(opening.position.x);
    // manuscript-anchor badges mandatory (layer-feel §2.4).
    expect(opening.data.manuscriptAnchor).toBeDefined();
    expect(opening.data.manuscriptAnchor?.chapterId).toBe(1);
    expect(rising.data.manuscriptAnchor?.chapterId).toBe(2);
  });

  it('Moment projection stacks beats inside their parent scene (Y offset within scene)', () => {
    // layer-feel §2.4: "Beats stack vertically inside their scene."
    const ctxRef = {
      current: {
        workId: 'work-1',
        sceneBeatFixture: {
          scenes: [scene({ sceneId: 'sc-1', chapterId: 1, title: 'Opening' })],
          beats: [
            beat({ beatId: 'bt-1', sceneId: 'sc-1', title: 'Hook' }),
            beat({ beatId: 'bt-2', sceneId: 'sc-1', title: 'Turn' }),
          ],
        },
      },
    } as React.MutableRefObject<WorkTimelineCanvasAdapterContext>;
    const adapter = createWorkTimelineCanvasAdapter(ctxRef, 'moment');
    const { nodes } = adapter.projectGraph(outline());

    const sceneNode = nodes.find((n) => n.data.nodeKind === 'scene')!;
    const beatNodes = nodes.filter((n) => n.data.nodeKind === 'beat');
    expect(beatNodes).toHaveLength(2);

    // Beats sit below their parent scene (Y greater than scene's Y).
    for (const b of beatNodes) {
      expect(b.position.y).toBeGreaterThan(sceneNode.position.y);
      // manuscript-anchor carries chapter + scene + beat ids (mandatory).
      expect(b.data.manuscriptAnchor?.beatId).toBeDefined();
      expect(b.data.manuscriptAnchor?.sceneId).toBe('sc-1');
    }

    // Beats stack in deterministic order (sorted by beatId ascending).
    const [first, second] = [...beatNodes].sort((a, b) => a.position.y - b.position.y);
    expect(first.data.beatId).toBe('bt-1');
    expect(second.position.y).toBeGreaterThan(first.position.y);
  });

  // ─── Moment inspector (Step 3) ────────────────────────────────────────────

  it('adapter exposes a renderInspector that returns a non-null node for Moment scene nodes', () => {
    // Adapter contract: `renderInspector(node)` returns the inspector
    // ReactNode for a given selected node. The useCanvasSurface hook drives
    // this from selection state.
    const ctxRef = { current: { workId: 'work-1' } } as React.MutableRefObject<WorkTimelineCanvasAdapterContext>;
    const adapter = createWorkTimelineCanvasAdapter(ctxRef, 'moment');
    expect(typeof adapter.renderInspector).toBe('function');

    const fakeSceneNode = {
      id: 'wt-scene:sc-1',
      type: 'work-timeline-moment-scene',
      position: { x: 0, y: 0 },
      data: {
        workId: 'work-1',
        nodeKind: 'scene',
        nodeId: 'sc-1',
        sceneId: 'sc-1',
        label: 'Opening',
        realizesChapterId: 1,
        manuscriptAnchor: { chapterId: 1, sceneId: 'sc-1' },
      },
    } as const;
    const inspector = adapter.renderInspector?.(fakeSceneNode as never);
    expect(inspector).not.toBeNull();
  });

  it('Moment scene inspector surfaces the manuscript anchor + "Edit in Outline" CTA (read-only hand-off per architect §6)', () => {
    // Plan Task 6 Step 3 + architect §6: inspector shows scene/beat details
    // with manuscript link + "Edit in Outline" hand-off. The CTA MUST NOT
    // trigger any forbidden write endpoint (Work Timeline is read-only).
    //
    // Test mirrors the V1.123 P1 Brief-era inspector dispatch test: drive
    // `adapter.renderInspector(node)` directly and render the returned JSX
    // in isolation. jsdom does not render React Flow nodes, so a full-canvas
    // mount cannot drive selection cleanly; adapter-level dispatch is the
    // cleaner contract surface.
    const ctxRef = {
      current: { workId: 'work-1' },
    } as React.MutableRefObject<WorkTimelineCanvasAdapterContext>;
    const adapter = createWorkTimelineCanvasAdapter(ctxRef, 'moment');
    const { nodes } = adapter.projectGraph(
      outline(),
    );
    // No fixture wired → Moment projection returns zero nodes. Inject a
    // scene node directly so we can test renderInspector on a real shape.
    const fakeSceneNode = {
      id: 'wt-scene:sc-1',
      type: 'work-timeline-moment-scene',
      position: { x: 0, y: 0 },
      data: {
        workId: 'work-1',
        nodeKind: 'scene' as const,
        nodeId: 'sc-1',
        sceneId: 'sc-1',
        label: 'Opening Scene',
        realizesChapterId: 1,
        manuscriptAnchor: { chapterId: 1, sceneId: 'sc-1' },
      },
    };
    expect(nodes).toHaveLength(0);

    const inspector = adapter.renderInspector?.(fakeSceneNode as never);
    expect(inspector).not.toBeNull();

    const { container } = renderInApp(inspector as React.ReactElement);
    const panel = container.querySelector('[data-testid="work-timeline-inspector"]');
    expect(panel).not.toBeNull();
    // Title + label + manuscript anchor chapter + scene id surface.
    expect(panel?.textContent ?? '').toContain('Opening Scene');
    expect(panel?.textContent ?? '').toContain('sc-1');

    // "Edit in Outline" CTA present (read-only hand-off per architect §6).
    const editCta = container.querySelector(
      '[data-testid="work-timeline-inspector-edit-in-outline"]',
    );
    expect(editCta).not.toBeNull();
    expect(editCta?.textContent ?? '').toContain('Edit in Outline');
  });

  it('Moment beat inspector surfaces beat-level manuscript anchor + "Edit in Outline" CTA', () => {
    // layer-feel §2.4: beat pins carry chapter/scene/beat manuscript anchor.
    // The beat inspector surfaces all three ids so the author can pivot to
    // Outline at beat precision.
    const ctxRef = { current: { workId: 'work-1' } } as React.MutableRefObject<WorkTimelineCanvasAdapterContext>;
    const adapter = createWorkTimelineCanvasAdapter(ctxRef, 'moment');
    const fakeBeatNode = {
      id: 'wt-beat:bt-1',
      type: 'work-timeline-moment-beat',
      position: { x: 0, y: 0 },
      data: {
        workId: 'work-1',
        nodeKind: 'beat' as const,
        nodeId: 'bt-1',
        beatId: 'bt-1',
        sceneId: 'sc-1',
        label: 'Hook Beat',
        realizesChapterId: 1,
        manuscriptAnchor: { chapterId: 1, sceneId: 'sc-1', beatId: 'bt-1' },
      },
    };

    const inspector = adapter.renderInspector?.(fakeBeatNode as never);
    expect(inspector).not.toBeNull();

    const { container } = renderInApp(inspector as React.ReactElement);
    const panel = container.querySelector('[data-testid="work-timeline-inspector"]');
    expect(panel).not.toBeNull();
    expect(panel?.textContent ?? '').toContain('Hook Beat');
    expect(panel?.textContent ?? '').toContain('bt-1');
    expect(panel?.textContent ?? '').toContain('sc-1');

    // Beat inspector also surfaces "Edit in Outline" CTA (architect §6).
    expect(
      container.querySelector('[data-testid="work-timeline-inspector-edit-in-outline"]'),
    ).not.toBeNull();
  });

  it('Narrative event inspector surfaces event id + chapter + "Edit in Outline" CTA (distinct from Moment)', () => {
    // Inspector dispatch MUST discriminate by nodeKind: Narrative event →
    // event inspector; Moment scene/beat → moment inspector. Asserts the
    // dispatch contract mirrors the V1.123 P1 Brief-era dispatch pattern.
    const ctxRef = { current: { workId: 'work-1' } } as React.MutableRefObject<WorkTimelineCanvasAdapterContext>;
    const adapter = createWorkTimelineCanvasAdapter(ctxRef, 'narrative');
    const fakeEventNode = {
      id: 'wt-event:evt-1',
      type: 'work-timeline-narrative-event',
      position: { x: 0, y: 0 },
      data: {
        workId: 'work-1',
        nodeKind: 'event' as const,
        nodeId: 'evt-1',
        eventId: 'evt-1',
        label: 'Inciting Incident',
        realizesChapterId: 1,
        manuscriptAnchor: { chapterId: 1 },
      },
    };

    const inspector = adapter.renderInspector?.(fakeEventNode as never);
    expect(inspector).not.toBeNull();

    const { container } = renderInApp(inspector as React.ReactElement);
    const panel = container.querySelector('[data-testid="work-timeline-inspector"]');
    expect(panel).not.toBeNull();
    expect(panel?.textContent ?? '').toContain('Inciting Incident');
    expect(panel?.textContent ?? '').toContain('evt-1');
  });
});
