/**
 * AnnotationInspector tests — V1.89 Deeper Manuscript Reading.
 *
 * Covers listing annotations, editing note/color, and deleting an annotation.
 */
import { describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { AnnotationInspector } from './annotation-inspector';
import type { ReadingAnnotation } from './reading-api';

function annotation(overrides: Partial<ReadingAnnotation> = {}): ReadingAnnotation {
  return {
    annotation_id: 'a-1',
    work_id: 'w-1',
    chapter: 1,
    start_offset: 0,
    end_offset: 5,
    selected_text: 'Hello',
    note: 'first note',
    color: 'yellow',
    created_at: '2026-07-04T00:00:00Z',
    updated_at: '2026-07-04T00:00:00Z',
    ...overrides,
  };
}

describe('AnnotationInspector', () => {
  it('renders all annotations', () => {
    const annotations = [
      annotation({ annotation_id: 'a-1', note: 'alpha note' }),
      annotation({ annotation_id: 'a-2', start_offset: 6, end_offset: 11, note: 'beta note' }),
    ];
    render(
      <AnnotationInspector
        annotations={annotations}
        onDelete={vi.fn()}
        onUpdate={vi.fn()}
      />,
    );
    expect(screen.getAllByRole('listitem')).toHaveLength(2);
    expect(screen.getByText('alpha note')).toBeInTheDocument();
    expect(screen.getByText('beta note')).toBeInTheDocument();
  });

  it('calls onUpdate with id and patch when note or color changes', async () => {
    const user = userEvent.setup();
    const onUpdate = vi.fn();
    render(
      <AnnotationInspector
        annotations={[annotation({ annotation_id: 'a-1', note: 'old note' })]}
        onDelete={vi.fn()}
        onUpdate={onUpdate}
      />,
    );

    await user.click(screen.getByRole('button', { name: /edit highlight/i }));
    const noteInput = screen.getByPlaceholderText(/add a note/i);
    await user.clear(noteInput);
    await user.type(noteInput, 'new note');
    await user.click(screen.getByRole('button', { name: /save/i }));

    await waitFor(() => {
      expect(onUpdate).toHaveBeenCalledWith('a-1', { note: 'new note', color: 'yellow' });
    });
  });

  it('calls onDelete when delete is clicked', async () => {
    const user = userEvent.setup();
    const onDelete = vi.fn();
    render(
      <AnnotationInspector
        annotations={[annotation({ annotation_id: 'a-1' })]}
        onDelete={onDelete}
        onUpdate={vi.fn()}
      />,
    );

    await user.click(screen.getByRole('button', { name: /delete highlight/i }));

    await waitFor(() => {
      expect(onDelete).toHaveBeenCalledWith('a-1');
    });
  });
});
