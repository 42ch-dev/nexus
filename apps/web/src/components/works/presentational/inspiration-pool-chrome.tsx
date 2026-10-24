/**
 * Inspiration Pool chrome — presentational extract (v1.222 P1 T2).
 *
 * Props-driven Card: header copy, the single-field add form (the add request
 * carries `title` and nothing else) and the pool rows with their status-driven
 * actions. No daemon hooks, no contracts, no router, no `useTranslation` —
 * `WorksInspirationPage` maps query data + locale copy to props, and the Design
 * Studio fixtures import this via `@web-works/*`.
 *
 * Action contract mirrors the service: Promote only for `idea` rows, Archive
 * for `idea` and `promoted`, `archived` rows are read-only, and an unexpected
 * status renders verbatim with no actions. There is no `set_default` control —
 * promotion always activates the new pool entry server-side.
 */
import { useId, type ReactNode } from 'react';
import { ArrowUpRight, Lightbulb } from 'lucide-react';

import {
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  EmptyState,
  ErrorState,
  Input,
  Label,
  LoadingState,
  type BadgeProps,
} from '@42ch/nexus-ui';

export type InspirationPoolChromeState = 'ready' | 'empty' | 'loading' | 'error';

/** Caller-owned copy (App i18n / Studio literal English). */
export interface InspirationPoolCopy {
  title: string;
  description: string;
  listAriaLabel: string;
  loadingLabel: string;
  emptyTitle: string;
  emptyDescription: string;
  errorTitle: string;
  errorDescription: string;
  retryLabel: string;
  addTitle: string;
  titleLabel: string;
  titlePlaceholder: string;
  submitLabel: string;
  submittingLabel: string;
  addedLabel: string;
  openWork: string;
  promoteLabel: string;
  promotingLabel: string;
  archiveLabel: string;
  archivingLabel: string;
}

export interface InspirationPoolRow {
  itemId: string;
  title: string;
  /** Stored status — opaque; only `idea` / `promoted` / `archived` are known. */
  status: string;
  /** Translated status, or the raw status when it is not one of the three. */
  statusLabel: string;
  /** Formatted added date. */
  addedText: string;
  /** Present once the item was promoted to a Work. */
  workId?: string;
}

export interface InspirationPoolWorkLinkArgs {
  workId: string;
  className: string;
  children: ReactNode;
}

export interface InspirationPoolChromeProps {
  copy: InspirationPoolCopy;
  state?: InspirationPoolChromeState;
  rows: InspirationPoolRow[];
  /** Response `total` for the rendered page. */
  total?: number;
  onRetry?: () => void;
  addTitleValue: string;
  onAddTitleChange: (value: string) => void;
  onAdd: (title: string) => void;
  adding?: boolean;
  /** Row id with an in-flight promote / archive (drives the pending label). */
  promotingItemId?: string | null;
  archivingItemId?: string | null;
  onPromote?: (row: InspirationPoolRow) => void;
  onArchive?: (row: InspirationPoolRow) => void;
  /** Host-owned Work link — the App injects a router `Link`. */
  renderWorkLink?: (args: InspirationPoolWorkLinkArgs) => ReactNode;
  'data-testid'?: string;
}

const STATUS_BADGE_VARIANT: Readonly<Record<string, BadgeProps['variant']>> = {
  idea: 'queued',
  promoted: 'running',
  archived: 'neutral',
};

const ROW_ACTION_CLASS_NAME =
  'inline-flex items-center gap-1.5 rounded-control border border-gray-500 bg-background-100 px-2 h-8 text-button-12 font-button text-gray-1000 transition-colors duration-state ease-standard hover:bg-background-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-1000 dark:focus-visible:ring-blue-700';

function DefaultWorkLink({ workId, className, children }: InspirationPoolWorkLinkArgs) {
  return (
    <a href={`/works/${encodeURIComponent(workId)}/outline`} className={className}>
      {children}
    </a>
  );
}

