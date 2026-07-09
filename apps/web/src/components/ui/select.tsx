/**
 * Select — DESIGN.md §Component Primitives/Select.
 *
 * Re-exported from @42ch/nexus-ui (V1.101 P2 promotion).
 * The package owns the presentational implementation; this file is a thin
 * re-export wrapper to avoid call-site churn in apps/web.
 * Add app-specific Select behavior (options, labels, data wiring, validation)
 * here only — never in the package.
 */
export { Select, type SelectProps } from '@42ch/nexus-ui';
