/**
 * WorkTimelineCanvas — V1.123 P2 Task 4 (layer switcher + default-layer logic).
 *
 * Verifies the Narrative↔Moment layer switcher UI + the Work-entry default
 * layer logic locked by:
 *   - `iterations/v1.123/specs/three-layer-architecture.md` §7.3 (UX-risk
 *     override — default = 'narrative' UNCONDITIONALLY in V1.123 because
 *     the V1.72 WorkOutline wire has no Scene/Beat data today).
 *   - `iterations/v1.123/specs/layer-feel-differentiation.md` §3.3
 *     (Narrative ↔ Moment explicit layer control via header tabs).
 *   - Plan `2026-07-18-v1.123-work-timeline-narrative-moment.md` Task 4 +
 *    Global Constraints ("Architect UX-risk override (LOCKED §7.3):
 *    `defaultLayer: 'narrative'` — NOT Moment.").
 *
 * Coverage:
 *   - Narrative + Moment tabs render in the canvas header (layer-feel §3.3).
 *   - Default layer = `'narrative'` UNCONDITIONALLY (architect §7.3
 *     override — even when scene/beat fixture data exists, the Work
 *     Timeline default is Narrative in V1.123 because real Works have no
 *     wire-scene/beat data today).
 *   - Clicking Moment tab switches active layer to Moment.
 *   - Clicking Narrative tab switches active layer to Narrative.
 *   - Switching layers is a discrete semantic swap (layer-feel §3.1).
 *   - Switching layers does NOT trigger any forbidden write endpoint
 *     (Work Timeline is read-only in V1.123 — `patchOutlineChapter`,
 *     `patchOutlineStructure`, `patchTimelineEvent` all stay unset during
 *     a Narrative→Moment→Narrative swap).
 *
 * Mount strategy mirrors `timeline-canvas/__tests__/layer-switcher.test.tsx`:
 * a mocked `NexusClient` resolves `getWorkOutline` to a per-test fixture,
 * and every forbidden write method is spied for negative assertions. The
 * TanStack Query hook drives the projection through `useCanvasSurface`;
 * MSW is not needed because the client mock intercepts before HTTP.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, screen, waitFor } from '@testing-library/react';

import { renderInApp } from '@/test/test-providers';
import type { NexusClient } from '@/lib/nexus';
import type { WorkOutline } from '@42ch/nexus-contracts';
import type {
  BeatFixture,
  SceneFixture,
} from '../../outline-canvas/graph-projection';

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

function makeMockClient(outlineData: WorkOutline): NexusClient {
  return {
    getWorkOutline: vi.fn().mockResolvedValue(outlineData),
    patchOutlineStructure: vi.fn(),
    patchOutlineChapter: vi.fn(),
    patchTimelineEvent: vi.fn(),
    health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
  } as unknown as NexusClient;
}

// ─── Layer switcher UI (Narrative ↔ Moment) ───────────────────────────────

describe('WorkTimelineCanvas — layer switcher UI (V1.123 P2 Task 4)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('renders Narrative + Moment layer tabs in the canvas header', async () => {
    const client = makeMockClient(outline());
    renderInApp(<WorkTimelineCanvas workId="work-1" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });

    // Both layer tabs render — explicit layer control per layer-feel §3.3.
    expect(screen.getByTestId('work-timeline-layer-tab-narrative')).toBeInTheDocument();
    expect(screen.getByTestId('work-timeline-layer-tab-moment')).toBeInTheDocument();
  });

  it("defaults to 'narrative' layer (architect §7.3 UX-risk override — UNCONDITIONAL in V1.123)", async () => {
    const client = makeMockClient(outline());
    renderInApp(<WorkTimelineCanvas workId="work-1" />, { client });

    const canvas = await screen.findByTestId('work-timeline-canvas');

    // Architect §7.3: default = 'narrative' regardless of scene/beat data.
    // The Narrative tab is pressed and the container's `data-active-layer`
    // mirror reads 'narrative'.
    const narrativeTab = screen.getByTestId('work-timeline-layer-tab-narrative');
    expect(narrativeTab).toHaveAttribute('aria-pressed', 'true');
    expect(canvas).toHaveAttribute('data-active-layer', 'narrative');
    // Moment tab is NOT pressed.
    expect(screen.getByTestId('work-timeline-layer-tab-moment')).toHaveAttribute(
      'aria-pressed',
      'false',
    );
  });

  it("defaults to 'narrative' even when the canvas injects a scene/beat fixture (override is UNCONDITIONAL)", async () => {
    // Architect §7.3: the override is unconditional because real Works
    // have no wire-scene/beat data today. Even when a fixture is wired
    // (Design Studio / test), the V1.123 default stays 'narrative' so
    // the surface does not flip its UX between real Works and test
    // fixtures. The default may flip to Moment in V1.124+ once the wire
    // exposes scenes/beats.
    const client = makeMockClient(outline());
    renderInApp(
      <WorkTimelineCanvas
        workId="work-1"
        sceneBeatFixture={{
          scenes: [scene({ sceneId: 'sc-1', chapterId: 1 })],
          beats: [beat({ beatId: 'bt-1', sceneId: 'sc-1' })],
        }}
      />,
      { client },
    );

    const canvas = await screen.findByTestId('work-timeline-canvas');

    expect(canvas).toHaveAttribute('data-active-layer', 'narrative');
    expect(screen.getByTestId('work-timeline-layer-tab-narrative')).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    expect(screen.getByTestId('work-timeline-layer-tab-moment')).toHaveAttribute(
      'aria-pressed',
      'false',
    );
  });

  it('clicking Moment tab switches active layer to Moment (Narrative → Moment)', async () => {
    const client = makeMockClient(outline());
    renderInApp(<WorkTimelineCanvas workId="work-1" />, { client });

    const canvas = await screen.findByTestId('work-timeline-canvas');

    // Default = Narrative.
    await waitFor(() => {
      expect(canvas).toHaveAttribute('data-active-layer', 'narrative');
    });

    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-moment'));

    await waitFor(() => {
      expect(canvas).toHaveAttribute('data-active-layer', 'moment');
    });
    expect(screen.getByTestId('work-timeline-layer-tab-moment')).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    expect(screen.getByTestId('work-timeline-layer-tab-narrative')).toHaveAttribute(
      'aria-pressed',
      'false',
    );
  });

  it('clicking Narrative tab switches active layer back to Narrative (Moment → Narrative)', async () => {
    const client = makeMockClient(outline());
    renderInApp(<WorkTimelineCanvas workId="work-1" />, { client });

    const canvas = await screen.findByTestId('work-timeline-canvas');

    // Default → Narrative; user clicks Moment then back to Narrative.
    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-moment'));
    await waitFor(() => {
      expect(canvas).toHaveAttribute('data-active-layer', 'moment');
    });

    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-narrative'));
    await waitFor(() => {
      expect(canvas).toHaveAttribute('data-active-layer', 'narrative');
    });
    expect(screen.getByTestId('work-timeline-layer-tab-narrative')).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    expect(screen.getByTestId('work-timeline-layer-tab-moment')).toHaveAttribute(
      'aria-pressed',
      'false',
    );
  });

  it('hides the layer switcher from the empty-state branch (no events to show)', async () => {
    // Architect §7 + Task 7: when the outline has zero events, the
    // empty-state panel owns the surface (Task 7 owns the visible copy;
    // Task 4 hides the switcher so it does not add noise to the empty
    // branch — mirrors the V1.122 Timeline layer-switcher gate).
    const client = makeMockClient(
      outline({ timeline_events: [] }),
    );
    renderInApp(<WorkTimelineCanvas workId="work-1" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
    });

    expect(screen.queryByTestId('work-timeline-layer-tab-narrative')).toBeNull();
    expect(screen.queryByTestId('work-timeline-layer-tab-moment')).toBeNull();
  });

  it('does NOT trigger any forbidden write endpoint during a layer swap (read-only surface)', async () => {
    // Architect §6: Work Timeline is read-only in V1.123. The layer
    // swap is a pure projection swap; no write path is wired. The
    // orchestrator does not own any mutation hook here (unlike V1.122
    // Timeline which wires `usePatchWorldKbEntity`). The negative
    // assertion mirrors the V1.122 Timeline write-boundary test
    // pattern, adapted for the V1.72 outline write paths.
    const client = makeMockClient(outline());
    renderInApp(<WorkTimelineCanvas workId="work-1" />, { client });

    await screen.findByTestId('work-timeline-canvas');

    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-moment'));
    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-narrative'));
    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-moment'));

    // Every outline write path stays unset during a three-step layer swap.
    expect((client.patchOutlineStructure as ReturnType<typeof vi.fn>)).not.toHaveBeenCalled();
    expect((client.patchOutlineChapter as ReturnType<typeof vi.fn>)).not.toHaveBeenCalled();
    expect((client.patchTimelineEvent as ReturnType<typeof vi.fn>)).not.toHaveBeenCalled();
  });
});
