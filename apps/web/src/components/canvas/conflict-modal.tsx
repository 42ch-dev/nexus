/**
 * Conflict resolution modal for the Strategy canvas write boundary.
 *
 * Implements the acceptance UX from canvas-strategy-surface.md §3.5:
 * - Headline: "This state changed while you were editing."
 * - Field-level summaries of server-side changes and local draft changes.
 * - Three actions: Use current, Reapply my edit, Review side-by-side.
 * - Accessibility: focus trap, live region, return focus, reduced-motion.
 */
import { useEffect, useId, useRef, useState } from 'react';
import { AlertTriangle, RefreshCw, Shield, Split } from 'lucide-react';

import type { PresetState } from '@/lib/canvas/preset-yaml';

export type ChangedField = 'label' | 'description' | 'nextTarget' | 'promptBody';

export interface ConflictModalDraft {
  label: string;
  description: string;
  nextTarget: string;
  promptBody: string;
}

export interface ConflictModalProps {
  open: boolean;
  currentRevision: number;
  draft: ConflictModalDraft;
  canonicalState?: PresetState;
  promptTemplateRef?: string;
  changedFields: readonly ChangedField[];
  onUseCurrent: () => void;
  onReapply: () => void;
  onDismiss: () => void;
}

export function ConflictModal({
  open,
  currentRevision,
  draft,
  canonicalState,
  promptTemplateRef,
  changedFields,
  onUseCurrent,
  onReapply,
  onDismiss,
}: ConflictModalProps) {
  const titleId = useId();
  const liveId = useId();
  const panelRef = useRef<HTMLDivElement>(null);
  const previousFocus = useRef<HTMLElement | null>(null);
  const [showReview, setShowReview] = useState(false);

  // Capture focus and install trap when opening.
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

  const canonicalLabel = canonicalState?.id ?? '';
  const canonicalDescription = canonicalState?.description ?? '';
  const canonicalNext =
    typeof canonicalState?.next === 'string' ? canonicalState.next : '';

  const serverChanges: ChangedField[] = [];
  if (canonicalLabel !== draft.label) serverChanges.push('label');
  if (canonicalDescription !== draft.description) serverChanges.push('description');
  if (canonicalNext !== draft.nextTarget) serverChanges.push('nextTarget');
  // Prompt body changes are not reflected in the manifest; we cannot detect
  // server-side prompt edits without a separate template-read contract.
  // Only treat the prompt as conflicted when the user actually changed it AND
  // the changedFields list confirms the conflict was on the prompt path.
  if (draft.promptBody && changedFields.includes('promptBody')) serverChanges.push('promptBody');

  const overlap = changedFields.filter((f) => serverChanges.includes(f));
  const canReapply = overlap.length === 0;

  const fieldLabel: Record<ChangedField, string> = {
    label: 'State label',
    description: 'Description',
    nextTarget: 'Next target',
    promptBody: 'Prompt template',
  };

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
              This state changed while you were editing.
            </h3>
            <p className="mt-1 text-copy-14 text-gray-900">
              Server revision is now{' '}
              <span className="font-mono">{currentRevision}</span>. Choose how to
              reconcile your changes.
            </p>
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
          {changedFields.map((f) => fieldLabel[f]).join(', ')}. The server changed{' '}
          {serverChanges.map((f) => fieldLabel[f]).join(', ') ?? 'nothing detectable'}.
          {overlap.length > 0
            ? ` Overlap on ${overlap.map((f) => fieldLabel[f]).join(', ')}. Review before reapplying.`
            : ' No overlap. You can safely reapply your edit.'}
        </div>

        <div className="mt-4 grid gap-3 rounded-card border border-gray-alpha-300 bg-background-100 p-3">
          <div>
            <h4 className="text-label-14 font-semibold text-gray-900">
              What changed on the server
            </h4>
            {serverChanges.length === 0 ? (
              <p className="text-copy-13 text-gray-700">
                No detectable changes to the fields you edited.
              </p>
            ) : (
              <ul className="mt-1 flex flex-wrap gap-1">
                {serverChanges.map((f) => (
                  <li
                    key={f}
                    className="rounded-pill border border-gray-alpha-300 px-2 py-0.5 text-label-12 text-gray-900"
                  >
                    {fieldLabel[f]}
                  </li>
                ))}
              </ul>
            )}
          </div>
          <div>
            <h4 className="text-label-14 font-semibold text-gray-900">
              What you were about to do
            </h4>
            <ul className="mt-1 flex flex-wrap gap-1">
              {changedFields.map((f) => (
                <li
                  key={f}
                  className="rounded-pill border border-canvas-write-conflict/30 bg-canvas-write-conflict/10 px-2 py-0.5 text-label-12 text-canvas-write-conflict"
                >
                  {fieldLabel[f]}
                </li>
              ))}
            </ul>
          </div>
        </div>

        {showReview ? (
          <div className="mt-4 grid gap-3 rounded-card border border-gray-alpha-300 bg-background-100 p-3">
            <ReviewRow
              label="State label"
              server={canonicalLabel}
              draft={draft.label}
              changed={changedFields.includes('label')}
            />
            <ReviewRow
              label="Description"
              server={canonicalDescription}
              draft={draft.description}
              changed={changedFields.includes('description')}
            />
            <ReviewRow
              label="Next target"
              server={canonicalNext}
              draft={draft.nextTarget}
              changed={changedFields.includes('nextTarget')}
            />
            {promptTemplateRef ? (
              <ReviewRow
                label={`Prompt template (${promptTemplateRef})`}
                server="(server content not fetched)"
                draft={draft.promptBody || '(empty)'}
                changed={changedFields.includes('promptBody')}
              />
            ) : null}
          </div>
        ) : null}

        <div className="mt-5 flex flex-wrap items-center justify-end gap-2">
          <button
            type="button"
            onClick={onDismiss}
            className="rounded-control border border-gray-alpha-400 px-4 py-2 text-button-12 text-gray-900 hover:bg-gray-alpha-100"
          >
            Keep editing
          </button>
          <button
            type="button"
            onClick={() => setShowReview((v) => !v)}
            className="rounded-control border border-gray-alpha-400 px-4 py-2 text-button-12 text-gray-900 hover:bg-gray-alpha-100"
          >
            <Split className="mr-1.5 inline h-4 w-4" aria-hidden />
            Review side-by-side
          </button>
          <button
            type="button"
            onClick={onReapply}
            disabled={!canReapply}
            title={
              canReapply
                ? 'Refetch and reapply your edit'
                : `Cannot reapply automatically because ${overlap.map((f) => fieldLabel[f]).join(', ')} also changed on the server.`
            }
            className="rounded-control border border-gray-alpha-400 px-4 py-2 text-button-12 text-gray-900 hover:bg-gray-alpha-100 disabled:cursor-not-allowed disabled:opacity-50"
          >
            <RefreshCw className="mr-1.5 inline h-4 w-4" aria-hidden />
            Reapply my edit
          </button>
          <button
            type="button"
            onClick={onUseCurrent}
            className="rounded-control bg-canvas-write-conflict px-4 py-2 text-button-12 text-white hover:bg-red-800"
          >
            <Shield className="mr-1.5 inline h-4 w-4" aria-hidden />
            Use current
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
        <p className="mt-1 text-copy-13 text-gray-900 break-words">{server}</p>
      </div>
      <div className="rounded-control bg-canvas-write-conflict/5 p-2">
        <span className="text-label-12 text-canvas-write-conflict">
          Your edit: {label}
          {changed ? null : ' (unchanged)'}
        </span>
        <p className="mt-1 text-copy-13 text-gray-900 break-words">{draft}</p>
      </div>
    </div>
  );
}
