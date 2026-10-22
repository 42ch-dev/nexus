/**
 * Brief inline editor (plan v1219-world-module T2) — a single textarea that
 * creates (or edits) the era KnowledgeEntry backing the Brief layer through
 * {@link useSaveBrief}. Product-owned behaviour:
 *
 *   - `保存 Brief` is disabled while the trimmed text is empty — no error
 *     dialog, no toast.
 *   - Save failure keeps the editor mounted with the draft intact and shows
 *     one actionable inline message (`Brief 保存失败：{reason}。草稿已保留，
 *     可直接重试。`); 422 `world_kb_validation_failed` renders
 *     `validation_summary.errors[]` as the reason.
 *   - Save success unmounts via `onSaved` — the graph invalidation in the
 *     mutation re-renders the saved card without a reload.
 */
import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Textarea } from '@/components/ui';
import { isWorldKbValidationError } from '@/lib/canvas/use-world-kb-data';
import { NexusClientError } from '@/lib/nexus';

import { useSaveBrief } from './use-brief-mutation';

/**
 * Extract `validation_summary.errors[]` from a World KB 422 error (shared
 * with the era-create dialog's copy of the same defensive narrowing).
 */
function extractValidationErrors(err: unknown): string[] {
  if (err === null || typeof err !== 'object') return [];
  if (!('details' in err)) return [];
  const details = err.details;
  if (details === null || typeof details !== 'object') return [];
  if (!('validation_summary' in details)) return [];
  const summary = details.validation_summary;
  if (summary === null || typeof summary !== 'object') return [];
  if (!('errors' in summary)) return [];
  const errors = summary.errors;
  if (!Array.isArray(errors)) return [];
  return errors.filter((e): e is string => typeof e === 'string');
}

export interface BriefEditorProps {
  worldId: string;
  /** Prefill when editing an existing Brief-era entity. */
  initialText?: string;
  entity?: { entityId: string; version: number; body?: Record<string, unknown> };
  onCancel: () => void;
  onSaved: () => void;
  /** Fired by the mutation when this save performs the first-transition Work unlock. */
  onFirstUnlock?: () => void;
}

export function BriefEditor({
  worldId,
  initialText,
  entity,
  onCancel,
  onSaved,
  onFirstUnlock,
}: BriefEditorProps) {
  const { t } = useTranslation('worlds');
  const save = useSaveBrief(worldId, { onFirstUnlock });
  const [text, setText] = useState(initialText ?? '');
  const [error, setError] = useState<string | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    textareaRef.current?.focus();
  }, []);

  const canSave = text.trim().length > 0;

  async function handleSave() {
    if (!canSave || save.isPending) return;
    setError(null);
    try {
      await save.mutateAsync({ text: text.trim(), entity });
      onSaved();
    } catch (err) {
      const reason = isWorldKbValidationError(err)
        ? extractValidationErrors(err).join('；')
        : err instanceof NexusClientError
          ? err.message
          : err instanceof Error
            ? err.message
            : String(err);
      // Draft is caller-state (`text`) — untouched by the failure; the editor
      // stays mounted so the copy's "可直接重试" is literally true.
      setError(t('worldApp.briefEditor.saveFailure', { reason }));
    }
  }

  return (
    <form
      data-testid="world-brief-editor"
      className="flex flex-col gap-3 rounded-card border border-gray-500 p-6"
      onSubmit={(e) => {
        e.preventDefault();
        void handleSave();
      }}
    >
      <Textarea
        ref={textareaRef}
        data-testid="world-brief-input"
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          if (error) setError(null);
        }}
        placeholder={t('worldApp.briefEditor.placeholder')}
        rows={3}
        aria-label={t('worldApp.briefEditor.ariaLabel')}
      />
      {error && (
        <p role="alert" data-testid="world-brief-error" className="text-copy-14 text-red-700">
          {error}
        </p>
      )}
      <div className="flex gap-2">
        <button
          type="button"
          data-testid="world-brief-cancel"
          className="rounded-control border border-gray-500 px-3 py-1.5 text-copy-14 text-gray-900 hover:bg-gray-alpha-100"
          onClick={onCancel}
        >
          {t('worldApp.briefEditor.cancel')}
        </button>
        <button
          type="submit"
          data-testid="world-brief-save"
          className="rounded-control bg-gray-1000 px-3 py-1.5 text-copy-14 text-gray-100 hover:bg-gray-900 disabled:opacity-50"
          disabled={!canSave || save.isPending}
        >
          {t('worldApp.briefEditor.save')}
        </button>
      </div>
    </form>
  );
}
