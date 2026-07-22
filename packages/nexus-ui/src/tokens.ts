/**
 * Nexus brand token constants — V1.83 foundation slice.
 * Normative cross-application values are defined in root DESIGN.md (P1);
 * this module exposes machine-consumable brand primitives for package consumers.
 */

export const brandColors = {
  deepBlue: '#0D2B3E',
  cyan: '#25D1E0',
  white: '#FFFFFF',
} as const;

export type BrandColorName = keyof typeof brandColors;

export const logoVariants = {
  /** Primary lockup — bright mark on brand deep-blue plate (default; matches `logo-primary.png`) */
  primary: 'logo-primary.svg',
  /** Lockup on white plate — only when a light/white surface is required (`logo-white-bg.png`) */
  whiteBg: 'logo-white-bg.svg',
  /** Timeline mark — dark-gray→white gradient for dark heroes / high-contrast panels */
  white: 'logo-white.svg',
  /** Timeline mark — light-gray→black gradient (static asset; tintable form is `<NexusMark>`) */
  mono: 'logo-mono.svg',
  /** Wordmark — lowercase `nexus`; inherits via currentColor */
  text: 'logo-text.svg',
} as const;

export type LogoVariantName = keyof typeof logoVariants;

/** Timeline mark viewBox (matches `assets/logos/logo-*.svg` marks) */
export const logoMarkViewBoxWidth = 284;
export const logoMarkViewBoxHeight = 28;
/** Width / height of the timeline mark viewBox (~10.14:1) */
export const logoMarkAspectRatio = logoMarkViewBoxWidth / logoMarkViewBoxHeight;

/** Minimum rendered logo height in px for legibility (gallery / general UI) */
export const logoMinSizePx = 24;

/**
 * Shell chrome lockup height in px (sidebar header, presentational shell fixtures).
 * Default shell uses the square `primary` plate (viewBox 284×284); at 20px height
 * the lockup fits typical sidebar chrome without clipping.
 */
export const logoShellHeightPx = 20;

/** Recommended clear space around the mark (multiple of logo height) */
export const logoClearSpaceRatio = 0.25;

/** Studio-only theme specimen ids (not a runtime theme switcher) */
export type LogoVariantTheme = 'elegant' | 'nature' | 'parchment' | 'scifi';

/** Left→right gradient stops for theme specimen marks */
export interface LogoVariantPalette {
  start: string;
  end: string;
}

/**
 * Default palettes for `<NexusLogoVariant>` — mood-derived from provenance
 * `logo-variants-*.png`. Override via `palette` prop; no asset import.
 */
export const logoVariantPalettes: Record<LogoVariantTheme, LogoVariantPalette> = {
  elegant: { start: '#8B7355', end: '#E8C36A' },
  nature: { start: '#1B4332', end: '#52B788' },
  parchment: { start: '#5C4033', end: '#D4B896' },
  scifi: { start: '#3B1F6E', end: '#25D1E0' },
};
