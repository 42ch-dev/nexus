import { fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';

import { Sheet, SheetClose, SheetContent, SheetTrigger } from './sheet';

/**
 * V1.148 P1 — promoted Sheet presentational primitive (DESIGN.md components.sheet).
 *
 * Pins: caller-owned copy (title/description/closeLabel), the v0.4 drawer
 * token recipe (scrim overlay, shadow-elevation-4, sheet width token), and
 * controlled open/close. Theme parity comes from `--nexus-*` token
 * projections (bg-scrim/background-100/gray-*), not literals.
 */
describe('Sheet', () => {
  it('renders title, description, and body copy owned by the caller', () => {
    render(
      <Sheet open>
        <SheetContent title="Inspector" description="Work rail" closeLabel="Close dialog">
          <p>Rail</p>
        </SheetContent>
      </Sheet>,
    );

    const dialog = screen.getByRole('dialog');
    expect(dialog).toHaveTextContent('Inspector');
    expect(dialog).toHaveTextContent('Work rail');
    expect(dialog).toHaveTextContent('Rail');
  });

  it('panel consumes the drawer token recipe (scrim overlay, elevation-4, sheet width)', () => {
    render(
      <Sheet open>
        <SheetContent title="Inspector" closeLabel="Close dialog">
          <p>Rail</p>
        </SheetContent>
      </Sheet>,
    );

    const overlayEl = document.body.querySelector('.fixed.inset-0');
    expect(overlayEl).not.toBeNull();
    expect(overlayEl!.className).toMatch(/\bbg-scrim\b/);

    const content = screen.getByRole('dialog');
    expect(content.className).toMatch(/\bshadow-elevation-4\b/);
    expect(content.className).toMatch(/\bw-sheet\b/);
    expect(content.className).toMatch(/\bbg-background-100\b/);
    expect(content.className).not.toMatch(/\[min\(100vw,280px\)\]/);
  });

  it('controlled open/close notifies the caller through onOpenChange', () => {
    const onOpenChange = vi.fn();
    function Harness() {
      const [open, setOpen] = useState(false);
      return (
        <Sheet
          open={open}
          onOpenChange={(next) => {
            onOpenChange(next);
            setOpen(next);
          }}
        >
          <SheetTrigger>Open</SheetTrigger>
          <SheetContent title="Inspector" closeLabel="Close dialog">
            <p>Rail</p>
            <SheetClose>Confirm</SheetClose>
          </SheetContent>
        </Sheet>
      );
    }
    render(<Harness />);

    expect(screen.queryByRole('dialog')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Open' }));
    expect(onOpenChange).toHaveBeenLastCalledWith(true);

    fireEvent.click(screen.getByRole('button', { name: 'Close dialog' }));
    expect(onOpenChange).toHaveBeenLastCalledWith(false);
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});
