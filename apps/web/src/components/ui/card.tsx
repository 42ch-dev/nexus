import { forwardRef, type HTMLAttributes } from 'react';

import { cn } from '@/lib/utils';

/**
 * Card — DESIGN.md §Component Primitives/Card.
 *
 * Default card: background-100 fill, gray-alpha-400 border, radius-card,
 * space-6 (24px) padding, optional shadow-card. Compact variant uses space-4;
 * hero/status variant uses space-8.
 */
const Card = forwardRef<HTMLDivElement, HTMLAttributes<HTMLDivElement>>(
  ({ className, ...props }, ref) => (
    <div
      ref={ref}
      className={cn(
        'rounded-card border border-gray-alpha-400 bg-background-100 p-6 text-gray-1000',
        className,
      )}
      {...props}
    />
  ),
);
Card.displayName = 'Card';

const CardHeader = forwardRef<HTMLDivElement, HTMLAttributes<HTMLDivElement>>(
  ({ className, ...props }, ref) => (
    <div ref={ref} className={cn('flex flex-col space-y-1.5 pb-4', className)} {...props} />
  ),
);
CardHeader.displayName = 'CardHeader';

const CardTitle = forwardRef<HTMLHeadingElement, HTMLAttributes<HTMLHeadingElement>>(
  ({ className, ...props }, ref) => (
    <h3
      ref={ref}
      className={cn('text-heading-16 font-heading leading-tight tracking-tight', className)}
      {...props}
    />
  ),
);
CardTitle.displayName = 'CardTitle';

const CardDescription = forwardRef<HTMLParagraphElement, HTMLAttributes<HTMLParagraphElement>>(
  ({ className, ...props }, ref) => (
    <p ref={ref} className={cn('text-copy-14 text-gray-900', className)} {...props} />
  ),
);
CardDescription.displayName = 'CardDescription';

const CardContent = forwardRef<HTMLDivElement, HTMLAttributes<HTMLDivElement>>(
  ({ className, ...props }, ref) => (
    <div ref={ref} className={cn('text-copy-14', className)} {...props} />
  ),
);
CardContent.displayName = 'CardContent';

export { Card, CardHeader, CardTitle, CardDescription, CardContent };
