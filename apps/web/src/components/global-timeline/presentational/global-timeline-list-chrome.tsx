/**
 * Global Timeline list chrome — presentational extract (V1.124 P2).
 *
 * Props-driven Card + World activity rows. No daemon hooks, no contracts,
 * no router, no `useTranslation`. App `GlobalTimelineView` maps query data →
 * row props; Design Studio fixtures import via `@web-global-timeline/*`.
 *
 * App hosts may pass `renderRow` to wrap each row in a react-router `Link`
 * (SPA navigation). Default row is a button (or plain `<a>` when `href` is set).
 */
import { type KeyboardEvent, type ReactNode } from 'react';
import { CalendarRange, Loader2 } from 'lucide-react';

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@42ch/nexus-ui';

export type GlobalTimelineLayer = 'brief' | 'narrative';

export interface GlobalTimelineListRow {
  id: string;
  label: string;
  activityText: string;
  lastEditedText?: string;
  layer?: GlobalTimelineLayer;
  /** Optional plain href when the host does not supply `renderRow`. */
  href?: string;
}

export type GlobalTimelineListState = 'ready' | 'empty' | 'loading' | 'error';

export type RenderGlobalTimelineRow = (
  row: GlobalTimelineListRow,
  className: string,
  content: ReactNode,
) => ReactNode;

export interface GlobalTimelineListChromeProps {
  title: string;
  description: string;
  listAriaLabel: string;
  rows: GlobalTimelineListRow[];
  state?: GlobalTimelineListState;
  emptyTitle: string;
  emptyDescription: string;
  loadingLabel?: string;
  errorDescription?: string;
  retryLabel?: string;
  onRetry?: () => void;
  /** Host-owned row wrapper (e.g. react-router `Link`). */
  renderRow?: RenderGlobalTimelineRow;
  onRowActivate?: (row: GlobalTimelineListRow) => void;
  'data-testid'?: string;
}

const ROW_CLASS_NAME =
  'flex w-full items-center gap-3 rounded-card border border-gray-alpha-400 p-3 text-left transition-colors duration-state ease-standard hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2';

function RowContent({ row }: { row: GlobalTimelineListRow }) {
  return (
    <>
      <CalendarRange
        className="h-5 w-5 shrink-0 text-blue-700"
        aria-hidden
      />
      <span className="min-w-0 flex-1">
        <span className="block truncate font-display text-display-20 tracking-tight text-gray-1000">
          {row.label}
        </span>
        <span
          className="block truncate text-copy-13 text-gray-700"
          data-testid="global-timeline-row-activity"
        >
          {row.activityText}
        </span>
        {row.lastEditedText ? (
          <span className="block truncate text-copy-13-mono text-gray-700">
            {row.lastEditedText}
          </span>
        ) : null}
      </span>
    </>
  );
}

function DefaultRow({
  row,
  onRowActivate,
}: {
  row: GlobalTimelineListRow;
  onRowActivate?: (row: GlobalTimelineListRow) => void;
}) {
  const content = <RowContent row={row} />;
  const shared = {
    className: ROW_CLASS_NAME,
    'data-testid': 'global-timeline-row' as const,
    'data-world-id': row.id,
    'data-layer': row.layer ?? 'narrative',
  };

  if (row.href) {
    return (
      <a href={row.href} {...shared}>
        {content}
      </a>
    );
  }

  return (
    <button
      type="button"
      {...shared}
      onClick={() => onRowActivate?.(row)}
      onKeyDown={(event: KeyboardEvent<HTMLButtonElement>) => {
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onRowActivate?.(row);
        }
      }}
    >
      {content}
    </button>
  );
}

/**
 * Presentational Global Timeline list — Card chrome + activity rows + empty /
 * loading / error frames. i18n-free; all copy arrives as props.
 */
export function GlobalTimelineListChrome({
  title,
  description,
  listAriaLabel,
  rows,
  state = 'ready',
  emptyTitle,
  emptyDescription,
  loadingLabel = 'Loading…',
  errorDescription = 'Could not load Timeline activity.',
  retryLabel = 'Retry',
  onRetry,
  renderRow,
  onRowActivate,
  'data-testid': testId = 'global-timeline-view',
}: GlobalTimelineListChromeProps) {
  if (state === 'loading') {
    return (
      <div data-testid="global-timeline-loading">
        <div className="flex items-center gap-2 py-6 text-copy-14 text-gray-700">
          <Loader2
            className="h-4 w-4 animate-spin text-blue-700"
            aria-hidden
          />
          <span>{loadingLabel}</span>
        </div>
      </div>
    );
  }

  if (state === 'error') {
    return (
      <div data-testid="global-timeline-error">
        <div
          role="alert"
          className="flex flex-col gap-2 rounded-card border border-error-surface-border bg-error-surface p-4"
        >
          <p className="text-heading-16 font-heading text-red-1000">
            Something went wrong
          </p>
          <p className="text-copy-14 text-red-900">{errorDescription}</p>
          {onRetry ? (
            <button
              type="button"
              onClick={onRetry}
              className="self-start text-label-14 font-medium text-blue-700 transition-colors duration-state ease-standard hover:text-blue-800"
            >
              {retryLabel}
            </button>
          ) : null}
        </div>
      </div>
    );
  }

  return (
    <Card className="shadow-card" data-testid={testId}>
      <CardHeader>
        <CardTitle voice="content">{title}</CardTitle>
        <CardDescription>{description}</CardDescription>
      </CardHeader>
      <CardContent>
        {state === 'empty' || rows.length === 0 ? (
          <div className="flex flex-col items-center justify-center gap-2 py-16 text-center">
            <p className="font-display text-display-24 text-gray-1000">
              {emptyTitle}
            </p>
            <p className="max-w-sm text-copy-14 text-gray-900">
              {emptyDescription}
            </p>
          </div>
        ) : (
          <ul className="flex flex-col gap-2" aria-label={listAriaLabel}>
            {rows.map((row) => {
              const content = <RowContent row={row} />;
              return (
                <li key={row.id}>
                  {renderRow ? (
                    renderRow(row, ROW_CLASS_NAME, content)
                  ) : (
                    <DefaultRow row={row} onRowActivate={onRowActivate} />
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}
