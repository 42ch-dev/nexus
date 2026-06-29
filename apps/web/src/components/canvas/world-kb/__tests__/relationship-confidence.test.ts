/**
 * V1.76 γ confidence banding + suggested-edge rendering tests.
 */
import { describe, it, expect } from 'vitest';

import {
  confidenceBand,
  confidenceEdgeStyle,
  formatConfidence,
  CONFIDENCE_LOW_CEIL,
  CONFIDENCE_MID_CEIL,
} from '../relationship-confidence';
import { deriveRelationshipEdges } from '../relationship-projection';
import type { WorldKbRelationshipProjection } from '@42ch/nexus-contracts';

function rel(overrides: Partial<WorldKbRelationshipProjection> = {}): WorldKbRelationshipProjection {
  return {
    relationship_id: 'rel_1',
    world_id: 'wld',
    source_entity_id: 'kb_a',
    target_entity_id: 'kb_b',
    relation_type: 'allied_with',
    symmetric: false,
    source_anchor_ids: [],
    needs_review: false,
    source: 'manual',
    version: 0,
    updated_at: '2026-06-30T00:00:00Z',
    projection_direction: 'stored',
    ...overrides,
  };
}

describe('confidenceBand', () => {
  it('classifies low band below 0.4', () => {
    expect(confidenceBand(0.0)).toBe('low');
    expect(confidenceBand(0.39)).toBe('low');
  });

  it('classifies mid band from 0.4 to below 0.7', () => {
    expect(confidenceBand(0.4)).toBe('mid');
    expect(confidenceBand(0.69)).toBe('mid');
  });

  it('classifies high band at or above 0.7', () => {
    expect(confidenceBand(0.7)).toBe('high');
    expect(confidenceBand(1.0)).toBe('high');
  });

  it('treats absent confidence as high (author-asserted full strength)', () => {
    expect(confidenceBand(undefined)).toBe('high');
    expect(confidenceBand(null)).toBe('high');
  });

  it('uses the locked thresholds 0.4 / 0.7', () => {
    expect(CONFIDENCE_LOW_CEIL).toBe(0.4);
    expect(CONFIDENCE_MID_CEIL).toBe(0.7);
  });
});

describe('confidenceEdgeStyle', () => {
  it('applies stepped stroke width + opacity per band', () => {
    const low = confidenceEdgeStyle('low', '#000', false);
    const mid = confidenceEdgeStyle('mid', '#000', false);
    const high = confidenceEdgeStyle('high', '#000', false);
    expect(low.strokeWidth).toBe(1);
    expect(low.opacity).toBe(0.3);
    expect(mid.strokeWidth).toBe(2);
    expect(mid.opacity).toBe(0.6);
    expect(high.strokeWidth).toBe(3);
    expect(high.opacity).toBe(1.0);
  });

  it('adds dashed stroke for suggested edges', () => {
    const dashed = confidenceEdgeStyle('high', '#000', true);
    expect(dashed.strokeDasharray).toBe('6 4');
    const solid = confidenceEdgeStyle('high', '#000', false);
    expect(solid.strokeDasharray).toBeUndefined();
  });
});

describe('formatConfidence', () => {
  it('formats a number to two decimals', () => {
    expect(formatConfidence(0.827)).toBe('0.83');
  });
  it('renders em-dash for absent confidence', () => {
    expect(formatConfidence(undefined)).toBe('—');
    expect(formatConfidence(null)).toBe('—');
  });
});

describe('deriveRelationshipEdges (V1.76)', () => {
  it('renders suggested edges as dashed with a suggested label marker', () => {
    const edges = deriveRelationshipEdges([rel({ needs_review: true, source: 'extraction' })]);
    expect(edges).toHaveLength(1);
    expect(edges[0].style?.strokeDasharray).toBe('6 4');
    expect(String(edges[0].label)).toContain('suggested');
    expect(edges[0].data?.needsReview).toBe(true);
  });

  it('renders confirmed edges as solid', () => {
    const edges = deriveRelationshipEdges([rel({ needs_review: false })]);
    expect(edges[0].style?.strokeDasharray).toBeUndefined();
    expect(edges[0].data?.needsReview).toBe(false);
  });

  it('applies confidence-stepped stroke width', () => {
    const lowEdge = deriveRelationshipEdges([rel({ confidence: 0.2 })]);
    const highEdge = deriveRelationshipEdges([rel({ confidence: 0.9 })]);
    expect(lowEdge[0].style?.strokeWidth).toBe(1);
    expect(highEdge[0].style?.strokeWidth).toBe(3);
  });
});
