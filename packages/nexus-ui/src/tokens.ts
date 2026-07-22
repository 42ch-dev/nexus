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
  /** Plain primary timeline mark — bright cyan gradient, no plate (`logo-primary.png` mark geometry) */
  primary: 'logo-primary.svg',
  /** Plain white-bg gradient mark — no plate; use on light surfaces when a plate is wrong */
  whiteBg: 'logo-white-bg.svg',
  /** Timeline mark — dark-gray→white gradient for dark heroes / high-contrast panels */
  white: 'logo-white.svg',
  /** Timeline mark — light-gray→black gradient (static asset; tintable form is `<NexusMark>`) */
  mono: 'logo-mono.svg',
  /** Wordmark — lowercase `nexus`; inherits via currentColor */
  text: 'logo-text.svg',
} as const;

export type LogoVariantName = keyof typeof logoVariants;

/** Square plate lockups — separate asset contract from plain wide marks. */
export const logoSquareVariants = {
  /** Primary lockup — bright mark on brand deep-blue plate (matches `logo-primary.png`) */
  primary: 'logo-primary-square.svg',
  /** Lockup on white plate — only when a light/white surface is required (`logo-white-bg.png`) */
  whiteBg: 'logo-white-bg-square.svg',
} as const;

export type LogoSquareVariantName = keyof typeof logoSquareVariants;

/** Timeline mark viewBox (matches `assets/logos/logo-*.svg` marks) */
export const logoMarkViewBoxWidth = 284;
export const logoMarkViewBoxHeight = 28;
/** Width / height of the timeline mark viewBox (~10.14:1) */
export const logoMarkAspectRatio = logoMarkViewBoxWidth / logoMarkViewBoxHeight;

/** Minimum rendered logo height in px for legibility (gallery / general UI) */
export const logoMinSizePx = 24;

/**
 * Legacy shell chrome lockup height in px (pre-compact baseline for gallery comparison).
 * Plate lockups use square assets (`logoSquareVariants`); timeline marks are wide.
 */
export const logoShellHeightPx = 20;

/**
 * Compact timeline mark height in px — shared SSOT for titlebar, Brand hero, and app icon.
 * −30% from {@link logoShellHeightPx}; wide marks size by height only.
 */
export const logoCompactMarkHeightPx = Math.round(logoShellHeightPx * 0.7);

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
