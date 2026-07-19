/**
 * Conflict-modal chrome — shared presentational shell (V1.124 P2).
 *
 * Overlay + dialog + field-diff chips + action tray + optional side-by-side
 * review. Props-first strings (no `useTranslation`, no RF, no contracts, no
 * daemon). Domain wrappers (Strategy / Outline / World KB) stay product
 * adapters and supply translated copy.
 *
 * Studio alias: `@web-canvas/conflict-modal-chrome`.
 */
import { useEffect, useId, useRef, useState, type ReactNode } from 'react';
import { AlertTriangle, RefreshCw, Shield, Split } from 'lucide-react';

export interface ConflictField<T extends string = string> {
  id: T;
  label: string;
  /** Server-side value, if known. When undefined the field is not shown as a server change. */
  serverValue?: string;
  /** Local draft value. When undefined the field is not shown as a local change. */
  localValue?: string;
}

export interface ConflictReviewRow {
  label: string;
  server: string;
  draft: string;
  changed: boolean;
}

export interface ConflictModalChromeProps<T extends string = string> {
  open: boolean;
  title: string;
  description?: ReactNode;
  /**
   * Node rendered after the server revision span in the description branch.
   * Additive (V1.73): lets domain variants append a trailing clause without
   * changing existing Strategy/Outline descriptions.
   */
  descriptionSuffix?: ReactNode;
  currentRevision: number;
  /** Fallback description when `description` is omitted. */
  revisionLabel?: string;
  /** Sentence after the revision number when no custom description is provided. */
  defaultDescription?: string;
  serverSectionTitle?: string;
  localSectionTitle?: string;
  serverNoChangesLabel?: string;
  localNoChangesLabel?: string;
  serverChanges: ConflictField<T>[];
  localChanges: ConflictField<T>[];
  reviewRows: ConflictReviewRow[];
  onUseCurrent: () => void;
  onReapply: () => void;
  onDismiss: () => void;
  useCurrentLabel?: string;
  reapplyLabel?: string;
  keepEditingLabel?: string;
  reviewLabel?: string;
  reapplyTitleEnabled?: string;
  reapplyTitleDisabled?: string;
  /** Live-region announcement fragments (screen readers). */
  liveRevisionText?: string;
  liveLocalChangesText?: string;
  liveServerChangesText?: string;
  liveOverlapText?: string;
  liveNoOverlapText?: string;
  liveNothingLabel?: string;
  liveNothingDetectableLabel?: string;
  /** Review row column labels. */
  reviewServerLabel?: (fieldLabel: string) => string;
  reviewLocalLabel?: (fieldLabel: string) => string;
  reviewUnchangedSuffix?: string;
}

const DEFAULT_REVIEW_SERVER = (label: string) => `Server · ${label}`;
const DEFAULT_REVIEW_LOCAL = (label: string) => `Your edit · ${label}`;

function ReviewRowChrome({
  label,
  server,
  draft,
  changed,
  reviewServerLabel,
  reviewLocalLabel,
  reviewUnchangedSuffix,
}: ConflictReviewRow & {
  reviewServerLabel: (fieldLabel: string) => string;
  reviewLocalLabel: (fieldLabel: string) => string;
  reviewUnchangedSuffix: string;
}) {
  return (
    <div className="grid gap-2 sm:grid-cols-2">
      <div className="rounded-control bg-gray-alpha-100 p-2">
        <span className="text-label-12 text-gray-700">
          {reviewServerLabel(label)}
        </span>
        <p className="mt-1 break-words text-copy-13 text-gray-900">{server}</p>
      </div>
      <div className="rounded-control bg-canvas-write-conflict/5 p-2">
        <span className="text-label-12 text-canvas-write-conflict">
          {reviewLocalLabel(label)}
          {changed ? null : ` ${reviewUnchangedSuffix}`}
        </span>
        <p className="mt-1 break-words text-copy-13 text-gray-900">{draft}</p>
      </div>
    </div>
  );
}

/**
 * Shared conflict-resolution modal shell. Callers supply field rows and all
 * user-visible strings; the shell computes overlap and disables reapply when
 * the server and draft touch the same field.
 */
