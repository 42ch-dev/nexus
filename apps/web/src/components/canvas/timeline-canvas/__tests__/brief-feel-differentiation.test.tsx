/**
 * TimelineCanvasAdapter — V1.123 P1 T4 (Brief-layer feel differentiation).
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

// ─── Brief layout options (Task 4 Step 2) ──────────────────────────────────

describe('TimelineCanvasAdapter — Brief-distinct layout options (layer-feel §2.2)', () => {
  it("Brief adapter emits wider rankSep + smaller nodeSep than Narrative default", () => {
    // layer-feel §2.2: Brief = "horizontal era sweep"; Task 4 brief Step 2
    // spec: "wide rankSep, small nodeSep" for the horizontal sweep feel.
    // The exact values are tuning knobs; the contract is: Brief rankSep >
    // Narrative rankSep (or Brief non-default) AND Brief nodeSep <
    // Narrative nodeSep (or Brief non-default).
    const briefAdapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'brief',
    );
    const narrativeAdapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'narrative',
    );

    // Both layers opt into LR (horizontal reading direction).
    expect(briefAdapter.layoutOptions?.direction).toBe('LR');
    expect(narrativeAdapter.layoutOptions?.direction).toBe('LR');

    // Brief carries explicit rankSep + nodeSep tuned for the era sweep.
    expect(briefAdapter.layoutOptions?.rankSep).toBeDefined();
    expect(briefAdapter.layoutOptions?.nodeSep).toBeDefined();
    const briefRank = briefAdapter.layoutOptions?.rankSep ?? 0;
    const briefNode = briefAdapter.layoutOptions?.nodeSep ?? 0;

    // Narrative (V1.122 default) leaves rankSep/nodeSep undefined so the
    // default in useAutoLayout (80/80) applies. Brief must override with
    // wider rankSep + smaller nodeSep.
    const narrativeRank = narrativeAdapter.layoutOptions?.rankSep ?? 80;
    const narrativeNode = narrativeAdapter.layoutOptions?.nodeSep ?? 80;

    expect(briefRank).toBeGreaterThan(narrativeRank);
    expect(briefNode).toBeLessThan(narrativeNode);
  });

  it("Brief adapter preserves hasSuppliedPositions so the temporal-sorted era positions survive first open", () => {
    // Batch A shipped `hasSuppliedPositions: true` so dagre does NOT collapse
    // the chronology on first open (Batch 1 reviewer note). Task 4 MUST
    // preserve that flag — the layer-specific rankSep/nodeSep only kick in
    // on explicit `relayout()`.
    const briefAdapter = createTimelineCanvasAdapter(
      { current: makeContext() },
      'brief',
    );
    expect(briefAdapter.layoutOptions?.hasSuppliedPositions).toBe(true);
  });

  it("Narrative adapter leaves rankSep/nodeSep at V1.122 default (undefined)", () => {
    // V1.122 regression: the Narrative adapter MUST NOT carry explicit
    // rankSep/nodeSep values that would change the V1.122 layout. V1.122
    // defaulted to useAutoLayout's internal defaults (80/80).
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

// ─── Brief-era inspector dispatch (Task 4 Step 3) ──────────────────────────

describe('TimelineCanvasAdapter.renderInspector — Brief-era dispatch', () => {
  it("routes a timeline-brief-era node to the Brief-era inspector (distinct from Narrative event inspector)", () => {
    // Task 4 brief Step 3: "Implement Brief-era inspector — when a Brief-era
    // node is selected, show era details: era-id, start_hint, end_hint (if
    // present), world-summary (full text)." The adapter's renderInspector
    // MUST dispatch on the node kind so Brief-era nodes do NOT render the
    // generic Narrative event inspector chrome.
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
    expect(nodes).toHaveLength(1);

    const inspector = adapter.renderInspector!(nodes[0]!);
    expect(inspector).not.toBeNull();
    // The dispatch contract: the returned JSX renders a Brief-era-distinct
    // inspector form. The `data-testid` lives on the rendered `<form>` (not
    // on the React element's props), so we render the inspector in
    // isolation and assert the form's testid surfaces.
    const { container } = renderInApp(inspector as ReactElement);
    const form = container.querySelector('[data-testid="timeline-brief-era-inspector"]');
    expect(form).not.toBeNull();
    // The Narrative inspector's testid MUST NOT surface here.
    expect(
      container.querySelector('[data-testid="timeline-inspector-title"]'),
    ).toBeNull();
  });

  it("Brief-era inspector surfaces era markers (eraId, startHint, endHint, worldSummary) from body.attributes", () => {
    // Architect §2.3 + §8: era markers ride in `body.attributes` and surface
    // on the Brief-era inspector chrome. The inspector renders the era id,
    // start/end hints, and the full world-summary text (not truncated like
    // the card chrome).
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

    // Render the inspector in isolation (no canvas wrap needed — the
    // inspector is a controlled form over the node data).
    const { container } = renderInApp(inspector as ReactElement);

    // The era marker fields surface verbatim — they are the era's identity.
    // `worldSummary` renders in full (not line-clamped like the card chrome).
    expect(container.textContent).toContain('era-first');
    expect(container.textContent).toContain('1000-01-01T00:00:00Z');
    expect(container.textContent).toContain('1100-01-01T00:00:00Z');
    expect(container.textContent).toContain('A time of myth and legend.');
  });

  it("Narrative event node still routes to the generic Timeline inspector (V1.122 regression)", () => {
    // V1.122 regression guard: a `timeline-event` node MUST still render the
    // generic Timeline inspector (title + body JSON editor). Task 4's Brief-
    // era dispatch is ADDITIVE; the Narrative path is unchanged.
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

    // Render and assert the V1.122 generic inspector surfaces, NOT the
    // Brief-era inspector.
    const { container } = renderInApp(inspector as ReactElement);
    expect(
      container.querySelector('[data-testid="timeline-inspector-title"]'),
    ).not.toBeNull();
    expect(
      container.querySelector('[data-testid="timeline-brief-era-inspector"]'),
    ).toBeNull();
  });
});

