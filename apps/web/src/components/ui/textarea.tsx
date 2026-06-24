import { forwardRef, type TextareaHTMLAttributes } from 'react';

import { cn } from '@/lib/utils';

/**
 * Textarea — DESIGN.md §Component Primitives/Textarea. Min height 96px,
 * background-100, gray-1000 text, gray-alpha-400 border, radius-control.
 */
export interface TextareaProps extends TextareaHTMLAttributes<HTMLTextAreaElement> {
  /** Marks the field invalid and switches the border to red-700. */
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
        'disabled:bg-gray-100 disabled:text-gray-700 disabled:cursor-not-allowed',
        invalid ? 'border-red-700' : 'border-gray-alpha-400',
        className,
      )}
      {...props}
    />
  ),
);
Textarea.displayName = 'Textarea';
