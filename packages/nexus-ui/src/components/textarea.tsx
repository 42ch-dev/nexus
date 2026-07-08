import { forwardRef, type TextareaHTMLAttributes } from 'react';

import { cn } from '../lib/cn';

/**
 * Textarea — DESIGN.md §Component Primitives/Textarea. Min height 96px,
 * background-100, gray-1000 text, gray-alpha-400 border, radius-control.
 *
 * Per the V1.100 form-field contract:
 * - `id` is app-owned — the control receives it via the standard prop.
 * - `aria-invalid` is mapped from the visual `invalid` prop via
 *   `invalid || undefined` coercion (false/omitted → attribute omitted).
 * - `aria-describedby` is app-owned — the app concatenates helper/error IDs
 *   and passes them via the standard prop.
 * - Helper text, error text, and required/optional copy are app-owned.
 */
export interface TextareaProps extends TextareaHTMLAttributes<HTMLTextAreaElement> {
  /** Marks the field invalid: switches border to red-700 and sets aria-invalid="true". */
  invalid?: boolean;
}

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaProps>(
  ({ className, invalid, ...props }, ref) => (
    <textarea
      ref={ref}
      aria-invalid={invalid || undefined}
      className={cn(
        'min-h-24 w-full rounded-control border bg-background-100 p-3 text-copy-14 text-gray-1000 transition-colors duration-state ease-standard',
        'placeholder:text-gray-700',
        'focus-visible:border-blue-700',
        'disabled:bg-gray-100 disabled:text-gray-700 disabled:cursor-not-allowed',
        invalid ? 'border-red-700' : 'border-gray-alpha-400',
        className,
      )}
      {...props}
    />
  ),
);
Textarea.displayName = 'Textarea';
