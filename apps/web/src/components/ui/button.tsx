import { Slot } from '@radix-ui/react-slot';
import { cva, type VariantProps } from 'class-variance-authority';
import { forwardRef, type ButtonHTMLAttributes } from 'react';

import { cn } from '@/lib/utils';

/**
 * Button — DESIGN.md §Component Primitives/Button.
 *
 * Variants map to the design-system token table (primary/secondary/tertiary/
 * destructive); sizes map to the small/default/large heights. The two-layer
 * focus ring is applied globally in src/index.css.
 */
const buttonVariants = cva(
  'inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-control font-button transition-colors duration-state ease-standard disabled:pointer-events-none disabled:bg-gray-100 disabled:text-gray-700 focus-visible:outline-none',
  {
    variants: {
      variant: {
        // primary: blue-700 bg, white text; hover blue-800
        primary: 'bg-blue-700 text-white hover:bg-blue-800 active:bg-blue-900',
        // secondary: background-100 bg, gray-1000 text, gray-alpha-400 border
        secondary:
          'bg-background-100 text-gray-1000 border border-gray-alpha-400 hover:bg-background-200 hover:border-gray-alpha-500',
        // tertiary: transparent, gray-1000 text; hover gray-alpha-100
        tertiary: 'bg-transparent text-gray-1000 hover:bg-gray-alpha-100',
        // destructive: red-800 bg, white text; hover red-700
        destructive: 'bg-red-800 text-white hover:bg-red-700 active:bg-red-900',
      },
      size: {
        // small: 32px height + button-12
        small: 'h-8 px-3 text-button-12',
        // default: 40px height + button-14
        default: 'h-10 px-4 text-button-14',
        // large: 48px height + button-14
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
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : 'button';
    return (
      <Comp className={cn(buttonVariants({ variant, size }), className)} ref={ref} {...props} />
    );
  },
);
Button.displayName = 'Button';

export { buttonVariants };
