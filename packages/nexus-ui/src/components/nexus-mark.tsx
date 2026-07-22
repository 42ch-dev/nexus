/**
 * Inline tintable Nexus mark — timeline geometry shared with `logo-mono.svg`.
 *
 * `logo-mono.svg` bakes the provenance grayscale gradient (light→black). This
 * component keeps a flat `currentColor` stroke/fill so buttons, badges, and
 * list rows can tint the mark. Wide timeline geometry (viewBox 0 0 284 28);
 * height-driven sizing is `w-auto` friendly.
 */

import { memo, useId } from 'react';

import {
  logoMarkAspectRatio,
  logoMarkViewBoxHeight,
  logoMarkViewBoxWidth,
  logoMinSizePx,
} from '../tokens';

export interface NexusMarkProps {
  /** Accessible label for the mark. Defaults to the product name. */
  label?: string;
  className?: string;
  /**
   * Rendered height in px. Width follows the timeline aspect ratio
   * (`logoMarkAspectRatio`) so the mark stays wide; override with CSS
   * `w-auto` + an explicit height class when preferred.
   */
  size?: number;
}

function NexusMarkImpl({
  label = 'Nexus',
  className,
  size = logoMinSizePx,
}: NexusMarkProps) {
  const titleId = useId();
  const width = size * logoMarkAspectRatio;

  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox={`0 0 ${logoMarkViewBoxWidth} ${logoMarkViewBoxHeight}`}
      role="img"
      width={width}
      height={size}
      className={className}
      aria-labelledby={titleId}
      style={{ width: 'auto', height: size, aspectRatio: `${logoMarkViewBoxWidth} / ${logoMarkViewBoxHeight}` }}
    >
      <title id={titleId}>{label}</title>
      <g fill="none" stroke="currentColor" strokeLinecap="butt">
        {/* Axis segments between node outer edges */}
        <path
          strokeWidth={3.5}
          d="M28 14 H64 M92 14 H128 M156 14 H192 M220 14 H256"
        />
        {/* Ring nodes (left pair + right pair) */}
        <circle cx={14} cy={14} r={12.125} strokeWidth={3.75} />
        <circle cx={78} cy={14} r={12.125} strokeWidth={3.75} />
        <circle cx={206} cy={14} r={12.125} strokeWidth={3.75} />
        <circle cx={270} cy={14} r={12.125} strokeWidth={3.75} />
      </g>
      {/* Solid center node */}
      <circle cx={142} cy={14} r={14} fill="currentColor" />
    </svg>
  );
}

/**
 * Static SVG with no derived state. Memoized defensively for future high-render
 * surfaces (lists, animations) without adding measurable cost today.
 */
export const NexusMark = memo(NexusMarkImpl);
