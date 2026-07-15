/**
 * FB-SE-002 — draft transition commit UI (edge inspector draft mode).
 *
 * The `DraftEdgeInspector` renders the Condition field plus the locked
 * Create Transition / Cancel actions, and routes commits through the
 * hook-owned draft commit callback (op: "create") and cancel through the
 * hook-owned draft removal.
 */
import { describe, expect, it, vi } from 'vitest';

import { renderInApp } from '@/test/test-providers';
import { noopClient } from '@/test/test-providers';
import { screen, fireEvent } from '@testing-library/react';

import { DraftEdgeInspector } from './edge-inspector';

describe('DraftEdgeInspector (FB-SE-002)', () => {
  it('renders the locked copy: Create Transition, Cancel, Condition', () => {
    renderInApp(
      <DraftEdgeInspector
        sourceStateId="draft"
        targetStateId="revise"
        isCommitting={false}
        onCommit={vi.fn()}
        onCancel={vi.fn()}
      />,
      { client: noopClient },
    );

    // Primary commit CTA — locked voice.
    expect(screen.getByRole('button', { name: 'Create' })).toBeInTheDocument();
    // Cancel CTA — locked voice.
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeInTheDocument();
    // Field labels — locked voice.
    expect(screen.getByText('Condition')).toBeInTheDocument();
    // Read-only source/target display.
    expect(screen.getByText('draft')).toBeInTheDocument();
    expect(screen.getByText('revise')).toBeInTheDocument();
  });

  it('commits with the entered condition', () => {
    const onCommit = vi.fn();
    renderInApp(
      <DraftEdgeInspector
        sourceStateId="draft"
        targetStateId="revise"
        isCommitting={false}
        onCommit={onCommit}
        onCancel={vi.fn()}
      />,
      { client: noopClient },
    );

    fireEvent.change(screen.getByLabelText('Condition'), { target: { value: 'word_count > 1000' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create' }));

    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(onCommit).toHaveBeenCalledWith({ condition: 'word_count > 1000' });
  });

  it('omits empty condition from the commit payload', () => {
    const onCommit = vi.fn();
    renderInApp(
      <DraftEdgeInspector
        sourceStateId="draft"
        targetStateId="revise"
        isCommitting={false}
        onCommit={onCommit}
        onCancel={vi.fn()}
      />,
      { client: noopClient },
    );

    fireEvent.click(screen.getByRole('button', { name: 'Create' }));
    expect(onCommit).toHaveBeenCalledWith({});
  });

  it('disables the commit button while committing and shows Creating…', () => {
    renderInApp(
      <DraftEdgeInspector
        sourceStateId="draft"
        targetStateId="revise"
        isCommitting={true}
        onCommit={vi.fn()}
        onCancel={vi.fn()}
      />,
      { client: noopClient },
    );

    const commit = screen.getByRole('button', { name: 'Creating…' });
    expect(commit).toBeDisabled();
  });

  it('cancel discards the draft without committing', () => {
    const onCancel = vi.fn();
    const onCommit = vi.fn();
    renderInApp(
      <DraftEdgeInspector
        sourceStateId="draft"
        targetStateId="revise"
        isCommitting={false}
        onCommit={onCommit}
        onCancel={onCancel}
      />,
      { client: noopClient },
    );

    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onCommit).not.toHaveBeenCalled();
  });
});
