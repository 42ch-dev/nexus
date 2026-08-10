/**
 * WorkTimelineCanvas — V1.123 P2 Task 7 (honest Moment-layer empty-state).
 *
 * Verifies the per-layer empty-state contract locked by:
 *   - `iterations/v1.123/specs/three-layer-architecture.md` §7 (honest
 *     empty-state per layer).
 *   - `iterations/v1.123/specs/layer-feel-differentiation.md` §2.4 + §7
 *     (Moment empty-state copy: "No scene or beat data yet" + CTA toward
 *     Narrative / Outline).
 *   - Plan `2026-07-18-v1.123-work-timeline-narrative-moment.md` Global
 *     Constraints: "Moment empty (no scene/beat data) → fallback to Narrative
 *     with explanation; Narrative empty → today's empty-state equivalent."
 *
 * Coverage:
 *   - Outline with events, no fixture → default Narrative renders events.
 *   - User switches to Moment → Moment-empty panel renders with i18n title +
 *     body + CTA. The empty panel is testable via `work-timeline-moment-empty-state`.
 *   - Clicking the CTA flips back to Narrative (escape hatch).
 *   - Outline globally empty → V1.122-style global empty-state preserves;
 *     the Moment-empty panel MUST NOT render on the global-empty branch.
 *   - V1.156 P2 T2 — Brief-empty panel: no bound World / no era data →
 *     honest empty-state (PD-2 copy: world-shape context comes from the
 *     bound World's Brief; NO "create Brief" CTA); CTA flips back to
 *     Narrative; era data present → canvas renders (no crash on Brief-era
 *     nodes); global-empty branch → Brief panel MUST NOT render.
 *
 * Mount strategy mirrors `layer-switcher.test.tsx`: a mocked `NexusClient`
 * resolves `getWorkOutline` to a per-test fixture; MSW is not needed because
 * the client mock intercepts before HTTP.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, screen, waitFor } from '@testing-library/react';

import { renderInApp } from '@/test/test-providers';
import type { NexusClient } from '@/lib/nexus';
import type {
  WorldKbEntityProjection,
  WorldKbGraphResponse,
  WorkOutline,
} from '@42ch/nexus-contracts';

import { WorkTimelineCanvas } from '../work-timeline-canvas';

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

function worldEntity(
  overrides: Partial<WorldKbEntityProjection> &
    Pick<WorldKbEntityProjection, 'key_block_id' | 'block_type' | 'canonical_name'>,
): WorldKbEntityProjection {
  return {
    world_id: 'world-es',
    status: 'confirmed',
    version: 1,
    ...overrides,
  } as WorldKbEntityProjection;
}

function eraEntity(
  overrides: Partial<WorldKbEntityProjection> &
    Pick<WorldKbEntityProjection, 'key_block_id' | 'canonical_name'>,
): WorldKbEntityProjection {
  const { key_block_id, canonical_name, body, ...rest } = overrides;
  return worldEntity({
    key_block_id,
    block_type: 'era',
    canonical_name,
    body: body ?? {
      attributes: {
        era_id: 'era-1',
        start_hint: '1000-01-01T00:00:00Z',
        end_hint: '1100-01-01T00:00:00Z',
        world_summary: 'The First Age',
      },
    },
    ...rest,
  });
}

/**
 * Mock client for the Work Timeline surface.
 *
 * `world` is optional (V1.156 P2 T2 — Work-Brief era data): when supplied,
 * the Work detail carries a bound `world_id` and `getWorldKbGraph` resolves
 * the bound World's graph (the Brief layer's era data source). When absent,
 * the Work is unbound → `useWorldKbGraph` is disabled → the Brief layer has
 * no era data (honest Brief-empty panel).
 */
function makeMockClient(
  outlineData: WorkOutline,
  world?: { worldId: string; graph: WorldKbGraphResponse },
): NexusClient {
  return {
    getWorkOutline: vi.fn().mockResolvedValue(outlineData),
    getWork: vi.fn().mockResolvedValue({
      work_id: 'work-1',
      world_id: world?.worldId ?? null,
    }),
    getWorldKbGraph: world ? vi.fn().mockResolvedValue(world.graph) : vi.fn(),
    patchOutlineStructure: vi.fn(),
    patchOutlineChapter: vi.fn(),
    patchTimelineEvent: vi.fn(),
    health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
  } as unknown as NexusClient;
}

// ─── Honest empty-state per layer ──────────────────────────────────────────

