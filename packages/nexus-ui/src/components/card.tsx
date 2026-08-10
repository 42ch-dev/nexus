import { type HTMLAttributes, type Ref } from 'react';

import { cn } from '../lib/cn';

/**
 * Card — DESIGN.md §Component Primitives/Card.
 *
 * Rest surface: background-100 fill, gray-alpha-400 border, radius-card,
 * space-6 padding, shadow-card (alias → elevation-1).
 *
 * `interactive` applies the v0.4 §Elevation interactive-card recipe: rest
 * elevation-1 → hover elevation-2 + translateY(-1px) over duration-popover
 * (160ms) ease-standard; pressed returns to elevation-1 with the transform
 * removed. Reduced-motion safe: the transform is gated behind `motion-safe:`
 * and transitions drop to instant under `motion-reduce:`. Activation wiring
 * (onClick, role, cursor) stays consumer-owned — this prop is elevation/motion
 * only. Default off: existing call sites render identically.
 */
export interface CardProps extends HTMLAttributes<HTMLDivElement> {
  /** Opts the card into the v0.4 interactive elevation recipe (hover lift). */
  interactive?: boolean;
  /** DOM ref forwarded to the underlying div (React 19 ref-as-prop). */
  ref?: Ref<HTMLDivElement>;
}

function Card({ className, interactive = false, ref, ...props }: CardProps) {
  return (
    <div
      ref={ref}
      className={cn(
        'rounded-card border border-gray-alpha-400 bg-background-100 p-6 text-gray-1000 shadow-card',
        interactive &&
          'transition-[box-shadow,transform] duration-popover ease-standard motion-reduce:transition-none hover:shadow-elevation-2 motion-safe:hover:-translate-y-px active:shadow-elevation-1 motion-safe:active:translate-y-0',
        className,
      )}
      {...props}
    />
  );
}
Card.displayName = 'Card';

function CardHeader({ className, ref, ...props }: HTMLAttributes<HTMLDivElement> & { ref?: Ref<HTMLDivElement> }) {
  return <div ref={ref} className={cn('flex flex-col space-y-1.5 pb-4', className)} {...props} />;
}
CardHeader.displayName = 'CardHeader';

export interface CardTitleProps extends HTMLAttributes<HTMLHeadingElement> {
  /**
   * Typography voice — DESIGN.md `components.card.title.voice` (V1.121 v0.4,
   * additive opt-in per P0 spec T8).
   *
   * - `'interface'` (default): sans `text-heading-16 font-heading` treatment —
   *   unchanged; used on all interface cards (settings, dialogs, dashboards).
   * - `'content'`: serif display tier `font-display text-display-20
   *   tracking-tight` — reserved for cards presenting a creative entity
   *   (work/world/brand-page). Greppable opt-in (`voice="content"`); serif
   *   discipline per §Design Concept.
   */
  voice?: 'interface' | 'content';
  /** DOM ref forwarded to the underlying h3 (React 19 ref-as-prop). */
  ref?: Ref<HTMLHeadingElement>;
}

function CardTitle({ className, voice = 'interface', ref, ...props }: CardTitleProps) {
  return (
    <h3
      ref={ref}
      className={cn(
        // Content voice intentionally omits `leading-tight`: the display-20
        // typography token supplies its own line-height.
        voice === 'content'
          ? 'font-display text-display-20 tracking-tight'
          : 'text-heading-16 font-heading leading-tight tracking-tight',
        className,
      )}
      {...props}
    />
  );
}
CardTitle.displayName = 'CardTitle';

function CardDescription({
  className,
  ref,
  ...props
}: HTMLAttributes<HTMLParagraphElement> & { ref?: Ref<HTMLParagraphElement> }) {
  return <p ref={ref} className={cn('text-copy-14 text-gray-900', className)} {...props} />;
}
CardDescription.displayName = 'CardDescription';

function CardContent({ className, ref, ...props }: HTMLAttributes<HTMLDivElement> & { ref?: Ref<HTMLDivElement> }) {
  return <div ref={ref} className={cn('text-copy-14', className)} {...props} />;
}
CardContent.displayName = 'CardContent';

export { Card, CardHeader, CardTitle, CardDescription, CardContent };
