/**
 * HighlightLayer tests — V1.89 Deeper Manuscript Reading.
 *
 * Covers drift-notice rendering when an annotation's offsets exceed the current
 * body length and the presence of highlight marks after applying in-bounds
 * annotations.
 */
import { useRef, type RefObject } from 'react';
import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';

import { HighlightLayer } from './highlight-layer';
import type { ReadingAnnotation } from '@42ch/nexus-contracts';

function annotation(overrides: Partial<ReadingAnnotation> = {}): ReadingAnnotation {
  return {
    annotation_id: 'a-1',
    work_id: 'w-1',
    chapter: 1,
    start_offset: 0,
    end_offset: 5,
    selected_text: 'Hello',
    color: 'yellow',
    created_at: '2026-07-04T00:00:00Z',
    updated_at: '2026-07-04T00:00:00Z',
    ...overrides,
  };
}

interface ProseProps {
  proseRef: RefObject<HTMLDivElement>;
}

function Prose({ proseRef }: ProseProps) {
  return (
    <div ref={proseRef} data-testid="prose">
      Hello world
    </div>
  );
}

function renderLayer(annotations: ReadingAnnotation[], bodyLength: number) {
  const proseRef = useRef<HTMLDivElement>(null);
  render(
    <HighlightLayer annotations={annotations} bodyLength={bodyLength} proseRef={proseRef}>
      <Prose proseRef={proseRef} />
    </HighlightLayer>,
  );
  return { proseRef };
}

describe('HighlightLayer', () => {
  it('renders a non-blocking drift notice when an annotation is out of bounds', () => {
    renderLayer([annotation({ end_offset: 100 })], 11);
    expect(screen.getByRole('note')).toHaveTextContent(/may have shifted after body edits/i);
    expect(screen.getByTestId('prose')).toBeInTheDocument();
  });

  it('does not render a drift notice when all annotations fit the body', () => {
    renderLayer([annotation({ start_offset: 0, end_offset: 5 })], 11);
    expect(screen.queryByRole('note')).not.toBeInTheDocument();
  });

  it('wraps in-bounds text in a mark element', () => {
    renderLayer([annotation({ start_offset: 0, end_offset: 5 })], 11);
    const mark = document.querySelector('mark[data-nexus-highlight]');
    expect(mark).toBeInTheDocument();
    expect(mark).toHaveTextContent('Hello');
    expect(mark).toHaveAttribute('data-nexus-highlight', 'yellow');
  });

  it('skips marks for out-of-bounds annotations', () => {
    renderLayer([annotation({ start_offset: 20, end_offset: 25 })], 11);
    expect(document.querySelector('mark[data-nexus-highlight]')).not.toBeInTheDocument();
  });
});
