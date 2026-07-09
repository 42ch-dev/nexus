/**
 * Re-export `cn` from the @42ch/nexus-ui package — the V1.100 class-merge SSOT.
 *
 * The `extendTailwindMerge` configuration with DESIGN.md font-size token class
 * groups lives in one place: `packages/nexus-ui/src/lib/cn.ts`.
 *
 * Call-sites import from `@/lib/utils` (this file) for compatibility;
 * deep imports from `@42ch/nexus-ui/src/*` are forbidden.
 */
export { cn } from '@42ch/nexus-ui';
