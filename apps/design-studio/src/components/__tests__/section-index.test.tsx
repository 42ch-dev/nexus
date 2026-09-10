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

describe('SectionIndex', () => {
  it('filters entries by trimmed case-insensitive substring', () => {
    render(<SectionIndex entries={ENTRIES} onNavigate={vi.fn()} />);

    fireEvent.change(screen.getByLabelText('Filter sections'), { target: { value: '  MOTION ' } });

    expect(screen.getByRole('link', { name: 'Motion' })).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Colors' })).not.toBeInTheDocument();
    expect(screen.getByText('1 matching section')).toBeInTheDocument();
  });

  it('supports keyboard recovery and Enter selection callbacks', () => {
    const onNavigate = vi.fn();
    render(<SectionIndex entries={ENTRIES} onNavigate={onNavigate} />);

    const input = screen.getByLabelText('Filter sections');
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    fireEvent.keyDown(screen.getByRole('link', { name: 'Colors' }), { key: 'Enter' });
    expect(onNavigate).toHaveBeenCalledWith(ENTRIES[0]);

    fireEvent.change(input, { target: { value: 'missing' } });
    expect(screen.getByText('No results for “missing”.')).toBeInTheDocument();

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
    expect(screen.getByText('2 sections')).toBeInTheDocument();
  });
});
