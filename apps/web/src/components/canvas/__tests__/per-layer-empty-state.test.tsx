/**
 * Per-layer honest empty-state copy — V1.123 P4 Task 7.
 *
 * Pins the honest empty-state copy contract from
 * `iterations/v1.123/specs/layer-feel-differentiation.md` §7 + plan Task 7:
 *
 *   | Layer | Empty title (EN intent)        | Empty body (EN intent)              |
 *   |-------|--------------------------------|-------------------------------------|
 *   | Brief | No era markers yet             | Brief shows the world's shape across
 *   |       |                                | ages. Switch to Narrative to browse
 *   |       |                                | events, or add era markers when the
 *   |       |                                | Brief carrier is ready.             |
 *   | Moment| No scene or beat data yet      | Moment is scene-precise and         |
 *   |       |                                | manuscript-anchored. Add scenes and |
 *   |       |                                | beats in Outline, or switch to      |
 *   |       |                                | Narrative for events.               |
 *
 * The Brief title + body together convey the spec's intent
 * "No era markers yet — switch to Narrative to see events." The Moment
 * title + body convey "No scene/beat data yet — switch to Narrative to
 * see events." The CTA always says "Switch to Narrative" (en) /
 * "切换到叙事" (zh-CN) for both surfaces.
 *
 * Coverage:
 *   - Brief empty-state renders on the World Timeline when the user
 *     clicks the Brief tab on a World with non-era entities but no eras.
 *     The rendered title + body + CTA match the i18n strings in both
 *     `en` and `zh-CN`.
 *   - Moment empty-state renders on the Work Timeline when the user
 *     clicks the Moment tab without a scene/beat fixture. Same i18n
 *     parity assertion.
 *
 * The test mounts each canvas orchestrator (not the empty-state component
 * directly) so the wiring through the orchestrator's empty-detection branch
 * is exercised end-to-end. The canvas orchestrator owns the layer swap that
 * triggers the empty-state branch — the empty-state component itself only
 * renders within the orchestrator's render tree.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, screen, waitFor } from '@testing-library/react';

import { renderInApp } from '@/test/test-providers';
import { i18n } from '@/lib/i18n/config';
import type { NexusClient } from '@/lib/nexus';
import type {
  WorldKbEntityProjection,
  WorldKbGraphResponse,
  WorkOutline,
} from '@42ch/nexus-contracts';

import { TimelineCanvas } from '../timeline-canvas/timeline-canvas';
import { WorkTimelineCanvas } from '../work-timeline-canvas/work-timeline-canvas';

// ─── Fixture builders ──────────────────────────────────────────────────────

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

function makeWorldMockClient(graph: WorldKbGraphResponse): NexusClient {
  return {
    getWorldKbGraph: vi.fn().mockResolvedValue(graph),
    worldKbPatchEntity: vi.fn(),
    worldKbPatchRelationship: vi.fn(),
    worldKbPromoteCandidate: vi.fn(),
    patchTimelineEvent: vi.fn(),
    patchOutlineStructure: vi.fn(),
    patchOutlineChapter: vi.fn(),
    getWorks: vi.fn().mockResolvedValue({ items: [], total: 0 }),
    health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
  } as unknown as NexusClient;
}

function makeWorkMockClient(outline: WorkOutline): NexusClient {
  return {
    getWorkOutline: vi.fn().mockResolvedValue(outline),
    getWork: vi.fn().mockResolvedValue({ work_id: 'work-1', world_id: null }),
    health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
  } as unknown as NexusClient;
}

async function setI18nLanguage(language: 'en' | 'zh-CN') {
  // Pre-set the localStorage preference so the LocaleProvider (mounted via
  // renderInApp) resolves to the target language. The provider's mount
  // effect calls `i18n.changeLanguage(resolvedLocale)` based on the stored
  // preference, so changing the i18n singleton alone is not enough — the
  // provider's effect would override it.
  if (typeof window !== 'undefined') {
    window.localStorage.setItem('nexus-web-locale', language);
  }
  await i18n.changeLanguage(language);
}

// ─── Brief empty-state — World Timeline ───────────────────────────────────

describe('TimelineCanvas — Brief empty-state copy (P4 Task 7)', () => {
  afterEach(async () => {
    vi.clearAllMocks();
    await setI18nLanguage('en');
  });

  it('renders the honest Brief-empty copy + Switch to Narrative CTA in English', async () => {
    // Graph has events but zero eras → Brief layer is empty when the user
    // explicitly switches to it (default layer is Narrative without era data).
    const graph: WorldKbGraphResponse = {
      entities: [
        worldEntity({
          key_block_id: 'kb-event-1',
          block_type: 'event',
          canonical_name: 'Coronation',
          body: { attributes: { occurred_at: '1042-03-01T00:00:00Z' } },
        }),
      ],
      source_anchors: [],
      relationships: [],
    };
    renderInApp(<TimelineCanvas worldId="world-es" />, {
      client: makeWorldMockClient(graph),
    });

    // Default layer is Narrative (no era data).
    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });

    // User switches to Brief → empty-state branch fires.
    fireEvent.click(screen.getByTestId('timeline-layer-tab-brief'));
    const emptyState = await screen.findByTestId('timeline-brief-empty-state');
    expect(emptyState).toBeInTheDocument();

    // Title conveys "no eras yet"; body conveys the Brief purpose + an escape
    // hatch back to Narrative. Per layer-feel §7, the body must mention
    // "Switch to Narrative" so the user has an actionable next step.
    expect(emptyState).toHaveTextContent('No era markers yet');
    expect(emptyState).toHaveTextContent('Switch to Narrative');

    // CTA is the primary affordance — same label as the body's escape hatch.
    const cta = screen.getByTestId('timeline-brief-empty-cta');
    expect(cta).toHaveTextContent('Switch to Narrative');

    // CTA click swaps back to Narrative — escape hatch actually works.
    fireEvent.click(cta);
    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });
  });

  it('renders Brief-empty copy in zh-CN with parity', async () => {
    await setI18nLanguage('zh-CN');

    const graph: WorldKbGraphResponse = {
      entities: [
        worldEntity({
          key_block_id: 'kb-event-1',
          block_type: 'event',
          canonical_name: '加冕礼',
          body: { attributes: { occurred_at: '1042-03-01T00:00:00Z' } },
        }),
      ],
      source_anchors: [],
      relationships: [],
    };
    renderInApp(<TimelineCanvas worldId="world-es" />, {
      client: makeWorldMockClient(graph),
    });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });

    fireEvent.click(screen.getByTestId('timeline-layer-tab-brief'));
    const emptyState = await screen.findByTestId('timeline-brief-empty-state');

    // zh-CN parity: title is localized + body still references the Narrative
    // escape hatch (translated).
    expect(emptyState).toHaveTextContent('尚无纪元标记');
    expect(emptyState).toHaveTextContent('切换到叙事');

    const cta = screen.getByTestId('timeline-brief-empty-cta');
    expect(cta).toHaveTextContent('切换到叙事');
  });
});

// ─── Moment empty-state — Work Timeline ───────────────────────────────────

describe('WorkTimelineCanvas — Moment empty-state copy (P4 Task 7)', () => {
  afterEach(async () => {
    vi.clearAllMocks();
    await setI18nLanguage('en');
  });

  it('renders the honest Moment-empty copy + Switch to Narrative CTA in English', async () => {
    // Outline has events but no scene/beat fixture is supplied → Moment layer
    // renders zero nodes when the user switches to it.
    const outline: WorkOutline = {
      work_id: 'work-1',
      outline_revision: 1,
      volumes: [],
      timeline_events: [
        { event_id: 'evt-1', title: 'Inciting Incident', realizes_chapter_id: 1 },
      ],
      foreshadows: [],
      chapter_titles: {},
      updated_at: '2026-07-18T00:00:00Z',
    } as WorkOutline;

    renderInApp(<WorkTimelineCanvas workId="work-1" />, {
      client: makeWorkMockClient(outline),
    });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });

    // User switches to Moment → empty-state branch fires (no fixture).
    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-moment'));
    const emptyState = await screen.findByTestId('work-timeline-moment-empty-state');
    expect(emptyState).toBeInTheDocument();

    // Title conveys "no scene/beat data yet"; body conveys the Moment
    // purpose + escape hatch back to Narrative. Per layer-feel §7, the body
    // must reference switching to Narrative for events.
    expect(emptyState).toHaveTextContent('No scene or beat data yet');
    expect(emptyState).toHaveTextContent('Narrative');

    const cta = screen.getByTestId('work-timeline-moment-empty-cta');
    expect(cta).toHaveTextContent('Switch to Narrative');

    // CTA click swaps back to Narrative.
    fireEvent.click(cta);
    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });
  });

  it('renders Moment-empty copy in zh-CN with parity', async () => {
    await setI18nLanguage('zh-CN');

    const outline: WorkOutline = {
      work_id: 'work-1',
      outline_revision: 1,
      volumes: [],
      timeline_events: [
        { event_id: 'evt-1', title: '触发事件', realizes_chapter_id: 1 },
      ],
      foreshadows: [],
      chapter_titles: {},
      updated_at: '2026-07-18T00:00:00Z',
    } as WorkOutline;

    renderInApp(<WorkTimelineCanvas workId="work-1" />, {
      client: makeWorkMockClient(outline),
    });

    await waitFor(() => {
      expect(screen.getByTestId('work-timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });

    fireEvent.click(screen.getByTestId('work-timeline-layer-tab-moment'));
    const emptyState = await screen.findByTestId('work-timeline-moment-empty-state');

    // zh-CN parity: title + body + CTA translated.
    expect(emptyState).toHaveTextContent('尚无场景或节拍数据');
    expect(emptyState).toHaveTextContent('切换到叙事');

    const cta = screen.getByTestId('work-timeline-moment-empty-cta');
    expect(cta).toHaveTextContent('切换到叙事');
  });
});
