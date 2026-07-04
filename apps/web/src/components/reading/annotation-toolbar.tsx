/**
 * AnnotationToolbar — V1.89 Deeper Manuscript Reading.
 *
 * Floating toolbar shown when the author selects text in the reading surface.
 * Offers a "Highlight" action that creates a persisted annotation at the
 * captured character offsets.
 */
import { Highlighter } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import type { TextSelection } from './use-text-selection';

export interface AnnotationToolbarProps {
  /** Viewport-relative X/Y for positioning. */
  position: { x: number; y: number } | null;
  /** Captured selection; toolbar is hidden when null. */
  selection: TextSelection | null;
  /** Called when the author clicks Highlight. */
  onHighlight: () => void;
  /** Whether the create mutation is in flight. */
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
  if (!position || !selection) return null;

  return (
    // The toolbar is rendered in a fixed container translated to the selection
    // viewport position so it floats above the prose without altering layout.
    // `pointer-events-auto` keeps clicks on the button from collapsing the
    // selection before the action fires.
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
      aria-label="Annotation actions"
    >
      <Button
        type="button"
        variant="secondary"
        size="small"
        onMouseDown={(event) => {
          // Prevent the mousedown from clearing the selection before onClick.
          event.preventDefault();
        }}
        onClick={onHighlight}
        disabled={isLoading}
        className="pointer-events-auto"
        aria-label="Highlight selection"
      >
        <Highlighter className="h-4 w-4" aria-hidden />
        Highlight
      </Button>
    </div>
  );
}
