/**
 * @vitest-environment jsdom
 */
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { ConflictModal } from './conflict-modal';

const baseProps = {
  open: true,
  currentRevision: 7,
  draft: {
    label: 'My label',
    description: 'My description',
    nextTarget: 'next-state',
    promptBody: '# prompt',
  },
  canonicalState: {
    id: 'server-label',
    description: 'Server description',
    next: 'other-state',
    terminal: false,
  } as const,
  promptTemplateRef: 'prompts/start.md',
  changedFields: ['label', 'description'] as const,
  onUseCurrent: vi.fn(),
  onReapply: vi.fn(),
  onDismiss: vi.fn(),
};

describe('ConflictModal', () => {
  it('renders the mandated headline and server revision', () => {
    render(<ConflictModal {...baseProps} />);
    expect(screen.getByText('This state changed while you were editing.')).toBeInTheDocument();
    expect(screen.getByText('7', { selector: 'span.font-mono' })).toBeInTheDocument();
  });

  it('lists what changed on the server and what the user edited', () => {
    render(<ConflictModal {...baseProps} />);
    const serverSection = screen.getByText('What changed on the server').closest('div')!;
    const draftSection = screen.getByText('What you were about to do').closest('div')!;
    expect(serverSection.textContent).toContain('State label');
    expect(serverSection.textContent).toContain('Description');
    expect(draftSection.textContent).toContain('State label');
    expect(draftSection.textContent).toContain('Description');
  });

  it('disables Reapply when server and draft overlap', () => {
    render(<ConflictModal {...baseProps} />);
    const reapply = screen.getByRole('button', { name: /Reapply my edit/i });
    expect(reapply).toBeDisabled();
  });

  it('enables Reapply when there is no overlap', () => {
    render(
      <ConflictModal
        {...baseProps}
        changedFields={['nextTarget']}
        canonicalState={{ ...baseProps.canonicalState, next: 'next-state' }}
      />,
    );
    const reapply = screen.getByRole('button', { name: /Reapply my edit/i });
    expect(reapply).toBeEnabled();
  });

  it('toggles the side-by-side review panel', async () => {
    const user = userEvent.setup();
    render(<ConflictModal {...baseProps} />);
    const review = screen.getByRole('button', { name: /Review side-by-side/i });
    await user.click(review);
    expect(screen.getByText('Server: State label')).toBeInTheDocument();
    expect(screen.getByText('Your edit: State label')).toBeInTheDocument();
  });

  it('calls onUseCurrent when the primary action is clicked', async () => {
    const user = userEvent.setup();
    const onUseCurrent = vi.fn();
    render(<ConflictModal {...baseProps} onUseCurrent={onUseCurrent} />);
    await user.click(screen.getByRole('button', { name: /Use current/i }));
    expect(onUseCurrent).toHaveBeenCalled();
  });

  it('calls onDismiss when Keep editing is clicked', async () => {
    const user = userEvent.setup();
    const onDismiss = vi.fn();
    render(<ConflictModal {...baseProps} onDismiss={onDismiss} />);
    await user.click(screen.getByRole('button', { name: /Keep editing/i }));
    expect(onDismiss).toHaveBeenCalled();
  });

  it('renders a polite live region describing the conflict', () => {
    render(<ConflictModal {...baseProps} />);
    const live = screen.getByRole('status');
    expect(live).toHaveAttribute('aria-live', 'polite');
    expect(live.textContent).toContain('Conflict detected on revision 7');
    expect(live.textContent).toContain('Overlap on State label, Description');
  });
});