describe('WorkTimelineCanvas — global empty-state preserved (V1.122 family)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('renders the global empty-state when the outline has zero events (Narrative empty)', async () => {
    // Plan Global Constraints: "Narrative empty (no events) → today's
    // empty-state equivalent." This is the V1.122 family regression — the
    // surface MUST NOT surface the Moment-empty panel on the global-empty
    // branch (different copy, different CTA).
    const client = makeMockClient(outline({ timeline_events: [] }));
    renderInApp(<WorkTimelineCanvas workId="work-1" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });

    // Global empty-state surfaces (defaultValue surfaces in the DOM).
    expect(screen.getByText("This work's timeline is empty")).toBeInTheDocument();

    // The Moment-empty panel MUST NOT render.
    expect(screen.queryByTestId('work-timeline-moment-empty-state')).toBeNull();
    // Layer switcher is hidden on the empty branch (Task 4 contract).
    expect(screen.queryByTestId('work-timeline-layer-tab-moment')).toBeNull();
  });
});

describe('WorkTimelineCanvas — Moment-layer honest empty-state (V1.123 P2 Task 7)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('does NOT render the Moment-empty panel when default layer is Narrative (events present, no fixture)', async () => {
    // Architect §7.3: default = Narrative unconditionally in V1.123. Even
    // when no fixture is wired, the default Narrative view MUST NOT surface
    // the Moment-empty panel — the panel only fires when the user has
    // explicitly switched to Moment (mirrors the V1.123 P1 Brief-empty
    // contract).
    const client = makeMockClient(outline());
    renderInApp(<WorkTimelineCanvas workId="work-1" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });

    // Default = Narrative; no Moment-empty panel.
    expect(screen.queryByTestId('work-timeline-moment-empty-state')).toBeNull();
  });

  it('renders the Moment-empty panel when the user switches to Moment with no fixture', async () => {
    // layer-feel §2.4 + §7 + plan Task 7: when the active layer is Moment
    // AND the projection is empty (no scene/beat data), the surface renders
    // the Moment-empty panel with i18n copy + a CTA back to Narrative.
    const client = makeMockClient(outline());
    renderInApp(<WorkTimelineCanvas workId="work-1" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });

    // Default = Narrative.
    expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
      'data-active-layer',
      'narrative',
    );

    // Switch to Moment — projection is empty (no fixture) → Moment-empty panel.
    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-moment'));

    const momentEmpty = await screen.findByTestId('work-timeline-moment-empty-state');
    expect(momentEmpty).toBeInTheDocument();
    // Title (layer-feel §7): "No scene or beat data yet".
    expect(momentEmpty).toHaveTextContent('No scene or beat data yet');
    // CTA: "Switch to Narrative" — the actionable escape hatch.
    expect(momentEmpty).toHaveTextContent('Switch to Narrative');
  });

  it('clicking the Moment-empty CTA switches back to Narrative (escape hatch)', async () => {
    // layer-feel §2.4 empty + plan Task 7 Step 1: CTA button on the
    // Moment-empty panel switches to Narrative. This is the escape hatch
    // from a Moment-empty Work.
    const client = makeMockClient(outline());
    renderInApp(<WorkTimelineCanvas workId="work-1" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });

    // Switch to Moment → Moment-empty panel renders.
    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-moment'));
    const cta = await screen.findByTestId('work-timeline-moment-empty-cta');
    expect(cta).toBeInTheDocument();

    // Click the CTA → active layer flips back to Narrative.
    fireEvent.click(cta);
    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });

    // The Moment-empty panel is gone once Narrative is active.
    expect(screen.queryByTestId('work-timeline-moment-empty-state')).toBeNull();
  });

  it('does NOT render the Moment-empty panel when a scene/beat fixture provides data', async () => {
    // When the fixture carries scenes/beats, the Moment projection emits
    // nodes and the surface renders the canvas — NOT the Moment-empty panel.
    const client = makeMockClient(outline());
    renderInApp(
      <WorkTimelineCanvas
        workId="work-1"
        sceneBeatFixture={{
          scenes: [
            {
              sceneId: 'sc-1',
              chapterId: 1,
              title: 'Opening',
              status: null,
            },
          ],
          beats: [],
        }}
      />,
      { client },
    );

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });

    // Switch to Moment — fixture has data → canvas renders, NOT the panel.
    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-moment'));

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'moment',
      );
    });
    expect(screen.queryByTestId('work-timeline-moment-empty-state')).toBeNull();
  });
});

// ─── Work-Brief honest empty-state (V1.156 P2 T2 — PD-2) ─────────────────

