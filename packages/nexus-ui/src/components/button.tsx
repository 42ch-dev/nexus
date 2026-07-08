import { Slot } from '@radix-ui/react-slot';
import { cva, type VariantProps } from 'class-variance-authority';
import { forwardRef, type ButtonHTMLAttributes } from 'react';

import { cn } from '../lib/cn';

const buttonVariants = cva(
  'inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-control font-button transition-colors duration-state ease-standard disabled:pointer-events-none disabled:bg-gray-100 disabled:text-gray-700 focus-visible:outline-none',
  {
    variants: {
      variant: {
        primary:
          'bg-blue-700 text-white hover:bg-blue-800 active:bg-blue-900 dark:bg-brand-cyan dark:text-brand-deep-blue dark:hover:bg-blue-800 dark:active:bg-blue-900',
        secondary:
          'bg-background-100 text-gray-1000 border border-gray-alpha-400 hover:bg-background-200 hover:border-gray-alpha-500',
        tertiary: 'bg-transparent text-gray-1000 hover:bg-gray-alpha-100',
        destructive:
          'bg-red-800 text-white hover:bg-red-700 active:bg-red-900 dark:text-brand-deep-blue',
      },
      size: {
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
