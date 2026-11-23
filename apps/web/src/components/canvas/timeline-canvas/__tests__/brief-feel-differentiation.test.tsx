/**
 *
 * Verifies the Brief-distinct feel contract locked by
 * `iterations/v1.123/specs/layer-feel-differentiation.md` §2.2 + §8
 * (P1 seed differentiation):
 *
 *   - **Brief layout options**: when `activeLayer === 'brief'`, the adapter
 *     emits wider `rankSep` + smaller `nodeSep` than the Narrative default
 *     so an explicit `relayout()` produces a more spacious horizontal era
 *     sweep (per layer-feel §2.2 "horizontal era sweep" + Task 4 brief
 *     Step 2: "wide `rankSep`, small `nodeSep`"). The default Narrative
 *     layout options are unchanged (V1.122 regression preserved).
 *   - **Brief-era inspector**: when a `timeline-brief-era` node is
 *     selected, `renderInspector` returns a Brief-era-distinct inspector
 *     that surfaces era markers (`eraId`, `startHint`, `endHint`,
 *     `worldSummary`) extracted from `body.attributes`. Distinct from the
 *     Narrative event inspector (generic title + body JSON editor).
 *
 * Token decision: the dedicated `--color-canvas-layer-brief-accent` token
 * (layer-feel §6.1) is **deferred to P4** because per
 * `tooling/design-tokens/AGENTS.md` + `apps/web/AGENTS.md` new tokens must
 * originate in the DESIGN.md SSOT (architect sign-off), and the layer-feel
 * spec §8 explicitly says P1 ships "seed differentiation" only. The Brief
 * feel for P1 rides on layout + density + node visual + inspector
 * differentiation; the dedicated accent token waits for the DESIGN.md SSOT
 * flow in P4. Batch A's choice to reuse the `worldkb` accent spine is
 * preserved (consistent with V1.121 surface accent pattern).
 */
import { describe, expect, it } from 'vitest';
import type { ReactElement } from 'react';

import { renderInApp } from '@/test/test-providers';
import type {
  WorldKbEntityProjection,
  WorldKbGraphResponse,
} from '@42ch/nexus-contracts';

import {
  createTimelineCanvasAdapter,
  type TimelineCanvasAdapterContext,
} from '../timeline-canvas-adapter';

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
        era_id: 'era-first',
        start_hint: '1000-01-01T00:00:00Z',
        end_hint: '1100-01-01T00:00:00Z',
        world_summary: 'A time of myth and legend.',
      },
    },
    ...rest,
  });
}

function makeContext(
  overrides: Partial<TimelineCanvasAdapterContext> = {},
): TimelineCanvasAdapterContext {
  return {
    worldId: 'world-7',
    ...overrides,
  };
}

describe('TimelineCanvasAdapter — Brief-distinct layout options (layer-feel §2.2)', () => {
  it("Brief adapter emits wider rankSep + smaller nodeSep than Narrative default", () => {
    const briefAdapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'brief',
    );
    const narrativeAdapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'narrative',
    );

    expect(briefAdapter.layoutOptions?.direction).toBe('LR');
    expect(narrativeAdapter.layoutOptions?.direction).toBe('LR');

    expect(briefAdapter.layoutOptions?.rankSep).toBeDefined();
    expect(briefAdapter.layoutOptions?.nodeSep).toBeDefined();
    const briefRank = briefAdapter.layoutOptions?.rankSep ?? 0;
    const briefNode = briefAdapter.layoutOptions?.nodeSep ?? 0;

    const narrativeRank = narrativeAdapter.layoutOptions?.rankSep ?? 80;
    const narrativeNode = narrativeAdapter.layoutOptions?.nodeSep ?? 80;

    expect(briefRank).toBeGreaterThan(narrativeRank);
    expect(briefNode).toBeLessThan(narrativeNode);
  });

  it("Brief adapter preserves hasSuppliedPositions so the temporal-sorted era positions survive first open", () => {
    const briefAdapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'brief',
    );
    expect(briefAdapter.layoutOptions?.hasSuppliedPositions).toBe(true);
  });

  it("Narrative adapter leaves rankSep/nodeSep at V1.122 default (undefined)", () => {
    const narrativeAdapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'narrative',
    );
    expect(narrativeAdapter.layoutOptions?.rankSep).toBeUndefined();
    expect(narrativeAdapter.layoutOptions?.nodeSep).toBeUndefined();
    expect(narrativeAdapter.layoutOptions?.direction).toBe('LR');
    expect(narrativeAdapter.layoutOptions?.hasSuppliedPositions).toBe(true);
  });
});

describe('TimelineCanvasAdapter.renderInspector — Brief-era dispatch', () => {
  it("routes a timeline-brief-era node to the Brief-era inspector (distinct from Narrative event inspector)", () => {
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
      body: {
        attributes: {
          era_id: 'era-first',
          start_hint: '1000-01-01T00:00:00Z',
          end_hint: '1100-01-01T00:00:00Z',
          world_summary: 'A time of myth and legend.',
        },
      },
    });
    const graph: WorldKbGraphResponse = {
      entities: [era],
      source_anchors: [],
      relationships: [],
    };

    const adapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'brief',
    );
    const { nodes } = adapter.projectGraph(graph);
    expect(nodes).toHaveLength(2);

    const inspector = adapter.renderInspector!(nodes[0]!);
    expect(inspector).not.toBeNull();
    const { container } = renderInApp(inspector as ReactElement);
    const form = container.querySelector('[data-testid="timeline-brief-era-inspector"]');
    expect(form).not.toBeNull();
    expect(
      container.querySelector('[data-testid="timeline-inspector-title"]'),
    ).toBeNull();
  });

  it("Brief-era inspector surfaces era markers (eraId, startHint, endHint, worldSummary) from body.attributes", () => {
    const era = eraEntity({
      key_block_id: 'kb-era-1',
      canonical_name: 'The First Age',
      body: {
        attributes: {
          era_id: 'era-first',
          start_hint: '1000-01-01T00:00:00Z',
          end_hint: '1100-01-01T00:00:00Z',
          world_summary: 'A time of myth and legend.',
        },
      },
    });
    const graph: WorldKbGraphResponse = {
      entities: [era],
      source_anchors: [],
      relationships: [],
    };

    const adapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'brief',
    );
    const { nodes } = adapter.projectGraph(graph);
    const inspector = adapter.renderInspector!(nodes[0]!);

    const { container } = renderInApp(inspector as ReactElement);

    expect(container.textContent).toContain('era-first');
    expect(container.textContent).toContain('1000-01-01T00:00:00Z');
    expect(container.textContent).toContain('1100-01-01T00:00:00Z');
    expect(container.textContent).toContain('A time of myth and legend.');
  });

  it("Narrative event node still routes to the generic Timeline inspector (V1.122 regression)", () => {
    const event = entity({
      key_block_id: 'kb-event-1',
      block_type: 'event',
      canonical_name: 'Coronation',
      body: { attributes: { occurred_at: '1042-03-01T00:00:00Z' } },
    });
    const graph: WorldKbGraphResponse = {
      entities: [event],
      source_anchors: [],
      relationships: [],
    };

    const adapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'narrative',
    );
    const { nodes } = adapter.projectGraph(graph);
    const inspector = adapter.renderInspector!(nodes[0]!);

    const { container } = renderInApp(inspector as ReactElement);
    expect(
      container.querySelector('[data-testid="timeline-inspector-title"]'),
    ).not.toBeNull();
    expect(
      container.querySelector('[data-testid="timeline-brief-era-inspector"]'),
    ).toBeNull();
  });
});

