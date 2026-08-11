/**
 * brief-era-tree — V1.159 P1 Task 1 unit tests.
 *
 * Covers the SDD task brief's six required cases:
 *   1. flat_eras_no_relationships        → all depth-0 roots
 *   2. two_level_nest                   → parent with one child
 *   3. three_level_nest                 → grandparent → parent → child
 *   4. cycle_a_to_b_to_a                → A placed once; back-edge detected;
 *                                        no infinite loop
 *   5. orphan_child_relation_target_not_in_entities → edge skipped; no crash
 *   6. mixed_typed_untyped              → era_type on some, absent on others
 *
 * Fixture conventions mirror the sibling `brief-projection.test.tsx`
 * (`eraEntity` builder + minimal required `WorldKbEntityProjection` fields).
 */
import { describe, expect, it, vi } from 'vitest';

import type {
  WorldKbEntityProjection,
  WorldKbRelationshipProjection,
} from '@42ch/nexus-contracts';

import { buildEraTree, type EraTreeNode } from '../brief-era-tree';

// ─── Fixture builders ──────────────────────────────────────────────────────

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
    // Default era body shape per architect §2.3: markers ride in
    // `body.attributes`. era_type is added per-fixture when needed.
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

// ─── Test helpers ──────────────────────────────────────────────────────────

/** Collect every node in a forest (breadth-first) for structural assertions. */
function flattenTree(tree: EraTreeNode[]): EraTreeNode[] {
  const all: EraTreeNode[] = [];
  const queue = [...tree];
  while (queue.length > 0) {
    const node = queue.shift() as EraTreeNode;
    all.push(node);
    queue.push(...node.children);
  }
  return all;
}

function idsAtDepth(tree: EraTreeNode[], depth: number): string[] {
  return flattenTree(tree)
    .filter((n) => n.depth === depth)
    .map((n) => n.era.key_block_id)
    .sort();
}

/** Silence + capture cycle warnings; returns the captured messages. */
function captureWarnings() {
  const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
  return warn;
}

// ─── Tests ─────────────────────────────────────────────────────────────────

