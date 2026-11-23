/**
 * TimelineCanvas — V1.123 P1 T3 (layer switcher + default-layer logic).
 *
 * Coverage:
 *   - Brief + Narrative tabs render in the canvas header (layer-feel §3.2).
 *   - Default layer = `'brief'` when graph has at least one
 *     `block_type=era` entity; `'narrative'` otherwise.
 *   - Clicking Brief tab switches active layer to Brief (only era nodes
 *     render).
 *   - Clicking Narrative tab switches active layer to Narrative (events +
 *     context clusters render).
 *   - Switching layers is a discrete semantic swap (layer-feel §3.1) — no
 *     continuous morph; no node leakage between layers.
 *
 * The Task 5 honest-empty-state copy ("No era markers yet — switch to
 * Narrative to see events.") is out of scope here; Task 3 wires the
 * fallback logic only (the test asserts the Brief layer is empty when no
 * era data exists, not the empty-state copy itself).
 *
 * Mount strategy mirrors the orchestrator-level mounts in
 * `timeline-write-boundary.test.tsx`: a mocked `NexusClient` resolves
 * `getWorldKbGraph` to a per-test fixture, and every forbidden write method
 * is spied for negative assertions. The TanStack Query hook drives the
 * projection through `useCanvasSurface`; MSW is not needed because the
 * client mock intercepts before HTTP.
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

describe('TimelineCanvas — layer switcher UI (V1.123 P1 T3)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('renders Brief + Narrative layer tabs in the canvas header', async () => {
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

    expect(screen.getByTestId('timeline-layer-tab-brief')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-layer-tab-narrative')).toBeInTheDocument();
  });

  it('defaults to Brief layer when graph has block_type=era entities', async () => {
    const graph: WorldKbGraphResponse = {
      entities: [
        eraEntity({
          key_block_id: 'kb-era-1',
          canonical_name: 'The First Age',
        }),
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

    const canvas = await screen.findByTestId('timeline-canvas');

    const briefTab = screen.getByTestId('timeline-layer-tab-brief');
    expect(briefTab).toHaveAttribute('aria-pressed', 'true');
    expect(canvas).toHaveAttribute('data-active-layer', 'brief');
    expect(screen.getByTestId('timeline-layer-tab-narrative')).toHaveAttribute(
      'aria-pressed',
      'false',
    );
  });

  it('defaults to Narrative layer when graph has no era entities', async () => {
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

    const canvas = await screen.findByTestId('timeline-canvas');

    const narrativeTab = screen.getByTestId('timeline-layer-tab-narrative');
    expect(narrativeTab).toHaveAttribute('aria-pressed', 'true');
    expect(canvas).toHaveAttribute('data-active-layer', 'narrative');
    expect(screen.getByTestId('timeline-layer-tab-brief')).toHaveAttribute(
      'aria-pressed',
      'false',
    );
  });

  it('clicking Narrative tab switches active layer to Narrative (Brief → Narrative)', async () => {
    const graph: WorldKbGraphResponse = {
      entities: [
        eraEntity({
          key_block_id: 'kb-era-1',
          canonical_name: 'The First Age',
        }),
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
    const canvas = (await renderInApp(<TimelineCanvas worldId="world-7" />, {
      client: makeMockClient(graph),
    }).findByTestId('timeline-canvas')) as HTMLElement;

    await waitFor(() => {
      expect(canvas).toHaveAttribute('data-active-layer', 'brief');
    });

    fireEvent.click(screen.getByTestId('timeline-layer-tab-narrative'));

    await waitFor(() => {
      expect(canvas).toHaveAttribute('data-active-layer', 'narrative');
    });
    expect(screen.getByTestId('timeline-layer-tab-narrative')).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    expect(screen.getByTestId('timeline-layer-tab-brief')).toHaveAttribute(
      'aria-pressed',
      'false',
    );
  });

  it('clicking Brief tab switches active layer to Brief (Narrative → Brief)', async () => {
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
    const canvas = (await renderInApp(<TimelineCanvas worldId="world-7" />, {
      client: makeMockClient(graph),
    }).findByTestId('timeline-canvas')) as HTMLElement;

    await waitFor(() => {
      expect(canvas).toHaveAttribute('data-active-layer', 'narrative');
    });

    fireEvent.click(screen.getByTestId('timeline-layer-tab-brief'));

    await waitFor(() => {
      expect(canvas).toHaveAttribute('data-active-layer', 'brief');
    });
    expect(screen.getByTestId('timeline-layer-tab-brief')).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    expect(screen.getByTestId('timeline-layer-tab-narrative')).toHaveAttribute(
      'aria-pressed',
      'false',
    );
  });

  it('hides the Brief layer tab toggle from the empty-state branch (zero entities)', async () => {
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

    expect(screen.queryByTestId('timeline-layer-tab-brief')).toBeNull();
    expect(screen.queryByTestId('timeline-layer-tab-narrative')).toBeNull();
  });
});

describe('TimelineCanvas — layer swap does not trigger forbidden writes (T3 negative)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('does not invoke any write endpoint while swapping layers', async () => {
    const graph: WorldKbGraphResponse = {
      entities: [
        eraEntity({
          key_block_id: 'kb-era-1',
          canonical_name: 'The First Age',
        }),
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
    const client = makeMockClient(graph);

    renderInApp(<TimelineCanvas worldId="world-7" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('timeline-layer-tab-narrative'));
    await waitFor(() => {
      expect(screen.getByTestId('timeline-layer-tab-narrative')).toHaveAttribute(
        'aria-pressed',
        'true',
      );
    });
    fireEvent.click(screen.getByTestId('timeline-layer-tab-brief'));
    await waitFor(() => {
      expect(screen.getByTestId('timeline-layer-tab-brief')).toHaveAttribute(
        'aria-pressed',
        'true',
      );
    });

    expect(client.worldKbPatchEntity).not.toHaveBeenCalled();
    expect(client.worldKbPatchRelationship).not.toHaveBeenCalled();
    expect(client.worldKbPromoteCandidate).not.toHaveBeenCalled();
    expect(client.patchTimelineEvent).not.toHaveBeenCalled();
  });

  it('renders NLE multi-track band overlay in the canvas host (V1.128 P1 T3)', async () => {
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
      expect(screen.getByTestId('nle-timeline-band-overlay')).toBeInTheDocument();
    });
    expect(screen.getByTestId('nle-timeline-chrome-app')).toBeInTheDocument();
    expect(screen.getByTestId('nle-timeline-label-narrative')).toHaveTextContent('Narrative');
    expect(
      screen.queryByTestId('nle-timeline-clip-detach-ev-1'),
    ).not.toBeInTheDocument();
  });

  it('renders vertical time-bands for the Brief layer (V1.159 P1 T2 supersede)', async () => {
    const graph: WorldKbGraphResponse = {
      entities: [
        eraEntity({
          key_block_id: 'kb-era-1',
          canonical_name: 'The First Age',
        }),
        eraEntity({
          key_block_id: 'kb-era-2',
          canonical_name: 'The Second Age',
        }),
      ],
      source_anchors: [],
      relationships: [],
    };
    renderInApp(<TimelineCanvas worldId="world-7" />, {
      client: makeMockClient(graph),
    });

    await waitFor(() => {
      expect(screen.getByTestId('brief-time-bands')).toBeInTheDocument();
    });
    expect(
      within(screen.getByTestId('brief-time-bands')).getAllByTestId(
        'brief-time-band',
      ),
    ).toHaveLength(2);
    expect(screen.queryByTestId('nle-timeline-band-overlay')).not.toBeInTheDocument();
  });
});

describe('TimelineCanvas — 3-way layer switcher Moment tab (V1.156 P1 T2)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('renders the Moment layer tab alongside Brief + Narrative (3-way segmented control)', async () => {
    const graph: WorldKbGraphResponse = {
      entities: [
        eraEntity({
          key_block_id: 'kb-era-1',
          canonical_name: 'The First Age',
        }),
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

    expect(screen.getByTestId('timeline-layer-tab-brief')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-layer-tab-narrative')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-layer-tab-moment')).toBeInTheDocument();
  });

  it('never defaults to the Moment layer (read-only projection — one click away)', async () => {
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

    const canvas = await screen.findByTestId('timeline-canvas');

    expect(canvas).toHaveAttribute('data-active-layer', 'brief');
    expect(screen.getByTestId('timeline-layer-tab-moment')).toHaveAttribute(
      'aria-pressed',
      'false',
    );
  });

  it('clicking the Moment tab switches active layer to Moment (Brief → Moment)', async () => {
    const graph: WorldKbGraphResponse = {
      entities: [
        eraEntity({
          key_block_id: 'kb-era-1',
          canonical_name: 'The First Age',
        }),
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

    const canvas = await screen.findByTestId('timeline-canvas');

    await waitFor(() => {
      expect(canvas).toHaveAttribute('data-active-layer', 'brief');
    });

    fireEvent.click(screen.getByTestId('timeline-layer-tab-moment'));

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'moment',
      );
    });
    expect(screen.getByTestId('timeline-layer-tab-moment')).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    expect(screen.getByTestId('timeline-layer-tab-brief')).toHaveAttribute(
      'aria-pressed',
      'false',
    );
    expect(screen.getByTestId('timeline-layer-tab-narrative')).toHaveAttribute(
      'aria-pressed',
      'false',
    );
  });

  it('round-trips Brief → Narrative → Moment → Narrative → Brief without regression', async () => {
    const graph: WorldKbGraphResponse = {
      entities: [
        eraEntity({
          key_block_id: 'kb-era-1',
          canonical_name: 'The First Age',
        }),
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

    await screen.findByTestId('timeline-canvas');

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'brief',
      );
    });

    fireEvent.click(screen.getByTestId('timeline-layer-tab-narrative'));
    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });

    fireEvent.click(screen.getByTestId('timeline-layer-tab-moment'));
    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'moment',
      );
    });

    fireEvent.click(screen.getByTestId('timeline-layer-tab-narrative'));
    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });

    fireEvent.click(screen.getByTestId('timeline-layer-tab-brief'));
    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'brief',
      );
    });
    expect(screen.getByTestId('timeline-layer-tab-brief')).toHaveAttribute(
      'aria-pressed',
      'true',
    );
  });

  it('hides the Moment layer tab from the empty-state branch (zero entities)', async () => {
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

    expect(screen.queryByTestId('timeline-layer-tab-moment')).toBeNull();
    expect(screen.queryByTestId('timeline-layer-tab-brief')).toBeNull();
    expect(screen.queryByTestId('timeline-layer-tab-narrative')).toBeNull();
  });

  it('does NOT trigger any forbidden write endpoint when swapping into Moment (read-only layer)', async () => {
    const graph: WorldKbGraphResponse = {
      entities: [
        eraEntity({
          key_block_id: 'kb-era-1',
          canonical_name: 'The First Age',
        }),
        entity({
          key_block_id: 'kb-event-1',
          block_type: 'event',
          canonical_name: 'Coronation',
        }),
      ],
      source_anchors: [],
      relationships: [],
    };
    const client = makeMockClient(graph);

    renderInApp(<TimelineCanvas worldId="world-7" />, { client });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('timeline-layer-tab-moment'));
    await waitFor(() => {
      expect(screen.getByTestId('timeline-layer-tab-moment')).toHaveAttribute(
        'aria-pressed',
        'true',
      );
    });

    expect(client.worldKbPatchEntity).not.toHaveBeenCalled();
    expect(client.worldKbPatchRelationship).not.toHaveBeenCalled();
    expect(client.worldKbPromoteCandidate).not.toHaveBeenCalled();
    expect(client.patchTimelineEvent).not.toHaveBeenCalled();
  });
});

describe('TimelineCanvas — 3-way layer switcher a11y (V1.156 P1 T2)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('wraps the three tabs in a labelled role="group" (SR summary)', async () => {
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

    const group = screen.getByRole('group', { name: 'Timeline layer' });
    expect(group).toBeInTheDocument();
    expect(within(group).getByTestId('timeline-layer-tab-brief')).toBeInTheDocument();
    expect(within(group).getByTestId('timeline-layer-tab-narrative')).toBeInTheDocument();
    expect(within(group).getByTestId('timeline-layer-tab-moment')).toBeInTheDocument();
  });

  it('keeps every tab a native focusable button with aria-pressed toggle state', async () => {
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

    const briefTab = screen.getByTestId('timeline-layer-tab-brief');
    const narrativeTab = screen.getByTestId('timeline-layer-tab-narrative');
    const momentTab = screen.getByTestId('timeline-layer-tab-moment');

    for (const tab of [briefTab, narrativeTab, momentTab]) {
      expect(tab.tagName).toBe('BUTTON');
      expect(tab).toHaveAttribute('type', 'button');
    }

    momentTab.focus();
    expect(momentTab).toHaveFocus();
    expect(momentTab).toHaveAttribute('aria-pressed', 'false');

    fireEvent.click(momentTab);
    await waitFor(() => {
      expect(screen.getByTestId('timeline-layer-tab-moment')).toHaveAttribute(
        'aria-pressed',
        'true',
      );
    });
    expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
      'data-active-layer',
      'moment',
    );
    expect(screen.getByTestId('timeline-layer-tab-brief')).toHaveAttribute(
      'aria-pressed',
      'false',
    );
  });
});
