/**
 * @vitest-environment jsdom
 */
import { useState } from 'react';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { WorldKbRelationshipConflictModal } from '../world-kb-relationship-conflict-modal';
import type { RelationshipForm } from '../relationship-inspector-logic';

const baseForm: RelationshipForm = {
  sourceEntityId: 'ent-1',
  targetEntityId: 'ent-2',
  relationType: 'references',
  customLabel: '',
  symmetric: false,
  confidence: 1,
  sourceAnchorIds: [],
};

const baseProps = {
  open: true,
  draft: {
    relationshipId: 'rel-1',
    sourceName: 'Aria Stormwind',
    targetName: 'Elena Vale',
    form: baseForm,
  },
  currentVersion: 5,
  onUseCurrent: vi.fn(),
  onReapply: vi.fn(),
  onDismiss: vi.fn(),
};

describe('WorldKbRelationshipConflictModal', () => {
  it('renders the relationship headline and server version', () => {
    render(<WorldKbRelationshipConflictModal {...baseProps} />);
    expect(
      screen.getByText('This relationship changed while you were editing it.'),
    ).toBeInTheDocument();
    expect(screen.getByText(/Aria Stormwind/i)).toBeInTheDocument();
    expect(screen.getByText(/Elena Vale/i)).toBeInTheDocument();
    expect(screen.getByText('5', { selector: 'span.font-mono' })).toBeInTheDocument();
  });

  it('calls onUseCurrent when the primary action is clicked', async () => {
    const user = userEvent.setup();
    const onUseCurrent = vi.fn();
    render(<WorldKbRelationshipConflictModal {...baseProps} onUseCurrent={onUseCurrent} />);
    await user.click(screen.getByRole('button', { name: /Use current/i }));
    expect(onUseCurrent).toHaveBeenCalledOnce();
  });

  it('disables Reapply because the server and local change overlap on the same relationship field', () => {
    render(<WorldKbRelationshipConflictModal {...baseProps} />);
    expect(screen.getByRole('button', { name: /Reapply my edit/i })).toBeDisabled();
  });

  it('calls onDismiss when Cancel is clicked', async () => {
    const user = userEvent.setup();
    const onDismiss = vi.fn();
    render(<WorldKbRelationshipConflictModal {...baseProps} onDismiss={onDismiss} />);
    await user.click(screen.getByRole('button', { name: /Cancel/i }));
    expect(onDismiss).toHaveBeenCalledOnce();
  });

  it('toggles the side-by-side review panel', async () => {
    const user = userEvent.setup();
    render(<WorldKbRelationshipConflictModal {...baseProps} />);
    await user.click(screen.getByRole('button', { name: /Review side-by-side/i }));
    expect(screen.getByText('Changed by another session')).toBeInTheDocument();
    // The review panel exposes the user's pending edit value in the draft cell.
    const draftCell = screen.getByText('Your edit: Relation type').closest('div')!;
    expect(draftCell.textContent).toMatch(/Aria Stormwind.*Elena Vale/);
  });

  it('exposes consistent keyboard access keys for the four actions', () => {
    render(<WorldKbRelationshipConflictModal {...baseProps} />);
    expect(screen.getByRole('button', { name: /Cancel/i })).toHaveAttribute('accessKey', 'c');
    expect(screen.getByRole('button', { name: /Review side-by-side/i })).toHaveAttribute('accessKey', 'r');
    expect(screen.getByRole('button', { name: /Reapply my edit/i })).toHaveAttribute('accessKey', 'a');
    expect(screen.getByRole('button', { name: /Use current/i })).toHaveAttribute('accessKey', 'u');
  });

  it('dismisses on Escape', async () => {
    const user = userEvent.setup();
    const onDismiss = vi.fn();
    render(<WorldKbRelationshipConflictModal {...baseProps} onDismiss={onDismiss} />);
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
          <WorldKbRelationshipConflictModal
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
    expect(screen.getByRole('button', { name: /Cancel/i })).toHaveFocus();
    await user.keyboard('{Escape}');
    expect(trigger).toHaveFocus();
  });
});
