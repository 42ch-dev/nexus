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
 *
 * V1.121 v0.4 adds the display tier (`text-display-*`), the `font-display`
 * family utility, the named box-shadow tokens (`shadow-elevation-*` plus the
 * legacy `shadow-card/popover/modal` aliases), the directional motion pair
 * (`duration-enter/exit`), and the canvas node width family
 * (`min-w-canvas-node-*`) so each merges within its own class group instead of
 * being misparsed (font-size → text-color) or never conflicting (unknown
 * classes are kept forever, so e.g. `duration-enter` + `duration-200` would
 * both survive). Threat model: the V1.94 silent-strip class of bug.
 */
const customTwMerge = extendTailwindMerge({
  extend: {
    classGroups: {
      'font-size': [
        'text-display-32',
        'text-display-24',
        'text-display-20',
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
      // V1.121 v0.4: content-voice serif family utility (DESIGN.md typography.font-display).
      'font-family': ['font-display'],
      // V1.121 v0.4: named box-shadow tokens — elevation scale + legacy aliases
      // (DESIGN.md §Elevation). Registering the aliases too keeps
      // `cn('shadow-elevation-2', 'shadow-card')` collapsing to one class.
      shadow: [
        'shadow-elevation-0',
        'shadow-elevation-1',
        'shadow-elevation-2',
        'shadow-elevation-3',
        'shadow-elevation-4',
        'shadow-card',
        'shadow-popover',
        'shadow-modal',
      ],
      // V1.121 v0.4: directional motion pair (DESIGN.md §Motion).
      duration: ['duration-enter', 'duration-exit'],
      // V1.121 v0.4: canvas node width family (DESIGN.md components.canvas.node-width;
      // registered in P0 alongside the `--canvas-node-width-*` structural CSS
      // vars + preset minWidth keys, verified/consumed by P2's sweep, applied
      // to node components in P3).
      'min-w': [
        'min-w-canvas-node-strategy-root',
        'min-w-canvas-node-strategy-primary',
        'min-w-canvas-node-strategy-secondary',
        'min-w-canvas-node-outline-scene-beat',
        'min-w-canvas-node-default',
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
