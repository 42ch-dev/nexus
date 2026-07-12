import { clsx, type ClassValue } from 'clsx';
import { extendTailwindMerge } from 'tailwind-merge';

/**
 * Custom tailwind-merge instance.
 *
 * DESIGN.md defines custom typography tokens under `fontSize` (e.g.
 * `text-button-14`, `text-heading-32`). tailwind-merge does not know these are
 * font-size classes, so it treats them as text-color classes and will drop a
 * real text-color class like `text-white` when a font-size class appears later.
 * We register all custom `text-*` tokens as font-size class groups so color and
 * size utilities coexist correctly.
 */
const customTwMerge = extendTailwindMerge({
  extend: {
    classGroups: {
      'font-size': [
        'text-heading-32',
        'text-heading-24',
        'text-heading-20',
        'text-heading-16',
        'text-label-14',
        'text-label-12',
        'text-copy-16',
        'text-copy-14',
        'text-copy-13',
        'text-button-14',
        'text-button-12',
        'text-label-12-mono',
        'text-copy-13-mono',
      ],
      // V1.113 P1: DESIGN.md custom token-backed utilities so cn() does not drop them.
      opacity: ['opacity-disabled'],
      'max-h': ['max-h-listbox'],
    },
  },
});

/**
 * Merge Tailwind classes with conditional logic.
 *
 * Standard shadcn/ui helper. `cn` is the single entry point for composing
 * component classNames so DESIGN.md tokens (Tailwind theme keys) resolve
 * correctly and conflicting utilities are de-duplicated.
 */
export function cn(...inputs: ClassValue[]): string {
  return customTwMerge(clsx(inputs));
}
