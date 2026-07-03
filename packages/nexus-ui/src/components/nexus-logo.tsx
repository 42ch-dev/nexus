/**
 * Pure presentational Nexus wordmark — renders a consumer-resolved SVG asset.
 *
 * The component is intentionally bundler-agnostic: it does NOT import `.svg`
 * files in its source. Consumers resolve the asset through their own bundler
 * (e.g. Vite) and pass the resulting URL as `src`.
 */

export type Variant = 'primary' | 'color' | 'white' | 'mono';

/** Maps each logo variant to its canonical asset filename. */
export const VARIANT_FILENAMES: Record<Variant, string> = {
  primary: 'logo-primary.svg',
  color: 'logo-color.svg',
  white: 'logo-white.svg',
  mono: 'logo-mono.svg',
};

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
      width="auto"
      decoding="async"
      className={className}
    />
  );
}
