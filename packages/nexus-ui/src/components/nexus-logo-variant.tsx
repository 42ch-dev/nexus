/**
 * Studio-only timeline mark specimens — palette-driven theme moods.
 *
 * Renders the same wide timeline geometry as `<NexusMark>` with a left→right
 * gradient from palette props (or `logoVariantPalettes` defaults). No SVG/PNG
 * asset imports and no runtime theme switcher — presentational only.
 */

import { memo, useId } from 'react';

import {
  logoMarkAspectRatio,
  logoMarkViewBoxHeight,
  logoMarkViewBoxWidth,
  logoMinSizePx,
  logoVariantPalettes,
  type LogoVariantPalette,
  type LogoVariantTheme,
} from '../tokens';

export type { LogoVariantPalette, LogoVariantTheme };

export interface NexusLogoVariantProps {
  /**
   * Specimen mood id. Selects a default palette from `logoVariantPalettes`
   * unless `palette` is provided. Not a product theme preference.
   */
  theme?: LogoVariantTheme;
  /** Explicit gradient stops; overrides `theme` defaults when set. */
  palette?: LogoVariantPalette;
  /** Accessible label for the mark. Defaults to the product name. */
  label?: string;
  className?: string;
  /** Rendered height in px. Width follows the timeline aspect ratio. */
  size?: number;
}

function resolvePalette(
  theme: LogoVariantTheme,
  palette: LogoVariantPalette | undefined,
): LogoVariantPalette {
  return palette ?? logoVariantPalettes[theme];
}

function NexusLogoVariantImpl({
  theme = 'elegant',
  palette,
  label = 'Nexus',
  className,
  size = logoMinSizePx,
}: NexusLogoVariantProps) {
  const titleId = useId();
  const gradientId = `nexus-logo-variant-${useId().replace(/:/g, '')}`;
  const { start, end } = resolvePalette(theme, palette);
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
      style={{
        width: 'auto',
        height: size,
        aspectRatio: `${logoMarkViewBoxWidth} / ${logoMarkViewBoxHeight}`,
      }}
    >
      <title id={titleId}>{label}</title>
      <defs>
        <linearGradient
          id={gradientId}
          x1={0}
          y1={logoMarkViewBoxHeight / 2}
          x2={logoMarkViewBoxWidth}
          y2={logoMarkViewBoxHeight / 2}
          gradientUnits="userSpaceOnUse"
        >
          <stop offset="0%" stopColor={start} />
          <stop offset="100%" stopColor={end} />
        </linearGradient>
      </defs>
      <g fill="none" stroke={`url(#${gradientId})`} strokeLinecap="butt">
        <path
          strokeWidth={3.5}
          d="M28 14 H64 M92 14 H128 M156 14 H192 M220 14 H256"
        />
        <circle cx={14} cy={14} r={12.125} strokeWidth={3.75} />
        <circle cx={78} cy={14} r={12.125} strokeWidth={3.75} />
        <circle cx={206} cy={14} r={12.125} strokeWidth={3.75} />
        <circle cx={270} cy={14} r={12.125} strokeWidth={3.75} />
      </g>
      <circle cx={142} cy={14} r={14} fill={`url(#${gradientId})`} />
    </svg>
  );
}

/**
 * Static specimen SVG. Memoized for gallery grids without measurable cost.
 */
export const NexusLogoVariant = memo(NexusLogoVariantImpl);
