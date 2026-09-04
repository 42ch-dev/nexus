import { cva, type VariantProps } from 'class-variance-authority';
import { type HTMLAttributes, type Ref } from 'react';

import { cn } from '../lib/cn';

/**
 * Badge / Status Pill — DESIGN.md §Component Primitives/Badge.
 *
 * Height 24px, px-2 (8px), radius-pill, label-12. Soft tone keeps tinted fills
 * with strengthened borders (raised to 16% alpha in FB-V1106-001 so each hue
 * reads distinctly) — the per-variant fill/text/border triple is projected as
 * `nexus-ui-badge-soft-*` tokens (v1.183 P0 R-V1121P1QC1-S001), resolving via
 * color-mix so the same class stays correct in both light and dark. Solid
 * tone uses semantic fills + high-contrast text; light solid fills sit one
 * step darker (`-800`) so white text clears AA, with `dark:bg-*-700` pins
 * keeping the bright dark fills (v1.183 P0 R-V1121P1T3-S001, AR-3). Dark
 * solid text follows the Button Contrast Invariant (bright fills →
 * `brand-deep-blue`, not white). State changes (variant/tone or theme swap)
 * ease over duration-state per the v0.4 motion scale.
 */
const badgeVariants = cva(
  'inline-flex items-center gap-1 rounded-pill border px-2 h-6 text-label-12 font-semibold whitespace-nowrap transition-colors duration-state ease-standard',
  {
    variants: {
      variant: {
        neutral: '',
        running: '',
        queued: '',
        warning: '',
        error: '',
        preset: '',
      },
      tone: {
        soft: '',
        solid: 'border-transparent',
      },
    },
    compoundVariants: [
      // ── soft (default): tinted fill + semantic text; strengthened borders ──
      {
        tone: 'soft',
        variant: 'neutral',
        class: 'bg-gray-alpha-100 text-gray-900 border-gray-alpha-400',
      },
      {
        tone: 'soft',
        variant: 'running',
        class:
          'bg-nexus-ui-badge-soft-running-bg text-nexus-ui-badge-soft-running-text border-nexus-ui-badge-soft-running-border',
      },
      {
        tone: 'soft',
        variant: 'queued',
        class:
          'bg-nexus-ui-badge-soft-queued-bg text-nexus-ui-badge-soft-queued-text border-nexus-ui-badge-soft-queued-border',
      },
      {
        tone: 'soft',
        variant: 'warning',
        class:
          'bg-nexus-ui-badge-soft-warning-bg text-nexus-ui-badge-soft-warning-text border-nexus-ui-badge-soft-warning-border',
      },
      {
        tone: 'soft',
        variant: 'error',
        class:
          'bg-nexus-ui-badge-soft-error-bg text-nexus-ui-badge-soft-error-text border-nexus-ui-badge-soft-error-border',
      },
      {
        tone: 'soft',
        variant: 'preset',
        class:
          'bg-nexus-ui-badge-soft-preset-bg text-nexus-ui-badge-soft-preset-text border-nexus-ui-badge-soft-preset-border',
      },
      // ── solid (opt-in): semantic fill + high-contrast text; no visible border ──
      // Light: white on dark fills. Dark: deep-blue on bright semantic fills
      // (Button Contrast Invariant); neutral keeps white on dark gray-200.
      // Light fills sit at -800 (white text clears AA, v1.183 P0 AR-3);
      // `dark:bg-*-700` pins keep the bright dark-theme fills unchanged.
      {
        tone: 'solid',
        variant: 'neutral',
        class: 'bg-gray-1000 text-white dark:bg-gray-200 dark:text-white',
      },
      {
        tone: 'solid',
        variant: 'running',
        class: 'bg-green-800 text-white dark:bg-green-700 dark:text-brand-deep-blue',
      },
      {
        tone: 'solid',
        variant: 'queued',
        class: 'bg-teal-800 text-white dark:bg-teal-700 dark:text-brand-deep-blue',
      },
      {
        tone: 'solid',
        variant: 'warning',
        class: 'bg-amber-800 text-white dark:bg-amber-700 dark:text-brand-deep-blue',
      },
      {
        tone: 'solid',
        variant: 'error',
        class: 'bg-red-800 text-white dark:text-brand-deep-blue',
      },
      {
        tone: 'solid',
        variant: 'preset',
        class: 'bg-purple-700 text-white dark:text-brand-deep-blue',
      },
    ],
    defaultVariants: { variant: 'neutral', tone: 'soft' },
  },
);

export interface BadgeProps
  extends HTMLAttributes<HTMLSpanElement>,
    VariantProps<typeof badgeVariants> {
  /** DOM ref forwarded to the underlying span (React 19 ref-as-prop). */
  ref?: Ref<HTMLSpanElement>;
}

export function Badge({ className, variant, tone, ref, ...props }: BadgeProps) {
  return <span ref={ref} className={cn(badgeVariants({ variant, tone }), className)} {...props} />;
}
Badge.displayName = 'Badge';
