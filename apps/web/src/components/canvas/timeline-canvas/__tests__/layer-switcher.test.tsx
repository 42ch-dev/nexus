/**
 * TimelineCanvas — V1.123 P1 T3 (layer switcher + default-layer logic).
 *
 * Verifies the Brief↔Narrative layer switcher UI + the World-entry default
 * layer logic locked by:
 *   - `iterations/v1.123/specs/three-layer-architecture.md` §7 + §8 (Brief
 *     default on World Timeline when era data exists; architect §7.3
 *     override is Work-Timeline-specific, NOT World-Timeline).
 *   - `iterations/v1.123/specs/layer-feel-differentiation.md` §3.2 (Brief ↔
 *     Narrative explicit layer control via header tabs).
 *   - Plan `2026-07-18-v1.123-world-timeline-brief-narrative.md` Global
 *     Constraints: "P1 only changes default **layer** within Timeline
 *     (Brief if `block_type=era` data, else Narrative)".
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
import { fireEvent, screen, waitFor } from '@testing-library/react';

import { renderInApp } from '@/test/test-providers';
import type { NexusClient } from '@/lib/nexus';
import type {
  WorldKbEntityProjection,
  WorldKbGraphResponse,
} from '@42ch/nexus-contracts';

import { TimelineCanvas } from '../timeline-canvas';

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
    health: vi.fn().mockResolvedValue({ status: 'ok', version: 'test' }),
  } as unknown as NexusClient;
}

// ─── Layer switcher UI (Brief ↔ Narrative) ─────────────────────────────────

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

    // Both layer tabs render — explicit layer control per layer-feel §3.2.
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

    // Default = Brief (plan Global Constraints + architect §7/§8). The
    // Brief tab is pressed and the container's `data-active-layer` mirror
    // reads 'brief'. We assert the data-attribute (not the rendered era
    // card text) because React Flow does not render readable node text in
    // jsdom — the projection contract itself is covered by
    // `brief-projection.test.tsx` at the adapter level.
    const briefTab = screen.getByTestId('timeline-layer-tab-brief');
    expect(briefTab).toHaveAttribute('aria-pressed', 'true');
    expect(canvas).toHaveAttribute('data-active-layer', 'brief');
    // Narrative tab is NOT pressed.
    expect(screen.getByTestId('timeline-layer-tab-narrative')).toHaveAttribute(
      'aria-pressed',
      'false',
    );
  });

  it('defaults to Narrative layer when graph has no era entities', async () => {
    // Task 3 brief Step 4 + plan Global Constraints: Brief default IF data
    // exists; else Narrative fallback. Task 5 owns the honest empty-state
    // copy ("No era markers yet — switch to Narrative to see events.");
    // here we only verify the fallback layer is Narrative.
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

    // Default = Narrative fallback (no era data).
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

    // Default = Brief (era data exists).
    await waitFor(() => {
      expect(canvas).toHaveAttribute('data-active-layer', 'brief');
    });

    // Click the Narrative tab — switches active layer to Narrative.
    fireEvent.click(screen.getByTestId('timeline-layer-tab-narrative'));

    // The Narrative tab is now pressed; the container's active-layer mirror
    // flips to 'narrative'. The discrete semantic swap per layer-feel §3.1
    // is verified at the projection contract level in
    // `brief-projection.test.tsx` (Brief layer excludes events; Narrative
    // layer excludes eras).
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
    // Graph with no era data — defaults to Narrative. User can still click
    // Brief to preview the (empty) Brief layer; Task 5 owns the honest
    // empty-state copy.
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

    // Default = Narrative (no era data).
    await waitFor(() => {
      expect(canvas).toHaveAttribute('data-active-layer', 'narrative');
    });

    // Click the Brief tab — switches active layer to Brief.
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
    // The empty-state branch renders <EmptyState> when the graph has zero
    // entities. The layer switcher targets the spatial canvas only — there
    // are no nodes to switch between on an empty graph. Task 5 owns the
    // per-layer empty-state copy; Task 3 only gates the switcher
    // visibility by the empty-state branch.
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

    // Empty graph → empty-state branch owns the surface; the layer tabs
    // are absent (nothing to switch between).
    expect(screen.queryByTestId('timeline-layer-tab-brief')).toBeNull();
    expect(screen.queryByTestId('timeline-layer-tab-narrative')).toBeNull();
  });
});

// ─── Mount-level sanity (no forbidden writes during layer swap) ────────────

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

    // Swap to Narrative then back to Brief.
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

    // Layer swap is a pure projection change — no writes fire.
    expect(client.worldKbPatchEntity).not.toHaveBeenCalled();
    expect(client.worldKbPatchRelationship).not.toHaveBeenCalled();
    expect(client.worldKbPromoteCandidate).not.toHaveBeenCalled();
    expect(client.patchTimelineEvent).not.toHaveBeenCalled();
  });
});
