import { type ReactNode } from 'react';

import { cn } from '../lib/cn';
import { EmptyState, ErrorState, LoadingState } from './states';

/**
 * ListStateContainer — DESIGN.md §Component Primitives/ListStateContainer.
 *
 * V1.149 P1 — L2 pattern component: the single four-branch container behind
 * list/query surfaces (loading / error / empty / ready). Composes the
 * promoted States primitives, so theme parity and voice come from their
 * token recipes. Pure presentational: status + caller-owned copy in,
 * children rendered only on ready; no query, router, or i18n imports.
 *
 * Empty vs "no selection context" is distinguished by the caller through
 * `emptyTitle` / `emptyHint` copy, not by this component.
 */
export interface ListStateContainerProps {
  status: 'loading' | 'error' | 'empty' | 'ready';
  /** Retry callback for the error branch (promoted ErrorState `onRetry`). */
  retry?: () => void;
  /** Empty-branch action slot (e.g. an EmptyCreateCard affordance). */
  emptyAction?: ReactNode;
  /** Empty-branch hint copy (promoted EmptyState description). */
  emptyHint?: string;
  /** Empty-branch headline copy (promoted EmptyState title; English fallback). */
  emptyTitle?: string;
  /** Loading-branch copy (promoted LoadingState label; fallback "Loading…"). */
  loadingLabel?: string;
  /** Error-branch headline copy (promoted ErrorState title; fallback set there). */
  errorTitle?: string;
  /** Retry control copy (promoted ErrorState retryLabel; fallback "Try again"). */
  retryLabel?: string;
  children: ReactNode;
  className?: string;
}

export function ListStateContainer({
  status,
  retry,
  emptyAction,
  emptyHint,
  emptyTitle = 'Nothing here yet',
  loadingLabel,
  errorTitle,
  retryLabel,
  children,
  className,
}: ListStateContainerProps) {
  return (
    <div className={cn(className)}>
      {status === 'loading' && <LoadingState label={loadingLabel} />}
      {status === 'error' && (
        <ErrorState title={errorTitle} onRetry={retry} retryLabel={retryLabel} />
      )}
      {status === 'empty' && (
        <EmptyState title={emptyTitle} description={emptyHint} action={emptyAction} />
      )}
      {status === 'ready' && children}
    </div>
  );
}