export function ConflictModalChrome<T extends string = string>({
  open,
  title,
  description,
  descriptionSuffix,
  currentRevision,
  revisionLabel = 'Server is at revision',
  defaultDescription = 'Choose how to resolve the conflict.',
  serverSectionTitle = 'Server changes',
  localSectionTitle = 'Your local changes',
  serverNoChangesLabel = 'No detectable server field changes.',
  localNoChangesLabel = 'No local field changes.',
  serverChanges,
  localChanges,
  reviewRows,
  onUseCurrent,
  onReapply,
  onDismiss,
  useCurrentLabel = 'Use current',
  reapplyLabel = 'Reapply my edit',
  keepEditingLabel = 'Keep editing',
  reviewLabel = 'Review',
  reapplyTitleEnabled = 'Reapply your local edit on top of the current server state',
  reapplyTitleDisabled = 'Cannot reapply — overlapping fields',
  liveRevisionText,
  liveLocalChangesText,
  liveServerChangesText,
  liveOverlapText,
  liveNoOverlapText,
  liveNothingLabel = 'nothing',
  liveNothingDetectableLabel = 'nothing detectable',
  reviewServerLabel = DEFAULT_REVIEW_SERVER,
  reviewLocalLabel = DEFAULT_REVIEW_LOCAL,
  reviewUnchangedSuffix = '(unchanged)',
}: ConflictModalChromeProps<T>) {
  const titleId = useId();
  const liveId = useId();
  const panelRef = useRef<HTMLDivElement>(null);
  const previousFocus = useRef<HTMLElement | null>(null);
  const [showReview, setShowReview] = useState(false);

  useEffect(() => {
    if (!open) {
      setShowReview(false);
      return;
    }
    previousFocus.current = document.activeElement as HTMLElement | null;
    const panel = panelRef.current;
    const firstFocusable = panel?.querySelector<HTMLElement>(
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
    );
    firstFocusable?.focus();

    function onKeyDown(event: KeyboardEvent) {
      if (event.key !== 'Tab' || !panel) return;
      const focusable = Array.from(
        panel.querySelectorAll<HTMLElement>(
          'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
        ),
      ).filter((el) => !(el as HTMLButtonElement).disabled);
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    }

    function onEscape(event: KeyboardEvent) {
      if (event.key === 'Escape') {
        event.stopPropagation();
        onDismiss();
      }
    }

    document.addEventListener('keydown', onKeyDown);
    document.addEventListener('keydown', onEscape);
    return () => {
      document.removeEventListener('keydown', onKeyDown);
      document.removeEventListener('keydown', onEscape);
      previousFocus.current?.focus();
    };
  }, [open, onDismiss]);

  if (!open) return null;

  const localIds = new Set(localChanges.map((f) => f.id));
  const overlap = serverChanges.filter((f) => localIds.has(f.id));
  const canReapply = overlap.length === 0;

  const resolvedLiveRevision =
    liveRevisionText ?? `Server revision ${currentRevision}.`;
  const resolvedLiveLocal =
    liveLocalChangesText ??
    `Local changes: ${localChanges.map((f) => f.label).join(', ') || liveNothingLabel}.`;
  const resolvedLiveServer =
    liveServerChangesText ??
    `Server changes: ${serverChanges.map((f) => f.label).join(', ') || liveNothingDetectableLabel}.`;
  const resolvedLiveOverlap =
    overlap.length > 0
      ? (liveOverlapText ??
        `Overlapping fields: ${overlap.map((f) => f.label).join(', ')}.`)
      : (liveNoOverlapText ?? 'No overlapping fields.');

  const disabledTitle = reapplyTitleDisabled;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-gray-1000/40 p-4"
      role="dialog"
      aria-modal="true"
      aria-labelledby={titleId}
      data-testid="conflict-modal-chrome"
    >
      <div
        ref={panelRef}
        className="w-full max-w-lg rounded-popover border border-canvas-write-conflict bg-background-100 p-6 shadow-modal"
      >
        <div className="flex items-start gap-3">
          <AlertTriangle
            className="mt-0.5 h-5 w-5 shrink-0 text-canvas-write-conflict"
            aria-hidden
          />
          <div>
            <h3
              id={titleId}
              className="text-heading-20 font-heading text-canvas-write-conflict"
            >
              {title}
            </h3>
            {description ? (
              <p className="mt-1 text-copy-14 text-gray-900">
                {description}{' '}
                <span className="font-mono">{currentRevision}</span>
                {descriptionSuffix ? <>{descriptionSuffix}</> : '.'}
              </p>
            ) : (
              <p className="mt-1 text-copy-14 text-gray-900">
                {revisionLabel}{' '}
                <span className="font-mono">{currentRevision}</span>.{' '}
                {defaultDescription}
              </p>
            )}
          </div>
        </div>

        <div
          id={liveId}
          className="sr-only"
          role="status"
          aria-live="polite"
          aria-atomic="true"
        >
          {resolvedLiveRevision} {resolvedLiveLocal} {resolvedLiveServer}{' '}
          {resolvedLiveOverlap}
        </div>

        <div className="mt-4 grid gap-3 rounded-card border border-gray-alpha-300 bg-background-100 p-3">
          <div>
            <h4 className="text-label-14 font-semibold text-gray-900">
              {serverSectionTitle}
            </h4>
            {serverChanges.length === 0 ? (
              <p className="text-copy-13 text-gray-700">{serverNoChangesLabel}</p>
            ) : (
              <ul className="mt-1 flex flex-wrap gap-1">
                {serverChanges.map((f) => (
                  <li
                    key={f.id}
                    className="rounded-pill border border-gray-alpha-300 px-2 py-0.5 text-label-12 text-gray-900"
                  >
                    {f.label}
                  </li>
                ))}
              </ul>
            )}
          </div>
          <div>
            <h4 className="text-label-14 font-semibold text-gray-900">
              {localSectionTitle}
            </h4>
            {localChanges.length === 0 ? (
              <p className="text-copy-13 text-gray-700">{localNoChangesLabel}</p>
            ) : (
              <ul className="mt-1 flex flex-wrap gap-1">
                {localChanges.map((f) => (
                  <li
                    key={f.id}
                    className="rounded-pill border border-canvas-write-conflict/30 bg-canvas-write-conflict/10 px-2 py-0.5 text-label-12 text-canvas-write-conflict"
                  >
                    {f.label}
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>

        {showReview ? (
          <div className="mt-4 grid gap-3 rounded-card border border-gray-alpha-300 bg-background-100 p-3">
            {reviewRows.map((row) => (
              <ReviewRowChrome
                key={row.label}
                {...row}
                reviewServerLabel={reviewServerLabel}
                reviewLocalLabel={reviewLocalLabel}
                reviewUnchangedSuffix={reviewUnchangedSuffix}
              />
            ))}
          </div>
        ) : null}

        <div className="mt-5 flex flex-wrap items-center justify-end gap-2">
          <button
            type="button"
            accessKey="c"
            onClick={onDismiss}
            className="rounded-control border border-gray-alpha-400 px-4 py-2 text-button-12 text-gray-900 hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
          >
            {keepEditingLabel}
          </button>
          <button
            type="button"
            accessKey="r"
            onClick={() => setShowReview((v) => !v)}
            className="rounded-control border border-gray-alpha-400 px-4 py-2 text-button-12 text-gray-900 hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2"
          >
            <Split className="mr-1.5 inline h-4 w-4" aria-hidden />
            {reviewLabel}
          </button>
          <button
            type="button"
            accessKey="a"
            onClick={onReapply}
            disabled={!canReapply}
            title={canReapply ? reapplyTitleEnabled : disabledTitle}
            className="rounded-control border border-gray-alpha-400 px-4 py-2 text-button-12 text-gray-900 hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-disabled"
          >
            <RefreshCw className="mr-1.5 inline h-4 w-4" aria-hidden />
            {reapplyLabel}
          </button>
          <button
            type="button"
            accessKey="u"
            onClick={onUseCurrent}
            className="rounded-control bg-canvas-write-conflict px-4 py-2 text-button-12 text-white hover:bg-red-800 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2 dark:text-brand-deep-blue"
          >
            <Shield className="mr-1.5 inline h-4 w-4" aria-hidden />
            {useCurrentLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
