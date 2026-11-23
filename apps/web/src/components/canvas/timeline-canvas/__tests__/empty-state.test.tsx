/**
 *
 * Verifies the per-layer empty-state contract locked by
 *   - `iterations/v1.123/specs/three-layer-architecture.md` §7 (honest
 *     empty-state per layer).
 *   - `iterations/v1.123/specs/layer-feel-differentiation.md` §2.2 + §7
 *     (Brief empty-state copy: "No era markers yet — switch to Narrative
 *     to see events."; Narrative empty = V1.122 family preserved).
 *   - Plan `2026-07-18-v1.123-world-timeline-brief-narrative.md` Global
 *     Constraints: "Brief empty (no era markers/world summary) → fallback
 *     to Narrative with explanation; Narrative empty (no events) → today's
 *     V1.122 empty-state."
 *
 * Coverage:
 *   - World entry with no entities at all → V1.122 global empty-state
 *     (unchanged regression). The layer switcher is hidden (Batch A T3).
 *   - World entry with events but no `block_type=era` entities → defaults
 *     to Narrative layer (Batch A T3); user can click the Brief tab and
 *     see the Brief-empty panel with copy + CTA → clicking CTA returns to
 *     Narrative.
 *   - World entry with era entities → defaults to Brief; no empty-state
 *     panel renders.
 *   - The Brief-empty panel uses i18n keys (`timeline.brief.emptyState.*`)
 *     so en + zh-CN catalogs both surface the copy.
 *
 * Mount strategy mirrors `layer-switcher.test.tsx`: a mocked `NexusClient`
 * resolves `getWorldKbGraph` to a per-test fixture; MSW is not needed
 * because the client mock intercepts before HTTP.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, screen, waitFor, within } from '@testing-library/react';

import { renderInApp } from '@/test/test-providers';
import type { NexusClient } from '@/lib/nexus';
import type {
  WorldKbEntityProjection,
  WorldKbGraphResponse,
} from '@42ch/nexus-contracts';

import { TimelineCanvas } from '../timeline-canvas';

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

function eraEntity(
  overrides: Partial<WorldKbEntityProjection> &
    Pick<WorldKbEntityProjection, 'key_block_id' | 'canonical_name'>,
): WorldKbEntityProjection {
  const { key_block_id, canonical_name, body, ...rest } = overrides;
  return entity({
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

function makeMockClient(graph: WorldKbGraphResponse): NexusClient {
  return {
    getWorldKbGraph: vi.fn().mockResolvedValue(graph),
    worldKbPatchEntity: vi.fn(),
    worldKbPatchRelationship: vi.fn(),
    worldKbPromoteCandidate: vi.fn(),
    patchTimelineEvent: vi.fn(),
    patchOutlineStructure: vi.fn(),
    patchOutlineChapter: vi.fn(),
    listWorks: vi.fn().mockResolvedValue({
      items: [],
      pagination: { limit: 20, has_more: false },
    }),
    health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
  } as unknown as NexusClient;
}

describe('TimelineCanvas — V1.122 global empty-state preserved (zero entities)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('renders the V1.122 global empty-state when the graph has zero entities', async () => {
    const graph: WorldKbGraphResponse = {
      entities: [],
      source_anchors: [],
      relationships: [],
    };
    renderInApp(<TimelineCanvas worldId="world-7" />, {
      client: makeMockClient(graph),
    });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
    });

    expect(screen.getByText('This World\'s timeline is empty')).toBeInTheDocument();

    expect(screen.queryByTestId('timeline-brief-empty-state')).toBeNull();
    expect(screen.queryByTestId('timeline-layer-tab-brief')).toBeNull();
  });
});

describe('TimelineCanvas — Brief-layer empty-state (V1.123 P1 T5)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('does not render the Brief-empty panel when the default layer is Narrative (no era data)', async () => {
    const graph: WorldKbGraphResponse = {
      entities: [
        entity({
          key_block_id: 'kb-event-1',
          block_type: 'event',
          canonical_name: 'Coronation',
          body: { attributes: { occurred_at: '1042-03-01T00:00:00Z' } },
        }),
      ],
      source_anchors: [],
      relationships: [],
    };
    renderInApp(<TimelineCanvas worldId="world-7" />, {
      client: makeMockClient(graph),
    });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
    });

    expect(screen.queryByTestId('timeline-brief-empty-state')).toBeNull();
  });

  it('renders the Brief-empty panel when the user switches to Brief with no era data', async () => {
    const graph: WorldKbGraphResponse = {
      entities: [
        entity({
          key_block_id: 'kb-event-1',
          block_type: 'event',
          canonical_name: 'Coronation',
          body: { attributes: { occurred_at: '1042-03-01T00:00:00Z' } },
        }),
      ],
      source_anchors: [],
      relationships: [],
    };
    renderInApp(<TimelineCanvas worldId="world-7" />, {
      client: makeMockClient(graph),
    });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
    });

    expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
      'data-active-layer',
      'narrative',
    );

    fireEvent.click(screen.getByTestId('timeline-layer-tab-brief'));

    const briefEmpty = await screen.findByTestId('timeline-brief-empty-state');
    expect(briefEmpty).toBeInTheDocument();
    expect(briefEmpty).toHaveTextContent('No era markers yet');
    expect(briefEmpty).toHaveTextContent('Switch to Narrative');
  });

  it('clicking the Brief-empty CTA switches back to Narrative', async () => {
    const graph: WorldKbGraphResponse = {
      entities: [
        entity({
          key_block_id: 'kb-event-1',
          block_type: 'event',
          canonical_name: 'Coronation',
          body: { attributes: { occurred_at: '1042-03-01T00:00:00Z' } },
        }),
      ],
      source_anchors: [],
      relationships: [],
    };
    renderInApp(<TimelineCanvas worldId="world-7" />, {
      client: makeMockClient(graph),
    });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('timeline-layer-tab-brief'));
    const cta = await screen.findByTestId('timeline-brief-empty-cta');
    expect(cta).toBeInTheDocument();

    fireEvent.click(cta);
    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });

    expect(screen.queryByTestId('timeline-brief-empty-state')).toBeNull();
  });

  it('does not render the Brief-empty panel when era data exists (Brief default)', async () => {
    const graph: WorldKbGraphResponse = {
      entities: [
        eraEntity({
          key_block_id: 'kb-era-1',
          canonical_name: 'The First Age',
        }),
      ],
      source_anchors: [],
      relationships: [],
    };
    renderInApp(<TimelineCanvas worldId="world-7" />, {
      client: makeMockClient(graph),
    });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
    });

    expect(screen.queryByTestId('timeline-brief-empty-state')).toBeNull();
    expect(screen.queryByText('This World\'s timeline is empty')).toBeNull();
  });
});


describe('TimelineCanvas — Moment-layer honest empty-state (V1.156 P1 T2)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('does NOT render the Moment-empty panel on the default layer (Moment is never the default)', async () => {
    const graph: WorldKbGraphResponse = {
      entities: [
        eraEntity({
          key_block_id: 'kb-era-1',
          canonical_name: 'The First Age',
        }),
      ],
      source_anchors: [],
      relationships: [],
    };
    renderInApp(<TimelineCanvas worldId="world-7" />, {
      client: makeMockClient(graph),
    });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
    });

    expect(screen.queryByTestId('timeline-moment-empty-state')).toBeNull();
  });

  it('renders the Moment-empty panel when the user switches to Moment with no bound-Works fixture', async () => {
    const graph: WorldKbGraphResponse = {
      entities: [
        entity({
          key_block_id: 'kb-event-1',
          block_type: 'event',
          canonical_name: 'Coronation',
          body: { attributes: { occurred_at: '1042-03-01T00:00:00Z' } },
        }),
      ],
      source_anchors: [],
      relationships: [],
    };
    renderInApp(<TimelineCanvas worldId="world-7" />, {
      client: makeMockClient(graph),
    });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
    });

    expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
      'data-active-layer',
      'narrative',
    );

    fireEvent.click(screen.getByTestId('timeline-layer-tab-moment'));

    const momentEmpty = await screen.findByTestId('timeline-moment-empty-state');
    expect(momentEmpty).toBeInTheDocument();
    expect(momentEmpty).toHaveTextContent('bound Works');
    expect(momentEmpty).toHaveTextContent('Outline');
    expect(momentEmpty).toHaveTextContent('Switch to Narrative');
  });

  it('clicking the Moment-empty CTA switches back to Narrative (escape hatch)', async () => {
    const graph: WorldKbGraphResponse = {
      entities: [
        entity({
          key_block_id: 'kb-event-1',
          block_type: 'event',
          canonical_name: 'Coronation',
          body: { attributes: { occurred_at: '1042-03-01T00:00:00Z' } },
        }),
      ],
      source_anchors: [],
      relationships: [],
    };
    renderInApp(<TimelineCanvas worldId="world-7" />, {
      client: makeMockClient(graph),
    });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('timeline-layer-tab-moment'));
    const cta = await screen.findByTestId('timeline-moment-empty-cta');
    expect(cta).toBeInTheDocument();

    fireEvent.click(cta);
    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });

    expect(screen.queryByTestId('timeline-moment-empty-state')).toBeNull();
  });

  it('does NOT render the Moment-empty panel when a scene/beat fixture provides data', async () => {
    const graph: WorldKbGraphResponse = {
      entities: [
        entity({
          key_block_id: 'kb-event-1',
          block_type: 'event',
          canonical_name: 'Coronation',
        }),
      ],
      source_anchors: [],
      relationships: [],
    };
    renderInApp(
      <TimelineCanvas
        worldId="world-7"
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
      { client: makeMockClient(graph) },
    );

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('timeline-layer-tab-moment'));

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'moment',
      );
    });
    expect(screen.queryByTestId('timeline-moment-empty-state')).toBeNull();
  });

  it('surfaces NO World-owned Moment authoring CTA (PD-3 — read/projection only)', async () => {
    const graph: WorldKbGraphResponse = {
      entities: [
        entity({
          key_block_id: 'kb-event-1',
          block_type: 'event',
          canonical_name: 'Coronation',
        }),
      ],
      source_anchors: [],
      relationships: [],
    };
    renderInApp(<TimelineCanvas worldId="world-7" />, {
      client: makeMockClient(graph),
    });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('timeline-layer-tab-moment'));
    const momentEmpty = await screen.findByTestId('timeline-moment-empty-state');

    const buttons = within(momentEmpty).getAllByRole('button');
    expect(buttons).toHaveLength(1);
    expect(buttons[0]).toHaveTextContent('Switch to Narrative');
  });
});
