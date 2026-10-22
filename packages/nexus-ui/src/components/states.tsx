import { Loader2 } from 'lucide-react';
import type { ReactNode } from 'react';

import { cn } from '../lib/cn';

/**
 * States — DESIGN.md §Component Primitives/States + §Voice & Content.
 *
 * V1.148 P1 — promoted presentational primitives (from apps/web
 * components/ui/states.tsx). Pure presentational: caller-owned copy via props
 * with English fallbacks matching the app `en` locale; apps/web overrides the
 * fallbacks with its i18n defaults through the wrapper. No app, query, router,
 * or i18n imports. Theme parity comes from `--nexus-*` token projections and
 * explicit `dark:` hooks.
 */

/**
 * Spinner — DESIGN.md §Motion. A small rotating loader used in loading states.
 * Honors prefers-reduced-motion via the global CSS rule (animation-duration 0).
 */
export function Spinner({ className }: { className?: string }) {
  return (
    <Loader2
      className={cn('h-4 w-4 animate-spin text-blue-1000 dark:text-blue-700', className)}
      aria-hidden
    />
  );
}

/**
 * LoadingState — present participle + ellipsis per DESIGN.md §Voice & Content.
 * Use inside a card/section while a query is pending. `label` is caller-owned
 * copy (fallback "Loading…").
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
 * start the local loop."). The headline is content voice (serif display tier)
 * per §Design Concept — "empty-state headlines on authoring surfaces".
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
      <p className="font-display text-display-24 text-gray-1000">{title}</p>
      {description && <p className="max-w-sm text-copy-14 text-gray-900">{description}</p>}
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}

/**
 * UnavailableState — orchestration engine idle or not started. Neutral tone
 * (not `ErrorState` crash framing) with an optional Retry action.
 * `retryLabel` is caller-owned copy (fallback "Try again").
 */
export function UnavailableState({
  title,
  description,
  onRetry,
  retryLabel = 'Try again',
  action,
  className,
}: {
  title: string;
  description?: string;
  onRetry?: () => void;
  retryLabel?: string;
  action?: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        'flex flex-col items-center justify-center gap-2 py-16 text-center',
        className,
      )}
    >
      <p className="font-display text-display-24 text-gray-1000">{title}</p>
      {description && <p className="max-w-sm text-copy-14 text-gray-900">{description}</p>}
      {(onRetry ?? action) && (
        <div className="mt-2 flex flex-wrap items-center justify-center gap-2">
          {onRetry && (
            <button
              type="button"
              onClick={onRetry}
              className="text-label-14 font-medium text-brand-deep-blue transition-colors duration-state ease-standard hover:opacity-80 dark:text-blue-700 dark:hover:text-blue-800 dark:hover:opacity-100"
            >
              {retryLabel}
            </button>
          )}
          {action}
        </div>
      )}
    </div>
  );
}

/**
 * ErrorState — DESIGN.md §Voice & Content error pattern: what happened + what
 * to do next. The transport `message` already came from the shared
 * ErrorResponse via NexusClientError (W-1 fix). `title` / `retryLabel` are
 * caller-owned copy (fallbacks "Could not load this view" / "Try again").
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
      className="flex flex-col gap-2 rounded-card border border-error-surface-border bg-error-surface p-4"
    >
      <p className="text-heading-16 font-heading text-red-1000">{title}</p>
      {description && <p className="text-copy-14 text-red-900">{description}</p>}
      {onRetry && (
        <button
          type="button"
          onClick={onRetry}
          className="self-start text-label-14 font-medium text-brand-deep-blue transition-colors duration-state ease-standard hover:opacity-80 dark:text-blue-700 dark:hover:text-blue-800 dark:hover:opacity-100"
        >
          {retryLabel}
        </button>
      )}
    </div>
  );
}
