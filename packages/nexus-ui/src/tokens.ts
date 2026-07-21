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
  /** Deep blue mark (flat primary color) — navigation and light-background shells */
  primary: 'logo-primary.svg',
  /** Cyan mark — bright logo for dark backgrounds / dark chrome */
  color: 'logo-color.svg',
  /** White mark — dark hero, photography overlays, high-contrast panels */
  white: 'logo-white.svg',
  /** Monotone mark — inline UI; inherits `color` via currentColor */
  mono: 'logo-mono.svg',
} as const;

export type LogoVariantName = keyof typeof logoVariants;

/** Minimum rendered logo height in px for legibility */
export const logoMinSizePx = 24;

/** Recommended clear space around the mark (multiple of logo height) */
export const logoClearSpaceRatio = 0.25;