function InspirationPoolRowItem({
  row,
  copy,
  promoting,
  archiving,
  onPromote,
  onArchive,
  renderWorkLink,
}: {
  row: InspirationPoolRow;
  copy: InspirationPoolCopy;
  promoting: boolean;
  archiving: boolean;
  onPromote?: (row: InspirationPoolRow) => void;
  onArchive?: (row: InspirationPoolRow) => void;
  renderWorkLink?: (args: InspirationPoolWorkLinkArgs) => ReactNode;
}) {
  const canPromote = row.status === 'idea';
  const canArchive = row.status === 'idea' || row.status === 'promoted';
  const workLink = renderWorkLink ?? DefaultWorkLink;

  return (
    <li
      className="flex flex-col gap-3 rounded-card border border-gray-alpha-400 p-3 sm:flex-row sm:items-center"
      data-testid="inspiration-pool-row"
      data-item-id={row.itemId}
      data-status={row.status}
    >
      <span className="flex min-w-0 flex-1 items-start gap-2">
        <Lightbulb className="mt-0.5 h-4 w-4 shrink-0 text-gray-700" aria-hidden />
        <span className="flex min-w-0 flex-col gap-1">
          <span className="truncate text-label-14 font-medium text-gray-1000">
            {row.title}
          </span>
          <span className="flex flex-wrap items-center gap-2 text-copy-13 text-gray-700">
            <Badge
              variant={STATUS_BADGE_VARIANT[row.status] ?? 'neutral'}
              data-testid="inspiration-pool-row-status"
            >
              {row.statusLabel}
            </Badge>
            <span data-testid="inspiration-pool-row-added">
              {copy.addedLabel}: {row.addedText}
            </span>
          </span>
        </span>
      </span>

      <span className="flex flex-wrap items-center gap-2">
        {row.status === 'promoted' && row.workId ? (
          workLink({
            workId: row.workId,
            className: ROW_ACTION_CLASS_NAME,
            children: (
              <>
                <ArrowUpRight className="h-3.5 w-3.5" aria-hidden />
                {copy.openWork}
              </>
            ),
          })
        ) : null}

        {canPromote ? (
          <Button
            type="button"
            variant="primary"
            size="small"
            disabled={promoting}
            aria-busy={promoting || undefined}
            onClick={() => onPromote?.(row)}
            data-testid="inspiration-pool-row-promote"
          >
            {promoting ? copy.promotingLabel : copy.promoteLabel}
          </Button>
        ) : null}

        {canArchive ? (
          <Button
            type="button"
            variant="secondary"
            size="small"
            disabled={archiving}
            aria-busy={archiving || undefined}
            onClick={() => onArchive?.(row)}
            data-testid="inspiration-pool-row-archive"
          >
            {archiving ? copy.archivingLabel : copy.archiveLabel}
          </Button>
        ) : null}
      </span>
    </li>
  );
}

/**
 * Presentational Inspiration Pool — header, add form, rows and the
 * loading / empty / error frames. i18n-free; every string arrives as a prop.
 */
export function InspirationPoolChrome({
  copy,
  state = 'ready',
  rows,
  total,
  onRetry,
  addTitleValue,
  onAddTitleChange,
  onAdd,
  adding = false,
  promotingItemId,
  archivingItemId,
  onPromote,
  onArchive,
  renderWorkLink,
  'data-testid': testId = 'inspiration-pool-chrome',
}: InspirationPoolChromeProps) {
  const titleFieldId = `${useId()}-inspiration-title`;
  const addDisabled = adding || addTitleValue.trim().length === 0;

  return (
    <Card className="shadow-card" data-testid={testId}>
      <CardHeader>
        <CardTitle voice="content">{copy.title}</CardTitle>
        <CardDescription>{copy.description}</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-6">
        <form
          className="flex flex-col gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            if (!addDisabled) onAdd(addTitleValue.trim());
          }}
          data-testid="inspiration-pool-add-form"
        >
          <h3 className="text-label-14 font-medium text-gray-1000">{copy.addTitle}</h3>
          <Label htmlFor={titleFieldId}>{copy.titleLabel}</Label>
          <div className="flex flex-col gap-2 sm:flex-row">
            <Input
              id={titleFieldId}
              value={addTitleValue}
              onChange={(event) => onAddTitleChange(event.target.value)}
              placeholder={copy.titlePlaceholder}
              disabled={adding}
              autoComplete="off"
            />
            <Button
              type="submit"
              variant="primary"
              disabled={addDisabled}
              aria-busy={adding || undefined}
              data-testid="inspiration-pool-add-submit"
            >
              {adding ? copy.submittingLabel : copy.submitLabel}
            </Button>
          </div>
        </form>

        {state === 'loading' ? (
          <LoadingState label={copy.loadingLabel} />
        ) : state === 'error' ? (
          <ErrorState
            title={copy.errorTitle}
            description={copy.errorDescription}
            retryLabel={copy.retryLabel}
            onRetry={onRetry}
          />
        ) : state === 'empty' || rows.length === 0 ? (
          <EmptyState title={copy.emptyTitle} description={copy.emptyDescription} />
        ) : (
          <>
            <ul className="flex flex-col gap-2" aria-label={copy.listAriaLabel}>
              {rows.map((row) => (
                <InspirationPoolRowItem
                  key={row.itemId}
                  row={row}
                  copy={copy}
                  promoting={promotingItemId === row.itemId}
                  archiving={archivingItemId === row.itemId}
                  onPromote={onPromote}
                  onArchive={onArchive}
                  renderWorkLink={renderWorkLink}
                />
              ))}
            </ul>
            {typeof total === 'number' ? (
              <p
                className="text-copy-13 text-gray-700"
                data-testid="inspiration-pool-count"
              >
                {/* mechanic 7 pins no count key; the list label names what is counted */}
                <span className="sr-only">{copy.listAriaLabel}: </span>
                {rows.length} / {total}
              </p>
            ) : null}
          </>
        )}
      </CardContent>
    </Card>
  );
}
