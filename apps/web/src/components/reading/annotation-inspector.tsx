/**
 * AnnotationInspector — V1.89 Deeper Manuscript Reading.
 *
 * Side panel listing the persisted annotations for the current chapter. Each
 * row shows a color swatch, the captured text snippet, an optional note, and
 * the creation time. Authors can edit the note/color or delete a highlight.
 */
import { useState } from 'react';
import { Pencil, Trash2, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { Select } from '@/components/ui/select';
import { cn } from '@/lib/utils';
import type { ReadingAnnotation } from '@42ch/nexus-contracts';

// V1.138 codegen inlines the color enum; derive it from the canonical type so
// it stays in sync with `@42ch/nexus-contracts` rather than being hardcoded.
type ReadingAnnotationColor = ReadingAnnotation['color'];

const ANNOTATION_COLORS: ReadingAnnotationColor[] = ['yellow', 'blue', 'green', 'pink'];

export interface AnnotationInspectorProps {
  annotations: ReadingAnnotation[];
  onUpdate: (annotationId: string, patch: { color?: ReadingAnnotationColor; note?: string }) => void;
  onDelete: (annotationId: string) => void;
  isUpdating?: boolean;
  isDeleting?: boolean;
  className?: string;
}

const SWATCH_CLASS: Record<ReadingAnnotationColor, string> = {
  yellow: 'bg-[var(--color-reading-annotation-highlight-yellow-background)] border-amber-700/30',
  blue: 'bg-[var(--color-reading-annotation-highlight-blue-background)] border-blue-1000/30 dark:border-blue-700/30',
  green: 'bg-[var(--color-reading-annotation-highlight-green-background)] border-green-700/30',
  pink: 'bg-[var(--color-reading-annotation-highlight-pink-background)] border-pink-700/30',
};

function formatDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, { dateStyle: 'short', timeStyle: 'short' });
}

export function AnnotationInspector({
  annotations,
  onUpdate,
  onDelete,
  isUpdating,
  isDeleting,
  className,
}: AnnotationInspectorProps) {
  const { t } = useTranslation('reading');
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draftNote, setDraftNote] = useState('');
  const [draftColor, setDraftColor] = useState<ReadingAnnotationColor>('yellow');

  function startEdit(annotation: ReadingAnnotation) {
    setEditingId(annotation.annotation_id);
    setDraftNote(annotation.note ?? '');
    setDraftColor(annotation.color);
  }

  function cancelEdit() {
    setEditingId(null);
    setDraftNote('');
  }

  function saveEdit(annotationId: string) {
    onUpdate(annotationId, { color: draftColor, note: draftNote });
    setEditingId(null);
  }

  return (
    <aside
      className={cn(
        'flex w-80 flex-col rounded-card border bg-[var(--color-reading-annotation-inspector-background)] border-[var(--color-reading-annotation-inspector-border)] text-[var(--color-reading-annotation-inspector-text)] shadow-card',
        className,
      )}
      aria-label={t('annotation.inspectorAriaLabel')}
    >
      <div className="border-b border-[var(--color-reading-annotation-inspector-border)] px-4 py-3">
        <h3 className="text-label-14 font-medium text-gray-1000">{t('annotation.title')}</h3>
        <p className="mt-0.5 text-copy-13 text-gray-700">
          {t('annotation.count', { count: annotations.length })}
        </p>
      </div>

      <div className="max-h-[60vh] overflow-y-auto p-2">
        {annotations.length === 0 && (
          <div className="px-3 py-6 text-center text-copy-14 text-gray-700">
            {t('annotation.empty')}
          </div>
        )}

        <ul className="flex flex-col gap-2">
          {annotations.map((annotation) => (
            <li
              key={annotation.annotation_id}
              className="rounded-control border border-gray-alpha-300 p-3"
            >
              {editingId === annotation.annotation_id ? (
                <div className="flex flex-col gap-2">
                  <Select
                    value={draftColor}
                    onChange={(event) => setDraftColor(event.target.value as ReadingAnnotationColor)}
                    aria-label={t('annotation.colorLabel')}
                  >
                    {ANNOTATION_COLORS.map((color) => (
                      <option key={color} value={color}>
                        {t(`annotation.colors.${color}`)}
                      </option>
                    ))}
                  </Select>
                  <Textarea
                    value={draftNote}
                    onChange={(event) => setDraftNote(event.target.value)}
                    placeholder={t('annotation.notePlaceholder')}
                    rows={3}
                    className="min-h-0 text-copy-14"
                  />
                  <div className="flex justify-end gap-2">
                    <Button
                      type="button"
                      variant="tertiary"
                      size="small"
                      onClick={cancelEdit}
                      aria-label={t('annotation.cancelEdit')}
                    >
                      <X className="h-4 w-4" aria-hidden />
                    </Button>
                    <Button
                      type="button"
                      variant="secondary"
                      size="small"
                      onClick={() => saveEdit(annotation.annotation_id)}
                      disabled={isUpdating}
                    >
                      {t('annotation.save')}
                    </Button>
                  </div>
                </div>
              ) : (
                <div className="flex flex-col gap-2">
                  <div className="flex items-start gap-2">
                    <span
                      className={cn(
                        'mt-0.5 h-4 w-4 shrink-0 rounded-sm border',
                        SWATCH_CLASS[annotation.color],
                      )}
                      aria-label={t('annotation.colorAria', { color: annotation.color })}
                    />
                    <blockquote className="line-clamp-3 flex-1 text-copy-14 text-gray-1000">
                      “{annotation.selected_text}”
                    </blockquote>
                  </div>
                  {annotation.note && (
                    <p className="text-copy-13 text-gray-700">{annotation.note}</p>
                  )}
                  <div className="flex items-center justify-between">
                    <time className="text-copy-12 text-gray-600" dateTime={annotation.created_at}>
                      {formatDate(annotation.created_at)}
                    </time>
                    <div className="flex items-center gap-1">
                      <Button
                        type="button"
                        variant="tertiary"
                        size="small"
                        onClick={() => startEdit(annotation)}
                        aria-label={t('annotation.edit')}
                      >
                        <Pencil className="h-4 w-4" aria-hidden />
                      </Button>
                      <Button
                        type="button"
                        variant="tertiary"
                        size="small"
                        onClick={() => onDelete(annotation.annotation_id)}
                        disabled={isDeleting}
                        aria-label={t('annotation.delete')}
                        className="text-red-700 hover:bg-red-700/10"
                      >
                        <Trash2 className="h-4 w-4" aria-hidden />
                      </Button>
                    </div>
                  </div>
                </div>
              )}
            </li>
          ))}
        </ul>
      </div>
    </aside>
  );
}
