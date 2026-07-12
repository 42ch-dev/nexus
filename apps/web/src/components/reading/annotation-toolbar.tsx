/**
 * AnnotationToolbar — V1.89 Deeper Manuscript Reading.
 *
 * Floating toolbar shown when the author selects text in the reading surface.
 * Offers a "Highlight" action that creates a persisted annotation at the
 * captured character offsets.
 */
import { Highlighter } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import type { TextSelection } from './use-text-selection';

export interface AnnotationToolbarProps {
  position: { x: number; y: number } | null;
  selection: TextSelection | null;
  onHighlight: () => void;
  isLoading?: boolean;
  className?: string;
}

export function AnnotationToolbar({
  position,
  selection,
  onHighlight,
  isLoading,
  className,
}: AnnotationToolbarProps) {
  const { t } = useTranslation('reading');
  if (!position || !selection) return null;

  return (
    <div
      className={cn(
        'pointer-events-none fixed left-0 top-0 z-40 flex',
        'bg-[var(--color-reading-selection-toolbar-background)] border border-[var(--color-reading-selection-toolbar-border)] text-[var(--color-reading-selection-toolbar-text)]',
        'rounded-popover shadow-[var(--color-reading-selection-toolbar-shadow)] px-2 py-1.5',
        className,
      )}
      style={{
        transform: `translate(${position.x}px, ${position.y}px) translateX(-50%) translateY(calc(-100% - 8px))`,
      }}
      role="toolbar"
      aria-label={t('annotation.toolbarAriaLabel')}
    >
      <Button
        type="button"
        variant="secondary"
        size="small"
        onMouseDown={(event) => {
          event.preventDefault();
        }}
        onClick={onHighlight}
        disabled={isLoading}
        className="pointer-events-auto"
        aria-label={t('annotation.highlightSelection')}
      >
        <Highlighter className="h-4 w-4" aria-hidden />
        {t('annotation.highlight')}
      </Button>
    </div>
  );
}
