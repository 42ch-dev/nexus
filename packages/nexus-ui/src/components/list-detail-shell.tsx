import { type ReactNode } from 'react';

import { cn } from '../lib/cn';

/**
 * ListDetailShell — DESIGN.md §Component Primitives/ListDetailShell.
 *
 * V1.150 P3 — L2 pattern component: the fixed-width responsive master/detail
 * split behind findings-page and modules-page (stack below lg, 360px detail
 * column at lg). The shell owns no open/close state: the caller renders the
 * detail panel or a "no selection" EmptyState in `detail`, and selection
 * state stays caller-owned. No drag-resize — no consumer implements it.
 * Pure presentational: both regions are caller-owned ReactNode slots; no
 * query, router, or i18n imports.
 */
export interface ListDetailShellProps {
  /** Master list region. */
  list: ReactNode;
  /** Detail region — the caller renders the panel or a "no selection" EmptyState. */
  detail: ReactNode;
  className?: string;
}

export function ListDetailShell({ list, detail, className }: ListDetailShellProps) {
  return (
    <div className={cn('grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_360px]', className)}>
      <div>{list}</div>
      <div>{detail}</div>
    </div>
  );
}
