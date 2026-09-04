import type { LucideIcon } from 'lucide-react';

import { cn } from '@/lib/utils';

/**
 * Card-sized empty-state create affordance — matches list-item card footprint
 * (V1.125 P2 AC-V1125-5). Used on Worlds/Works index pages when the library
 * is empty so the primary action is as prominent as a content card.
 */
export function EmptyCreateCard({
  icon: Icon,
  title,
  description,
  onClick,
  disabled,
  titleAttr,
  className,
  'data-testid': testId,
}: {
  icon: LucideIcon;
  title: string;
  description: string;
  onClick: () => void;
  disabled?: boolean;
  /** Native tooltip when disabled (e.g. desktop-only gate). */
  titleAttr?: string;
  className?: string;
  'data-testid'?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      tabIndex={disabled ? -1 : undefined}
      title={titleAttr}
      data-testid={testId}
      className={cn(
        'flex w-full min-h-[7.5rem] flex-col items-center justify-center gap-2 rounded-card border border-dashed border-gray-alpha-400 p-6 text-center motion-reduce:transition-none',
        disabled
          ? 'cursor-not-allowed opacity-disabled'
          : 'transition-colors duration-state ease-standard hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2',
        className,
      )}
    >
      <Icon className="h-8 w-8 shrink-0 text-brand-deep-blue dark:text-blue-700" aria-hidden />
      <span className="font-display text-display-20 tracking-tight text-gray-1000">{title}</span>
      <span className="max-w-sm text-copy-14 text-gray-700">{description}</span>
    </button>
  );
}
