import { fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { ConfirmDialog } from './confirm-dialog';

/**
 * V1.149 P1 — L2 pattern component (DESIGN.md components.confirm-dialog).
 *
 * Pins: caller-owned copy (title/description/labels), controlled open/close
 * via onOpenChange, confirm/cancel wiring, the destructive variant mapping to
 * the Button destructive recipe, and the pending busy state (actions disabled
 * + spinner). Theme parity is inherited from the Dialog + Button token
 * recipes.
 */
describe('ConfirmDialog', () => {
  it('renders caller-owned title, description, and action labels', () => {
    render(
      <ConfirmDialog
        open
        onOpenChange={() => {}}
        title="Delete Work"
        description="This cannot be undone."
        confirmLabel="Delete"
        cancelLabel="Cancel"
        onConfirm={() => {}}
      />,
    );

    const dialog = screen.getByRole('dialog');
    expect(dialog).toHaveTextContent('Delete Work');
    expect(dialog).toHaveTextContent('This cannot be undone.');
    expect(screen.getByRole('button', { name: 'Delete' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeInTheDocument();
  });

  it('cancel notifies the caller through onOpenChange(false)', () => {
    const onOpenChange = vi.fn();
    render(
      <ConfirmDialog
        open
        onOpenChange={onOpenChange}
        title="Delete Work"
        confirmLabel="Delete"
        cancelLabel="Cancel"
        onConfirm={() => {}}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it('confirm invokes onConfirm without closing the dialog itself', () => {
    const onConfirm = vi.fn();
    const onOpenChange = vi.fn();
    render(
      <ConfirmDialog
        open
        onOpenChange={onOpenChange}
        title="Delete Work"
        confirmLabel="Delete"
        cancelLabel="Cancel"
        onConfirm={onConfirm}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Delete' }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(onOpenChange).not.toHaveBeenCalled();
  });

  it('destructive maps the confirm action to the Button destructive recipe', () => {
    render(
      <ConfirmDialog
        open
        onOpenChange={() => {}}
        title="Delete Work"
        confirmLabel="Delete"
        cancelLabel="Cancel"
        destructive
        onConfirm={() => {}}
      />,
    );

    const confirm = screen.getByRole('button', { name: 'Delete' });
    expect(confirm.className).toMatch(/\bbg-red-800\b/);
    expect(confirm.className).not.toMatch(/\bbg-blue-700\b/);
  });

  it('non-destructive confirm uses the primary recipe and cancel stays secondary', () => {
    render(
      <ConfirmDialog
        open
        onOpenChange={() => {}}
        title="Save changes"
        confirmLabel="Save"
        cancelLabel="Cancel"
        onConfirm={() => {}}
      />,
    );

    expect(screen.getByRole('button', { name: 'Save' }).className).toMatch(/\bbg-blue-700\b/);
    expect(screen.getByRole('button', { name: 'Cancel' }).className).toMatch(/\bbg-background-100\b/);
  });

  it('pending disables both actions and shows the busy spinner', () => {
    render(
      <ConfirmDialog
        open
        onOpenChange={() => {}}
        title="Delete Work"
        confirmLabel="Delete"
        cancelLabel="Cancel"
        pending
        onConfirm={vi.fn()}
      />,
    );

    expect(screen.getByRole('button', { name: 'Delete' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled();
    expect(document.querySelector('svg.animate-spin')).not.toBeNull();
  });

  it('pending actions do not fire callbacks', () => {
    const onConfirm = vi.fn();
    const onOpenChange = vi.fn();
    render(
      <ConfirmDialog
        open
        onOpenChange={onOpenChange}
        title="Delete Work"
        confirmLabel="Delete"
        cancelLabel="Cancel"
        pending
        onConfirm={onConfirm}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Delete' }));
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(onConfirm).not.toHaveBeenCalled();
    expect(onOpenChange).not.toHaveBeenCalled();
  });

  it('controlled open/close notifies the caller through onOpenChange', () => {
    function Harness() {
      const [open, setOpen] = useState(true);
      return (
        <ConfirmDialog
          open={open}
          onOpenChange={setOpen}
          title="Delete Work"
          confirmLabel="Delete"
          cancelLabel="Cancel"
          onConfirm={() => {}}
        />
      );
    }

    render(<Harness />);
    expect(screen.getByRole('dialog')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('panel inherits the modal token recipe from the promoted Dialog', () => {
    render(
      <ConfirmDialog
        open
        onOpenChange={() => {}}
        title="Delete Work"
        confirmLabel="Delete"
        cancelLabel="Cancel"
        onConfirm={() => {}}
      />,
    );

    const content = screen.getByRole('dialog');
    expect(content.className).toMatch(/\bshadow-elevation-4\b/);
    expect(content.className).toMatch(/\bbg-background-100\b/);
  });
});
