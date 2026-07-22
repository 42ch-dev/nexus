/**
 * Pure presentational Nexus wordmark — renders a consumer-resolved SVG asset.
 *
 * The component is intentionally bundler-agnostic: it does NOT import `.svg`
 * files in its source. Consumers resolve the asset through their own bundler
 * (e.g. Vite) and pass the resulting URL as `src`.
 *
 * Variants include timeline marks (`primary` | `color` | `white` | `mono`) and
 * the `text` wordmark. All are wide-aspect assets — size by height only.
 */

import { logoVariants, type LogoVariantName } from '../tokens';

/** Backward-compatible alias for the canonical logo variant name. */
export type Variant = LogoVariantName;

/** Backward-compatible alias; canonical definition lives in {@link ../tokens}. */
export { logoVariants as VARIANT_FILENAMES };

export interface NexusLogoProps {
  /** Which brand variant to render (mark or wordmark). */
  variant: Variant;
  /** Consumer-resolved SVG URL. */
  src: string;
  /** Accessible label. Defaults to the product name. */
  label?: string;
  className?: string;
  /**
   * Rendered height in px. Defaults to 32. Width is left auto so wide-aspect
   * marks and the wordmark preserve their intrinsic ratios.
   */
  size?: number;
}

export function NexusLogo({
  variant,
  src,
  label = 'Nexus',
  className,
  size = 32,
}: NexusLogoProps) {
  // `variant` is part of the public contract (documents which asset `src` resolves)
  // and is intentionally unused at render time — the consumer supplies `src`.
  void variant;

  return (
    <img
      src={src}
      alt={label}
      height={size}
      decoding="async"
      className={className}
      style={{ width: 'auto', height: size }}
    />
  );
}
