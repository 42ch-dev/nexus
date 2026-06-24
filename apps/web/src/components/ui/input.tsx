import { forwardRef, type InputHTMLAttributes } from 'react';

import { cn } from '@/lib/utils';

/**
 * Input — DESIGN.md §Component Primitives/Input.
 *
 * Height 40px, background-100, gray-1000 text, gray-alpha-400 border,
 * radius-control. Error variant uses red-700 border. Disabled: gray-100 fill,
 * gray-700 text. The two-layer focus ring is global (src/index.css).
 */
export interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  /** Marks the field invalid and switches the border to red-700. */
  invalid?: boolean;
}

export const Input = forwardRef<HTMLInputElement, InputProps>(
  ({ className, invalid, ...props }, ref) => (
    <input
      ref={ref}
      aria-invalid={invalid || undefined}
      className={cn(
        'h-10 w-full rounded-control border bg-background-100 px-3 text-copy-14 text-gray-1000 transition-colors duration-state ease-standard',
        'placeholder:text-gray-700',
        'disabled:bg-gray-100 disabled:text-gray-700 disabled:cursor-not-allowed',
        invalid ? 'border-red-700' : 'border-gray-alpha-400',
        className,
      )}
      {...props}
    />
  ),
);
Input.displayName = 'Input';