describe('buildEraTree', () => {
  it('flat_eras_no_relationships → all eras are depth-0 roots', () => {
    const entities = [
      eraEntity({ key_block_id: 'era-b', canonical_name: 'Age of Embers' }),
      eraEntity({ key_block_id: 'era-a', canonical_name: 'Age of Dawn' }),
      eraEntity({ key_block_id: 'era-c', canonical_name: 'Age of Ash' }),
    ];

    const tree = buildEraTree(entities, []);

    // 3 roots, one per era, all at depth 0 with no children.
    expect(tree).toHaveLength(3);
    expect(tree.every((n) => n.depth === 0)).toBe(true);
    expect(tree.every((n) => n.children.length === 0)).toBe(true);
    expect(idsAtDepth(tree, 0)).toEqual(['era-a', 'era-b', 'era-c']);
  });

  it('two_level_nest → parent root with one child at depth 1', () => {
    const entities = [
      eraEntity({ key_block_id: 'era-child', canonical_name: 'Early Reign' }),
      eraEntity({ key_block_id: 'era-parent', canonical_name: 'First Kingdom' }),
    ];
    const relationships = [parentEraRel('era-parent', 'era-child', 0)];

    const tree = buildEraTree(entities, relationships);

    expect(tree).toHaveLength(1);
    const root = tree[0];
    expect(root.era.key_block_id).toBe('era-parent');
    expect(root.depth).toBe(0);
    expect(root.children).toHaveLength(1);
    expect(root.children[0].era.key_block_id).toBe('era-child');
    expect(root.children[0].depth).toBe(1);
    expect(root.children[0].children).toHaveLength(0);
    expect(idsAtDepth(tree, 0)).toEqual(['era-parent']);
    expect(idsAtDepth(tree, 1)).toEqual(['era-child']);
  });

  it('three_level_nest → grandparent → parent → child at depths 0/1/2', () => {
    const entities = [
      eraEntity({ key_block_id: 'era-child', canonical_name: 'Sub-age of Wheels' }),
      eraEntity({ key_block_id: 'era-parent', canonical_name: 'Age of Wheels' }),
      eraEntity({ key_block_id: 'era-grand', canonical_name: 'Kingdom of Metal' }),
    ];
    const relationships = [
      parentEraRel('era-grand', 'era-parent', 0),
      parentEraRel('era-parent', 'era-child', 1),
    ];

    const tree = buildEraTree(entities, relationships);

    expect(tree).toHaveLength(1);
    const grand = tree[0];
    expect(grand.era.key_block_id).toBe('era-grand');
    expect(grand.depth).toBe(0);
    expect(grand.children).toHaveLength(1);
    const parent = grand.children[0];
    expect(parent.era.key_block_id).toBe('era-parent');
    expect(parent.depth).toBe(1);
    expect(parent.children).toHaveLength(1);
    const child = parent.children[0];
    expect(child.era.key_block_id).toBe('era-child');
    expect(child.depth).toBe(2);
    expect(child.children).toHaveLength(0);
    expect(idsAtDepth(tree, 0)).toEqual(['era-grand']);
    expect(idsAtDepth(tree, 1)).toEqual(['era-parent']);
    expect(idsAtDepth(tree, 2)).toEqual(['era-child']);
  });

  it('cycle_a_to_b_to_a → each era placed once; back-edge detected; no infinite loop', () => {
    const warn = captureWarnings();
    const entities = [
      eraEntity({ key_block_id: 'era-a', canonical_name: 'Era A' }),
      eraEntity({ key_block_id: 'era-b', canonical_name: 'Era B' }),
    ];
    const relationships = [
      parentEraRel('era-a', 'era-b', 0),
      parentEraRel('era-b', 'era-a', 1),
    ];

    const tree = buildEraTree(entities, relationships);

    // Both eras are placed exactly once; A (smallest id) is the depth-0 root
    // with B nested beneath it; the A→B→A back-edge is skipped, not re-rendered.
    const all = flattenTree(tree);
    expect(all.map((n) => n.era.key_block_id).sort()).toEqual([
      'era-a',
      'era-b',
    ]);
    expect(tree).toHaveLength(1);
    expect(tree[0].era.key_block_id).toBe('era-a');
    expect(tree[0].children).toHaveLength(1);
    expect(tree[0].children[0].era.key_block_id).toBe('era-b');
    expect(tree[0].children[0].depth).toBe(1);
    // The back-edge (B → A) must have been detected and logged.
    expect(warn).toHaveBeenCalledTimes(1);
    expect(warn.mock.calls[0][0]).toContain('era-b" -> "era-a');
    expect(warn.mock.calls[0][0]).toContain('cycle detected');
  });

  it('orphan_child_relation_target_not_in_entities → edge skipped; no crash', () => {
    const warn = captureWarnings();
    const entities = [
      eraEntity({ key_block_id: 'era-parent', canonical_name: 'Only Era' }),
    ];
    const relationships = [
      // Target `era-missing` is not in the entities set — the edge is orphaned.
      parentEraRel('era-parent', 'era-missing', 0),
      // Source `era-ghost` is also not in the entities set — no valid edge.
      parentEraRel('era-ghost', 'era-parent', 1),
    ];

    const tree = buildEraTree(entities, relationships);

    expect(tree).toHaveLength(1);
    expect(tree[0].era.key_block_id).toBe('era-parent');
    expect(tree[0].depth).toBe(0);
    expect(tree[0].children).toHaveLength(0);
    // Orphan edges are skipped silently — no crash, no warning noise.
    expect(warn).not.toHaveBeenCalled();
  });

  it('mixed_typed_untyped → typed and untyped eras both render in the tree', () => {
    const entities = [
      eraEntity({
        key_block_id: 'era-typed',
        canonical_name: 'Kingdom of Dawn',
        body: { attributes: { era_type: 'kingdom' } },
      }),
      eraEntity({
        key_block_id: 'era-untyped',
        canonical_name: 'Silent Age',
        // No era_type attribute at all (legacy flat-era data).
        body: { attributes: {} },
      }),
      eraEntity({
        key_block_id: 'era-empty',
        canonical_name: 'Empty-Typed Age',
        // Empty string era_type is treated as absent (mirrors adapter
        // readString convention).
        body: { attributes: { era_type: '' } },
      }),
      eraEntity({
        key_block_id: 'era-sub',
        canonical_name: 'Sub-age of Dawn',
        body: { attributes: { era_type: 'sub-age' } },
      }),
    ];
    const relationships = [parentEraRel('era-typed', 'era-sub', 0)];

    const tree = buildEraTree(entities, relationships);

    const byId = new Map(flattenTree(tree).map((n) => [n.era.key_block_id, n]));
    // All four eras render (typed, untyped, empty-typed, nested child).
    expect(byId.size).toBe(4);
    expect(byId.get('era-typed')?.era_type).toBe('kingdom');
    expect(byId.get('era-sub')?.era_type).toBe('sub-age');
    expect(byId.get('era-untyped')?.era_type).toBeUndefined();
    expect(byId.get('era-empty')?.era_type).toBeUndefined();
    // Untyped eras are depth-0 roots; the typed parent nests its typed child.
    expect(idsAtDepth(tree, 0)).toEqual(['era-empty', 'era-typed', 'era-untyped']);
    expect(idsAtDepth(tree, 1)).toEqual(['era-sub']);
  });

  it('multi_parent_dag → era with two parents appears under both (no data loss)', () => {
    // Greptile P1: two parent_era edges targeting the same child must NOT
    // silently vanish. The tree builder renders a DAG forest — the child
    // appears under BOTH parents. No warnings (multi-parent is valid data).
    const entities = [
      eraEntity({ key_block_id: 'era-a', canonical_name: 'Parent A' }),
      eraEntity({ key_block_id: 'era-b', canonical_name: 'Parent B' }),
      eraEntity({ key_block_id: 'era-c', canonical_name: 'Child C' }),
    ];
    const relationships = [
      parentEraRel('era-a', 'era-c', 0),
      parentEraRel('era-b', 'era-c', 1),
    ];

    const warn = captureWarnings();
    const tree = buildEraTree(entities, relationships);

    // C appears TWICE — once under A, once under B (DAG forest, no loss).
    const all = flattenTree(tree);
    const cNodes = all.filter((n) => n.era.key_block_id === 'era-c');
    expect(cNodes).toHaveLength(2);
    expect(cNodes.every((n) => n.depth === 1)).toBe(true);

    // A and B are depth-0 roots, each with C as child.
    expect(tree).toHaveLength(2);
    expect(tree[0].era.key_block_id).toBe('era-a');
    expect(tree[0].children).toHaveLength(1);
    expect(tree[0].children[0].era.key_block_id).toBe('era-c');
    expect(tree[1].era.key_block_id).toBe('era-b');
    expect(tree[1].children).toHaveLength(1);
    expect(tree[1].children[0].era.key_block_id).toBe('era-c');

    // No warnings — multi-parent is valid data in a DAG.
    expect(warn).not.toHaveBeenCalled();
  });
});
