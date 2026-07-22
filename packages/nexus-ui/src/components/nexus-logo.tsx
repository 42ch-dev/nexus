/**
 * Pure presentational Nexus wordmark — renders a consumer-resolved SVG asset.
 *
 * The component is intentionally bundler-agnostic: it does NOT import `.svg`
 * files in its source. Consumers resolve the asset through their own bundler
 * (e.g. Vite) and pass the resulting URL as `src`.
 *
 * Variant groups:
 * - Square plate lockups: `primary`, `whiteBg` (width-fill in gallery fixtures).
 * - Timeline marks: `white`, `mono` (wide aspect — size by height only).
 * - Wordmark: `text` — always pair with consumer-resolved `logo-text.svg`.
 *
 * When UI needs the Nexus **logo text** (lowercase wordmark), use `variant="text"`
 * with `logo-text.svg`. Do not typeset "nexus" / "Nexus" with UI fonts as a brand
 * substitute. Apps/web exposes this as `NexusTextLogo`.
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
  /** When false, prevents native browser image drag (e.g. titlebar chrome). */
  draggable?: boolean;
}

export function NexusLogo({
  variant,
  src,
  label = 'Nexus',
  className,
  size = 32,
  draggable,
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
      draggable={draggable}
      className={className}
      style={{ width: 'auto', height: size }}
    />
  );
}
