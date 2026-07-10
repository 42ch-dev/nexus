import { forwardRef, type SelectHTMLAttributes } from 'react';

import { cn } from '../lib/cn';

/**
 * Select — DESIGN.md §Component Primitives/Select (native `<select>`).
 *
 * Height 40px, background-100, gray-1000 text, gray-alpha-400 border,
 * radius-control. Asymmetric inline padding (`ps-3 pe-8`) insets the native
 * chevron from the right border in closed, disabled, and invalid states.
 * Error variant uses red-700 border. Disabled: gray-100 fill, gray-700 text. The two-layer focus ring is global (consumer index.css).
 *
 * Per the V1.101 Select promotion contract:
 * - Native `<select>` + app-owned `<option>` / `<optgroup>` children.
 * - No Radix Trigger/Value/Item compound parts; no package `open` API.
 * - `id` is app-owned — the control receives it via the standard prop.
 * - `aria-invalid` is mapped from the visual `invalid` prop via
 *   `invalid || undefined` coercion (false/omitted → attribute omitted).
 * - `aria-describedby` is app-owned — the app concatenates helper/error IDs
 *   and passes them via the standard prop.
 * - Helper text, error text, and required/optional copy are app-owned.
 */
export interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> {
  /** Marks the field invalid: switches border to red-700 and sets aria-invalid="true". */
  invalid?: boolean;
}

export const Select = forwardRef<HTMLSelectElement, SelectProps>(
  ({ className, invalid, children, ...props }, ref) => (
    <select
      ref={ref}
      aria-invalid={invalid || undefined}
      className={cn(
        'h-10 w-full rounded-control border bg-background-100 ps-3 pe-8 text-copy-14 text-gray-1000 transition-colors duration-state ease-standard',
        'focus-visible:border-blue-700',
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
