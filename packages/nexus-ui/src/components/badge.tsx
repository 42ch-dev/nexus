import { cva, type VariantProps } from 'class-variance-authority';
import { type HTMLAttributes, type Ref } from 'react';

import { cn } from '../lib/cn';

/**
 * Badge / Status Pill — DESIGN.md §Component Primitives/Badge.
 *
 * Height 24px, px-2 (8px), radius-pill, label-12. Soft tone keeps tinted fills
 * with strengthened borders (raised to 16% alpha in FB-V1106-001 so each hue
 * reads distinctly). Solid tone uses semantic fills + high-contrast text.
 * Alpha layers use `color-mix(...)` so the same class stays correct in both
 * light and dark. Dark solid text follows the Button Contrast Invariant
 * (bright fills → `brand-deep-blue`, not white). State changes (variant/tone
 * or theme swap) ease over duration-state per the v0.4 motion scale.
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
          'bg-[color-mix(in_srgb,var(--color-green-700)_16%,transparent)] text-green-1000 border-[color-mix(in_srgb,var(--color-green-700)_50%,transparent)]',
      },
      {
        tone: 'soft',
        variant: 'queued',
        class:
          'bg-[color-mix(in_srgb,var(--color-teal-700)_16%,transparent)] text-teal-1000 border-[color-mix(in_srgb,var(--color-teal-700)_50%,transparent)]',
      },
      {
        tone: 'soft',
        variant: 'warning',
        class:
          'bg-[color-mix(in_srgb,var(--color-amber-700)_16%,transparent)] text-amber-1000 border-[color-mix(in_srgb,var(--color-amber-700)_50%,transparent)]',
      },
      {
        tone: 'soft',
        variant: 'error',
        class:
          'bg-[color-mix(in_srgb,var(--color-red-700)_16%,transparent)] text-red-1000 border-[color-mix(in_srgb,var(--color-red-700)_50%,transparent)]',
      },
      {
        tone: 'soft',
        variant: 'preset',
        class:
          'bg-[color-mix(in_srgb,var(--color-purple-700)_16%,transparent)] text-purple-1000 border-[color-mix(in_srgb,var(--color-purple-700)_50%,transparent)]',
      },
      // ── solid (opt-in): semantic fill + high-contrast text; no visible border ──
      // Light: white on dark fills. Dark: deep-blue on bright semantic fills
      // (Button Contrast Invariant); neutral keeps white on dark gray-200.
      {
        tone: 'solid',
        variant: 'neutral',
        class: 'bg-gray-1000 text-white dark:bg-gray-200 dark:text-white',
      },
      {
        tone: 'solid',
        variant: 'running',
        class: 'bg-green-700 text-white dark:text-brand-deep-blue',
      },
      {
        tone: 'solid',
        variant: 'queued',
        class: 'bg-teal-700 text-white dark:text-brand-deep-blue',
      },
      {
        tone: 'solid',
        variant: 'warning',
        class: 'bg-amber-700 text-white dark:text-brand-deep-blue',
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
