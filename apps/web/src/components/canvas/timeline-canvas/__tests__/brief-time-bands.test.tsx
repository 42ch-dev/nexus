/**
 *
 * Covers the SDD task brief's seven required cases:
 *   1. renders_flat_eras_as_depth_0_bands   — no nesting; all bands at root
 *   2. renders_nested_eras_with_indentation — child bands indent past parent
 *   3. applies_type_coloring                — kingdom/age/sub-age differ
 *   4. shows_type_badge_when_era_type_present
 *   5. hides_type_badge_when_era_type_absent
 *   6. expand_collapse_nested_bands         — depth>0 caret toggles children
 *   7. renders_world_summary_snippet        — clamped secondary line
 *
 * Trees are built through the real `buildEraTree` (Task 1) so the tests
 * exercise the Task 1 → Task 2 pipeline end to end. Fixture builders mirror
 * the sibling `brief-era-tree.test.ts`.
 */
import { describe, expect, it } from 'vitest';
import { fireEvent, screen } from '@testing-library/react';

import type {
  WorldKbEntityProjection,
  WorldKbRelationshipProjection,
} from '@42ch/nexus-contracts';

import { renderInApp } from '@/test/test-providers';

import { buildEraTree, type EraTreeNode } from '../brief-era-tree';
import { BriefTimeBands } from '../brief-time-bands';

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
  attributes: Record<string, unknown> = {},
): WorldKbEntityProjection {
  return eraEntity({
    key_block_id: keyBlockId,
    canonical_name: canonicalName,
    body: {
      attributes: { era_type: eraType, ...attributes },
    },
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

describe('BriefTimeBands', () => {
  it('renders_flat_eras_as_depth_0_bands', () => {
    const tree = buildEraTree(
      [
        eraEntity({ key_block_id: 'kb-era-1', canonical_name: 'First Age' }),
        eraEntity({ key_block_id: 'kb-era-2', canonical_name: 'Second Age' }),
        eraEntity({ key_block_id: 'kb-era-3', canonical_name: 'Third Age' }),
      ],
      [],
    );

    const { container } = renderBands(tree);

    expect(screen.getAllByTestId('brief-time-band')).toHaveLength(3);
    expect(container.querySelectorAll('[data-depth="0"]')).toHaveLength(3);
    expect(container.querySelectorAll('[data-depth="1"]')).toHaveLength(0);
    expect(screen.queryByTestId('brief-time-band-toggle')).toBeNull();
    expect(screen.getByText('First Age')).toBeInTheDocument();
    expect(screen.getByText('Second Age')).toBeInTheDocument();
    expect(screen.getByText('Third Age')).toBeInTheDocument();
  });

  it('renders_nested_eras_with_indentation', () => {
    const tree = buildEraTree(
      [
        eraEntity({ key_block_id: 'kb-kingdom', canonical_name: 'Bronze Kingdom' }),
        eraEntity({ key_block_id: 'kb-age', canonical_name: 'First Age' }),
        eraEntity({ key_block_id: 'kb-epoch', canonical_name: 'Early Epoch' }),
      ],
      [
        parentEraRel('kb-kingdom', 'kb-age', 1),
        parentEraRel('kb-age', 'kb-epoch', 2),
      ],
    );

    const { container } = renderBands(tree);

    expect(bandOf(container, 'kb-kingdom').parentElement).toHaveStyle({
      paddingLeft: '0px',
    });
    expect(bandOf(container, 'kb-age').parentElement).toHaveStyle({
      paddingLeft: '24px',
    });
    expect(bandOf(container, 'kb-epoch').parentElement).toHaveStyle({
      paddingLeft: '48px',
    });
    expect(
      (bandOf(container, 'kb-age').parentElement as HTMLElement).style.paddingLeft,
    ).not.toBe(
      (bandOf(container, 'kb-kingdom').parentElement as HTMLElement).style
        .paddingLeft,
    );
  });

  it('applies_type_coloring', () => {
    const tree = buildEraTree(
      [
        typedEra('kb-kingdom', 'Bronze Kingdom', 'kingdom'),
        typedEra('kb-age', 'First Age', 'age'),
        typedEra('kb-sub-age', 'Early Sub-age', 'sub-age'),
      ],
      [],
    );

    const { container } = renderBands(tree);

    expect(bandOf(container, 'kb-kingdom').style.backgroundColor).toBe('var(--color-amber-900)');
    expect(bandOf(container, 'kb-age').style.backgroundColor).toBe('var(--color-amber-800)');
    expect(bandOf(container, 'kb-sub-age').style.backgroundColor).toBe('var(--color-gray-700)');
    const kingdomColor = bandOf(container, 'kb-kingdom').style.backgroundColor;
    const ageColor = bandOf(container, 'kb-age').style.backgroundColor;
    const subAgeColor = bandOf(container, 'kb-sub-age').style.backgroundColor;
    expect(kingdomColor).not.toBe(ageColor);
    expect(ageColor).not.toBe(subAgeColor);
    expect(kingdomColor).not.toBe(subAgeColor);
  });

  it('shows_type_badge_when_era_type_present', () => {
    const tree = buildEraTree([typedEra('kb-age', 'First Age', 'age')], []);

    renderBands(tree);

    const badge = screen.getByTestId('brief-time-band-type-badge');
    expect(badge).toBeInTheDocument();
    expect(badge).toHaveTextContent('age');
  });

  it('hides_type_badge_when_era_type_absent', () => {
    const tree = buildEraTree(
      [eraEntity({ key_block_id: 'kb-era-1', canonical_name: 'First Age' })],
      [],
    );

    renderBands(tree);

    expect(screen.queryByTestId('brief-time-band-type-badge')).toBeNull();
    expect(screen.getByText('First Age')).toBeInTheDocument();
  });

  it('expand_collapse_nested_bands', () => {
    const tree = buildEraTree(
      [
        eraEntity({ key_block_id: 'kb-kingdom', canonical_name: 'Bronze Kingdom' }),
        eraEntity({ key_block_id: 'kb-age', canonical_name: 'First Age' }),
        eraEntity({ key_block_id: 'kb-epoch', canonical_name: 'Early Epoch' }),
      ],
      [
        parentEraRel('kb-kingdom', 'kb-age', 1),
        parentEraRel('kb-age', 'kb-epoch', 2),
      ],
    );

    const { container } = renderBands(tree);

    const toggles = screen.getAllByTestId('brief-time-band-toggle');
    expect(toggles).toHaveLength(1);
    const toggle = toggles[0] as HTMLElement;
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(bandOf(container, 'kb-epoch')).toBeInTheDocument();

    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(
      container.querySelector('[data-era-id="kb-epoch"][data-testid="brief-time-band"]'),
    ).toBeNull();
    expect(bandOf(container, 'kb-age')).toBeInTheDocument();
    expect(bandOf(container, 'kb-kingdom')).toBeInTheDocument();

    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(bandOf(container, 'kb-epoch')).toBeInTheDocument();
  });

  it('renders_world_summary_snippet', () => {
    const summary =
      'The long age of bronze and ash, when the first kingdoms rose from the river deltas and the old songs were still sung.';
    const tree = buildEraTree(
      [
        eraEntity({
          key_block_id: 'kb-age',
          canonical_name: 'First Age',
          body: { attributes: { world_summary: summary } },
        }),
      ],
      [],
    );

    const { container } = renderBands(tree);

    const snippet = screen.getByTestId('brief-time-band-summary');
    expect(snippet).toHaveTextContent(summary);
    expect(snippet).toHaveClass('line-clamp-2');
    expect(snippet).toHaveAttribute('title', summary);
    expect(container.querySelectorAll('[data-testid="brief-time-band-summary"]')).toHaveLength(1);
  });
});
