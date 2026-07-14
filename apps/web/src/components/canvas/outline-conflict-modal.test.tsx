/**
 * @vitest-environment jsdom
 */
import { useState } from 'react';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { OutlineConflictModal, type OutlineChangedField } from './outline-conflict-modal';

const baseProps = {
  open: true,
  currentRevision: 3,
  draft: {
    fields: ['chapter_title', 'move_chapter'] as OutlineChangedField[],
    conflictingPath: 'volumes/1',
  },
  onUseCurrent: vi.fn(),
  onReapply: vi.fn(),
  onDismiss: vi.fn(),
};

describe('OutlineConflictModal', () => {
  it('renders the outline headline and server revision', () => {
    render(<OutlineConflictModal {...baseProps} />);
    expect(screen.getByRole('heading', { name: 'Outline Conflict' })).toBeInTheDocument();
    expect(screen.getByText(/This entry changed while you were editing/i)).toBeInTheDocument();
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
    expect(screen.getByRole('button', { name: /^Reapply$/i })).toBeEnabled();
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

  it('exposes consistent keyboard access keys for the four actions', () => {
    render(<OutlineConflictModal {...baseProps} />);
    expect(screen.getByRole('button', { name: /Keep editing/i })).toHaveAttribute('accessKey', 'c');
    expect(screen.getByRole('button', { name: /Review side-by-side/i })).toHaveAttribute('accessKey', 'r');
    expect(screen.getByRole('button', { name: /^Reapply$/i })).toHaveAttribute('accessKey', 'a');
    expect(screen.getByRole('button', { name: /Use current/i })).toHaveAttribute('accessKey', 'u');
  });

  it('dismisses on Escape', async () => {
    const user = userEvent.setup();
    const onDismiss = vi.fn();
    render(<OutlineConflictModal {...baseProps} onDismiss={onDismiss} />);
    await user.keyboard('{Escape}');
    expect(onDismiss).toHaveBeenCalledOnce();
  });

  it('returns focus to the triggering element when it closes', async () => {
    const user = userEvent.setup();
    function Wrapper() {
      const [open, setOpen] = useState(false);
      return (
        <>
          <button type="button" onClick={() => setOpen(true)}>Open conflict</button>
          <OutlineConflictModal
            {...baseProps}
            open={open}
            onDismiss={() => setOpen(false)}
          />
        </>
      );
    }
    render(<Wrapper />);
    const trigger = screen.getByRole('button', { name: 'Open conflict' });
    await user.click(trigger);
    expect(screen.getByRole('button', { name: /Keep editing/i })).toHaveFocus();
    await user.keyboard('{Escape}');
    expect(trigger).toHaveFocus();
  });
});
