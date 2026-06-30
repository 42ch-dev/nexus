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
import { deriveRelationshipEdges, filterRelationshipEdgesByConfidence } from '../relationship-projection';
import type { Edge } from '@xyflow/react';
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

// Regression for qc3-W3: the threshold filter compares the slider value
// directly against `0.0`–`1.0` confidence (slider range 0.0–1.0, step 0.05).
// An earlier revision stored the slider as 0–100 and compared it against 0–1
// confidence, hiding every confirmed edge at the first non-zero step.
describe('filterRelationshipEdgesByConfidence (qc3-W3 regression)', () => {
  function edgesFor(confidence: number | undefined, needsReview = false): Edge[] {
    return deriveRelationshipEdges([
      rel({ confidence: confidence ?? undefined, needs_review: needsReview, source: needsReview ? 'extraction' : 'manual' }),
    ]);
  }

  it('shows all edges when threshold is 0', () => {
    const edges = [...edgesFor(0.2), ...edgesFor(0.9)];
    expect(filterRelationshipEdgesByConfidence(edges, 0)).toHaveLength(2);
  });

  it('hides a confirmed edge whose confidence is below the threshold', () => {
    // confidence 0.5 vs threshold 0.7 → hidden (this is the qc3-W3 bug: under
    // the old 0–100 slider, threshold 0.7 was stored as 70 and 0.5 >= 70 was
    // always false, so the edge vanished; with the 0.0–1.0 slider it correctly
    // hides only because 0.5 < 0.7).
    const filtered = filterRelationshipEdgesByConfidence(edgesFor(0.5), 0.7);
    expect(filtered).toHaveLength(0);
  });

  it('keeps a confirmed edge whose confidence meets the threshold', () => {
    const filtered = filterRelationshipEdgesByConfidence(edgesFor(0.8), 0.7);
    expect(filtered).toHaveLength(1);
  });

  it('always keeps suggested edges regardless of threshold (triage invariant)', () => {
    const filtered = filterRelationshipEdgesByConfidence(edgesFor(0.1, true), 0.7);
    expect(filtered).toHaveLength(1);
  });

  it('always keeps manual edges without a confidence value', () => {
    const filtered = filterRelationshipEdgesByConfidence(edgesFor(undefined), 0.99);
    expect(filtered).toHaveLength(1);
  });

  it('a threshold of 0.05 no longer wipes out a 0.5-confidence edge', () => {
    // Direct regression: under the buggy 0–100 slider, step "0.05" (label) was
    // stored as 5, and confidence 0.5 >= 5 was false → edge hidden. With the
    // normalized 0.0–1.0 slider, 0.5 >= 0.05 is true → edge kept.
    const filtered = filterRelationshipEdgesByConfidence(edgesFor(0.5), 0.05);
    expect(filtered).toHaveLength(1);
  });
});
