/**
 * @nexus/design-tokens — shared token compiler type declarations.
 *note: 
 * Exposes the internal tooling signatures for the Studio Vite plugin and the
 * Node generation/check scripts. This is NOT a package public API surface.
 */

/** Parsed repo-root DESIGN pair frontmatter (light + dark). */
export interface DesignPair {
  light: Record<string, unknown>;
  dark: Record<string, unknown>;
}

/** Derived projection outputs written to checked-in artifacts. */
export interface TokenProjection {
  /** note: tooling/design-tokens/src/tokens.css */
  css: string;
  /** note: packages/nexus-ui/theme.css */
  brandCss: string;
  /** note: packages/nexus-ui/src/generated-brand.ts */
  brandTokens: string;
}

/** Read both DESIGN files and parse their frontmatter (enforces parity). */
export function loadDesignPair(repoRoot: string): Promise<DesignPair>;

/** Project the DESIGN pair into the three derived output strings. */
export function projectDesign(pair: DesignPair): TokenProjection;
