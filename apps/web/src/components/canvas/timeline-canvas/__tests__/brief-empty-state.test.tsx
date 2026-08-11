/**
 * Brief empty-state + backward-compat — V1.159 P1 Task 4.
 *
 * Hardens the Brief layer against every era-shape edge case and pins the
 * "新建 era" entry visibility contract (spec §3.3.3 V1.159 amendment
 * "Create entry" + plan Task 4 DoD):
 *
 *   1. empty_era_data_shows_v1_123_empty_state — no `block_type=era`
 *      entities → the V1.123 Brief empty-state panel (title + CTA) AND the
 *      "新建 era" button both render (the create-first-era path).
 *   2. flat_eras_render_as_depth_0_bands        — untyped, unnested eras →
 *      depth-0 bands, default Brief-accent color, no type badge (V1.156
 *      flat rendering backward compatible).
 *   3. mixed_typed_untyped_render_correctly     — some eras typed, some not
 *      → per-type color + badge on typed bands only; untyped bands keep
 *      the default color and no badge.
 *   4. partial_nesting_orphans_and_nested       — a forest mixing depth-0
 *      orphan roots with nested branches renders both shapes.
 *   5. new_era_button_visible_on_empty_state    — the Brief empty panel
 *      still surfaces the create entry (author can start the Brief).
 *   6. new_era_button_hidden_on_non_brief_layer — Narrative/Moment layers
 *      never show the Brief-only create entry.
 *
 * Component-level cases build trees through the real `buildEraTree` (Task 1)
 * so the Task 1 → Task 2 → Task 4 pipeline is exercised end to end (mirrors
 * the sibling `brief-time-bands.test.tsx`). Canvas-level cases mount
 * `TimelineCanvas` with a mocked `NexusClient` (mirrors
 * `empty-state.test.tsx`).
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import type {
  WorldKbEntityProjection,
  WorldKbGraphResponse,
  WorldKbRelationshipProjection,
} from '@42ch/nexus-contracts';

import { renderInApp } from '@/test/test-providers';
import type { NexusClient } from '@/lib/nexus';

import { buildEraTree, type EraTreeNode } from '../brief-era-tree';
import { BriefTimeBands } from '../brief-time-bands';
import { TimelineCanvas } from '../timeline-canvas';

// ─── Component fixture builders (mirror brief-time-bands.test.tsx) ─────────

function eraEntity(
  overrides: Partial<WorldKbEntityProjection> &
    Pick<WorldKbEntityProjection, 'key_block_id' | 'canonical_name'>,
): WorldKbEntityProjection {
  const { key_block_id, canonical_name, body, ...rest } = overrides;
  return {
    world_id: 'world-7',
    block_type: 'era',
    status: 'confirmed',
    version: 1,
    key_block_id,
    canonical_name,
    body:
      body ??
      ({
        attributes: {
          era_id: key_block_id,
          start_hint: '1000-01-01T00:00:00Z',
          end_hint: '1100-01-01T00:00:00Z',
          world_summary: `${canonical_name} summary`,
        },
      } as WorldKbEntityProjection['body']),
    ...rest,
  };
}

function typedEra(
  keyBlockId: string,
  canonicalName: string,
  eraType: string,
): WorldKbEntityProjection {
  return eraEntity({
    key_block_id: keyBlockId,
    canonical_name: canonicalName,
    body: { attributes: { era_type: eraType } },
  });
}

function parentEraRel(
  sourceEntityId: string,
  targetEntityId: string,
  index: number,
): WorldKbRelationshipProjection {
  return {
    relationship_id: `rel-${index}`,
    world_id: 'world-7',
    source_entity_id: sourceEntityId,
    target_entity_id: targetEntityId,
    relation_type: 'custom',
    custom_label: 'parent_era',
    symmetric: false,
    source_anchor_ids: [],
    needs_review: false,
    source: 'manual',
    version: 1,
    updated_at: '2026-08-11T00:00:00Z',
    projection_direction: 'stored',
  };
}

function renderBands(tree: EraTreeNode[]) {
  return renderInApp(<BriefTimeBands tree={tree} />);
}

function bandOf(container: HTMLElement, eraId: string): HTMLElement {
  const el = container.querySelector(
    `[data-era-id="${eraId}"][data-testid="brief-time-band"]`,
  );
  if (!(el instanceof HTMLElement)) {
    throw new Error(`band for era ${eraId} not found`);
  }
  return el;
}

// ─── Canvas fixture builders (mirror empty-state.test.tsx) ─────────────────

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

/** Graph with events but ZERO `block_type=era` entities (Brief is empty). */
function eventsOnlyGraph(): WorldKbGraphResponse {
  return {
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
}

/** Graph with era data → Brief is the default layer. */
function eraGraph(): WorldKbGraphResponse {
  return {
    entities: [eraEntity({ key_block_id: 'kb-era-1', canonical_name: 'First Age' })],
    source_anchors: [],
    relationships: [],
  };
}

// ─── Tests ─────────────────────────────────────────────────────────────────

describe('Brief empty-state + backward-compat (V1.159 P1 T4)', () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('empty_era_data_shows_v1_123_empty_state', async () => {
    // No `block_type=era` entities → the V1.123 Brief empty-state panel
    // (title + Narrative CTA) renders when the user switches to Brief, and
    // the "新建 era" create entry is visible alongside it (T4 DoD: the
    // author can create their first era from the empty state).
    renderInApp(<TimelineCanvas worldId="world-7" />, {
      client: makeMockClient(eventsOnlyGraph()),
    });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
    });

    // Default = Narrative (no era data).
    fireEvent.click(screen.getByTestId('timeline-layer-tab-brief'));

    // V1.123 Brief empty-state panel with the locked copy + escape hatch.
    const briefEmpty = await screen.findByTestId('timeline-brief-empty-state');
    expect(briefEmpty).toHaveTextContent('No era markers yet');
    expect(briefEmpty).toHaveTextContent('Switch to Narrative');

    // The create entry is present on the empty state (create-first-era path).
    const createEntry = screen.getByTestId('timeline-create-era-entry');
    expect(createEntry).toBeInTheDocument();
    expect(createEntry).toHaveTextContent(/New era/i);
  });

  it('flat_eras_render_as_depth_0_bands', () => {
    // Untyped, unnested eras → every band sits at depth 0 with the default
    // Brief-accent color and NO type badge (V1.156 flat rendering
    // backward compatible — spec §3.3.3 "Unknown or absent era_type falls
    // back to the default Brief accent").
    const tree = buildEraTree(
      [
        eraEntity({ key_block_id: 'kb-era-1', canonical_name: 'First Age' }),
        eraEntity({ key_block_id: 'kb-era-2', canonical_name: 'Second Age' }),
      ],
      [],
    );

    const { container } = renderBands(tree);

    expect(screen.getAllByTestId('brief-time-band')).toHaveLength(2);
    // Both are depth-0 roots; no nested levels anywhere.
    expect(container.querySelectorAll('[data-depth="0"]')).toHaveLength(2);
    expect(container.querySelectorAll('[data-depth="1"]')).toHaveLength(0);
    // Default color on both; no type badges at all.
    expect(bandOf(container, 'kb-era-1')).toHaveStyle({
      backgroundColor: 'var(--color-canvas-layer-brief-accent)',
    });
    expect(bandOf(container, 'kb-era-2')).toHaveStyle({
      backgroundColor: 'var(--color-canvas-layer-brief-accent)',
    });
    expect(screen.queryByTestId('brief-time-band-type-badge')).toBeNull();
  });

  it('mixed_typed_untyped_render_correctly', () => {
    // Partial typing: the typed era gets its per-type color + verbatim
    // badge; the untyped era keeps the default color and no badge.
    const tree = buildEraTree(
      [
        typedEra('kb-kingdom', 'Bronze Kingdom', 'kingdom'),
        eraEntity({ key_block_id: 'kb-era-1', canonical_name: 'First Age' }),
      ],
      [],
    );

    const { container } = renderBands(tree);

    const typedBand = bandOf(container, 'kb-kingdom');
    expect(typedBand).toHaveStyle({
      backgroundColor: 'var(--color-amber-900)',
    });
    expect(typedBand).toHaveTextContent('kingdom');

    const untypedBand = bandOf(container, 'kb-era-1');
    expect(untypedBand).toHaveStyle({
      backgroundColor: 'var(--color-canvas-layer-brief-accent)',
    });

    // Exactly one badge — only the typed era carries one.
    const badges = screen.getAllByTestId('brief-time-band-type-badge');
    expect(badges).toHaveLength(1);
    expect(badges[0]).toHaveTextContent('kingdom');
  });

  it('partial_nesting_orphans_and_nested', () => {
    // Partial nesting: a nested branch (kingdom → age) alongside a flat
    // orphan root (Third Age) — both shapes render in one forest.
    const tree = buildEraTree(
      [
        eraEntity({ key_block_id: 'kb-kingdom', canonical_name: 'Bronze Kingdom' }),
        eraEntity({ key_block_id: 'kb-age', canonical_name: 'First Age' }),
        eraEntity({ key_block_id: 'kb-orphan', canonical_name: 'Third Age' }),
      ],
      [parentEraRel('kb-kingdom', 'kb-age', 1)],
    );

    const { container } = renderBands(tree);

    expect(screen.getAllByTestId('brief-time-band')).toHaveLength(3);
    // Nested branch: parent at depth 0, child indented to depth 1.
    expect(bandOf(container, 'kb-kingdom').parentElement).toHaveAttribute(
      'data-depth',
      '0',
    );
    expect(bandOf(container, 'kb-age').parentElement).toHaveAttribute(
      'data-depth',
      '1',
    );
    expect(bandOf(container, 'kb-age').parentElement).toHaveStyle({
      paddingLeft: '24px',
    });
    // Orphan root: depth 0, no indent, rendered as a sibling band.
    expect(bandOf(container, 'kb-orphan').parentElement).toHaveAttribute(
      'data-depth',
      '0',
    );
    expect(bandOf(container, 'kb-orphan').parentElement).toHaveStyle({
      paddingLeft: '0px',
    });
  });

  it('new_era_button_visible_on_empty_state', async () => {
    // The Brief empty-state panel still surfaces the create entry — the
    // author can start authoring the Brief from an empty world (T4 DoD:
    // "visible when Brief layer active, even on empty state").
    const user = userEvent.setup();
    renderInApp(<TimelineCanvas worldId="world-7" />, {
      client: makeMockClient(eventsOnlyGraph()),
    });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('timeline-layer-tab-brief'));
    await screen.findByTestId('timeline-brief-empty-state');

    // Create entry present on the empty panel → clicking opens the create
    // dialog (the first-era creation path works from the empty state).
    const createEntry = screen.getByTestId('timeline-create-era-entry');
    expect(createEntry).toBeInTheDocument();
    await user.click(createEntry);
    expect(screen.getByLabelText(/Era name/i)).toBeInTheDocument();
  });

  it('new_era_button_hidden_on_non_brief_layer', async () => {
    // The create entry is Brief-layer chrome only: hidden on Narrative and
    // Moment, back again on Brief (spec §3.3.3 "Create entry" — Brief
    // owns era authoring; Work-Brief is read-only).
    const user = userEvent.setup();
    renderInApp(<TimelineCanvas worldId="world-7" />, {
      client: makeMockClient(eraGraph()),
    });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toBeInTheDocument();
    });

    // Brief is the default layer (era data exists) → entry visible.
    expect(screen.getByTestId('timeline-create-era-entry')).toBeInTheDocument();

    // Narrative → hidden.
    await user.click(screen.getByTestId('timeline-layer-tab-narrative'));
    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'narrative',
      );
    });
    expect(screen.queryByTestId('timeline-create-era-entry')).toBeNull();

    // Moment → hidden (Moment is a read-only projection; no World-era
    // authoring affordance).
    await user.click(screen.getByTestId('timeline-layer-tab-moment'));
    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'moment',
      );
    });
    expect(screen.queryByTestId('timeline-create-era-entry')).toBeNull();

    // Back to Brief → visible again.
    await user.click(screen.getByTestId('timeline-layer-tab-brief'));
    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas')).toHaveAttribute(
        'data-active-layer',
        'brief',
      );
    });
    expect(screen.getByTestId('timeline-create-era-entry')).toBeInTheDocument();
  });
});
