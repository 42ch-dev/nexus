/**
 * HighlightLayer — V1.89 Deeper Manuscript Reading.
 *
 * Renders persisted highlights over `ReadingProse` by wrapping the DOM ranges
 * that correspond to each annotation's character offsets in a `<mark>` element.
 * Highlights are reapplied whenever the body or annotations change.
 *
 * Drift handling: if an annotation's `end_offset` is greater than the current
 * body length, a non-blocking notice is shown at the top of the prose. The
 * highlight itself is skipped so it cannot mis-render over unrelated text.
 */
import { useLayoutEffect, useRef } from 'react';

import { cn } from '@/lib/utils';
import type { ReadingAnnotation, ReadingAnnotationColor } from '@42ch/nexus-contracts';
import { rangeFromOffsets } from './use-text-selection';

export interface HighlightLayerProps {
  annotations: ReadingAnnotation[];
  /** Plain-text body length used for drift detection. */
  bodyLength: number;
  children: React.ReactNode;
  className?: string;
}

const HIGHLIGHT_CLASS: Record<ReadingAnnotationColor, string> = {
  yellow: 'bg-[var(--color-reading-annotation-highlight-yellow-background)] text-[var(--color-reading-annotation-highlight-yellow-text)]',
  blue: 'bg-[var(--color-reading-annotation-highlight-blue-background)] text-[var(--color-reading-annotation-highlight-blue-text)]',
  green: 'bg-[var(--color-reading-annotation-highlight-green-background)] text-[var(--color-reading-annotation-highlight-green-text)]',
  pink: 'bg-[var(--color-reading-annotation-highlight-pink-background)] text-[var(--color-reading-annotation-highlight-pink-text)]',
};

const HIGHLIGHT_SELECTOR = 'mark[data-nexus-highlight]';

function clearHighlights(container: HTMLElement) {
  const marks = container.querySelectorAll(HIGHLIGHT_SELECTOR);
  for (const mark of marks) {
    const parent = mark.parentNode;
    if (!parent) continue;
    while (mark.firstChild) {
      parent.insertBefore(mark.firstChild, mark);
    }
    parent.removeChild(mark);
  }
}

function wrapRange(range: Range, color: ReadingAnnotationColor) {
  const contents = range.extractContents();
  const mark = document.createElement('mark');
  mark.className = cn('rounded-sm', HIGHLIGHT_CLASS[color]);
  mark.setAttribute('data-nexus-highlight', color);
  mark.appendChild(contents);
  range.insertNode(mark);
  return mark;
}

function applyHighlights(container: HTMLElement, annotations: ReadingAnnotation[], bodyLength: number) {
  clearHighlights(container);

  // Only render in-bounds highlights. Sort by start offset so nested/partial
  // overlaps are applied in document order.
  const inBounds = annotations
    .filter((a) => a.end_offset <= bodyLength)
    .sort((a, b) => a.start_offset - b.start_offset || a.end_offset - b.end_offset);

  for (const annotation of inBounds) {
    const range = rangeFromOffsets(container, annotation.start_offset, annotation.end_offset);
    if (!range) continue;
    wrapRange(range, annotation.color);
  }
}

export function HighlightLayer({ annotations, bodyLength, children, className }: HighlightLayerProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const hasDrift = annotations.some((a) => a.end_offset > bodyLength);

  useLayoutEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    applyHighlights(container, annotations, bodyLength);
  }, [annotations, bodyLength]);

  return (
    <div ref={containerRef} className={cn('relative', className)}>
      {hasDrift && (
        <div
          className="mb-3 rounded-card border border-amber-700/30 bg-amber-700/10 px-4 py-3 text-copy-14 text-amber-1000"
          role="note"
          aria-live="polite"
        >
          标注可能因正文编辑而偏移 / This highlight may have shifted after body edits
        </div>
      )}
      {children}
    </div>
  );
}
