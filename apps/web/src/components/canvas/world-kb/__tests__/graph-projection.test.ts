/**
 * World KB graph projection tests — pure adapter coverage (V1.73 P0 A6).
 *
 * Verifies the World → node/edge projection: lane layout, candidate trailing
 * lane, source-anchor edge derivation, lifecycle mapping, and the freshness
 * summary/entry-count helpers used by the command-palette indicator.
 */
import { describe, expect, it } from 'vitest';

import type {
  WorldKbCandidateProjection,
  WorldKbEntityProjection,
  WorldKbGraphResponse,
  WorldKbSourceAnchorProjection,
} from '@42ch/nexus-contracts';

import {
  candidateToNodeData,
  deriveEdges,
  entryCountOf,
  entityToNodeData,
  graphSummary,
  layoutNodes,
} from '../graph-projection';
import { lifecycleFromStatus } from '../types';

function entity(overrides: Partial<WorldKbEntityProjection> = {}): WorldKbEntityProjection {
  return {
    key_block_id: 'kb-1',
    world_id: 'w-1',
    block_type: 'character',
    canonical_name: 'Aria Stormwind',
    status: 'confirmed',
    version: 3,
    source_anchor_count: 2,
    ...overrides,
  };
}

function candidate(overrides: Partial<WorldKbCandidateProjection> = {}): WorldKbCandidateProjection {
  return {
    candidate_id: 'cand-1',
    job_id: 'job-1',
    world_id: 'w-1',
    block_type: 'character',
    canonical_name: 'Aria (candidate)',
    version: 1,
    ...overrides,
  };
}

function anchor(overrides: Partial<WorldKbSourceAnchorProjection> = {}): WorldKbSourceAnchorProjection {
  return {
    source_anchor_id: 'sa-1',
    key_block_id: 'kb-1',
    source_type: 'chapter',
    reference: 'ch3',
    ...overrides,
  };
}

describe('entityToNodeData / candidateToNodeData', () => {
  it('maps a confirmed entity to node data with confirmed lifecycle', () => {
    const d = entityToNodeData(entity(), 'w-1');
    expect(d.keyBlockId).toBe('kb-1');
    expect(d.entityKind).toBe('character');
    expect(d.lifecycle).toBe('confirmed');
    expect(d.sourceAnchorCount).toBe(2);
    expect(d.computable).toBe(false);
  });

  it('maps a pending candidate to node data with pending lifecycle', () => {
    const d = candidateToNodeData(candidate());
    expect(d.candidateId).toBe('cand-1');
    expect(d.lifecycle).toBe('pending');
    expect(d.keyBlockId).toBeUndefined();
  });

  it('flags computable block kinds', () => {
    expect(entityToNodeData(entity({ block_type: 'beat' }), 'w-1').computable).toBe(true);
    expect(entityToNodeData(entity({ block_type: 'act' }), 'w-1').computable).toBe(true);
    expect(entityToNodeData(entity({ block_type: 'character' }), 'w-1').computable).toBe(false);
  });
});

describe('lifecycleFromStatus', () => {
  it('maps raw statuses to the four-state badge', () => {
    expect(lifecycleFromStatus('pending')).toBe('pending');
    expect(lifecycleFromStatus('confirmed')).toBe('confirmed');
    expect(lifecycleFromStatus('rejected')).toBe('rejected');
    expect(lifecycleFromStatus('merged')).toBe('merged');
    // manual / unknown / empty normalize to confirmed.
    expect(lifecycleFromStatus('manual')).toBe('confirmed');
    expect(lifecycleFromStatus(undefined)).toBe('confirmed');
  });
});

describe('layoutNodes', () => {
  it('stacks entities by BlockType lane and candidates in a trailing lane', () => {
    const nodes = layoutNodes(
      [entity({ key_block_id: 'a', block_type: 'character' }), entity({ key_block_id: 'b', block_type: 'scene' })],
      [candidate({ candidate_id: 'c' })],
      'w-1',
    );
    const ids = nodes.map((n) => n.id);
    expect(ids).toEqual(['entity:a', 'entity:b', 'candidate:c']);
    // Candidate x position is greater than both entity lanes (trailing lane).
    const candidateX = nodes.find((n) => n.id === 'candidate:c')!.position.x;
    const entityXs = nodes.filter((n) => n.id.startsWith('entity:')).map((n) => n.position.x);
    expect(candidateX).toBeGreaterThan(Math.max(...entityXs));
  });

  it('stacks same-lane entities vertically', () => {
    const nodes = layoutNodes(
      [
        entity({ key_block_id: 'a', block_type: 'character' }),
        entity({ key_block_id: 'b', block_type: 'character' }),
      ],
      [],
      'w-1',
    );
    expect(nodes[0].position.x).toBe(nodes[1].position.x);
    expect(nodes[1].position.y).toBeGreaterThan(nodes[0].position.y);
  });
});

describe('deriveEdges', () => {
  it('produces read-only source-anchor edges from anchor to entity', () => {
    const edges = deriveEdges([anchor(), anchor({ source_anchor_id: 'sa-2' })]);
    expect(edges).toHaveLength(2);
    expect(edges[0].source).toBe('anchor-node:sa-1');
    expect(edges[0].target).toBe('entity:kb-1');
    expect(edges[0].selectable).toBe(false);
    expect(edges[0].focusable).toBe(true);
  });
});

describe('freshness helpers', () => {
  const graph: WorldKbGraphResponse = {
    entities: [entity(), entity({ key_block_id: 'kb-2' })],
    source_anchors: [anchor()],
    relationships: [],
  };

  it('counts entities + candidates for the indicator', () => {
    expect(entryCountOf(graph, 1)).toBe(3);
    expect(entryCountOf(undefined, 0)).toBe(0);
  });

  it('renders a screen-reader-friendly summary', () => {
    expect(graphSummary(graph, 1)).toMatch(/2 entities, 0 relationships, 1 source anchor, 1 pending candidate/);
    expect(graphSummary(undefined, 0)).toMatch(/not loaded/);
  });
});
