/**
 * @vitest-environment jsdom
 */
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { OutlineConflictModal } from './outline-conflict-modal';

const baseProps = {
  open: true,
  currentRevision: 3,
  draft: {
    fields: ['chapter_title', 'move_chapter'] as const,
    conflictingPath: 'volumes/1',
  },
  onUseCurrent: vi.fn(),
  onReapply: vi.fn(),
  onDismiss: vi.fn(),
};

describe('OutlineConflictModal', () => {
  it('renders the outline headline and server revision', () => {
    render(<OutlineConflictModal {...baseProps} />);
    expect(screen.getByText('This outline changed while you were editing.')).toBeInTheDocument();
    expect(screen.getByText('3', { selector: 'span.font-mono' })).toBeInTheDocument();
  });

  it('lists the local changed fields', () => {
    render(<OutlineConflictModal {...baseProps} />);
    const draftSection = screen.getByText('What you were about to do').closest('div')!;
    expect(draftSection.textContent).toContain('Chapter title');
    expect(draftSection.textContent).toContain('Move chapter');
  });

  it('enables Reapply because the server change path does not overlap known fields', () => {
    render(<OutlineConflictModal {...baseProps} />);
    expect(screen.getByRole('button', { name: /Reapply my edit/i })).toBeEnabled();
  });

  it('calls onUseCurrent when the primary action is clicked', async () => {
    const user = userEvent.setup();
    const onUseCurrent = vi.fn();
    render(<OutlineConflictModal {...baseProps} onUseCurrent={onUseCurrent} />);
    await user.click(screen.getByRole('button', { name: /Use current/i }));
    expect(onUseCurrent).toHaveBeenCalled();
  });

  it('calls onDismiss when Keep editing is clicked', async () => {
    const user = userEvent.setup();
    const onDismiss = vi.fn();
    render(<OutlineConflictModal {...baseProps} onDismiss={onDismiss} />);
    await user.click(screen.getByRole('button', { name: /Keep editing/i }));
    expect(onDismiss).toHaveBeenCalled();
  });

  it('toggles the side-by-side review panel', async () => {
    const user = userEvent.setup();
    render(<OutlineConflictModal {...baseProps} />);
    await user.click(screen.getByRole('button', { name: /Review side-by-side/i }));
    expect(screen.getByText('Server: Chapter title')).toBeInTheDocument();
    expect(screen.getByText('Your edit: Chapter title')).toBeInTheDocument();
  });
});
