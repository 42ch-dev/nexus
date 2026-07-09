import { forwardRef, type LabelHTMLAttributes } from 'react';

import { cn } from '../lib/cn';

/**
 * Label — DESIGN.md §Component Primitives. label-14 weight 500; uses
 * gray-1000 text and is wired to its control via the standard `htmlFor`.
 *
 * Per the V1.100 form-field contract:
 * - `htmlFor` is app-owned — the label receives it as a standard prop and
 *   does not generate, cache, or own IDs.
 * - Required/optional indicators (e.g. "*", "(required)") are app-owned copy
 *   rendered by the app alongside the label, not by this component.
 * - Nesting (`<label><input /></label>`) is allowed by spec but the default
 *   composition pattern uses explicit `htmlFor` + `id` association.
 */
export type LabelProps = LabelHTMLAttributes<HTMLLabelElement>;

export const Label = forwardRef<HTMLLabelElement, LabelProps>(
  ({ className, ...props }, ref) => (
    <label
      ref={ref}
      className={cn('text-label-14 font-medium text-gray-1000', className)}
      {...props}
    />
  ),
);
Label.displayName = 'Label';
