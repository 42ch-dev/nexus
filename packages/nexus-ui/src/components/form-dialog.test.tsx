import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { FormDialog } from './form-dialog';

/**
 * V1.149 P1 — L2 pattern component (DESIGN.md components.form-dialog).
 *
 * Pins: caller-owned fields render inside a native form, submit/cancel
 * wiring, the inline error slot (role=alert + error-surface tokens), the
 * action bar recipe, and the pending busy state (actions disabled +
 * spinner). Fields, validation, and mutation stay caller-owned — the dialog
 * owns layout, copy slots, and busy/disable behavior only.
 */
describe('FormDialog', () => {
  it('renders caller-owned fields, title, and action labels inside a form', () => {
    render(
      <FormDialog
        open
        onOpenChange={() => {}}
        title="Create agent"
        submitLabel="Create"
        cancelLabel="Cancel"
        onSubmit={() => {}}
      >
        <label htmlFor="name">Name</label>
        <input id="name" name="name" />
      </FormDialog>,
    );

    const dialog = screen.getByRole('dialog');
    expect(dialog).toHaveTextContent('Create agent');
    expect(screen.getByLabelText('Name')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Create' })).toHaveAttribute('type', 'submit');
    expect(screen.getByRole('button', { name: 'Cancel' })).toHaveAttribute('type', 'button');
  });

  it('native form submit invokes the caller onSubmit', () => {
    const onSubmit = vi.fn((event: { preventDefault: () => void }) => event.preventDefault());
    render(
      <FormDialog
        open
        onOpenChange={() => {}}
        title="Create agent"
        submitLabel="Create"
        cancelLabel="Cancel"
        onSubmit={onSubmit}
      >
        <input name="name" />
      </FormDialog>,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Create' }));
    expect(onSubmit).toHaveBeenCalledTimes(1);
  });

  it('cancel notifies the caller through onOpenChange(false)', () => {
    const onOpenChange = vi.fn();
    render(
      <FormDialog
        open
        onOpenChange={onOpenChange}
        title="Create agent"
        submitLabel="Create"
        cancelLabel="Cancel"
      >
        <input name="name" />
      </FormDialog>,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it('renders the inline error slot with the error-surface recipe', () => {
    render(
      <FormDialog
        open
        onOpenChange={() => {}}
        title="Create agent"
        submitLabel="Create"
        cancelLabel="Cancel"
        error="An agent with this name already exists."
      >
        <input name="name" />
      </FormDialog>,
    );

    const alert = screen.getByRole('alert');
    expect(alert).toHaveTextContent('An agent with this name already exists.');
    expect(alert.className).toMatch(/\bbg-error-surface\b/);
    expect(alert.className).toMatch(/\bborder-error-surface-border\b/);
    expect(alert.className).toMatch(/\btext-red-900\b/);
  });

  it('omits the error slot when the caller passes no error', () => {
    render(
      <FormDialog
        open
        onOpenChange={() => {}}
        title="Create agent"
        submitLabel="Create"
        cancelLabel="Cancel"
      >
        <input name="name" />
      </FormDialog>,
    );

    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('pending disables both actions, shows the spinner, and blocks submit', () => {
    const onSubmit = vi.fn();
    render(
      <FormDialog
        open
        onOpenChange={() => {}}
        title="Create agent"
        submitLabel="Create"
        cancelLabel="Cancel"
        pending
        onSubmit={onSubmit}
      >
        <input name="name" />
      </FormDialog>,
    );

    expect(screen.getByRole('button', { name: 'Create' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled();
    expect(document.querySelector('svg.animate-spin')).not.toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Create' }));
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it('pending intercepts implicit submit before it reaches the caller handler', () => {
    const onSubmit = vi.fn();
    render(
      <FormDialog
        open
        onOpenChange={() => {}}
        title="Create agent"
        submitLabel="Create"
        cancelLabel="Cancel"
        pending
        onSubmit={onSubmit}
      >
        <input name="name" />
      </FormDialog>,
    );

    // Enter-key implicit submit path: a submit event on the form itself.
    const form = document.querySelector('form')!;
    fireEvent.submit(form);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it('implicit submit without onSubmit does not navigate or unmount the dialog', () => {
    render(
      <FormDialog
        open
        onOpenChange={() => {}}
        title="Create agent"
        submitLabel="Create"
        cancelLabel="Cancel"
      >
        <input name="name" />
      </FormDialog>,
    );

    const form = document.querySelector('form')!;
    fireEvent.submit(form);
    // preventDefault keeps jsdom from attempting native navigation; the
    // dialog stays mounted and no reload wipes its state.
    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });

  it('pending blocks Escape-key dismissal', () => {
    const onOpenChange = vi.fn();
    render(
      <FormDialog
        open
        onOpenChange={onOpenChange}
        title="Create agent"
        submitLabel="Create"
        cancelLabel="Cancel"
        pending
      >
        <input name="name" />
      </FormDialog>,
    );

    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onOpenChange).not.toHaveBeenCalled();
    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });

  it('pending blocks outside-click dismissal', async () => {
    const onOpenChange = vi.fn();
    const { container } = render(
      <FormDialog
        open
        onOpenChange={onOpenChange}
        title="Create agent"
        submitLabel="Create"
        cancelLabel="Cancel"
        pending
      >
        <input name="name" />
      </FormDialog>,
    );

    // Radix attaches its document pointerdown listener after a tick.
    await new Promise((resolve) => setTimeout(resolve, 0));
    const overlay = container.ownerDocument.body.querySelector('.fixed.inset-0')!;
    fireEvent.pointerDown(overlay, { pointerType: 'mouse', bubbles: true, button: 0 });
    fireEvent.click(overlay, { bubbles: true });
    expect(onOpenChange).not.toHaveBeenCalled();
    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });

  it('close control dismisses through onOpenChange(false) when not pending', () => {
    const onOpenChange = vi.fn();
    render(
      <FormDialog
        open
        onOpenChange={onOpenChange}
        title="Create agent"
        submitLabel="Create"
        cancelLabel="Cancel"
      >
        <input name="name" />
      </FormDialog>,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Close dialog' }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it('close control falls back to the default label and honors a closeLabel override', () => {
    const { unmount } = render(
      <FormDialog
        open
        onOpenChange={() => {}}
        title="Create agent"
        submitLabel="Create"
        cancelLabel="Cancel"
      >
        <input name="name" />
      </FormDialog>,
    );
    expect(screen.getByRole('button', { name: 'Close dialog' })).toBeInTheDocument();
    unmount();

    render(
      <FormDialog
        open
        onOpenChange={() => {}}
        title="Create agent"
        submitLabel="Create"
        cancelLabel="Cancel"
        closeLabel="Dismiss"
      >
        <input name="name" />
      </FormDialog>,
    );
    expect(screen.getByRole('button', { name: 'Dismiss' })).toBeInTheDocument();
  });

  it('panel inherits the modal token recipe from the promoted Dialog', () => {
    render(
      <FormDialog
        open
        onOpenChange={() => {}}
        title="Create agent"
        submitLabel="Create"
        cancelLabel="Cancel"
      >
        <input name="name" />
      </FormDialog>,
    );

    const content = screen.getByRole('dialog');
    expect(content.className).toMatch(/\bshadow-elevation-4\b/);
    expect(content.className).toMatch(/\bbg-background-100\b/);
  });
});
