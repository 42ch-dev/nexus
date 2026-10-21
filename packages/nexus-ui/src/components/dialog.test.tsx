import { fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { Dialog, DialogClose, DialogContent, DialogTrigger } from './dialog';

/**
 * V1.148 P1 — promoted Dialog presentational primitive (DESIGN.md components.dialog).
 *
 * Pins: caller-owned copy (title/description/closeLabel), the v0.4 modal token
 * recipe (scrim overlay, shadow-elevation-4, dialog sizing tokens), and
 * controlled open/close via Radix callbacks. Theme parity comes from the
 * `--nexus-*` token projections (bg-scrim/background-100/gray-*), not literals.
 */
describe('Dialog', () => {
  it('renders title, description, and body copy owned by the caller', () => {
    render(
      <Dialog open>
        <DialogContent title="Delete Work" description="This cannot be undone." closeLabel="Close dialog">
          <p>Body</p>
        </DialogContent>
      </Dialog>,
    );

    const dialog = screen.getByRole('dialog');
    expect(dialog).toHaveTextContent('Delete Work');
    expect(dialog).toHaveTextContent('This cannot be undone.');
    expect(dialog).toHaveTextContent('Body');
  });

  it('panel consumes the modal token recipe (scrim overlay, elevation-4, dialog sizing)', () => {
    const { container } = render(
      <Dialog open>
        <DialogContent title="Delete Work" closeLabel="Close dialog">
          <p>Body</p>
        </DialogContent>
      </Dialog>,
    );

    const overlayEl = container.ownerDocument.body.querySelector('.fixed.inset-0');
    expect(overlayEl).not.toBeNull();
    expect(overlayEl!.className).toMatch(/\bbg-scrim\b/);
    expect(overlayEl!.className).not.toMatch(/bg-black\//);

    const content = screen.getByRole('dialog');
    expect(content.className).toMatch(/\bshadow-elevation-4\b/);
    expect(content.className).toMatch(/\bw-dialog\b/);
    expect(content.className).toMatch(/\bmax-w-dialog\b/);
    expect(content.className).toMatch(/\bmax-h-dialog\b/);
    expect(content.className).toMatch(/\bbg-background-100\b/);
    expect(content.className).not.toMatch(/\[calc\(|\[min\(|max-h-\[85vh\]|max-w-\[560px\]/);
  });

  it('close control announces caller-owned copy', () => {
    render(
      <Dialog open>
        <DialogContent title="Delete Work" closeLabel="Dismiss">
          <p>Body</p>
        </DialogContent>
      </Dialog>,
    );

    expect(screen.getByRole('button', { name: 'Dismiss' })).toBeInTheDocument();
  });

  it('falls back to the default close label when the caller omits closeLabel', () => {
    render(
      <Dialog open>
        <DialogContent title="Delete Work">
          <p>Body</p>
        </DialogContent>
      </Dialog>,
    );

    expect(screen.getByRole('button', { name: 'Close dialog' })).toBeInTheDocument();
  });

  it('controlled open/close notifies the caller through onOpenChange', () => {
    const onOpenChange = vi.fn();
    function Harness() {
      const [open, setOpen] = useState(false);
      return (
        <Dialog
          open={open}
          onOpenChange={(next) => {
            onOpenChange(next);
            setOpen(next);
          }}
        >
          <DialogTrigger>Open</DialogTrigger>
          <DialogContent title="Delete Work" closeLabel="Close dialog">
            <p>Body</p>
            <DialogClose>Confirm</DialogClose>
          </DialogContent>
        </Dialog>
      );
    }
    render(<Harness />);

    expect(screen.queryByRole('dialog')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Open' }));
    expect(onOpenChange).toHaveBeenLastCalledWith(true);
    expect(screen.getByRole('dialog')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Close dialog' }));
    expect(onOpenChange).toHaveBeenLastCalledWith(false);
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});
