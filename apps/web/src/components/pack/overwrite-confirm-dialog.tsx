/**
 * Overwrite confirmation dialog — gates the data-loss import path
 * (V1.152 P1 T3, DF-77).
 *
 * Shown by {@link PackPanel} before an `overwrite` import actually mutates:
 * the author must explicitly confirm that existing entries will be replaced.
 *
 * A11y (WCAG 2.1 AA, brief T4) — follows the V1.124
 * `ConflictModalChrome` focus-trap precedent:
 * - `role="dialog"` + `aria-modal="true"`, labelled by the dialog title;
 * - focus moves into the dialog when it opens and returns to the trigger
 *   when it closes (cleanup restores `document.activeElement`);
 * - Tab wraps between the first and last focusable control (shift-tab
 *   included) while open;
 * - Escape cancels (equivalent to Cancel — the destructive path is never
 *   dismissed by accident via backdrop click; the modal has no backdrop
 *   handler on purpose).
 */
import { useEffect, useId, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { AlertTriangle } from 'lucide-react';

import { Button } from '@/components/ui/button';

export interface OverwriteConfirmDialogProps {
  open: boolean;
  /**
   * To-be-replaced entry count, when a source exists (e.g. a future
   * pre-flight). Omitted when unknown — the import response details only
   * arrive after the import, so the panel currently cannot pre-compute this.
   */
  overwriteCount?: number;
  onConfirm: () => void;
  onCancel: () => void;
}

export function OverwriteConfirmDialog({
  open,
  overwriteCount,
  onConfirm,
  onCancel,
}: OverwriteConfirmDialogProps) {
  const { t } = useTranslation('pack');
  const titleId = useId();
  const panelRef = useRef<HTMLDivElement>(null);
  const previousFocus = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!open) return;
    previousFocus.current = document.activeElement as HTMLElement | null;
    const panel = panelRef.current;
    const firstFocusable = panel?.querySelector<HTMLElement>(
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
    );
    firstFocusable?.focus();

    function onKeyDown(event: KeyboardEvent) {
      if (event.key === 'Escape') {
        event.stopPropagation();
        onCancel();
        return;
      }
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

    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('keydown', onKeyDown);
      previousFocus.current?.focus();
    };
  }, [open, onCancel]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-gray-1000/40 p-4"
      role="dialog"
      aria-modal="true"
      aria-labelledby={titleId}
      data-testid="overwrite-confirm-dialog"
    >
      <div
        ref={panelRef}
        className="w-full max-w-md rounded-popover border border-red-700/30 bg-background-100 p-6 shadow-modal"
      >
        <div className="flex items-start gap-3">
          <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-red-1000" aria-hidden />
          <div>
            <h3 id={titleId} className="text-heading-20 font-heading text-gray-1000">
              {t('confirm.title')}
            </h3>
            <p className="mt-1 text-copy-14 text-gray-900">{t('confirm.warning')}</p>
            {overwriteCount !== undefined ? (
              <p className="mt-1 text-copy-13 text-gray-700">
                {t('confirm.count', { count: overwriteCount })}
              </p>
            ) : null}
          </div>
        </div>
        <div className="mt-5 flex flex-wrap items-center justify-end gap-2">
          <Button
            type="button"
            variant="tertiary"
            size="small"
            onClick={onCancel}
            data-testid="overwrite-confirm-cancel"
          >
            {t('confirm.cancel')}
          </Button>
          <Button
            type="button"
            variant="destructive"
            size="small"
            onClick={onConfirm}
            data-testid="overwrite-confirm-ok"
          >
            {t('confirm.confirm')}
          </Button>
        </div>
      </div>
    </div>
  );
}
