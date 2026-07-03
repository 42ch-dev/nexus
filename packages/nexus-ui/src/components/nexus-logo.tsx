/**
 * Pure presentational Nexus wordmark — renders a consumer-resolved SVG asset.
 *
 * The component is intentionally bundler-agnostic: it does NOT import `.svg`
 * files in its source. Consumers resolve the asset through their own bundler
 * (e.g. Vite) and pass the resulting URL as `src`.
 */

import { logoVariants, type LogoVariantName } from '../tokens';

/** Backward-compatible alias for the canonical logo variant name. */
export type Variant = LogoVariantName;

/** Backward-compatible alias; canonical definition lives in {@link ../tokens}. */
export { logoVariants as VARIANT_FILENAMES };

export interface NexusLogoProps {
  /** Which brand variant to render. */
  variant: Variant;
  /** Consumer-resolved SVG URL. */
  src: string;
  /** Accessible label. Defaults to the product name. */
  label?: string;
  className?: string;
  /** Rendered height in px. Defaults to 32. */
  size?: number;
}

export function NexusLogo({
  variant,
  src,
  label = 'Nexus',
  className,
  size = 32,
}: NexusLogoProps) {
  return (
    <img
      src={src}
      alt={label}
      height={size}
      decoding="async"
      className={className}
    />
  );
}
