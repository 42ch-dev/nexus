/**
 * Nexus brand token constants — V1.83 foundation slice.
 * Normative cross-application values are defined in root DESIGN.md (P1);
 * this module exposes machine-consumable brand primitives for package consumers.
 */

export const brandColors = {
  deepBlue: '#1E3A5F',
  cyan: '#25D1E0',
  white: '#FFFFFF',
} as const;

export type BrandColorName = keyof typeof brandColors;

export const logoVariants = {
  /** Cyan mark — primary brand color on light surfaces or accent on dark hero */
  color: 'logo-color.svg',
  /** Deep blue mark — navigation and light-background shells */
  dark: 'logo-dark.svg',
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
