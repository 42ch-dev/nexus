import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { SectionIndex } from '@/components/section-index';
import type { GalleryEntry } from '@/lib/gallery-index';

const ENTRIES: readonly GalleryEntry[] = [
  {
    path: '/tokens',
    id: 'tokens-colors',
    label: 'Colors',
    keywords: ['palette'],
    importPaths: ['@nexus/design-tokens'],
  },
  {
    path: '/tokens',
    id: 'tokens-motion',
    label: 'Motion',
    keywords: ['duration'],
    importPaths: ['@nexus/design-tokens'],
  },
];
function getPoliteStatus() {
  return screen.getByText((_, element) => element?.getAttribute('aria-live') === 'polite');
}


describe('SectionIndex', () => {
  it('filters entries by trimmed case-insensitive substring', () => {
    render(<SectionIndex entries={ENTRIES} onNavigate={vi.fn()} />);

    fireEvent.change(screen.getByLabelText('Filter sections'), { target: { value: '  MOTION ' } });

    expect(screen.getByRole('link', { name: 'Motion' })).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Colors' })).not.toBeInTheDocument();
    expect(getPoliteStatus()).toHaveTextContent(/\b1\b/);
    expect(getPoliteStatus().textContent?.toLowerCase()).toContain('matching');
  });

  it('supports keyboard recovery and Enter selection callbacks', () => {
    const onNavigate = vi.fn();
    render(<SectionIndex entries={ENTRIES} onNavigate={onNavigate} />);

    const input = screen.getByLabelText('Filter sections');
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    fireEvent.keyDown(screen.getByRole('link', { name: 'Colors' }), { key: 'Enter' });
    expect(onNavigate).toHaveBeenCalledWith(ENTRIES[0]);

    fireEvent.change(input, { target: { value: 'missing' } });
    expect(screen.queryByRole('link')).not.toBeInTheDocument();
    expect(getPoliteStatus().textContent?.toLowerCase()).toMatch(/no matching/);
    const listId = input.getAttribute('aria-controls');
    expect(listId).toBeTruthy();
    expect(document.getElementById(listId!)).toBeInTheDocument();

    fireEvent.keyDown(input, { key: 'Escape' });
    expect(input).toHaveValue('');
    expect(document.activeElement).toBe(input);
  });

  it('clears the query from the visible Clear control', () => {
    render(<SectionIndex entries={ENTRIES} onNavigate={vi.fn()} />);

    const input = screen.getByLabelText('Filter sections');
    fireEvent.change(input, { target: { value: 'motion' } });
    fireEvent.click(screen.getByRole('button', { name: 'Clear' }));

    expect(input).toHaveValue('');
    expect(getPoliteStatus()).toHaveTextContent(/\b2\b/);
    expect(screen.getAllByRole('link')).toHaveLength(2);
  });
});
