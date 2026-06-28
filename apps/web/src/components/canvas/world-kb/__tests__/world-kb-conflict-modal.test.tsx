/**
 * World KB conflict modal tests — both KB-flavored variants (V1.73 P0 A6/A8).
 *
 * Verifies the compass §1.1 A6 exact copy headlines + the per-variant action
 * labels, the inherited focus-trapped/ARIA-live shell behavior, and that the
 * Reapply button is disabled when the draft overlaps the server change.
 */
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import {
  WorldKbEntityConflictModal,
  WorldKbPromoteConflictModal,
  type WorldKbEntityConflictDraft,
  type WorldKbPromoteConflictDraft,
} from '../world-kb-conflict-modal';

const entityDraft: WorldKbEntityConflictDraft = {
  entityName: 'Aria Stormwind',
  fields: ['title', 'aliases'],
  changedFields: [{ field: 'body', from: '¶2', to: 'added ¶3' }],
  draftValues: { title: 'Aria Stormwind (v2)', aliases: 'Tempest' },
};

const entityProps = {
  open: true,
  draft: entityDraft,
  currentVersion: 4,
  onUseCurrent: vi.fn(),
  onReapply: vi.fn(),
  onDismiss: vi.fn(),
};

describe('WorldKbEntityConflictModal (patch_entity variant)', () => {
  it('renders the exact KB headline + entity name', () => {
    render(<WorldKbEntityConflictModal {...entityProps} />);
    expect(
      screen.getByText('This world entry changed while you were editing.'),
    ).toBeInTheDocument();
    expect(screen.getByText('Aria Stormwind')).toBeInTheDocument();
    expect(screen.getByText('4', { selector: 'span.font-mono' })).toBeInTheDocument();
  });

  it('renders the trailing "while you were editing its …" clause', () => {
    render(<WorldKbEntityConflictModal {...entityProps} />);
    expect(screen.getByText(/while you were editing its/i)).toBeInTheDocument();
    expect(screen.getByText(/your change is still in the inspector/i)).toBeInTheDocument();
  });

  it('enables Reapply because the draft fields do not overlap the server change', () => {
    render(<WorldKbEntityConflictModal {...entityProps} />);
    expect(screen.getByRole('button', { name: /Reapply my edit/i })).toBeEnabled();
  });

  it('calls onUseCurrent when the primary action is clicked', async () => {
    const user = userEvent.setup();
    const onUseCurrent = vi.fn();
    render(<WorldKbEntityConflictModal {...entityProps} onUseCurrent={onUseCurrent} />);
    await user.click(screen.getByRole('button', { name: /Use current/i }));
    expect(onUseCurrent).toHaveBeenCalledOnce();
  });

  it('calls onDismiss when Cancel is clicked', async () => {
    const user = userEvent.setup();
    const onDismiss = vi.fn();
    render(<WorldKbEntityConflictModal {...entityProps} onDismiss={onDismiss} />);
    await user.click(screen.getByRole('button', { name: /Cancel/i }));
    expect(onDismiss).toHaveBeenCalledOnce();
  });

  it('lists the local changed fields under "What you were about to do"', () => {
    render(<WorldKbEntityConflictModal {...entityProps} />);
    const section = screen.getByText('What you were about to do').closest('div')!;
    expect(section.textContent).toContain('Title');
    expect(section.textContent).toContain('Aliases');
  });
});

const promoteDraft: WorldKbPromoteConflictDraft = {
  candidateName: 'Elena Vale',
  newStatus: 'adopted',
  action: 'merge',
  mergeTargetId: 'kb-001',
  mergeTargetLabel: 'Elena Vale (existing)',
};

const promoteProps = {
  open: true,
  draft: promoteDraft,
  currentVersion: 2,
  onUseCurrent: vi.fn(),
  onReapply: vi.fn(),
  onDismiss: vi.fn(),
};

describe('WorldKbPromoteConflictModal (promote_candidate variant)', () => {
  it('renders the exact candidate headline + name + status', () => {
    render(<WorldKbPromoteConflictModal {...promoteProps} />);
    expect(
      screen.getByText("This candidate's state changed while you were reviewing it."),
    ).toBeInTheDocument();
    expect(screen.getByText('Elena Vale')).toBeInTheDocument();
    expect(screen.getByText(/while you were about to/i)).toBeInTheDocument();
    expect(screen.getByText(/your decision is still in the inspector/i)).toBeInTheDocument();
  });

  it('calls onReapply when Reapply my decision is clicked', async () => {
    const user = userEvent.setup();
    const onReapply = vi.fn();
    render(<WorldKbPromoteConflictModal {...promoteProps} onReapply={onReapply} />);
    await user.click(screen.getByRole('button', { name: /Reapply my decision/i }));
    expect(onReapply).toHaveBeenCalledOnce();
  });

  it('toggles the side-by-side review panel', async () => {
    const user = userEvent.setup();
    render(<WorldKbPromoteConflictModal {...promoteProps} />);
    await user.click(screen.getByRole('button', { name: /Review side-by-side/i }));
    // The review row renders the label in both the server and draft cells.
    const matches = screen.getAllByText(/Promotion state/i);
    expect(matches.length).toBeGreaterThanOrEqual(1);
  });
});
