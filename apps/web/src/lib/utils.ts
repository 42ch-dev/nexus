import { clsx, type ClassValue } from 'clsx';
import { twMerge } from 'tailwind-merge';

/**
 * Merge Tailwind classes with conditional logic.
 *
 * Standard shadcn/ui helper. `cn` is the single entry point for composing
 * component classNames so DESIGN.md tokens (Tailwind theme keys) resolve
 * correctly and conflicting utilities are de-duplicated.
 */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