describe('WorkTimelineCanvas — Brief-layer honest empty-state (V1.156 P2 T2)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('renders the Brief-empty panel when the user switches to Brief with no bound World', async () => {
    // PD-2 + spec §3.3.3 empty-state honesty: Brief on the Work Timeline is
    // a read-only projection of the bound World's Brief. A Work with no
    // bound World has no world-shape context → honest empty-state panel
    // with a CTA back to Narrative. NO "create Brief" CTA — the Work does
    // NOT own Brief authoring (Brief is World spine).
    const client = makeMockClient(outline());
    renderInApp(<WorkTimelineCanvas workId="work-1" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });

    // Default = Narrative (architect §7.3); switch to Brief.
    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-brief'));

    const briefEmpty = await screen.findByTestId('work-timeline-brief-empty-state');
    expect(briefEmpty).toBeInTheDocument();
    // Title: world-shape context comes from the bound World.
    expect(briefEmpty).toHaveTextContent('No world-shape context yet');
    // Copy explains the projection source (bound World's Brief).
    expect(briefEmpty).toHaveTextContent('bound to a World with era markers');
    // CTA: "Switch to Narrative" — the actionable escape hatch.
    expect(briefEmpty).toHaveTextContent('Switch to Narrative');
    // NO Work-owned Brief authoring CTA (PD-2).
    expect(briefEmpty).not.toHaveTextContent('Create Brief');
    expect(briefEmpty).not.toHaveTextContent('Add era');
  });

  it('renders the Brief-empty panel when the bound World has no era markers', async () => {
    // Work IS bound to a World, but the World's KB graph has no
    // `block_type=era` entities → the Brief projection is empty → honest
    // empty-state panel (world-shape context appears when the bound World
    // has era markers).
    const client = makeMockClient(outline(), {
      worldId: 'world-es',
      graph: {
        entities: [
          worldEntity({
            key_block_id: 'kb-event-1',
            block_type: 'event',
            canonical_name: 'Coronation',
          }),
        ],
        source_anchors: [],
        relationships: [],
      },
    });
    renderInApp(<WorkTimelineCanvas workId="work-1" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-brief'));

    const briefEmpty = await screen.findByTestId('work-timeline-brief-empty-state');
    expect(briefEmpty).toBeInTheDocument();
    expect(briefEmpty).toHaveTextContent('No world-shape context yet');
  });

  it('clicking the Brief-empty CTA switches back to Narrative (escape hatch)', async () => {
    const client = makeMockClient(outline());
    renderInApp(<WorkTimelineCanvas workId="work-1" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });

    // Switch to Brief → Brief-empty panel renders.
    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-brief'));
    const cta = await screen.findByTestId('work-timeline-brief-empty-cta');
    expect(cta).toBeInTheDocument();

    // Click the CTA → active layer flips back to Narrative.
    fireEvent.click(cta);
    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });

    // The Brief-empty panel is gone once Narrative is active.
    expect(screen.queryByTestId('work-timeline-brief-empty-state')).toBeNull();
  });

  it('does NOT render the Brief-empty panel when the bound World has era data', async () => {
    // Work bound to a World WITH era markers → the Brief projection emits
    // era nodes and the canvas renders — NOT the empty-state panel. This
    // also exercises the full render path (CanvasShell + summarize + NLE
    // band) on Brief-era nodes (P1 fix-wave lesson: no crash on Brief-era
    // nodes in alt-view/summarize paths).
    const client = makeMockClient(outline(), {
      worldId: 'world-es',
      graph: {
        entities: [
          eraEntity({
            key_block_id: 'kb-era-1',
            canonical_name: 'The First Age',
          }),
        ],
        source_anchors: [],
        relationships: [],
      },
    });
    renderInApp(<WorkTimelineCanvas workId="work-1" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });

    // Switch to Brief — era data exists → canvas renders, NOT the panel.
    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-brief'));

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'brief',
      );
    });
    expect(screen.queryByTestId('work-timeline-brief-empty-state')).toBeNull();
    // The canvas shell renders (no crash on Brief-era nodes).
    expect(
      screen.queryByTestId('work-timeline-canvas-layer-transition'),
    ).not.toBeNull();
  });

  it('does NOT render the Brief-empty panel on the global-empty branch', async () => {
    // Outline has zero events → the global empty-state branch owns the
    // surface (V1.122 family — preserved). Even with ?layer=brief in the
    // URL, the global empty-state renders and the Brief-empty panel MUST
    // NOT appear (mirrors the Moment-empty gate on the global branch).
    const client = makeMockClient(outline({ timeline_events: [] }));
    renderInApp(<WorkTimelineCanvas workId="work-1" />, {
      client,
      initialRouterEntries: ['/works/work-1/timeline?layer=brief'],
    });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });

    expect(screen.getByText("This work's timeline is empty")).toBeInTheDocument();
    expect(screen.queryByTestId('work-timeline-brief-empty-state')).toBeNull();
    // Layer switcher is hidden on the empty branch (Task 4 contract).
    expect(screen.queryByTestId('work-timeline-layer-tab-brief')).toBeNull();
  });
});
