/**
 * World KB relationship confidence banding (V1.76 γ).
 *
 * PM-locked stepped bands (NOT continuous linear): low / mid / high at the
 * 0.4 / 0.7 thresholds. Each band maps to a DESIGN.md confidence token color
 * + a stroke-width / opacity pair for edge rendering. The color band IS the
 * proportion signal — badge size stays uniform (8px).
 *
 * See compass §Phase 2b "Confidence-weighting UX (product-manager countersigned)".
 */
import type { CSSProperties } from 'react';

/** The three stepped confidence bands. */
export type ConfidenceBand = 'low' | 'mid' | 'high';

/** Low band threshold: confidence strictly below this is `low`. */
export const CONFIDENCE_LOW_CEIL = 0.4;
/** Mid band ceiling: confidence strictly below this (and ≥ low ceil) is `mid`. */
export const CONFIDENCE_MID_CEIL = 0.7;

/**
 * Classify a confidence value into a stepped band.
 *
 * - `< 0.4` → `low`
 * - `0.4 .. < 0.7` → `mid`
 * - `>= 0.7` → `high`
 *
 * `undefined` / `null` confidence (manual author-created edges without a
 * confidence) classify as `high` so they always render at full strength
 * (manual edges are author-asserted — treat them as the strongest signal).
 */
export function confidenceBand(confidence: number | undefined | null): ConfidenceBand {
  if (confidence == null) return 'high';
  if (confidence < CONFIDENCE_LOW_CEIL) return 'low';
  if (confidence < CONFIDENCE_MID_CEIL) return 'mid';
  return 'high';
}

/** DESIGN.md CSS custom property for each band's badge color. */
export const CONFIDENCE_BAND_COLOR_VAR: Record<ConfidenceBand, string> = {
  low: 'var(--color-canvas-worldkb-relationship-confidence-low)',
  mid: 'var(--color-canvas-worldkb-relationship-confidence-mid)',
  high: 'var(--color-canvas-worldkb-relationship-confidence-high)',
};

/** Human-readable band label (Title Case per DESIGN.md voice). */
export const CONFIDENCE_BAND_LABEL: Record<ConfidenceBand, string> = {
  low: 'Low',
  mid: 'Medium',
  high: 'High',
};

/** Edge stroke width (px) per band (PM-locked: 1 / 2 / 3). */
const CONFIDENCE_BAND_STROKE: Record<ConfidenceBand, number> = {
  low: 1,
  mid: 2,
  high: 3,
};

/** Edge stroke opacity per band (PM-locked: 0.3 / 0.6 / 1.0). */
const CONFIDENCE_BAND_OPACITY: Record<ConfidenceBand, number> = {
  low: 0.3,
  mid: 0.6,
  high: 1.0,
};

/**
 * Build a React Flow edge `style` object from a confidence band + base stroke
 * color. Applies the PM-locked stroke-width + opacity. When `dashed` is true
 * (suggested / `needs_review` edges), the edge also gets a dashed stroke.
 */
export function confidenceEdgeStyle(
  band: ConfidenceBand,
  strokeColor: string,
  dashed: boolean,
): CSSProperties {
  const style: CSSProperties = {
    stroke: strokeColor,
    strokeWidth: CONFIDENCE_BAND_STROKE[band],
    opacity: CONFIDENCE_BAND_OPACITY[band],
  };
  if (dashed) {
    style.strokeDasharray = '6 4';
  }
  return style;
}

/**
 * Format a confidence value for display (e.g. `0.82`), or `—` when absent.
 */
export function formatConfidence(confidence: number | undefined | null): string {
  if (confidence == null) return '—';
  return confidence.toFixed(2);
}
