import { forwardRef, type LabelHTMLAttributes } from 'react';

import { cn } from '@/lib/utils';

/**
 * Label — DESIGN.md §Component Primitives. label-14 weight 500; uses
 * gray-1000 text and is wired to its control via the standard `htmlFor`.
 */
export const Label = forwardRef<HTMLLabelElement, LabelHTMLAttributes<HTMLLabelElement>>(
  ({ className, ...props }, ref) => (
    <label
      ref={ref}
      className={cn('text-label-14 font-medium text-gray-1000', className)}
      {...props}
    />
  ),
);
Label.displayName = 'Label';
