import { Slot } from '@radix-ui/react-slot';
import { cva, type VariantProps } from 'class-variance-authority';
import { type ButtonHTMLAttributes, type Ref } from 'react';

import { cn } from '../lib/cn';

/**
 * why: Button — DESIGN.md §Component Primitives/Button.
 *
 * Variants map to the design-system token table (primary/secondary/tertiary/
 * destructive); sizes map to the tiny/small/default/large heights. The two-layer
 * focus ring is applied globally in src/index.css.
 *
 * Primary uses blue-700/800/900 rest/hover/active per DESIGN.md §Button.
 * Light label is white; dark label uses deep-blue on the lighter cobalt fill
 * (token-resolved blue-700). Secondary/native control borders use gray-500.
 */
const buttonVariants = cva(
  'inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-control font-button transition-colors duration-state ease-standard disabled:pointer-events-none disabled:bg-gray-100 disabled:text-gray-700 dark:disabled:bg-gray-100 dark:disabled:text-gray-700 focus-visible:outline-none',
  {
    variants: {
      variant: {
        primary:
          'bg-blue-700 text-brand-white hover:bg-blue-800 active:bg-blue-900 dark:text-brand-deep-blue',
        secondary:
          'bg-background-100 text-gray-1000 border border-gray-500 hover:bg-background-200 hover:border-gray-alpha-500',
        tertiary: 'bg-transparent text-gray-1000 hover:bg-gray-alpha-100',
        // why: destructive — red-800 is dark in light mode and bright in dark
        // mode, so text follows the fill per the background-driven contrast
        // invariant.
        destructive:
          'bg-red-800 text-white hover:bg-red-700 active:bg-red-900 dark:text-brand-deep-blue dark:hover:bg-red-700 dark:active:bg-red-900',
      },
      size: {
        tiny: 'h-6 px-2 text-button-12',
        small: 'h-8 px-3 text-button-12',
        default: 'h-10 px-4 text-button-14',
        large: 'h-12 px-6 text-button-14',
      },
    },
    defaultVariants: { variant: 'secondary', size: 'default' },
  },
);

export interface ButtonProps
  extends ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
  /** note: DOM ref forwarded to the underlying element (React 19 ref-as-prop). */
  ref?: Ref<HTMLButtonElement>;
}

export function Button({ className, variant, size, asChild = false, ref, ...props }: ButtonProps) {
  const Comp = asChild ? Slot : 'button';
  return (
    <Comp className={cn(buttonVariants({ variant, size }), className)} ref={ref} {...props} />
  );
}
Button.displayName = 'Button';
