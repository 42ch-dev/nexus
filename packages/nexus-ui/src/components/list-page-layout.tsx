import { type ReactNode } from 'react';

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from './card';

/**
 * ListPageLayout — DESIGN.md §Component Primitives/ListPageLayout.
 *
 * V1.150 P1 — L2 pattern component: the shared list-page chrome behind
 * findings/works/chapters/sessions/schedule/capabilities/strategies/modules/
 * memory pages (Card > CardHeader title/description + refresh action >
 * filters row > results > load-more footer). Composes the promoted Card
 * primitives, so theme parity and voice come from their token recipes.
 * Pure presentational: copy via string props, visuals via ReactNode slots;
 * filter state, list data, and pagination all stay caller-owned — the caller
 * composes ListStateContainer/Table inside `children` (composition, not
 * prop tunneling). No query, router, or i18n imports.
 */
export interface ListPageLayoutProps {
  /** Page headline copy (CardTitle). */
  title: string;
  /** Page description copy (CardDescription). */
  description?: string;
  /** Filter/toolbar row; Selects/Inputs and their state stay caller-owned. */
  filters?: ReactNode;
  /** Header actions slot (e.g. a refresh Button). */
  actions?: ReactNode;
  /** Results region; the caller composes ListStateContainer/Table here. */
  children: ReactNode;
  /** Pagination/load-more slot (e.g. LoadMore). */
  footer?: ReactNode;
  className?: string;
}

export function ListPageLayout({
  title,
  description,
  filters,
  actions,
  children,
  footer,
  className,
}: ListPageLayoutProps) {
  return (
    <Card className={className}>
      <CardHeader>
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div>
            <CardTitle>{title}</CardTitle>
            {description && <CardDescription>{description}</CardDescription>}
          </div>
          {actions}
        </div>
      </CardHeader>
      <CardContent>
        {filters && <div className="mb-4 flex flex-wrap items-end gap-3">{filters}</div>}
        {children}
        {footer && <div className="mt-4">{footer}</div>}
      </CardContent>
    </Card>
  );
}
