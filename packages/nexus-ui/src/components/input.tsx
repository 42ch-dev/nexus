import { forwardRef, type InputHTMLAttributes } from 'react';

import { cn } from '../lib/cn';

/**
 * Input — DESIGN.md §Component Primitives/Input.
 *
 * Height 40px, background-100, gray-1000 text, gray-alpha-400 border,
 * radius-control. Error variant uses red-700 border. Disabled (v0.4
 * `input-select-textarea.disabled`): gray-100 fill, gray-700 text,
 * gray-alpha-300 border. The two-layer focus ring is global (src/index.css).
 *
 * Per the V1.100 form-field contract:
 * - `id` is app-owned — the control receives it via the standard prop.
 * - `aria-invalid` is mapped from the visual `invalid` prop via
 *   `invalid || undefined` coercion (false/omitted → attribute omitted).
 * - `aria-describedby` is app-owned — the app concatenates helper/error IDs
 *   and passes them via the standard prop.
 * - Helper text, error text, and required/optional copy are app-owned.
 */
export interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  /** Marks the field invalid: switches border to red-700 and sets aria-invalid="true". */
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
        'focus-visible:border-blue-700',
        'disabled:bg-gray-100 disabled:text-gray-700 disabled:border-gray-alpha-300 disabled:cursor-not-allowed',
        invalid ? 'border-red-700' : 'border-gray-alpha-400',
        className,
      )}
      {...props}
    />
  ),
);
Input.displayName = 'Input';
