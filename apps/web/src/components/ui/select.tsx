import { forwardRef, type SelectHTMLAttributes } from 'react';

import { cn } from '@/lib/utils';

/**
 * Select — native styled select, DESIGN.md §Component Primitives/Select.
 *
 * Uses the native element for accessibility (keyboard, screen-reader) and
 * applies the DESIGN.md control styling. Height 40px, background-100,
 * gray-1000 text, gray-alpha-400 border, radius-control.
 */
export interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> {
  invalid?: boolean;
}

export const Select = forwardRef<HTMLSelectElement, SelectProps>(
  ({ className, invalid, children, ...props }, ref) => (
    <select
      ref={ref}
      aria-invalid={invalid || undefined}
      className={cn(
        'h-10 w-full rounded-control border bg-background-100 px-3 text-copy-14 text-gray-1000 transition-colors duration-state ease-standard',
        'disabled:bg-gray-100 disabled:text-gray-700 disabled:cursor-not-allowed',
        invalid ? 'border-red-700' : 'border-gray-alpha-400',
        className,
      )}
      {...props}
    >
      {children}
    </select>
  ),
);
Select.displayName = 'Select';
