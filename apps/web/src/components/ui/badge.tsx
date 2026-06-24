import { cva, type VariantProps } from 'class-variance-authority';
import { forwardRef, type HTMLAttributes } from 'react';

import { cn } from '@/lib/utils';

/**
 * Badge / Status Pill — DESIGN.md §Component Primitives/Badge.
 *
 * Height 24px, px-2 (8px), radius-pill, label-12. Variant backgrounds/borders
 * use the semantic accent at low alpha. Because DESIGN.md tokens are solid
 * hex CSS variables (not split channels), alpha layers are expressed with
 * `color-mix(...)` so the same class stays correct in both light and dark.
 */
const badgeVariants = cva(
  'inline-flex items-center gap-1 rounded-pill border px-2 h-6 text-label-12 font-semibold whitespace-nowrap',
  {
    variants: {
      variant: {
        // neutral: gray-alpha-100 bg, gray-900 text, gray-alpha-300 border
        neutral: 'bg-gray-alpha-100 text-gray-900 border-gray-alpha-300',
        // running: green-700 @10% bg, green-1000 text, green-700 @30% border
        running:
          'bg-[color-mix(in_srgb,var(--color-green-700)_10%,transparent)] text-green-1000 border-[color-mix(in_srgb,var(--color-green-700)_30%,transparent)]',
        // queued: teal-700 @10% / teal-1000 / teal-700 @30%
        queued:
          'bg-[color-mix(in_srgb,var(--color-teal-700)_10%,transparent)] text-teal-1000 border-[color-mix(in_srgb,var(--color-teal-700)_30%,transparent)]',
        // warning: amber-700 @12% / amber-1000 / amber-700 @30%
        warning:
          'bg-[color-mix(in_srgb,var(--color-amber-700)_12%,transparent)] text-amber-1000 border-[color-mix(in_srgb,var(--color-amber-700)_30%,transparent)]',
        // error: red-700 @12% / red-1000 / red-700 @30%
        error:
          'bg-[color-mix(in_srgb,var(--color-red-700)_12%,transparent)] text-red-1000 border-[color-mix(in_srgb,var(--color-red-700)_30%,transparent)]',
        // preset: purple-700 @10% / purple-1000 / purple-700 @30%
        preset:
          'bg-[color-mix(in_srgb,var(--color-purple-700)_10%,transparent)] text-purple-1000 border-[color-mix(in_srgb,var(--color-purple-700)_30%,transparent)]',
      },
    },
    defaultVariants: { variant: 'neutral' },
  },
);

export interface BadgeProps
  extends HTMLAttributes<HTMLSpanElement>,
    VariantProps<typeof badgeVariants> {}

export const Badge = forwardRef<HTMLSpanElement, BadgeProps>(
  ({ className, variant, ...props }, ref) => (
    <span ref={ref} className={cn(badgeVariants({ variant }), className)} {...props} />
  ),
);
Badge.displayName = 'Badge';

export { badgeVariants };
