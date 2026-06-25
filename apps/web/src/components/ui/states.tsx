import { Loader2 } from 'lucide-react';
import type { ReactNode } from 'react';

import { cn } from '@/lib/utils';

/**
 * Spinner — DESIGN.md §Motion. A small rotating loader used in loading states.
 * Honors prefers-reduced-motion via the global CSS rule (animation-duration 0).
 */
export function Spinner({ className }: { className?: string }) {
  return (
    <Loader2
      className={cn('h-4 w-4 animate-spin text-gray-700', className)}
      aria-hidden
    />
  );
}

/**
 * LoadingState — present participle + ellipsis per DESIGN.md §Voice & Content.
 * Use inside a card/section while a query is pending.
 */
export function LoadingState({ label = 'Loading…' }: { label?: string }) {
  return (
    <div className="flex items-center gap-2 py-6 text-copy-14 text-gray-700">
      <Spinner />
      <span>{label}</span>
    </div>
  );
}

/**
 * EmptyState — DESIGN.md §Voice & Content (sentence case, first action).
 * Points the author to the next step (e.g. "No works yet. Create a Work to
 * start the local loop.").
 */
export function EmptyState({
  title,
  description,
  action,
  className,
}: {
  title: string;
  description?: string;
  action?: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn('flex flex-col items-center justify-center gap-2 py-16 text-center', className)}>
      <p className="text-heading-16 font-heading text-gray-1000">{title}</p>
      {description && <p className="max-w-sm text-copy-14 text-gray-900">{description}</p>}
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}

/**
 * ErrorState — DESIGN.md §Voice & Content error pattern: what happened + what
 * to do next. The transport `message` already came from the shared
 * ErrorResponse via NexusClientError (W-1 fix).
 */
export function ErrorState({
  title = 'Could not load this view',
  description,
  onRetry,
  retryLabel = 'Try again',
}: {
  title?: string;
  description?: string;
  onRetry?: () => void;
  retryLabel?: string;
}) {
  return (
    <div
      role="alert"
      className="flex flex-col gap-2 rounded-card border border-[color-mix(in_srgb,var(--color-red-700)_30%,transparent)] bg-[color-mix(in_srgb,var(--color-red-700)_6%,transparent)] p-4"
    >
      <p className="text-heading-16 font-heading text-red-1000">{title}</p>
      {description && <p className="text-copy-14 text-red-900">{description}</p>}
      {onRetry && (
        <button
          type="button"
          onClick={onRetry}
          className="self-start text-label-14 font-medium text-blue-700 transition-colors duration-state ease-standard hover:text-blue-800"
        >
          {retryLabel}
        </button>
      )}
    </div>
  );
}
