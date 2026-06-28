/**
 * Generic conflict-resolution modal shell.
 *
 * Extracted from the V1.71 Strategy canvas `ConflictModal` so the same visual
 * pattern and accessibility behavior can be reused for the Outline+Timeline
 * canvas. Callers supply the conflict fields and review rows; the shell
 * computes overlap and disables reapply when the server and draft touch the
 * same field.
 */
import { useEffect, useId, useRef, useState } from 'react';
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

export interface ConflictModalBaseProps<T extends string = string> {
  open: boolean;
  title: string;
  description?: string;
  currentRevision: number;
  revisionLabel?: string;
  serverSectionTitle?: string;
  localSectionTitle?: string;
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
}

export function ConflictModalBase<T extends string = string>({
  open,
  title,
  description,
  currentRevision,
  revisionLabel = 'Server revision is now',
  serverSectionTitle = 'What changed on the server',
  localSectionTitle = 'What you were about to do',
  serverChanges,
  localChanges,
  reviewRows,
  onUseCurrent,
  onReapply,
  onDismiss,
  useCurrentLabel = 'Use current',
  reapplyLabel = 'Reapply my edit',
  keepEditingLabel = 'Keep editing',
  reviewLabel = 'Review side-by-side',
}: ConflictModalBaseProps<T>) {
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

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-gray-1000/40 p-4"
      role="dialog"
      aria-modal="true"
      aria-labelledby={titleId}
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
                <span className="font-mono">{currentRevision}</span>.
              </p>
            ) : (
              <p className="mt-1 text-copy-14 text-gray-900">
                {revisionLabel}{' '}
                <span className="font-mono">{currentRevision}</span>. Choose how
                to reconcile your changes.
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
          Conflict detected on revision {currentRevision}. You changed{' '}
          {localChanges.map((f) => f.label).join(', ') || 'nothing'}. The server
          changed{' '}
          {serverChanges.map((f) => f.label).join(', ') || 'nothing detectable'}.
          {overlap.length > 0
            ? ` Overlap on ${overlap.map((f) => f.label).join(', ')}. Review before reapplying.`
            : ' No overlap. You can safely reapply your edit.'}
        </div>

        <div className="mt-4 grid gap-3 rounded-card border border-gray-alpha-300 bg-background-100 p-3">
          <div>
            <h4 className="text-label-14 font-semibold text-gray-900">
              {serverSectionTitle}
            </h4>
            {serverChanges.length === 0 ? (
              <p className="text-copy-13 text-gray-700">
                No detectable changes to the fields you edited.
              </p>
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
              <p className="text-copy-13 text-gray-700">No local changes.</p>
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
              <ReviewRow key={row.label} {...row} />
            ))}
          </div>
        ) : null}

        <div className="mt-5 flex flex-wrap items-center justify-end gap-2">
          <button
            type="button"
            onClick={onDismiss}
            className="rounded-control border border-gray-alpha-400 px-4 py-2 text-button-12 text-gray-900 hover:bg-gray-alpha-100"
          >
            {keepEditingLabel}
          </button>
          <button
            type="button"
            onClick={() => setShowReview((v) => !v)}
            className="rounded-control border border-gray-alpha-400 px-4 py-2 text-button-12 text-gray-900 hover:bg-gray-alpha-100"
          >
            <Split className="mr-1.5 inline h-4 w-4" aria-hidden />
            {reviewLabel}
          </button>
          <button
            type="button"
            onClick={onReapply}
            disabled={!canReapply}
            title={
              canReapply
                ? 'Refetch and reapply your edit'
                : `Cannot reapply automatically because ${overlap.map((f) => f.label).join(', ')} also changed on the server.`
            }
            className="rounded-control border border-gray-alpha-400 px-4 py-2 text-button-12 text-gray-900 hover:bg-gray-alpha-100 disabled:cursor-not-allowed disabled:opacity-50"
          >
            <RefreshCw className="mr-1.5 inline h-4 w-4" aria-hidden />
            {reapplyLabel}
          </button>
          <button
            type="button"
            onClick={onUseCurrent}
            className="rounded-control bg-canvas-write-conflict px-4 py-2 text-button-12 text-white hover:bg-red-800"
          >
            <Shield className="mr-1.5 inline h-4 w-4" aria-hidden />
            {useCurrentLabel}
          </button>
        </div>
      </div>
    </div>
  );
}

function ReviewRow({
  label,
  server,
  draft,
  changed,
}: {
  label: string;
  server: string;
  draft: string;
  changed: boolean;
}) {
  return (
    <div className="grid gap-2 sm:grid-cols-2">
      <div className="rounded-control bg-gray-alpha-100 p-2">
        <span className="text-label-12 text-gray-700">Server: {label}</span>
        <p className="mt-1 break-words text-copy-13 text-gray-900">{server}</p>
      </div>
      <div className="rounded-control bg-canvas-write-conflict/5 p-2">
        <span className="text-label-12 text-canvas-write-conflict">
          Your edit: {label}
          {changed ? null : ' (unchanged)'}
        </span>
        <p className="mt-1 break-words text-copy-13 text-gray-900">{draft}</p>
      </div>
    </div>
  );
}
