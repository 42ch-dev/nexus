import { cva, type VariantProps } from 'class-variance-authority';
import { forwardRef, type HTMLAttributes } from 'react';

import { cn } from '../lib/cn';

const badgeVariants = cva(
  'inline-flex items-center gap-1 rounded-pill border px-2 h-6 text-label-12 font-semibold whitespace-nowrap',
  {
    variants: {
      variant: {
        neutral: 'bg-gray-alpha-100 text-gray-900 border-gray-alpha-300',
        running:
          'bg-[color-mix(in_srgb,var(--color-green-700)_10%,transparent)] text-green-1000 border-[color-mix(in_srgb,var(--color-green-700)_30%,transparent)]',
        queued:
          'bg-[color-mix(in_srgb,var(--color-teal-700)_10%,transparent)] text-teal-1000 border-[color-mix(in_srgb,var(--color-teal-700)_30%,transparent)]',
        warning:
          'bg-[color-mix(in_srgb,var(--color-amber-700)_12%,transparent)] text-amber-1000 border-[color-mix(in_srgb,var(--color-amber-700)_30%,transparent)]',
        error:
          'bg-[color-mix(in_srgb,var(--color-red-700)_12%,transparent)] text-red-1000 border-[color-mix(in_srgb,var(--color-red-700)_30%,transparent)]',
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
