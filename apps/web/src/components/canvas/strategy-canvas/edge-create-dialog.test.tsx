/**
 * FB-SE-004 — Keyboard edge-creation dialog (§4.4).
 *
 * The dialog renders the locked Voice & Content labels (Choose source →
 * Choose target → Choose edge kind, Create Transition, Cancel) and routes
 * the commit through the `onCommit` callback that sends
 * `strategy.patch_transition` with `op: "create"` (same as the spatial path).
 */
import { describe, expect, it, vi } from 'vitest';

import { renderInApp, noopClient } from '@/test/test-providers';
import { screen, fireEvent } from '@testing-library/react';
import type { PresetState } from '@/lib/canvas/preset-yaml';

import { EdgeCreateDialog } from './edge-create-dialog';

const states: PresetState[] = [
  { id: 'draft' },
  { id: 'revise' },
  { id: 'done' },
];

function renderDialog(overrides: Partial<React.ComponentProps<typeof EdgeCreateDialog>> = {}) {
  const props = {
    open: true,
    onOpenChange: vi.fn(),
    states,
    onCommit: vi.fn(),
    isCommitting: false,
    ...overrides,
  };
  renderInApp(<EdgeCreateDialog {...props} />, { client: noopClient });
  return props;
}

describe('EdgeCreateDialog (FB-SE-004)', () => {
  it('renders the locked copy when open: title, step labels, Create Transition, Cancel', () => {
    renderDialog();

    // Dialog title (heading).
    expect(screen.getByRole('heading', { name: 'Create Transition' })).toBeInTheDocument();
    // Step labels — locked voice.
    expect(screen.getByText('Choose source')).toBeInTheDocument();
    expect(screen.getByText('Choose target')).toBeInTheDocument();
    expect(screen.getByText('Choose edge kind')).toBeInTheDocument();
    // Commit + Cancel CTAs — locked voice.
    expect(screen.getByRole('button', { name: 'Create Transition' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeInTheDocument();
  });

  it('does not render when closed', () => {
    renderDialog({ open: false });

    expect(screen.queryByRole('heading', { name: 'Create Transition' })).not.toBeInTheDocument();
  });

  it('target list excludes the chosen source', () => {
    renderDialog();

    fireEvent.change(screen.getByLabelText('Choose source'), { target: { value: 'draft' } });

    const targetSelect = screen.getByLabelText('Choose target') as HTMLSelectElement;
    const targetValues = Array.from(targetSelect.options).map((o) => o.value);
    expect(targetValues).not.toContain('draft');
    expect(targetValues).toContain('revise');
    expect(targetValues).toContain('done');
  });

  it('commits with source, target, kind, and condition', () => {
    const onCommit = vi.fn();
    renderDialog({ onCommit });

    fireEvent.change(screen.getByLabelText('Choose source'), { target: { value: 'draft' } });
    fireEvent.change(screen.getByLabelText('Choose target'), { target: { value: 'revise' } });
    fireEvent.change(screen.getByLabelText('Choose edge kind'), { target: { value: 'branch' } });
    fireEvent.change(screen.getByLabelText('Condition'), { target: { value: 'word_count > 1000' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create Transition' }));

    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(onCommit).toHaveBeenCalledWith({
      sourceStateId: 'draft',
      targetStateId: 'revise',
      transitionKind: 'branch',
      condition: 'word_count > 1000',
    });
  });

  it('disables Create Transition until both source and target are chosen', () => {
    renderDialog();

    // Initially disabled — no source/target.
    expect(screen.getByRole('button', { name: 'Create Transition' })).toBeDisabled();

    // Choose source only — still disabled (no target).
    fireEvent.change(screen.getByLabelText('Choose source'), { target: { value: 'draft' } });
    expect(screen.getByRole('button', { name: 'Create Transition' })).toBeDisabled();

    // Choose target — now enabled.
    fireEvent.change(screen.getByLabelText('Choose target'), { target: { value: 'revise' } });
    expect(screen.getByRole('button', { name: 'Create Transition' })).not.toBeDisabled();
  });

  it('cancel closes the dialog without committing', () => {
    const onOpenChange = vi.fn();
    const onCommit = vi.fn();
    renderDialog({ onOpenChange, onCommit });

    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));

    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(onCommit).not.toHaveBeenCalled();
  });

  it('shows Creating… and disables actions while committing', () => {
    renderDialog({ isCommitting: true });

    // Commit button shows pending copy.
    const commit = screen.getByRole('button', { name: 'Creating…' });
    expect(commit).toBeDisabled();
    // Cancel is disabled while committing.
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled();
  });
});
